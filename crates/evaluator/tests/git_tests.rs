//! Tests for the git clone/fetch module.
//! Uses git2 to create a temporary repository, then exercises `clone_or_fetch`.
#![expect(clippy::unwrap_used, clippy::expect_used, reason = "Fine in tests")]

use git2::{Repository, Signature, Time};
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
fn nix_ref_and_rev_queries_select_non_default_commits() {
  let upstream_dir = TempDir::new().unwrap();
  let work_dir = TempDir::new().unwrap();
  let upstream = Repository::init(upstream_dir.path()).unwrap();
  let sig = Signature::now("Test", "test@example.com").unwrap();
  let tree_id = upstream.index().unwrap().write_tree().unwrap();
  let tree = upstream.find_tree(tree_id).unwrap();
  let main = upstream
    .commit(Some("HEAD"), &sig, &sig, "main", &tree, &[])
    .unwrap();
  let main = upstream.find_commit(main).unwrap();
  let next = upstream
    .commit(Some("refs/heads/next"), &sig, &sig, "next", &tree, &[&main])
    .unwrap();
  drop(tree);
  drop(main);

  let url = format!("file://{}?ref=next", upstream_dir.path().display());
  let (repo_path, resolved) = circus_evaluator::git::clone_or_fetch(
    &url,
    work_dir.path(),
    "test-project",
    None,
  )
  .expect("clone with Nix ref failed");
  let checkout = Repository::open(repo_path).unwrap();

  assert_eq!(resolved, next.to_string());
  assert_eq!(checkout.head().unwrap().target(), Some(next));

  let url = format!("file://{}?rev={next}", upstream_dir.path().display());
  let (repo_path, resolved) = circus_evaluator::git::clone_or_fetch(
    &url,
    work_dir.path(),
    "rev-project",
    None,
  )
  .expect("clone with Nix revision failed");
  let checkout = Repository::open(repo_path).unwrap();

  assert_eq!(resolved, next.to_string());
  assert_eq!(checkout.head().unwrap().target(), Some(next));
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

#[test]
fn newest_annotated_tag_uses_tagger_time() {
  let upstream_dir = TempDir::new().unwrap();
  let work_dir = TempDir::new().unwrap();
  let upstream = Repository::init(upstream_dir.path()).unwrap();
  let old_sig =
    Signature::new("Test", "test@example.com", &Time::new(1_000, 0)).unwrap();
  let new_sig =
    Signature::new("Test", "test@example.com", &Time::new(2_000, 0)).unwrap();
  let tree_id = upstream.index().unwrap().write_tree().unwrap();
  let tree = upstream.find_tree(tree_id).unwrap();
  let old_id = upstream
    .commit(Some("HEAD"), &old_sig, &old_sig, "old", &tree, &[])
    .unwrap();
  let old = upstream.find_commit(old_id).unwrap();
  let new_id = upstream
    .commit(Some("HEAD"), &new_sig, &new_sig, "new", &tree, &[&old])
    .unwrap();
  let new = upstream.find_commit(new_id).unwrap();
  let recent_tagger =
    Signature::new("Test", "test@example.com", &Time::new(3_000, 0)).unwrap();
  let old_tagger =
    Signature::new("Test", "test@example.com", &Time::new(1_500, 0)).unwrap();
  upstream
    .tag(
      "recent-tag",
      old.as_object(),
      &recent_tagger,
      "recent tag on old commit",
      false,
    )
    .unwrap();
  upstream
    .tag(
      "old-tag",
      new.as_object(),
      &old_tagger,
      "old tag on new commit",
      false,
    )
    .unwrap();

  let url = format!("file://{}", upstream_dir.path().display());
  let mut refs = circus_evaluator::git::list_matching_refs(
    &url,
    work_dir.path(),
    "tag-project",
    None,
    Some("*"),
  )
  .unwrap();
  circus_evaluator::git::retain_newest_tag(&mut refs);

  assert_eq!(refs.len(), 1);
  assert_eq!(refs[0].name, "recent-tag");
}
