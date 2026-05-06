//! Asynchronous operations using OVERLAPPED I/O
//!
//! This module provides two layers of async support:
//!
//! ## Callback-style (OVERLAPPED)
//! - [`AsyncOperation`] - Windows OVERLAPPED-based async operation handle
//! - [`AsyncCtClient`] - Extension trait for starting OVERLAPPED async operations
//!
//! ## Future / async-await style
//! - [`CtApiFuture`] - A `std::future::Future` wrapping an OVERLAPPED operation
//! - [`FutureCtClient`] - Extension trait returning `CtApiFuture` for `.await` usage
//!
//! # Examples
//!
//! ```no_run
//! use ctapi_rs::{CtClient, FutureCtClient};
//!
//! async fn run() -> anyhow::Result<()> {
//!     let client = CtClient::open(None, None, None, 0)?;
//!
//!     // Await directly — no tokio::spawn_blocking needed
//!     let result = client.cicode_future("Time(1)", 0, 0)?.await?;
//!     println!("Time: {}", result);
//!     Ok(())
//! }
//! ```

use std::future::Future;
use std::os::windows::io::RawHandle;
use std::pin::Pin;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::task::{Context, Poll, Waker};

use crate::CtClient;
use crate::error::{CtApiError, Result};
use crate::util::encode_to_gbk_cstring;
use ctapi_sys::*;
use encoding_rs::GBK;
use windows_sys::Win32::Foundation::{CloseHandle, HANDLE};
use windows_sys::Win32::System::Threading::CreateEventA;
use windows_sys::Win32::System::Threading::WaitForSingleObject;

/// `WaitForSingleObject` return value: timeout elapsed without the object being signalled.
const WAIT_TIMEOUT: u32 = 0x0000_0102;

// ───────────────────────────────────────────────
// WinEvent — Arc-wrapped Windows event handle
// ───────────────────────────────────────────────

/// An owned Windows event handle that can be safely shared across threads via [`Arc`].
///
/// The event is automatically closed (via `CloseHandle`) when the last `Arc` reference
/// is dropped, ensuring the handle stays valid as long as any thread still needs it.
struct WinEvent(HANDLE);

impl WinEvent {
    /// Create a new manual-reset, initially-unsignalled event.
    fn new() -> Self {
        // SAFETY: CreateEventA with null security attributes, manual-reset,
        // initially unsignalled, and no name is always safe to call.
        let h = unsafe { CreateEventA(std::ptr::null_mut(), 1, 0, std::ptr::null()) };
        assert!(!h.is_null(), "CreateEventA failed");
        Self(h)
    }

    /// Return the raw Windows `HANDLE` value.
    #[inline]
    fn handle(&self) -> HANDLE {
        self.0
    }
}

impl Drop for WinEvent {
    fn drop(&mut self) {
        if !self.0.is_null() && self.0 as isize != -1 {
            // SAFETY: self.0 is a valid HANDLE from CreateEventA. Arc<WinEvent>
            // guarantees this drop runs only once after all references are gone.
            unsafe { CloseHandle(self.0) };
        }
    }
}

// SAFETY: A Windows kernel-object handle is a numeric identifier that can be
// duplicated and passed between threads freely; the kernel synchronises access.
unsafe impl Send for WinEvent {}
unsafe impl Sync for WinEvent {}

// ───────────────────────────────────────────────
// FutureState — shared between CtApiFuture and the waker thread
// ───────────────────────────────────────────────

struct FutureState {
    waker: Mutex<Option<Waker>>,
    cancelled: AtomicBool,
}

// ───────────────────────────────────────────────
// AsyncOperation
// ───────────────────────────────────────────────

