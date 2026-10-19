//! SSRF guards for URLs configured by project members.

use std::{
  error::Error as StdError,
  io,
  net::{IpAddr, SocketAddr},
  sync::OnceLock,
};

use circus_types::net::{AllowedHost, is_internal_ip, normalize_host};
use reqwest::dns::{Addrs, Name, Resolve, Resolving};

static ALLOWED: OnceLock<Vec<AllowedHost>> = OnceLock::new();

#[derive(Debug, thiserror::Error)]
#[error("the internal host allowlist is already set")]
pub struct AllowlistAlreadySet;

/// Lets project URLs reach `hosts` for the rest of the process.
///
/// # Errors
///
/// Fails when an earlier call already set the allowlist.
pub fn allow_internal(
  hosts: &[AllowedHost],
) -> Result<(), AllowlistAlreadySet> {
  ALLOWED.set(hosts.to_vec()).map_err(|_| AllowlistAlreadySet)
}

fn allowed_name(host: &str) -> bool {
  allowed()
    .any(|allowed| matches!(allowed, AllowedHost::Name(name) if name == host))
}

fn allowed_ip(ip: IpAddr) -> bool {
  allowed()
    .any(|allowed| matches!(allowed, AllowedHost::Range(range) if range.contains(ip)))
}

fn allowed() -> impl Iterator<Item = &'static AllowedHost> {
  ALLOWED.get().into_iter().flatten()
}

/// Whether a URL host is internal and not allowlisted. Names are not
/// resolved, [`public_only`](crate::public_only) clients check those.
#[must_use]
pub fn is_internal_host(raw_host: &str) -> bool {
  let host = normalize_host(raw_host);

  if allowed_name(&host) {
    return false;
  }

  host.parse::<IpAddr>().map_or_else(
    |_| {
      host == "localhost"
        || host.ends_with(".localhost")
        || host.ends_with(".internal")
    },
    |ip| is_internal_ip(ip) && !allowed_ip(ip),
  )
}

/// The addresses `host` resolves to on `port`, minus internal ones that are
/// not allowlisted.
///
/// # Errors
///
/// Fails when resolution fails or leaves only internal addresses.
pub async fn resolve_public(
  host: &str,
  port: u16,
) -> io::Result<Vec<SocketAddr>> {
  let host = normalize_host(host);
  let named = allowed_name(&host);
  let public = tokio::net::lookup_host((host.as_str(), port))
    .await?
    .filter(|addr| named || !is_internal_ip(addr.ip()) || allowed_ip(addr.ip()))
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
