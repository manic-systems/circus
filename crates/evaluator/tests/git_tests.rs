//! Tests for the git clone/fetch module.
//! Uses gix to create a temporary repository, then exercises `clone_or_fetch`.
#![expect(clippy::unwrap_used, clippy::expect_used, reason = "Fine in tests")]

use gix::{
  ObjectId,
  actor::Signature,
  date::Time,
  refs::transaction::PreviousValue,
};
use tempfile::TempDir;

fn signature(seconds: i64) -> Signature {
  Signature {
    name:  "Test".into(),
    email: "test@example.com".into(),
    time:  Time::new(seconds, 0),
  }
}

fn commit(
  repo: &gix::Repository,
  reference: &str,
  message: &str,
  seconds: i64,
  parents: &[ObjectId],
) -> ObjectId {
  let tree = repo
    .write_object(gix::objs::Tree::empty())
    .unwrap()
    .detach();
  let signature = signature(seconds);
  let mut time = gix::date::parse::TimeBuf::default();
  let signature = signature.to_ref(&mut time);
  repo
    .commit_as(
      signature,
      signature,
      reference,
      message,
      tree,
      parents.iter().copied(),
    )
    .unwrap()
    .detach()
}

#[test]
fn test_clone_or_fetch_clones_new_repo() {
  let upstream_dir = TempDir::new().unwrap();
  let work_dir = TempDir::new().unwrap();

  let upstream = gix::init(upstream_dir.path()).unwrap();
  commit(&upstream, "HEAD", "initial", 0, &[]);

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
  let upstream = gix::init(upstream_dir.path()).unwrap();
  let main = commit(&upstream, "HEAD", "main", 0, &[]);
  let next = commit(&upstream, "refs/heads/next", "next", 0, &[main]);

  let url = format!("file://{}?ref=next", upstream_dir.path().display());
  let (repo_path, resolved) = circus_evaluator::git::clone_or_fetch(
    &url,
    work_dir.path(),
    "test-project",
    None,
  )
  .expect("clone with Nix ref failed");
  let checkout = gix::open(repo_path).unwrap();

  assert_eq!(resolved, next.to_string());
  assert_eq!(checkout.head_id().unwrap().detach(), next);

  let url = format!("file://{}?rev={next}", upstream_dir.path().display());
  let (repo_path, resolved) = circus_evaluator::git::clone_or_fetch(
    &url,
    work_dir.path(),
    "rev-project",
    None,
  )
  .expect("clone with Nix revision failed");
  let checkout = gix::open(repo_path).unwrap();

  assert_eq!(resolved, next.to_string());
  assert_eq!(checkout.head_id().unwrap().detach(), next);
}

#[test]
fn test_clone_or_fetch_fetches_existing() {
  let upstream_dir = TempDir::new().unwrap();
  let work_dir = TempDir::new().unwrap();

  let upstream = gix::init(upstream_dir.path()).unwrap();
  let initial = commit(&upstream, "HEAD", "initial", 0, &[]);

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
  let second = commit(&upstream, "HEAD", "second", 0, &[initial]);

  // Second fetch
  let (_, hash2): (std::path::PathBuf, String) =
    circus_evaluator::git::clone_or_fetch(
      &url,
      work_dir.path(),
      "test-project",
      None,
    )
    .expect("second fetch failed");

  assert_eq!(hash1, initial.to_string());
  assert_eq!(hash2, second.to_string());
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
  let upstream = gix::init(upstream_dir.path()).unwrap();
  let old = commit(&upstream, "HEAD", "old", 1_000, &[]);
  let new = commit(&upstream, "HEAD", "new", 2_000, &[old]);
  for (name, target, seconds) in
    [("recent-tag", old, 3_000), ("old-tag", new, 1_500)]
  {
    let tagger = signature(seconds);
    let mut time = gix::date::parse::TimeBuf::default();
    upstream
      .tag(
        name,
        target,
        gix::objs::Kind::Commit,
        Some(tagger.to_ref(&mut time)),
        name,
        PreviousValue::MustNotExist,
      )
      .unwrap();
  }

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
