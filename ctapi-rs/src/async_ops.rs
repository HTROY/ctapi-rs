//! Asynchronous operations using OVERLAPPED I/O
//!
//! This module provides asynchronous versions of CtAPI operations that support
//! non-blocking I/O through Windows OVERLAPPED structures.

use crate::error::{CtApiError, Result};
use crate::CtClient;
use ctapi_sys::*;
use encoding_rs::*;
use std::ffi::CString;
use windows_sys::Win32::Foundation::{HANDLE, CloseHandle};

extern "system" {
    fn CreateEventA(
        lp_event_attributes: *mut std::ffi::c_void,
        bManualReset: i32,
        bInitialState: i32,
        lp_name: *const u8,
    ) -> HANDLE;
}

/// Helper function: Convert string to GBK encoded CString
fn encode_to_gbk_cstring(s: &str) -> std::result::Result<CString, std::ffi::NulError> {
    let (encoded, _, _) = GBK.encode(s);
    CString::new(encoded)
}

/// Represents an asynchronous operation handle
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
/// client.cicode_async("SomeFunction()", 0, 0, &mut async_op)?;
///
/// // Do other work...
///
/// // Wait for completion
/// let result = async_op.get_result(&client)?;
/// println!("Result: {}", result);
/// # Ok::<(), ctapi_rs::CtApiError>(())
/// ```
pub struct AsyncOperation {
    overlapped: OVERLAPPED,
    buffer: Vec<u8>,
    event_handle: HANDLE,
}

impl AsyncOperation {
    /// Create a new async operation
    ///
    /// Initializes a new OVERLAPPED structure for asynchronous operations.
    pub fn new() -> Self {
        Self::with_buffer_size(256)
    }

    /// Create a new async operation with custom buffer size
    ///
    /// # Parameters
    /// * `buffer_size` - Size of the internal buffer for results
    pub fn with_buffer_size(buffer_size: usize) -> Self {
        // Create an event for the OVERLAPPED structure
        let event_handle = unsafe { CreateEventA(std::ptr::null_mut(), 1, 0, std::ptr::null()) };

        // Allocate buffer for results
        let mut buffer = vec![0u8; buffer_size];

        let mut overlapped = OVERLAPPED::new();
        overlapped.hEvent = event_handle as *mut std::ffi::c_void;
        overlapped.dwStatus = 0;
        overlapped.dwLength = 0;
        overlapped.pData = buffer.as_mut_ptr();

        Self {
            overlapped,
            buffer,
            event_handle,
        }
    }

    /// Get mutable reference to internal OVERLAPPED structure
    ///
    /// # Safety
    ///
    /// This is unsafe because the OVERLAPPED structure must not be modified
    /// while an operation is in progress. Misuse can lead to undefined behavior.
    pub unsafe fn overlapped_mut(&mut self) -> *mut OVERLAPPED {
        &mut self.overlapped
    }

    /// Check if the async operation has completed
    ///
    /// # Return Value
    /// Returns `true` if the operation has completed, `false` otherwise.
    ///
    /// # Examples
    /// ```no_run
    /// # use ctapi_rs::{CtClient, AsyncOperation};
    /// # let client = CtClient::open(None, None, None, 0)?;
    /// let mut async_op = AsyncOperation::new();
    /// client.cicode_async("Sleep(5)", 0, 0, &mut async_op)?;
    ///
    /// while !async_op.is_complete() {
    ///     // Do other work
    ///     std::thread::sleep(std::time::Duration::from_millis(100));
    /// }
    /// # Ok::<(), ctapi_rs::CtApiError>(())
    /// ```
    pub fn is_complete(&self) -> bool {
        // Check if operation has completed using STATUS_PENDING check
        // dwStatus != STATUS_PENDING (0x103) means operation completed
        const STATUS_PENDING: DWORD = 0x103;
        self.overlapped.dwStatus != STATUS_PENDING
    }