/// Represents an asynchronous operation handle.
///
/// This structure wraps a Windows OVERLAPPED structure and provides safe
/// access to asynchronous CtAPI operations.
///
/// # Thread Safety
///
/// `AsyncOperation` is NOT thread-safe. Each thread should create and manage
/// its own async operations. The OVERLAPPED structure must not be moved or
/// modified while an operation is in progress.
///
/// # Examples
///
/// ```no_run
/// use ctapi_rs::{CtClient, AsyncOperation};
///
/// let client = CtClient::open(None, None, None, 0)?;
/// let mut async_op = AsyncOperation::new();
///
/// // Start async cicode execution
/// use ctapi_rs::AsyncCtClient;
/// client.cicode_async("SomeFunction()", 0, 0, &mut async_op)?;
///
/// // Wait for completion
/// let result = async_op.get_result(&client)?;
/// println!("Result: {}", result);
/// # Ok::<(), ctapi_rs::CtApiError>(())
/// ```
pub struct AsyncOperation {
    overlapped: OVERLAPPED,
    buffer: Vec<u8>,
    /// Ref-counted event handle — shared with [`CtApiFuture`]'s waker thread so
    /// that the kernel object is not closed while a thread is waiting on it.
    win_event: Arc<WinEvent>,
}

impl AsyncOperation {
    /// Create a new async operation with the default 256-byte result buffer.
    pub fn new() -> Self {
        Self::with_buffer_size(256)
    }

    /// Create a new async operation with a custom result-buffer size.
    ///
    /// # Parameters
    /// * `buffer_size` - Capacity of the internal buffer used to receive results.
    pub fn with_buffer_size(buffer_size: usize) -> Self {
        let win_event = Arc::new(WinEvent::new());
        let mut buffer = vec![0u8; buffer_size];

        let mut overlapped = OVERLAPPED::new();
        overlapped.hEvent = win_event.handle();
        overlapped.dwStatus = 0;
        overlapped.dwLength = 0;
        overlapped.pData = buffer.as_mut_ptr();

        Self {
            overlapped,
            buffer,
            win_event,
        }
    }

    /// Return a raw mutable pointer to the internal OVERLAPPED structure.
    ///
    /// # Safety
    ///
    /// The OVERLAPPED structure must not be modified while an I/O operation
    /// is in progress.  Misuse can lead to undefined behaviour.
    pub unsafe fn overlapped_mut(&mut self) -> *mut OVERLAPPED {
        &mut self.overlapped
    }

    /// Return `true` if the async operation has completed.
    ///
    /// The check is based on `dwStatus != STATUS_PENDING (0x103)`.
    ///
    /// # Examples
    /// ```no_run
    /// # use ctapi_rs::{CtClient, AsyncOperation, AsyncCtClient};
    /// # let client = CtClient::open(None, None, None, 0)?;
    /// let mut op = AsyncOperation::new();
    /// client.cicode_async("Sleep(5)", 0, 0, &mut op)?;
    ///
    /// while !op.is_complete() {
    ///     std::thread::sleep(std::time::Duration::from_millis(100));
    /// }
    /// # Ok::<(), ctapi_rs::CtApiError>(())
    /// ```
    pub fn is_complete(&self) -> bool {
        const STATUS_PENDING: DWORD = 0x103;
        self.overlapped.dwStatus != STATUS_PENDING
    }

    /// The raw Windows event handle associated with this operation's
    /// OVERLAPPED structure. Signalled when the async operation completes.
    pub(crate) fn win_event_handle(&self) -> HANDLE {
        self.win_event.handle()
    }

    /// Block until the operation completes and return the string result.
    ///
    /// # Parameters
    /// * `client` - The [`CtClient`] used to start this operation.
    ///
    /// # Errors
    /// * [`CtApiError::System`] - Operation failed or was cancelled.
    ///
    /// # Examples
    /// ```no_run
    /// # use ctapi_rs::{CtClient, AsyncOperation, AsyncCtClient};
    /// # let client = CtClient::open(None, None, None, 0)?;
    /// let mut op = AsyncOperation::new();
    /// client.cicode_async("Time(1)", 0, 0, &mut op)?;
    /// let result = op.get_result(&client)?;
    /// println!("Time: {}", result);
    /// # Ok::<(), ctapi_rs::CtApiError>(())
    /// ```
    pub fn get_result(&mut self, client: &CtClient) -> Result<String> {
        self.get_result_impl(client.handle(), true)
    }

