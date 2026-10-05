//! The cache half of the remote-execution API: `ActionCache`, the
//! `ContentAddressableStorage` batch calls, `ByteStream` and `Capabilities`.

use std::{
  collections::HashMap,
  sync::{Arc, atomic::Ordering::Relaxed},
};

use bytes::Bytes;
use http::{Request, Response};
use hyper::body::{Frame, Incoming};
use prost::Message as _;
use tokio::io::{AsyncReadExt as _, AsyncSeekExt as _, AsyncWriteExt as _};
use zstd::stream::raw::{Decoder, InBuffer, Operation as _, OutBuffer};

use crate::{
  digest::{Digest, Hasher},
  grpc::{
    Body,
    Code,
    MAX_MESSAGE,
    Messages,
    Status,
    end,
    frame,
    reply,
    streaming,
  },
  metrics::Counters,
  proto::{
    ACTION_CACHE,
    ActionResultBlobs,
    BYTESTREAM,
    BatchReadBlobsRequest,
    BatchReadBlobsResponse,
    BatchUpdateBlobsRequest,
    BatchUpdateBlobsResponse,
    BlobUpload,
    CAPABILITIES,
    CAS,
    Compressor,
    FindMissingBlobsRequest,
    FindMissingBlobsResponse,
    GetActionResultRequest,
    GetCapabilitiesRequest,
    QueryWriteStatusRequest,
    QueryWriteStatusResponse,
    ReadRequest,
    ReadResponse,
    ServerCapabilities,
    UpdateActionResultRequest,
    WriteRequest,
    WriteResponse,
    tree_file_digests,
  },
  store::{Store, Upload},
};

const READ_CHUNK: usize = 1 << 20;
/// Batch calls stay well under the 4 MiB message limit most gRPC clients
/// keep by default.
const MAX_BATCH_TOTAL_SIZE: u64 = 2 << 20;

pub struct Instance {
  pub cas:     Store,
  pub ac:      Store,
  pub metrics: Counters,
}

impl Instance {
  /// Whether CAS holds `digest`. The empty blob is always present.
  fn has(&self, digest: &Digest) -> bool {
    digest.is_empty_blob() || self.cas.size(digest.key()) == Some(digest.size)
  }

  async fn read_blob(
    &self,
    digest: &Digest,
  ) -> Result<Option<Vec<u8>>, Status> {
    if digest.is_empty_blob() {
      return Ok(Some(Vec::new()));
    }

    let blob = self.cas.read(digest.key()).await?;
    Ok(blob.filter(|bytes| bytes.len() as u64 == digest.size))
  }

  /// Whether every blob `action_result` names is in CAS, including the files
  /// of each output directory's `Tree`.
  async fn complete(
    &self,
    action: &Digest,
    action_result: &[u8],
  ) -> Result<bool, Status> {
    let named = ActionResultBlobs::decode(action_result, action.function)?;

    if !named.blobs.iter().all(|digest| self.has(digest)) {
      return Ok(false);
    }

    for tree in &named.trees {
      let Some(bytes) = self.read_blob(tree).await? else {
        return Ok(false);
      };

      if !tree_file_digests(&bytes, tree.function)?
        .iter()
        .all(|digest| self.has(digest))
      {
        return Ok(false);
      }
    }

    Ok(true)
  }
}

/// What one listener serves, with its write permission.
pub struct Service {
  pub instances: HashMap<String, Arc<Instance>>,
  pub writable:  bool,
}

impl Service {
  pub async fn handle(&self, request: Request<Incoming>) -> Response<Body> {
    let (parts, body) = request.into_parts();
    let mut messages = Messages::new(body);
    let encoding = parts.headers.get("grpc-encoding");

    let outcome = if encoding.is_some_and(|value| value != "identity") {
      Err(Status::unimplemented("compressed gRPC messages"))
    } else {
      self.dispatch(parts.uri.path(), &mut messages).await
    };

    match outcome {
      Ok(response) => response,
      Err(status) => {
        if status.code() != Code::NotFound {
          tracing::debug!(
            path = parts.uri.path(),
            ?status,
            "remote cache call failed"
          );
        }

        messages.drain().await;
        status.into_response()
      },
    }
  }

