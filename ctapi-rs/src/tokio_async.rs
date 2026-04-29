//! Tokio async runtime integration
//!
//! This module provides integration with the Tokio async runtime, allowing
//! CtAPI operations to be used with Rust's `async`/`await` syntax via
//! [`tokio::task::spawn_blocking`].
//!
//! # Feature flag
//!
//! Available only when the `tokio-support` feature is enabled:
//!
//! ```toml
//! ctapi-rs = { version = "...", features = ["tokio-support"] }
//! ```
//!
//! # Design
//!
//! The Citect SCADA C API is inherently blocking. Each method in this module
//! offloads the blocking call to Tokio's blocking-thread pool via
//! `spawn_blocking`, leaving the async runtime free to drive other tasks.
//!
//! For operations that natively support Windows OVERLAPPED I/O (e.g. Cicode),
//! consider [`FutureCtClient`](crate::FutureCtClient) instead — it avoids a
//! dedicated thread entirely.
//!
//! # Examples
//!
//! ```no_run
//! use ctapi_rs::{CtClient, TokioCtClient};
//! use std::sync::Arc;
//!
//! #[tokio::main]
//! async fn main() -> anyhow::Result<()> {
//!     let client = Arc::new(CtClient::open(None, None, None, 0)?);
//!
//!     let time = client.cicode_tokio("Time(1)", 0, 0).await?;
//!     println!("Server time: {}", time);
//!
//!     let temp = client.tag_read_tokio("Temperature").await?;
//!     println!("Temperature: {}", temp);
//!
//!     Ok(())
//! }
//! ```

use crate::error::Result;
use crate::{AsyncCtClient, AsyncOperation, CtClient, CtList, CtTagValueItems};
use std::sync::Arc;

// ───────────────────────────────────────────────
// TokioCtClient
// ───────────────────────────────────────────────

/// Extension trait providing `async`/`await`-compatible methods for
/// [`CtClient`].
///
/// All methods offload the blocking CtAPI call to Tokio's blocking-thread
/// pool via [`tokio::task::spawn_blocking`], so the async runtime is never
/// stalled.
///
/// # Implementations
///
/// The trait is implemented for both `CtClient` (by value / reference) and
/// `Arc<CtClient>` so callers can choose whether to clone or share the
/// client.
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
///     // Concurrent reads — spawn multiple Tokio tasks
///     let c1 = Arc::clone(&client);
///     let c2 = Arc::clone(&client);
///     let (v1, v2) = tokio::try_join!(
///         tokio::spawn(async move { c1.tag_read_tokio("BIT_1").await }),
///         tokio::spawn(async move { c2.tag_read_tokio("BIT_2").await }),
///     )?;
///     println!("BIT_1={}, BIT_2={}", v1?, v2?);
///     Ok(())
/// }
/// ```
#[allow(async_fn_in_trait)]
pub trait TokioCtClient {
    /// Execute a Cicode function asynchronously.
    ///
    /// Equivalent to [`CtClient::cicode`] but non-blocking in async contexts.
    ///
    /// # Parameters
    /// * `cmd`    - Cicode command string (e.g. `"Time(1)"`).
    /// * `vh_win` - Window handle, usually `0`.
    /// * `mode`   - Execution mode flag.
    ///
    /// # Examples
    /// ```no_run
    /// # use ctapi_rs::{CtClient, TokioCtClient};
    /// # #[tokio::main]
    /// # async fn main() -> anyhow::Result<()> {
    /// let client = CtClient::open(None, None, None, 0)?;
    /// let result = client.cicode_tokio("Time(1)", 0, 0).await?;
    /// println!("Server time: {}", result);
    /// # Ok(()) }
    /// ```
    async fn cicode_tokio(&self, cmd: &str, vh_win: u32, mode: u32) -> Result<String>;

    /// Read a tag value asynchronously.
    ///
    /// # Parameters
    /// * `tag` - Tag name.
    ///
    /// # Examples
    /// ```no_run
    /// # use ctapi_rs::{CtClient, TokioCtClient};
    /// # #[tokio::main]
    /// # async fn main() -> anyhow::Result<()> {
    /// let client = CtClient::open(None, None, None, 0)?;
    /// let value = client.tag_read_tokio("Temperature").await?;
    /// println!("Temperature: {}", value);
    /// # Ok(()) }
    /// ```
    async fn tag_read_tokio(&self, tag: &str) -> Result<String>;

    /// Read a tag value together with extended metadata (timestamp, quality).
    ///
    /// # Parameters
    /// * `tag` - Tag name.
    ///
    /// # Return Value
    /// Returns a tuple of `(value_string, CtTagValueItems)`.
    ///
    /// # Examples
    /// ```no_run
    /// # use ctapi_rs::{CtClient, TokioCtClient};
    /// # #[tokio::main]
    /// # async fn main() -> anyhow::Result<()> {
    /// let client = CtClient::open(None, None, None, 0)?;
    /// let (value, meta) = client.tag_read_ex_tokio("Pressure").await?;
    /// println!("Pressure: {}  quality: {}", value, meta.quality_general);
    /// # Ok(()) }
    /// ```
    async fn tag_read_ex_tokio(&self, tag: &str) -> Result<(String, CtTagValueItems)>;

