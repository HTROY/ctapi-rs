//! Object search related implementation
use crate::CtClient;
use crate::error::{CtApiError, Result};
use crate::util::encode_to_gbk_cstring;
use ctapi_sys::*;
use encoding_rs::*;
use std::ffi::{CString, c_void};
use std::io::{Error, ErrorKind};
use std::marker::PhantomData;
use std::os::windows::io::RawHandle;
use std::rc::{Rc, Weak};
use windows_sys::Win32::Foundation::{ERROR_SUCCESS, GetLastError, SetLastError};

// Both bundled CtApi.dll versions report this status when a search is exhausted.
const CT_ERROR_NOT_FOUND: u32 = crate::ERROR_USER_DEFINED_BASE + 347;

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
    client: CtClient,
    _borrow: PhantomData<&'a CtClient>,
    handle: RawHandle,
    table_name: CString,
    filter: Option<CString>,
    cluster: Option<CString>,
    current: Option<Rc<()>>,
    argument_error: Option<CtApiError>,
    is_end: bool,
}

impl<'a> CtFind<'a> {
    pub(super) fn new(
        client: &'a super::CtClient,
        table_name: CString,
        filter: Option<CString>,
        cluster: Option<CString>,
    ) -> Self {
        Self {
            client: client.clone(),
            _borrow: PhantomData,
            handle: std::ptr::null_mut(),
            table_name,
            filter,
            cluster,
            current: None,
            argument_error: None,
            is_end: false,
        }
    }

    pub(super) fn failed(client: &'a super::CtClient, error: CtApiError) -> Self {
        let mut search = Self::new(client, CString::default(), None, None);
        search.argument_error = Some(error);
        search
    }

    /// Advance the search, propagating errors reported by CtAPI.
    ///
    /// A returned record is valid until the next advance or until the search
    /// is dropped. CtAPI may use the last-error channel to report failure.
    pub fn try_next(&mut self) -> Result<Option<FindObject>> {
        self.current = None;
        if let Some(error) = self.argument_error.take() {
            self.is_end = true;
            return Err(error);
        }
        if self.is_end {
            return Ok(None);
        }

        let mut object = std::ptr::null_mut();
        let filter = self
            .filter
            .as_ref()
            .map_or(std::ptr::null(), |s| s.as_ptr());
        // SAFETY: The client and query inputs outlive each synchronous call.
        // Clear stale thread-local error state before invoking the vendor API.
        let (found, error) = unsafe {
            SetLastError(ERROR_SUCCESS);
            let found = if self.handle.is_null() {
                self.handle = match &self.cluster {
                    Some(cluster) => ctFindFirstEx(
                        self.client.handle(),
                        self.table_name.as_ptr(),
                        filter,
                        cluster.as_ptr(),
                        &mut object,
                        0,
                    ),
                    None => ctFindFirst(
                        self.client.handle(),
                        self.table_name.as_ptr(),
                        filter,
                        &mut object,
                        0,
                    ),
                };
                !self.handle.is_null()
            } else {
                ctFindNext(self.handle, &mut object) != 0
            };
            (found, GetLastError())
        };

        if !found {
            self.is_end = true;
            return check_search_end(error).map(|()| None);
        }
        if object.is_null() {
            self.is_end = true;
            return Err(Error::new(
                ErrorKind::InvalidData,
                "CtAPI returned a null record handle",
            )
            .into());
        }

        let current = Rc::new(());
        let record = FindObject {
            handle: object,
            current: Rc::downgrade(&current),
        };
        self.current = Some(current);
        Ok(Some(record))
    }

    /// Ask CtAPI for the number of records in the current open search.
    pub fn record_count(&self) -> Result<usize> {
        if self.handle.is_null() || self.is_end {
            return Err(
                Error::new(ErrorKind::InvalidInput, "CtAPI search has no active cursor").into(),
            );
        }
        // SAFETY: This cursor holds its connection and remains open.
        let count = unsafe { ctFindNumRecords(self.handle) };
        usize::try_from(count).map_err(|_| Error::last_os_error().into())
    }
}

fn check_search_end(status: u32) -> Result<()> {
    match status {
        CT_ERROR_NOT_FOUND => Ok(()),
        ERROR_SUCCESS => {
            Err(Error::other("CtAPI stopped the search without reporting an end status").into())
        }
        code => Err(Error::from_raw_os_error(code as i32).into()),
    }
}

impl Iterator for CtFind<'_> {
    type Item = FindObject;

    fn next(&mut self) -> Option<Self::Item> {
        self.try_next().ok().flatten()
    }
}

impl Drop for CtFind<'_> {
    fn drop(&mut self) {
        self.current = None;
        if !self.handle.is_null() {
            // SAFETY: The search owns this valid handle and closes it once.
            unsafe { ctFindClose(self.handle) };
        }
    }
}

/// A search record valid until its cursor advances or closes.
#[derive(Debug)]
pub struct FindObject {
    handle: RawHandle,
    current: Weak<()>,
}

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
        let name = encode_to_gbk_cstring(name.as_ref())?;
        if self.current.strong_count() == 0 || self.handle.is_null() {
            return Err(Error::new(
                ErrorKind::InvalidInput,
                "CtAPI search record is no longer current",
            )
            .into());
        }
        // SAFETY: The current record has a live cursor token. Neither the
        // search nor this record can be used from another thread.
        // name is a GBK-encoded CString. buffer is a fixed-size stack array.
        // len is a local stack variable.
        unsafe {
            if ctGetProperty(
                self.handle,
                name.as_ptr(),
                buffer.as_mut_ptr() as *mut c_void,
                256,
                &mut len,
                DBTYPEENUM::DBTYPE_STR as DWORD,
            ) == 0
            {
                return Err(std::io::Error::last_os_error().into());
            }
            let bytes = &buffer[..(len as usize).min(buffer.len())];
            let field = bytes.split(|&byte| byte == 0).next().unwrap_or_default();
            Ok(GBK.decode(field).0.into_owned())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn search_completion_requires_the_citect_end_status() {
        assert!(check_search_end(CT_ERROR_NOT_FOUND).is_ok());
        assert!(check_search_end(ERROR_SUCCESS).is_err());
        let error =
            check_search_end(windows_sys::Win32::Foundation::ERROR_ACCESS_DENIED).unwrap_err();
        assert!(
            matches!(error, CtApiError::System(error) if error.kind() == ErrorKind::PermissionDenied)
        );
    }

    #[test]
    fn record_properties_require_a_live_cursor() {
        let current = Rc::new(());
        let record = FindObject {
            handle: std::ptr::dangling_mut::<c_void>(),
            current: Rc::downgrade(&current),
        };
        drop(current);

        let error = record.get_property("TAG").unwrap_err();
        assert!(matches!(error, CtApiError::System(e) if e.kind() == ErrorKind::InvalidInput));
    }
}
