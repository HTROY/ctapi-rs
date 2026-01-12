//! Tokio async runtime integration
//!
//! This module provides integration with the Tokio async runtime, allowing
//! CtAPI operations to be used with Rust's async/await syntax.
//!
//! # Features
//!
//! This module is only available when the `tokio-support` feature is enabled.
//!
//! # Examples
//!
//! ```no_run
//! use ctapi_rs::{CtClient, TokioCtClient};
//!
//! #[tokio::main]
//! async fn main() -> anyhow::Result<()> {
//!     let client = CtClient::open(None, None, None, 0)?;
//!     
//!     // Use async/await syntax
//!     let result = client.cicode_tokio("Time(1)", 0, 0).await?;
//!     println!("Result: {}", result);
//!     
//!     Ok(())
//! }
//! ```

use crate::error::Result;
use crate::{AsyncCtClient, AsyncOperation, CtClient, CtList};
use std::sync::Arc;

/// Extension trait for tokio async operations on CtClient
///
/// This trait provides async/await-compatible methods for CtAPI operations.
/// All methods return Futures that can be awaited in async contexts.
///
/// # Thread Safety
///
/// The CtClient can be wrapped in an Arc and shared across tasks safely.
///
/// # Examples
///
/// ```no_run
/// use ctapi_rs::{CtClient, TokioCtClient};
/// use std::sync::Arc;
///
/// #[tokio::main]
/// async fn main() -> anyhow::Result<()> {
///     let client = Arc::new(CtClient::open(None, None, None, 0)?);
///     
///     // Spawn multiple concurrent tasks
///     let mut handles = vec![];
///     
///     for i in 0..5 {
///         let client = Arc::clone(&client);
///         let handle = tokio::spawn(async move {
///             client.cicode_tokio(&format!("GetValue({})", i), 0, 0).await
///         });
///         handles.push(handle);
///     }
///     
///     // Wait for all tasks to complete
///     for handle in handles {
///         let result = handle.await??;
///         println!("Result: {}", result);
///     }
///     
///     Ok(())
/// }
/// ```
#[allow(async_fn_in_trait)]
pub trait TokioCtClient {
    /// Execute Cicode function asynchronously using tokio
    ///
    /// This method returns a Future that can be awaited. The operation
    /// runs on a background thread pool to avoid blocking the tokio runtime.
    ///
    /// # Parameters
    /// * `cmd` - Cicode command string
    /// * `vh_win` - Window handle, usually 0
    /// * `mode` - Execution mode flag
    ///
    /// # Return Value
    /// Returns a Future that resolves to the command result.
    ///
    /// # Examples
    /// ```no_run
    /// # use ctapi_rs::{CtClient, TokioCtClient};
    /// # #[tokio::main]
    /// # async fn main() -> anyhow::Result<()> {
    /// let client = CtClient::open(None, None, None, 0)?;
    /// let result = client.cicode_tokio("Time(1)", 0, 0).await?;
    /// println!("Current time: {}", result);
    /// # Ok(())
    /// # }
    /// ```
    async fn cicode_tokio(&self, cmd: &str, vh_win: u32, mode: u32) -> Result<String>;

    /// Read a tag value asynchronously using tokio
    ///
    /// # Parameters
    /// * `tag` - Tag name to read
    ///
    /// # Return Value
    /// Returns a Future that resolves to the tag value as a String.
    ///
    /// # Examples
    /// ```no_run
    /// # use ctapi_rs::{CtClient, TokioCtClient};
    /// # #[tokio::main]
    /// # async fn main() -> anyhow::Result<()> {
    /// let client = CtClient::open(None, None, None, 0)?;
    /// let temp = client.tag_read_tokio("Temperature").await?;
    /// println!("Temperature: {}", temp);
    /// # Ok(())
    /// # }
    /// ```
    async fn tag_read_tokio(&self, tag: &str) -> Result<String>;

    /// Write a tag value asynchronously using tokio
    ///
    /// # Parameters
    /// * `tag` - Tag name to write
    /// * `value` - Value to write (as string)
    ///
    /// # Return Value
    /// Returns a Future that resolves when the write completes.
    ///
    /// # Examples
    /// ```no_run
    /// # use ctapi_rs::{CtClient, TokioCtClient};
    /// # #[tokio::main]
    /// # async fn main() -> anyhow::Result<()> {
    /// let client = CtClient::open(None, None, None, 0)?;
    /// client.tag_write_tokio("Setpoint", "25.5").await?;
    /// # Ok(())
    /// # }
    /// ```
    async fn tag_write_tokio(&self, tag: &str, value: &str) -> Result<()>;
}

