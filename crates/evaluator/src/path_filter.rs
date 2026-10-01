use std::path::{Path, PathBuf};

use circus_common::models::{ActiveJobset, Evaluation};
use gix::{
  ObjectId,
  pathspec::{Defaults, Search},
};

fn matching_path_changed(
  repo_path: &Path,
  base_commit: &str,
  commit: &str,
  path_filters: &[String],
) -> gix::Result<bool> {
  let repo = gix::open(repo_path)?;
  let tree = |hash: &str| -> gix::Result<_> {
    let id = ObjectId::from_hex(hash.as_bytes()).map_err(gix::Error::from)?;
    repo.find_commit(id)?.tree()
  };
  let base_tree = tree(base_commit)?;
  let head_tree = tree(commit)?;

  let mut patterns = Vec::new();
  for path_filter in path_filters {
    patterns.push(path_filter.as_str());
    if let Some(root_pattern) = path_filter.strip_prefix("**/") {
      patterns.push(root_pattern);
    }
  }
  let patterns = patterns
    .into_iter()
    .map(|pattern| {
      gix::pathspec::parse(pattern.as_bytes(), Defaults::default())
    })
    .collect::<Result<Vec<_>, _>>()
    .map_err(gix::Error::from)?;
  let mut search = Search::from_specs(patterns, None, Path::new(""))
    .map_err(gix::Error::from)?;

  // Without rename tracking a move reports both paths, matching libgit2's
  // default and gating on a filter that only names the source.
  let changes = repo.diff_tree_to_tree(
    &base_tree,
    &head_tree,
    gix::diff::Options::default(),
  )?;
  Ok(changes.iter().any(|change| {
    search
      .pattern_matching_relative_path(
        change.location(),
        Some(false),
        &mut |_, _, _, _| false,
      )
      .is_some_and(|matched| !matched.is_excluded())
  }))
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
  use gix::{ObjectId, actor::Signature, date::Time, objs::tree::EntryKind};
  use tempfile::TempDir;

  use super::matching_path_changed;

  fn commit(repo: &gix::Repository, path: &str, content: &str) -> ObjectId {
    let parent = repo.head_id().ok().map(gix::Id::detach);
    let base_tree = parent.map_or_else(
      || {
        repo
          .write_object(gix::objs::Tree::empty())
          .expect("empty tree")
          .detach()
      },
      |parent| {
        repo
          .find_commit(parent)
          .expect("parent")
          .tree_id()
          .expect("tree")
          .detach()
      },
    );
    let blob = repo.write_blob(content).expect("write blob").detach();
    let mut editor = repo.edit_tree(base_tree).expect("edit tree");
    editor
      .upsert(path, EntryKind::Blob, blob)
      .expect("stage file");
    let tree = editor.write().expect("write tree").detach();
    let signature = Signature {
      name:  "Test".into(),
      email: "test@example.com".into(),
      time:  Time::new(0, 0),
    };
    let mut time = gix::date::parse::TimeBuf::default();
    let signature = signature.to_ref(&mut time);
    repo
      .commit_as(signature, signature, "HEAD", path, tree, parent)
      .expect("commit")
      .detach()
  }

  #[test]
  fn git_pathspecs_gate_directory_and_glob_changes() {
    let dir = TempDir::new().expect("tempdir");
    let repo = gix::init(dir.path()).expect("init repo");
    let base = commit(&repo, "README.md", "initial");
    let unrelated = commit(&repo, "README.md", "unrelated");
    let nested =
      commit(&repo, "packages/hardened-kernel/default.nix", "kernel");
    let root = commit(&repo, "flake.nix", "flake");
    let source = commit(&repo, "src/a/b.nix", "source");

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
    assert!(
      matching_path_changed(
        dir.path(),
        &root.to_string(),
        &source.to_string(),
        &["src/*.nix".to_string()],
      )
      .expect("compare glob crossing a directory separator")
    );
  }
}
