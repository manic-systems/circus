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
//! TODO: for private repositories the forge fetcher needs its own credentials
//! (Nix `access-tokens` / netrc); Circus's git clone credentials do not carry
//! over to the in-process fetcher. We've got to fix this for Evix, or more
//! directly, in the C API bindings.

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
#[must_use]
pub fn source_flake_ref(repository_url: &str, rev: &str) -> SourceFlakeRef {
  ParsedRepo::parse(repository_url).into_flake_ref(rev)
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

  /// `git+<scheme>` prefix for the fallback fetcher.
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
  /// A local filesystem path (absolute), used only in tests and defensive
  /// fallbacks. Mapped to `git+file://` so the git fetcher excludes `.git`.
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

  fn into_flake_ref(self, rev: &str) -> SourceFlakeRef {
    let (scheme, user, host, path) = match self {
      Self::Local(path) => return local_git_ref(&path, rev),
      Self::Remote {
        scheme,
        user,
        host,
        path,
      } => (scheme, user, host, path),
    };

    let segments = path.split('/').filter(|s| !s.is_empty()).count();

    match host.as_str() {
      "github.com" if segments == 2 => forge_ref("github", &path, rev),
      "gitlab.com" if segments == 2 => forge_ref("gitlab", &path, rev),
      "git.sr.ht" if segments == 2 => forge_ref("sourcehut", &path, rev),
      _ => generic_git_ref(&scheme, user.as_deref(), &host, &path, rev),
    }
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

/// `git+file://<path>` for a local checkout. The git fetcher copies the tracked
/// tree (excluding `.git` and gitignored paths), unlike the bare-path `path:`
/// fetcher which copies the directory verbatim.
fn local_git_ref(path: &str, rev: &str) -> SourceFlakeRef {
  pinned(&format!("git+file://{path}"), rev)
}

/// Append `?rev=<rev>` to a `git+*` base and derive its `allowed-uris`.
fn pinned(base: &str, rev: &str) -> SourceFlakeRef {
  let flake_ref = if rev.is_empty() {
    base.to_string()
  } else {
    format!("{base}?rev={rev}")
  };
  SourceFlakeRef {
    allowed_uris: scheme_url_and_parent(base),
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

/// A `git+<scheme>://...` url and its parent directory, covering the prefixes
/// `checkURI` matches against the rev-suffixed fetch.
fn scheme_url_and_parent(base: &str) -> Vec<String> {
  let mut out = vec![base.to_string()];
  if let Some(slash) = base.rfind('/')
    && slash + 1 < base.len()
  {
    out.push(base[..=slash].to_string());
  }
  out
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

  #[test]
  fn github_https_maps_to_shorthand() {
    let r = source_flake_ref("https://github.com/owner/repo", REV);
    assert_eq!(r.flake_ref, "github:owner/repo/abc123def456");
    assert_eq!(r.allowed_uris, vec!["github:owner/repo"]);
  }

  #[test]
  fn github_dot_git_suffix_is_stripped() {
    let r = source_flake_ref("https://github.com/owner/repo.git", REV);
    assert_eq!(r.flake_ref, "github:owner/repo/abc123def456");
  }

  #[test]
  fn github_ssh_url_still_uses_tarball_shorthand() {
    // A user builds `github:owner/repo` regardless of the clone transport, so
    // the shorthand (not git+ssh) is what reproduces their hash.
    let r = source_flake_ref("ssh://git@github.com/owner/repo.git", REV);
    assert_eq!(r.flake_ref, "github:owner/repo/abc123def456");
  }

  #[test]
  fn github_scp_form_is_handled() {
    let r = source_flake_ref("git@github.com:owner/repo.git", REV);
    assert_eq!(r.flake_ref, "github:owner/repo/abc123def456");
  }

  #[test]
  fn gitlab_https_maps_to_shorthand() {
    let r = source_flake_ref("https://gitlab.com/owner/repo", REV);
    assert_eq!(r.flake_ref, "gitlab:owner/repo/abc123def456");
    assert_eq!(r.allowed_uris, vec!["gitlab:owner/repo"]);
  }

  #[test]
  fn sourcehut_keeps_tilde_owner() {
    let r = source_flake_ref("https://git.sr.ht/~owner/repo", REV);
    assert_eq!(r.flake_ref, "sourcehut:~owner/repo/abc123def456");
    assert_eq!(r.allowed_uris, vec!["sourcehut:~owner/repo"]);
  }

  #[test]
  fn gitlab_subgroup_falls_back_to_generic_git() {
    let r = source_flake_ref("https://gitlab.com/group/subgroup/repo", REV);
    assert_eq!(
      r.flake_ref,
      "git+https://gitlab.com/group/subgroup/repo?rev=abc123def456"
    );
  }

  #[test]
  fn self_hosted_https_uses_git_plus_https() {
    let r = source_flake_ref("https://git.example.com/owner/repo.git", REV);
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
    let r = source_flake_ref("ssh://git@git.example.com/owner/repo", REV);
    assert_eq!(
      r.flake_ref,
      "git+ssh://git@git.example.com/owner/repo?rev=abc123def456"
    );
  }

  #[test]
  fn host_casing_is_normalized() {
    let r = source_flake_ref("https://GitHub.com/Owner/Repo", REV);
    assert_eq!(r.flake_ref, "github:Owner/Repo/abc123def456");
  }

  #[test]
  fn local_path_maps_to_git_file() {
    let r = source_flake_ref("/var/lib/circus/work/proj", REV);
    assert_eq!(
      r.flake_ref,
      "git+file:///var/lib/circus/work/proj?rev=abc123def456"
    );
    let f = source_flake_ref("file:///tmp/checkout", REV);
    assert_eq!(f.flake_ref, "git+file:///tmp/checkout?rev=abc123def456");
  }

  #[test]
  fn empty_rev_omits_pin() {
    let r = source_flake_ref("https://github.com/owner/repo", "");
    assert_eq!(r.flake_ref, "github:owner/repo");
    let g = source_flake_ref("https://git.example.com/owner/repo", "");
    assert_eq!(g.flake_ref, "git+https://git.example.com/owner/repo");
  }
}