    /// Write a tag value asynchronously.
    ///
    /// The `value` is converted to a string before being sent to the CtAPI.
    /// Both numeric and string values are accepted.
    ///
    /// # Parameters
    /// * `tag`   - Tag name.
    /// * `value` - Value to write (any type whose `Display` matches what
    ///             Citect expects).
    ///
    /// # Examples
    /// ```no_run
    /// # use ctapi_rs::{CtClient, TokioCtClient};
    /// # #[tokio::main]
    /// # async fn main() -> anyhow::Result<()> {
    /// let client = CtClient::open(None, None, None, 0)?;
    /// client.tag_write_tokio("Setpoint", "25.5").await?;
    /// client.tag_write_tokio("Pump_Start", "1").await?;
    /// # Ok(()) }
    /// ```
    async fn tag_write_tokio(&self, tag: &str, value: &str) -> Result<()>;
}

// ── impl for CtClient ────────────────────────────────────────────────────────

impl TokioCtClient for CtClient {
    async fn cicode_tokio(&self, cmd: &str, vh_win: u32, mode: u32) -> Result<String> {
        let client = Arc::new(self.clone());
        let cmd = cmd.to_string();
        spawn_blocking_result(move || {
            let mut op = AsyncOperation::new();
            client.cicode_async(&cmd, vh_win, mode, &mut op)?;
            op.get_result(&client)
        })
        .await
    }

    async fn tag_read_tokio(&self, tag: &str) -> Result<String> {
        let client = self.clone();
        let tag = tag.to_string();
        spawn_blocking_result(move || client.tag_read(&tag)).await
    }

    async fn tag_read_ex_tokio(&self, tag: &str) -> Result<(String, CtTagValueItems)> {
        let client = self.clone();
        let tag = tag.to_string();
        spawn_blocking_result(move || {
            let mut items = CtTagValueItems::default();
            let value = client.tag_read_ex(&tag, &mut items)?;
            Ok((value, items))
        })
        .await
    }

    async fn tag_write_tokio(&self, tag: &str, value: &str) -> Result<()> {
        let client = self.clone();
        let tag = tag.to_string();
        let value = value.to_string();
        spawn_blocking_result(move || client.tag_write_str(&tag, &value).map(|_| ())).await
    }
}

// ── impl for Arc<CtClient> ───────────────────────────────────────────────────

impl TokioCtClient for Arc<CtClient> {
    async fn cicode_tokio(&self, cmd: &str, vh_win: u32, mode: u32) -> Result<String> {
        let client = Arc::clone(self);
        let cmd = cmd.to_string();
        spawn_blocking_result(move || {
            let mut op = AsyncOperation::new();
            client.cicode_async(&cmd, vh_win, mode, &mut op)?;
            op.get_result(&client)
        })
        .await
    }

    async fn tag_read_tokio(&self, tag: &str) -> Result<String> {
        let client = Arc::clone(self);
        let tag = tag.to_string();
        spawn_blocking_result(move || client.tag_read(&tag)).await
    }

    async fn tag_read_ex_tokio(&self, tag: &str) -> Result<(String, CtTagValueItems)> {
        let client = Arc::clone(self);
        let tag = tag.to_string();
        spawn_blocking_result(move || {
            let mut items = CtTagValueItems::default();
            let value = client.tag_read_ex(&tag, &mut items)?;
            Ok((value, items))
        })
        .await
    }

    async fn tag_write_tokio(&self, tag: &str, value: &str) -> Result<()> {
        let client = Arc::clone(self);
        let tag = tag.to_string();
        let value = value.to_string();
        spawn_blocking_result(move || client.tag_write_str(&tag, &value).map(|_| ())).await
    }
}

// ───────────────────────────────────────────────
// TokioCtList
// ───────────────────────────────────────────────

/// Extension trait providing `async`/`await`-compatible methods for
/// [`CtList`].
///
/// # Notes
///
/// [`CtList`] is not `Send` (it borrows from [`CtClient`] and holds mutable
/// state), so methods that require crossing await points hold the list in the
/// same task.  The read itself is performed via Windows OVERLAPPED I/O and
/// polled with short [`tokio::time::sleep`] intervals.
///
/// # Examples
///
/// ```no_run
/// use ctapi_rs::{CtClient, TokioCtList};
///
/// #[tokio::main]
/// async fn main() -> anyhow::Result<()> {
///     let client = CtClient::open(None, None, None, 0)?;
///     let mut list = client.list_new(0)?;
///     list.add_tag("Temperature")?;
///     list.add_tag("Pressure")?;
///
///     list.read_tokio().await?;
///
///     println!("Temp:  {}", list.read_tag("Temperature", 0)?);
///     println!("Press: {}", list.read_tag("Pressure",    0)?);
///     Ok(())
/// }
/// ```
#[allow(async_fn_in_trait)]
pub trait TokioCtList {
    /// Read all tags in the list asynchronously.
    ///
    /// Starts the OVERLAPPED read and awaits its completion using
    /// [`tokio::time::sleep`] polling.
    ///
    /// After this future resolves, call [`CtList::read_tag`] to retrieve
    /// individual values.
    async fn read_tokio(&mut self) -> Result<()>;