  async fn dispatch(
    &self,
    path: &str,
    messages: &mut Messages,
  ) -> Result<Response<Body>, Status> {
    match path.strip_prefix('/').and_then(|path| path.split_once('/')) {
      Some((ACTION_CACHE, "GetActionResult")) => {
        self.get_action_result(messages).await
      },
      Some((ACTION_CACHE, "UpdateActionResult")) => {
        self.update_action_result(messages).await
      },
      Some((BYTESTREAM, "Read")) => self.read(messages).await,
      Some((BYTESTREAM, "Write")) => self.write(messages).await,
      Some((BYTESTREAM, "QueryWriteStatus")) => {
        self.query_write_status(messages).await
      },
      Some((CAS, "FindMissingBlobs")) => {
        self.find_missing_blobs(messages).await
      },
      Some((CAS, "BatchUpdateBlobs")) => {
        self.batch_update_blobs(messages).await
      },
      Some((CAS, "BatchReadBlobs")) => self.batch_read_blobs(messages).await,
      Some((CAPABILITIES, "GetCapabilities")) => {
        self.capabilities(messages).await
      },
      _ => Err(Status::unimplemented(format!("no method {path}"))),
    }
  }

  fn instance(&self, name: &str) -> Result<&Arc<Instance>, Status> {
    self.instances.get(name).ok_or_else(|| {
      Status::invalid_argument(format!("unknown instance {name:?}"))
    })
  }

  async fn deny_writes(&self, messages: &mut Messages) -> Result<(), Status> {
    if self.writable {
      return Ok(());
    }

    messages.drain().await;
    Err(Status::permission_denied("this listener is read-only"))
  }

  async fn get_action_result(
    &self,
    messages: &mut Messages,
  ) -> Result<Response<Body>, Status> {
    let request = GetActionResultRequest::try_from(messages.single().await?)?;
    let instance = self.instance(&request.instance_name)?;

    let Some(entry) = instance.ac.read(request.action.key()).await? else {
      instance.metrics.ac_misses.fetch_add(1, Relaxed);
      return Err(Status::not_found("no action result"));
    };

    // An entry naming an evicted blob, or one unreadable now, is a miss.
    if !instance
      .complete(&request.action, &entry)
      .await
      .unwrap_or(false)
    {
      instance.metrics.ac_misses.fetch_add(1, Relaxed);
      return Err(Status::not_found("action result names a missing blob"));
    }

    instance.metrics.ac_hits.fetch_add(1, Relaxed);
    instance
      .metrics
      .wire_out
      .fetch_add(entry.len() as u64, Relaxed);
    instance
      .metrics
      .data_out
      .fetch_add(entry.len() as u64, Relaxed);

    Ok(reply(&entry))
  }

  async fn update_action_result(
    &self,
    messages: &mut Messages,
  ) -> Result<Response<Body>, Status> {
    self.deny_writes(messages).await?;

    let request =
      UpdateActionResultRequest::try_from(messages.single().await?)?;
    let instance = self.instance(&request.instance_name)?;

    if !instance
      .complete(&request.action, &request.action_result)
      .await?
    {
      return Err(Status::failed_precondition(
        "the action result names blobs missing from CAS",
      ));
    }

    if request.action_result.len() as u64 > instance.ac.budget() {
      return Err(Status::resource_exhausted(
        "action result exceeds the action cache budget",
      ));
    }

    instance
      .ac
      .write(request.action.key(), &request.action_result)
      .await?;

    let size = request.action_result.len() as u64;
    instance.metrics.wire_in.fetch_add(size, Relaxed);
    instance.metrics.data_in.fetch_add(size, Relaxed);

    Ok(reply(&request.action_result))
  }

