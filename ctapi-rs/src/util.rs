//! Internal utilities shared across modules.

use std::ffi::CString;

use encoding_rs::GBK;

/// Encode a Rust string as a GBK-encoded, null-terminated C string.
pub(crate) fn encode_to_gbk_cstring(s: &str) -> std::result::Result<CString, std::ffi::NulError> {
    let (encoded, _, _) = GBK.encode(s);
    CString::new(encoded)
}

/// Convert a connection or query field without changing unrepresentable input.
pub(crate) fn encode_strict(s: &str, field: &str) -> crate::error::Result<CString> {
    let invalid = || {
        std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            format!("{field} is not a valid GBK C string"),
        )
    };
    let (encoded, _, had_errors) = GBK.encode(s);
    if had_errors {
        return Err(invalid().into());
    }
    CString::new(encoded).map_err(|_| invalid().into())
}
