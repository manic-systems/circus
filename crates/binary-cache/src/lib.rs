#![allow(
  clippy::all,
  clippy::cargo,
  clippy::nursery,
  clippy::pedantic,
  clippy::restriction,
  unsafe_code
)]

//! Nix binary-cache primitives used by Circus.
//!
//! The implementation is vendored from Harmonia at
//! `12c6742560dd1ba1e66f5cc2f04cabd4e99ae754`. The crate root deliberately
//! keeps Harmonia-compatible aliases so this crate can become a thin shim when
//! Harmonia publishes the needed crates.

extern crate self as harmonia_file_core;
extern crate self as harmonia_file_nar;
extern crate self as harmonia_store_content_address;
extern crate self as harmonia_store_nar_info;
extern crate self as harmonia_store_path;
extern crate self as harmonia_store_path_info;
extern crate self as harmonia_utils_base_encoding;
extern crate self as harmonia_utils_hash;
extern crate self as harmonia_utils_io;
extern crate self as harmonia_utils_signature;

pub mod base_encoding;
mod content_address;
mod file_core;
mod file_nar;
mod io;
mod nar_info;
mod path_info;
mod signature;
mod store_path;

#[path = "hash/algo.rs"] mod algo;
#[path = "hash/borrowed.rs"] mod borrowed;
#[path = "hash/fmt.rs"] pub mod fmt;
#[path = "hash/owned.rs"] mod owned;
#[path = "hash/sha256.rs"] mod sha256;
#[path = "hash/sink.rs"] mod sink;
#[path = "hash/view.rs"] mod view;

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("hash has wrong length {length} != {} for hash type '{algorithm}'", algorithm.size())]
pub struct InvalidHashError {
  pub(crate) algorithm: Algorithm,
  pub(crate) length:    usize,
}

pub mod hash {
  pub use crate::{
    Algorithm,
    BorrowedHash,
    Context,
    Hash,
    HashFormat,
    HashSink,
    HashView,
    InvalidHashError,
    Sha256,
    UnknownAlgorithm,
    fmt,
  };
}

pub use algo::{Algorithm, UnknownAlgorithm};
pub use base_encoding::{Base, base32, base64_len, decode_for_base};
pub use borrowed::BorrowedHash;
pub use content_address::{
  ContentAddress,
  ContentAddressMethod,
  ContentAddressMethodAlgorithm,
  ParseContentAddressError,
  make_store_path_from_ca,
};
pub use file_core::{
  Directory,
  FileSystemObject,
  FileTree,
  MemoryTree,
  Opaque,
  Regular,
  ShallowTree,
  Symlink,
};
pub use file_nar::{
  ByteString,
  NarByteStream,
  NarFileInfo,
  NarParser,
  NarReader,
  NarRestorer,
  NarWriteError,
  NarWriter,
  RestoreOptions,
  archive,
  dump,
  listing,
  padded_reader,
  parse_nar,
  parse_nar_listing,
  restore,
};
pub use fmt::HashFormat;
pub use io::{
  AsyncBufReadCompat,
  AsyncBytesRead,
  BytesReader,
  DEFAULT_BUF_SIZE,
  DEFAULT_MAX_BUF_SIZE,
  DEFAULT_RESERVED_BUF_SIZE,
  DrainInto,
  Lending,
  LentReader,
  RESERVED_BUF_SIZE,
  TeeWriter,
  TryReadBytesLimited,
  TryReadU64,
  wire,
};
pub use nar_info::{
  NarInfo,
  UnkeyedNarInfo,
  build_narinfo,
  format_narinfo_txt,
};
pub use owned::Hash;
pub use path_info::{
  NarHash,
  Pure,
  StorePathKeyed,
  UnkeyedValidPathInfo,
  ValidPathInfo,
  fingerprint_path,
};
pub use sha256::Sha256;
pub use signature::{
  GenerateKeyError,
  ParseKeyError,
  ParseSignatureError,
  PublicKey,
  RawSignature,
  SIGNATURE_BYTES,
  SecretKey,
  Signature,
  SignatureSet,
};
pub use sink::{Context, HashSink};
pub use store_path::{
  FromStoreDirStr,
  ParseStorePathError,
  StoreDir,
  StoreDirDisplay,
  StoreDirError,
  StorePath,
  StorePathError,
  StorePathHash,
  StorePathName,
  StorePathNameError,
  StorePathSet,
  into_name,
};
pub use view::HashView;
