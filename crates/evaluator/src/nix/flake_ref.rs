//! Map a project's `repository_url` and a resolved commit to the canonical
//! flake reference its users would evaluate.
//!
//! Circus clones each repository locally to resolve commits and read
//! declarative config, but it must not evaluate that local checkout as the
//! flake source. A bare local path is parsed by Nix without a `baseDir`, so it
//! falls through to the `path:` fetcher, which copies the working tree verbatim
//! (including `.git`) and yields a store path that matches neither
//! `github:owner/repo` nor any `git+*` ref.
//!
//! XXX: Switching fetchers does not help, the `github:` tarball fetcher and the
//! `git+*` fetcher produce different NARs for the same commit, so only the
//! *exact* fetcher the user builds with reproduces their hash.
//!
//! To keep Circus-produced derivations hash-identical to `nix build
//! github:owner/repo`, evaluation goes through the same fetcher the user would:
//! the `github:`/`gitlab:`/`sourcehut:` shorthand for the known forges, or a
//! `git+<scheme>` ref pinned by `?rev=` for anything else.
//!
//! Local and `file://` sources are pinned to the working checkout Circus
//! already cloned (`repo_path`), not to the `repository_url`. The latter is
//! frequently a bare repository, which has no worktree for the git fetcher to
//! resolve `?rev=` against (evix null-pointers while locking it); the checkout
//! is a normal working tree that already contains the resolved commit, and a
//! local source has no public-forge hash to reproduce anyway.
use std::path::Path;

use circus_common::{CiError, error::Result};
use url::Url;

use super::flake_lock::UriPrefixes;

/// Canonical flake reference for a repository at a specific revision, plus the
/// `allowed-uris` prefixes `restrict-eval`'s `checkURI` accepts for it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceFlakeRef {
  /// Pinned flake reference handed to the evaluator, e.g. `github:o/r/<rev>`.
  pub flake_ref:    String,
  /// `checkURI`-acceptable prefixes (no rev/query) used when `restrict-eval`
  /// is enabled so the root source itself stays fetchable.
  pub allowed_uris: Vec<String>,
}

