use std::path::{Path, PathBuf};

use circus_common::error::{CiError, Result};
use git2::Repository;

/// Refspecs fetched on every sync. The first is the standard branch fetch.
/// The two remaining refspecs make pull-request / merge-request commits
/// reachable so the evaluator can check them out when a webhook pushes
/// an evaluation for a PR head commit:
///
///   - `refs/pull/*/head` is GitHub, Gitea, and Forgejo's PR ref.
///   - `refs/merge-requests/*/head` is GitLab's MR ref.
///
/// Forges that don't publish these refs (Cgit, plain Git remotes) will
/// fail the fetch on these refspecs; we treat that as non-fatal.
const FETCH_REFSPECS_REQUIRED: &[&str] = &[
  "refs/heads/*:refs/remotes/origin/*",
  "refs/tags/*:refs/tags/*",
];
const FETCH_REFSPECS_OPTIONAL: &[&str] = &[
  "refs/pull/*/head:refs/remotes/origin/pr/*",
  "refs/merge-requests/*/head:refs/remotes/origin/mr/*",
];

fn fetch_all_refs(repo: &Repository) -> Result<()> {
  let mut remote = repo.find_remote("origin")?;
  remote.fetch(FETCH_REFSPECS_REQUIRED, None, None)?;
  // PR/MR refspecs are forge-specific; ignore failures so a plain Git
  // remote without pull refs still evaluates.
  for spec in FETCH_REFSPECS_OPTIONAL {
    if let Err(e) = remote.fetch(&[*spec], None, None) {
      tracing::debug!(refspec = spec, "Optional fetch failed: {e}");
    }
  }
  Ok(())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum RefKind {
  Branch,
  Tag,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiscoveredRef {
  pub kind:        RefKind,
  pub name:        String,
  pub commit_hash: String,
}

fn glob_matches(pattern: &str, value: &str) -> bool {
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

fn resolve_ref(
  repo: &Repository,
  git_ref: &str,
) -> Result<(String, git2::Oid)> {
  let reference = repo.find_reference(git_ref)?;
  let commit = reference.peel_to_commit()?;
  Ok((commit.id().to_string(), commit.id()))
}

fn clone_or_open_and_fetch(
  url: &str,
  work_dir: &Path,
  project_name: &str,
) -> Result<(PathBuf, Repository, bool)> {
  let repo_path = work_dir.join(project_name);
  let is_fetch = repo_path.exists();
  let repo = if is_fetch {
    let repo = Repository::open(&repo_path)?;
    fetch_all_refs(&repo)?;
    repo
  } else {
    let repo = Repository::clone(url, &repo_path)?;
    fetch_all_refs(&repo)?;
    repo
  };
  Ok((repo_path, repo, is_fetch))
}

/// Clone or update the repository and list refs matching the given branch
/// and/or tag glob patterns.
///
/// # Errors
///
/// Returns an error if the repository cannot be cloned/fetched or its refs
/// cannot be enumerated.
pub fn list_matching_refs(
  url: &str,
  work_dir: &Path,
  project_name: &str,
  branch_pattern: Option<&str>,
  tag_pattern: Option<&str>,
) -> Result<Vec<DiscoveredRef>> {
  let (_repo_path, repo, _is_fetch) =
    clone_or_open_and_fetch(url, work_dir, project_name)?;
  let mut refs = Vec::new();

  if let Some(pattern) = branch_pattern {
    for reference in repo.references_glob("refs/remotes/origin/*")? {
      let reference = reference?;
      let Some(name) = reference
        .name()
        .ok()
        .and_then(|name| name.strip_prefix("refs/remotes/origin/"))
      else {
        continue;
      };
      if name == "HEAD" || !glob_matches(pattern, name) {
        continue;
      }
      let commit = reference.peel_to_commit()?;
      refs.push(DiscoveredRef {
        kind:        RefKind::Branch,
        name:        name.to_string(),
        commit_hash: commit.id().to_string(),
      });
    }
  }

  if let Some(pattern) = tag_pattern {
    for reference in repo.references_glob("refs/tags/*")? {
      let reference = reference?;
      let Some(name) = reference
        .name()
        .ok()
        .and_then(|name| name.strip_prefix("refs/tags/"))
      else {
        continue;
      };
      if !glob_matches(pattern, name) {
        continue;
      }
      let commit = reference.peel_to_commit()?;
      refs.push(DiscoveredRef {
        kind:        RefKind::Tag,
        name:        name.to_string(),
        commit_hash: commit.id().to_string(),
      });
    }
  }

  refs.sort_by(|a, b| a.kind.cmp(&b.kind).then_with(|| a.name.cmp(&b.name)));
  refs.dedup_by(|a, b| a.kind == b.kind && a.name == b.name);
  Ok(refs)
}

/// Clone or fetch a repository. Returns (`repo_path`, `commit_hash`).
///
/// If `branch` is `Some`, resolve `refs/remotes/origin/<branch>` instead of
/// HEAD.
///
/// # Errors
///
/// Returns error if git operations fail.
#[tracing::instrument(skip(work_dir))]
pub fn clone_or_fetch(
  url: &str,
  work_dir: &Path,
  project_name: &str,
  branch: Option<&str>,
) -> Result<(PathBuf, String)> {
  let (repo_path, repo, is_fetch) =
    clone_or_open_and_fetch(url, work_dir, project_name)?;

  // Resolve commit from remote refs (which are always up-to-date after fetch).
  // When no branch is specified, detect the default branch from local HEAD's
  // tracking target.
  let branch_name = if let Some(b) = branch {
    b.to_string()
  } else {
    let head = repo.head()?;
    head.shorthand().unwrap_or("master").to_string()
  };

  let git_ref = if branch_name.starts_with("refs/") {
    branch_name.clone()
  } else {
    format!("refs/remotes/origin/{branch_name}")
  };
  let (hash, oid) = resolve_ref(&repo, &git_ref).map_err(|e| {
    CiError::NotFound(format!(
      "Git ref '{branch_name}' not found ({git_ref}): {e}"
    ))
  })?;
  let commit = repo.find_commit(oid)?;

  // After fetch, update the working tree so nix evaluation sees the latest
  // files. Skip on fresh clone since the checkout is already current.
  if is_fetch {
    repo.checkout_tree(
      commit.as_object(),
      Some(git2::build::CheckoutBuilder::new().force()),
    )?;
    repo.set_head_detached(commit.id())?;
  }

  Ok((repo_path, hash))
}

/// Clone or update the repository and check out the named branch or tag,
/// returning the checkout path and resolved commit hash.
///
/// # Errors
///
/// Returns an error if the repository cannot be cloned/fetched, the ref does
/// not resolve, or the checkout fails.
pub fn checkout_named_ref(
  url: &str,
  work_dir: &Path,
  project_name: &str,
  kind: RefKind,
  name: &str,
) -> Result<(PathBuf, String)> {
  let git_ref = match kind {
    RefKind::Branch => format!("refs/remotes/origin/{name}"),
    RefKind::Tag => format!("refs/tags/{name}"),
  };
  let (repo_path, repo, _is_fetch) =
    clone_or_open_and_fetch(url, work_dir, project_name)?;
  let (hash, oid) = resolve_ref(&repo, &git_ref).map_err(|e| {
    CiError::NotFound(format!("Git ref '{git_ref}' not found: {e}"))
  })?;
  let commit = repo.find_commit(oid)?;
  repo.checkout_tree(
    commit.as_object(),
    Some(git2::build::CheckoutBuilder::new().force()),
  )?;
  repo.set_head_detached(commit.id())?;
  Ok((repo_path, hash))
}

/// Fetch from origin and check out a specific commit SHA. Used to
/// evaluate a pushed PR head commit that may not be a branch tip.
///
/// The repo must already exist (callers invoke `clone_or_fetch` first to
/// establish the working tree). After this returns, `repo_path` has the
/// requested commit checked out and is ready for nix evaluation.
///
/// # Errors
///
/// Returns `NotFound` if the SHA is not reachable from any fetched ref.
#[tracing::instrument(skip(work_dir))]
pub fn fetch_and_checkout_commit(
  url: &str,
  work_dir: &Path,
  project_name: &str,
  commit_sha: &str,
) -> Result<PathBuf> {
  let repo_path = work_dir.join(project_name);

  let repo = if repo_path.exists() {
    Repository::open(&repo_path)?
  } else {
    Repository::clone(url, &repo_path)?
  };

  fetch_all_refs(&repo)?;

  let oid = git2::Oid::from_str(commit_sha).map_err(|e| {
    CiError::Validation(format!("Invalid commit SHA '{commit_sha}': {e}"))
  })?;

  let commit = repo.find_commit(oid).map_err(|e| {
    CiError::NotFound(format!(
      "Commit {commit_sha} not reachable on origin (fetched branches and \
       pull/merge-request refs): {e}"
    ))
  })?;

  repo.checkout_tree(
    commit.as_object(),
    Some(git2::build::CheckoutBuilder::new().force()),
  )?;
  repo.set_head_detached(commit.id())?;

  Ok(repo_path)
}