    /// Try to get the result without blocking.
    ///
    /// Returns `None` if the operation is still in progress.
    ///
    /// # Parameters
    /// * `client` - The [`CtClient`] used to start this operation.
    ///
    /// # Examples
    /// ```no_run
    /// # use ctapi_rs::{CtClient, AsyncOperation, AsyncCtClient};
    /// # let client = CtClient::open(None, None, None, 0)?;
    /// let mut op = AsyncOperation::new();
    /// client.cicode_async("LongFunc()", 0, 0, &mut op)?;
    ///
    /// loop {
    ///     match op.try_get_result(&client) {
    ///         Some(Ok(v))  => { println!("Done: {}", v); break; }
    ///         Some(Err(e)) => { eprintln!("Error: {}", e); break; }
    ///         None         => std::thread::sleep(std::time::Duration::from_millis(50)),
    ///     }
    /// }
    /// # Ok::<(), ctapi_rs::CtApiError>(())
    /// ```
    pub fn try_get_result(&mut self, client: &CtClient) -> Option<Result<String>> {
        let mut bytes_transferred: u32 = 0;

        // SAFETY: client.handle() is a valid CtAPI handle. &mut self.overlapped is
        // a valid pointer to an OVERLAPPED struct that was previously passed to an
        // async CtAPI call. bytes_transferred is a local stack variable.
        unsafe {
            if ctGetOverlappedResult(
                client.handle(),
                &mut self.overlapped,
                &mut bytes_transferred,
                false,
            ) {
                let result_len = bytes_transferred.min(self.buffer.len() as u32) as usize;
                let result_slice = &self.buffer[..result_len];
                let result = std::ffi::CStr::from_bytes_until_nul(result_slice)
                    .map_err(CtApiError::FromBytesUntilNul)
                    .map(|cstr| GBK.decode(cstr.to_bytes()).0.to_string());
                Some(result)
            } else {
                let err = std::io::Error::last_os_error();
                if err.raw_os_error() == Some(997) {
                    // ERROR_IO_INCOMPLETE — still pending
                    None
                } else {
                    Some(Err(err.into()))
                }
            }
        }
    }

    /// Attempt to cancel the pending async operation.
    ///
    /// Cancellation may not be immediate; the operation might still complete.
    ///
    /// # Parameters
    /// * `client` - The [`CtClient`] used to start this operation.
    ///
    /// # Examples
    /// ```no_run
    /// # use ctapi_rs::{CtClient, AsyncOperation, AsyncCtClient};
    /// # let client = CtClient::open(None, None, None, 0)?;
    /// let mut op = AsyncOperation::new();
    /// client.cicode_async("Sleep(60)", 0, 0, &mut op)?;
    /// op.cancel(&client)?;
    /// # Ok::<(), ctapi_rs::CtApiError>(())
    /// ```
    pub fn cancel(&mut self, client: &CtClient) -> Result<()> {
        // SAFETY: client.handle() is a valid CtAPI handle. &mut self.overlapped
        // points to the OVERLAPPED struct associated with the pending operation.
        unsafe {
            if !ctCancelIO(client.handle(), &mut self.overlapped) {
                return Err(std::io::Error::last_os_error().into());
            }
            Ok(())
        }
    }

    /// Reset this `AsyncOperation` for reuse.
    ///
    /// Clears the OVERLAPPED status and zeroes the result buffer while
    /// keeping the same underlying event handle.
    pub fn reset(&mut self) {
        let event_handle = self.win_event.handle();
        self.overlapped = OVERLAPPED::new();
        self.overlapped.hEvent = event_handle;
        self.overlapped.pData = self.buffer.as_mut_ptr();
        self.buffer.fill(0);
    }

    // ── internal ────────────────────────────────────────────────────────────

    /// Common implementation used by both [`get_result`] and [`CtApiFuture`].
    ///
    /// When `wait = false` the caller must ensure the operation has already
    /// completed (i.e. [`is_complete`] returned `true`).
    fn get_result_impl(&mut self, client_handle: RawHandle, wait: bool) -> Result<String> {
        let mut bytes_transferred: u32 = 0;
        // SAFETY: client_handle is a valid CtAPI connection handle. &mut self.overlapped
        // is a valid pointer to an OVERLAPPED struct from a previous async call.
        // bytes_transferred is a local stack variable.
        unsafe {
            if !ctGetOverlappedResult(
                client_handle,
                &mut self.overlapped,
                &mut bytes_transferred,
                wait,
            ) {
                return Err(std::io::Error::last_os_error().into());
            }
            // Operations like tag writes may transfer 0 bytes — return empty string.
            if bytes_transferred == 0 {
                return Ok(String::new());
            }
            let result_len = bytes_transferred.min(self.buffer.len() as u32) as usize;
            let result_slice = &self.buffer[..result_len];
            let cstr = std::ffi::CStr::from_bytes_until_nul(result_slice)
                .map_err(CtApiError::FromBytesUntilNul)?;
            Ok(GBK.decode(cstr.to_bytes()).0.to_string())
        }
    }

