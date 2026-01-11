//! Citect SCADA API client implementation
use crate::error::{CtApiError, Result};

use ctapi_sys::*;
use encoding_rs::*;

use std::ffi::{CStr, CString};
use std::fmt::Display;
use std::io::Error;
use std::ops::{Add, Sub};
use std::os::windows::io::RawHandle;
use std::os::windows::raw::HANDLE;

const NULL: HANDLE = 0 as HANDLE;

/// Helper function: Convert string to GBK encoded CString
fn encode_to_gbk_cstring(s: &str) -> std::result::Result<CString, std::ffi::NulError> {
    let (encoded, _, _) = GBK.encode(s);
    CString::new(encoded)
}

/// Helper function: Safely extract string from buffer
fn extract_string_from_buffer(buffer: &[i8]) -> std::result::Result<String, CtApiError> {
    // Convert i8 array to u8 array to meet CStr::from_bytes_until_nul requirements
    let u8_buffer: &[u8] = unsafe { std::mem::transmute(buffer) };

    // Create CStr, ensure null-terminated
    let cstr = CStr::from_bytes_until_nul(u8_buffer).map_err(CtApiError::FromBytesUntilNul)?;

    // Decode to UTF-8 string using GBK
    let decoded = GBK.decode(cstr.to_bytes()).0.to_string();
    Ok(decoded)
}

/// Optimized decoding function: Specifically handles API response buffer decoding
/// Unifies string extraction and GBK decoding with better error handling
fn decode_response_buffer(buffer: &[i8]) -> Result<String> {
    // Use extract_string_from_buffer, which already includes correct string extraction and GBK decoding
    let decoded_string = extract_string_from_buffer(buffer)?;

    // Check for empty response
    if decoded_string.is_empty() {
        return Err(CtApiError::Other {
            code: 0,
            message: "API returned empty response".to_string(),
        });
    }

    Ok(decoded_string)
}

/// Citect SCADA API client structure
///
/// # Thread Safety
///
/// `CtClient` implements `Send` and `Sync`, allowing it to be safely shared across threads.
/// However, users must be aware of the following:
///
/// - The underlying CtAPI.dll handle is shared when cloning
/// - Multiple threads can call read operations concurrently
/// - Write operations should be synchronized by the caller if needed
/// - When using `Arc<CtClient>`, ensure all derived objects (`CtFind`, `CtList`) are
///   dropped before the client to avoid use-after-free
///
/// # Safety
///
/// The `Send` and `Sync` implementations assume that CtAPI.dll functions are thread-safe
/// for concurrent reads on the same handle. This is based on Citect SCADA documentation.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct CtClient {
    handle: RawHandle,
}

// SAFETY: CtClient only contains a raw handle pointer.
// The CtAPI.dll library is documented to be thread-safe for concurrent operations
// on the same connection handle. The handle itself is just a pointer value that
// can be safely sent between threads.
unsafe impl Send for CtClient {}

// SAFETY: Multiple threads can safely call CtAPI functions with the same handle
// for read operations. Write operations through tag_write use FFI calls that
// are synchronized by the underlying CtAPI.dll implementation.
unsafe impl Sync for CtClient {}

impl CtClient {
    /// Get client handle (internal use)
    pub(crate) fn handle(&self) -> RawHandle {
        self.handle
    }

