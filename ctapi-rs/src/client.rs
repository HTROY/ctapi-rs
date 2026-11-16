//! Citect SCADA API client implementation
use crate::error::{CtApiError, Result};
use crate::util::{encode_strict, encode_to_gbk_cstring};

use ctapi_sys::*;
use encoding_rs::*;

use std::ffi::{CStr, CString};
use std::fmt::Display;
use std::io::Error;
// Add, Sub were used by tag_write bounds (removed)
use std::os::windows::io::RawHandle;
use std::os::windows::raw::HANDLE;
use std::sync::Arc;

/// Helper function: Safely extract string from buffer
pub(crate) fn extract_string_from_buffer(buffer: &[i8]) -> std::result::Result<String, CtApiError> {
    // SAFETY: i8 and u8 have identical layout (1 byte, alignment 1). The pointer
    // comes from a live &[i8] reference, so it is valid for buffer.len() bytes.
    let u8_buffer: &[u8] =
        unsafe { std::slice::from_raw_parts(buffer.as_ptr() as *const u8, buffer.len()) };

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

/// Timestamp and quality metadata returned by an extended tag read.
#[derive(Debug, Clone, Copy)]
#[repr(C, packed)]
#[allow(missing_docs)]
pub struct CtTagValueItems {
    pub length: u32,
    pub timestamp: u64,
    pub value_timestamp: u64,
    pub quality_timestamp: u64,
    pub quality_general: u8,
    pub quality_substatus: u8,
    pub quality_limit: u8,
    pub quality_extended_substatus: u8,
    pub quality_datasource_error: u32,
    pub boverride: bool,
    pub control_mode: bool,
}

impl CtTagValueItems {
    /// Return the size of the native metadata structure.
    pub fn length(&self) -> u32 {
        self.length
    }
}

impl Default for CtTagValueItems {
    fn default() -> Self {
        Self::from(ctapi_sys::CtTagValueItems::default())
    }
}

impl From<ctapi_sys::CtTagValueItems> for CtTagValueItems {
    fn from(value: ctapi_sys::CtTagValueItems) -> Self {
        Self {
            length: std::mem::size_of::<ctapi_sys::CtTagValueItems>() as u32,
            timestamp: value.timestamp,
            value_timestamp: value.value_timestamp,
            quality_timestamp: value.quality_timestamp,
            quality_general: value.quality_general,
            quality_substatus: value.quality_substatus,
            quality_limit: value.quality_limit,
            quality_extended_substatus: value.quality_extended_substatus,
            quality_datasource_error: value.quality_datasource_error,
            boverride: value.boverride != 0,
            control_mode: value.control_mode != 0,
        }
    }
}

/// Citect SCADA connection shared by its Rust clones.
///
/// CtAPI supports sharing the connection for reads. Callers should coordinate
/// multi-operation sequences and writes when the order of operations matters.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CtClient {
    inner: Arc<ClientHandle>,
}

#[derive(Debug, PartialEq, Eq)]
struct ClientHandle(RawHandle);

// SAFETY: CtAPI permits its client handle to be used by multiple threads.
// The last Arc closes the handle, after all users release their ownership.
unsafe impl Send for ClientHandle {}
unsafe impl Sync for ClientHandle {}

impl CtClient {
    pub(crate) fn from_handle(handle: RawHandle) -> Self {
        Self {
            inner: Arc::new(ClientHandle(handle)),
        }
    }

    pub(crate) fn handle(&self) -> RawHandle {
        self.inner.0
    }

    pub(crate) fn same_connection(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.inner, &other.inner)
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
    /// use ctapi_rs::CtClient;
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
        let computer = encode_strict(computer.unwrap_or_default(), "computer")?;
        let user = encode_strict(user.unwrap_or_default(), "user")?;
        let password = encode_strict(password.unwrap_or_default(), "password")?;

