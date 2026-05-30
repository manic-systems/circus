//! Presigned-upload flow on the agent side.
//!
//! Triggered after a successful build when the runner's
//! `BuildAssignment.presignedUpload` is set. For each output:
//!
//! 1. Query `nix path-info --json` for narHash, narSize, references, deriver,
//!    and CA.
//! 2. Ask the runner for a presigned PUT URL via `Runner.requestPresignedUrls`.
//! 3. Stream `nix-store --dump` through an in-process compressor (zstd, xz, or
//!    gzip from `async-compression`) directly into the PUT body while a
//!    `HashingReader` tracks the on-the-wire SHA-256 and length. The NAR is
//!    never buffered in agent memory.
//! 4. Tell the runner via `Runner.notifyUploadComplete` with the final NarInfo
//!    so it can persist + sign.
//!
//! Failure of any single output is reported into `errorMessage` of the
//! BuildResult; the build itself is not retroactively failed, matching
//! Hydra's behaviour.

use std::{
  pin::Pin,
  process::Stdio,
  sync::{
    Arc,
    atomic::{AtomicU64, Ordering},
  },
  task::{Context, Poll},
  time::Instant,
};

use anyhow::Context as _;
use async_compression::{
  Level,
  tokio::bufread::{GzipEncoder, XzEncoder, ZstdEncoder},
};
use circus_proto::{nar_info, runner};
use parking_lot::Mutex;
use sha2::{Digest as _, Sha256};
use tokio::{
  io::{AsyncRead, BufReader, ReadBuf},
  process::Command,
};
use tokio_util::io::ReaderStream;

use crate::build::ResolvedOutput;

/// Per-output narinfo discovered via `nix path-info --json`.
#[derive(Debug, Clone)]
pub struct PathMetadata {
  pub store_path: String,
  pub nar_hash:   String,
  pub nar_size:   u64,
  pub references: Vec<String>,
  pub deriver:    Option<String>,
  pub ca:         Option<String>,
}

/// Outcome of one round of uploads.
pub struct UploadStats {
  pub successes:  Vec<String>,
  pub failures:   Vec<(String, String)>,
  pub elapsed_ms: u64,
}

/// Run the full upload flow for every output. Returns when each path
/// has been recorded either as a success or a failure.
pub async fn upload_all(
  runner_cap: &runner::Client,
  machine_id: &str,
  build_id: &str,
  compression: &str,
  outputs: &[ResolvedOutput],
) -> anyhow::Result<UploadStats> {
  let started = Instant::now();
  let mut successes = Vec::new();
  let mut failures = Vec::new();

  // path-info for each output. A miss drops the path from the upload
  // set with a recorded reason.
  let mut metadata: Vec<PathMetadata> = Vec::with_capacity(outputs.len());
  for o in outputs {
    match query_path_info(&o.path).await {
      Ok(m) => metadata.push(m),
      Err(e) => failures.push((o.path.clone(), format!("path-info: {e}"))),
    }
  }
  if metadata.is_empty() {
    return Ok(UploadStats {
      successes,
      failures,
      elapsed_ms: started.elapsed().as_millis() as u64,
    });
  }

  // Batch-request presigned URLs (one Runner RPC, N entries).
  let presigned = match request_presigned(
    runner_cap, machine_id, build_id, &metadata,
  )
  .await
  {
    Ok(v) => v,
    Err(e) => {
      for m in &metadata {
        failures.push((m.store_path.clone(), format!("presign: {e}")));
      }
      return Ok(UploadStats {
        successes,
        failures,
        elapsed_ms: started.elapsed().as_millis() as u64,
      });
    },
  };

  let http = reqwest::Client::builder()
    .pool_max_idle_per_host(2)
    .build()
    .context("build reqwest client")?;

  for (meta, slot) in metadata.iter().zip(presigned.iter()) {
    if !slot.error.is_empty() {
      failures.push((meta.store_path.clone(), slot.error.clone()));
      continue;
    }
    match upload_one(&http, meta, slot, compression).await {
      Ok(uploaded) => {
        if let Err(e) = notify_complete(
          runner_cap,
          machine_id,
          build_id,
          meta,
          slot,
          &uploaded,
          compression,
        )
        .await
        {
          failures.push((meta.store_path.clone(), format!("notify: {e}")));
        } else {
          successes.push(meta.store_path.clone());
        }
      },
      Err(e) => {
        failures.push((meta.store_path.clone(), format!("upload: {e}")));
      },
    }
  }

  Ok(UploadStats {
    successes,
    failures,
    elapsed_ms: started.elapsed().as_millis() as u64,
  })
}