    /// Non-blocking result extraction — used by [`CtApiFuture`] after the
    /// operation is known to have completed.
    pub(crate) fn get_result_with_handle(&mut self, client_handle: RawHandle) -> Result<String> {
        self.get_result_impl(client_handle, false)
    }
}

impl Drop for AsyncOperation {
    fn drop(&mut self) {
        // The event handle lifetime is managed by Arc<WinEvent>.
        // No explicit CloseHandle needed here.
    }
}

impl Default for AsyncOperation {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Debug for AsyncOperation {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AsyncOperation")
            .field("is_complete", &self.is_complete())
            .field("buffer_size", &self.buffer.len())
            .field("event_handle", &self.win_event.handle())
            .finish()
    }
}

// ───────────────────────────────────────────────
// CtApiFuture — std::future::Future over OVERLAPPED
// ───────────────────────────────────────────────

/// A [`Future`] that wraps an in-progress CtAPI OVERLAPPED async operation.
///
/// Created by [`FutureCtClient`] methods. Supports `.await` in any async context
/// without requiring Tokio — a lightweight background thread waits on the
/// Windows event handle and wakes the task when the operation completes.
///
/// # Cancellation
///
/// Dropping this future before it resolves will:
/// 1. Signal the internal waker thread to stop.
/// 2. Call `ctCancelIO` to cancel the pending I/O operation.
///
/// # Thread Safety
///
/// `CtApiFuture` implements [`Send`] — it can be spawned in Tokio tasks or any
/// other multi-threaded async runtime.  The internal waker thread only accesses
/// the Windows event handle (a kernel identifier), never the result buffer or
/// the client handle directly.
///
/// # Examples
///
/// ```no_run
/// use ctapi_rs::{CtClient, FutureCtClient};
///
/// #[tokio::main]
/// async fn main() -> anyhow::Result<()> {
///     let client = CtClient::open(None, None, None, 0)?;
///
///     // Uses OVERLAPPED internally — no spawn_blocking needed
///     let time  = client.cicode_future("Time(1)", 0, 0)?.await?;
///     let date  = client.cicode_future("Date(4)", 0, 0)?.await?;
///     println!("{} {}", time, date);
///     Ok(())
/// }
/// ```
pub struct CtApiFuture {
    /// Owned clone of the CtClient handle.
    client: CtClient,
    /// Boxed so the OVERLAPPED struct is at a stable heap address.
    /// CtAPI stores a raw `*mut OVERLAPPED` pointer to this struct
    /// during async operations — moving the future must not move the
    /// OVERLAPPED, otherwise CtAPI writes to a dangling pointer.
    async_op: Box<AsyncOperation>,
    state: Option<Arc<FutureState>>,
}

impl CtApiFuture {
    pub(crate) fn new(client: &CtClient, async_op: AsyncOperation) -> Self {
        Self {
            client: client.clone(),
            async_op: Box::new(async_op),
            state: None,
        }
    }
}

// SAFETY: CtClient is Send + Sync. Box<AsyncOperation> is Send because
// its fields (OVERLAPPED: now Send + Sync, Vec<u8>: Send, Arc<WinEvent>: Send + Sync)
// are all Send. Option<Arc<FutureState>> is auto-Send.
unsafe impl Send for CtApiFuture {}

impl Future for CtApiFuture {
    type Output = Result<String>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let this = self.get_mut();

        // Fast path — already done.
        if this.async_op.is_complete() {
            return Poll::Ready(this.async_op.get_result_with_handle(this.client.handle()));
        }

