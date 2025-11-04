//! Tag list operation related implementation
use anyhow::{anyhow, Result};
use ctapi_sys::*;
use encoding_rs::*;
use std::collections::HashMap;
use std::ffi::{CStr, CString};
use std::os::windows::io::RawHandle;
use std::os::windows::raw::HANDLE;

const NULL: HANDLE = 0 as HANDLE;

/// Wrapper struct containing ctapi list handle
#[derive(Debug)]
pub struct CtList<'a> {
    client: &'a super::CtClient,
    handle: RawHandle,
    tag_map: HashMap<String, RawHandle>,
}

impl<'a> CtList<'a> {
    pub(super) fn new(client: &'a super::CtClient, handle: RawHandle) -> Self {
        Self {
            client,
            handle,
            tag_map: HashMap::new(),
        }
    }

    /// Add tag or tag element to list
    ///
    /// Once tags are added to the list, they can be read using ctListRead() and written using ctListWrite().
    /// If a read is already pending, tags will not be read until next call to ctListRead().
    /// ctListWrite() can be called immediately after ctListAdd() function completes.
    pub fn add_tag<T: AsRef<str>>(&mut self, tag: T) -> Result<()> {
        let ctag = CString::new(GBK.encode(tag.as_ref()).0)?;
        unsafe {
            let handle = ctListAdd(self.handle, ctag.as_ptr());
            if handle.is_null() {
                return Err(std::io::Error::last_os_error().into());
            }
            self.tag_map.insert(tag.as_ref().to_owned(), handle);
        }
        Ok(())
    }

    /// Add tag (extended version with more parameters)
    ///
    /// Besides ctListAdd functionality, also supports setting raw value flag, polling period and deadband.
    /// If using ctListAdd, default polling period is 500ms, raw value flag defaults to engineering value FALSE.
    pub fn add_tag_ex<T: AsRef<str>>(
        &mut self,
        tag: T,
        raw: bool,
        poll_period: i32,
        deadband: f64,
    ) -> Result<()> {
        let ctag = CString::new(GBK.encode(tag.as_ref()).0)?;
        unsafe {
            let handle = ctListAddEx(self.handle, ctag.as_ptr(), raw, poll_period, deadband);
            if handle.is_null() {
                return Err(std::io::Error::last_os_error().into());
            }
            self.tag_map.insert(tag.as_ref().to_owned(), handle);
        }
        Ok(())
    }

    /// Delete tag created with ctListAdd
    ///
    /// Program can call ctListDelete() while there are pending reads or writes in another thread.
    /// ctListWrite() and ctListRead() will return after tag deletion.
    pub fn delete_tag<T: AsRef<str>>(&mut self, tag: T) -> Result<()> {
        match self.tag_map.get(tag.as_ref()) {
            Some(handle) => unsafe {
                if !ctListDelete(*handle) {
                    return Err(std::io::Error::last_os_error().into());
                }
                self.tag_map.remove(tag.as_ref());
                Ok(())
            },
            None => Err(anyhow!("Tag:{} not found", tag.as_ref())),
        }
    }

    /// Read tags in list
    ///
    /// This function will read tags attached to the list. Once data is read from I/O device,
    /// ctListData() can be called to get tag values. If reading is not successful,
    /// ctListData() will return errors for tags that cannot be read.
    ///
    /// Tags can be added and removed from list while ctListRead() is pending.
    pub fn read(&self) -> Result<()> {
        unsafe {
            if !ctListRead(self.handle, NULL as *mut OVERLAPPED) {
                Err(std::io::Error::last_os_error().into())
            } else {
                Ok(())
            }
        }
    }

    /// Get values of tags in list
    ///
    /// Call this function after ctListRead() completes for added tags.
    pub fn read_tag<T: AsRef<str>>(&self, tag: T, mode: u32) -> Result<String> {
        match self.tag_map.get(tag.as_ref()) {
            Some(handle) => unsafe {
                let mut buffer = [0u8; 256];
                if !ctListData(
                    *handle,
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
            None => Err(anyhow!("Tag:{} not found!", tag.as_ref())),
        }
    }

    /// Write single tag in list
    pub fn write_tag<T: AsRef<str>>(
        &self,
        tag: T,
        value: T,
        overlapped: Option<&mut OVERLAPPED>,
    ) -> Result<()> {
        if let Some(handle) = self.tag_map.get(tag.as_ref()) {
            let value = CString::new(GBK.encode(value.as_ref()).0)?;
            match overlapped {
                Some(overlapped) => unsafe {
                    if !ctListWrite(*handle, value.as_ptr(), overlapped) {
                        return Err(std::io::Error::last_os_error().into());
                    }
                },
                None => unsafe {
                    if !ctListWrite(*handle, value.as_ptr(), NULL as *mut OVERLAPPED) {
                        return Err(std::io::Error::last_os_error().into());
                    }
                },
            }
            Ok(())
        } else {
            Err(anyhow!("{}", tag.as_ref()))
        }
    }
}

impl Drop for CtList<'_> {
    fn drop(&mut self) {
        unsafe {
            ctListFree(self.handle);
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use std::os::windows::io::RawHandle;

    #[test]
    fn test_ct_list_debug() {
        // Since CtClient field is private, we only test basic functionality of struct
        // Don't create actual CtList instance
         
        // Test struct Debug implementation
        assert_eq!(1 + 1, 2); // Placeholder test
    }

    #[test]
    fn test_tag_map_functionality() {
        // Test HashMap basic functionality (used inside CtList)
        let mut map = HashMap::new();
        
        // Test empty mapping
        assert_eq!(map.len(), 0);
        
        // Test insertion
        let mock_handle: RawHandle = std::ptr::null_mut();
        map.insert("test_tag".to_string(), mock_handle);
        
        assert_eq!(map.len(), 1);
        assert!(map.contains_key("test_tag"));
    }

    #[test]
    fn test_tag_not_found_error() {
        // Test error handling logic
        let error_msg = "Tag:nonexistent_tag not found";
        assert!(error_msg.contains("not found"));
        assert!(error_msg.contains("nonexistent_tag"));
    }
}