#[derive(Debug, Clone)]
struct PresignSlot {
  nar_url:  String,
  nar_path: String,
  error:    String,
}

async fn request_presigned(
  runner_cap: &runner::Client,
  machine_id: &str,
  build_id: &str,
  metadata: &[PathMetadata],
) -> Result<Vec<PresignSlot>, capnp::Error> {
  let mut req = runner_cap.request_presigned_urls_request();
  {
    let mut p = req.get();
    p.set_machine_id(machine_id);
    p.set_build_id(build_id);
    let mut list = p.init_request(metadata.len() as u32);
    for (i, m) in metadata.iter().enumerate() {
      let mut slot = list.reborrow().get(i as u32);
      slot.set_store_path(m.store_path.as_str());
      slot.set_nar_hash(m.nar_hash.as_str());
      slot.set_nar_size(m.nar_size);
    }
  }
  let resp = req.send().promise.await?;
  let inner = resp.get()?.get_responses()?;
  if inner.len() != metadata.len() as u32 {
    return Err(capnp::Error::failed(format!(
      "presign response length mismatch: expected {}, got {}",
      metadata.len(),
      inner.len()
    )));
  }
  let mut out = Vec::with_capacity(inner.len() as usize);
  for entry in inner.iter() {
    out.push(PresignSlot {
      nar_url:  entry.get_nar_url()?.to_str()?.to_owned(),
      nar_path: entry.get_nar_path()?.to_str()?.to_owned(),
      error:    entry.get_error_message()?.to_str()?.to_owned(),
    });
  }
  Ok(out)
}

/// Tally of compressed bytes pushed up the wire.
struct UploadedBytes {
  file_hash: String, // `sha256:<hex>`
  file_size: u64,
}

async fn upload_one(
  http: &reqwest::Client,
  meta: &PathMetadata,
  slot: &PresignSlot,
  compression: &str,
) -> anyhow::Result<UploadedBytes> {
  // Spawn `nix-store --dump <path>` and keep its stdout as a streaming
  // AsyncRead. The PUT body reads from it on demand, so the NAR is
  // never materialised in agent memory.
  let mut dump = Command::new("nix-store")
    .arg("--dump")
    .arg(&meta.store_path)
    .stdout(Stdio::piped())
    .kill_on_drop(true)
    .spawn()
    .with_context(|| format!("spawn nix-store --dump {}", meta.store_path))?;
  let stdout = dump
    .stdout
    .take()
    .ok_or_else(|| anyhow::anyhow!("dump stdout missing"))?;
  let buffered = BufReader::new(stdout);

  let encoded: Pin<Box<dyn AsyncRead + Send>> = match compression {
    "zstd" => Box::pin(ZstdEncoder::with_quality(buffered, Level::Precise(19))),
    "xz" => Box::pin(XzEncoder::with_quality(buffered, Level::Precise(6))),
    "gzip" => Box::pin(GzipEncoder::new(buffered)),
    "none" | "" => Box::pin(buffered),
    other => return Err(anyhow::anyhow!("unsupported compression: {other}")),
  };

  // Tee the compressed bytes through a hasher + counter while reqwest
  // pulls them. parking_lot::Mutex gives us sync access from poll_read
  // without an await point.
  let hasher = Arc::new(Mutex::new(Sha256::new()));
  let counter = Arc::new(AtomicU64::new(0));
  let reader = HashingReader {
    inner:   encoded,
    hasher:  Arc::clone(&hasher),
    counter: Arc::clone(&counter),
  };
  let stream = ReaderStream::with_capacity(reader, 64 * 1024);
  let body = reqwest::Body::wrap_stream(stream);

  let resp = http
    .put(&slot.nar_url)
    .body(body)
    .send()
    .await
    .with_context(|| format!("PUT {}", slot.nar_url))?;
  let status = resp.status();
  if !status.is_success() {
    let text = resp.text().await.unwrap_or_default();
    return Err(anyhow::anyhow!("S3 PUT returned {status}: {text}"));
  }

  // Drain the child. nix-store --dump on success returns 0 after EOF.
  let child_status = dump.wait().await?;
  if !child_status.success() {
    return Err(anyhow::anyhow!(
      "nix-store --dump exited with {child_status}"
    ));
  }

  // Finalise the rolling hash. `Mutex::into_inner` only works on the
  // unwrapped value; we take(...) to move it out from behind the Arc.
  let final_hash = {
    let inner = Arc::try_unwrap(hasher)
      .map_err(|_| anyhow::anyhow!("hasher Arc still has live readers"))?
      .into_inner();
    inner.finalize()
  };
  Ok(UploadedBytes {
    file_hash: format!("sha256:{}", hex::encode(final_hash)),
    file_size: counter.load(Ordering::Acquire),
  })
}

