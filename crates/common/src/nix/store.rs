use std::fmt;

/// A validated Nix store path.
///
/// Rejects path traversal, overly long paths, and paths outside the
/// configured store directory.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StorePath(String);

impl StorePath {
  /// Validate and construct a store path.
  ///
  /// # Errors
  ///
  /// Returns error for path traversal, wrong prefix, or paths exceeding
  /// 512 characters.
  pub fn parse(path: &str, store_dir: &str) -> Result<Self, String> {
    if !Self::is_valid(path, store_dir) {
      return Err(format!(
        "invalid store path: must be under {store_dir}/, at most 512 chars, \
         with no path traversal"
      ));
    }
    Ok(Self(path.to_string()))
  }

  /// Check validity without constructing.
  #[must_use]
  pub fn is_valid(path: &str, store_dir: &str) -> bool {
    let store_dir = store_dir.trim_end_matches('/');
    let prefix = format!("{store_dir}/");
    path.starts_with(prefix.as_str())
      && !path.contains("..")
      && path.len() < 512
  }

  #[must_use]
  pub fn as_str(&self) -> &str {
    &self.0
  }
}

impl fmt::Display for StorePath {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    f.write_str(&self.0)
  }
}

impl AsRef<str> for StorePath {
  fn as_ref(&self) -> &str {
    &self.0
  }
}

/// A validated 32-character Nix store hash.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NixHash(String);

impl NixHash {
  /// Validate and construct a Nix hash.
  ///
  /// # Errors
  ///
  /// Returns error if the hash is not exactly 32 lowercase alphanumeric
  /// characters.
  pub fn parse(hash: &str) -> Result<Self, String> {
    if !Self::is_valid(hash) {
      return Err(
        "nix hash must be exactly 32 lowercase alphanumeric characters"
          .to_string(),
      );
    }
    Ok(Self(hash.to_string()))
  }

  /// Check validity without constructing.
  #[must_use]
  pub fn is_valid(hash: &str) -> bool {
    hash.len() == 32
      && hash
        .chars()
        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit())
  }

  #[must_use]
  pub fn as_str(&self) -> &str {
    &self.0
  }
}

impl fmt::Display for NixHash {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    f.write_str(&self.0)
  }
}

impl AsRef<str> for NixHash {
  fn as_ref(&self) -> &str {
    &self.0
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn valid_store_path() {
    assert!(StorePath::is_valid(
      "/nix/store/aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa-hello-2.12",
      "/nix/store",
    ));
  }

  #[test]
  fn valid_store_path_nested() {
    assert!(StorePath::is_valid(
      "/nix/store/aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa-hello-2.12/bin/hello",
      "/nix/store",
    ));
  }

  #[test]
  fn store_path_rejects_path_traversal() {
    assert!(!StorePath::is_valid(
      "/nix/store/aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa-hello/../../../etc/passwd",
      "/nix/store",
    ));
  }

  #[test]
  fn store_path_rejects_relative_path() {
    assert!(!StorePath::is_valid("nix/store/something", "/nix/store"));
  }

  #[test]
  fn store_path_rejects_wrong_prefix() {
    assert!(!StorePath::is_valid(
      "/tmp/nix/store/something",
      "/nix/store"
    ));
    assert!(!StorePath::is_valid("/etc/passwd", "/nix/store"));
    assert!(!StorePath::is_valid("/nix/var/something", "/nix/store"));
  }

  #[test]
  fn store_path_rejects_empty() {
    assert!(!StorePath::is_valid("", "/nix/store"));
  }

  #[test]
  fn store_path_rejects_just_prefix() {
    assert!(StorePath::is_valid("/nix/store/", "/nix/store"));
  }

  #[test]
  fn store_path_rejects_overly_long() {
    let long_path = format!("/nix/store/{}", "a".repeat(512));
    assert!(!StorePath::is_valid(&long_path, "/nix/store"));
  }

  #[test]
  fn store_path_rejects_double_dot_embedded() {
    assert!(!StorePath::is_valid("/nix/store/abc..def", "/nix/store"));
  }

  #[test]
  fn valid_store_path_custom_store_dir() {
    assert!(StorePath::is_valid(
      "/opt/nix/store/aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa-hello-2.12",
      "/opt/nix/store",
    ));
    assert!(!StorePath::is_valid(
      "/nix/store/aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa-hello-2.12",
      "/opt/nix/store",
    ));
  }

  #[test]
  fn valid_nix_hash_lowercase_alpha() {
    assert!(NixHash::is_valid("abcdefghijklmnopqrstuvwxyzabcdef"));
  }

  #[test]
  fn valid_nix_hash_digits() {
    assert!(NixHash::is_valid("01234567890123456789012345678901"));
  }

  #[test]
  fn valid_nix_hash_mixed() {
    assert!(NixHash::is_valid("a1b2c3d4e5f6a7b8c9d0e1f2a3b4c5d6"));
  }

  #[test]
  fn nix_hash_rejects_uppercase() {
    assert!(!NixHash::is_valid("ABCDEFGHIJKLMNOPQRSTUVWXYZABCDEF"));
  }

  #[test]
  fn nix_hash_rejects_mixed_case() {
    assert!(!NixHash::is_valid("abcdefghijklmnopqrstuvwxyzAbcdeF"));
  }

  #[test]
  fn nix_hash_rejects_too_short() {
    assert!(!NixHash::is_valid("abcdef1234567890"));
  }

  #[test]
  fn nix_hash_rejects_too_long() {
    assert!(!NixHash::is_valid("abcdefghijklmnopqrstuvwxyzabcdefg"));
  }

  #[test]
  fn nix_hash_rejects_empty() {
    assert!(!NixHash::is_valid(""));
  }

  #[test]
  fn nix_hash_rejects_special_chars() {
    assert!(!NixHash::is_valid("abcdefghijklmnopqrstuvwxyz!@#$%^"));
  }

  #[test]
  fn nix_hash_rejects_spaces() {
    assert!(!NixHash::is_valid("abcdefghijklmnop rstuvwxyzabcdef"));
  }

  #[test]
  fn nix_hash_rejects_path_traversal_attempt() {
    assert!(!NixHash::is_valid("../../../../../../etc/passwd__"));
  }

  #[test]
  fn nix_hash_rejects_sql_injection_attempt() {
    assert!(!NixHash::is_valid("' OR 1=1; DROP TABLE builds;--"));
  }

  #[test]
  fn store_path_parse_returns_validated_type() {
    let sp = StorePath::parse(
      "/nix/store/aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa-hello-2.12",
      "/nix/store",
    );
    assert_eq!(
      sp.expect("valid store path").as_str(),
      "/nix/store/aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa-hello-2.12"
    );
  }

  #[test]
  fn nix_hash_parse_returns_validated_type() {
    let h = NixHash::parse("abcdefghijklmnopqrstuvwxyzabcdef");
    assert_eq!(
      h.expect("valid nix hash").as_str(),
      "abcdefghijklmnopqrstuvwxyzabcdef"
    );
  }
}
