use std::fmt;

/// A validated flake reference safe to pass to Nix commands.
///
/// Constructed only through [`parse`](Self::parse) or
/// [`from_url`](Self::from_url), both of which reject local filesystem refs
/// (`path:`, `file:`, `.`, `..`, `~`, absolute paths, etc.).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Ref(String);

impl Ref {
  /// Parse and validate an untrusted flake reference string.
  ///
  /// Rejects local filesystem refs and returns the validated reference.
  /// Remote refs (`github:`, `gitlab:`, `git+https://`, …) are accepted.
  ///
  /// # Errors
  ///
  /// Returns a description of why the value was rejected.
  pub fn parse(value: &str) -> Result<Self, String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
      return Err("flake ref cannot be empty".to_string());
    }
    if trimmed.len() > 2048 {
      return Err("flake ref must be at most 2048 characters".to_string());
    }
    if trimmed.contains('\0') {
      return Err("flake ref must not contain null bytes".to_string());
    }

    const LOCAL_EXACT: &[&str] = &[".", "..", "~"];
    const LOCAL_PREFIXES: &[&str] =
      &["path:", "file:", "git+file:", "/", "./", "../", "~/"];

    let lower = trimmed.to_ascii_lowercase();
    if LOCAL_EXACT.contains(&trimmed)
      || LOCAL_PREFIXES.iter().any(|p| lower.starts_with(p))
    {
      return Err("local filesystem flake refs are not allowed".to_string());
    }
    Ok(Self(trimmed.to_string()))
  }

  /// Convert a repository URL to a validated flake reference.
  ///
  /// GitHub and GitLab URLs are converted to their native flake ref formats
  /// (`github:owner/repo`, `gitlab:owner/repo`). Other HTTPS URLs get a
  /// `git+` prefix so Nix clones via git. URLs that are already valid flake
  /// refs are returned as-is. The result is validated against the same rules
  /// as [`parse`](Self::parse).
  ///
  /// # Errors
  ///
  /// Returns error if the resulting flake ref would be a local filesystem
  /// reference.
  pub fn from_url(url: &str) -> Result<Self, String> {
    let url_trimmed = url.trim().trim_end_matches('/');

    // Already a flake ref (github:, gitlab:, git+, sourcehut:, etc.)
    if url_trimmed.contains(':')
      && !url_trimmed.starts_with("http://")
      && !url_trimmed.starts_with("https://")
    {
      return Self::parse(url_trimmed);
    }

    // Extract host + path from HTTP(S) URLs
    let without_scheme = url_trimmed
      .strip_prefix("https://")
      .or_else(|| url_trimmed.strip_prefix("http://"))
      .unwrap_or(url_trimmed);
    let without_dotgit = without_scheme.trim_end_matches(".git");

    // github.com/owner/repo -> github:owner/repo
    if let Some(path) = without_dotgit.strip_prefix("github.com/") {
      return Self::parse(&format!("github:{path}"));
    }

    // gitlab.com/owner/repo -> gitlab:owner/repo
    if let Some(path) = without_dotgit.strip_prefix("gitlab.com/") {
      return Self::parse(&format!("gitlab:{path}"));
    }

    // Any other HTTPS/HTTP URL: prefix with git+ so nix clones it
    if url_trimmed.starts_with("https://") || url_trimmed.starts_with("http://")
    {
      return Self::parse(&format!("git+{url_trimmed}"));
    }

    Self::parse(url_trimmed)
  }

  /// Return the flake ref string with a `?rev=` query parameter appended.
  #[must_use]
  pub fn with_revision(&self, rev: &str) -> String {
    format!("{}?rev={rev}", self.0)
  }

  #[must_use]
  pub fn as_str(&self) -> &str {
    &self.0
  }
}

impl fmt::Display for Ref {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    f.write_str(&self.0)
  }
}

impl AsRef<str> for Ref {
  fn as_ref(&self) -> &str {
    &self.0
  }
}

#[cfg(test)]
#[expect(clippy::unwrap_used, reason = "Fine in tests")]
mod tests {
  use super::*;

  #[test]
  fn parse_rejects_local_filesystem_refs() {
    for value in [
      "path:/var/lib/circus",
      "file:///var/lib/circus",
      "git+file:///var/lib/circus/repo",
      "/var/lib/circus",
      ".",
      "..",
      "~",
      "./repo",
      "../repo",
      "~/repo",
    ] {
      assert!(Ref::parse(value).is_err(), "{value}");
    }
  }

  #[test]
  fn parse_accepts_remote_refs() {
    assert!(Ref::parse("github:owner/repo").is_ok());
    assert!(Ref::parse("git+https://example.com/repo").is_ok());
    assert!(Ref::parse("https://example.com/repo").is_ok());
  }

  #[test]
  fn from_url_github_https() {
    assert_eq!(
      Ref::from_url("https://github.com/notashelf/rags")
        .unwrap()
        .as_str(),
      "github:notashelf/rags"
    );
    assert_eq!(
      Ref::from_url("https://github.com/NixOS/nixpkgs")
        .unwrap()
        .as_str(),
      "github:NixOS/nixpkgs"
    );
    assert_eq!(
      Ref::from_url("https://github.com/owner/repo.git")
        .unwrap()
        .as_str(),
      "github:owner/repo"
    );
    assert_eq!(
      Ref::from_url("http://github.com/owner/repo")
        .unwrap()
        .as_str(),
      "github:owner/repo"
    );
    assert_eq!(
      Ref::from_url("https://github.com/owner/repo/")
        .unwrap()
        .as_str(),
      "github:owner/repo"
    );
  }

  #[test]
  fn from_url_gitlab_https() {
    assert_eq!(
      Ref::from_url("https://gitlab.com/owner/repo")
        .unwrap()
        .as_str(),
      "gitlab:owner/repo"
    );
    assert_eq!(
      Ref::from_url("https://gitlab.com/group/subgroup/repo.git")
        .unwrap()
        .as_str(),
      "gitlab:group/subgroup/repo"
    );
  }

  #[test]
  fn from_url_already_flake_ref() {
    assert_eq!(
      Ref::from_url("github:owner/repo").unwrap().as_str(),
      "github:owner/repo"
    );
    assert_eq!(
      Ref::from_url("gitlab:owner/repo").unwrap().as_str(),
      "gitlab:owner/repo"
    );
    assert_eq!(
      Ref::from_url("git+https://example.com/repo.git")
        .unwrap()
        .as_str(),
      "git+https://example.com/repo.git"
    );
    assert_eq!(
      Ref::from_url("sourcehut:~user/repo").unwrap().as_str(),
      "sourcehut:~user/repo"
    );
  }

  #[test]
  fn from_url_rejects_local_path() {
    assert!(Ref::from_url("path:/some/local/path").is_err());
  }

  #[test]
  fn from_url_other_https() {
    assert_eq!(
      Ref::from_url("https://codeberg.org/owner/repo")
        .unwrap()
        .as_str(),
      "git+https://codeberg.org/owner/repo"
    );
    assert_eq!(
      Ref::from_url("https://sr.ht/~user/repo").unwrap().as_str(),
      "git+https://sr.ht/~user/repo"
    );
  }

  #[test]
  fn with_revision_appends_query() {
    let r = Ref::parse("github:owner/repo").unwrap();
    assert_eq!(r.with_revision("abc123"), "github:owner/repo?rev=abc123");
  }

  #[test]
  fn display_shows_inner_string() {
    let r = Ref::parse("github:owner/repo").unwrap();
    assert_eq!(format!("{r}"), "github:owner/repo");
  }
}