    /// Wait for the async operation to complete and get the result
    ///
    /// This method blocks until the operation completes and returns the result.
    ///
    /// # Parameters
    /// * `client` - The CtClient instance used for the operation
    ///
    /// # Return Value
    /// Returns the operation result as a String.
    ///
    /// # Errors
    /// * [`CtApiError::System`] - Operation failed or was cancelled
    ///
    /// # Examples
    /// ```no_run
    /// # use ctapi_rs::{CtClient, AsyncOperation, AsyncCtClient};
    /// # let client = CtClient::open(None, None, None, 0)?;
    /// let mut async_op = AsyncOperation::new();
    /// client.cicode_async("Time(1)", 0, 0, &mut async_op)?;
    ///
    /// let result = async_op.get_result(&client)?;
    /// println!("Time: {}", result);
    /// # Ok::<(), ctapi_rs::CtApiError>(())
    /// ```
    pub fn get_result(&mut self, client: &CtClient) -> Result<String> {
        let mut bytes_transferred: u32 = 0;

        unsafe {
            // Wait for completion
            if !ctGetOverlappedResult(
                client.handle(),
                &mut self.overlapped,
                &mut bytes_transferred,
                true, // bWait = true
            ) {
                return Err(std::io::Error::last_os_error().into());
            }

            // Extract result from the buffer
            let result_len = bytes_transferred.min(self.buffer.len() as u32) as usize;
            let result_slice = &self.buffer[..result_len];
            let cstr = std::ffi::CStr::from_bytes_until_nul(result_slice)
                .map_err(CtApiError::FromBytesUntilNul)?;
            let decoded = GBK.decode(cstr.to_bytes()).0.to_string();
            Ok(decoded)
        }
    }

    /// Try to get the result without blocking
    ///
    /// Returns `None` if the operation is still in progress.
    ///
    /// # Parameters
    /// * `client` - The CtClient instance used for the operation
    ///
    /// # Return Value
    /// Returns `Some(Result<String>)` if complete, `None` if still pending.
    ///
    /// # Examples
    /// ```no_run
    /// # use ctapi_rs::{CtClient, AsyncOperation, AsyncCtClient};
    /// # let client = CtClient::open(None, None, None, 0)?;
    /// let mut async_op = AsyncOperation::new();
    /// client.cicode_async("LongRunningFunction()", 0, 0, &mut async_op)?;
    ///
    /// loop {
    ///     match async_op.try_get_result(&client) {
    ///         Some(Ok(result)) => {
    ///             println!("Got result: {}", result);
    ///             break;
    ///         }
    ///         Some(Err(e)) => {
    ///             eprintln!("Error: {}", e);
    ///             break;
    ///         }
    ///         None => {
    ///             // Still running, do other work
    ///             std::thread::sleep(std::time::Duration::from_millis(100));
    ///         }
    ///     }
    /// }
    /// # Ok::<(), ctapi_rs::CtApiError>(())
    /// ```
    pub fn try_get_result(&mut self, client: &CtClient) -> Option<Result<String>> {
        let mut bytes_transferred: u32 = 0;

        unsafe {
            // Don't wait, just check status
            if ctGetOverlappedResult(
                client.handle(),
                &mut self.overlapped,
                &mut bytes_transferred,
                false, // bWait = false
            ) {
                // Operation completed successfully
                let result_len = bytes_transferred.min(self.buffer.len() as u32) as usize;
                let result_slice = &self.buffer[..result_len];
                let result = std::ffi::CStr::from_bytes_until_nul(result_slice)
                    .map_err(CtApiError::FromBytesUntilNul)
                    .map(|cstr| GBK.decode(cstr.to_bytes()).0.to_string());
                Some(result)
            } else {
                let error = std::io::Error::last_os_error();
                if error.raw_os_error() == Some(997) {
                    // ERROR_IO_INCOMPLETE
                    None
                } else {
                    Some(Err(error.into()))
                }
            }
        }
    }

