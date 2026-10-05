//! Digests, the functions that produce them, and the store keys they map to.
//! Both functions give 32 bytes, so a BLAKE3 key carries a prefix to keep
//! the two apart on disk.

use std::{fmt, str::FromStr};

use sha2::{Digest as _, Sha256};

use crate::grpc::Status;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub enum DigestFunction {
  #[default]
  Sha256,
  Blake3,
}

impl DigestFunction {
  pub const ALL: [Self; 2] = [Self::Sha256, Self::Blake3];

  pub fn hasher(self) -> Hasher {
    match self {
      Self::Sha256 => Hasher::Sha256(Sha256::new()),
      Self::Blake3 => Hasher::Blake3(Box::new(blake3::Hasher::new())),
    }
  }

  pub fn hash(self, data: &[u8]) -> Hash {
    let mut hasher = self.hasher();
    hasher.update(data);
    hasher.finish()
  }
}

/// The REAPI `DigestFunction.Value`, where unset means SHA-256.
impl TryFrom<i32> for DigestFunction {
  type Error = Status;

  fn try_from(value: i32) -> Result<Self, Status> {
    match value {
      0 | 1 => Ok(Self::Sha256),
      9 => Ok(Self::Blake3),
      other => {
        Err(Status::invalid_argument(format!(
          "digest function {other} is not supported, only SHA-256 and BLAKE3"
        )))
      },
    }
  }
}

impl From<DigestFunction> for i32 {
  fn from(function: DigestFunction) -> Self {
    match function {
      DigestFunction::Sha256 => 1,
      DigestFunction::Blake3 => 9,
    }
  }
}

/// The name a `ByteStream` resource uses, as in `blobs/blake3/{hash}/{size}`.
impl fmt::Display for DigestFunction {
  fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
    formatter.write_str(match self {
      Self::Sha256 => "sha256",
      Self::Blake3 => "blake3",
    })
  }
}

impl FromStr for DigestFunction {
  type Err = Status;

  fn from_str(name: &str) -> Result<Self, Status> {
    match name {
      "sha256" => Ok(Self::Sha256),
      "blake3" => Ok(Self::Blake3),
      other => {
        Err(Status::invalid_argument(format!(
          "unknown digest function {other:?}"
        )))
      },
    }
  }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Hash([u8; 32]);

impl Hash {
  /// The directory an object lives under, its first byte in hex.
  pub fn shard(&self) -> String {
    hex::encode(&self.0[..1])
  }
}

impl fmt::Display for Hash {
  fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
    formatter.write_str(&hex::encode(self.0))
  }
}

impl FromStr for Hash {
  type Err = Status;

  fn from_str(hex: &str) -> Result<Self, Status> {
    let mut bytes = [0; 32];
    hex::decode_to_slice(hex, &mut bytes).map_err(|_| {
      Status::invalid_argument(format!(
        "digest hash {hex:?} is not 64 hex digits"
      ))
    })?;

    Ok(Self(bytes))
  }
}

/// A blob's hash, the function that produced it and its size.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Digest {
  pub function: DigestFunction,
  pub hash:     Hash,
  pub size:     u64,
}

impl Digest {
  /// The empty blob is always present.
  pub fn is_empty_blob(&self) -> bool {
    self.size == 0 && self.hash == self.function.hash(&[])
  }

  pub const fn key(&self) -> Key {
    Key {
      function: self.function,
      hash:     self.hash,
    }
  }
}

/// What the store files an object under. The function is part of it since
/// the two hash the same bytes differently.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Key {
  pub function: DigestFunction,
  pub hash:     Hash,
}

impl fmt::Display for Key {
  fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
    if self.function == DigestFunction::Sha256 {
      write!(formatter, "{}", self.hash)
    } else {
      write!(formatter, "{}-{}", self.function, self.hash)
    }
  }
}

impl FromStr for Key {
  type Err = Status;

  fn from_str(name: &str) -> Result<Self, Status> {
    let (function, hash) = match name.split_once('-') {
      Some((function, hash)) => (function.parse()?, hash),
      None => (DigestFunction::Sha256, name),
    };

    Ok(Self {
      function,
      hash: hash.parse()?,
    })
  }
}

pub enum Hasher {
  Sha256(Sha256),
  Blake3(Box<blake3::Hasher>),
}

impl Hasher {
  pub fn update(&mut self, data: &[u8]) {
    match self {
      Self::Sha256(hasher) => hasher.update(data),
      Self::Blake3(hasher) => {
        hasher.update(data);
      },
    }
  }

  pub fn finish(self) -> Hash {
    match self {
      Self::Sha256(hasher) => Hash(hasher.finalize().into()),
      Self::Blake3(hasher) => Hash(*hasher.finalize().as_bytes()),
    }
  }
}