/// AsyncRead adapter that updates a SHA-256 hasher and a byte counter
/// on every successful read. No buffering; data flows straight through.
struct HashingReader {
  inner:   Pin<Box<dyn AsyncRead + Send>>,
  hasher:  Arc<Mutex<Sha256>>,
  counter: Arc<AtomicU64>,
}

impl AsyncRead for HashingReader {
  fn poll_read(
    mut self: Pin<&mut Self>,
    cx: &mut Context<'_>,
    buf: &mut ReadBuf<'_>,
  ) -> Poll<std::io::Result<()>> {
    let prev = buf.filled().len();
    let result = self.inner.as_mut().poll_read(cx, buf);
    if let Poll::Ready(Ok(())) = &result {
      let new = &buf.filled()[prev..];
      if !new.is_empty() {
        self.hasher.lock().update(new);
        self.counter.fetch_add(new.len() as u64, Ordering::AcqRel);
      }
    }
    result
  }
}

async fn query_path_info(store_path: &str) -> anyhow::Result<PathMetadata> {
  let out = Command::new("nix")
    .args(["path-info", "--json", "--closure-size", store_path])
    .output()
    .await
    .context("nix path-info")?;
  if !out.status.success() {
    return Err(anyhow::anyhow!("nix path-info exited with {}", out.status));
  }
  let v: serde_json::Value =
    serde_json::from_slice(&out.stdout).context("parse path-info json")?;

  // Nix 2.x emits `{path: {narHash, narSize, ...}}` for one path and a
  // top-level array for closure queries. Normalise to one object.
  let obj = match &v {
    serde_json::Value::Object(o) => {
      o.values()
        .next()
        .and_then(|v| v.as_object())
        .ok_or_else(|| anyhow::anyhow!("empty path-info object"))?
    },
    serde_json::Value::Array(arr) => {
      arr
        .first()
        .and_then(|v| v.as_object())
        .ok_or_else(|| anyhow::anyhow!("empty path-info array"))?
    },
    _ => return Err(anyhow::anyhow!("unexpected path-info shape")),
  };

  let nar_hash = obj
    .get("narHash")
    .and_then(|v| v.as_str())
    .ok_or_else(|| anyhow::anyhow!("missing narHash"))?
    .to_owned();
  let nar_size = obj.get("narSize").and_then(|v| v.as_u64()).unwrap_or(0);
  let references = obj
    .get("references")
    .and_then(|v| v.as_array())
    .map(|arr| {
      arr
        .iter()
        .filter_map(|v| v.as_str().map(str::to_owned))
        .collect()
    })
    .unwrap_or_default();
  let deriver = obj
    .get("deriver")
    .and_then(|v| v.as_str())
    .map(str::to_owned);
  let ca = obj.get("ca").and_then(|v| v.as_str()).map(str::to_owned);

  Ok(PathMetadata {
    store_path: store_path.to_owned(),
    nar_hash,
    nar_size,
    references,
    deriver,
    ca,
  })
}

async fn notify_complete(
  runner_cap: &runner::Client,
  machine_id: &str,
  build_id: &str,
  meta: &PathMetadata,
  slot: &PresignSlot,
  uploaded: &UploadedBytes,
  compression: &str,
) -> Result<(), capnp::Error> {
  let mut req = runner_cap.notify_upload_complete_request();
  {
    let mut p = req.get();
    p.set_machine_id(machine_id);
    p.set_build_id(build_id);
    let mut ni: nar_info::Builder<'_> = p.init_nar_info();
    ni.set_store_path(meta.store_path.as_str());
    ni.set_nar_hash(meta.nar_hash.as_str());
    ni.set_nar_size(meta.nar_size);
    ni.set_file_hash(uploaded.file_hash.as_str());
    ni.set_file_size(uploaded.file_size);
    ni.set_compression(compression);
    ni.set_url(slot.nar_path.as_str());
    ni.set_deriver(meta.deriver.as_deref().unwrap_or(""));
    ni.set_ca(meta.ca.as_deref().unwrap_or(""));
    ni.set_sig("");
    let mut refs = ni.init_references(meta.references.len() as u32);
    for (i, r) in meta.references.iter().enumerate() {
      refs.set(i as u32, r.as_str());
    }
  }
  req.send().promise.await?;
  Ok(())
}
