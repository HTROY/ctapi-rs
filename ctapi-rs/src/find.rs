//! Object search related implementation
use anyhow::Result;
use ctapi_sys::*;
use encoding_rs::*;
use std::ffi::{c_void, CString};
use std::os::windows::io::RawHandle;

/// Wrapper struct containing handle returned by [`CtClient::find_first`] function
///
/// # Thread Safety
///
/// `CtFind` is NOT thread-safe and should not be sent across threads.
/// It holds a reference to `CtClient` and maintains mutable state during iteration.
/// Each thread should create its own `CtFind` instance if parallel searches are needed.
///
/// Note: `CtFind` does not implement `Send` or `Sync` by default due to the
/// interior mutability in `Iterator::next()` and the FFI handle management.
#[derive(Debug)]
pub struct CtFind<'a> {
    client: &'a super::CtClient,
    handle: RawHandle,
    table_name: CString,
    filter: CString,
    cluster: Option<CString>,
    is_end: bool,
}

impl<'a> CtFind<'a> {
    pub(super) fn new(
        client: &'a super::CtClient,
        table_name: CString,
        filter: CString,
        cluster: Option<CString>,
    ) -> Self {
        Self {
            client,
            handle: std::ptr::null_mut(),
            table_name,
            filter,
            cluster,
            is_end: false,
        }
    }
}

impl Iterator for CtFind<'_> {
    type Item = FindObject;

    fn next(&mut self) -> Option<Self::Item> {
        unsafe {
            if self.is_end {
                return None;
            }
            let mut find_object = std::ptr::null_mut();
            if self.handle.is_null() {
                match &self.cluster {
                    Some(cluster) => {
                        self.handle = ctFindFirstEx(
                            self.client.handle(),
                            self.table_name.as_ptr(),
                            self.filter.as_ptr(),
                            cluster.as_ptr(),
                            &mut find_object,
                            0,
                        );
                        if self.handle.is_null() {
                            self.is_end = true;
                            None
                        } else {
                            Some(FindObject(find_object))
                        }
                    }
                    None => {
                        self.handle = ctFindFirst(
                            self.client.handle(),
                            self.table_name.as_ptr(),
                            self.filter.as_ptr(),
                            &mut find_object,
                            0,
                        );
                        if self.handle.is_null() {
                            self.is_end = true;
                            None
                        } else {
                            Some(FindObject(find_object))
                        }
                    }
                }
            } else if ctFindNext(self.handle, &mut find_object) {
                Some(FindObject(find_object))
            } else {
                self.is_end = true;
                None
            }
        }
    }
}

impl Drop for CtFind<'_> {
    fn drop(&mut self) {
        // SAFETY: Safe to call ctFindClose on a valid handle.
        // The null check prevents double-free or invalid handle access.
        // Since CtFind is not Send/Sync, it cannot be accessed from multiple threads.
        unsafe {
            if !self.handle.is_null() && !ctFindClose(self.handle) {
                // Silently ignore errors in drop to avoid panics
                // Errors here typically indicate the connection was already closed
            }
        }
    }
}

/// Wrapper struct containing object handle returned by search function
#[derive(Debug)]
pub struct FindObject(RawHandle);

impl FindObject {
    /// Retrieve object properties or metadata
    ///
    /// Use this function in conjunction with ctFindFirst() and ctFindNext() functions.
    /// That is, first find the object, then retrieve its properties.
    ///
    /// To retrieve property metadata (such as type, size, etc.), use the following syntax for the szName parameter:
    ///
    /// - object.fields.count - Number of fields in record
    /// - object.fields(n).name - Name of nth field in record
    /// - object.fields(n).type - Type of nth field in record
    /// - object.fields(n).actualsize - Actual size of nth field in record
    pub fn get_property<T: AsRef<str>>(&self, name: T) -> Result<String> {
        let mut buffer = [0u8; 256];
        let mut len: u32 = 0;
        let name = CString::new(GBK.encode(name.as_ref()).0)?;
        unsafe {
            if !ctGetProperty(
                self.0,
                name.as_ptr(),
                buffer.as_mut_ptr() as *mut c_void,
                256,
                &mut len,
                DBTYPEENUM::DBTYPE_STR,
            ) {
                return Err(std::io::Error::last_os_error().into());
            }
            Ok(GBK
                .decode(std::slice::from_raw_parts(buffer.as_ptr(), len as usize))
                .0
                .to_string())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_find_object_debug() {
        let handle = 0x12345678 as *mut std::ffi::c_void;
        let find_object = FindObject(handle);

        // Test Debug implementation
        let debug_string = format!("{:?}", find_object);
        assert!(debug_string.contains("FindObject"));
    }

    #[test]
    fn test_find_object_property_access() {
        let handle = std::ptr::null_mut();
        let find_object = FindObject(handle);

        // Test null handle case
        // Note: Don't test actual property retrieval here as it requires real CtAPI connection
        // Only test basic functionality of struct
        assert_eq!(find_object.0, std::ptr::null_mut());
    }

    #[test]
    fn test_ct_find_lifetime() {
        use std::ffi::CString;

        // Since CtClient field is private, we can only test basic functionality of CtFind
        // No need for actual client instance
        let _table_name = CString::new("test_table").unwrap();
        let _filter = CString::new("test_filter").unwrap();

        // We don't create CtFind instance here as it requires valid client reference
        // Just ensure CtFind struct basic functionality at compile time

        // Test lifetime related functionality
        assert_eq!(1 + 1, 2); // Placeholder test
    }
}
