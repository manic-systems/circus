use std::{
  pin::Pin,
  sync::{
    Arc,
    atomic::{AtomicU64, Ordering},
  },
  task::{Context, Poll},
  time::Duration,
};

use async_compression::tokio::bufread::{GzipDecoder, XzDecoder, ZstdDecoder};
use base64::{Engine as _, engine::general_purpose::STANDARD as B64};
use color_eyre::eyre::{Context as _, bail, eyre};
use futures::TryStreamExt as _;
use parking_lot::Mutex;
use sha2::{Digest as _, Sha256};
use tokio::io::{AsyncRead, AsyncReadExt as _, BufReader, ReadBuf};
use tokio_util::io::StreamReader;

#[derive(Debug, Clone)]
pub struct UploadedNar {
  pub nar_hash:  String,
  pub nar_size:  u64,
  pub file_hash: String,
  pub file_size: u64,
}

#[derive(Debug, Clone)]
pub struct VerifyRequest {
  pub get_url:     String,
  pub compression: String,
  pub nar_hash:    String,
  pub nar_size:    u64,
  pub file_hash:   Option<String>,
  pub file_size:   Option<u64>,
}

fn http_client() -> &'static reqwest::Client {
  static CLIENT: std::sync::OnceLock<reqwest::Client> =
    std::sync::OnceLock::new();
  CLIENT.get_or_init(|| {
    reqwest::Client::builder()
      .connect_timeout(Duration::from_secs(10))
      .timeout(Duration::from_secs(30))
      .build()
      .expect("build upload-verify HTTP client")
  })
}

pub async fn verify(req: VerifyRequest) -> color_eyre::Result<UploadedNar> {
  let response = http_client()
    .get(&req.get_url)
    .send()
    .await
    .with_context(|| format!("GET {}", redact_url_query(&req.get_url)))?
    .error_for_status()
    .context("uploaded NAR GET returned error")?;
  let stream = response.bytes_stream();
  let stream = stream.map_err(std::io::Error::other);
  let raw = StreamReader::new(stream);
  let file_hasher = Arc::new(Mutex::new(Sha256::new()));
  let file_counter = Arc::new(AtomicU64::new(0));
  let hashed = HashingReader {
    inner:   Box::pin(raw),
    hasher:  Arc::clone(&file_hasher),
    counter: Arc::clone(&file_counter),
  };
  let buffered = BufReader::new(hashed);

  let mut reader: Pin<Box<dyn AsyncRead + Send>> =
    match req.compression.as_str() {
      "zstd" => Box::pin(ZstdDecoder::new(buffered)),
      "xz" => Box::pin(XzDecoder::new(buffered)),
      "gzip" | "gz" => Box::pin(GzipDecoder::new(buffered)),
      "none" | "" => Box::pin(buffered),
      other => bail!("unsupported upload compression: {other}"),
    };

  let mut nar_hasher = Sha256::new();
  let mut nar_size = 0u64;
  let mut buf = vec![0u8; 128 * 1024];
  loop {
    let n = reader.read(&mut buf).await.context("read uploaded NAR")?;
    if n == 0 {
      break;
    }
    nar_hasher.update(&buf[..n]);
    nar_size = nar_size.saturating_add(n as u64);
    // Abort once the decompressed stream passes the declared size, bounding
    // a malicious or mismatched upload.
    if nar_size > req.nar_size {
      bail!(
        "uploaded NAR exceeds declared size {}: decompressed at least \
         {nar_size} bytes",
        req.nar_size
      );
    }
  }
  drop(reader);

  let computed_nar = nar_hasher.finalize();
  let computed_file = {
    let hasher = Arc::try_unwrap(file_hasher)
      .map_err(|_| eyre!("file hasher still has live readers"))?
      .into_inner();
    hasher.finalize()
  };
  let file_size = file_counter.load(Ordering::Acquire);
  let file_hash = format!("sha256:{}", hex::encode(computed_file));
  if let Some(expected_file_hash) = req.file_hash.as_deref()
    && !hash_matches(expected_file_hash, computed_file.as_slice())?
  {
    bail!(
      "uploaded file hash mismatch: reported {expected_file_hash}, computed \
       {file_hash}"
    );
  }
  if let Some(expected_file_size) = req.file_size
    && expected_file_size != file_size
  {
    bail!(
      "uploaded file size mismatch: reported {expected_file_size}, computed \
       {file_size}"
    );
  }
  if req.nar_size != nar_size {
    bail!(
      "uploaded NAR size mismatch: reported {}, computed {nar_size}",
      req.nar_size
    );
  }
  if !hash_matches(&req.nar_hash, computed_nar.as_slice())? {
    bail!(
      "uploaded NAR hash mismatch: reported {}, computed sha256:{}",
      req.nar_hash,
      hex::encode(computed_nar)
    );
  }

  // Store and sign in Nix sha256 base32, the form a client re-encodes the
  // nar hash to before verifying.
  let nar_hash = format!(
    "sha256:{}",
    circus_nix::base32::encode_sha256(&computed_nar)
  );

  Ok(UploadedNar {
    nar_hash,
    nar_size,
    file_hash,
    file_size,
  })
}

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
    if matches!(&result, Poll::Ready(Ok(()))) {
      let new = &buf.filled()[prev..];
      if !new.is_empty() {
        self.hasher.lock().update(new);
        self.counter.fetch_add(new.len() as u64, Ordering::AcqRel);
      }
    }
    result
  }
}

