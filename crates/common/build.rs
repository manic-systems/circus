use std::{env, path::PathBuf, process::Command};

fn main() {
  println!("cargo:rerun-if-env-changed=CIRCUS_BUILD_SHA");

  if let Some(sha) = build_sha() {
    println!("cargo:rustc-env=BUILD_SHA={sha}");
  }
}

/// Nix builds have no git metadata in the source, so the flake passes its
/// revision instead.
fn build_sha() -> Option<String> {
  if let Some(sha) = env::var_os("CIRCUS_BUILD_SHA") {
    let sha = sha.to_string_lossy().trim().to_owned();
    return (!sha.is_empty()).then_some(sha);
  }
  read_git_sha()
}

fn read_git_sha() -> Option<String> {
  let manifest_dir = PathBuf::from(env::var_os("CARGO_MANIFEST_DIR")?);
  let workspace_root = manifest_dir.parent()?.parent()?;
  if !workspace_root.join(".git").exists() {
    return None;
  }

  println!(
    "cargo:rerun-if-changed={}",
    workspace_root.join(".git/HEAD").display()
  );

  let output = Command::new("git")
    .args(["rev-parse", "--short=12", "HEAD"])
    .current_dir(workspace_root)
    .output()
    .ok()?;
  if !output.status.success() {
    return None;
  }

  let sha = String::from_utf8_lossy(&output.stdout).trim().to_owned();
  (!sha.is_empty()).then_some(sha)
}
