use std::path::{Path, PathBuf};

use circus_common::models::{ActiveJobset, Evaluation};
use git2::{DiffOptions, Oid, Repository};

fn matching_path_changed(
  repo_path: &Path,
  base_commit: &str,
  commit: &str,
  path_filters: &[String],
) -> Result<bool, git2::Error> {
  let repo = Repository::open(repo_path)?;
  let base = repo.find_commit(Oid::from_str(base_commit)?)?;
  let head = repo.find_commit(Oid::from_str(commit)?)?;
  let base_tree = base.tree()?;
  let head_tree = head.tree()?;
  let mut options = DiffOptions::new();
  for path_filter in path_filters {
    options.pathspec(path_filter);
    if let Some(root_pattern) = path_filter.strip_prefix("**/") {
      options.pathspec(root_pattern);
    }
  }
  let diff = repo.diff_tree_to_tree(
    Some(&base_tree),
    Some(&head_tree),
    Some(&mut options),
  )?;
  Ok(diff.deltas().next().is_some())
}

pub async fn should_evaluate(
  repo_path: &Path,
  evaluation: &Evaluation,
  jobset: &ActiveJobset,
) -> bool {
  if jobset.path_filters.is_empty() {
    return true;
  }
  let Some(base_commit) = evaluation.source_base_commit.clone() else {
    return true;
  };

  let repo_path = PathBuf::from(repo_path);
  let commit = evaluation.commit_hash.clone();
  let path_filters = jobset.path_filters.clone();
  match tokio::task::spawn_blocking(move || {
    matching_path_changed(&repo_path, &base_commit, &commit, &path_filters)
  })
  .await
  {
    Ok(Ok(matches)) => matches,
    Ok(Err(error)) => {
      tracing::warn!(
        eval_id = %evaluation.id,
        "Could not compare source paths; evaluating conservatively: {error}"
      );
      true
    },
    Err(error) => {
      tracing::warn!(
        eval_id = %evaluation.id,
        "Source path comparison task failed; evaluating conservatively: {error}"
      );
      true
    },
  }
}

#[cfg(test)]
mod tests {
  use std::{fs, path::Path};

  use git2::{IndexAddOption, Oid, Repository, Signature};
  use tempfile::TempDir;

  use super::matching_path_changed;

  fn commit(repo: &Repository, root: &Path, path: &str, content: &str) -> Oid {
    let file = root.join(path);
    fs::create_dir_all(file.parent().expect("test path has a parent"))
      .expect("create test directory");
    fs::write(file, content).expect("write test file");
    let mut index = repo.index().expect("open index");
    index
      .add_all(["*"], IndexAddOption::DEFAULT, None)
      .expect("stage files");
    index.write().expect("write index");
    let tree_id = index.write_tree().expect("write tree");
    let tree = repo.find_tree(tree_id).expect("find tree");
    let signature =
      Signature::now("Test", "test@example.com").expect("signature");
    let parent = repo.head().ok().and_then(|head| head.peel_to_commit().ok());
    let parents = parent.iter().collect::<Vec<_>>();
    repo
      .commit(Some("HEAD"), &signature, &signature, path, &tree, &parents)
      .expect("commit")
  }

  #[test]
  fn git_pathspecs_gate_directory_and_glob_changes() {
    let dir = TempDir::new().expect("tempdir");
    let repo = Repository::init(dir.path()).expect("init repo");
    let base = commit(&repo, dir.path(), "README.md", "initial");
    let unrelated = commit(&repo, dir.path(), "README.md", "unrelated");
    let nested = commit(
      &repo,
      dir.path(),
      "packages/hardened-kernel/default.nix",
      "kernel",
    );
    let root = commit(&repo, dir.path(), "flake.nix", "flake");

    assert!(
      !matching_path_changed(
        dir.path(),
        &base.to_string(),
        &unrelated.to_string(),
        &["packages/hardened-kernel".to_string()],
      )
      .expect("compare unrelated change")
    );
    assert!(
      matching_path_changed(
        dir.path(),
        &unrelated.to_string(),
        &nested.to_string(),
        &["packages/hardened-kernel".to_string()],
      )
      .expect("compare directory change")
    );
    assert!(
      matching_path_changed(
        dir.path(),
        &unrelated.to_string(),
        &nested.to_string(),
        &["packages/hardened-kernel/default.nix".to_string()],
      )
      .expect("compare exact file change")
    );
    assert!(
      matching_path_changed(
        dir.path(),
        &nested.to_string(),
        &root.to_string(),
        &["**/*.nix".to_string()],
      )
      .expect("compare root glob change")
    );
  }
}