fn redact_url_query(url: &str) -> String {
  url.find('?').map_or_else(
    || url.to_owned(),
    |pos| format!("{}?<redacted>", &url[..pos]),
  )
}

fn hash_matches(text: &str, computed: &[u8]) -> color_eyre::Result<bool> {
  let expected = parse_sha256_hash(text)?;
  Ok(expected == computed)
}

fn parse_sha256_hash(text: &str) -> color_eyre::Result<Vec<u8>> {
  if let Some(sri) = text.strip_prefix("sha256-") {
    let mut padded = sri.to_owned();
    while padded.len() % 4 != 0 {
      padded.push('=');
    }
    let bytes = B64
      .decode(padded)
      .with_context(|| format!("decode SRI sha256 hash {text}"))?;
    if bytes.len() != 32 {
      bail!(
        "sha256 hash {text} decoded to {} bytes, expected 32",
        bytes.len()
      );
    }
    return Ok(bytes);
  }
  if let Some(hex) = text.strip_prefix("sha256:")
    && hex.len() == 64
    && hex.bytes().all(|b| b.is_ascii_hexdigit())
  {
    return hex::decode(hex).context("decode sha256 hex hash");
  }
  if let Some(nix32) = text.strip_prefix("sha256:") {
    return circus_nix::base32::decode_sha256(nix32)
      .map_err(|e| eyre!("{e}"))
      .with_context(|| format!("decode Nix base32 sha256 hash {text}"));
  }
  bail!("unsupported sha256 hash format: {text}")
}

#[cfg(test)]
mod tests {
  use base64::{Engine as _, engine::general_purpose::STANDARD as B64};

  use super::hash_matches;

  #[test]
  fn hash_matches_sha256_hex() {
    let bytes = [7u8; 32];
    let text = format!("sha256:{}", hex::encode(bytes));
    assert!(hash_matches(&text, &bytes).expect("hex hash should parse"));
    assert!(!hash_matches(&text, &[8u8; 32]).expect("hex hash should parse"));
  }

  #[test]
  fn hash_matches_sri_base64() {
    let bytes = [0u8; 32];
    let text = format!("sha256-{}", B64.encode(bytes));
    assert!(hash_matches(&text, &bytes).expect("SRI hash should parse"));
  }
}