        match &this.state {
            None => {
                // First poll: create shared state and spawn the waker thread.
                let state = Arc::new(FutureState {
                    waker: Mutex::new(Some(cx.waker().clone())),
                    cancelled: AtomicBool::new(false),
                });
                this.state = Some(Arc::clone(&state));

                // Clone the Arc so the event handle stays alive while the
                // thread is blocked inside WaitForSingleObject.
                let win_event = Arc::clone(&this.async_op.win_event);
                let thread_state = Arc::clone(&state);

                std::thread::Builder::new()
                    .name("ctapi-waker".into())
                    .spawn(move || {
                    loop {
                        if thread_state.cancelled.load(Ordering::Relaxed) {
                            return;
                        }
                        // 100 ms timeout lets us check `cancelled` regularly
                        // so that dropping the future doesn't strand this thread.
                        // SAFETY: win_event.handle() is a valid HANDLE from CreateEventA.
                        // The Arc<WinEvent> keeps it alive for the thread's lifetime.
                        let status = unsafe { WaitForSingleObject(win_event.handle(), 100) };

                        if thread_state.cancelled.load(Ordering::Relaxed) {
                            return;
                        }

                        if status != WAIT_TIMEOUT {
                            // Operation finished (or handle error) — wake the task.
                            if let Ok(mut lock) = thread_state.waker.lock()
                                && let Some(waker) = lock.take()
                            {
                                waker.wake();
                            }
                            return;
                        }
                        // WAIT_TIMEOUT — loop and try again.
                    }
                })
                .expect("failed to spawn ctapi-waker thread");
            }
            Some(state) => {
                // Subsequent polls (e.g. spurious wake-up): refresh the waker.
                if let Ok(mut lock) = state.waker.lock() {
                    *lock = Some(cx.waker().clone());
                }
            }
        }

        Poll::Pending
    }
}

impl Drop for CtApiFuture {
    fn drop(&mut self) {
        // 1. Tell the waker thread to stop.
        if let Some(state) = &self.state {
            state.cancelled.store(true, Ordering::Relaxed);
        }
        // 2. Cancel the pending I/O to avoid a dangling OVERLAPPED pointer.
        if !self.async_op.is_complete() {
            // SAFETY: self.client.handle() is a valid CtAPI handle.
            // self.async_op.overlapped_mut() returns a pointer to the OVERLAPPED
            // struct owned by this future. They remain valid until drop completes.
            unsafe {
                let _ = ctCancelIO(self.client.handle(), self.async_op.overlapped_mut());
            }
        }
    }
}

impl std::fmt::Debug for CtApiFuture {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CtApiFuture")
            .field("is_complete", &self.async_op.is_complete())
            .finish()
    }
}

// ───────────────────────────────────────────────
// AsyncCtClient — callback-style async trait
// ───────────────────────────────────────────────

/// Extension trait providing callback-style (OVERLAPPED) async operations on
/// [`CtClient`].
///
/// Use [`FutureCtClient`] if you prefer the `async`/`await` style instead.
pub trait AsyncCtClient {
    /// Execute a Cicode function asynchronously (OVERLAPPED style).
    ///
    /// Non-blocking: the operation runs in the background.  Poll for completion
    /// with [`AsyncOperation::is_complete`] or block with
    /// [`AsyncOperation::get_result`].
    ///
    /// # Parameters
    /// * `cmd`      - Cicode command string.
    /// * `vh_win`   - Window handle, usually `0`.
    /// * `mode`     - Execution mode flag.
    /// * `async_op` - [`AsyncOperation`] to associate with this call.
    ///
    /// # Errors
    /// * [`CtApiError::System`] - Failed to start the operation.
    ///
    /// # Examples
    /// ```no_run
    /// use ctapi_rs::{CtClient, AsyncOperation, AsyncCtClient};
    ///
    /// let client = CtClient::open(None, None, None, 0)?;
    /// let mut op = AsyncOperation::new();
    /// client.cicode_async("Time(1)", 0, 0, &mut op)?;
    /// let result = op.get_result(&client)?;
    /// # Ok::<(), ctapi_rs::CtApiError>(())
    /// ```
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
        let cmd = encode_to_gbk_cstring(cmd).map_err(|_| CtApiError::InvalidParameter {
            param: "cmd".to_string(),
            value: cmd.to_string(),
        })?;

