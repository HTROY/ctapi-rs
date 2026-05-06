//! Tag list operation related implementation
use super::CtClient;
use crate::error::{CtApiError, Result};
use ctapi_sys::*;
use encoding_rs::*;
use std::collections::HashMap;
use std::ffi::{CStr, CString};
use std::os::windows::io::RawHandle;
use std::os::windows::raw::HANDLE;
use std::sync::{Arc, RwLock};

const NULL: HANDLE = 0 as HANDLE;

/// Opaque CtAPI tag/list handle, explicitly made [`Send`] + [`Sync`].
///
/// # Safety
///
/// The handle is an opaque identifier obtained from `ctListNew` / `ctListAdd`
/// that is only ever passed back to CtAPI functions.  Concurrent access is
/// controlled by the enclosing [`CtList`] through its `RwLock`, so no data
/// race is possible.
#[derive(Clone, Copy)]
#[repr(transparent)]
struct ListHandle(RawHandle);
unsafe impl Send for ListHandle {}
unsafe impl Sync for ListHandle {}

/// Wrapper struct containing a CtAPI list handle.
///
/// # Thread Safety
///
/// `CtList` is [`Send`] + [`Sync`].
///
/// ## Lock strategy
///
/// | Field      | Synchronization | Rationale |
/// |------------|-----------------|-----------|
/// | `handle`   | **None** (immutable after `new`) | The list handle from `ctListNew` never changes; direct access is safe from any thread. |
/// | `tag_map`  | **[`RwLock`]**  | Tag lookups (`read_tag`, `write_tag`) vastly outnumber structural changes (`add_tag`, `delete_tag`). A `RwLock` lets multiple readers proceed in parallel while writes remain exclusive. |
///
/// As a result:
/// - `read()` / `read_async()` are **completely lock-free**.
/// - `read_tag()` / `write_tag()` acquire a **shared read lock** — multiple
///   threads can call them simultaneously.
/// - `add_tag()` / `delete_tag()` acquire an **exclusive write lock** — they
///   serialize against each other and against concurrent readers, but these
///   operations are rare in practice.
///
/// ## Concurrent usage — TOCTOU awareness
///
/// Between a `read()` call returning and a subsequent `read_tag()` call,
/// another thread may add or delete tags. This means `read_tag()` can
/// return [`TagNotFound`](crate::CtApiError::TagNotFound) for a tag that
/// existed when `read()` was called but was deleted before `read_tag()` ran.
/// This is inherent to lock-free reads and is consistent with the CtAPI
/// guarantee that tag data is valid until the next `ctListRead`.
///
/// # Examples
///
/// ```no_run
/// use ctapi_rs::CtClient;
/// use std::sync::Arc;
///
/// let client = Arc::new(CtClient::open(None, None, None, 0)?);
/// let list = Arc::new(Arc::clone(&client).list_new(0)?);
/// list.add_tag("Temperature")?;
/// list.add_tag("Pressure")?;
/// list.read()?;
///
/// // Multiple threads can call read_tag concurrently.
/// let list2 = Arc::clone(&list);
/// let t = std::thread::spawn(move || list2.read_tag("Temperature", 0).unwrap());
/// println!("Pressure: {}", list.read_tag("Pressure", 0)?);
/// println!("Temperature: {}", t.join().unwrap());
/// # Ok::<(), anyhow::Error>(())
/// ```
pub struct CtList {
    client: Arc<CtClient>,
    /// The CtAPI list handle returned by `ctListNew`.
    /// Immutable after construction — no lock required.
    handle: ListHandle,
    /// Tag name → per-tag handle returned by `ctListAdd`.
    ///
    /// `RwLock` instead of `Mutex` because tag reads vastly outnumber
    /// tag additions / removals in typical usage.
    tag_map: RwLock<HashMap<String, ListHandle>>,
}

impl std::fmt::Debug for CtList {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let tag_map = self.tag_map.read().expect("CtList tag_map RwLock poisoned");
        f.debug_struct("CtList")
            .field("handle", &self.handle.0)
            .field("tag_count", &tag_map.len())
            .finish()
    }
}

impl CtList {
    pub(super) fn new(client: Arc<CtClient>, handle: RawHandle) -> Self {
        Self {
            client,
            handle: ListHandle(handle),
            tag_map: RwLock::new(HashMap::new()),
        }
    }

    /// Add tag or tag element to list
    ///
    /// Once tags are added to the list, they can be read using ctListRead() and
    /// written using ctListWrite().  If a read is already pending, tags will
    /// not be read until next call to ctListRead().  ctListWrite() can be
    /// called immediately after ctListAdd() completes.
    ///
    /// Acquires an **exclusive write lock** on the tag map.
    pub fn add_tag<T: AsRef<str>>(&self, tag: T) -> Result<()> {
        let ctag = CString::new(GBK.encode(tag.as_ref()).0)?;
        let mut tag_map = self
            .tag_map
            .write()
            .expect("CtList tag_map RwLock poisoned");
        // SAFETY: self.handle.0 is a valid CtAPI list handle. ctag is a
        // GBK-encoded CString whose pointer is valid for this call.
        unsafe {
            let handle = ctListAdd(self.handle.0, ctag.as_ptr());
            if handle.is_null() {
                return Err(std::io::Error::last_os_error().into());
            }
            tag_map.insert(tag.as_ref().to_owned(), ListHandle(handle));
        }
        Ok(())
    }

