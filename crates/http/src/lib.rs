//! Outbound HTTP clients shared by every Circus component.

pub mod guard;

use std::{future::Future, time::Duration};

use reqwest::{
  Client,
  ClientBuilder,
  RequestBuilder,
  Response,
  StatusCode,
  Url,
  redirect::Policy,
};
use serde::de::DeserializeOwned;

pub const USER_AGENT: &str = concat!("circus/", env!("CARGO_PKG_VERSION"));

/// The REST API version every GitHub call pins.
pub const GITHUB_API_VERSION: &str = "2026-03-10";

/// How much of a failed response's body goes into [`Error::Status`].
pub const ERROR_BODY_LIMIT: usize = 4 * 1024;

/// Default cap for JSON API responses.
pub const JSON_LIMIT: usize = 1024 * 1024;

const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);
const API_TIMEOUT: Duration = Duration::from_secs(30);
const MAX_REDIRECTS: usize = 5;

/// A client for API calls, bounded end to end.
pub fn api() -> ClientBuilder {
  base().timeout(API_TIMEOUT)
}

/// A client for bodies of any size, with no overall timeout. reqwest's
/// `read_timeout` also runs while the request body uploads, so only add it
/// for downloads.
pub fn transfer() -> ClientBuilder {
  base()
}

/// Refuses internal addresses, for URLs that project members configure.
pub fn public_only(builder: ClientBuilder) -> ClientBuilder {
  // A proxy would resolve the target itself, past the resolver.
  builder.dns_resolver(guard::PublicResolver).no_proxy()
}

fn base() -> ClientBuilder {
  Client::builder()
    .user_agent(USER_AGENT)
    .connect_timeout(CONNECT_TIMEOUT)
    // reqwest keeps headers like `PRIVATE-TOKEN` on a cross-host redirect.
    .redirect(Policy::custom(|attempt| {
      let first = attempt.previous().first();

      if attempt.previous().len() > MAX_REDIRECTS {
        attempt.error(RedirectRefused::TooMany)
      } else if first.is_some_and(|first| same_site(first, attempt.url())) {
        attempt.follow()
      } else {
        let to = redact(attempt.url());
        attempt.error(RedirectRefused::CrossOrigin(to))
      }
    }))
}

/// Whether a redirect from `from` to `to` stays on the same origin, or only
/// upgrades the same host from http on 80 to https on 443.
fn same_site(from: &Url, to: &Url) -> bool {
  from.origin() == to.origin()
    || (from.scheme() == "http"
      && to.scheme() == "https"
      && from.host() == to.host()
      && from.port_or_known_default() == Some(80)
      && to.port_or_known_default() == Some(443))
}

/// `url` without its query string or fragment.
#[must_use]
pub fn redact(url: &Url) -> String {
  let mut url = url.clone();
  url.set_query(None);
  url.set_fragment(None);
  url.to_string()
}

#[derive(Debug, thiserror::Error)]
enum RedirectRefused {
  #[error("refused a redirect to {0}, a different origin")]
  CrossOrigin(String),
  #[error("refused more than {MAX_REDIRECTS} redirects")]
  TooMany,
}

#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum Error {
  #[error("request{} failed", url_suffix(.url.as_deref()))]
  Request {
    url:    Option<String>,
    #[source]
    source: reqwest::Error,
  },
  #[error("{url} returned {status}{}", body_suffix(.body))]
  Status {
    url:    String,
    status: StatusCode,
    body:   String,
  },
  #[error("{url} sent more than {limit} bytes")]
  TooLarge { url: String, limit: usize },
  #[error("{url} sent a body that does not parse")]
  Decode {
    url:    String,
    #[source]
    source: serde_json::Error,
  },
}

impl Error {
  /// The response status, for errors that got one.
  #[must_use]
  pub const fn status(&self) -> Option<StatusCode> {
    match self {
      Self::Status { status, .. } => Some(*status),
      Self::Request { .. } | Self::TooLarge { .. } | Self::Decode { .. } => {
        None
      },
    }
  }
}

fn url_suffix(url: Option<&str>) -> String {
  url.map(|url| format!(" to {url}")).unwrap_or_default()
}

fn body_suffix(body: &str) -> String {
  if body.is_empty() {
    String::new()
  } else {
    format!(": {body}")
  }
}

fn request_error(url: Option<&Url>, source: reqwest::Error) -> Error {
  Error::Request {
    url:    url.or_else(|| source.url()).map(redact),
    source: source.without_url(),
  }
}

pub trait RequestExt {
  /// Sends the request, keeping signed query strings out of the error.
  fn send_redacted(
    self,
  ) -> impl Future<Output = Result<Response, Error>> + Send;

  /// Sends the request and fails on any status outside 2xx.
  fn send_checked(self)
  -> impl Future<Output = Result<Response, Error>> + Send;
}

impl RequestExt for RequestBuilder {
  async fn send_redacted(self) -> Result<Response, Error> {
    let (client, request) = self.build_split();
    let request = request.map_err(|source| request_error(None, source))?;
    let url = request.url().clone();

    client
      .execute(request)
      .await
      .map_err(|source| request_error(Some(&url), source))
  }

  async fn send_checked(self) -> Result<Response, Error> {
    self.send_redacted().await?.checked().await
  }
}

pub trait ResponseExt: Sized {
  /// Fails with [`Error::Status`] unless the status is 2xx.
  fn checked(self) -> impl Future<Output = Result<Self, Error>> + Send;

  /// The body, failing past `limit` bytes.
  fn bytes_capped(
    self,
    limit: usize,
  ) -> impl Future<Output = Result<Vec<u8>, Error>> + Send;

  /// The body as JSON, failing past `limit` bytes.
  fn json_capped<T: DeserializeOwned>(
    self,
    limit: usize,
  ) -> impl Future<Output = Result<T, Error>> + Send;
}

impl ResponseExt for Response {
  async fn checked(self) -> Result<Self, Error> {
    let status = self.status();

    if status.is_success() {
      return Ok(self);
    }

    let url = redact(self.url());
    let body = error_body(self).await;
    Err(Error::Status { url, status, body })
  }

  async fn bytes_capped(mut self, limit: usize) -> Result<Vec<u8>, Error> {
    let url = self.url().clone();
    let too_large = || {
      Error::TooLarge {
        url: redact(&url),
        limit,
      }
    };

    if self.content_length().is_some_and(|len| len > limit as u64) {
      return Err(too_large());
    }

    let mut body = Vec::new();

    while let Some(chunk) = self
      .chunk()
      .await
      .map_err(|source| request_error(Some(&url), source))?
    {
      if body.len() + chunk.len() > limit {
        return Err(too_large());
      }
      body.extend_from_slice(&chunk);
    }

    Ok(body)
  }

  async fn json_capped<T: DeserializeOwned>(
    self,
    limit: usize,
  ) -> Result<T, Error> {
    let url = redact(self.url());
    let body = self.bytes_capped(limit).await?;
    serde_json::from_slice(&body)
      .map_err(|source| Error::Decode { url, source })
  }
}

/// Up to [`ERROR_BODY_LIMIT`] bytes of the body, for an error message.
async fn error_body(mut response: Response) -> String {
  let mut body = Vec::new();

  while body.len() < ERROR_BODY_LIMIT {
    match response.chunk().await {
      Ok(Some(chunk)) => body.extend_from_slice(&chunk),
      Ok(None) | Err(_) => break,
    }
  }

  body.truncate(ERROR_BODY_LIMIT);
  String::from_utf8_lossy(&body).trim().to_owned()
}
