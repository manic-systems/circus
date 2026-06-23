//! Shared assembly of binary-cache observability data for the admin JSON API
//! ([`routes::admin`]) and the dashboard Caches pages ([`routes::dashboard`]).
//!
//! A cache has no first-class table: identity is a name (`global` or a project
//! name) that maps to a `narinfo_cache.project_id` scope (`None` for global).
//! Both surfaces resolve a [`CacheRef`], then read storage from `narinfo_cache`
//! and traffic from `cache_traffic`.

use circus_config::Config;

use crate::{error::ApiError, signing, state::AppState};

/// Identity and provenance of one cache.
pub struct CacheRef {
  /// `global` or the project name.
  pub name:      String,
  /// `None` for the global cache, the project id otherwise.
  pub scope:     Option<uuid::Uuid>,
  /// Whether the cache is currently serving (config/project toggle).
  pub active:    bool,
  /// A project's own `cache_url` override, if set.
  pub cache_url: Option<String>,
}

impl CacheRef {
  /// Whether this is the global cache.
  #[must_use]
  pub const fn is_global(&self) -> bool {
    self.scope.is_none()
  }

  /// Lowercase scope kind for JSON payloads (`global` / `project`).
  #[must_use]
  pub const fn scope_kind(&self) -> &'static str {
    if self.is_global() {
      "global"
    } else {
      "project"
    }
  }

  /// Capitalized scope label for badges (`Global` / `Project`).
  #[must_use]
  pub const fn scope_label(&self) -> &'static str {
    if self.is_global() {
      "Global"
    } else {
      "Project"
    }
  }
}

/// The global cache plus every cache-enabled project, in listing order.
///
/// # Errors
///
/// Returns a database error if the project listing fails.
pub async fn list_cache_refs(
  state: &AppState,
) -> Result<Vec<CacheRef>, ApiError> {
  let mut refs = vec![CacheRef {
    name:      "global".to_owned(),
    scope:     None,
    active:    state.config.cache.enabled,
    cache_url: state.config.cache.cache_url.clone(),
  }];

  // 10k is far above any realistic project count; pagination here would only
  // add noise for an admin overview surface.
  let projects =
    circus_common::repo::projects::list(&state.pool, 10_000, 0).await?;
  for project in projects {
    if project.cache_enabled {
      refs.push(CacheRef {
        name:      project.name,
        scope:     Some(project.id),
        active:    project.cache_enabled,
        cache_url: project.cache_url,
      });
    }
  }
  Ok(refs)
}

/// Resolve a cache name to its [`CacheRef`]. `global` always resolves; a
/// project name resolves when the project exists (regardless of whether its
/// cache is enabled, so the detail page can show an inactive cache).
///
/// # Returns
///
/// `None` when `name` is neither `global` nor an existing project.
///
/// # Errors
///
/// Returns a database error other than not-found.
pub async fn resolve_cache_ref(
  state: &AppState,
  name: &str,
) -> Result<Option<CacheRef>, ApiError> {
  if name == "global" {
    return Ok(Some(CacheRef {
      name:      "global".to_owned(),
      scope:     None,
      active:    state.config.cache.enabled,
      cache_url: state.config.cache.cache_url.clone(),
    }));
  }
  match circus_common::repo::projects::get_by_name(&state.pool, name).await {
    Ok(project) => {
      Ok(Some(CacheRef {
        name:      project.name,
        scope:     Some(project.id),
        active:    project.cache_enabled,
        cache_url: project.cache_url,
      }))
    },
    Err(circus_common::CiError::NotFound(_)) => Ok(None),
    Err(error) => Err(ApiError(error)),
  }
}

/// The substituter URL a consumer adds to `nix.conf` for this cache.
///
/// Global uses `cache.cache_url`. A project uses its own `cache_url` override
/// when set, else derives `<site>/projects/<name>/nix-cache/` from the global
/// base. Returns `None` when no global `cache_url` is configured and the
/// project has no override.
#[must_use]
pub fn substituter_url(
  config: &Config,
  cache_ref: &CacheRef,
) -> Option<String> {
  if !cache_ref.active {
    return None;
  }
  if cache_ref.is_global() {
    return config.cache.cache_url.clone();
  }
  circus_config::project_cache_url(
    config.cache.cache_url.as_deref(),
    &cache_ref.name,
    cache_ref.cache_url.as_deref(),
  )
}

/// The public signing key consumers add to `trusted-public-keys`. Same value
/// for every cache (one server keypair). `None` when signing is disabled or
/// the key cannot be derived.
#[must_use]
pub fn public_key(config: &Config) -> Option<String> {
  signing::signing_public_key(config)
}

/// A ready-to-paste `nix.conf` fragment wiring this cache as a substituter.
/// `None` when there is no substituter URL to advertise.
#[must_use]
pub fn nix_conf_snippet(
  substituter: Option<&str>,
  public_key: Option<&str>,
) -> Option<String> {
  let substituter = substituter?;
  let mut snippet = format!("substituters = {substituter}\n");
  if let Some(key) = public_key {
    snippet.push_str("trusted-public-keys = ");
    snippet.push_str(key);
    snippet.push('\n');
  }
  Some(snippet)
}