  async fn read(
    &self,
    messages: &mut Messages,
  ) -> Result<Response<Body>, Status> {
    let request = ReadRequest::try_from(messages.single().await?)?;
    let digest = request.resource.digest;
    let instance = Arc::clone(self.instance(&request.resource.instance)?);

    let mut zstd = match request.resource.compressor {
      Compressor::Identity => None,
      Compressor::Zstd if request.limit != 0 => {
        return Err(Status::invalid_argument(
          "read_limit must be zero for a compressed read",
        ));
      },
      Compressor::Zstd => {
        Some(zstd::bulk::Compressor::new(
          zstd::DEFAULT_COMPRESSION_LEVEL,
        )?)
      },
    };

    if digest.is_empty_blob() {
      let frames = match zstd.as_mut() {
        None => vec![Ok(end(None))],
        Some(zstd) => {
          let data = Bytes::from(zstd.compress(&[])?);
          vec![
            Ok(Frame::data(frame(&ReadResponse { data }.encode_to_vec()))),
            Ok(end(None)),
          ]
        },
      };
      return Ok(streaming(futures::stream::iter(frames)));
    }

    let Some(mut file) = instance.cas.open_object(digest.key()).await? else {
      instance.metrics.cas_misses.fetch_add(1, Relaxed);
      return Err(Status::not_found("no such blob"));
    };

    instance.metrics.cas_hits.fetch_add(1, Relaxed);

    let length = file.metadata().await?.len();
    if length != digest.size {
      return Err(Status::not_found("stored blob has another size"));
    }
    if request.offset > length {
      return Err(Status::invalid_argument("read_offset is past the blob"));
    }

    let available = length - request.offset;
    let mut remaining = if request.limit == 0 {
      available
    } else {
      request.limit.min(available)
    };
    file.seek(std::io::SeekFrom::Start(request.offset)).await?;

    Ok(streaming(async_stream::stream! {
      let mut buffer = vec![0; READ_CHUNK];
      while remaining > 0 {
        let want = usize::try_from(remaining).map_or(READ_CHUNK, |left| left.min(READ_CHUNK));
        match file.read(&mut buffer[..want]).await {
          Ok(0) => {
            yield Ok(end(Some(&Status::internal("blob ended early"))));
            return;
          },
          Ok(read) => {
            remaining -= read as u64;
            // Each chunk is its own zstd frame, which decoders read as one stream.
            let data = match zstd.as_mut().map(|zstd| zstd.compress(&buffer[..read])) {
              None => Bytes::copy_from_slice(&buffer[..read]),
              Some(Ok(compressed)) => Bytes::from(compressed),
              Some(Err(error)) => {
                yield Ok(end(Some(&Status::internal(error))));
                return;
              },
            };
            instance.metrics.data_out.fetch_add(read as u64, Relaxed);
            instance.metrics.wire_out.fetch_add(data.len() as u64, Relaxed);
            yield Ok(Frame::data(frame(&ReadResponse { data }.encode_to_vec())));
          },
          Err(error) => {
            yield Ok(end(Some(&Status::internal(error))));
            return;
          },
        }
      }

      yield Ok(end(None));
    }))
  }

