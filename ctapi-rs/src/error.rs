//! CtAPI error handling module
//!
//! Provides definition and handling of CtAPI-specific error types.

use std::ffi::NulError;

use std::ffi::FromBytesUntilNulError;
use std::io;
use std::str::Utf8Error;
use thiserror::Error;

/// CtAPI-specific error type
#[derive(Error, Debug)]
pub enum CtApiError {
    /// CtAPI system call failed
    #[error("CtAPI system call failed: {0}")]
    System(#[from] io::Error),

    /// UTF-8 encoding/decoding error
    #[error("UTF-8 encoding/decoding error: {0}")]
    Encoding(#[from] Utf8Error),

    /// Conversion error from bytes until null character
    #[error("Conversion error from bytes until null character: {0}")]
    FromBytesUntilNul(#[from] FromBytesUntilNulError),

    /// Null pointer error
    #[error("Null pointer error: {0}")]
    NullPointer(#[from] NulError),

    /// Tag not found
    #[error("Tag '{tag}' not found")]
    TagNotFound {
        /// Name of tag not found
        tag: String,
    },

    /// Connection failed
    #[error("Connection to Citect SCADA failed: {message}")]
    ConnectionFailed {
        /// Error message description
        message: String,
    },

    /// Invalid parameter
    #[error("Invalid parameter: {param} = {value}")]
    InvalidParameter {
        /// Parameter name
        param: String,
        /// Parameter value
        value: String,
    },

    /// Timeout error
    #[error("Operation timeout")]
    Timeout,

    /// Unsupported operation
    #[error("Unsupported operation: {operation}")]
    UnsupportedOperation {
        /// Name of unsupported operation
        operation: String,
    },

    /// Other CtAPI error
    #[error("CtAPI error code: {code}{}", if message.is_empty() { String::new() } else { format!(", message: {}", message) })]
    Other {
        /// Error code
        code: u32,
        /// Error message
        message: String,
    },
}

impl CtApiError {
    /// Create error from system error code
    pub fn from_error_code(code: u32) -> Self {
        match code {
            0 => CtApiError::Other {
                code,
                message: "Success".to_string(),
            },
            1..=999 => CtApiError::Other {
                code,
                message: String::new(),
            },
            _ => CtApiError::Other {
                code,
                message: "Unknown error".to_string(),
            },
        }
    }

    /// Check if this is a connection-related error
    pub fn is_connection_error(&self) -> bool {
        matches!(
            self,
            CtApiError::ConnectionFailed { .. } | CtApiError::Timeout
        )
    }

    /// Check if this is a tag-related error
    pub fn is_tag_error(&self) -> bool {
        matches!(self, CtApiError::TagNotFound { .. })
    }
}

/// Conversion From trait implementation
impl From<String> for CtApiError {
    fn from(tag: String) -> Self {
        CtApiError::TagNotFound { tag }
    }
}

impl From<&str> for CtApiError {
    fn from(tag: &str) -> Self {
        CtApiError::TagNotFound {
            tag: tag.to_string(),
        }
    }
}

/// Convenient type alias
pub type Result<T> = std::result::Result<T, CtApiError>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_creation() {
        let error = CtApiError::TagNotFound {
            tag: "test_tag".to_string(),
        };
        assert!(error.is_tag_error());
        assert_eq!(error.to_string(), "Tag 'test_tag' not found");
    }

    #[test]
    fn test_error_from_io() {
        let io_error = io::Error::new(io::ErrorKind::NotFound, "file not found");
        let ct_error: CtApiError = io_error.into();
        assert!(matches!(ct_error, CtApiError::System(_)));
    }

    #[test]
    fn test_error_code() {
        let error = CtApiError::from_error_code(123);
        assert_eq!(
            error.to_string(),
            "CtAPI error code: 123"
        );
    }
}
