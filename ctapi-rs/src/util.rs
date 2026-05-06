//! Internal utilities shared across modules.

use std::ffi::CString;

use encoding_rs::GBK;

/// Encode a Rust string as a GBK-encoded, null-terminated C string.
pub(crate) fn encode_to_gbk_cstring(s: &str) -> std::result::Result<CString, std::ffi::NulError> {
    let (encoded, _, _) = GBK.encode(s);
    CString::new(encoded)
}