    /// Add tag (extended version with more parameters)
    ///
    /// Besides ctListAdd functionality, also supports setting raw value flag,
    /// polling period and deadband.  If using ctListAdd, default polling
    /// period is 500ms, raw value flag defaults to engineering value FALSE.
    ///
    /// Acquires an **exclusive write lock** on the tag map.
    pub fn add_tag_ex<T: AsRef<str>>(
        &self,
        tag: T,
        raw: bool,
        poll_period: i32,
        deadband: f64,
    ) -> Result<()> {
        let ctag = CString::new(GBK.encode(tag.as_ref()).0)?;
        let mut tag_map = self
            .tag_map
            .write()
            .expect("CtList tag_map RwLock poisoned");
        // SAFETY: self.handle.0 is a valid CtAPI list handle. ctag is a
        // GBK-encoded CString. raw, poll_period, deadband are primitive
        // values matching the CtAPI parameter types.
        unsafe {
            let handle = ctListAddEx(self.handle.0, ctag.as_ptr(), raw, poll_period, deadband);
            if handle.is_null() {
                return Err(std::io::Error::last_os_error().into());
            }
            tag_map.insert(tag.as_ref().to_owned(), ListHandle(handle));
        }
        Ok(())
    }

    /// Delete tag created with ctListAdd
    ///
    /// Program can call ctListDelete() while there are pending reads or writes
    /// in another thread.  ctListWrite() and ctListRead() will return after
    /// tag deletion.
    ///
    /// Acquires an **exclusive write lock** on the tag map.
    pub fn delete_tag<T: AsRef<str>>(&self, tag: T) -> Result<()> {
        let mut tag_map = self
            .tag_map
            .write()
            .expect("CtList tag_map RwLock poisoned");
        match tag_map.get(tag.as_ref()) {
            Some(handle) =>
            // SAFETY: handle.0 is a valid tag handle from ctListAdd/ctListAddEx.
            // The write lock on tag_map prevents concurrent access.
            unsafe {
                if !ctListDelete(handle.0) {
                    return Err(std::io::Error::last_os_error().into());
                }
                tag_map.remove(tag.as_ref());
                Ok(())
            },
            None => Err(CtApiError::TagNotFound {
                tag: tag.as_ref().to_string(),
            }),
        }
    }

    /// Read tags in list
    ///
    /// This function will read tags attached to the list.  Once data is read
    /// from I/O device, ctListData() can be called to get tag values.  If
    /// reading is not successful, ctListData() will return errors for tags
    /// that cannot be read.
    ///
    /// Tags can be added and removed from list while ctListRead() is pending.
    ///
    /// **Lock-free**: accesses the immutable list handle directly.
    pub fn read(&self) -> Result<()> {
        // SAFETY: self.handle.0 is a valid CtAPI list handle. NULL OVERLAPPED
        // pointer means synchronous (blocking) read.
        unsafe {
            if !ctListRead(self.handle.0, NULL as *mut OVERLAPPED) {
                Err(std::io::Error::last_os_error().into())
            } else {
                Ok(())
            }
        }
    }

    /// Read tags in list asynchronously
    ///
    /// Non-blocking version of [`read`].  The read operation starts and
    /// completes in the background.  Use [`AsyncOperation::get_result`] or
    /// poll for completion.
    ///
    /// **Lock-free**: accesses the immutable list handle directly.
    ///
    /// # Parameters
    /// * `async_op` - [`AsyncOperation`](crate::AsyncOperation) to track this operation.
    ///
    /// # Examples
    /// ```no_run
    /// # use ctapi_rs::{CtClient, AsyncOperation};
    /// # use std::sync::Arc;
    /// let client = Arc::new(CtClient::open(None, None, None, 0)?);
    /// let list = Arc::clone(&client).list_new(0)?;
    /// list.add_tag("Tag1")?;
    ///
    /// let mut async_op = AsyncOperation::new();
    /// list.read_async(&mut async_op)?;
    ///
    /// while !async_op.is_complete() {
    ///     std::thread::sleep(std::time::Duration::from_millis(10));
    /// }
    ///
    /// let value = list.read_tag("Tag1", 0)?;
    /// # Ok::<(), anyhow::Error>(())
    /// ```
    pub fn read_async(&self, async_op: &mut crate::AsyncOperation) -> Result<()> {
        // SAFETY: self.handle.0 is a valid CtAPI list handle. async_op.overlapped_mut()
        // returns a valid OVERLAPPED pointer that tracks async completion.
        unsafe {
            if !ctListRead(self.handle.0, async_op.overlapped_mut()) {
                let error = std::io::Error::last_os_error();
                if error.raw_os_error() != Some(997) {
                    return Err(error.into());
                }
            }
            Ok(())
        }
    }

