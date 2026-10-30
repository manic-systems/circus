//! Repository-scoped installation tokens minted from a GitHub App for effects.

use std::time::{SystemTime, UNIX_EPOCH};

use circus_config::GithubAppConfig;
use circus_http::ResponseExt as _;
use data_encoding::{BASE64, BASE64URL_NOPAD};
use misstep::{Result, ResultExt as _, bail, report};
use ring::{
  rand::SystemRandom,
  signature::{RSA_PKCS1_SHA256, RsaKeyPair},
};
use serde::Deserialize;

pub struct GithubApp {
  app_id:  u64,
  key:     RsaKeyPair,
  api_url: String,
  http:    reqwest::Client,
}

#[derive(Deserialize)]
struct Installation {
  id: u64,
}

#[derive(Deserialize)]
struct AccessToken {
  token: String,
}

impl GithubApp {
  /// # Errors
  ///
  /// Returns an error when the private key is missing or not an RSA key.
  pub fn new(cfg: &GithubAppConfig) -> Result<Self> {
    let pem = cfg.private_key.as_deref().ok_or_else(|| {
      report!("github_app needs private_key or private_key_file")
    })?;
    let der = BASE64
      .decode(
        pem
          .lines()
          .filter(|line| !line.starts_with("-----"))
          .collect::<String>()
          .as_bytes(),
      )
      .context("github_app private key is not PEM")?;
    // GitHub hands out PKCS#1 keys, `openssl pkcs8` converts them to PKCS#8.
    let key = RsaKeyPair::from_der(&der)
      .or_else(|_| RsaKeyPair::from_pkcs8(&der))
      .map_err(|e| report!("github_app private key is not RSA: {e}"))?;
    Ok(Self {
      app_id: cfg.app_id,
      key,
      api_url: cfg.api_url.trim_end_matches('/').to_owned(),
      http: circus_http::api().build()?,
    })
  }

  fn jwt(&self) -> Result<String> {
    let now = SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs();
    // GitHub rejects tokens issued in its future, so backdate for clock skew.
    let header = BASE64URL_NOPAD.encode(br#"{"alg":"RS256","typ":"JWT"}"#);
    let claims = BASE64URL_NOPAD.encode(
      serde_json::json!({
        "iat": now - 60,
        "exp": now + 540,
        "iss": self.app_id.to_string(),
      })
      .to_string()
      .as_bytes(),
    );
    let message = format!("{header}.{claims}");
    let mut signature = vec![0; self.key.public().modulus_len()];
    self
      .key
      .sign(
        &RSA_PKCS1_SHA256,
        &SystemRandom::new(),
        message.as_bytes(),
        &mut signature,
      )
      .map_err(|e| report!("sign GitHub App JWT: {e}"))?;
    Ok(format!("{message}.{}", BASE64URL_NOPAD.encode(&signature)))
  }

  /// Mint an installation token with write access to owner and repo.
  ///
  /// # Errors
  ///
  /// Returns an error when the app is not installed on the repository or
  /// GitHub refuses the request.
  pub async fn repository_token(
    &self,
    owner: &str,
    repo: &str,
  ) -> Result<String> {
    let jwt = self.jwt()?;
    let installation = self
      .http
      .get(format!(
        "{}/repos/{owner}/{repo}/installation",
        self.api_url
      ))
      .bearer_auth(&jwt)
      .header("Accept", "application/vnd.github+json")
      .header("X-GitHub-Api-Version", circus_http::GITHUB_API_VERSION)
      .send()
      .await?;
    if !installation.status().is_success() {
      bail!(
        "GitHub App is not installed on {owner}/{repo}: {}",
        installation.status()
      );
    }
    let installation = installation
      .json_capped::<Installation>(circus_http::JSON_LIMIT)
      .await?;
    let token = self
      .http
      .post(format!(
        "{}/app/installations/{}/access_tokens",
        self.api_url, installation.id
      ))
      .bearer_auth(&jwt)
      .header("Accept", "application/vnd.github+json")
      .header("X-GitHub-Api-Version", circus_http::GITHUB_API_VERSION)
      .json(&serde_json::json!({
        "repositories": [repo],
        "permissions": { "contents": "write" },
      }))
      .send()
      .await?;
    if !token.status().is_success() {
      bail!(
        "GitHub refused a token for {owner}/{repo}: {}",
        token.status()
      );
    }
    Ok(
      token
        .json_capped::<AccessToken>(circus_http::JSON_LIMIT)
        .await?
        .token,
    )
  }
}