    /// Open connection to Citect SCADA API
    ///
    /// Initializes CTAPI.DLL and establishes connection to Citect SCADA. If Citect SCADA
    /// is not running when this function is called, the function will exit and report an error.
    /// This function must be called before calling any other CTAPI functions.
    ///
    /// # Parameters
    /// * `computer` - Optional computer name or IP address. If None, connects to local computer
    /// * `user` - Optional username. If None, uses empty string
    /// * `password` - Optional password. If None, uses empty string
    /// * `mode` - Connection mode flags (see CT_OPEN_* constants in [`crate::constants`])
    ///
    /// # Return Value
    /// Returns `Result` containing client handle, returns error if connection fails
    ///
    /// # Errors
    /// * [`CtApiError::ConnectionFailed`] - Cannot establish connection
    /// * [`CtApiError::System`] - System call failed
    ///
    /// # Examples
    /// ```no_run
    /// use ctapi_rs::{CtClient, Result};
    ///
    /// // Connect to local Citect SCADA
    /// let client = CtClient::open(None, None, None, 0)?;
    ///
    /// // Connect to remote computer
    /// let client = CtClient::open(
    ///     Some("192.168.1.100"),
    ///     Some("Manager"),
    ///     Some("password"),
    ///     0
    /// )?;
    ///
    /// // Use reconnect mode
    /// use ctapi_rs::constants::CT_OPEN_RECONNECT;
    /// let client = CtClient::open(None, None, None, CT_OPEN_RECONNECT)?;
    /// # Ok::<(), ctapi_rs::CtApiError>(())
    /// ```
    pub fn open(
        computer: Option<&str>,
        user: Option<&str>,
        password: Option<&str>,
        mode: u32,
    ) -> Result<Self> {
        let computer = computer.and_then(|s| CString::new(s).ok());
        let user = user.and_then(|s| CString::new(s).ok());
        let password = password.and_then(|s| CString::new(s).ok());

        unsafe {
            let handle = ctOpen(
                computer.unwrap_or_default().as_ptr(),
                user.unwrap_or_default().as_ptr(),
                password.unwrap_or_default().as_ptr(),
                mode,
            );
            if handle.is_null() {
                Err(std::io::Error::last_os_error().into())
            } else {
                Ok(Self { handle })
            }
        }
    }

    /// Read tag value
    ///
    /// Reads the value, quality, and timestamp of a given tag and returns the data using
    /// Citect SCADA scaling in string format. The function requests to retrieve the given tag
    /// from the Citect SCADA I/O server.
    ///
    /// # Parameters
    /// * `tag` - Tag name, must be valid UTF-8 string
    ///
    /// # Return Value
    /// Returns string representation of tag value, returns error if read fails
    ///
    /// # Errors
    /// * [`CtApiError::TagNotFound`] - Tag does not exist
    /// * [`CtApiError::System`] - System call failed
    /// * [`CtApiError::Encoding`] - Encoding/decoding error
    ///
    /// # Examples
    /// ```no_run
    /// use ctapi_rs::CtClient;
    ///
    /// let client = CtClient::open(None, None, None, 0)?;
    ///
    /// // Read single tag
    /// let value = client.tag_read("Temperature")?;
    /// println!("Temperature value: {}", value);
    /// # Ok::<(), ctapi_rs::CtApiError>(())
    /// ```
    pub fn tag_read<T: AsRef<str>>(&self, tag: T) -> Result<String> {
        // Use fixed-size buffer to prevent buffer overflow
        let mut buffer = [0i8; 256];

        // Convert input tag to GBK encoding for compatibility
        let tag = encode_to_gbk_cstring(tag.as_ref()).map_err(|_| CtApiError::TagNotFound {
            tag: tag.as_ref().to_string(),
        })?;

        unsafe {
            if !ctTagRead(
                self.handle,
                tag.as_ptr(),
                buffer.as_mut_ptr(),
                buffer.len() as DWORD,
            ) {
                return Err(std::io::Error::last_os_error().into());
            }

            // Use optimized decoding function, unified handling of string extraction, validation and GBK decoding
            decode_response_buffer(&buffer)
        }
    }