/// Build the canonical [`SourceFlakeRef`] for `repository_url` at `rev`.
///
/// `rev` is a resolved commit SHA. Known forges (github.com, gitlab.com,
/// git.sr.ht) map to their tarball shorthand so the source store path matches
/// `nix build <shorthand>`; every other host falls back to a `git+<scheme>`
/// reference pinned by `?rev=`. GitLab subgroups (more than `owner/repo`) have
/// no clean shorthand and also take the `git+https` fallback.
///
/// Local and `file://` sources ignore the parsed path and pin `git+file://` to
/// `repo_path`, the working checkout Circus already cloned (see module docs).
pub fn source_flake_ref(
  repository_url: &str,
  rev: &str,
  repo_path: &Path,
) -> Result<SourceFlakeRef> {
  ParsedRepo::parse(repository_url).into_flake_ref(rev, repo_path)
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Scheme {
  Https,
  Http,
  Ssh,
  Other(String),
}

impl Scheme {
  fn from_str(s: &str) -> Self {
    match s.to_ascii_lowercase().as_str() {
      "https" | "git+https" => Self::Https,
      "http" | "git+http" => Self::Http,
      "ssh" | "git+ssh" | "git" => Self::Ssh,
      other => Self::Other(other.to_string()),
    }
  }

  /// `git+<scheme>` prefix for the generic fetcher.
  fn git_prefix(&self) -> String {
    match self {
      Self::Https => "git+https".to_string(),
      Self::Http => "git+http".to_string(),
      Self::Ssh => "git+ssh".to_string(),
      Self::Other(s) => format!("git+{s}"),
    }
  }
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum ParsedRepo {
  /// A local filesystem path (absolute), mapped to `git+file://` so the git
  /// fetcher excludes `.git`.
  Local(String),
  Remote {
    scheme: Scheme,
    user:   Option<String>,
    host:   String,
    /// Path with no leading/trailing slash and no `.git` suffix.
    path:   String,
  },
}

impl ParsedRepo {
  fn parse(url: &str) -> Self {
    let url = url.trim();

    if let Some(path) = url.strip_prefix("file://") {
      return Self::Local(path.to_string());
    }

    let Some((scheme, rest)) = url.split_once("://") else {
      // Either an scp-like `git@host:owner/repo` (rejected by repository_url
      // validation, handled defensively) or a bare local path.
      if let Some((authority, path)) = scp_split(url) {
        let (user, host) = split_user_host(authority);
        return Self::Remote {
          scheme: Scheme::Ssh,
          user,
          host,
          path: clean_path(path),
        };
      }
      return Self::Local(url.to_string());
    };

    let (authority, path) = match rest.split_once('/') {
      Some((a, p)) => (a, p),
      None => (rest, ""),
    };
    let (user, host) = split_user_host(authority);

    Self::Remote {
      scheme: Scheme::from_str(scheme),
      user,
      host,
      path: clean_path(path),
    }
  }

  fn into_flake_ref(
    self,
    rev: &str,
    repo_path: &Path,
  ) -> Result<SourceFlakeRef> {
    let (scheme, user, host, path) = match self {
      Self::Local(_) => return local_git_ref(repo_path, rev),
      Self::Remote {
        scheme,
        user,
        host,
        path,
      } => (scheme, user, host, path),
    };

    let segments = path.split('/').filter(|s| !s.is_empty()).count();

    let primary = match host.as_str() {
      "github.com" if segments == 2 => forge_ref("github", &path, rev),
      "gitlab.com" if segments == 2 => forge_ref("gitlab", &path, rev),
      "git.sr.ht" if segments == 2 => forge_ref("sourcehut", &path, rev),
      _ => generic_git_ref(&scheme, user.as_deref(), &host, &path, rev),
    };

    Ok(primary)
  }
}

/// `git+<scheme>://[user@]host/path?rev=<rev>` for hosts without a tarball
/// shorthand. This is the canonical ref a user on that host would build, so
/// hashes still match within that forge.
fn generic_git_ref(
  scheme: &Scheme,
  user: Option<&str>,
  host: &str,
  path: &str,
  rev: &str,
) -> SourceFlakeRef {
  let userinfo = match (scheme, user) {
    (Scheme::Ssh, Some(u)) => format!("{u}@"),
    _ => String::new(),
  };
  let base = format!("{}://{userinfo}{host}/{path}", scheme.git_prefix());
  pinned(&base, rev)
}

/// `git+file://<repo_path>` for the local working checkout. The git fetcher
/// copies the tracked tree (excluding `.git` and gitignored paths), unlike the
/// bare-path `path:` fetcher which copies the directory verbatim. `repo_path`
/// is the checkout Circus cloned, which has a worktree and the resolved commit;
/// the bare `repository_url` would have neither.
fn local_git_ref(repo_path: &Path, rev: &str) -> Result<SourceFlakeRef> {
  let repo_path = if repo_path.is_absolute() {
    repo_path.to_path_buf()
  } else {
    std::env::current_dir()
      .map_err(|error| {
        CiError::NixEval(format!(
          "Failed to resolve relative repository checkout {}: {error}",
          repo_path.display()
        ))
      })?
      .join(repo_path)
  };
  let url = Url::from_file_path(&repo_path).map_err(|()| {
    CiError::NixEval(format!(
      "Failed to convert repository checkout to a file URL: {}",
      repo_path.display()
    ))
  })?;
  Ok(pinned(&format!("git+{url}"), rev))
}

/// Append `?rev=<rev>` to a `git+*` base and derive its `allowed-uris`.
fn pinned(base: &str, rev: &str) -> SourceFlakeRef {
  let flake_ref = if rev.is_empty() {
    base.to_string()
  } else {
    format!("{base}?rev={rev}")
  };
  SourceFlakeRef {
    allowed_uris: UriPrefixes::from_uri(base).into_vec(),
    flake_ref,
  }
}

/// `<forge>:<path>/<rev>` plus the `<forge>:<path>` allowed-uri.
///
/// The rev is omitted from the allowed-uri because `checkURI` rejects the
/// `?narHash`/rev-suffixed form (see `flake_lock`'s input mapping).
fn forge_ref(forge: &str, path: &str, rev: &str) -> SourceFlakeRef {
  let allowed = format!("{forge}:{path}");
  let flake_ref = if rev.is_empty() {
    allowed.clone()
  } else {
    format!("{allowed}/{rev}")
  };
  SourceFlakeRef {
    flake_ref,
    allowed_uris: vec![allowed],
  }
}

/// Split an scp-like `git@host:owner/repo` into (`authority`, `path`).
/// Returns `None` when the colon belongs to a `scheme://` or `host:port` form.
fn scp_split(url: &str) -> Option<(&str, &str)> {
  let (left, right) = url.split_once(':')?;
  // A numeric segment after the colon is a port, not an scp path.
  if right
    .chars()
    .take_while(|c| *c != '/')
    .all(|c| c.is_ascii_digit())
    && right.contains('/')
  {
    return None;
  }
  if left.is_empty() || right.is_empty() {
    return None;
  }
  Some((left, right))
}

/// Split `[user@]host[:port]` into (`user`, lowercased `host`).
fn split_user_host(authority: &str) -> (Option<String>, String) {
  let (user, hostport) = match authority.split_once('@') {
    Some((u, h)) => (Some(u.to_string()), h),
    None => (None, authority),
  };
  let host = hostport
    .split_once(':')
    .map_or(hostport, |(h, _port)| h)
    .to_ascii_lowercase();
  (user, host)
}

/// Strip leading/trailing slashes and a single trailing `.git`.
fn clean_path(path: &str) -> String {
  path
    .trim_matches('/')
    .strip_suffix(".git")
    .unwrap_or_else(|| path.trim_matches('/'))
    .trim_matches('/')
    .to_string()
}

#[cfg(test)]
mod tests {
  use super::*;

  const REV: &str = "abc123def456";
  const CHECKOUT: &str = "/var/lib/circus/evaluator/proj";

  /// Resolve a remote ref; `repo_path` is irrelevant for non-local sources.
  fn sref(url: &str, rev: &str) -> SourceFlakeRef {
    source_flake_ref(url, rev, Path::new(CHECKOUT))
      .expect("test source reference")
  }

  #[test]
  fn github_https_maps_to_shorthand() {
    let r = sref("https://github.com/owner/repo", REV);
    assert_eq!(r.flake_ref, "github:owner/repo/abc123def456");
    assert_eq!(r.allowed_uris, vec!["github:owner/repo"]);
  }

  #[test]
  fn github_dot_git_suffix_is_stripped() {
    let r = sref("https://github.com/owner/repo.git", REV);
    assert_eq!(r.flake_ref, "github:owner/repo/abc123def456");
  }

  #[test]
  fn github_ssh_url_still_uses_tarball_shorthand() {
    // A user builds `github:owner/repo` regardless of the clone transport, so
    // the shorthand (not git+ssh) is what reproduces their hash.
    let r = sref("ssh://git@github.com/owner/repo.git", REV);
    assert_eq!(r.flake_ref, "github:owner/repo/abc123def456");
  }

  #[test]
  fn github_scp_form_is_handled() {
    let r = sref("git@github.com:owner/repo.git", REV);
    assert_eq!(r.flake_ref, "github:owner/repo/abc123def456");
  }

  #[test]
  fn gitlab_https_maps_to_shorthand() {
    let r = sref("https://gitlab.com/owner/repo", REV);
    assert_eq!(r.flake_ref, "gitlab:owner/repo/abc123def456");
    assert_eq!(r.allowed_uris, vec!["gitlab:owner/repo"]);
  }

  #[test]
  fn sourcehut_keeps_tilde_owner() {
    let r = sref("https://git.sr.ht/~owner/repo", REV);
    assert_eq!(r.flake_ref, "sourcehut:~owner/repo/abc123def456");
    assert_eq!(r.allowed_uris, vec!["sourcehut:~owner/repo"]);
  }

  #[test]
  fn gitlab_subgroup_falls_back_to_generic_git() {
    let r = sref("https://gitlab.com/group/subgroup/repo", REV);
    assert_eq!(
      r.flake_ref,
      "git+https://gitlab.com/group/subgroup/repo?rev=abc123def456"
    );
  }

  #[test]
  fn self_hosted_https_uses_git_plus_https() {
    let r = sref("https://git.example.com/owner/repo.git", REV);
    assert_eq!(
      r.flake_ref,
      "git+https://git.example.com/owner/repo?rev=abc123def456"
    );
    assert_eq!(r.allowed_uris, vec![
      "git+https://git.example.com/owner/repo",
      "git+https://git.example.com/owner/",
    ]);
  }

  #[test]
  fn self_hosted_ssh_preserves_user() {
    let r = sref("ssh://git@git.example.com/owner/repo", REV);
    assert_eq!(
      r.flake_ref,
      "git+ssh://git@git.example.com/owner/repo?rev=abc123def456"
    );
  }

  #[test]
  fn host_casing_is_normalized() {
    let r = sref("https://GitHub.com/Owner/Repo", REV);
    assert_eq!(r.flake_ref, "github:Owner/Repo/abc123def456");
  }

  #[test]
  fn local_sources_pin_git_file_to_the_checkout() {
    // A bare local path and a file:// origin both resolve to the working
    // checkout (`repo_path`), not the parsed source path: the origin may be a
    // bare repo with no worktree for the git fetcher to lock against.
    let bare = source_flake_ref(
      "/var/lib/circus/test-repos/test-flake.git",
      REV,
      Path::new(CHECKOUT),
    )
    .expect("test source reference");
    assert_eq!(
      bare.flake_ref,
      "git+file:///var/lib/circus/evaluator/proj?rev=abc123def456"
    );

    let file_url = source_flake_ref(
      "file:///var/lib/circus/test-repos/test-flake.git",
      REV,
      Path::new(CHECKOUT),
    )
    .expect("test source reference");
    assert_eq!(
      file_url.flake_ref,
      "git+file:///var/lib/circus/evaluator/proj?rev=abc123def456"
    );
  }

  #[test]
  fn empty_rev_omits_pin() {
    let r = sref("https://github.com/owner/repo", "");
    assert_eq!(r.flake_ref, "github:owner/repo");
    let g = sref("https://git.example.com/owner/repo", "");
    assert_eq!(g.flake_ref, "git+https://git.example.com/owner/repo");
  }
  #[test]
  fn local_checkout_path_is_percent_encoded() {
    let local = source_flake_ref(
      "/var/lib/circus/test-repos/test-flake.git",
      REV,
      Path::new("/var/lib/circus/evaluator/a project"),
    )
    .expect("test source reference");
    assert_eq!(
      local.flake_ref,
      "git+file:///var/lib/circus/evaluator/a%20project?rev=abc123def456"
    );
    assert!(local.allowed_uris.iter().all(|uri| !uri.contains(' ')));
  }
}
