//! OIDC ID-token verification for agent registration.
//!
//! An agent may present an OIDC JWT (e.g. a GitHub Actions ID token) in
//! `register` instead of a bearer token. We verify the RS256 signature
//! against the issuer's JWKS, check `iss`/`aud`/`exp`, and gate on an
//! `owner/repo` allowlist. Keys are cached per `kid` and refreshed on a TTL
//! or on an unknown `kid` (issuers rotate signing keys).

use std::{collections::HashMap, time::Duration};

use circus_common::config::RpcOidcConfig;
use color_eyre::eyre::{Context as _, bail, eyre};
use jsonwebtoken::{Algorithm, DecodingKey, Validation, decode, decode_header};
use serde::Deserialize;
use tokio::{sync::Mutex, time::Instant};

/// Refresh cached JWKS at least this often even if every `kid` still resolves.
const JWKS_TTL: Duration = Duration::from_hours(1);

/// Identity extracted from a verified ID token. Phase-4 ref gating consumes
/// the subject/ref; registration only needs `repository`.
#[derive(Debug, Clone)]
pub struct VerifiedIdentity {
  pub repository:   String,
  pub subject:      String,
  pub workflow_ref: Option<String>,
  pub ref_name:     Option<String>,
}

#[derive(Deserialize)]
struct GitHubClaims {
  repository:   String,
  #[serde(default)]
  sub:          String,
  #[serde(default)]
  workflow_ref: Option<String>,
  #[serde(default, rename = "ref")]
  ref_name:     Option<String>,
}

#[derive(Deserialize)]
struct OidcDiscovery {
  jwks_uri: String,
}

#[derive(Deserialize)]
struct JwkSet {
  keys: Vec<Jwk>,
}

#[derive(Deserialize, Clone)]
struct Jwk {
  kid: String,
  #[serde(default)]
  kty: String,
  n:   String,
  e:   String,
}

#[derive(Default)]
struct CacheState {
  keys:              HashMap<String, Jwk>,
  fetched_at:        Option<Instant>,
  resolved_jwks_uri: Option<String>,
}

pub struct OidcVerifier {
  http:                     reqwest::Client,
  issuer:                   String,
  jwks_url:                 Option<String>,
  audiences:                Vec<String>,
  allowed_repositories:     Vec<String>,
  allowed_subjects:         Vec<String>,
  allowed_subject_prefixes: Vec<String>,
  allowed_workflow_refs:    Vec<String>,
  allowed_refs:             Vec<String>,
  cache:                    Mutex<CacheState>,
}

impl OidcVerifier {
  /// # Errors
  ///
  /// Returns an error if the HTTP client cannot be built.
  pub fn new(cfg: &RpcOidcConfig) -> color_eyre::Result<Self> {
    let http = reqwest::Client::builder()
      .connect_timeout(Duration::from_secs(10))
      .timeout(Duration::from_secs(20))
      .build()
      .context("build OIDC http client")?;
    Ok(Self {
      http,
      issuer: cfg.issuer.trim_end_matches('/').to_owned(),
      jwks_url: cfg.jwks_url.clone(),
      audiences: cfg.audiences.clone(),
      allowed_repositories: cfg.allowed_repositories.clone(),
      allowed_subjects: cfg.allowed_subjects.clone(),
      allowed_subject_prefixes: cfg.allowed_subject_prefixes.clone(),
      allowed_workflow_refs: cfg.allowed_workflow_refs.clone(),
      allowed_refs: cfg.allowed_refs.clone(),
      cache: Mutex::new(CacheState::default()),
    })
  }

  /// Verify a presented ID token end to end.
  ///
  /// # Errors
  ///
  /// Returns an error when the token is malformed, the signature or claims
  /// do not validate, or the repository is not on the allowlist. The caller
  /// logs the detail and returns a generic failure to the agent.
  pub async fn verify(
    &self,
    token: &str,
  ) -> color_eyre::Result<VerifiedIdentity> {
    let header = decode_header(token).context("decode JWT header")?;
    if header.alg != Algorithm::RS256 {
      bail!("unexpected JWT alg {:?}, want RS256", header.alg);
    }
    let kid = header.kid.ok_or_else(|| eyre!("JWT has no kid"))?;
    let jwk = self.jwk_for_kid(&kid).await?;
    let key = DecodingKey::from_rsa_components(&jwk.n, &jwk.e)
      .context("build decoding key from JWK")?;

    let mut validation = Validation::new(Algorithm::RS256);
    validation.set_issuer(&[self.issuer.as_str()]);
    validation.set_audience(&self.audiences);
    validation.set_required_spec_claims(&["exp", "iss", "aud", "sub"]);

    let data =
      decode::<GitHubClaims>(token, &key, &validation).context("verify JWT")?;
    let claims = data.claims;

    if !self
      .allowed_repositories
      .iter()
      .any(|r| r == &claims.repository)
    {
      bail!("repository {} is not allowed", claims.repository);
    }
    if !claim_allowed(
      &claims.sub,
      &self.allowed_subjects,
      &self.allowed_subject_prefixes,
    ) {
      bail!("subject {} is not allowed", claims.sub);
    }
    if !optional_claim_allowed(
      claims.workflow_ref.as_deref(),
      &self.allowed_workflow_refs,
    ) {
      bail!(
        "workflow_ref {} is not allowed",
        claims.workflow_ref.as_deref().unwrap_or("<missing>")
      );
    }
    if !optional_claim_allowed(claims.ref_name.as_deref(), &self.allowed_refs) {
      bail!(
        "ref {} is not allowed",
        claims.ref_name.as_deref().unwrap_or("<missing>")
      );
    }

    Ok(VerifiedIdentity {
      repository:   claims.repository,
      subject:      claims.sub,
      workflow_ref: claims.workflow_ref,
      ref_name:     claims.ref_name,
    })
  }

