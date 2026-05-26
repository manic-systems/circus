use std::path::{Path, PathBuf};

use circus_common::error::Result;
use git2::Repository;

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
  let repo_path = work_dir.join(project_name);

  let is_fetch = repo_path.exists();

  let repo = if is_fetch {
    let repo = Repository::open(&repo_path)?;
    // Fetch origin. Scope the borrow so `remote` is dropped before we move
    // `repo`
    {
      let mut remote = repo.find_remote("origin")?;
      remote.fetch(&["refs/heads/*:refs/remotes/origin/*"], None, None)?;
    }
    repo
  } else {
    Repository::clone(url, &repo_path)?
  };

  // Resolve commit from remote refs (which are always up-to-date after fetch).
  // When no branch is specified, detect the default branch from local HEAD's
  // tracking target.
  let branch_name = if let Some(b) = branch {
    b.to_string()
  } else {
    let head = repo.head()?;
    head.shorthand().unwrap_or("master").to_string()
  };

  let remote_ref = format!("refs/remotes/origin/{branch_name}");
  let reference = repo.find_reference(&remote_ref).map_err(|e| {
    circus_common::error::CiError::NotFound(format!(
      "Branch '{branch_name}' not found ({remote_ref}): {e}"
    ))
  })?;
  let commit = reference.peel_to_commit()?;
  let hash = commit.id().to_string();

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
