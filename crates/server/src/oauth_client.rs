//! OAuth 2.0 authorization-code client shared by GitHub sign-in and OIDC.

use std::fmt;

use data_encoding::BASE64URL_NOPAD;
use reqwest::{
  Client,
  RequestBuilder,
  StatusCode,
  header::{ACCEPT, CONTENT_TYPE},
};
use ring::digest::{SHA256, digest};
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use url::{Url, form_urlencoded};

/// An unguessable value such as a `state`, nonce, or PKCE verifier.
#[derive(Clone, Deserialize, Serialize)]
#[serde(transparent)]
pub struct Secret(String);

impl Secret {
  /// 32 random bytes, which also satisfies the 43 character minimum RFC 7636
  /// sets for PKCE verifiers.
  pub fn random() -> Result<Self, getrandom::Error> {
    let mut bytes = [0u8; 32];
    getrandom::fill(&mut bytes)?;
    Ok(Self(BASE64URL_NOPAD.encode(&bytes)))
  }

  pub fn as_str(&self) -> &str {
    &self.0
  }

  pub fn pkce_challenge(&self) -> String {
    BASE64URL_NOPAD.encode(digest(&SHA256, self.0.as_bytes()).as_ref())
  }
}

impl fmt::Debug for Secret {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    f.write_str("Secret(..)")
  }
}

/// How the client authenticates to the token endpoint.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ClientAuth {
  Basic,
  RequestBody,
}

#[derive(Debug, thiserror::Error)]
pub enum TokenError {
  #[error("token request failed")]
  Request(#[source] reqwest::Error),
  #[error("token endpoint rejected the code with {error}")]
  Rejected {
    error:       String,
    description: Option<String>,
  },
  #[error("token endpoint answered {0}")]
  Status(StatusCode),
  #[error("token endpoint sent an unreadable response")]
  Malformed(#[source] serde_json::Error),
}

#[derive(Deserialize)]
struct ErrorResponse {
  error:             String,
  error_description: Option<String>,
}

pub struct AuthCodeClient {
  pub client_id:     String,
  pub client_secret: Option<String>,
  pub auth_url:      Url,
  pub token_url:     Url,
  pub redirect_uri:  String,
  pub auth:          ClientAuth,
}

impl AuthCodeClient {
  pub fn authorize_url<'scope>(
    &self,
    state: &Secret,
    scopes: impl IntoIterator<Item = &'scope str>,
    extra: &[(&str, &str)],
  ) -> Url {
    let scope = scopes.into_iter().collect::<Vec<_>>().join(" ");
    let mut url = self.auth_url.clone();
    let mut query = url.query_pairs_mut();

    query
      .append_pair("response_type", "code")
      .append_pair("client_id", &self.client_id)
      .append_pair("redirect_uri", &self.redirect_uri)
      .append_pair("state", state.as_str());

    if !scope.is_empty() {
      query.append_pair("scope", &scope);
    }

    query.extend_pairs(extra);
    drop(query);
    url
  }

  /// Redeems `code`. GitHub reports a bad code as a 200 with an `error` body,
  /// so an `error` field only counts when no token parses.
  pub async fn exchange<T: DeserializeOwned>(
    &self,
    http: &Client,
    code: &str,
    pkce_verifier: Option<&Secret>,
  ) -> Result<T, TokenError> {
    let response = self
      .token_request(http, code, pkce_verifier)
      .send()
      .await
      .map_err(TokenError::Request)?;
    let status = response.status();
    let body = response.bytes().await.map_err(TokenError::Request)?;
    let token = status
      .is_success()
      .then(|| serde_json::from_slice::<T>(&body));

    if let Some(Ok(token)) = token {
      return Ok(token);
    }

    if let Ok(error) = serde_json::from_slice::<ErrorResponse>(&body) {
      return Err(TokenError::Rejected {
        error:       error.error,
        description: error.error_description,
      });
    }

    match token {
      Some(Err(error)) => Err(TokenError::Malformed(error)),
      _ => Err(TokenError::Status(status)),
    }
  }

  fn token_request(
    &self,
    http: &Client,
    code: &str,
    pkce_verifier: Option<&Secret>,
  ) -> RequestBuilder {
    let mut form = form_urlencoded::Serializer::new(String::new());
    form
      .append_pair("grant_type", "authorization_code")
      .append_pair("code", code)
      .append_pair("redirect_uri", &self.redirect_uri);

    if let Some(verifier) = pkce_verifier {
      form.append_pair("code_verifier", verifier.as_str());
    }

    let mut request = http
      .post(self.token_url.clone())
      .header(ACCEPT, "application/json")
      .header(CONTENT_TYPE, "application/x-www-form-urlencoded");

    match (&self.client_secret, self.auth) {
      (Some(secret), ClientAuth::Basic) => {
        let encode = |value: &str| {
          form_urlencoded::byte_serialize(value.as_bytes()).collect::<String>()
        };
        request =
          request.basic_auth(encode(&self.client_id), Some(encode(secret)));
      },
      (Some(secret), ClientAuth::RequestBody) => {
        form
          .append_pair("client_id", &self.client_id)
          .append_pair("client_secret", secret);
      },
      (None, _) => {
        form.append_pair("client_id", &self.client_id);
      },
    }

    request.body(form.finish())
  }
}
