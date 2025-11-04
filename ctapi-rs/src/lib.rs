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

pub mod client;
pub mod error;
pub mod find;
pub mod list;
pub mod scaling;
pub mod constants;

pub use crate::client::*;
pub use crate::find::*;
pub use crate::list::*;
pub use crate::scaling::*;
pub use crate::constants::*;

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
    use std::{thread::sleep, time::Duration};

    const COMPUTER: &str = "192.168.1.12";
    const USER: &str = "Manager";
    const PASSWORD: &str = "Citect";

    fn is_send<T: Send>(_t: T) {}

    #[test]
    fn client_tag_read_ex_test() {
        let mut value = CtTagValueItems::default();
        let client = CtClient::open(Some(COMPUTER), Some(USER), Some(PASSWORD), 0).unwrap();
        // is_send(client);
        let result = client.tag_read_ex("BIT_1", &mut value);
        println!("{result:?} {value:?}");
    }

    #[test]
    fn client_find_first_test() {
        let client = CtClient::open(Some(COMPUTER), Some(USER), Some(PASSWORD), 0).unwrap();
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
    fn list_test() {
        let mut client = CtClient::open(Some(COMPUTER), Some(USER), Some(PASSWORD), 0).unwrap();
        let mut list = client.list_new(0).unwrap();
        list.add_tag("BIT_1").unwrap();
        list.read().unwrap();
        println!("{}", list.read_tag("BIT_1", 0).unwrap());
        let v = list.delete_tag("BIT_1");
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
                );
            })
            .collect();
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
    fn client_drop_test() {
        let client = CtClient::open(Some(COMPUTER), Some(USER), Some(PASSWORD), 0).unwrap();
        println!("{:?}", client.tag_read("BIT_1"));
        sleep(Duration::from_secs(15));
        drop(client);
    }
}