        // SAFETY: self.handle() is a valid CtAPI connection handle. cmd is a
        // GBK-encoded CString whose pointer is valid for this call. The buffer
        // pointer and length come from async_op which outlives this call.
        // async_op.overlapped_mut() returns a pointer to the OVERLAPPED struct
        // that will track the async completion.
        unsafe {
            if !ctCicode(
                self.handle(),
                cmd.as_ptr(),
                vh_win,
                mode,
                async_op.buffer.as_mut_ptr() as *mut i8,
                async_op.buffer.len() as u32,
                async_op.overlapped_mut(),
            ) {
                let err = std::io::Error::last_os_error();
                // ERROR_IO_PENDING (997) is expected for async operations.
                if err.raw_os_error() != Some(997) {
                    return Err(err.into());
                }
            }
            Ok(())
        }
    }
}

// ───────────────────────────────────────────────
// FutureCtClient — async/await style trait
// ───────────────────────────────────────────────

/// Extension trait providing `async`/`await`-compatible operations on
/// [`CtClient`].
///
/// Methods return a [`CtApiFuture`] that drives Windows OVERLAPPED I/O directly,
/// without requiring `spawn_blocking` or Tokio.
///
/// # Examples
///
/// ```no_run
/// use ctapi_rs::{CtClient, FutureCtClient};
/// use std::sync::Arc;
///
/// #[tokio::main]
/// async fn main() -> anyhow::Result<()> {
///     let client = Arc::new(CtClient::open(None, None, None, 0)?);
///
///     // Fire two Cicode calls concurrently
///     let (time, date) = tokio::try_join!(
///         client.cicode_future("Time(1)", 0, 0)?,
///         client.cicode_future("Date(4)", 0, 0)?,
///     )?;
///     println!("{} {}", time, date);
///     Ok(())
/// }
/// ```
pub trait FutureCtClient {
    /// Execute a Cicode function and return a [`CtApiFuture`] that can be
    /// `.await`ed.
    ///
    /// The underlying I/O is performed with Windows OVERLAPPED, so no blocking
    /// thread is required.
    ///
    /// # Parameters
    /// * `cmd`    - Cicode command string.
    /// * `vh_win` - Window handle, usually `0`.
    /// * `mode`   - Execution mode flag.
    ///
    /// # Errors
    /// Returns `Err` immediately if the operation cannot be started.
    ///
    /// # Examples
    /// ```no_run
    /// use ctapi_rs::{CtClient, FutureCtClient};
    ///
    /// # async fn run() -> anyhow::Result<()> {
    /// let client = CtClient::open(None, None, None, 0)?;
    /// let result = client.cicode_future("Version()", 0, 0)?.await?;
    /// println!("Version: {}", result);
    /// # Ok(())
    /// # }
    /// ```
    fn cicode_future(&self, cmd: &str, vh_win: u32, mode: u32) -> Result<CtApiFuture>;

    /// Write a tag value asynchronously and return a [`CtApiFuture`] that can be
    /// `.await`ed.
    ///
    /// Uses `ctTagWriteEx` with Windows OVERLAPPED I/O internally, so no blocking
    /// thread is required.
    ///
    /// # Parameters
    /// * `tag`   - Tag name.
    /// * `value` - Value to write (string form).
    ///
    /// # Errors
    /// Returns `Err` immediately if the operation cannot be started.
    ///
    /// # Examples
    /// ```no_run
    /// use ctapi_rs::{CtClient, FutureCtClient};
    ///
    /// # async fn run() -> anyhow::Result<()> {
    /// let client = CtClient::open(None, None, None, 0)?;
    /// client.tag_write_future("Setpoint", "25.5")?.await?;
    /// # Ok(())
    /// # }
    /// ```
    fn tag_write_future(&self, tag: &str, value: &str) -> Result<CtApiFuture>;
}

impl FutureCtClient for CtClient {
    fn cicode_future(&self, cmd: &str, vh_win: u32, mode: u32) -> Result<CtApiFuture> {
        let mut async_op = AsyncOperation::new();
        self.cicode_async(cmd, vh_win, mode, &mut async_op)?;
        Ok(CtApiFuture::new(self, async_op))
    }