impl TokioCtClient for CtClient {
    async fn cicode_tokio(&self, cmd: &str, vh_win: u32, mode: u32) -> Result<String> {
        let client = Arc::new(self.clone());
        let cmd = cmd.to_string();

        tokio::task::spawn_blocking(move || {
            let mut async_op = AsyncOperation::new();
            client.cicode_async(&cmd, vh_win, mode, &mut async_op)?;
            async_op.get_result(&client)
        })
        .await
        .map_err(|e| crate::error::CtApiError::Other {
            code: 0,
            message: e.to_string(),
        })?
    }

    async fn tag_read_tokio(&self, tag: &str) -> Result<String> {
        let client = self.clone();
        let tag = tag.to_string();

        tokio::task::spawn_blocking(move || client.tag_read(&tag))
            .await
            .map_err(|e| crate::error::CtApiError::Other {
                code: 0,
                message: e.to_string(),
            })?
    }

    async fn tag_write_tokio(&self, tag: &str, value: &str) -> Result<()> {
        let client = self.clone();
        let tag = tag.to_string();
        let value_copy = value.to_string();

        tokio::task::spawn_blocking(move || {
            // Try parsing as numeric types (Copy types that work with tag_write)
            if let Ok(num) = value_copy.parse::<f64>() {
                client.tag_write(&tag, num)
            } else if let Ok(num) = value_copy.parse::<i32>() {
                client.tag_write(&tag, num)
            } else {
                // String values are not supported due to trait bounds requiring Copy
                Err(crate::error::CtApiError::InvalidParameter {
                    param: "value".to_string(),
                    value: value_copy,
                })
            }
        })
        .await
        .map_err(|e| crate::error::CtApiError::Other {
            code: 0,
            message: e.to_string(),
        })?
        .map(|_| ())
    }
}

impl TokioCtClient for Arc<CtClient> {
    async fn cicode_tokio(&self, cmd: &str, vh_win: u32, mode: u32) -> Result<String> {
        let client = Arc::clone(self);
        let cmd = cmd.to_string();

        tokio::task::spawn_blocking(move || {
            let mut async_op = AsyncOperation::new();
            client.cicode_async(&cmd, vh_win, mode, &mut async_op)?;
            async_op.get_result(&client)
        })
        .await
        .map_err(|e| crate::error::CtApiError::Other {
            code: 0,
            message: e.to_string(),
        })?
    }

    async fn tag_read_tokio(&self, tag: &str) -> Result<String> {
        let client = Arc::clone(self);
        let tag = tag.to_string();

        tokio::task::spawn_blocking(move || client.tag_read(&tag))
            .await
            .map_err(|e| crate::error::CtApiError::Other {
                code: 0,
                message: e.to_string(),
            })?
    }

    async fn tag_write_tokio(&self, tag: &str, value: &str) -> Result<()> {
        let client = Arc::clone(self);
        let tag = tag.to_string();
        let value_copy = value.to_string();

        tokio::task::spawn_blocking(move || {
            // Try parsing as numeric types (Copy types that work with tag_write)
            if let Ok(num) = value_copy.parse::<f64>() {
                client.tag_write(&tag, num)
            } else if let Ok(num) = value_copy.parse::<i32>() {
                client.tag_write(&tag, num)
            } else {
                // String values are not supported due to trait bounds requiring Copy
                Err(crate::error::CtApiError::InvalidParameter {
                    param: "value".to_string(),
                    value: value_copy,
                })
            }
        })
        .await
        .map_err(|e| crate::error::CtApiError::Other {
            code: 0,
            message: e.to_string(),
        })?
        .map(|_| ())
    }
}

/// Extension trait for tokio async operations on CtList
#[allow(async_fn_in_trait)]
pub trait TokioCtList {
    /// Read all tags in the list asynchronously using tokio
    ///
    /// # Return Value
    /// Returns a Future that resolves when all tags are read.
    ///
    /// # Examples
    /// ```no_run
    /// # use ctapi_rs::{CtClient, TokioCtList};
    /// # #[tokio::main]
    /// # async fn main() -> anyhow::Result<()> {
    /// let client = CtClient::open(None, None, None, 0)?;
    /// let mut list = client.list_new(0)?;
    /// list.add_tag("Temperature")?;
    /// list.add_tag("Pressure")?;
    ///
    /// list.read_tokio().await?;
    ///
    /// let temp = list.read_tag("Temperature", 0)?;
    /// let press = list.read_tag("Pressure", 0)?;
    /// # Ok(())
    /// # }
    /// ```
    async fn read_tokio(&mut self) -> Result<()>;
}

impl<'a> TokioCtList for CtList<'a> {
    async fn read_tokio(&mut self) -> Result<()> {
        let mut async_op = AsyncOperation::new();
        self.read_async(&mut async_op)
            .map_err(|e| crate::error::CtApiError::Other {
                code: 0,
                message: e.to_string(),
            })?;

        // Poll until complete
        while !async_op.is_complete() {
            tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    #[tokio::test]
    async fn test_tokio_integration_compiles() {
        // This test just ensures the tokio integration compiles
        // Actual testing requires a Citect SCADA connection
    }
}
