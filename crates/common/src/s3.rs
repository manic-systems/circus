//! Minimal AWS `SigV4` presigning helpers for S3-compatible binary cache
//! stores.
//!
//! Circus uses this in two places:
//!
//! - queue-runner RPC presigned PUTs for agents uploading NARs,
//! - server-side presigned GET redirects for private S3-backed NAR downloads.
//!
//! Keeping both flows here guarantees they agree on bucket parsing, optional
//! prefixes, endpoint style, and canonical signing.

use std::time::{Duration, SystemTime};

use chrono::{DateTime, Utc};
use hmac::{Hmac, KeyInit as _, Mac};
use percent_encoding::{AsciiSet, NON_ALPHANUMERIC, utf8_percent_encode};
use sha2::{Digest as _, Sha256};
use url::Url;

use crate::config::S3CacheConfig;

type HmacSha256 = Hmac<Sha256>;
const AWS_QUERY_ENCODE_SET: AsciiSet = NON_ALPHANUMERIC
  .remove(b'-')
  .remove(b'_')
  .remove(b'.')
  .remove(b'~');

#[derive(Clone)]
pub struct Credentials {
  pub access_key:    String,
  pub secret_key:    String,
  pub session_token: Option<String>,
}

impl std::fmt::Debug for Credentials {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    f.debug_struct("Credentials")
      .field("access_key", &self.access_key)
      .field("secret_key", &"[REDACTED]")
      .field(
        "session_token",
        &self.session_token.as_ref().map(|_| "[REDACTED]"),
      )
      .finish()
  }
}

/// S3-compatible presigner for one bucket and optional key prefix.
#[derive(Clone, Debug)]
pub struct Presigner {
  pub credentials:    Credentials,
  pub region:         String,
  pub bucket:         String,
  pub prefix:         Option<String>,
  pub endpoint_url:   Option<String>,
  pub use_path_style: bool,
}

impl Presigner {
  /// Build a presigner from `s3://bucket[/prefix]` plus `[cache_upload.s3]`.
  ///
  /// # Returns
  ///
  /// Returns `None` for non-S3 URIs or when explicit credentials are missing.
  /// This helper intentionally does not discover IAM role credentials;
  /// operators should provision the access key/secret through their secret
  /// manager.
  #[must_use]
  pub fn from_config(store_uri: &str, cfg: &S3CacheConfig) -> Option<Self> {
    let target = parse_s3_store_uri(store_uri)?;
    let access_key = cfg.access_key_id.clone()?;
    let secret_key = cfg.secret_access_key.clone()?;
    Some(Self {
      credentials:    Credentials {
        access_key,
        secret_key,
        session_token: cfg.session_token.clone(),
      },
      region:         cfg.region.clone().unwrap_or_else(|| "us-east-1".into()),
      bucket:         target.bucket,
      prefix:         combine_prefixes(
        target.prefix.as_deref(),
        cfg.prefix.as_deref(),
      ),
      endpoint_url:   cfg.endpoint_url.clone(),
      use_path_style: cfg.use_path_style,
    })
  }

  #[must_use]
  pub fn presign_put(&self, key: &str, expiry: Duration) -> String {
    self.presign_at("PUT", key, expiry, SystemTime::now())
  }

  #[must_use]
  pub fn presign_get(&self, key: &str, expiry: Duration) -> String {
    self.presign_at("GET", key, expiry, SystemTime::now())
  }

  #[must_use]
  pub fn object_key(&self, key: &str) -> String {
    let key = key.trim_start_matches('/');
    self
      .prefix
      .as_ref()
      .map_or_else(|| key.to_owned(), |prefix| format!("{prefix}/{key}"))
  }

