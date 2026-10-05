//! The REAPI and `ByteStream` messages the cache reads and writes, with only
//! the fields it uses. Tags follow `remote_execution.proto` and
//! `bytestream.proto`. Requests decode into typed digests and resources.

use std::str::FromStr;

use bytes::Bytes;
use prost::Message;

use crate::{
  digest::{Digest, DigestFunction},
  grpc::Status,
};

/// The gRPC service names, the `package.Service` part of each method path.
pub const ACTION_CACHE: &str = "build.bazel.remote.execution.v2.ActionCache";
pub const CAS: &str =
  "build.bazel.remote.execution.v2.ContentAddressableStorage";
pub const CAPABILITIES: &str = "build.bazel.remote.execution.v2.Capabilities";
pub const BYTESTREAM: &str = "google.bytestream.ByteStream";

impl From<prost::DecodeError> for Status {
  fn from(error: prost::DecodeError) -> Self {
    Self::invalid_argument(format!("malformed protobuf: {error}"))
  }
}

fn wire_size(size: u64) -> i64 {
  i64::try_from(size).unwrap_or(i64::MAX)
}

fn wire_offset(value: i64) -> Result<u64, Status> {
  u64::try_from(value)
    .map_err(|_| Status::invalid_argument("negative offset or limit"))
}

/// The REAPI `Compressor.Value`s the cache speaks.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Compressor {
  Identity,
  Zstd,
}

impl TryFrom<i32> for Compressor {
  type Error = Status;

  fn try_from(value: i32) -> Result<Self, Status> {
    match value {
      0 => Ok(Self::Identity),
      1 => Ok(Self::Zstd),
      other => {
        Err(Status::invalid_argument(format!(
          "compressor {other} is not supported, only zstd"
        )))
      },
    }
  }
}

impl From<Compressor> for i32 {
  fn from(compressor: Compressor) -> Self {
    match compressor {
      Compressor::Identity => 0,
      Compressor::Zstd => 1,
    }
  }
}

#[derive(Message)]
struct WireDigest {
  #[prost(string, tag = "1")]
  hash:       String,
  #[prost(int64, tag = "2")]
  size_bytes: i64,
}

impl WireDigest {
  fn typed(self, function: DigestFunction) -> Result<Digest, Status> {
    Ok(Digest {
      function,
      hash: self.hash.parse()?,
      size: u64::try_from(self.size_bytes)
        .map_err(|_| Status::invalid_argument("digest size is negative"))?,
    })
  }

  fn required(
    digest: Option<Self>,
    function: DigestFunction,
  ) -> Result<Digest, Status> {
    digest
      .ok_or_else(|| Status::invalid_argument("missing digest"))?
      .typed(function)
  }
}

impl From<Digest> for WireDigest {
  fn from(digest: Digest) -> Self {
    Self {
      hash:       digest.hash.to_string(),
      size_bytes: wire_size(digest.size),
    }
  }
}

fn typed_all(
  wire: impl IntoIterator<Item = WireDigest>,
  function: DigestFunction,
) -> Result<Vec<Digest>, Status> {
  wire
    .into_iter()
    .map(|digest| digest.typed(function))
    .collect()
}

#[derive(Message)]
struct ActionResult {
  #[prost(message, repeated, tag = "2")]
  output_files:       Vec<OutputFile>,
  #[prost(message, repeated, tag = "3")]
  output_directories: Vec<OutputDirectory>,
  #[prost(message, optional, tag = "6")]
  stdout_digest:      Option<WireDigest>,
  #[prost(message, optional, tag = "8")]
  stderr_digest:      Option<WireDigest>,
}

#[derive(Message)]
struct OutputFile {
  #[prost(message, optional, tag = "2")]
  digest: Option<WireDigest>,
}

#[derive(Message)]
struct OutputDirectory {
  #[prost(message, optional, tag = "3")]
  tree_digest: Option<WireDigest>,
}

#[derive(Message)]
struct Tree {
  #[prost(message, optional, tag = "1")]
  root:     Option<Directory>,
  #[prost(message, repeated, tag = "2")]
  children: Vec<Directory>,
}

#[derive(Message)]
struct Directory {
  #[prost(message, repeated, tag = "1")]
  files: Vec<FileNode>,
}

#[derive(Message)]
struct FileNode {
  #[prost(message, optional, tag = "2")]
  digest: Option<WireDigest>,
}

/// The blobs an `ActionResult` names in CAS, and the `Tree` of each output
/// directory.
pub struct ActionResultBlobs {
  pub blobs: Vec<Digest>,
  pub trees: Vec<Digest>,
}