  async fn write(
    &self,
    messages: &mut Messages,
  ) -> Result<Response<Body>, Status> {
    let message = messages
      .next()
      .await?
      .ok_or_else(|| Status::invalid_argument("missing request message"))?;

    self.deny_writes(messages).await?;

    let mut request = WriteRequest::try_from(message)?;

    let resource = request.resource.take().ok_or_else(|| {
      Status::invalid_argument("the first write names no resource")
    })?;

    let digest = resource.0.digest;
    let compressor = resource.0.compressor;
    let instance = self.instance(&resource.0.instance)?;

    if digest.size > instance.cas.budget() {
      messages.drain().await;
      return Err(Status::resource_exhausted("blob exceeds the CAS budget"));
    }

    if instance.has(&digest) {
      messages.drain().await;
      let response = match compressor {
        Compressor::Identity => WriteResponse::committed(digest.size),
        Compressor::Zstd => WriteResponse::already_present(),
      };
      return Ok(reply(&response.encode_to_vec()));
    }

    let mut sink = Sink {
      upload:  instance.cas.upload().await?,
      hasher:  digest.function.hasher(),
      written: 0,
      limit:   digest.size,
    };
    let mut decoder = match compressor {
      Compressor::Identity => None,
      Compressor::Zstd => Some((Decoder::new()?, vec![0; READ_CHUNK])),
    };
    let mut received = 0_u64;

    loop {
      if request.resource.is_some_and(|named| named != resource) {
        return Err(Status::invalid_argument(
          "resource_name changed mid-write",
        ));
      }

      // A compressed write's offsets count the compressed bytes sent.
      if request.offset != received {
        return Err(Status::invalid_argument(format!(
          "write_offset {} where {received} was expected",
          request.offset
        )));
      }

      received += request.data.len() as u64;

      match decoder.as_mut() {
        None => sink.put(&request.data).await?,
        Some((decoder, decoded)) => {
          let mut input = InBuffer::around(&request.data);
          loop {
            let mut output = OutBuffer::around(&mut decoded[..]);
            decoder.run(&mut input, &mut output).map_err(|error| {
              Status::invalid_argument(format!("corrupt zstd upload: {error}"))
            })?;

            let produced = output.pos();
            sink.put(&decoded[..produced]).await?;

            if input.pos() == input.src.len() && produced < decoded.len() {
              break;
            }
          }
        },
      }

      if request.finish {
        break;
      }

      let message = messages.next().await?.ok_or_else(|| {
        Status::invalid_argument("upload ended without finish_write")
      })?;
      request = WriteRequest::try_from(message)?;
    }

    messages.drain().await;

    if sink.written != digest.size || sink.hasher.finish() != digest.hash {
      return Err(Status::invalid_argument("upload does not match its digest"));
    }

    instance
      .cas
      .commit(sink.upload, digest.key(), digest.size)
      .await?;

    instance.metrics.wire_in.fetch_add(received, Relaxed);
    instance.metrics.data_in.fetch_add(digest.size, Relaxed);

    Ok(reply(&WriteResponse::committed(received).encode_to_vec()))
  }

  async fn query_write_status(
    &self,
    messages: &mut Messages,
  ) -> Result<Response<Body>, Status> {
    let request = QueryWriteStatusRequest::try_from(messages.single().await?)?;
    let resource = request.resource.0;

    if !self.instance(&resource.instance)?.has(&resource.digest) {
      return Err(Status::not_found("no such upload"));
    }

    let response = QueryWriteStatusResponse::complete(resource.digest.size);
    Ok(reply(&response.encode_to_vec()))
  }

  async fn find_missing_blobs(
    &self,
    messages: &mut Messages,
  ) -> Result<Response<Body>, Status> {
    let request = FindMissingBlobsRequest::try_from(messages.single().await?)?;
    let instance = self.instance(&request.instance_name)?;

    let response = request
      .digests
      .into_iter()
      .filter(|digest| {
        let present = instance.has(digest);

        if present {
          instance.cas.touch(digest.key());
          instance.metrics.cas_hits.fetch_add(1, Relaxed);
        } else {
          instance.metrics.cas_misses.fetch_add(1, Relaxed);
        }

        !present
      })
      .collect::<FindMissingBlobsResponse>();

    Ok(reply(&response.encode_to_vec()))
  }

  async fn batch_update_blobs(
    &self,
    messages: &mut Messages,
  ) -> Result<Response<Body>, Status> {
    self.deny_writes(messages).await?;

    let request = BatchUpdateBlobsRequest::try_from(messages.single().await?)?;
    let instance = self.instance(&request.instance_name)?;

    let mut results = Vec::with_capacity(request.blobs.len());

    for BlobUpload {
      digest,
      data,
      compressor,
    } in request.blobs
    {
      instance
        .metrics
        .wire_in
        .fetch_add(data.len() as u64, Relaxed);

      let status = match decompress(data, compressor, digest.size) {
        Err(status) => Some(status),
        Ok(data)
          if data.len() as u64 != digest.size
            || digest.function.hash(&data) != digest.hash =>
        {
          Some(Status::invalid_argument("blob does not match its digest"))
        },
        Ok(_) if digest.size > instance.cas.budget() => {
          Some(Status::resource_exhausted("blob exceeds the CAS budget"))
        },
        Ok(_) if instance.has(&digest) => None,
        Ok(data) => {
          instance.metrics.data_in.fetch_add(digest.size, Relaxed);
          instance
            .cas
            .write(digest.key(), &data)
            .await
            .err()
            .map(Status::from)
        },
      };

      results.push((digest, status));
    }

    let response = results.into_iter().collect::<BatchUpdateBlobsResponse>();
    Ok(reply(&response.encode_to_vec()))
  }

