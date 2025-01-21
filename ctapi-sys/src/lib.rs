#![allow(dead_code)]
use std::os::windows::raw::HANDLE;
use std::{ffi::c_void, os::raw::c_char};
pub use windows_sys::Win32::System::IO::OVERLAPPED;
pub type LPCSTR = *const c_char;
pub type LPSTR = *mut c_char;
pub type DWORD = u32;

#[derive(Clone, Copy, Debug)]
#[repr(C, packed)]
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
    /// Get the ct tag value items's length.
    pub fn length(&self) -> u32 {
        self.length
    }
}

impl Default for CtTagValueItems {
    fn default() -> Self {
        Self {
            length: 38,
            timestamp: 0,
            value_timestamp: 0,
            quality_timestamp: 0,
            quality_general: 0,
            quality_substatus: 0,
            quality_limit: 0,
            quality_extended_substatus: 0,
            quality_datasource_error: 0,
            boverride: false,
            control_mode: false,
        }
    }
}

/// A struct reprent the range of value
#[derive(Debug, Clone, Copy)]
#[repr(C, packed)]
pub struct CtHScale {
    zero: f64,
    full: f64,
}

impl CtHScale {
    /// Create a new cthscale
    pub fn new(zero: f64, full: f64) -> Self {
        Self { zero, full }
    }

    /// Get the cthscale's zero.
    pub fn zero(&self) -> f64 {
        self.zero
    }

    /// Set the cthscale's zero.
    pub fn set_zero(&mut self, zero: f64) {
        self.zero = zero;
    }

    /// Get the cthscale's full.
    pub fn full(&self) -> f64 {
        self.full
    }

    /// Set the cthscale's full.
    pub fn set_full(&mut self, full: f64) {
        self.full = full;
    }
}

impl Default for CtHScale {
    fn default() -> Self {
        Self {
            zero: 0.0,
            full: 0.0,
        }
    }
}

#[derive(Debug, Clone, Copy)]
#[repr(C, packed)]
pub struct CtScale {
    raw: CtHScale,
    eng: CtHScale,
}

impl CtScale {
    pub fn new(raw: CtHScale, eng: CtHScale) -> Self {
        Self { raw, eng }
    }

    /// Get the ctscale's raw.
    pub fn raw(&self) -> CtHScale {
        self.raw
    }

    /// Set the ctscale's raw.
    pub fn set_raw(&mut self, raw: CtHScale) {
        self.raw = raw;
    }

    /// Get the ctscale's eng.
    pub fn eng(&self) -> CtHScale {
        self.eng
    }

    /// Set the ctscale's eng.
    pub fn set_eng(&mut self, eng: CtHScale) {
        self.eng = eng;
    }
}

impl Default for CtScale {
    fn default() -> Self {
        Self::new(CtHScale::default(), CtHScale::default())
    }
}
#[repr(C)]
#[allow(non_camel_case_types)]
pub enum DBTYPEENUM {
    DBTYPE_EMPTY = 0,
    DBTYPE_NULL = 1,
    DBTYPE_I2 = 2,
    DBTYPE_I4 = 3,
    DBTYPE_R4 = 4,
    DBTYPE_R8 = 5,
    DBTYPE_CY = 6,
    DBTYPE_DATE = 7,
    DBTYPE_BSTR = 8,
    DBTYPE_IDISPATCH = 9,
    DBTYPE_ERROR = 10,
    DBTYPE_BOOL = 11,
    DBTYPE_VARIANT = 12,
    DBTYPE_IUNKNOWN = 13,
    DBTYPE_DECIMAL = 14,
    DBTYPE_UI1 = 17,
    DBTYPE_ARRAY = 0x2000,
    DBTYPE_BYREF = 0x4000,
    DBTYPE_I1 = 16,
    DBTYPE_UI2 = 18,
    DBTYPE_UI4 = 19,
    DBTYPE_I8 = 20,
    DBTYPE_UI8 = 21,
    DBTYPE_GUID = 72,
    DBTYPE_VECTOR = 0x1000,
    DBTYPE_RESERVED = 0x8000,
    DBTYPE_BYTES = 128,
    DBTYPE_STR = 129,
    DBTYPE_WSTR = 130,
    DBTYPE_NUMERIC = 131,
    DBTYPE_UDT = 132,
    DBTYPE_DBDATE = 133,
    DBTYPE_DBTIME = 134,
    DBTYPE_DBTIMESTAMP = 135,
}

