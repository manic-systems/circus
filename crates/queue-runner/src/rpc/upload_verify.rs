use std::{
  collections::HashSet,
  pin::Pin,
  sync::{
    Arc,
    atomic::{AtomicU64, Ordering},
  },
  task::{Context, Poll},
  time::Duration,
};

use async_compression::tokio::bufread::{GzipDecoder, XzDecoder, ZstdDecoder};
use circus_binary_cache::{archive::NarEvent, parse_nar};
use color_eyre::eyre::{Context as _, bail, eyre};
use data_encoding::{BASE64, HEXLOWER, HEXLOWER_PERMISSIVE};
use futures::{StreamExt as _, TryStreamExt as _};
use parking_lot::Mutex;
use sha2::{Digest as _, Sha256};
use tokio::io::{AsyncRead, BufReader, ReadBuf};
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
  pub references:  Vec<String>,
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

  let reader: Pin<Box<dyn AsyncRead + Send>> = match req.compression.as_str() {
    "zstd" => Box::pin(ZstdDecoder::new(buffered)),
    "xz" => Box::pin(XzDecoder::new(buffered)),
    "gzip" | "gz" => Box::pin(GzipDecoder::new(buffered)),
    "none" | "" => Box::pin(buffered),
    other => bail!("unsupported upload compression: {other}"),
  };

  let mut tap = NarTap {
    inner:   reader,
    hasher:  Sha256::new(),
    size:    0,
    limit:   req.nar_size,
    scanner: RefScanner::new(&req.references)?,
  };
  {
    let mut events = std::pin::pin!(parse_nar(&mut tap));
    while let Some(event) = events.next().await {
      if let NarEvent::File { mut reader, .. } =
        event.context("parse uploaded NAR")?
      {
        tokio::io::copy(&mut reader, &mut tokio::io::sink())
          .await
          .context("read uploaded NAR")?;
      }
    }
  }
  let trailing = tokio::io::copy(&mut tap, &mut tokio::io::sink())
    .await
    .context("read uploaded NAR")?;
  if trailing != 0 {
    bail!("uploaded NAR has {trailing} bytes of trailing data");
  }
  let NarTap {
    inner,
    hasher,
    size: nar_size,
    scanner,
    ..
  } = tap;
  drop(inner);
  let missing = scanner.missing();
  if !missing.is_empty() {
    bail!(
      "uploaded NAR does not mention its declared references: {}",
      missing.join(", ")
    );
  }

  let computed_nar = hasher.finalize();
  let computed_file = {
    let hasher = Arc::try_unwrap(file_hasher)
      .map_err(|_| eyre!("file hasher still has live readers"))?
      .into_inner();
    hasher.finalize()
  };
  let file_size = file_counter.load(Ordering::Acquire);
  let file_hash = format!("sha256:{}", HEXLOWER.encode(&computed_file));
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
      HEXLOWER.encode(&computed_nar)
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

struct NarTap {
  inner:   Pin<Box<dyn AsyncRead + Send>>,
  hasher:  Sha256,
  size:    u64,
  limit:   u64,
  scanner: RefScanner,
}

impl AsyncRead for NarTap {
  fn poll_read(
    self: Pin<&mut Self>,
    cx: &mut Context<'_>,
    buf: &mut ReadBuf<'_>,
  ) -> Poll<std::io::Result<()>> {
    let this = self.get_mut();
    let prev = buf.filled().len();
    std::task::ready!(this.inner.as_mut().poll_read(cx, buf))?;
    let new = &buf.filled()[prev..];
    this.size = this.size.saturating_add(new.len() as u64);
    // Abort once the decompressed stream passes the declared size, bounding
    // a malicious or mismatched upload.
    if this.size > this.limit {
      return Poll::Ready(Err(std::io::Error::new(
        std::io::ErrorKind::InvalidData,
        format!(
          "uploaded NAR exceeds declared size {}: decompressed at least {} \
           bytes",
          this.limit, this.size
        ),
      )));
    }
    this.hasher.update(new);
    this.scanner.feed(new);
    Poll::Ready(Ok(()))
  }
}

const HASH_PART_LEN: usize = 32;

/// Mirrors Nix's reference scanner.
struct RefScanner {
  wanted: HashSet<[u8; HASH_PART_LEN]>,
  found:  HashSet<[u8; HASH_PART_LEN]>,
  tail:   Vec<u8>,
}

impl RefScanner {
  fn new(references: &[String]) -> color_eyre::Result<Self> {
    let wanted = references
      .iter()
      .map(|reference| {
        let name = reference.rsplit('/').next().unwrap_or_default();
        name
          .as_bytes()
          .get(..HASH_PART_LEN)
          .filter(|hash| hash.iter().copied().all(is_nix32))
          .and_then(|hash| hash.try_into().ok())
          .ok_or_else(|| eyre!("reference {reference} has no store path hash"))
      })
      .collect::<color_eyre::Result<_>>()?;
    Ok(Self {
      wanted,
      found: HashSet::new(),
      tail: Vec::with_capacity(2 * HASH_PART_LEN),
    })
  }

  fn feed(&mut self, data: &[u8]) {
    if self.found.len() == self.wanted.len() {
      return;
    }
    // Windows that straddle the previous chunk live only in the seam.
    let mut seam = std::mem::take(&mut self.tail);
    seam.extend_from_slice(&data[..data.len().min(HASH_PART_LEN - 1)]);
    self.scan(&seam);
    self.scan(data);

    let keep = HASH_PART_LEN - 1;
    if data.len() >= keep {
      seam.clear();
      seam.extend_from_slice(&data[data.len() - keep..]);
    } else {
      seam.drain(..seam.len().saturating_sub(keep));
    }
    self.tail = seam;
  }

  fn scan(&mut self, data: &[u8]) {
    let mut start = 0;
    while let Some(window) = data.get(start..start + HASH_PART_LEN) {
      if let Some(bad) = window.iter().rposition(|byte| !is_nix32(*byte)) {
        start += bad + 1;
        continue;
      }
      if let Ok(hash) = <[u8; HASH_PART_LEN]>::try_from(window)
        && self.wanted.contains(&hash)
      {
        self.found.insert(hash);
      }
      start += 1;
    }
  }

  fn missing(&self) -> Vec<String> {
    self
      .wanted
      .difference(&self.found)
      .map(|hash| String::from_utf8_lossy(hash).into_owned())
      .collect()
  }
}

const fn is_nix32(byte: u8) -> bool {
  matches!(byte, b'0'..=b'9' | b'a'..=b'd' | b'f'..=b'n' | b'p'..=b's' | b'v'..=b'z')
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
    let bytes = BASE64
      .decode(padded.as_bytes())
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
    return HEXLOWER_PERMISSIVE
      .decode(hex.as_bytes())
      .context("decode sha256 hex hash");
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
  use data_encoding::{BASE64, HEXLOWER};

  use super::hash_matches;

  #[test]
  fn hash_matches_sha256_hex() {
    let bytes = [7u8; 32];
    let text = format!("sha256:{}", HEXLOWER.encode(&bytes));
    assert!(hash_matches(&text, &bytes).expect("hex hash should parse"));
    assert!(!hash_matches(&text, &[8u8; 32]).expect("hex hash should parse"));
  }

  #[test]
  fn hash_matches_sri_base64() {
    let bytes = [0u8; 32];
    let text = format!("sha256-{}", BASE64.encode(&bytes));
    assert!(hash_matches(&text, &bytes).expect("SRI hash should parse"));
  }
}
