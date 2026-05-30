//! Minimal AWS SigV4 presigned PUT URL generator for S3 / S3-compatible
//! endpoints (e.g. MinIO).
//!
//! We do NOT pull in `aws-sdk-s3`: the surface we need is one API call
//! (presign a PUT for NAR upload) and we already have `hmac`, `sha2`,
//! and `hex` in the workspace. The full SDK pulls ~70 transitive crates
//! we'd otherwise be free of.
//!
//! Reference: AWS Signature Version 4, query-string presigning.
//! <https://docs.aws.amazon.com/AmazonS3/latest/API/sigv4-query-string-auth.html>

use std::time::{Duration, SystemTime, UNIX_EPOCH};

use circus_common::config::S3CacheConfig;
use hmac::{Hmac, KeyInit as _, Mac};
use sha2::{Digest as _, Sha256};

type HmacSha256 = Hmac<Sha256>;

/// Credentials lifted from `S3CacheConfig`. The config layer is the
/// authoritative source: rotation belongs in whatever provisions
/// `fc.toml` (systemd `LoadCredential`, Vault, sealed-secrets, etc.) so
/// the runner only ever sees an explicit access key + secret pair and
/// does not have to reason about credential expiry.
#[derive(Clone, Debug)]
pub struct Credentials {
  pub access_key:    String,
  pub secret_key:    String,
  pub session_token: Option<String>,
}

/// All the bits we need to produce a presigned URL. Stays alive on the
/// runner alongside the `AgentPool`; one instance per S3 bucket the
/// operator wants to push to.
#[derive(Clone, Debug)]
pub struct Presigner {
  pub credentials:    Credentials,
  pub region:         String,
  pub bucket:         String,
  pub endpoint_url:   Option<String>, // e.g. https://minio.internal:9000
  pub use_path_style: bool,
}

impl Presigner {
  /// Build a `Presigner` from the user-facing `S3CacheConfig` plus a
  /// parsed `store_uri` of the form `s3://bucket`. Returns `None` when
  /// the URI is not S3-shaped or credentials are missing.
  #[must_use]
  pub fn from_config(store_uri: &str, cfg: &S3CacheConfig) -> Option<Self> {
    let bucket = store_uri.strip_prefix("s3://")?.trim_end_matches('/');
    if bucket.is_empty() {
      return None;
    }
    let access_key = cfg.access_key_id.clone()?;
    let secret_key = cfg.secret_access_key.clone()?;
    Some(Self {
      credentials:    Credentials {
        access_key,
        secret_key,
        session_token: cfg.session_token.clone(),
      },
      region:         cfg.region.clone().unwrap_or_else(|| "us-east-1".into()),
      bucket:         bucket.to_owned(),
      endpoint_url:   cfg.endpoint_url.clone(),
      use_path_style: cfg.use_path_style,
    })
  }

  /// Presign a PUT URL for `key` (a path inside the bucket, no leading
  /// slash), valid for `expiry`, signed against the wall clock. Use
  /// [`Presigner::presign_at`] when a deterministic timestamp is needed
  /// (tests, signed-time-window negotiation).
  pub fn presign_put(&self, key: &str, expiry: Duration) -> String {
    self.presign_at("PUT", key, expiry, SystemTime::now())
  }