#[cfg(target_os = "windows")]
#[link(name = "CtApi", kind = "raw-dylib")]
#[allow(non_snake_case)]
extern "system" {
    ///FFI API function
    pub fn ctCancelIO(hCTAPI: HANDLE, pctOverlapped: *mut OVERLAPPED) -> bool;
    pub fn ctCicode(
        hCTAPI: HANDLE,
        sCmd: LPCSTR,
        vhWin: DWORD,
        nMode: DWORD,
        sResult: LPSTR,
        dwLength: DWORD,
        pctOverlapped: *mut OVERLAPPED,
    ) -> bool;
    pub fn ctClientCreate() -> HANDLE;
    pub fn ctClientDestroy(hCTAPI: HANDLE) -> bool;
    pub fn ctClose(hCTAPI: HANDLE) -> bool;
    pub fn ctCloseEx(hCTAPI: HANDLE, bDestroy: bool) -> bool;
    pub fn ctEngToRaw(
        pResult: *mut f64,
        dValue: f64,
        pScale: *const CtScale,
        dwMode: DWORD,
    ) -> bool;
    pub fn ctFindClose(hnd: HANDLE) -> bool;
    pub fn ctFindFirst(
        hCTAPI: HANDLE,
        szTableName: LPCSTR,
        szFilter: LPCSTR,
        pObjHnd: *mut HANDLE,
        dwFlags: DWORD,
    ) -> HANDLE;
    pub fn ctFindFirstEx(
        hCTAPI: HANDLE,
        szTableName: LPCSTR,
        szFilter: LPCSTR,
        szCluster: LPCSTR,
        pObjHnd: *mut HANDLE,
        dwFlags: DWORD,
    ) -> HANDLE;
    pub fn ctFindNext(hnd: HANDLE, pObjHnd: *mut HANDLE) -> bool;
    pub fn ctFindNumRecords(hnd: HANDLE) -> i32;
    pub fn ctFindPrev(hnd: HANDLE, pObjHnd: *mut HANDLE) -> bool;
    pub fn ctFindScroll(hnd: HANDLE, dwMode: DWORD, dwOffset: i32, pObjHnd: *mut HANDLE) -> DWORD;
    pub fn ctGetOverlappedResult(
        hCTAPI: HANDLE,
        lpctOverlapped: *mut OVERLAPPED,
        pBytes: *mut DWORD,
        bWait: bool,
    ) -> bool;
    pub fn ctGetProperty(
        hnd: HANDLE,
        szName: LPCSTR,
        pData: *mut c_void,
        dwBufferLength: DWORD,
        dwResultLength: *mut DWORD,
        dwType: DBTYPEENUM,
    ) -> bool;
    //fn ctHasOverlappedIsCompleted(lpctOverlapped: *mut CtOverlapped) -> bool;
    pub fn ctListAdd(hCTAPI: HANDLE, sTag: LPCSTR) -> HANDLE;
    pub fn ctListAddEx(
        hList: HANDLE,
        sTag: LPCSTR,
        bRaw: bool,
        nPollPerodMS: i32,
        dDeadban: f64,
    ) -> HANDLE;
    pub fn ctListData(hTag: HANDLE, pBuffer: *mut c_void, dwLength: DWORD, dwMode: DWORD) -> bool;
    pub fn ctListDelete(hTag: HANDLE) -> bool;
    pub fn ctListEvent(hCTAPI: HANDLE, dwMode: DWORD) -> HANDLE;
    pub fn ctListFree(hList: HANDLE) -> bool;
    pub fn ctListItem(
        hTag: HANDLE,
        dwitem: DWORD,
        pBuffer: *mut c_void,
        dwLength: DWORD,
        dwMode: DWORD,
    ) -> bool;
    pub fn ctListNew(hTag: HANDLE, dwMode: DWORD) -> HANDLE;
    pub fn ctListRead(hList: HANDLE, pctOverlapped: *mut OVERLAPPED) -> bool;
    pub fn ctListWrite(hTag: HANDLE, sValue: LPCSTR, pctOverlapped: *mut OVERLAPPED) -> bool;
    pub fn ctOpen(sComputer: LPCSTR, sUser: LPCSTR, sPassword: LPCSTR, nMode: u32) -> HANDLE;
    pub fn ctOpenEx(
        sComputer: LPCSTR,
        sUser: LPCSTR,
        sPassword: LPCSTR,
        nMode: DWORD,
        hCTAPI: HANDLE,
    ) -> bool;
    pub fn ctRawToEng(
        pResult: *mut f64,
        dValue: f64,
        pScale: *const CtScale,
        dwMode: DWORD,
    ) -> bool;
    pub fn ctTagGetProperty(
        hCTAPI: HANDLE,
        szTagName: LPCSTR,
        szProperty: LPCSTR,
        pData: *mut c_void,
        dwBufferLength: DWORD,
        dwType: DWORD,
    ) -> bool;
    pub fn ctTagRead(hCTAPI: HANDLE, sTag: LPCSTR, sValue: LPSTR, dwLength: DWORD) -> bool;
    pub fn ctTagReadEx(
        hCTAPI: HANDLE,
        sTag: LPCSTR,
        sValue: LPSTR,
        dwLength: DWORD,
        pctTagvalueItems: *mut CtTagValueItems,
    ) -> bool;
    pub fn ctTagWrite(hCTAPI: HANDLE, sTag: LPCSTR, sValue: LPCSTR) -> bool;
    pub fn ctTagWriteEx(
        hCTAPI: HANDLE,
        sTag: LPCSTR,
        sValue: LPCSTR,
        pctOverlapped: *mut OVERLAPPED,
    ) -> bool;

}