  #[must_use]
  pub fn presign_at(
    &self,
    method: &str,
    key: &str,
    expiry: Duration,
    now: SystemTime,
  ) -> String {
    let object_key = self.object_key(key);
    let (host, base_url) = self.host_and_base(&object_key);
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
      .map(|(k, v)| format!("{}={}", aws_query_encode(k), aws_query_encode(v)))
      .collect::<Vec<_>>()
      .join("&");
    let canonical_uri =
      canonical_path(&object_key, self.use_path_style.then_some(&self.bucket));
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

  #[expect(
    clippy::option_if_let_else,
    reason = "the if-let/else structure is clearer than a combinator chain \
              for this control flow"
  )]
  fn host_and_base(&self, key: &str) -> (String, String) {
    let key = key.trim_start_matches('/');
    let encoded_key = encoded_key_path(key);
    if let Some(endpoint) = &self.endpoint_url {
      let endpoint_url = Url::parse(endpoint).ok();
      let endpoint_host = endpoint_url
        .as_ref()
        .map_or_else(|| endpoint.to_owned(), endpoint_authority);
      if self.use_path_style {
        let mut base = endpoint.trim_end_matches('/').to_owned();
        base.push('/');
        base.push_str(&self.bucket);
        if !key.is_empty() {
          base.push('/');
          base.push_str(&encoded_key);
        }
        (endpoint_host, base)
      } else {
        let scheme = endpoint_url.as_ref().map_or("https", Url::scheme);
        let host = format!("{}.{endpoint_host}", self.bucket);
        let mut base = format!("{scheme}://{host}");
        if !key.is_empty() {
          base.push('/');
          base.push_str(&encoded_key);
        }
        (host, base)
      }
    } else if self.use_path_style {
      let host = format!("s3.{}.amazonaws.com", self.region);
      let base = format!("https://{host}/{}/{encoded_key}", self.bucket);
      (host, base)
    } else {
      let host = format!("{}.s3.{}.amazonaws.com", self.bucket, self.region);
      let base = format!("https://{host}/{encoded_key}");
      (host, base)
    }
  }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct S3StoreTarget {
  bucket: String,
  prefix: Option<String>,
}

fn parse_s3_store_uri(store_uri: &str) -> Option<S3StoreTarget> {
  let rest = store_uri.strip_prefix("s3://")?.trim_end_matches('/');
  if rest.is_empty() {
    return None;
  }
  let (bucket, prefix) =
    rest.split_once('/').map_or((rest, None), |(bucket, p)| {
      (bucket, normalize_prefix(Some(p)))
    });
  if bucket.is_empty() {
    return None;
  }
  Some(S3StoreTarget {
    bucket: bucket.to_owned(),
    prefix,
  })
}

#[must_use]
pub fn s3_store_uri_with_prefix(
  store_uri: &str,
  cfg: Option<&S3CacheConfig>,
) -> String {
  let Some(cfg) = cfg else {
    return store_uri.to_owned();
  };
  let Some(target) = parse_s3_store_uri(store_uri) else {
    return store_uri.to_owned();
  };
  let prefix =
    combine_prefixes(target.prefix.as_deref(), cfg.prefix.as_deref());
  prefix.map_or_else(
    || format!("s3://{}", target.bucket),
    |prefix| format!("s3://{}/{prefix}", target.bucket),
  )
}

fn combine_prefixes(a: Option<&str>, b: Option<&str>) -> Option<String> {
  match (normalize_prefix(a), normalize_prefix(b)) {
    (Some(a), Some(b)) => Some(format!("{a}/{b}")),
    (Some(a), None) => Some(a),
    (None, Some(b)) => Some(b),
    (None, None) => None,
  }
}

fn normalize_prefix(prefix: Option<&str>) -> Option<String> {
  prefix
    .map(|p| p.trim_matches('/'))
    .filter(|p| !p.is_empty())
    .map(ToOwned::to_owned)
}

fn format_iso8601(t: SystemTime) -> String {
  let ts: DateTime<Utc> = DateTime::<Utc>::from(t);
  ts.format("%Y%m%dT%H%M%SZ").to_string()
}

