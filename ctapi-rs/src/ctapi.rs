//! Safety binding for CtApi.dll
use anyhow::{anyhow, Result};
use ctapi_sys::*;
use encoding_rs::*;
use libc::strnlen;
use std::collections::HashMap;
use std::ffi::{c_void, CStr, CString};
use std::fmt::Display;
use std::io::Error;
use std::ops::{Add, Sub};
use std::os::windows::io::RawHandle;
use std::os::windows::raw::HANDLE;

//re-export ctapi_sys
pub use ctapi_sys::CtHScale;
pub use ctapi_sys::CtScale;
pub use ctapi_sys::CtTagValueItems;

const NULL: HANDLE = 0 as HANDLE;

/// A wrap struct contanit a handle returned by function [`CtClient::find_first`]
#[derive(Debug)]
pub struct CtFind<'a> {
    client: &'a CtClient,
    handle: RawHandle,
    table_name: CString,
    filter: CString,
    cluster: Option<CString>,
    is_end: bool,
}

impl<'a> CtFind<'a> {
    fn new(
        client: &'a CtClient,
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
                            self.client.handle,
                            self.table_name.as_ptr(),
                            self.filter.as_ptr(),
                            cluster.as_ptr(),
                            &mut find_object,
                            0,
                        );
                        if self.handle.is_null() {
                            self.is_end = true;
                            return None;
                        } else {
                            return Some(FindObject(find_object));
                        }
                    }
                    None => {
                        self.handle = ctFindFirst(
                            self.client.handle,
                            self.table_name.as_ptr(),
                            self.filter.as_ptr(),
                            &mut find_object,
                            0,
                        );
                        if self.handle.is_null() {
                            self.is_end = true;
                            return None;
                        } else {
                            return Some(FindObject(find_object));
                        }
                    }
                }
            }
            if ctFindNext(self.handle, &mut find_object) {
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
        unsafe {
            ctFindClose(self.handle);
        }
    }
}
/// A wrap struct contanit a handle returned by function [`CtClient::find_first`] or
/// [`CtClient::find_first_ex`]
#[derive(Debug)]
pub struct FindObject(RawHandle);

impl FindObject {
    /// Retrieves an object property or meta data for an object.
    ///
    /// Use this function in conjunction with the ctFindFirst() and ctFindNext() functions. i.e. First,
    /// you find an object, then you retrieve its properties.
    ///
    /// To retrieve property meta data such as type, size and so on, use the following syntax for the
    /// szName argument:
    ///
    /// - object.fields.count - the number of fields in the record
    /// - object.fields(n).name - the name of the nth field of the record
    /// - object.fields(n).type - the type of the nth field of the record
    /// - object.fields(n).actualsize - the actual size of the nth field of the record
    ///
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

// impl<'a> IntoIterator for &'a CtFind {
//     type Item = FindObject;

//     type IntoIter = CtFindIter;

//     fn into_iter(self) -> Self::IntoIter {
//         self.iter()
//     }
// }

/// A wrap struct contanit a handle of the ctlist
#[derive(Debug)]
pub struct CtList<'a> {
    client: &'a CtClient,
    handle: RawHandle,
    tag_map: HashMap<String, RawHandle>,
}

impl<'a> CtList<'a> {
    fn new(client: &'a CtClient, handle: RawHandle) -> Self {
        Self {
            client,
            handle,
            tag_map: HashMap::new(),
        }
    }

