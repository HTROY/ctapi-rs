//! Nonblocking Rust adapters for synchronous CtAPI calls.
//!
//! Workers own the connection, arguments and result storage until CtAPI
//! returns. Dropping a waiter abandons the result. A call that has already
//! started can still finish and have effects on the server.

use crate::client::extract_string_from_buffer;
use crate::error::{CtApiError, Result};
use crate::util::encode_to_gbk_cstring;
use crate::{CtClient, CtList};
use ctapi_sys::OVERLAPPED;
use std::fmt;
use std::future::Future;
use std::io::{Error, ErrorKind};
use std::os::windows::io::{AsRawHandle, FromRawHandle, OwnedHandle};
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::pin::Pin;
use std::sync::{Arc, Condvar, Mutex};
use std::task::{Context, Poll, Waker};
use windows_sys::Win32::System::Threading::CreateEventA;

const DEFAULT_BUFFER_SIZE: usize = 256;

enum Phase<T> {
    Queued,
    Running,
    Finished(Option<Result<T>>),
}

struct Completion<T> {
    phase: Phase<T>,
    waker: Option<Waker>,
}

impl<T> Completion<T> {
    fn take_result(&mut self) -> Option<Result<T>> {
        match &mut self.phase {
            Phase::Queued | Phase::Running => None,
            Phase::Finished(result) => Some(result.take().unwrap_or_else(|| {
                Err(Error::new(
                    ErrorKind::InvalidInput,
                    "CtAPI operation result was already collected",
                )
                .into())
            })),
        }
    }
}

struct Shared<T> {
    completion: Mutex<Completion<T>>,
    ready: Condvar,
}

struct Work<T> {
    shared: Arc<Shared<T>>,
}

impl<T> Work<T> {
    fn spawn(job: impl FnOnce() -> Result<T> + Send + 'static) -> Result<Self>
    where
        T: Send + 'static,
    {
        let shared = Arc::new(Shared {
            completion: Mutex::new(Completion {
                phase: Phase::Queued,
                waker: None,
            }),
            ready: Condvar::new(),
        });
        let worker = Arc::clone(&shared);
        std::thread::Builder::new()
            .name("ctapi-call".into())
            .spawn(move || {
                {
                    let mut completion = worker.completion.lock().unwrap();
                    if !matches!(completion.phase, Phase::Queued) {
                        return;
                    }
                    completion.phase = Phase::Running;
                }

                let result = catch_unwind(AssertUnwindSafe(job))
                    .unwrap_or_else(|_| Err(Error::other("CtAPI worker panicked").into()));
                let waker = {
                    let mut completion = worker.completion.lock().unwrap();
                    completion.phase = Phase::Finished(Some(result));
                    completion.waker.take()
                };
                worker.ready.notify_all();
                if let Some(waker) = waker {
                    waker.wake();
                }
            })?;
        Ok(Self { shared })
    }

    fn is_complete(&self) -> bool {
        let completion = self.shared.completion.lock().unwrap();
        matches!(completion.phase, Phase::Finished(_))
    }

    fn try_result(&self) -> Option<Result<T>> {
        self.shared.completion.lock().unwrap().take_result()
    }

    fn wait(&self) -> Result<T> {
        let mut completion = self.shared.completion.lock().unwrap();
        loop {
            if let Some(result) = completion.take_result() {
                return result;
            }
            completion = self.shared.ready.wait(completion).unwrap();
        }
    }

    fn poll(&self, cx: &Context<'_>) -> Poll<Result<T>> {
        let waker = cx.waker().clone();
        let old_waker = {
            let mut completion = self.shared.completion.lock().unwrap();
            if let Some(result) = completion.take_result() {
                return Poll::Ready(result);
            }
            completion.waker.replace(waker)
        };
        drop(old_waker);
        Poll::Pending
    }

    fn cancel(&self) -> Result<()> {
        let waker = {
            let mut completion = self.shared.completion.lock().unwrap();
            match completion.phase {
                Phase::Queued => {
                    completion.phase = cancelled();
                    completion.waker.take()
                }
                Phase::Running => {
                    return Err(CtApiError::UnsupportedOperation {
                        operation: "cancel an already-running CtAPI call".to_owned(),
                    });
                }
                Phase::Finished(_) => return Ok(()),
            }
        };
        self.shared.ready.notify_all();
        if let Some(waker) = waker {
            waker.wake();
        }
        Ok(())
    }
}

fn cancelled<T>() -> Phase<T> {
    Phase::Finished(Some(Err(Error::new(
        ErrorKind::Interrupted,
        "CtAPI call cancelled before it started",
    )
    .into())))
}

impl<T> Drop for Work<T> {
    fn drop(&mut self) {
        let waker = {
            let mut completion = self.shared.completion.lock().unwrap();
            if matches!(completion.phase, Phase::Queued) {
                completion.phase = cancelled();
            }
            completion.waker.take()
        };
        drop(waker);
    }
}

// Kept solely for callers that explicitly opt into raw, caller-managed I/O.
struct RawOperation {
    overlapped: OVERLAPPED,
    _buffer: Box<[u8]>,
    _event: OwnedHandle,
}

impl RawOperation {
    fn new(buffer_size: usize) -> Self {
        let mut buffer = vec![0; buffer_size].into_boxed_slice();
        // SAFETY: An unnamed manual-reset event with default security.
        let handle = unsafe { CreateEventA(std::ptr::null_mut(), 1, 0, std::ptr::null()) };
        assert!(!handle.is_null(), "CreateEventA failed");
        // SAFETY: CreateEventA transferred a valid handle to this caller.
        let event = unsafe { OwnedHandle::from_raw_handle(handle) };
        let mut overlapped = OVERLAPPED::new();
        overlapped.hEvent = event.as_raw_handle();
        overlapped.pData = buffer.as_mut_ptr();
        Self {
            overlapped,
            _buffer: buffer,
            _event: event,
        }
    }
}

/// A caller-managed handle to a nonblocking CtAPI operation.
///
/// Safe methods submit blocking native calls to owned workers. Use the
/// originating client when collecting the result, even for list operations.
pub struct AsyncOperation {
    buffer_size: usize,
    active: Option<(CtClient, Work<String>)>,
    raw: Option<Box<RawOperation>>,
}

impl AsyncOperation {
    /// Create an operation with the default Cicode response-buffer size.
    pub fn new() -> Self {
        Self::with_buffer_size(DEFAULT_BUFFER_SIZE)
    }