impl ActionResultBlobs {
  pub fn decode(
    action_result: &[u8],
    function: DigestFunction,
  ) -> Result<Self, Status> {
    let result = ActionResult::decode(action_result)?;

    let files = result
      .output_files
      .into_iter()
      .filter_map(|file| file.digest);

    let trees = result
      .output_directories
      .into_iter()
      .filter_map(|directory| directory.tree_digest);

    Ok(Self {
      blobs: typed_all(
        files
          .chain(result.stdout_digest)
          .chain(result.stderr_digest),
        function,
      )?,
      trees: typed_all(trees, function)?,
    })
  }
}

/// The digests of every file in a `Tree`, its root and children alike.
pub fn tree_file_digests(
  tree: &[u8],
  function: DigestFunction,
) -> Result<Vec<Digest>, Status> {
  let tree = Tree::decode(tree)?;

  typed_all(
    tree
      .root
      .into_iter()
      .chain(tree.children)
      .flat_map(|directory| directory.files)
      .filter_map(|file| file.digest),
    function,
  )
}

#[derive(Message)]
struct WireGetActionResultRequest {
  #[prost(string, tag = "1")]
  instance_name:   String,
  #[prost(message, optional, tag = "2")]
  action_digest:   Option<WireDigest>,
  #[prost(int32, tag = "6")]
  digest_function: i32,
}

pub struct GetActionResultRequest {
  pub instance_name: String,
  pub action:        Digest,
}

impl TryFrom<Bytes> for GetActionResultRequest {
  type Error = Status;

  fn try_from(message: Bytes) -> Result<Self, Status> {
    let wire = WireGetActionResultRequest::decode(message)?;
    let function = wire.digest_function.try_into()?;

    Ok(Self {
      instance_name: wire.instance_name,
      action:        WireDigest::required(wire.action_digest, function)?,
    })
  }
}

#[derive(Message)]
struct WireUpdateActionResultRequest {
  #[prost(string, tag = "1")]
  instance_name:   String,
  #[prost(message, optional, tag = "2")]
  action_digest:   Option<WireDigest>,
  #[prost(bytes = "bytes", tag = "3")]
  action_result:   Bytes,
  #[prost(int32, tag = "5")]
  digest_function: i32,
}

/// The action result is stored and served as the client's bytes.
pub struct UpdateActionResultRequest {
  pub instance_name: String,
  pub action:        Digest,
  pub action_result: Bytes,
}

impl TryFrom<Bytes> for UpdateActionResultRequest {
  type Error = Status;

  fn try_from(message: Bytes) -> Result<Self, Status> {
    let wire = WireUpdateActionResultRequest::decode(message)?;
    let function = wire.digest_function.try_into()?;

    Ok(Self {
      instance_name: wire.instance_name,
      action:        WireDigest::required(wire.action_digest, function)?,
      action_result: wire.action_result,
    })
  }
}

#[derive(Message)]
struct WireFindMissingBlobsRequest {
  #[prost(string, tag = "1")]
  instance_name:   String,
  #[prost(message, repeated, tag = "2")]
  blob_digests:    Vec<WireDigest>,
  #[prost(int32, tag = "3")]
  digest_function: i32,
}

#[derive(Message)]
struct WireBatchReadBlobsRequest {
  #[prost(string, tag = "1")]
  instance_name:          String,
  #[prost(message, repeated, tag = "2")]
  digests:                Vec<WireDigest>,
  #[prost(int32, repeated, tag = "3")]
  acceptable_compressors: Vec<i32>,
  #[prost(int32, tag = "4")]
  digest_function:        i32,
}

pub struct FindMissingBlobsRequest {
  pub instance_name: String,
  pub digests:       Vec<Digest>,
}

impl TryFrom<Bytes> for FindMissingBlobsRequest {
  type Error = Status;

  fn try_from(message: Bytes) -> Result<Self, Status> {
    let wire = WireFindMissingBlobsRequest::decode(message)?;

    Ok(Self {
      instance_name: wire.instance_name,
      digests:       typed_all(
        wire.blob_digests,
        wire.digest_function.try_into()?,
      )?,
    })
  }
}

pub struct BatchReadBlobsRequest {
  pub instance_name: String,
  pub digests:       Vec<Digest>,
  /// How to send the blobs back, zstd when the client accepts it.
  pub compressor:    Compressor,
}