    /// Write a single tag in the list asynchronously.
    ///
    /// # Parameters
    /// * `tag`   - Tag name (must already be added via [`CtList::add_tag`]).
    /// * `value` - Value string to write.
    async fn write_tag_tokio(&mut self, tag: &str, value: &str) -> Result<()>;
}

impl<'a> TokioCtList for CtList<'a> {
    async fn read_tokio(&mut self) -> Result<()> {
        let mut op = AsyncOperation::new();
        self.read_async(&mut op)
            .map_err(|e| crate::error::CtApiError::Other {
                code: 0,
                message: e.to_string(),
            })?;

        // Poll with a short sleep to yield control back to the runtime.
        while !op.is_complete() {
            tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;
        }
        Ok(())
    }

    async fn write_tag_tokio(&mut self, tag: &str, value: &str) -> Result<()> {
        let mut op = AsyncOperation::new();
        self.write_tag_async(tag, value, &mut op)
            .map_err(|e| crate::error::CtApiError::Other {
                code: 0,
                message: e.to_string(),
            })?;

        while !op.is_complete() {
            tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;
        }
        Ok(())
    }
}

// ───────────────────────────────────────────────
// Helpers
// ───────────────────────────────────────────────

/// Run `f` on Tokio's blocking thread pool and map a `JoinError` into
/// [`CtApiError::Other`].
async fn spawn_blocking_result<F, T>(f: F) -> Result<T>
where
    F: FnOnce() -> Result<T> + Send + 'static,
    T: Send + 'static,
{
    tokio::task::spawn_blocking(f)
        .await
        .map_err(|e| crate::error::CtApiError::Other {
            code: 0,
            message: e.to_string(),
        })?
}

// ───────────────────────────────────────────────
// Tests
// ───────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    /// Ensure the module compiles and basic trait bounds are satisfied.
    #[tokio::test]
    async fn test_tokio_integration_compiles() {
        fn assert_send<T: Send>() {}
        assert_send::<CtClient>();
        assert_send::<Arc<CtClient>>();
        // CtApiFuture should also be Send
        assert_send::<crate::CtApiFuture>();
    }

    /// Verify that TokioCtClient is object-safe enough to use via trait references.
    #[tokio::test]
    #[ignore = "Requires actual Citect SCADA connection"]
    async fn test_arc_client_trait() {
        let client = Arc::new(
            CtClient::open(Some("127.0.0.1"), Some("Engineer"), Some("Citect"), 0).unwrap(),
        );

        // Both Arc<CtClient> and CtClient impl TokioCtClient
        let _v1 = client.tag_read_tokio("BIT_1").await.unwrap();
        let _v2 = (*client).tag_read_tokio("BIT_1").await.unwrap();
    }

    #[tokio::test]
    #[ignore = "Requires actual Citect SCADA connection"]
    async fn test_concurrent_reads() {
        let client = Arc::new(
            CtClient::open(Some("127.0.0.1"), Some("Engineer"), Some("Citect"), 0).unwrap(),
        );

        let tags = ["BIT_1", "BIT_2", "BIT_3"];
        let mut handles = vec![];

        for tag in &tags {
            let c = Arc::clone(&client);
            let t = tag.to_string();
            handles.push(tokio::spawn(async move { c.tag_read_tokio(&t).await }));
        }

        for handle in handles {
            let result = handle.await.unwrap();
            println!("{:?}", result);
        }
    }

    #[tokio::test]
    #[ignore = "Requires actual Citect SCADA connection"]
    async fn test_tag_read_ex_tokio() {
        let client =
            CtClient::open(Some("127.0.0.1"), Some("Engineer"), Some("Citect"), 0).unwrap();
        let (value, meta) = client.tag_read_ex_tokio("BIT_1").await.unwrap();
        println!("value={} quality={}", value, meta.quality_general);
    }

    #[tokio::test]
    #[ignore = "Requires actual Citect SCADA connection"]
    async fn test_future_client_with_tokio() {
        use crate::FutureCtClient;

        let client =
            CtClient::open(Some("127.0.0.1"), Some("Engineer"), Some("Citect"), 0).unwrap();

        // FutureCtClient uses OVERLAPPED — compare result with spawn_blocking approach.
        let future_result = client.cicode_future("Time(1)", 0, 0).unwrap().await;
        let blocking_result = client.cicode_tokio("Time(1)", 0, 0).await;

        println!("future:   {:?}", future_result);
        println!("blocking: {:?}", blocking_result);
    }
}