    /// Create an operation with a custom Cicode response-buffer size.
    pub fn with_buffer_size(buffer_size: usize) -> Self {
        Self {
            buffer_size,
            active: None,
            raw: None,
        }
    }

    /// Get a raw CtAPI OVERLAPPED pointer for caller-managed foreign I/O.
    ///
    /// # Safety
    ///
    /// The caller must use the raw CtAPI bindings to submit, synchronize,
    /// cancel, and collect any request using this pointer. The operation
    /// and its native buffers must stay alive until CtAPI is finished with
    /// them. Safe methods on this type manage only Rust worker requests.
    pub unsafe fn overlapped_mut(&mut self) -> *mut OVERLAPPED {
        let raw = self
            .raw
            .get_or_insert_with(|| Box::new(RawOperation::new(self.buffer_size)));
        &mut raw.overlapped
    }

    /// Return whether the safe operation has completed or is idle.
    pub fn is_complete(&self) -> bool {
        self.active
            .as_ref()
            .is_none_or(|(_, work)| work.is_complete())
    }

    /// Wait for and collect the result of the safe operation.
    pub fn get_result(&mut self, client: &CtClient) -> Result<String> {
        self.work(client)?.wait()
    }

    /// Collect the result once ready, or return None while work is pending.
    pub fn try_get_result(&mut self, client: &CtClient) -> Option<Result<String>> {
        match self.work(client) {
            Ok(work) => work.try_result(),
            Err(error) => Some(Err(error)),
        }
    }

    /// Cancel a queued operation before it starts.
    ///
    /// An already-running native call returns an unsupported-operation error.
    /// A completed operation retains its result.
    pub fn cancel(&mut self, client: &CtClient) -> Result<()> {
        self.work(client)?.cancel()
    }

