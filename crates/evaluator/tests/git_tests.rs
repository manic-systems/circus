//! Tests for the git clone/fetch module.
//! Uses git2 to create a temporary repository, then exercises `clone_or_fetch`.

use git2::{Repository, Signature};
use tempfile::TempDir;

#[test]
fn test_clone_or_fetch_clones_new_repo() {
  let upstream_dir = TempDir::new().unwrap();
  let work_dir = TempDir::new().unwrap();

  // Create a non-bare repo to clone from (bare repos have no HEAD by default)
  let upstream = Repository::init(upstream_dir.path()).unwrap();
  // Create initial commit
  {
    let sig = Signature::now("Test", "test@example.com").unwrap();
    let tree_id = upstream.index().unwrap().write_tree().unwrap();
    let tree = upstream.find_tree(tree_id).unwrap();
    upstream
      .commit(Some("HEAD"), &sig, &sig, "initial", &tree, &[])
      .unwrap();
  }

  let url = format!("file://{}", upstream_dir.path().display());
  let result = circus_evaluator::git::clone_or_fetch(
    &url,
    work_dir.path(),
    "test-project",
    None,
  );

  assert!(
    result.is_ok(),
    "clone_or_fetch should succeed: {:?}",
    result.err()
  );
  let (repo_path, hash): (std::path::PathBuf, String) = result.unwrap();
  assert!(repo_path.exists());
  assert!(!hash.is_empty());
  assert_eq!(hash.len(), 40); // full SHA-1
}

#[test]
fn test_clone_or_fetch_fetches_existing() {
  let upstream_dir = TempDir::new().unwrap();
  let work_dir = TempDir::new().unwrap();

  let upstream = Repository::init(upstream_dir.path()).unwrap();
  {
    let sig = Signature::now("Test", "test@example.com").unwrap();
    let tree_id = upstream.index().unwrap().write_tree().unwrap();
    let tree = upstream.find_tree(tree_id).unwrap();
    upstream
      .commit(Some("HEAD"), &sig, &sig, "initial", &tree, &[])
      .unwrap();
  }

  let url = format!("file://{}", upstream_dir.path().display());

  // First clone
  let (_, hash1): (std::path::PathBuf, String) =
    circus_evaluator::git::clone_or_fetch(
      &url,
      work_dir.path(),
      "test-project",
      None,
    )
    .expect("first clone failed");

  // Make another commit upstream
  {
    let sig = Signature::now("Test", "test@example.com").unwrap();
    let tree_id = upstream.index().unwrap().write_tree().unwrap();
    let tree = upstream.find_tree(tree_id).unwrap();
    let head = upstream.head().unwrap().peel_to_commit().unwrap();
    upstream
      .commit(Some("HEAD"), &sig, &sig, "second", &tree, &[&head])
      .unwrap();
  }

  // Second fetch
  let (_, hash2): (std::path::PathBuf, String) =
    circus_evaluator::git::clone_or_fetch(
      &url,
      work_dir.path(),
      "test-project",
      None,
    )
    .expect("second fetch failed");

  assert!(!hash1.is_empty());
  assert!(!hash2.is_empty());
}

#[test]
fn test_clone_invalid_url_returns_error() {
  let work_dir = TempDir::new().unwrap();
  let result = circus_evaluator::git::clone_or_fetch(
    "file:///nonexistent/repo",
    work_dir.path(),
    "bad-proj",
    None,
  );
  assert!(result.is_err());
}