impl TryFrom<Bytes> for BatchReadBlobsRequest {
  type Error = Status;

  fn try_from(message: Bytes) -> Result<Self, Status> {
    let wire = WireBatchReadBlobsRequest::decode(message)?;

    Ok(Self {
      instance_name: wire.instance_name,
      digests:       typed_all(wire.digests, wire.digest_function.try_into()?)?,
      compressor:    if wire
        .acceptable_compressors
        .contains(&Compressor::Zstd.into())
      {
        Compressor::Zstd
      } else {
        Compressor::Identity
      },
    })
  }
}

#[derive(Message)]
struct WireBatchUpdateBlobsRequest {
  #[prost(string, tag = "1")]
  instance_name:   String,
  #[prost(message, repeated, tag = "2")]
  requests:        Vec<WireBlobUpload>,
  #[prost(int32, tag = "5")]
  digest_function: i32,
}

#[derive(Message)]
struct WireBlobUpload {
  #[prost(message, optional, tag = "1")]
  digest:     Option<WireDigest>,
  #[prost(bytes = "bytes", tag = "2")]
  data:       Bytes,
  #[prost(int32, tag = "3")]
  compressor: i32,
}

pub struct BlobUpload {
  pub digest:     Digest,
  pub data:       Bytes,
  pub compressor: Compressor,
}

pub struct BatchUpdateBlobsRequest {
  pub instance_name: String,
  pub blobs:         Vec<BlobUpload>,
}

impl TryFrom<Bytes> for BatchUpdateBlobsRequest {
  type Error = Status;

  fn try_from(message: Bytes) -> Result<Self, Status> {
    let wire = WireBatchUpdateBlobsRequest::decode(message)?;
    let function = wire.digest_function.try_into()?;

    let blobs = wire
      .requests
      .into_iter()
      .map(|blob| {
        Ok(BlobUpload {
          digest:     WireDigest::required(blob.digest, function)?,
          data:       blob.data,
          compressor: blob.compressor.try_into()?,
        })
      })
      .collect::<Result<_, Status>>()?;

    Ok(Self {
      instance_name: wire.instance_name,
      blobs,
    })
  }
}

#[derive(Message)]
pub struct GetCapabilitiesRequest {
  #[prost(string, tag = "1")]
  pub instance_name: String,
}

/// A blob named `{instance}/blobs/{hash}/{size}`, with the function after
/// `blobs/` when it is not SHA-256.
#[derive(Debug, PartialEq, Eq)]
pub struct BlobResource {
  pub instance:   String,
  pub digest:     Digest,
  /// Set by a `compressed-blobs/zstd/...` name.
  pub compressor: Compressor,
}

/// An upload named `{instance}/uploads/{uuid}/blobs/...`.
#[derive(Debug, PartialEq, Eq)]
pub struct UploadResource(pub BlobResource);

/// The segments before `blobs` or `compressed-blobs/zstd`, and the rest.
fn split_resource(
  name: &str,
) -> Result<(Vec<&str>, Digest, Compressor), Status> {
  let malformed =
    || Status::invalid_argument(format!("bad resource name {name:?}"));
  let segments = name.split('/').collect::<Vec<_>>();

  let at = segments
    .iter()
    .rposition(|segment| matches!(*segment, "blobs" | "compressed-blobs"))
    .ok_or_else(malformed)?;
  let (prefix, rest) = segments.split_at(at);

  let (compressor, tail) = match rest {
    ["blobs", tail @ ..] => (Compressor::Identity, tail),
    ["compressed-blobs", "zstd", tail @ ..] => (Compressor::Zstd, tail),
    _ => return Err(malformed()),
  };

  let (function, hash, size) = match tail {
    [function, hash, size] => (function.parse()?, hash, size),
    [hash, size] => (DigestFunction::Sha256, hash, size),
    _ => return Err(malformed()),
  };

  let digest = Digest {
    function,
    hash: hash.parse()?,
    size: size.parse().map_err(|_| malformed())?,
  };

  Ok((prefix.to_vec(), digest, compressor))
}

impl FromStr for BlobResource {
  type Err = Status;

  fn from_str(name: &str) -> Result<Self, Status> {
    let (prefix, digest, compressor) = split_resource(name)?;

    Ok(Self {
      instance: prefix.join("/"),
      digest,
      compressor,
    })
  }
}

impl FromStr for UploadResource {
  type Err = Status;