    /// Cancel the pending async operation
    ///
    /// Attempts to cancel the operation. Note that cancellation may not be
    /// immediate and the operation may still complete.
    ///
    /// # Parameters
    /// * `client` - The CtClient instance used for the operation
    ///
    /// # Return Value
    /// Returns `Ok(true)` if cancellation was successful.
    ///
    /// # Examples
    /// ```no_run
    /// # use ctapi_rs::{CtClient, AsyncOperation, AsyncCtClient};
    /// # let client = CtClient::open(None, None, None, 0)?;
    /// let mut async_op = AsyncOperation::new();
    /// client.cicode_async("Sleep(60)", 0, 0, &mut async_op)?;
    ///
    /// // Changed mind, cancel it
    /// async_op.cancel(&client)?;
    /// # Ok::<(), ctapi_rs::CtApiError>(())
    /// ```
    pub fn cancel(&mut self, client: &CtClient) -> Result<bool> {
        unsafe {
            if !ctCancelIO(client.handle(), &mut self.overlapped) {
                return Err(std::io::Error::last_os_error().into());
            }
            Ok(true)
        }
    }

    /// Reset the async operation for reuse
    ///
    /// Clears the OVERLAPPED structure and buffer, allowing the same
    /// AsyncOperation instance to be used for a new operation.
    pub fn reset(&mut self) {
        // Reset OVERLAPPED but preserve the event handle
        let event_handle = self.event_handle;
        self.overlapped = OVERLAPPED::new();
        self.overlapped.hEvent = event_handle as *mut std::ffi::c_void;
        self.overlapped.pData = self.buffer.as_mut_ptr();
        self.buffer.fill(0);
    }
}

impl Drop for AsyncOperation {
    fn drop(&mut self) {
        // Clean up the event handle
        if !self.event_handle.is_null() && self.event_handle as isize != -1 {
            unsafe {
                CloseHandle(self.event_handle);
            }
        }
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
            .field("event_handle", &self.event_handle)
            .finish()
    }
}

/// Extension trait for async operations on CtClient
pub trait AsyncCtClient {
    /// Execute Cicode function asynchronously
    ///
    /// Non-blocking version of `cicode()`. The operation will complete in the
    /// background and can be polled or waited on using the returned AsyncOperation.
    ///
    /// # Parameters
    /// * `cmd` - Cicode command string
    /// * `vh_win` - Window handle, usually 0
    /// * `mode` - Execution mode flag
    /// * `async_op` - AsyncOperation to use for this operation
    ///
    /// # Return Value
    /// Returns `Ok(())` if the operation was started successfully.
    ///
    /// # Errors
    /// * [`CtApiError::System`] - Failed to start operation
    ///
    /// # Examples
    /// ```no_run
    /// use ctapi_rs::{CtClient, AsyncOperation, AsyncCtClient};
    ///
    /// let client = CtClient::open(None, None, None, 0)?;
    /// let mut async_op = AsyncOperation::new();
    ///
    /// client.cicode_async("Time(1)", 0, 0, &mut async_op)?;
    /// let result = async_op.get_result(&client)?;
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
                let error = std::io::Error::last_os_error();
                // ERROR_IO_PENDING (997) is expected for async operations
                if error.raw_os_error() != Some(997) {
                    return Err(error.into());
                }
            }
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_async_operation_creation() {
        let async_op = AsyncOperation::new();
        assert!(!async_op.event_handle.is_null());
        assert_eq!(async_op.buffer.len(), 256);
    }

    #[test]
    fn test_async_operation_with_buffer_size() {
        let async_op = AsyncOperation::with_buffer_size(512);
        assert!(!async_op.event_handle.is_null());
        assert_eq!(async_op.buffer.len(), 512);
    }

    #[test]
    fn test_async_operation_reset() {
        let mut async_op = AsyncOperation::new();
        let original_handle = async_op.event_handle;
        async_op.buffer[0] = 42;
        async_op.reset();
        // After reset, the event handle should remain the same
        assert_eq!(original_handle, async_op.event_handle);
        assert_eq!(async_op.buffer[0], 0);
    }

    #[test]
    fn test_async_operation_debug() {
        let async_op = AsyncOperation::new();
        let debug_str = format!("{:?}", async_op);
        assert!(debug_str.contains("AsyncOperation"));
    }
}
