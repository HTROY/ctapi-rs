#![warn(missing_docs, missing_debug_implementations)]
#![allow(dead_code)]

//! A library for Citect SCADA
pub mod ctapi;
pub use crate::ctapi::*;

pub mod error;
pub use crate::error::*;

pub mod constants;
pub use crate::constants::*;

// re-export anyhow::Result
pub use anyhow::Result;
