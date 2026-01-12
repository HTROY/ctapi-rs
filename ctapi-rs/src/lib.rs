#![warn(missing_docs, missing_debug_implementations)]
#![allow(dead_code)]
//! Safe bindings for Citect SCADA API
//!
//! This module provides a safe Rust interface for interacting with Citect SCADA system CtAPI.
//! Main features include:
//! - Client connection management
//! - Tag read/write operations
//! - Object search and property retrieval
//! - Tag list management
//! - Engineering units and raw value conversion
//! - Asynchronous operations with OVERLAPPED I/O

pub mod async_ops;
pub mod client;
pub mod constants;
pub mod error;
pub mod find;
pub mod list;
pub mod scaling;

#[cfg(feature = "tokio-support")]
pub mod tokio_async;

pub use crate::async_ops::{AsyncCtClient, AsyncOperation};
pub use crate::client::*;
pub use crate::constants::*;
pub use crate::find::*;
pub use crate::list::*;
pub use crate::scaling::*;

#[cfg(feature = "tokio-support")]
pub use crate::tokio_async::{TokioCtClient, TokioCtList};

// re-export anyhow::Result
pub use anyhow::Result;

// re-export commonly used types from ctapi_sys
pub use ctapi_sys::CtHScale;
pub use ctapi_sys::CtScale;
pub use ctapi_sys::CtTagValueItems;

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;
    use std::sync::Arc;
    use std::{thread::sleep, time::Duration};

    /// Helper function to get connection parameters from environment variables
    /// Returns None for each parameter if the corresponding environment variable is not set
    fn get_connection_params() -> (Option<String>, Option<String>, Option<String>) {
        let computer = std::env::var("CITECT_COMPUTER").ok();
        let user = std::env::var("CITECT_USER").ok();
        let password = std::env::var("CITECT_PASSWORD").ok();
        (computer, user, password)
    }

    fn is_send<T: Send>(_t: T) {}

    #[test]
    #[ignore = "Requires actual Citect SCADA connection"]
    fn client_tag_read_ex_test() {
        let (computer, user, password) = get_connection_params();
        let mut value = CtTagValueItems::default();
        let client =
            CtClient::open(computer.as_deref(), user.as_deref(), password.as_deref(), 0).unwrap();
        // is_send(client);
        let result = client.tag_read_ex("BIT_1", &mut value);
        println!("{result:?} {value:?}");
    }

    #[test]
    #[ignore = "Requires actual Citect SCADA connection"]
    fn client_find_first_test() {
        let (computer, user, password) = get_connection_params();
        let client =
            CtClient::open(computer.as_deref(), user.as_deref(), password.as_deref(), 0).unwrap();
        let result = client.find_first("Tag", "CLUSTER=Cluster1", None);
        for object in result {
            println!(
                "{:?}, {:?}",
                object.get_property("TAG").unwrap(),
                object.get_property("COMMENT").unwrap(),
            );
        }
    }

    #[test]
    #[ignore = "Requires actual Citect SCADA connection"]
    fn list_test() {
        let (computer, user, password) = get_connection_params();
        let client =
            CtClient::open(computer.as_deref(), user.as_deref(), password.as_deref(), 0).unwrap();
        let mut list = client.list_new(0).unwrap();
        list.add_tag("BIT_1").unwrap();
        list.read().unwrap();
        println!("{}", list.read_tag("BIT_1", 0).unwrap());
        let v = list.delete_tag("BIT_1");
        println!("{:?}", v);
    }

    #[test]
    #[ignore = "Requires actual Citect SCADA connection"]
    fn multi_client_test() {
        let (computer, user, password) = get_connection_params();
        let client1 =
            CtClient::open(computer.as_deref(), user.as_deref(), password.as_deref(), 0).unwrap();
        let result = client1.find_first("Tag", "CLUSTER=Cluster1", None);
        let _res: Vec<()> = result
            .map(|object| {
                println!(
                    "{:?}, {:?}",
                    object.get_property("TAG").unwrap(),
                    object.get_property("COMMENT").unwrap(),
                );
            })
            .collect();
    }

    #[test]
    #[ignore = "Requires actual Citect SCADA connection"]
    fn multi_thread_test() {
        // This test verifies that CtClient can be safely shared across threads using Arc
        let (computer, user, password) = get_connection_params();
        let client =
            CtClient::open(computer.as_deref(), user.as_deref(), password.as_deref(), 0).unwrap();
        let client = std::sync::Arc::new(client);

        let client1 = Arc::clone(&client);
        let client2 = Arc::clone(&client);

        let handler1 = std::thread::spawn(move || {
            let thread_id = std::thread::current().id();

            // Test concurrent reads
            assert!(client1.tag_read("BIT_1").is_ok());

            // Each thread creates its own CtFind (not shared)
            let tags = client1.find_first("Tag", "CLUSTER=Cluster1", None);
            for tag in tags {
                println!(
                    "thread {:?}: TAG={:?}, COMMENT={:?}",
                    thread_id,
                    tag.get_property("TAG").unwrap(),
                    tag.get_property("COMMENT").unwrap(),
                );
            }
            // CtFind is dropped here, before thread exits
        });

        let handler2 = std::thread::spawn(move || {
            let thread_id = std::thread::current().id();

            // Test concurrent writes
            assert!(client2.tag_write("BIT_1", 1).is_ok());

            // Each thread creates its own CtFind
            let tags = client2.find_first("Tag", "CLUSTER=Cluster1", None);
            for tag in tags {
                println!(
                    "thread {:?}: TAG={:?}, COMMENT={:?}",
                    thread_id,
                    tag.get_property("TAG").unwrap(),
                    tag.get_property("COMMENT").unwrap(),
                );
            }
            // CtFind is dropped here, before thread exits
        });

        handler1.join().unwrap();
        handler2.join().unwrap();

        // Arc<CtClient> is dropped here, after all threads finish and all CtFind objects are dropped
    }

    #[test]
    #[ignore = "Requires actual Citect SCADA connection"]
    fn client_find_alarm_test() {
        let (computer, user, password) = get_connection_params();
        let client =
            CtClient::open(computer.as_deref(), user.as_deref(), password.as_deref(), 0).unwrap();
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
                chrono::Local
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
        }
    }

    #[test]
    #[ignore = "Requires actual Citect SCADA connection"]
    fn client_drop_test() {
        let (computer, user, password) = get_connection_params();
        let client =
            CtClient::open(computer.as_deref(), user.as_deref(), password.as_deref(), 0).unwrap();
        println!("{:?}", client.tag_read("BIT_1"));
        sleep(Duration::from_secs(15));
        drop(client);
    }
}
