//! Build identity shared by every circus binary.

use std::sync::LazyLock;

/// Release version from the workspace manifest.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Short commit the binary was built from. This is absent when built outside a
/// git checkout without the flake passing its revision.
pub const SHA: Option<&str> = option_env!("BUILD_SHA");

static LONG: LazyLock<String> = LazyLock::new(|| {
  SHA.map_or_else(|| VERSION.to_owned(), |sha| format!("{VERSION} ({sha})"))
});

/// Version with the build commit when one is known.
#[must_use]
pub fn long() -> &'static str {
  &LONG
}