  fn from_str(name: &str) -> Result<Self, Status> {
    let (prefix, digest, compressor) = split_resource(name)?;

    let [instance @ .., "uploads", _] = prefix.as_slice() else {
      return Err(Status::invalid_argument(format!(
        "bad upload resource name {name:?}"
      )));
    };

    Ok(Self(BlobResource {
      instance: instance.join("/"),
      digest,
      compressor,
    }))
  }
}

#[derive(Message)]
struct WireReadRequest {
  #[prost(string, tag = "1")]
  resource_name: String,
  #[prost(int64, tag = "2")]
  read_offset:   i64,
  #[prost(int64, tag = "3")]
  read_limit:    i64,
}

pub struct ReadRequest {
  pub resource: BlobResource,
  pub offset:   u64,
  /// Zero reads to the end.
  pub limit:    u64,
}

impl TryFrom<Bytes> for ReadRequest {
  type Error = Status;

  fn try_from(message: Bytes) -> Result<Self, Status> {
    let wire = WireReadRequest::decode(message)?;

    Ok(Self {
      resource: wire.resource_name.parse()?,
      offset:   wire_offset(wire.read_offset)?,
      limit:    wire_offset(wire.read_limit)?,
    })
  }
}

/// Shares its one field with `ReadRequest`.
pub struct QueryWriteStatusRequest {
  pub resource: UploadResource,
}

impl TryFrom<Bytes> for QueryWriteStatusRequest {
  type Error = Status;

  fn try_from(message: Bytes) -> Result<Self, Status> {
    let wire = WireReadRequest::decode(message)?;

    Ok(Self {
      resource: wire.resource_name.parse()?,
    })
  }
}

#[derive(Message)]
struct WireWriteRequest {
  #[prost(string, tag = "1")]
  resource_name: String,
  #[prost(int64, tag = "2")]
  write_offset:  i64,
  #[prost(bool, tag = "3")]
  finish_write:  bool,
  #[prost(bytes = "bytes", tag = "10")]
  data:          Bytes,
}

pub struct WriteRequest {
  /// Required on the first message of a write, optional after.
  pub resource: Option<UploadResource>,
  pub offset:   u64,
  pub finish:   bool,
  pub data:     Bytes,
}

impl TryFrom<Bytes> for WriteRequest {
  type Error = Status;

  fn try_from(message: Bytes) -> Result<Self, Status> {
    let wire = WireWriteRequest::decode(message)?;

    let resource = if wire.resource_name.is_empty() {
      None
    } else {
      Some(wire.resource_name.parse()?)
    };

    Ok(Self {
      resource,
      offset: wire_offset(wire.write_offset)?,
      finish: wire.finish_write,
      data: wire.data,
    })
  }
}

#[derive(Message)]
pub struct ReadResponse {
  #[prost(bytes = "bytes", tag = "10")]
  pub data: Bytes,
}

#[derive(Message)]
pub struct WriteResponse {
  #[prost(int64, tag = "1")]
  committed_size: i64,
}

impl WriteResponse {
  pub fn committed(size: u64) -> Self {
    Self {
      committed_size: wire_size(size),
    }
  }

  /// What a compressed upload of a blob already present answers.
  pub const fn already_present() -> Self {
    Self { committed_size: -1 }
  }
}

#[derive(Message)]
pub struct QueryWriteStatusResponse {
  #[prost(int64, tag = "1")]
  committed_size: i64,
  #[prost(bool, tag = "2")]
  complete:       bool,
}

impl QueryWriteStatusResponse {
  pub fn complete(size: u64) -> Self {
    Self {
      committed_size: wire_size(size),
      complete:       true,
    }
  }
}

#[derive(Message)]
pub struct FindMissingBlobsResponse {
  #[prost(message, repeated, tag = "2")]
  missing_blob_digests: Vec<WireDigest>,
}

impl FromIterator<Digest> for FindMissingBlobsResponse {
  fn from_iter<I: IntoIterator<Item = Digest>>(missing: I) -> Self {
    Self {
      missing_blob_digests: missing.into_iter().map(WireDigest::from).collect(),
    }
  }
}

/// A `google.rpc.Status`.
#[derive(Message)]
struct RpcStatus {
  #[prost(int32, tag = "1")]
  code:    i32,
  #[prost(string, tag = "2")]
  message: String,
}

impl From<Option<Status>> for RpcStatus {
  fn from(status: Option<Status>) -> Self {
    status.map_or_else(Self::default, |status| {
      Self {
        code:    status.code() as i32,
        message: status.message().to_owned(),
      }
    })
  }
}