    /// Read tag value (extended version)
    ///
    /// Besides reading the tag value, also returns timestamp, quality and other metadata information.
    /// This is useful for applications that need time series data or quality information.
    ///
    /// # Parameters
    /// * `tag` - Tag name
    /// * `tagvalue_items` - Output tag value items structure containing timestamp and quality information
    ///
    /// # Return Value
    /// Returns string representation of tag value, returns error if read fails
    ///
    /// # Errors
    /// * [`CtApiError::TagNotFound`] - Tag does not exist
    /// * [`CtApiError::System`] - System call failed
    ///
    /// # Examples
    /// ```no_run
    /// use ctapi_rs::{CtClient, CtTagValueItems};
    ///
    /// let client = CtClient::open(None, None, None, 0)?;
    /// let mut value_items = CtTagValueItems::default();
    ///
    /// let value = client.tag_read_ex("Pressure", &mut value_items)?;
    /// println!("Pressure value: {}", value);
    /// println!("Timestamp: {}", value_items.timestamp);
    /// println!("Quality: {}", value_items.quality_general);
    /// # Ok::<(), ctapi_rs::CtApiError>(())
    /// ```
    pub fn tag_read_ex<T: AsRef<str>>(
        &self,
        tag: T,
        tagvalue_items: &mut CtTagValueItems,
    ) -> Result<String> {
        let mut buffer = [0i8; 256];
        let tag = encode_to_gbk_cstring(tag.as_ref()).map_err(|_| CtApiError::TagNotFound {
            tag: tag.as_ref().to_string(),
        })?;

        unsafe {
            if !ctTagReadEx(
                self.handle,
                tag.as_ptr(),
                buffer.as_mut_ptr(),
                256,
                tagvalue_items,
            ) {
                return Err(std::io::Error::last_os_error().into());
            }

            // Use optimized decoding function, unified handling of string extraction, validation and GBK decoding
            decode_response_buffer(&buffer)
        }
    }

    /// Write tag value
    ///
    /// Writes value, quality and timestamp to the given Citect SCADA I/O device variable tag.
    /// The value is converted to the correct data type, then scaled and written to the tag.
    ///
    /// # Parameters
    /// * `tag` - Tag name
    /// * `value` - Value to write, must implement Display trait
    ///
    /// # Return Value
    /// Returns whether operation was successful
    ///
    /// # Errors
    /// * [`CtApiError::TagNotFound`] - Tag does not exist or not writable
    /// * [`CtApiError::System`] - System call failed
    ///
    /// # Examples
    /// ```no_run
    /// use ctapi_rs::CtClient;
    ///
    /// let client = CtClient::open(None, None, None, 0)?;
    ///
    /// // Write numeric value
    /// client.tag_write("Temperature", 25.5)?;
    ///
    /// // Write boolean value
    /// client.tag_write("Pump_Start", true)?;
    ///
    /// // Write string value
    /// client.tag_write("Status", "Running")?;
    /// # Ok::<(), ctapi_rs::CtApiError>(())
    /// ```
    pub fn tag_write<T, U>(&self, tag: T, value: U) -> Result<bool>
    where
        T: AsRef<str>,
        U: Display + Add<Output = U> + Sub<Output = U> + Copy + PartialEq,
    {
        // Use helper function to optimize encoding process
        let tag = encode_to_gbk_cstring(tag.as_ref()).map_err(|_| CtApiError::TagNotFound {
            tag: tag.as_ref().to_string(),
        })?;
        let s_value = CString::new(value.to_string())?;

        unsafe {
            if !ctTagWrite(self.handle, tag.as_ptr(), s_value.as_ptr()) {
                return Err(std::io::Error::last_os_error().into());
            }
            Ok(true)
        }
    }