  async fn batch_read_blobs(
    &self,
    messages: &mut Messages,
  ) -> Result<Response<Body>, Status> {
    let request = BatchReadBlobsRequest::try_from(messages.single().await?)?;
    let instance = self.instance(&request.instance_name)?;

    let total = request
      .digests
      .iter()
      .map(|digest| digest.size)
      .sum::<u64>();

    if total > MAX_BATCH_TOTAL_SIZE {
      return Err(Status::invalid_argument(format!(
        "batch of {total} bytes exceeds max_batch_total_size_bytes"
      )));
    }

    let mut results = Vec::with_capacity(request.digests.len());

    for digest in request.digests {
      let result = instance
        .read_blob(&digest)
        .await
        .and_then(|blob| blob.ok_or_else(|| Status::not_found("no such blob")))
        .and_then(|blob| {
          match request.compressor {
            Compressor::Identity => Ok((blob, Compressor::Identity)),
            Compressor::Zstd => {
              let compressed =
                zstd::bulk::compress(&blob, zstd::DEFAULT_COMPRESSION_LEVEL)?;
              Ok((compressed, Compressor::Zstd))
            },
          }
        });

      match &result {
        Ok((data, _)) => {
          instance.metrics.cas_hits.fetch_add(1, Relaxed);
          instance.metrics.data_out.fetch_add(digest.size, Relaxed);
          instance
            .metrics
            .wire_out
            .fetch_add(data.len() as u64, Relaxed);
        },
        Err(_) => {
          instance.metrics.cas_misses.fetch_add(1, Relaxed);
        },
      }

      results.push((digest, result));
    }

    let response = results.into_iter().collect::<BatchReadBlobsResponse>();
    Ok(reply(&response.encode_to_vec()))
  }

  async fn capabilities(
    &self,
    messages: &mut Messages,
  ) -> Result<Response<Body>, Status> {
    let request = GetCapabilitiesRequest::decode(messages.single().await?)?;
    self.instance(&request.instance_name)?;

    let capabilities =
      ServerCapabilities::cache_only(self.writable, MAX_BATCH_TOTAL_SIZE);

    Ok(reply(&capabilities.encode_to_vec()))
  }
}

/// Where an upload's bytes go once decompressed, refusing more than the
/// digest names so a small compressed upload cannot fill the disk.
struct Sink {
  upload:  Upload,
  hasher:  Hasher,
  written: u64,
  limit:   u64,
}

impl Sink {
  async fn put(&mut self, data: &[u8]) -> Result<(), Status> {
    self.written += data.len() as u64;
    if self.written > self.limit {
      return Err(Status::invalid_argument("upload is larger than its digest"));
    }

    self.upload.file.write_all(data).await?;
    self.hasher.update(data);
    Ok(())
  }
}

/// A batch blob's data, decompressed to at most `size` bytes.
fn decompress(
  data: Bytes,
  compressor: Compressor,
  size: u64,
) -> Result<Bytes, Status> {
  match compressor {
    Compressor::Identity => Ok(data),
    Compressor::Zstd => {
      let capacity = usize::try_from(size)
        .ok()
        .filter(|capacity| *capacity <= MAX_MESSAGE)
        .ok_or_else(|| Status::invalid_argument("batch blob is too large"))?;
      zstd::bulk::decompress(&data, capacity)
        .map(Bytes::from)
        .map_err(|error| {
          Status::invalid_argument(format!("corrupt zstd blob: {error}"))
        })
    },
  }
}
