//! SSRF guards for URLs configured by project members.

use std::{
  error::Error as StdError,
  io,
  net::{IpAddr, SocketAddr},
};

use circus_types::net::{is_internal_ip, normalize_host};
use reqwest::dns::{Addrs, Name, Resolve, Resolving};

/// Whether a URL host is internal. Names are not resolved,
/// [`public_only`](crate::public_only) clients check those.
#[must_use]
pub fn is_internal_host(raw_host: &str) -> bool {
  let host = normalize_host(raw_host);

  host.parse::<IpAddr>().map_or_else(
    |_| {
      host == "localhost"
        || host.ends_with(".localhost")
        || host.ends_with(".internal")
    },
    is_internal_ip,
  )
}

/// The addresses `host` resolves to on `port`, minus internal ones.
///
/// # Errors
///
/// Fails when resolution fails or leaves only internal addresses.
pub async fn resolve_public(
  host: &str,
  port: u16,
) -> io::Result<Vec<SocketAddr>> {
  let host = normalize_host(host);
  let public = tokio::net::lookup_host((host.as_str(), port))
    .await?
    .filter(|addr| !is_internal_ip(addr.ip()))
    .collect::<Vec<SocketAddr>>();

  if public.is_empty() {
    return Err(io::Error::other(InternalOnly(host)));
  }

  Ok(public)
}

#[derive(Debug, thiserror::Error)]
#[error("{0} resolves only to internal addresses")]
struct InternalOnly(String);

/// The system resolver, minus internal addresses.
pub(crate) struct PublicResolver;

impl Resolve for PublicResolver {
  fn resolve(&self, name: Name) -> Resolving {
    let host = name.as_str().to_owned();

    Box::pin(async move {
      let public = resolve_public(&host, 0)
        .await
        .map_err(|error| Box::new(error) as Box<dyn StdError + Send + Sync>)?;

      Ok(Box::new(public.into_iter()) as Addrs)
    })
  }
}