  async fn jwk_for_kid(&self, kid: &str) -> color_eyre::Result<Jwk> {
    {
      let cache = self.cache.lock().await;
      if let Some(jwk) = cache.keys.get(kid)
        && !is_stale(cache.fetched_at)
      {
        return Ok(jwk.clone());
      }
    }
    self.refresh().await?;
    let cache = self.cache.lock().await;
    cache
      .keys
      .get(kid)
      .cloned()
      .ok_or_else(|| eyre!("no JWKS key for kid {kid}"))
  }

  async fn refresh(&self) -> color_eyre::Result<()> {
    let uri = self.resolve_jwks_uri().await?;
    let set: JwkSet = self
      .http
      .get(&uri)
      .send()
      .await
      .context("fetch JWKS")?
      .error_for_status()
      .context("JWKS endpoint returned error")?
      .json()
      .await
      .context("parse JWKS")?;
    let keys = set
      .keys
      .into_iter()
      .filter(|k| k.kty.is_empty() || k.kty == "RSA")
      .map(|k| (k.kid.clone(), k))
      .collect();
    let mut cache = self.cache.lock().await;
    cache.keys = keys;
    cache.fetched_at = Some(Instant::now());
    drop(cache);
    Ok(())
  }

  async fn resolve_jwks_uri(&self) -> color_eyre::Result<String> {
    if let Some(uri) = &self.jwks_url {
      return Ok(uri.clone());
    }
    if let Some(uri) = &self.cache.lock().await.resolved_jwks_uri {
      return Ok(uri.clone());
    }
    let url = format!("{}/.well-known/openid-configuration", self.issuer);
    let disc: OidcDiscovery = self
      .http
      .get(&url)
      .send()
      .await
      .context("fetch OIDC discovery")?
      .error_for_status()
      .context("OIDC discovery returned error")?
      .json()
      .await
      .context("parse OIDC discovery")?;
    self.cache.lock().await.resolved_jwks_uri = Some(disc.jwks_uri.clone());
    Ok(disc.jwks_uri)
  }
}

fn is_stale(fetched_at: Option<Instant>) -> bool {
  fetched_at.is_none_or(|t| t.elapsed() >= JWKS_TTL)
}

fn claim_allowed(value: &str, exact: &[String], prefixes: &[String]) -> bool {
  (exact.is_empty() && prefixes.is_empty())
    || exact.iter().any(|allowed| allowed == value)
    || prefixes
      .iter()
      .any(|prefix| value.starts_with(prefix.as_str()))
}

fn optional_claim_allowed(value: Option<&str>, exact: &[String]) -> bool {
  exact.is_empty()
    || value.is_some_and(|value| exact.iter().any(|v| v == value))
}

#[cfg(test)]
mod tests {
  use super::{claim_allowed, optional_claim_allowed};

  #[test]
  fn empty_claim_policy_allows_any_value() {
    assert!(claim_allowed(
      "repo:owner/repo:ref:refs/heads/main",
      &[],
      &[]
    ));
    assert!(optional_claim_allowed(None, &[]));
  }

  #[test]
  fn exact_and_prefix_claim_policies_are_enforced() {
    assert!(claim_allowed(
      "repo:owner/repo:ref:refs/heads/main",
      &["repo:owner/repo:ref:refs/heads/main".into()],
      &[],
    ));
    assert!(claim_allowed(
      "repo:owner/repo:ref:refs/heads/main",
      &[],
      &["repo:owner/repo:ref:refs/heads/".into()],
    ));
    assert!(!claim_allowed("repo:owner/repo:pull_request", &[], &[
      "repo:owner/repo:ref:refs/heads/".into()
    ],));
    assert!(optional_claim_allowed(
      Some("owner/repo/.github/workflows/circus-builder.yml@refs/heads/main"),
      &[
        "owner/repo/.github/workflows/circus-builder.yml@refs/heads/main"
          .into()
      ],
    ));
    assert!(!optional_claim_allowed(None, &["required".into()]));
  }
}