#[derive(Message)]
pub struct BatchUpdateBlobsResponse {
  #[prost(message, repeated, tag = "1")]
  responses: Vec<UpdatedBlob>,
}

#[derive(Message)]
struct UpdatedBlob {
  #[prost(message, optional, tag = "1")]
  digest: Option<WireDigest>,
  #[prost(message, optional, tag = "2")]
  status: Option<RpcStatus>,
}

/// One status per blob, `None` meaning stored.
impl FromIterator<(Digest, Option<Status>)> for BatchUpdateBlobsResponse {
  fn from_iter<I: IntoIterator<Item = (Digest, Option<Status>)>>(
    results: I,
  ) -> Self {
    Self {
      responses: results
        .into_iter()
        .map(|(digest, status)| {
          UpdatedBlob {
            digest: Some(digest.into()),
            status: Some(status.into()),
          }
        })
        .collect(),
    }
  }
}

#[derive(Message)]
pub struct BatchReadBlobsResponse {
  #[prost(message, repeated, tag = "1")]
  responses: Vec<ReadBlob>,
}

#[derive(Message)]
struct ReadBlob {
  #[prost(message, optional, tag = "1")]
  digest:     Option<WireDigest>,
  #[prost(bytes = "vec", tag = "2")]
  data:       Vec<u8>,
  #[prost(message, optional, tag = "3")]
  status:     Option<RpcStatus>,
  #[prost(int32, tag = "4")]
  compressor: i32,
}

/// Each blob's data as sent, with the compressor it was sent under.
impl FromIterator<(Digest, Result<(Vec<u8>, Compressor), Status>)>
  for BatchReadBlobsResponse
{
  fn from_iter<
    I: IntoIterator<Item = (Digest, Result<(Vec<u8>, Compressor), Status>)>,
  >(
    results: I,
  ) -> Self {
    Self {
      responses: results
        .into_iter()
        .map(|(digest, result)| {
          let (data, compressor, status) = match result {
            Ok((data, compressor)) => (data, compressor, None),
            Err(status) => (Vec::new(), Compressor::Identity, Some(status)),
          };
          ReadBlob {
            digest: Some(digest.into()),
            data,
            status: Some(status.into()),
            compressor: compressor.into(),
          }
        })
        .collect(),
    }
  }
}

#[derive(Message)]
pub struct ServerCapabilities {
  #[prost(message, optional, tag = "1")]
  cache_capabilities: Option<CacheCapabilities>,
  #[prost(message, optional, tag = "4")]
  low_api_version:    Option<SemVer>,
  #[prost(message, optional, tag = "5")]
  high_api_version:   Option<SemVer>,
}

#[derive(Message)]
struct CacheCapabilities {
  #[prost(int32, repeated, tag = "1")]
  digest_functions:                   Vec<i32>,
  #[prost(message, optional, tag = "2")]
  action_cache_update_capabilities:   Option<ActionCacheUpdateCapabilities>,
  #[prost(int64, tag = "4")]
  max_batch_total_size_bytes:         i64,
  #[prost(int32, tag = "5")]
  symlink_absolute_path_strategy:     i32,
  #[prost(int32, repeated, tag = "6")]
  supported_compressors:              Vec<i32>,
  #[prost(int32, repeated, tag = "7")]
  supported_batch_update_compressors: Vec<i32>,
}

#[derive(Message)]
struct ActionCacheUpdateCapabilities {
  #[prost(bool, tag = "1")]
  update_enabled: bool,
}

#[derive(Message)]
struct SemVer {
  #[prost(int32, tag = "1")]
  major: i32,
  #[prost(int32, tag = "2")]
  minor: i32,
}

impl ServerCapabilities {
  /// A cache without remote execution.
  pub fn cache_only(update_enabled: bool, max_batch_total_size: u64) -> Self {
    let version = |minor| Some(SemVer { major: 2, minor });

    Self {
      cache_capabilities: Some(CacheCapabilities {
        digest_functions:                   DigestFunction::ALL
          .into_iter()
          .map(i32::from)
          .collect(),
        action_cache_update_capabilities:   Some(
          ActionCacheUpdateCapabilities { update_enabled },
        ),
        max_batch_total_size_bytes:         wire_size(max_batch_total_size),
        // DISALLOWED
        symlink_absolute_path_strategy:     1,
        supported_compressors:              vec![Compressor::Zstd.into()],
        supported_batch_update_compressors: vec![Compressor::Zstd.into()],
      }),
      low_api_version:    version(0),
      high_api_version:   version(3),
    }
  }
}