  /// Presign for an arbitrary HTTP method and pinned timestamp. Public
  /// so the test suite (and any cross-region time-budgeting code) can
  /// produce deterministic signatures without resorting to env-var
  /// backdoors.
  pub fn presign_at(
    &self,
    method: &str,
    key: &str,
    expiry: Duration,
    now: SystemTime,
  ) -> String {
    let (host, base_url) = self.host_and_base(key);
    let datetime = format_iso8601(now);
    let date = &datetime[..8];
    let credential_scope = format!("{date}/{}/s3/aws4_request", self.region);
    let credential =
      format!("{}/{credential_scope}", self.credentials.access_key);

    let expiry_secs = expiry.as_secs().clamp(1, 7 * 24 * 60 * 60);

    let mut query: Vec<(String, String)> = vec![
      ("X-Amz-Algorithm".into(), "AWS4-HMAC-SHA256".into()),
      ("X-Amz-Credential".into(), credential),
      ("X-Amz-Date".into(), datetime.clone()),
      ("X-Amz-Expires".into(), expiry_secs.to_string()),
      ("X-Amz-SignedHeaders".into(), "host".into()),
    ];
    if let Some(tok) = &self.credentials.session_token {
      query.push(("X-Amz-Security-Token".into(), tok.clone()));
    }
    query.sort_by(|a, b| a.0.cmp(&b.0));

    let canonical_query = query
      .iter()
      .map(|(k, v)| format!("{}={}", uri_encode(k, true), uri_encode(v, true)))
      .collect::<Vec<_>>()
      .join("&");
    let canonical_uri =
      canonical_path(key, self.use_path_style.then(|| &self.bucket));
    let canonical_headers = format!("host:{host}\n");
    let signed_headers = "host";
    let payload_hash = "UNSIGNED-PAYLOAD";

    let canonical_request = [
      method,
      canonical_uri.as_str(),
      canonical_query.as_str(),
      canonical_headers.as_str(),
      signed_headers,
      payload_hash,
    ]
    .join("\n");
    let canonical_hash =
      hex::encode(Sha256::digest(canonical_request.as_bytes()));

    let string_to_sign = format!(
      "AWS4-HMAC-SHA256\n{datetime}\n{credential_scope}\n{canonical_hash}"
    );

    let signing_key = derive_signing_key(
      &self.credentials.secret_key,
      date,
      &self.region,
      "s3",
    );
    let signature =
      hex::encode(hmac_sha256(&signing_key, string_to_sign.as_bytes()));

    format!("{base_url}?{canonical_query}&X-Amz-Signature={signature}")
  }

  /// `(host, base_url_without_query)`. The returned `host` is what goes
  /// into the canonical `host` header during signing; for virtual-hosted
  /// style that's `{bucket}.{endpoint_host}`, not the bare endpoint.
  /// Path-style URLs put the bucket in the path; virtual-hosted style puts
  /// it in the subdomain.
  fn host_and_base(&self, key: &str) -> (String, String) {
    let key = key.trim_start_matches('/');
    if let Some(endpoint) = &self.endpoint_url {
      let endpoint = endpoint.trim_end_matches('/');
      let endpoint_host = endpoint
        .trim_start_matches("https://")
        .trim_start_matches("http://")
        .split('/')
        .next()
        .unwrap_or(endpoint)
        .to_owned();
      if self.use_path_style {
        let base = format!("{endpoint}/{}/{key}", self.bucket);
        (endpoint_host, base)
      } else {
        let scheme = if endpoint.starts_with("http://") {
          "http"
        } else {
          "https"
        };
        let host = format!("{}.{endpoint_host}", self.bucket);
        let base = format!("{scheme}://{host}/{key}");
        (host, base)
      }
    } else if self.use_path_style {
      let host = format!("s3.{}.amazonaws.com", self.region);
      let base = format!("https://{host}/{}/{key}", self.bucket);
      (host, base)
    } else {
      let host = format!("{}.s3.{}.amazonaws.com", self.bucket, self.region);
      let base = format!("https://{host}/{key}");
      (host, base)
    }
  }
}

fn format_iso8601(t: SystemTime) -> String {
  // YYYYMMDDTHHMMSSZ
  let secs = t.duration_since(UNIX_EPOCH).unwrap_or_default().as_secs();
  let (year, month, day, hour, minute, second) = unix_to_civil(secs);
  format!("{year:04}{month:02}{day:02}T{hour:02}{minute:02}{second:02}Z")
}

/// Howard Hinnant's `civil_from_days`; converts epoch seconds to
/// `(Y,M,D,h,m,s)` without pulling in `chrono` for one call.
fn unix_to_civil(secs: u64) -> (u32, u32, u32, u32, u32, u32) {
  let days = (secs / 86400) as i64;
  let secs_of_day = (secs % 86400) as u32;
  let hour = secs_of_day / 3600;
  let minute = (secs_of_day % 3600) / 60;
  let second = secs_of_day % 60;

  let z = days + 719_468;
  let era = z.div_euclid(146_097);
  let doe = (z - era * 146_097) as u64;
  let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
  let y = yoe as i64 + era * 400;
  let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
  let mp = (5 * doy + 2) / 153;
  let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
  let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
  let y = if m <= 2 { y + 1 } else { y };
  (y as u32, m, d, hour, minute, second)
}