    /// Get values of tags in list
    ///
    /// Call this function after [`read`] completes for added tags.
    ///
    /// Acquires a **shared read lock** on the tag map — multiple threads may
    /// call `read_tag` concurrently without blocking each other.
    pub fn read_tag<T: AsRef<str>>(&self, tag: T, mode: u32) -> Result<String> {
        let tag_map = self.tag_map.read().expect("CtList tag_map RwLock poisoned");
        match tag_map.get(tag.as_ref()) {
            Some(handle) =>
            // SAFETY: handle.0 is a valid tag handle from ctListAdd. buffer is a
            // fixed-size stack array. mode is a valid DWORD flag.
            unsafe {
                let mut buffer = [0u8; 256];
                if !ctListData(
                    handle.0,
                    buffer.as_mut_ptr().cast(),
                    buffer.len() as DWORD,
                    mode,
                ) {
                    return Err(std::io::Error::last_os_error().into());
                }
                Ok(GBK
                    .decode(CStr::from_bytes_until_nul(buffer.as_ref())?.to_bytes())
                    .0
                    .to_string())
            },
            None => Err(CtApiError::TagNotFound {
                tag: tag.as_ref().to_string(),
            }),
        }
    }

    /// Write single tag in list
    ///
    /// Acquires a **shared read lock** on the tag map — multiple threads may
    /// call `write_tag` concurrently without blocking each other.
    pub fn write_tag<T: AsRef<str>>(&self, tag: T, value: T) -> Result<()> {
        let tag_map = self.tag_map.read().expect("CtList tag_map RwLock poisoned");
        if let Some(handle) = tag_map.get(tag.as_ref()) {
            let cvalue = CString::new(GBK.encode(value.as_ref()).0)?;
            // SAFETY: handle.0 is a valid tag handle. cvalue is a GBK-encoded
            // CString. NULL OVERLAPPED means synchronous write.
            unsafe {
                if !ctListWrite(handle.0, cvalue.as_ptr(), NULL as *mut OVERLAPPED) {
                    return Err(std::io::Error::last_os_error().into());
                }
            }
            Ok(())
        } else {
            Err(CtApiError::TagNotFound {
                tag: tag.as_ref().to_string(),
            })
        }
    }

    /// Write single tag in list asynchronously
    ///
    /// Non-blocking version of [`write_tag`].  The write completes in the
    /// background.
    ///
    /// Acquires a **shared read lock** on the tag map — multiple threads may
    /// call `write_tag_async` concurrently without blocking each other.
    ///
    /// # Parameters
    /// * `tag`      - Tag name to write.
    /// * `value`    - Value to write.
    /// * `async_op` - [`AsyncOperation`](crate::AsyncOperation) to track this operation.
    ///
    /// # Examples
    /// ```no_run
    /// # use ctapi_rs::{CtClient, AsyncOperation};
    /// # use std::sync::Arc;
    /// let client = Arc::new(CtClient::open(None, None, None, 0)?);
    /// let list = Arc::clone(&client).list_new(0)?;
    /// list.add_tag("Tag1")?;
    ///
    /// let mut async_op = AsyncOperation::new();
    /// list.write_tag_async("Tag1", "42", &mut async_op)?;
    ///
    /// while !async_op.is_complete() {
    ///     std::thread::sleep(std::time::Duration::from_millis(10));
    /// }
    /// # Ok::<(), anyhow::Error>(())
    /// ```
    pub fn write_tag_async<T: AsRef<str>>(
        &self,
        tag: T,
        value: T,
        async_op: &mut crate::AsyncOperation,
    ) -> Result<()> {
        let tag_map = self.tag_map.read().expect("CtList tag_map RwLock poisoned");
        if let Some(handle) = tag_map.get(tag.as_ref()) {
            let cvalue = CString::new(GBK.encode(value.as_ref()).0)?;
            // SAFETY: handle.0 is a valid tag handle. cvalue is a GBK-encoded
            // CString. async_op.overlapped_mut() returns a valid OVERLAPPED pointer.
            unsafe {
                if !ctListWrite(handle.0, cvalue.as_ptr(), async_op.overlapped_mut()) {
                    let error = std::io::Error::last_os_error();
                    if error.raw_os_error() != Some(997) {
                        return Err(error.into());
                    }
                }
            }
            Ok(())
        } else {
            Err(CtApiError::TagNotFound {
                tag: tag.as_ref().to_string(),
            })
        }
    }
}

impl Drop for CtList {
    fn drop(&mut self) {
        if !self.handle.0.is_null() {
            // Safety: the handle was created by ctListNew and is valid.
            // `handle` is a plain field — no lock needed in Drop.
            // Arc guarantees Drop runs only after all clones are gone,
            // so no other thread can be using the handle concurrently.
            unsafe { ctListFree(self.handle.0) };
        }
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn test_list_thread_safety() {
        // Verify Send + Sync at compile time.
        fn assert_send<T: Send>() {}
        fn assert_sync<T: Sync>() {}
        assert_send::<super::CtList>();
        assert_sync::<super::CtList>();
    }
}