    /// Clear the operation for reuse after completion.
    pub fn reset(&mut self) -> Result<()> {
        if !self.is_complete() {
            return Err(
                Error::new(ErrorKind::WouldBlock, "CtAPI operation is still running").into(),
            );
        }
        self.active = None;
        self.raw = None;
        Ok(())
    }

    pub(crate) fn start(
        &mut self,
        client: &CtClient,
        job: impl FnOnce() -> Result<String> + Send + 'static,
    ) -> Result<()> {
        if !self.is_complete() {
            return Err(
                Error::new(ErrorKind::WouldBlock, "CtAPI operation is still running").into(),
            );
        }
        self.active = Some((client.clone(), Work::spawn(job)?));
        Ok(())
    }

    fn work(&self, client: &CtClient) -> Result<&Work<String>> {
        match &self.active {
            Some((owner, work)) if owner.same_connection(client) => Ok(work),
            Some(_) => Err(Error::new(
                ErrorKind::InvalidInput,
                "CtAPI operation belongs to a different client",
            )
            .into()),
            None => Err(Error::new(
                ErrorKind::InvalidInput,
                "No CtAPI operation has been submitted",
            )
            .into()),
        }
    }
}

impl Default for AsyncOperation {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Debug for AsyncOperation {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("AsyncOperation")
            .field("is_complete", &self.is_complete())
            .field("buffer_size", &self.buffer_size)
            .finish()
    }
}

/// An awaitable client call. A started native call retains its input and client
/// until it returns, even if this future is dropped.
pub struct CtApiFuture(Work<String>);

impl Future for CtApiFuture {
    type Output = Result<String>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        self.0.poll(cx)
    }
}

impl fmt::Debug for CtApiFuture {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("CtApiFuture")
            .field("is_complete", &self.0.is_complete())
            .finish()
    }
}

/// Nonblocking submission of Cicode calls with explicit result collection.
pub trait AsyncCtClient {
    /// Submit Cicode execution and collect its result via the operation handle.
    fn cicode_async(
        &self,
        cmd: &str,
        vh_win: u32,
        mode: u32,
        async_op: &mut AsyncOperation,
    ) -> Result<()>;
}

impl AsyncCtClient for CtClient {
    fn cicode_async(
        &self,
        cmd: &str,
        vh_win: u32,
        mode: u32,
        async_op: &mut AsyncOperation,
    ) -> Result<()> {
        let job = cicode_task(self, cmd, vh_win, mode, async_op.buffer_size)?;
        async_op.start(self, job)
    }
}

fn cicode_task(
    client: &CtClient,
    cmd: &str,
    vh_win: u32,
    mode: u32,
    buffer_size: usize,
) -> Result<impl FnOnce() -> Result<String> + Send + use<>> {
    let cmd = encode_to_gbk_cstring(cmd).map_err(|_| CtApiError::InvalidParameter {
        param: "cmd".to_owned(),
        value: cmd.to_owned(),
    })?;
    u32::try_from(buffer_size)
        .map_err(|_| Error::new(ErrorKind::InvalidInput, "Cicode buffer is too large"))?;
    let client = client.clone();
    Ok(move || {
        let mut storage = vec![0; buffer_size.max(1)];
        let buffer = &mut storage[..buffer_size];
        client.cicode_into(&cmd, vh_win, mode, buffer)?;
        if buffer.is_empty() {
            return Ok(String::new());
        }
        extract_string_from_buffer(buffer)
    })
}

/// Awaitable Cicode and tag write operations.
pub trait FutureCtClient {
    /// Submit a Cicode call and return its future.
    fn cicode_future(&self, cmd: &str, vh_win: u32, mode: u32) -> Result<CtApiFuture>;
    /// Submit a tag write and return its future.
    fn tag_write_future(&self, tag: &str, value: &str) -> Result<CtApiFuture>;
}

impl FutureCtClient for CtClient {
    fn cicode_future(&self, cmd: &str, vh_win: u32, mode: u32) -> Result<CtApiFuture> {
        let job = cicode_task(self, cmd, vh_win, mode, DEFAULT_BUFFER_SIZE)?;
        Ok(CtApiFuture(Work::spawn(job)?))
    }