fn canonical_path(key: &str, path_style_bucket: Option<&String>) -> String {
  let key = key.trim_start_matches('/');
  let segments: Vec<String> =
    key.split('/').map(|s| uri_encode(s, false)).collect();
  match path_style_bucket {
    Some(b) => format!("/{}/{}", uri_encode(b, false), segments.join("/")),
    None => format!("/{}", segments.join("/")),
  }
}

/// AWS-flavoured URI encoder. Unreserved characters per RFC 3986
/// `A-Z a-z 0-9 - _ . ~` plus `/` when `encode_slash = false`.
fn uri_encode(s: &str, encode_slash: bool) -> String {
  let mut out = String::with_capacity(s.len());
  for b in s.bytes() {
    let unreserved =
      b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.' | b'~');
    if unreserved || (!encode_slash && b == b'/') {
      out.push(b as char);
    } else {
      out.push_str(&format!("%{b:02X}"));
    }
  }
  out
}

fn derive_signing_key(
  secret: &str,
  date: &str,
  region: &str,
  service: &str,
) -> Vec<u8> {
  let k_date = hmac_sha256(format!("AWS4{secret}").as_bytes(), date.as_bytes());
  let k_region = hmac_sha256(&k_date, region.as_bytes());
  let k_service = hmac_sha256(&k_region, service.as_bytes());
  hmac_sha256(&k_service, b"aws4_request")
}

fn hmac_sha256(key: &[u8], data: &[u8]) -> Vec<u8> {
  let mut mac =
    HmacSha256::new_from_slice(key).expect("HMAC accepts any key length");
  mac.update(data);
  mac.finalize().into_bytes().to_vec()
}

#[cfg(test)]
mod tests {
  use super::*;

  /// Locked-in AWS reference vector ensures we are byte-for-byte
  /// compatible with what S3 actually accepts.
  ///
  /// Drawn from AWS doc example "Example: Signing the request as a
  /// presigned URL":
  /// <https://docs.aws.amazon.com/AmazonS3/latest/API/sigv4-query-string-auth-example.html>
  ///
  /// Inputs:
  /// - access AKIAIOSFODNN7EXAMPLE
  /// - secret wJalrXUtnFEMI/K7MDENG/bPxRfiCYEXAMPLEKEY
  /// - region us-east-1
  /// - bucket examplebucket, key test.txt
  /// - time 20130524T000000Z, expires 86400
  /// - virtual-hosted style
  /// Expected signature:
  /// aeeed9bbccd4d02ee5c0109b86d86835f995330da4c265957d157751f604d404
  #[test]
  fn matches_aws_reference_get_vector() {
    let presigner = Presigner {
      credentials:    Credentials {
        access_key:    "AKIAIOSFODNN7EXAMPLE".into(),
        secret_key:    "wJalrXUtnFEMI/K7MDENG/bPxRfiCYEXAMPLEKEY".into(),
        session_token: None,
      },
      region:         "us-east-1".into(),
      bucket:         "examplebucket".into(),
      // The AWS doc reference vector was computed when bucket URLs
      // were `<bucket>.s3.amazonaws.com` (legacy global endpoint),
      // not the regional `s3.us-east-1.amazonaws.com`. Force that
      // host by overriding the endpoint.
      endpoint_url:   Some("https://s3.amazonaws.com".into()),
      use_path_style: false,
    };
    let pinned = UNIX_EPOCH + Duration::from_secs(1_369_353_600);
    let url = presigner.presign_at(
      "GET",
      "test.txt",
      Duration::from_secs(86_400),
      pinned,
    );
    assert!(
      url.contains("X-Amz-Signature=aeeed9bbccd4d02ee5c0109b86d86835f995330da4c265957d157751f604d404"),
      "URL did not contain expected signature: {url}"
    );
  }
}