    /// Execute Cicode function
    ///
    /// Executes Cicode function on the connected Citect SCADA computer.
    /// Allows control of Citect SCADA or getting information returned from Cicode functions.
    /// Can call built-in or user-defined Cicode functions.
    ///
    /// # Parameters
    /// * `cmd` - Cicode command string containing function name and parameters
    /// * `vh_win` - Window handle, usually 0
    /// * `mode` - Execution mode flag
    ///
    /// # Return Value
    /// Returns string result of function execution
    ///
    /// # Errors
    /// * [`CtApiError::UnsupportedOperation`] - Function not supported
    /// * [`CtApiError::System`] - System call failed
    ///
    /// # Examples
    /// ```no_run
    /// use ctapi_rs::CtClient;
    ///
    /// let client = CtClient::open(None, None, None, 0)?;
    ///
    /// // Get current time
    /// let time = client.cicode("Time(1)", 0, 0)?;
    /// println!("Current time: {}", time);
    ///
    /// // Call custom Cicode function
    /// let result = client.cicode("MyCustomFunction(123)", 0, 0)?;
    /// # Ok::<(), ctapi_rs::CtApiError>(())
    /// ```
    pub fn cicode(&self, cmd: &str, vh_win: u32, mode: u32) -> Result<String> {
        let mut buffer = [0i8; 256];
        let cmd = encode_to_gbk_cstring(cmd).map_err(|_| CtApiError::InvalidParameter {
            param: "cmd".to_string(),
            value: cmd.to_string(),
        })?;

        unsafe {
            if !ctCicode(
                self.handle,
                cmd.as_ptr(),
                vh_win,
                mode,
                buffer.as_mut_ptr(),
                buffer.len() as DWORD,
                NULL as *mut OVERLAPPED,
            ) {
                return Err(std::io::Error::last_os_error().into());
            }

            // Use helper function for decoding, improving code consistency
            decode_response_buffer(&buffer)
        }
    }

    /// Find first object matching criteria
    pub fn find_first(
        &self,
        table_name: &str,
        filter: &str,
        cluster: Option<&str>,
    ) -> super::CtFind<'_> {
        // Optimization: Use helper function to avoid unnecessary unsafe code
        let table_name =
            encode_to_gbk_cstring(table_name).unwrap_or_else(|_| CString::new("").unwrap());
        let filter = encode_to_gbk_cstring(filter).unwrap_or_else(|_| CString::new("").unwrap());

        match cluster {
            Some(cluster) => {
                let cluster =
                    encode_to_gbk_cstring(cluster).unwrap_or_else(|_| CString::new("").unwrap());
                super::CtFind::new(self, table_name, filter, Some(cluster))
            }
            None => super::CtFind::new(self, table_name, filter, None),
        }
    }

    /// Create new list
    pub fn list_new(&self, mode: u32) -> Result<super::CtList<'_>> {
        unsafe {
            let handle = ctListNew(self.handle, mode);
            if handle.is_null() {
                return Err(std::io::Error::last_os_error().into());
            }
            Ok(super::CtList::new(self, handle))
        }
    }
}

impl Drop for CtClient {
    fn drop(&mut self) {
        // SAFETY: This is safe because:
        // 1. We're the last owner of this particular CtClient instance
        // 2. The handle is valid (or null, which ctClose handles safely)
        // 3. When using Arc<CtClient>, Rust ensures this is called only once
        //    after all references are gone
        // 
        // Note: If derived objects (CtFind, CtList) outlive the client in unsafe code,
        // this could cause use-after-free. Users should ensure proper lifetimes.
        unsafe {
            if !self.handle.is_null() && !ctClose(self.handle) {
                let os_error = Error::last_os_error();
                eprintln!("Warning: ctClose failed in CtClient::drop: {os_error}");
            }
        }
    }
}

/// Initialize resources for new CtAPI client instance
pub fn ct_client_create() -> Result<CtClient> {
    let handle = unsafe { ctClientCreate() };

    if handle.is_null() {
        return Err(Error::last_os_error().into());
    }
    Ok(CtClient { handle })
}

