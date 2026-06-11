//! Minimal glob matching for ref patterns.

/// Match `value` against `pattern`, where `*` matches any run and `?` one char.
#[must_use]
pub fn glob_matches(pattern: &str, value: &str) -> bool {
  fn inner(pattern: &[u8], value: &[u8]) -> bool {
    match pattern {
      [] => value.is_empty(),
      [b'*', rest @ ..] => {
        inner(rest, value) || (!value.is_empty() && inner(pattern, &value[1..]))
      },
      [b'?', rest @ ..] => !value.is_empty() && inner(rest, &value[1..]),
      [ch, rest @ ..] => {
        value.first().is_some_and(|v| v == ch) && inner(rest, &value[1..])
      },
    }
  }
  inner(pattern.as_bytes(), value.as_bytes())
}

#[cfg(test)]
mod tests {
  use super::glob_matches;

  #[test]
  fn matches_wildcards_and_single_chars() {
    assert!(glob_matches("release-*", "release-2026"));
    assert!(!glob_matches("release-*", "main"));
    assert!(glob_matches("v1.?", "v1.0"));
    assert!(!glob_matches("v1.?", "v1.10"));
    assert!(glob_matches("*", ""));
    assert!(!glob_matches("", "x"));
  }
}
