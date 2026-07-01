#![allow(dead_code)]

pub mod archive;

use std::{
  path::PathBuf,
  time::{Duration, SystemTime, UNIX_EPOCH},
};

use bytes::Bytes;
use proptest::{collection, prelude::*};

pub fn arb_byte_string() -> impl Strategy<Value = Bytes> {
  any::<Vec<u8>>().prop_map(Bytes::from)
}

pub fn arb_duration() -> impl Strategy<Value = Duration> {
  any::<u64>().prop_map(Duration::from_nanos)
}

pub fn arb_file_component() -> impl Strategy<Value = String> {
  arb_filename()
}

pub fn arb_filename() -> impl Strategy<Value = String> {
  proptest::string::string_regex(r"[A-Za-z0-9._+-]{1,32}")
    .expect("filename regex is valid")
    .prop_filter("not a pseudo-directory entry", |name| {
      name != "." && name != ".."
    })
}

pub fn arb_path() -> impl Strategy<Value = PathBuf> {
  collection::vec(arb_filename(), 1..5).prop_map(|parts| {
    let mut path = PathBuf::new();
    for part in parts {
      path.push(part);
    }
    path
  })
}

pub fn arb_system_time() -> impl Strategy<Value = SystemTime> {
  any::<u64>().prop_map(|secs| UNIX_EPOCH + Duration::from_secs(secs))
}