/// Clean up resources for given CtAPI instance
///
/// # Safety
///
/// The caller must ensure that:
/// - `h_ctapi` is a valid HANDLE obtained from a previous CtAPI function call
/// - `h_ctapi` has not been destroyed or freed previously
/// - No other threads are concurrently using this handle
pub unsafe fn ct_client_destroy(h_ctapi: HANDLE) -> Result<bool> {
    if !ctClientDestroy(h_ctapi) {
        return Err(Error::last_os_error().into());
    }
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::CtApiError;

    #[test]
    fn test_client_drop() {
        // Test that client drop doesn't crash
        // Since real CtAPI connection is needed, only test basic functionality of struct
        let handle = std::ptr::null_mut();
        let client = CtClient { handle };

        // Test struct basic functionality
        assert_eq!(client.handle, std::ptr::null_mut());
    }

    #[test]
    fn test_handle_getter() {
        let handle = std::ptr::null_mut();
        let client = CtClient { handle };

        assert_eq!(client.handle(), handle);
    }

    #[test]
    fn test_error_types() {
        // Test error type related functionality
        let error = CtApiError::TagNotFound {
            tag: "test_tag".to_string(),
        };
        assert!(error.is_tag_error());
        assert!(!error.is_connection_error());
    }

    #[test]
    fn test_unsafe_trait_implementations() {
        // Test Send and Sync trait implementations
        fn assert_send<T: Send>() {}
        fn assert_sync<T: Sync>() {}

        assert_send::<CtClient>();
        assert_sync::<CtClient>();
    }

    #[test]
    fn test_client_equality() {
        let handle1 = 0x12345678 as *mut std::ffi::c_void;
        let handle2 = 0x12345678 as *mut std::ffi::c_void;
        let handle3 = 0x87654321 as *mut std::ffi::c_void;

        let client1 = CtClient { handle: handle1 };
        let client2 = CtClient { handle: handle2 };
        let client3 = CtClient { handle: handle3 };

        // Equal handles should be equal
        assert_eq!(client1, client2);
        assert_ne!(client1, client3);

        // Test cloning
        let client1_clone = client1.clone();
        assert_eq!(client1, client1_clone);
        
        // Prevent drop from being called on fake handles
        std::mem::forget(client1);
        std::mem::forget(client2);
        std::mem::forget(client3);
        std::mem::forget(client1_clone);
    }

    #[test]
    fn test_decode_response_buffer() {
        // Test empty buffer
        let empty_buffer: Vec<i8> = Vec::new();
        let result = decode_response_buffer(&empty_buffer);
        assert!(result.is_err());

        // Test buffer with only null characters
        let null_buffer = vec![0i8; 10];
        let result = decode_response_buffer(&null_buffer);
        assert!(result.is_err());

        // Test valid string buffer (avoid using stack array)
        let test_string = "Hello World";
        let mut buffer: Vec<i8> = Vec::with_capacity(256);
        buffer.extend_from_slice(
            &test_string
                .as_bytes()
                .iter()
                .map(|&b| b as i8)
                .collect::<Vec<i8>>(),
        );
        buffer.push(0); // Null character termination
        buffer.extend_from_slice(&vec![0i8; 256 - buffer.len()]);

        let result = decode_response_buffer(&buffer);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), test_string);
    }

    #[test]
    fn test_extract_string_from_buffer() {
        // Test empty buffer - should fail as there's no null terminator
        let empty_buffer: Vec<i8> = Vec::new();
        let result = extract_string_from_buffer(&empty_buffer);
        assert!(result.is_err());

        // Test buffer with only null characters
        let null_buffer = vec![0i8; 5];
        let result = extract_string_from_buffer(&null_buffer);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), "");

        // Test string with null character termination
        let test_string = "Test String";
        let mut buffer: Vec<i8> = Vec::with_capacity(256);
        buffer.extend_from_slice(
            &test_string
                .as_bytes()
                .iter()
                .map(|&b| b as i8)
                .collect::<Vec<i8>>(),
        );
        buffer.push(0); // Null character termination
        buffer.extend_from_slice(&vec![0i8; 256 - buffer.len()]);

        let result = extract_string_from_buffer(&buffer);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), test_string);
    }
}
