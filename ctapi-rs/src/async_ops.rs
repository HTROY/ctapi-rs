//! Asynchronous operations using OVERLAPPED I/O
//!
//! This module provides asynchronous versions of CtAPI operations that support
//! non-blocking I/O through Windows OVERLAPPED structures.

use crate::error::{CtApiError, Result};
use crate::CtClient;
use ctapi_sys::*;
use encoding_rs::*;
use std::ffi::CString;

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
    buffer: Vec<i8>,
}

impl AsyncOperation {
    /// Create a new async operation
    ///
    /// Initializes a new OVERLAPPED structure for asynchronous operations.
    pub fn new() -> Self {
        Self {
            overlapped: unsafe { std::mem::zeroed() },
            buffer: vec![0i8; 256],
        }
    }

    /// Create a new async operation with custom buffer size
    ///
    /// # Parameters
    /// * `buffer_size` - Size of the internal buffer for results
    pub fn with_buffer_size(buffer_size: usize) -> Self {
        Self {
            overlapped: unsafe { std::mem::zeroed() },
            buffer: vec![0i8; buffer_size],
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

    /// Get mutable reference to internal buffer
    pub(crate) fn buffer_mut(&mut self) -> &mut [i8] {
        &mut self.buffer
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
        // Check Internal field of OVERLAPPED to determine completion
        // Note: This is a simplified check. For production, use ctGetOverlappedResult
        unsafe {
            let internal = *(&self.overlapped as *const OVERLAPPED as *const usize);
            internal != 259 // STATUS_PENDING
        }
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

            // Decode the result from buffer
            let u8_buffer: &[u8] = std::mem::transmute(&self.buffer[..]);
            let cstr = std::ffi::CStr::from_bytes_until_nul(u8_buffer)
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
                let u8_buffer: &[u8] = std::mem::transmute(&self.buffer[..]);
                let result = std::ffi::CStr::from_bytes_until_nul(u8_buffer)
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
        self.overlapped = unsafe { std::mem::zeroed() };
        self.buffer.fill(0);
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
                async_op.buffer_mut().as_mut_ptr(),
                async_op.buffer_mut().len() as u32,
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
        assert_eq!(async_op.buffer.len(), 256);
    }

    #[test]
    fn test_async_operation_with_buffer_size() {
        let async_op = AsyncOperation::with_buffer_size(512);
        assert_eq!(async_op.buffer.len(), 512);
    }

    #[test]
    fn test_async_operation_reset() {
        let mut async_op = AsyncOperation::new();
        async_op.buffer[0] = 42;
        async_op.reset();
        assert_eq!(async_op.buffer[0], 0);
    }

    #[test]
    fn test_async_operation_debug() {
        let async_op = AsyncOperation::new();
        let debug_str = format!("{:?}", async_op);
        assert!(debug_str.contains("AsyncOperation"));
        assert!(debug_str.contains("buffer_size"));
    }
}