        // SAFETY: ctOpen is an FFI call. All CString pointers are valid for the
        // duration of the call. mode is a valid u32 flag value.
        unsafe {
            let handle = ctOpen(computer.as_ptr(), user.as_ptr(), password.as_ptr(), mode);
            if handle.is_null() {
                Err(std::io::Error::last_os_error().into())
            } else {
                Ok(Self::from_handle(handle))
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
        let tag =
            encode_to_gbk_cstring(tag.as_ref()).map_err(|_| CtApiError::InvalidParameter {
                param: "tag".to_string(),
                value: tag.as_ref().to_string(),
            })?;

        // SAFETY: self.handle() is a valid CtAPI connection handle. tag is a
        // GBK-encoded CString valid for this call. buffer is a fixed-size
        // stack array whose pointer and length are valid.
        unsafe {
            if ctTagRead(
                self.handle(),
                tag.as_ptr(),
                buffer.as_mut_ptr(),
                buffer.len() as DWORD,
            ) == 0
            {
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
    /// // Copy fields from packed struct before use to avoid misaligned reference
    /// let ts = { value_items.timestamp };
    /// let quality = { value_items.quality_general };
    /// println!("Timestamp: {}", ts);
    /// println!("Quality: {}", quality);
    /// # Ok::<(), ctapi_rs::CtApiError>(())
    /// ```
    pub fn tag_read_ex<T: AsRef<str>>(
        &self,
        tag: T,
        tagvalue_items: &mut CtTagValueItems,
    ) -> Result<String> {
        let mut buffer = [0i8; 256];
        let tag =
            encode_to_gbk_cstring(tag.as_ref()).map_err(|_| CtApiError::InvalidParameter {
                param: "tag".to_string(),
                value: tag.as_ref().to_string(),
            })?;

        // SAFETY: self.handle() is a valid CtAPI connection handle. tag is a
        // GBK-encoded CString valid for this call. buffer is a fixed-size stack
        // array. The native metadata buffer has the exact initialized C layout.
        unsafe {
            let mut native_items = ctapi_sys::CtTagValueItems::default();
            if ctTagReadEx(
                self.handle(),
                tag.as_ptr(),
                buffer.as_mut_ptr(),
                buffer.len() as DWORD,
                &mut native_items,
            ) == 0
            {
                return Err(std::io::Error::last_os_error().into());
            }
            *tagvalue_items = native_items.into();

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
    /// // Write a float value
    /// client.tag_write("Temperature", 25.5_f64)?;
    ///
    /// // Write an integer value
    /// client.tag_write("Counter", 42_i32)?;
    ///
    /// // For string/bool values, use tag_write_str instead:
    /// client.tag_write_str("Status", "Running")?;
    /// client.tag_write_str("Pump_Start", "1")?;
    /// # Ok::<(), ctapi_rs::CtApiError>(())
    /// ```
    pub fn tag_write<T, U>(&self, tag: T, value: U) -> Result<()>
    where
        T: AsRef<str>,
        U: Display,
    {
        let tag =
            encode_to_gbk_cstring(tag.as_ref()).map_err(|_| CtApiError::InvalidParameter {
                param: "tag".to_string(),
                value: tag.as_ref().to_string(),
            })?;
        let s_value = CString::new(value.to_string())?;

        self.tag_write_encoded(&tag, &s_value)
    }

    /// Write tag value as a plain string
    ///
    /// Unlike [`tag_write`], this method accepts any string value without
    /// requiring the value type to implement `Add + Sub + Copy`.  It is
    /// particularly useful for writing enum-like string tags or when the
    /// value is already a `String` / `&str`.
    ///
    /// # Parameters
    /// * `tag`   - Tag name
    /// * `value` - Value string to write (GBK-encoded internally)
    ///
    /// # Return Value
    /// Returns `true` if the write succeeded.
    ///
    /// # Errors
    /// * [`CtApiError::TagNotFound`] - Tag does not exist or is not writable
    /// * [`CtApiError::System`]      - System call failed
    ///
    /// # Examples
    /// ```no_run
    /// use ctapi_rs::CtClient;
    ///
    /// let client = CtClient::open(None, None, None, 0)?;
    /// client.tag_write_str("Status", "Running")?;
    /// client.tag_write_str("Setpoint", "25.5")?;
    /// # Ok::<(), ctapi_rs::CtApiError>(())
    /// ```
    pub fn tag_write_str<T: AsRef<str>>(&self, tag: T, value: &str) -> Result<()> {
        let tag =
            encode_to_gbk_cstring(tag.as_ref()).map_err(|_| CtApiError::InvalidParameter {
                param: "tag".to_string(),
                value: tag.as_ref().to_string(),
            })?;
        let s_value = encode_to_gbk_cstring(value).map_err(|_| CtApiError::InvalidParameter {
            param: "value".to_string(),
            value: value.to_string(),
        })?;

        self.tag_write_encoded(&tag, &s_value)
    }

    pub(crate) fn tag_write_encoded(&self, tag: &CStr, value: &CStr) -> Result<()> {
        // SAFETY: The connection and both strings outlive the synchronous write.
        if unsafe { ctTagWrite(self.handle(), tag.as_ptr(), value.as_ptr()) } == 0 {
            return Err(Error::last_os_error().into());
        }
        Ok(())
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
        let cmd = encode_to_gbk_cstring(cmd).map_err(|_| CtApiError::InvalidParameter {
            param: "cmd".to_string(),
            value: cmd.to_string(),
        })?;
        let mut buffer = [0i8; 256];
        self.cicode_into(&cmd, vh_win, mode, &mut buffer)?;
        decode_response_buffer(&buffer)
    }

    pub(crate) fn cicode_into(
        &self,
        cmd: &CStr,
        vh_win: u32,
        mode: u32,
        buffer: &mut [i8],
    ) -> Result<()> {
        let capacity = DWORD::try_from(buffer.len()).map_err(|_| {
            Error::new(
                std::io::ErrorKind::InvalidInput,
                "Cicode buffer is too large",
            )
        })?;
        // SAFETY: This is a synchronous call. The input string, result buffer
        // and connection stay alive until CtAPI returns.
        if unsafe {
            ctCicode(
                self.handle(),
                cmd.as_ptr(),
                vh_win,
                mode,
                buffer.as_mut_ptr(),
                capacity,
                std::ptr::null_mut(),
            )
        } == 0
        {
            return Err(Error::last_os_error().into());
        }
        Ok(())
    }

    /// Find objects matching the supplied criteria.
    ///
    /// Use try_find_first and CtFind::try_next when errors must be
    /// distinguished from the end of the search.
    pub fn find_first(
        &self,
        table_name: &str,
        filter: &str,
        cluster: Option<&str>,
    ) -> super::CtFind<'_> {
        match self.try_find_first(table_name, Some(filter), cluster) {
            Ok(search) => search,
            Err(error) => super::CtFind::failed(self, error),
        }
    }

    /// Prepare a checked search. A missing filter is passed as a null pointer.
    pub fn try_find_first(
        &self,
        table_name: &str,
        filter: Option<&str>,
        cluster: Option<&str>,
    ) -> Result<super::CtFind<'_>> {
        let table_name = encode_strict(table_name, "table name")?;
        let filter = filter
            .map(|field| encode_strict(field, "filter"))
            .transpose()?;
        let cluster = cluster
            .map(|field| encode_strict(field, "cluster"))
            .transpose()?;
        Ok(super::CtFind::new(self, table_name, filter, cluster))
    }

    /// Create new list
    ///
    /// The list holds a reference to its owning connection.
    ///
    /// # Examples
    /// ```no_run
    /// use ctapi_rs::CtClient;
    /// use std::sync::Arc;
    ///
    /// let client = Arc::new(CtClient::open(None, None, None, 0)?);
    /// let list = Arc::clone(&client).list_new(0)?;
    /// # Ok::<(), anyhow::Error>(())
    /// ```
    pub fn list_new(self: Arc<Self>, mode: u32) -> Result<super::CtList> {
        // SAFETY: self.handle() is a valid CtAPI connection handle. mode is a
        // valid DWORD flag value. The returned handle is wrapped in CtList
        // which manages its lifetime.
        unsafe {
            let handle = ctListNew(self.handle(), mode);
            if handle.is_null() {
                return Err(std::io::Error::last_os_error().into());
            }
            Ok(super::CtList::new(self, handle))
        }
    }
}

impl Drop for ClientHandle {
    fn drop(&mut self) {
        // SAFETY: The last owner is responsible for closing this handle once.
        if !self.0.is_null() && unsafe { ctClose(self.0) } == 0 {
            let os_error = Error::last_os_error();
            eprintln!("Warning: ctClose failed in CtClient::drop: {os_error}");
        }
    }
}

/// Initialize resources for new CtAPI client instance
pub fn ct_client_create() -> Result<CtClient> {
    // SAFETY: ctClientCreate takes no parameters and returns a new CtAPI handle
    // or null on failure. The handle is returned inside a CtClient which will
    // call ctClose on drop.
    let handle = unsafe { ctClientCreate() };

    if handle.is_null() {
        return Err(Error::last_os_error().into());
    }
    Ok(CtClient::from_handle(handle))
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
    // SAFETY: the caller guarantees h_ctapi is a valid HANDLE not concurrently
    // in use. ctClientDestroy is a simple FFI call with no other preconditions.
    if unsafe { ctClientDestroy(h_ctapi) } == 0 {
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
        let client = CtClient::from_handle(handle);

        // Test struct basic functionality
        assert_eq!(client.handle(), std::ptr::null_mut());
    }

    #[test]
    fn test_handle_getter() {
        let handle = std::ptr::null_mut();
        let client = CtClient::from_handle(handle);

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

        let client1 = CtClient::from_handle(handle1);
        let client2 = CtClient::from_handle(handle2);
        let client3 = CtClient::from_handle(handle3);

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
    fn clones_share_one_native_owner() {
        let original = CtClient::from_handle(std::ptr::null_mut());
        let cloned = original.clone();
        assert!(original.same_connection(&cloned));
        assert_eq!(Arc::strong_count(&original.inner), 2);
        drop(original);
        assert_eq!(Arc::strong_count(&cloned.inner), 1);
    }

    #[test]
    fn native_quality_flags_are_normalized() {
        let raw = ctapi_sys::CtTagValueItems {
            boverride: 255,
            control_mode: 0,
            ..Default::default()
        };
        let metadata = CtTagValueItems::from(raw);
        assert!(metadata.boverride);
        assert!(!metadata.control_mode);
        assert_eq!(
            metadata.length() as usize,
            std::mem::size_of::<ctapi_sys::CtTagValueItems>()
        );
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