    /// Adds a tag or tag element to the list. Once the tag has been added to the list,
    /// it may be read using ctListRead() and written to using ctListWrite(). If a read
    /// is already pending, the tag will not be read until the next time ctListRead()
    /// is called. ctListWrite() may be called immediately after the ctListAdd()
    /// function has completed.
    pub fn add<T: AsRef<str>>(&mut self, tag: T) -> Result<()> {
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

    /// Performs the same as ctListAdd, but with 2 additional new arguments. Adds a tag, or tag
    /// element, to the list. Once the tag has been added to the list, it may be read using
    /// ctListRead() and written to using ctListWrite(). If a read is already pending, the tag
    /// will not be read until the next time ctListRead() is called. ctListWrite() may be
    /// called immediately after the ctListAdd() function has completed.
    ///
    /// If ctListAdd is called instead of ctListAddEx, The poll period of the subscription for
    /// the tag defaults to 500 milliseconds, and the bRaw flag defaults to the engineering
    /// value of FALSE.
    pub fn add_ex<T: AsRef<str>>(
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

    /// Frees a tag created with ctListAdd. Your program is permitted to call ctListDelete() while
    /// a read or write is pending on another thread. The ctListWrite() and ctListRead() will
    /// return once the tag has been deleted.
    pub fn delete<T: AsRef<str>>(&mut self, tag: T) -> Result<()> {
        match self.tag_map.get(tag.as_ref()) {
            Some(handle) => {
                unsafe {
                    if !ctListDelete(*handle) {
                        return Err(std::io::Error::last_os_error().into());
                    }
                }
                self.tag_map.remove(tag.as_ref());
                Ok(())
            }
            None => Err(anyhow!("tag:{} not found", tag.as_ref())),
        }
    }

    /// Reads the tags on the list. This function will read tags which are attached to the list.
    /// Once the data has been read from the I/O Devices, you may call ctListData()to get the
    /// values of the tags. If the read does not succeed, ctListData() will return an error
    /// for the tags that cannot be read.
    ///
    /// While ctListRead() is pending you are allowed to add and delete tags from the list. If you
    /// delete a tag from the list while ctListRead() is pending, it may still be read one more
    /// time. The next time ctListRead() is called, the tag will not be read. If you add a tag to
    /// the list while ctListRead() is pending, the tag will not be read until the next time
    /// ctListRead() is called. You may call ctListData() for this tag as soon as you have added
    /// it. In this case ctListData() will not succeed, and GetLastError() will return GENERIC_INVALID_DATA.
    ///
    /// You can only have 1 pending read command on each list. If you call ctListRead() again for
    /// the same list, the function will not succeed.
    ///
    /// Before freeing the list, check that there are no reads still pending. wait for the any
    /// current ctListRead() to return and then delete the list.
    pub fn read(&self) -> Result<()> {
        unsafe {
            if !ctListRead(self.handle, NULL as *mut OVERLAPPED) {
                Err(std::io::Error::last_os_error().into())
            } else {
                Ok(())
            }
        }
    }

    /// Gets the value of a tag on the list. Call this function after ctListRead() has completed
    /// for the added tag. You may call ctListData() while subsequent ctListRead() functions are
    /// pending, and the last data read will be returned. If you wish to get the value of a
    /// specific quality part of a tag element item data use ctListItem which includes the same
    /// parameters with the addition of the dwItem parameter.
    pub fn data<T: AsRef<str>>(&self, tag: T, mode: u32) -> Result<String> {
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

    /// Writes to a single tag on the list.
    pub fn write<T: AsRef<str>>(
        &self,
        tag: T,
        value: T,
        pct_overlapped: Option<&mut OVERLAPPED>,
    ) -> Result<()> {
        if let Some(handle) = self.tag_map.get(tag.as_ref()) {
            let value = CString::new(GBK.encode(value.as_ref()).0)?;
            match pct_overlapped {
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
/// A wrap struct contanit a handle of the ctapi
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct CtClient {
    handle: RawHandle,
}

unsafe impl Send for CtClient {}
unsafe impl Sync for CtClient {}

impl CtClient {
    /// Opens a connection to the Citect SCADA API.
    ///
    /// The CTAPI.DLL is initialized and a connection is made to Citect SCADA. If Citect SCADA is not
    /// running when this function is called, the function will exit and report an error. This function
    /// needs to be called before any other CTAPI function to initialize the connection to Citect SCADA.
    ///
    /// If you use the CT_OPEN_RECONNECT mode, and the connection is lost, the CTAPI will attempt to
    /// reconnect to Citect SCADA. When the connection has been re-established, you can continue to
    /// use the CTAPI. However, while the connection is down, every function will return errors. If
    /// a connection cannot be created the first time ctOpen() is called, a valid handle is still
    /// returned; however GetLastError() will indicate an error.
    ///
    /// If you do not use the CT_OPEN_RECONNECT mode, and the connection to Citect SCADA is lost, you
    /// need to free handles returned from the CTAPI and call ctClose() to free the connection. You
    /// need to then call ctOpen() to re-establish the connection and re-create any handles.
    ///
    /// Note: To use the CTAPI on a remote computer without installing Citect SCADA, you will need
    /// to copy the following files from the \[bin\] directory to your remote computer: CTAPI.dll,
    /// CT_IPC.dll, CTENG32.dll, CTRES32.dll, CTUTIL32.dll, CIDEBUGHELP.dll, CTUTILMANAGEDHELPER.dll.
    ///
    /// If calling this function from a remote computer, a valid username and a non-blank password
    /// needs to be used.Open a connection to the CitectSCADA API. The CTAPI.DLL is initialized and
    /// a connection is made to CitectSCADA. If CitectSCADA is not running when this
    ///
    /// # Example
    /// ```no_run
    /// # const COMPUTER: &str = "";
    /// # const USER: &str = "";
    /// # const PASSWORD: &str = "";
    /// use ctapi_rs::*;
    /// let handle = CtClient::open(Some(COMPUTER),Some(USER),Some(PASSWORD),0);
    /// assert!(handle.is_ok());
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

    /// Open a connection to the Citect SCADA API.
    ///
    /// # Example
    /// ```no_run
    /// # const COMPUTER: &str = "";
    /// # const USER: &str = "";
    /// # const PASSWORD: &str = "";
    /// use ctapi_rs::*;
    /// let client = ct_create_client().unwrap();
    /// let res = client.connect(Some(COMPUTER),Some(USER),Some(PASSWORD),0);
    /// assert!(res.is_ok());
    /// ```
    pub fn connect(
        &self,
        computer: Option<&str>,
        user: Option<&str>,
        password: Option<&str>,
        mode: u32,
    ) -> Result<()> {
        assert!(self.handle.is_null(), "Client handle is null");
        let computer = computer.and_then(|s| CString::new(s).ok());
        let user = user.and_then(|s| CString::new(s).ok());
        let password = password.and_then(|s| CString::new(s).ok());
        unsafe {
            match ctOpenEx(
                computer.unwrap_or_default().as_ptr(),
                user.unwrap_or_default().as_ptr(),
                password.unwrap_or_default().as_ptr(),
                mode,
                self.handle,
            ) {
                false => Err(std::io::Error::last_os_error().into()),
                true => Ok(()),
            }
        }
    }

    /// Reads the value, quality and timestamp, not only a value. The data will be returned in string
    /// format and scaled using the CitectSCADA scales.
    ///
    /// # Examples
    /// ```no_run
    /// # const COMPUTER: &str = "";
    /// # const USER: &str = "";
    /// # const PASSWORD: &str = "";
    /// use ctapi_rs::*;
    /// let mut client = CtClient::open(Some(COMPUTER),Some(USER),Some(PASSWORD),0).unwrap();
    /// let result = client.tag_read("tag_name");
    /// assert_eq!("0",result.unwrap());
    /// ```
    pub fn tag_read<T: AsRef<str>>(&self, tag: T) -> Result<String> {
        let mut buffer = [0i8; 256];
        let tag = unsafe { CString::from_vec_unchecked(GBK.encode(tag.as_ref()).0.into_owned()) };
        unsafe {
            if !ctTagRead(
                self.handle,
                tag.as_ptr(),
                buffer.as_mut_ptr(),
                buffer.len() as DWORD,
            ) {
                return Err(std::io::Error::last_os_error().into());
            }

            // return decoded string
            Ok(GBK
                .decode(std::slice::from_raw_parts(
                    buffer.as_ptr() as *const u8,
                    strnlen(buffer.as_ptr(), 256),
                ))
                .0
                .to_string())
            // let s = CStr::from_ptr(buffer.as_ptr()).to_str()?;
            // Ok(s)
        }
    }

    /// Performs the same as ctTagRead, but with an additional new argument. Reads the value, quality
    /// and timestamp, not only a value. The data will be returned in string format and scaled using
    /// the Citect SCADA scales.
    /// The function will request the given tag from the Citect SCADA I/O Server. If the tag is in
    /// the I/O Servers device cache the data will be returned from the cache. If the tag is not in
    /// the device cache then the tag will be read from the I/O Device. The time taken to execute this
    /// function will be dependent on the performance of the I/O Device. The calling thread is blocked
    /// until the read is finished.
    ///
    /// # Examples
    /// ```
    /// # const COMPUTER: &str = "";
    /// # const USER: &str = "";
    /// # const PASSWORD: &str = "";
    /// use ctapi_rs::*;
    /// use ctapi_sys::*;
    /// let mut value : CtTagValueItems = Default::default();
    /// let mut client = CtClient::open(Some(COMPUTER),Some(USER),Some(PASSWORD),0).unwrap();
    /// let result = client.tag_read_ex("did",&mut value);
    /// println!("{result:?}");
    /// assert!(result.is_ok());
    /// ```
    pub fn tag_read_ex<T: AsRef<str>>(
        &self,
        tag: T,
        tagvalue_items: &mut CtTagValueItems,
    ) -> Result<String> {
        let mut buffer = [0i8; 256];
        let tag = unsafe { CString::from_vec_unchecked(GBK.encode(tag.as_ref()).0.into_owned()) };
        unsafe {
            if !ctTagReadEx(
                self.handle,
                tag.as_ptr(),
                buffer.as_mut_ptr(),
                256,
                tagvalue_items,
            ) {
                return Err(Error::last_os_error().into());
            }
            Ok(GBK
                .decode(std::slice::from_raw_parts(
                    buffer.as_ptr() as *const u8,
                    strnlen(buffer.as_ptr(), 256),
                ))
                .0
                .to_string())
        }
    }

    /// Writes to the given Citect SCADA I/O Device variable tag.
    /// The value, quality and timestamp, not only a value, is converted into the correct data type,
    /// then scaled and then written to the tag. If writing to an array element only a single element
    /// of the array is written to. This function will generate a write request to the I/O Server. The
    /// time taken to complete this function will be dependent on the performance of the I/O Device.
    /// The calling thread is blocked until the write is completed. Writing operation will succeed only
    /// for those tag elements which have read/write access.
    ///
    /// # Examples
    /// ```no_run
    /// # const COMPUTER: &str = "";
    /// # const USER: &str = "";
    /// # const PASSWORD: &str = "";
    /// use ctapi_rs::*;
    /// let mut client = CtClient::open(Some(COMPUTER),Some(USER),Some(PASSWORD),0).unwrap();
    /// let result = client.tag_write("wid",0x34d);
    /// assert!(result.unwrap());
    /// ```
    pub fn tag_write<T, U>(&self, tag: T, value: U) -> Result<bool>
    where
        T: AsRef<str>,
        U: Display + Add<Output = U> + Sub<Output = U> + Copy + PartialEq,
    {
        let tag = unsafe { CString::from_vec_unchecked(GBK.encode(tag.as_ref()).0.into_owned()) };
        let s_value = CString::new(value.to_string())?;
        unsafe {
            if !ctTagWrite(self.handle, tag.as_ptr(), s_value.as_ptr()) {
                return Err(std::io::Error::last_os_error().into());
            }
            Ok(true)
        }
    }

    /// WriteTagEx
    pub fn tag_write_ex<T, U>(
        &self,
        tag: T,
        value: U,
        overlapped: Option<&mut OVERLAPPED>,
    ) -> Result<bool>
    where
        T: AsRef<str>,
        U: Display + Add<Output = U> + Sub<Output = U> + Copy + PartialEq,
    {
        let tag = unsafe { CString::from_vec_unchecked(GBK.encode(tag.as_ref()).0.into_owned()) };
        let value = value.to_string();
        let value = unsafe { CString::from_vec_unchecked(GBK.encode(&value).0.into_owned()) };

        match overlapped {
            Some(overlapped) => unsafe {
                if !ctTagWriteEx(self.handle, tag.as_ptr(), value.as_ptr(), overlapped) {
                    return Err(std::io::Error::last_os_error().into());
                }
                Ok(true)
            },
            None => unsafe {
                if !ctTagWriteEx(
                    self.handle,
                    tag.as_ptr(),
                    value.as_ptr(),
                    NULL as *mut OVERLAPPED,
                ) {
                    return Err(std::io::Error::last_os_error().into());
                }
                Ok(true)
            },
        }
    }

    /// Cancels a pending overlapped I/O operation.
    ///
    /// When the I/O command is canceled, the event will be signaled to show that the command has
    /// completed. The status will be set to the Citect SCADA error ERROR CANCELED. If the command
    /// completes before you can cancel it, ctCancelIO() will return FALSE, and GetLastError() will
    /// return GENERIC_CANNOT_CANCEL. The status of the overlapped operation will be the completion
    /// status of the command.
    ///
    /// The CTAPI interface will automatically cancel any pending I/O commands when you call ct_close().
    /// # Examples
    ///
    /// ```
    /// # const COMPUTER: &str = "";
    /// # const USER: &str = "";
    /// # const PASSWORD: &str = "";
    /// use ctapi_rs::*;
    /// let mut client = CtClient::open(Some(COMPUTER),Some(USER),Some(PASSWORD),0).unwrap();
    /// let result = client.cancel_io(None);
    /// assert!(result.is_ok());
    ///
    /// ```
    pub fn cancel_io(&mut self, pct_overlapped: Option<&mut OVERLAPPED>) -> Result<bool> {
        match pct_overlapped {
            Some(p_overlapped) => unsafe {
                ctCancelIO(self.handle, p_overlapped);
                Ok(false)
            },
            None => unsafe {
                let reslut = ctCancelIO(self.handle, NULL as *mut OVERLAPPED);
                if reslut {
                    Ok(true)
                } else {
                    Err(Error::last_os_error().into())
                }
            },
        }
    }

    /// Executes a Cicode function on the connected Citect SCADA computer.
    ///
    /// This allows you to control Citect SCADA or to get information returned from Cicode functions.
    /// You may call either built in or user defined Cicode functions. Cancels a pending overlapped
    /// I/O operation.
    ///
    /// The function name and arguments to that function are passed as a single string. Standard
    /// Citect SCADA conversion is applied to convert the data from string type into the type
    /// expected by the function. When passing strings put the strings between the Citect SCADA
    /// string delimiters.
    ///
    /// Functions which expect pointers or arrays are not supported. Functions which expect pointers
    /// are functions which update the arguments. This includes functions DspGetMouse(), DspAnGetPos(),
    /// StrWord(), and so on. Functions which expect arrays to be passed or returned are not supported,
    /// for example TableMath(), TrnSetTable(), TrnGetTable(). You may work around these limitations by
    /// calling a Cicode wrapper function which in turn calls the function you require.
    ///
    /// If the Cicode function you are calling takes a long time to execute, is pre-empt or blocks,
    /// then the result of the function cannot be returned in the sResult argument. The Cicode
    /// function will, however, execute correctly.
    ///
    /// # Example
    /// ```
    /// # const COMPUTER: &str = "";
    /// # const USER: &str = "";
    /// # const PASSWORD: &str = "";
    /// use ctapi_rs::*;
    /// let mut client = CtClient::open(Some(COMPUTER),Some(USER),Some(PASSWORD),0).unwrap();
    /// let result = client.cicode("Time(1)",0,0,None);
    /// assert!(result.is_ok());
    /// ```
    pub fn cicode(&mut self, cmd: &str, vh_win: u32, mode: u32) -> Result<String> {
        let mut buffer = [0i8; 256];
        let cmd = unsafe { CString::from_vec_unchecked(GBK.encode(cmd).0.into_owned()) };

        unsafe {
            if !ctCicode(
                self.handle,
                cmd.as_ptr(),
                vh_win,
                mode,
                buffer.as_mut_ptr(),
                256,
                NULL as *mut OVERLAPPED,
            ) {
                return Err(std::io::Error::last_os_error().into());
            }
            Ok(GBK
                .decode(std::slice::from_raw_parts(
                    buffer.as_ptr() as *const u8,
                    strnlen(buffer.as_ptr(), 256),
                ))
                .0
                .to_string())
        }
    }

    /// Nonblock version of cicode function
    pub fn cicode_nonblock(
        &mut self,
        cmd: &str,
        vh_win: u32,
        mode: u32,
        pct_overlapped: &mut OVERLAPPED,
        buffer: &mut [i8],
    ) -> Result<String> {
        //let mut buffer = [0i8; 256];
        let cmd = unsafe { CString::from_vec_unchecked(GBK.encode(cmd).0.into_owned()) };

        unsafe {
            ctCicode(
                self.handle,
                cmd.as_ptr(),
                vh_win,
                mode,
                buffer.as_mut_ptr(),
                256,
                pct_overlapped,
            );
            Ok("".to_owned())
        }
    }

    /// Searches for the first object in the specified table, device, trend, tag, or alarm data which
    /// satisfies the filter string.
    ///
    /// A handle to the found object is returned via pObjHnd. The object handle is used to retrieve the
    /// object properties. To find the next object, call the ctFindNext function with the returned
    /// search handle.
    ///
    /// If you experience server performance problems when using ctFindFirst() refer to CPULoadCount
    /// and CpuLoadSleepMS.
    ///
    /// # Example
    /// ```
    /// # const COMPUTER: &str = "";
    /// # const USER: &str = "";
    /// # const PASSWORD: &str = "";
    /// use ctapi_rs::*;
    /// let mut client = CtClient::open(Some(COMPUTER),Some(USER),Some(PASSWORD),0).unwrap();
    /// let result = client.find_first("Alarm", "", None);
    /// assert!(result.is_ok());
    /// ```
    pub fn find_first(&self, table_name: &str, filter: &str, cluster: Option<&str>) -> CtFind {
        let table_name =
            unsafe { CString::from_vec_unchecked(GBK.encode(table_name.as_ref()).0.into_owned()) };
        let filter =
            unsafe { CString::from_vec_unchecked(GBK.encode(filter.as_ref()).0.into_owned()) };
        match cluster {
            Some(cluster) => {
                let cluster = unsafe {
                    CString::from_vec_unchecked(GBK.encode(cluster.as_ref()).0.into_owned())
                };
                CtFind::new(self, table_name, filter, Some(cluster))
            }
            None => CtFind::new(self, table_name, filter, None),
        }
    }

    /// Creates a new list. The CTAPI provides two methods to read data from I/O Devices. Each
    /// level varies in its complexity and performance. The simplest way to read data is via the
    /// ctTagRead() function. This function reads the value of a single variable, and the result
    /// is returned as a formatted engineering string.
    ///
    /// The List functions provide a higher level of performance for reading data than the tag
    /// based interface, The List functions also provide support for overlapped operations.
    ///
    /// The list functions allow a group of tags to be defined and then read as a single request.
    /// They provide a simple tag based interface to data which is provided in formatted
    /// engineering data. You can create several lists and control each individually.
    ///
    /// Tags can be added to, or deleted from lists dynamically, even if a read operation is
    /// pending on the list.
    ///
    /// # Example
    /// ```
    /// # const COMPUTER: &str = "";
    /// # const USER: &str = "";
    /// # const PASSWORD: &str = "";
    /// use ctapi_rs::*;
    /// let mut client = CtClient::open(Some(COMPUTER),Some(USER),Some(PASSWORD),0).unwrap();
    /// let result = client.list_new(0);
    /// assert!(result.is_ok());
    /// ```
    pub fn list_new(&mut self, mode: u32) -> Result<CtList> {
        unsafe {
            let handle = ctListNew(self.handle, mode);
            if handle.is_null() {
                return Err(std::io::Error::last_os_error().into());
            }
            Ok(CtList::new(self, handle))
        }
    }

    // pub fn tag_get_property<T>(tag_name: &str, property: &str, tag_type: DBTYPEENUM) -> T {
    //     let (result, length) = match tag_type {
    //         DBTYPEENUM::DBTYPE_EMPTY => todo!(),
    //         DBTYPEENUM::DBTYPE_NULL => todo!(),
    //         DBTYPEENUM::DBTYPE_I2 => (0i16, 2),
    //         DBTYPEENUM::DBTYPE_I4 => todo!(),
    //         DBTYPEENUM::DBTYPE_R4 => todo!(),
    //         DBTYPEENUM::DBTYPE_R8 => todo!(),
    //         DBTYPEENUM::DBTYPE_CY => todo!(),
    //         DBTYPEENUM::DBTYPE_DATE => todo!(),
    //         DBTYPEENUM::DBTYPE_BSTR => todo!(),
    //         DBTYPEENUM::DBTYPE_IDISPATCH => todo!(),
    //         DBTYPEENUM::DBTYPE_ERROR => todo!(),
    //         DBTYPEENUM::DBTYPE_BOOL => todo!(),
    //         DBTYPEENUM::DBTYPE_VARIANT => todo!(),
    //         DBTYPEENUM::DBTYPE_IUNKNOWN => todo!(),
    //         DBTYPEENUM::DBTYPE_DECIMAL => todo!(),
    //         DBTYPEENUM::DBTYPE_UI1 => todo!(),
    //         DBTYPEENUM::DBTYPE_ARRAY => todo!(),
    //         DBTYPEENUM::DBTYPE_BYREF => todo!(),
    //         DBTYPEENUM::DBTYPE_I1 => todo!(),
    //         DBTYPEENUM::DBTYPE_UI2 => todo!(),
    //         DBTYPEENUM::DBTYPE_UI4 => todo!(),
    //         DBTYPEENUM::DBTYPE_I8 => todo!(),
    //         DBTYPEENUM::DBTYPE_UI8 => todo!(),
    //         DBTYPEENUM::DBTYPE_GUID => todo!(),
    //         DBTYPEENUM::DBTYPE_VECTOR => todo!(),
    //         DBTYPEENUM::DBTYPE_RESERVED => todo!(),
    //         DBTYPEENUM::DBTYPE_BYTES => todo!(),
    //         DBTYPEENUM::DBTYPE_STR => todo!(),
    //         DBTYPEENUM::DBTYPE_WSTR => todo!(),
    //         DBTYPEENUM::DBTYPE_NUMERIC => todo!(),
    //         DBTYPEENUM::DBTYPE_UDT => todo!(),
    //         DBTYPEENUM::DBTYPE_DBDATE => todo!(),
    //         DBTYPEENUM::DBTYPE_DBTIME => todo!(),
    //         DBTYPEENUM::DBTYPE_DBTIMESTAMP => todo!(),
    //     };
    //     result
    // }
}

impl Drop for CtClient {
    fn drop(&mut self) {
        unsafe {
            if !ctClose(self.handle) {
                let os_error = Error::last_os_error();
                println!("last OS error: {os_error}");
            }
        }
    }
}

/// ctClientCreate initializes the resources for a new CtAPI client instance. Once you have called
/// ctClientCreate, you can pass the handle returned to ctOpenEx to establish communication with
/// the CtAPI server.
///
/// Consider a situation where you try to communicate to the CtAPI server and the server takes a
/// long time to respond (or doesn't respond at all). If you just call ctOpen, you haven't been
/// given a handle to the CtAPI instance, so you can't cancel the ctOpen by calling ctCancelIO.
/// But if you use ctClientCreate and then call ctOpenEx, you can use the handle returned by
/// ctClientCreate to cancel the ctOpenEx.
pub fn ct_client_create() -> Result<CtClient> {
    let handle = unsafe { ctClientCreate() };

    if handle.is_null() {
        return Err(Error::last_os_error().into());
    }
    Ok(CtClient { handle })
}

/// Cleans up the resources of the given CtAPI instance. Unlike ctClose, ctClientDestroy does not
/// close the connection to the CtAPI server.
///
/// You need to call ctCloseEx with bDestroy equal to FALSE before calling ctClientDestroy.
///
/// # Safety
pub unsafe fn ct_client_destroy(h_ctapi: HANDLE) -> Result<bool> {
    if !ctClientDestroy(h_ctapi) {
        return Err(Error::last_os_error().into());
    }
    Ok(true)
}

// /// Closes the connection between the application and the CtAPI. When called, any pending commands
// /// will be canceled. You need to free any handles allocated before calling ctClose(). These
// /// handles are not freed when ctClose() is called. Call this function from an application on
// /// shutdown or when a major error occurs on the connection.
// ///
// /// # Examples
// /// ```
// /// # const COMPUTER: &str = "50.1.99.30";
// /// # const USER: &str = "";
// /// # const PASSWORD: &str = "";
// /// use ctapi_rs::*;
// /// let handle = ct_open(Some(COMPUTER),Some(USER),Some(PASSWORD),0);
// /// let result = ct_close(handle.unwrap());
// /// assert!(result.is_ok());
// /// ```
// pub fn ct_close(h_ctapi: HANDLE) -> Result<bool> {
//     unsafe {
//         if !ctClose(h_ctapi) {
//             return Err(std::io::Error::last_os_error().into());
//         }
//         Ok(true)
//     }
// }

/// Converts the engineering scale variable into raw I/O Device scale. This is not necessary for
/// the Tag functions as Citect SCADA will do the scaling. Scaling is not necessary for digitals,
/// strings or if no scaling occurs between the values in the I/O Device and the Engineering
/// values. You need to know the scaling for each variables as specified in the Citect SCADA
/// Variable Tags table.
///
/// # Example
/// ```no_run
/// use ctapi_rs::*;
/// use ctapi_sys::*;
/// let scale = Ctscale::new(Cthscale::new(0.0,32000.0), Cthscale::new(0.0,100.0));
/// let result = ct_eng_to_raw(42.23, &scale, CT_SCALE_RANGE_CHECK);
/// println!("{result:?}");
/// assert!(result.is_ok());
/// ```
pub fn ct_eng_to_raw(value: f64, scale: &CtScale, mode: u32) -> Result<f64> {
    let mut result = 0.0;
    unsafe {
        if !ctEngToRaw(&mut result, value, scale, mode) {
            return Err(Error::last_os_error().into());
        }
    }
    Ok(result)
}

/// Converts the raw I/O Device scale variable into Engineering scale. This is not necessary for
/// the Tag functions as Citect SCADA will do the scaling. Scaling is not necessary for digitals,
/// strings or if no scaling occurs between the values in the I/O Device and the Engineering
/// values. You need to know the scaling for each variables as specified in the Citect SCADA
/// Variable Tags table.
///
/// # Example
/// ```no_run
/// use ctapi_rs::*;
/// use ctapi_sys::*;
/// use ctapi_rs::constants::*;
/// let scale = Ctscale::new(Cthscale::new(0.0,32000.0), Cthscale::new(0.0,100.0));
/// let result = ct_raw_to_eng(2000.0, &scale, CT_SCALE_RANGE_CHECK);
/// println!("{result:?}");
/// assert!(result.is_ok());
/// ```
pub fn ct_raw_to_eng(value: f64, scale: &CtScale, mode: u32) -> Result<f64> {
    let mut result = 0.0;
    unsafe {
        if !ctRawToEng(&mut result, value, scale, mode) {
            return Err(Error::last_os_error().into());
        }
    }
    Ok(result)
}

#[cfg(test)]
mod tests {
    use std::{thread::sleep, time::Duration};

    use super::*;
    use chrono::{Local, TimeZone};
    const COMPUTER: &str = "192.168.1.12";
    const USER: &str = "Manager";
    const PASSWORD: &str = "Citect";

    fn is_send<T: Send>(_t: T) {}

    #[test]
    fn client_tag_read_ex_test() {
        use ctapi_sys::*;
        let mut value = CtTagValueItems::default();
        let client = CtClient::open(Some(COMPUTER), Some(USER), Some(PASSWORD), 0).unwrap();
        // is_send(client);
        let result = client.tag_read_ex("BIT_1", &mut value);
        println!("{result:?} {value:?}");
    }

    #[test]
    fn client_find_first_test() {
        let client = CtClient::open(Some(COMPUTER), Some(USER), Some(PASSWORD), 0).unwrap();
        //is_copy(client);
        let result = client.find_first("Tag", "CLUSTER=Cluster1", None);
        for object in result {
            println!(
                "{:?}, {:?}",
                object.get_property("TAG").unwrap(),
                object.get_property("COMMENT").unwrap(),
                // object.get_property("ONDATE").unwrap()
            );
        }
    }

    #[test]
    fn list_test() {
        let mut client = CtClient::open(Some(COMPUTER), Some(USER), Some(PASSWORD), 0).unwrap();
        let mut list = client.list_new(0).unwrap();
        //drop(client);
        list.add("BIT_1").unwrap();
        list.read().unwrap();
        println!("{}", list.data("BIT_1", 0).unwrap());
        let v = list.delete("BIT_1");
        println!("{:?}", v);
    }

    #[test]
    fn multi_client_test() {
        let client1 = CtClient::open(Some(COMPUTER), Some(USER), Some(PASSWORD), 0).unwrap();
        let result = client1.find_first("Tag", "CLUSTER=Cluster1", None);
        let _res: Vec<()> = result
            .map(|object| {
                println!(
                    "{:?}, {:?}",
                    object.get_property("TAG").unwrap(),
                    object.get_property("COMMENT").unwrap(),
                    // object.get_property("ONDATE").unwrap()
                );
            })
            .collect();
        // for object in result {
        //     println!(
        //         "{}, {}, {}",
        //         object.get_property("object.fields(2).name").unwrap(),
        //         object.get_property("object.fields(2).type").unwrap(),
        //         object.get_property("object.fields(2).actualsize").unwrap(),
        //         //object.get_property("CLUSTER").unwrap()
        //     );
        // }
    }

    #[test]
    fn multi_thread_test() {
        let client = CtClient::open(Some(COMPUTER), Some(USER), Some(PASSWORD), 0).unwrap();
        let client1 = std::sync::Arc::new(client);
        let client2 = client1.clone();
        let handler1 = std::thread::spawn(move || {
            assert!(client1.tag_read("BIT_1").is_ok());
            let tags = client1.find_first("Tag", "CLUSTER=Cluster1", None);
            let thread_id = std::thread::current().id();
            for tag in tags {
                println!(
                    "thread id: {:?} {:?}, {:?}",
                    thread_id,
                    tag.get_property("TAG").unwrap(),
                    tag.get_property("COMMENT").unwrap(),
                );
            }
        });
        let handler2 = std::thread::spawn(move || {
            assert!(client2.tag_write("BIT_1", 1).is_ok());
            let tags = client2.find_first("Tag", "CLUSTER=Cluster1", None);
            let thread_id = std::thread::current().id();
            for tag in tags {
                println!(
                    "thread id: {:?} {:?}, {:?}",
                    thread_id,
                    tag.get_property("TAG").unwrap(),
                    tag.get_property("COMMENT").unwrap(),
                );
            }
        });
        handler1.join().unwrap();
        handler2.join().unwrap();
    }

    #[test]
    fn client_find_alarm_test() {
        // ALMQUERY,Database,TagName,Starttime,StarttimeMs,Endtime,EndtimeMs,Period
        let client = CtClient::open(Some(COMPUTER), Some(USER), Some(PASSWORD), 0).unwrap();
        let tag_name = "Feed_SPC_11";
        let time = chrono::Utc::now();
        let start_time = time
            .checked_sub_signed(chrono::Duration::days(80))
            .unwrap()
            .timestamp();
        let end_time = time.timestamp();
        let query_str = format!(
            "ALMQUERY,AdvAlm,{},{},0,{},0,0.001",
            &tag_name, &start_time, &end_time
        );
        let result = client.find_first(&query_str, "", None);
        for object in result {
            println!(
                "{}, OnMilli:{}, Comments:{},  {}",
                Local
                    .timestamp_opt(
                        object
                            .get_property("DateTime")
                            .unwrap()
                            .parse::<i64>()
                            .unwrap(),
                        0
                    )
                    .unwrap(),
                object.get_property("MSeconds").unwrap(),
                object.get_property("Comment").unwrap(),
                object.get_property("Value").unwrap()
            );
            // object.get_property("TAG").unwrap();
        }
    }

    #[test]
    fn client_drop_test() {
        let client = CtClient::open(Some(COMPUTER), Some(USER), Some(PASSWORD), 0).unwrap();
        println!("{:?}", client.tag_read("BIT_1"));
        sleep(Duration::from_secs(15));
        drop(client);
    }

    // #[test]
    // fn overlapped_test() {
    //     let mut client = CtClient::open(Some(""), Some(""), Some(""), 0).unwrap();
    //     let mut overlapped = Overlapped::initialize_with_autoreset_event().unwrap();
    //     let mut buffer = [0i8; 256];
    //     let result = client
    //         .cicode_nonblock("Time(1)", 0, 0, &mut overlapped, &mut buffer)
    //         .unwrap();
    //     let mut bytes = 0u32;
    //     unsafe {
    //         //std::thread::sleep(std::time::Duration::from_millis(50));
    //         while ctGetOverlappedResult(client.handle, overlapped.raw(), &mut bytes, true) {
    //             let s = GBK.decode(std::slice::from_raw_parts(
    //                 buffer.as_ptr() as *const u8,
    //                 strnlen(buffer.as_ptr(), 256),
    //             ));
    //             println!("{:?} {:?}", s.0, std::io::Error::last_os_error());
    //         }
    //     }
    //     println!("{:?} {:?}", overlapped, result);
    // }
}
