//!User defined Error
use std::{ffi::NulError, fmt::Debug, str::Utf8Error};
use thiserror::Error;

///User defined Result
#[doc(hidden)]
#[derive(Error, Debug)]
pub enum UserError {
    #[error("CtApi Call error")]
    CtApiError(std::io::Error),
    #[error("Parse error")]
    Utf8Error(Utf8Error),
    #[error("Null error")]
    NulError(NulError),
    #[error("Tag:{0} not found")]
    TagNotFound(String),
}

impl From<Utf8Error> for UserError {
    fn from(s: Utf8Error) -> Self {
        UserError::Utf8Error(s)
    }
}

impl From<std::io::Error> for UserError {
    fn from(s: std::io::Error) -> Self {
        UserError::CtApiError(s)
    }
}

impl From<NulError> for UserError {
    fn from(s: NulError) -> Self {
        UserError::NulError(s)
    }
}

impl From<String> for UserError {
    fn from(s: String) -> Self {
        UserError::TagNotFound(s)
    }
}

impl From<&str> for UserError {
    fn from(s: &str) -> Self {
        UserError::TagNotFound(s.into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn user_error_debug() {
        let error = UserError::TagNotFound(String::from("error"));
        println!("{:?}", error);
    }
}