fn canonical_path(key: &str, path_style_bucket: Option<&String>) -> String {
  let key = key.trim_start_matches('/');
  let segments: Vec<String> = key.split('/').map(aws_path_encode).collect();
  path_style_bucket.map_or_else(
    || format!("/{}", segments.join("/")),
    |b| format!("/{}/{}", aws_path_encode(b), segments.join("/")),
  )
}

fn aws_query_encode(s: &str) -> String {
  utf8_percent_encode(s, &AWS_QUERY_ENCODE_SET).to_string()
}

fn aws_path_encode(s: &str) -> String {
  utf8_percent_encode(s, &AWS_QUERY_ENCODE_SET).to_string()
}

fn encoded_key_path(key: &str) -> String {
  key
    .split('/')
    .map(aws_path_encode)
    .collect::<Vec<_>>()
    .join("/")
}

fn endpoint_authority(url: &Url) -> String {
  let host = url.host_str().unwrap_or_default();
  url
    .port()
    .map_or_else(|| host.to_owned(), |port| format!("{host}:{port}"))
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
  #[expect(
    clippy::expect_used,
    reason = "HMAC::new_from_slice only fails if key length is wrong; signing \
              key is always valid"
  )]
  {
    let mut mac =
      HmacSha256::new_from_slice(key).expect("HMAC accepts any key length");
    mac.update(data);
    mac.finalize().into_bytes().to_vec()
  }
}

#[cfg(test)]
mod tests {
  use std::time::{Duration, UNIX_EPOCH};

  use super::*;

  #[test]
  fn parses_bucket_and_prefix_from_store_uri_and_config() {
    let cfg = S3CacheConfig {
      prefix: Some("cache".into()),
      access_key_id: Some("AKIA".into()),
      secret_access_key: Some("secret".into()),
      ..S3CacheConfig::default()
    };
    let presigner = Presigner::from_config("s3://bucket/root", &cfg);
    assert_eq!(
      presigner.as_ref().map(|p| p.bucket.as_str()),
      Some("bucket")
    );
    assert_eq!(
      presigner.as_ref().and_then(|p| p.prefix.as_deref()),
      Some("root/cache")
    );
    assert_eq!(
      presigner.as_ref().map(|p| p.object_key("nar/x.nar.zst")),
      Some("root/cache/nar/x.nar.zst".to_owned())
    );
    assert_eq!(
      s3_store_uri_with_prefix("s3://bucket/root", Some(&cfg)),
      "s3://bucket/root/cache"
    );
  }

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
      prefix:         None,
      endpoint_url:   Some("https://s3.amazonaws.com".into()),
      use_path_style: false,
    };
    #[expect(
      clippy::duration_suboptimal_units,
      reason = "pinned timestamp for AWS reference vector"
    )]
    let pinned = UNIX_EPOCH + Duration::from_secs(1_369_353_600);
    let url = presigner.presign_at(
      "GET",
      "test.txt",
      #[expect(
        clippy::duration_suboptimal_units,
        reason = "pinned expiry for AWS reference vector"
      )]
      Duration::from_secs(86_400),
      pinned,
    );
    assert!(
      url.contains("X-Amz-Signature=aeeed9bbccd4d02ee5c0109b86d86835f995330da4c265957d157751f604d404"),
      "URL did not contain expected signature: {url}"
    );
  }

  #[test]
  fn includes_endpoint_port_in_signed_host() {
    let presigner = Presigner {
      credentials:    Credentials {
        access_key:    "AKIA".into(),
        secret_key:    "secret".into(),
        session_token: None,
      },
      region:         "us-east-1".into(),
      bucket:         "bucket".into(),
      prefix:         None,
      endpoint_url:   Some("https://minio.example.com:9000".into()),
      use_path_style: true,
    };
    let url = presigner.presign_at(
      "GET",
      "nar/example.nar.zst",
      Duration::from_mins(1),
      UNIX_EPOCH,
    );

    assert!(url.starts_with(
      "https://minio.example.com:9000/bucket/nar/example.nar.zst?"
    ));
    assert!(url.contains("X-Amz-Signature="));
  }
}