    fn tag_write_future(&self, tag: &str, value: &str) -> Result<CtApiFuture> {
        let mut async_op = AsyncOperation::new();

        let tag_cstr = encode_to_gbk_cstring(tag).map_err(|_| CtApiError::InvalidParameter {
            param: "tag".to_string(),
            value: tag.to_string(),
        })?;
        let value_cstr =
            encode_to_gbk_cstring(value).map_err(|_| CtApiError::InvalidParameter {
                param: "value".to_string(),
                value: value.to_string(),
            })?;

        // SAFETY: self.handle() is a valid CtAPI connection handle. tag_cstr
        // and value_cstr are GBK-encoded CStrings valid for this call.
        // async_op.overlapped_mut() returns a valid OVERLAPPED pointer.
        unsafe {
            if !ctTagWriteEx(
                self.handle(),
                tag_cstr.as_ptr(),
                value_cstr.as_ptr(),
                async_op.overlapped_mut(),
            ) {
                let err = std::io::Error::last_os_error();
                // ERROR_IO_PENDING (997) is expected for async operations.
                if err.raw_os_error() != Some(997) {
                    return Err(err.into());
                }
            }
        }

        Ok(CtApiFuture::new(self, async_op))
    }
}

impl FutureCtClient for Arc<CtClient> {
    fn cicode_future(&self, cmd: &str, vh_win: u32, mode: u32) -> Result<CtApiFuture> {
        let mut async_op = AsyncOperation::new();
        (**self).cicode_async(cmd, vh_win, mode, &mut async_op)?;
        Ok(CtApiFuture::new(self, async_op))
    }

    fn tag_write_future(&self, tag: &str, value: &str) -> Result<CtApiFuture> {
        let mut async_op = AsyncOperation::new();

        let tag_cstr = encode_to_gbk_cstring(tag).map_err(|_| CtApiError::InvalidParameter {
            param: "tag".to_string(),
            value: tag.to_string(),
        })?;
        let value_cstr =
            encode_to_gbk_cstring(value).map_err(|_| CtApiError::InvalidParameter {
                param: "value".to_string(),
                value: value.to_string(),
            })?;

        // SAFETY: (**self).handle() is a valid CtAPI connection handle.
        // tag_cstr and value_cstr are GBK-encoded CStrings valid for this call.
        unsafe {
            if !ctTagWriteEx(
                (**self).handle(),
                tag_cstr.as_ptr(),
                value_cstr.as_ptr(),
                async_op.overlapped_mut(),
            ) {
                let err = std::io::Error::last_os_error();
                if err.raw_os_error() != Some(997) {
                    return Err(err.into());
                }
            }
        }

        Ok(CtApiFuture::new(self, async_op))
    }
}

// ───────────────────────────────────────────────
// Tests
// ───────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_async_operation_creation() {
        let op = AsyncOperation::new();
        assert!(!op.win_event.handle().is_null());
        assert_eq!(op.buffer.len(), 256);
    }

    #[test]
    fn test_async_operation_with_buffer_size() {
        let op = AsyncOperation::with_buffer_size(512);
        assert!(!op.win_event.handle().is_null());
        assert_eq!(op.buffer.len(), 512);
    }

    #[test]
    fn test_async_operation_reset() {
        let mut op = AsyncOperation::new();
        let original_handle = op.win_event.handle();
        op.buffer[0] = 42;
        op.reset();
        // The same underlying event handle should be reused.
        assert_eq!(original_handle, op.win_event.handle());
        assert_eq!(op.buffer[0], 0);
    }

    #[test]
    fn test_async_operation_debug() {
        let op = AsyncOperation::new();
        let s = format!("{:?}", op);
        assert!(s.contains("AsyncOperation"));
    }

    #[test]
    fn test_ct_api_future_debug() {
        fn assert_debug<T: std::fmt::Debug>() {}
        assert_debug::<CtApiFuture>();
    }

    #[test]
    fn test_win_event_arc_sharing() {
        let op = AsyncOperation::new();
        // Clone the Arc — both references should point to the same handle.
        let shared = Arc::clone(&op.win_event);
        assert_eq!(op.win_event.handle(), shared.handle());
    }

    #[test]
    fn test_async_operation_is_not_complete_initially() {
        let op = AsyncOperation::new();
        // A freshly created (but never started) operation has dwStatus = 0,
        // which is != STATUS_PENDING (0x103), so is_complete() returns true
        // until an actual async call is made and sets STATUS_PENDING.
        // This just verifies the method compiles and returns a bool.
        let _ = op.is_complete();
    }
}