    fn tag_write_future(&self, tag: &str, value: &str) -> Result<CtApiFuture> {
        let tag = encode_to_gbk_cstring(tag)?;
        let value = encode_to_gbk_cstring(value)?;
        let client = self.clone();
        Ok(CtApiFuture(Work::spawn(move || {
            client
                .tag_write_encoded(&tag, &value)
                .map(|()| String::new())
        })?))
    }
}

impl FutureCtClient for Arc<CtClient> {
    fn cicode_future(&self, cmd: &str, vh_win: u32, mode: u32) -> Result<CtApiFuture> {
        (**self).cicode_future(cmd, vh_win, mode)
    }

    fn tag_write_future(&self, tag: &str, value: &str) -> Result<CtApiFuture> {
        (**self).tag_write_future(tag, value)
    }
}

/// An awaitable list call that reports the result of the native operation.
pub struct CtListFuture(Work<()>);

impl Future for CtListFuture {
    type Output = Result<()>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        self.0.poll(cx)
    }
}

impl fmt::Debug for CtListFuture {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("CtListFuture")
            .field("is_complete", &self.0.is_complete())
            .finish()
    }
}

/// Awaitable poll-list reads and writes.
pub trait FutureCtList {
    /// Refresh the list and return its future.
    fn read_future(&self) -> Result<CtListFuture>;
    /// Write a list tag and return its future.
    fn write_tag_future(&self, tag: &str, value: &str) -> Result<CtListFuture>;
}

impl FutureCtList for CtList {
    fn read_future(&self) -> Result<CtListFuture> {
        let list = self.share();
        Ok(CtListFuture(Work::spawn(move || list.read())?))
    }

    fn write_tag_future(&self, tag: &str, value: &str) -> Result<CtListFuture> {
        Ok(CtListFuture(Work::spawn(self.write_task(tag, value)?)?))
    }
}

impl FutureCtList for Arc<CtList> {
    fn read_future(&self) -> Result<CtListFuture> {
        (**self).read_future()
    }

    fn write_tag_future(&self, tag: &str, value: &str) -> Result<CtListFuture> {
        (**self).write_tag_future(tag, value)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc;

    #[test]
    fn pending_work_is_bound_to_the_originating_client() {
        let client = CtClient::from_handle(std::ptr::null_mut());
        let other_client = CtClient::from_handle(std::ptr::null_mut());
        let mut operation = AsyncOperation::with_buffer_size(512);
        let (started, running) = mpsc::channel();
        let (finish, release) = mpsc::channel();
        operation
            .start(&client, move || {
                started.send(()).unwrap();
                release.recv().unwrap();
                Ok("complete".to_owned())
            })
            .unwrap();
        running.recv().unwrap();

        assert!(operation.try_get_result(&client).is_none());
        assert!(operation.try_get_result(&other_client).unwrap().is_err());
        assert!(operation.reset().is_err());
        assert!(operation.cancel(&client).is_err());
        finish.send(()).unwrap();
        assert_eq!(operation.get_result(&client.clone()).unwrap(), "complete");
        operation.reset().unwrap();
        assert!(operation.is_complete());
    }

    #[test]
    fn worker_survives_a_dropped_waiter() {
        let (started, running) = mpsc::channel();
        let (finish, release) = mpsc::channel();
        let (completed, result) = mpsc::channel();
        let work = Work::spawn(move || {
            started.send(()).unwrap();
            release.recv().unwrap();
            completed.send(()).unwrap();
            Ok(())
        })
        .unwrap();
        running.recv().unwrap();
        drop(work);

        finish.send(()).unwrap();
        result.recv().unwrap();
    }

    #[tokio::test]
    async fn list_future_returns_worker_success_and_failure() {
        let success = CtListFuture(Work::spawn(|| Ok(())).unwrap());
        success.await.unwrap();

        let failure =
            CtListFuture(Work::spawn(|| Err(Error::other("test read failure").into())).unwrap());
        assert_eq!(
            failure.await.unwrap_err().to_string(),
            "CtAPI system call failed: test read failure"
        );
    }
}
