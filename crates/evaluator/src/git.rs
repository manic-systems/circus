use std::{
  collections::HashSet,
  path::{Path, PathBuf},
  sync::atomic::AtomicBool,
};

use circus_common::{
  error::{CiError, Result},
  glob::glob_matches,
};
use gix::{
  ObjectId,
  Repository,
  bstr::{BStr, ByteSlice},
  progress::Discard,
  refs::{
    FullName,
    transaction::{Change, LogChange, PreviousValue, RefEdit, RefLog},
  },
  remote::{Direction, fetch::RefMap},
};
use url::Url;

/// Query parameters understood by Nix's Git flake fetcher rather than by the
/// remote Git HTTP endpoint.
const NIX_GIT_QUERY_PARAMS: &[&str] = &[
  "allRefs",
  "dir",
  "narHash",
  "ref",
  "rev",
  "shallow",
  "submodules",
];

/// Return a cloneable repository URL with Nix flake attributes removed.
///
/// Unknown query parameters remain intact because self-hosted Git servers may
/// use them for authentication or routing.
fn clone_url(source: &str) -> String {
  let Ok(mut url) = Url::parse(source) else {
    return source.to_string();
  };
  let retained = url
    .query_pairs()
    .filter(|(key, _)| !NIX_GIT_QUERY_PARAMS.contains(&key.as_ref()))
    .map(|(key, value)| (key.into_owned(), value.into_owned()))
    .collect::<Vec<_>>();
  url.set_query(None);
  if !retained.is_empty() {
    url.query_pairs_mut().extend_pairs(retained);
  }
  url.set_fragment(None);
  url.into()
}

fn source_attribute(source: &str, attribute: &str) -> Option<String> {
  Url::parse(source)
    .ok()?
    .query_pairs()
    .find_map(|(key, value)| (key == attribute).then(|| value.into_owned()))
}

/// Refspecs fetched on every sync. Pull-request and merge-request heads make
/// webhook-pushed PR commits reachable, from GitHub/Gitea/Forgejo and GitLab
/// respectively. Remotes without them simply match nothing.
const FETCH_REFSPECS: &[&str] = &[
  "+HEAD:refs/circus/remote-head",
  "+refs/heads/*:refs/remotes/origin/*",
  "+refs/tags/*:refs/tags/*",
  "+refs/pull/*/head:refs/circus/pr/*",
  "+refs/merge-requests/*/head:refs/circus/mr/*",
];

/// Local namespaces mirrored from the remote, so a ref there that the remote
/// no longer advertises is stale and a deleted branch or PR stops resolving.
const MIRRORED_NAMESPACES: &[&str] = &[
  "refs/remotes/origin/",
  "refs/tags/",
  "refs/circus/pr/",
  "refs/circus/mr/",
];

const DEFAULT_BRANCH_REF: &str = "refs/circus/remote-head";

/// Reflog entries need a committer, which the evaluator's service user rarely
/// configures.
const CONFIG_OVERRIDES: &[&str] = &[
  "gitoxide.committer.nameFallback=circus",
  "gitoxide.committer.emailFallback=circus@localhost",
];

fn open(repo_path: &Path) -> Result<Repository> {
  Ok(gix::open_opts(
    repo_path,
    gix::open::Options::default()
      .config_overrides(CONFIG_OVERRIDES.iter().copied()),
  )?)
}

fn fetch(repo: &Repository, url: &str, interrupt: &AtomicBool) -> Result<()> {
  let outcome = repo
    .remote_at(url)?
    .with_refspecs(FETCH_REFSPECS, Direction::Fetch)?
    .connect(Direction::Fetch)?
    .prepare_fetch(Discard, gix::remote::ref_map::Options::default())?
    .receive(Discard, interrupt)?;
  prune(repo, &outcome.ref_map)
}

fn prune(repo: &Repository, ref_map: &RefMap) -> Result<()> {
  if ref_map.mappings.is_empty() {
    tracing::warn!("Remote mapped no refs, skipping prune");
    return Ok(());
  }

  let fetched = ref_map
    .mappings
    .iter()
    .filter_map(|mapping| mapping.local.as_ref().map(|name| name.as_bstr()))
    .collect::<HashSet<&BStr>>();
  let mut stale = Vec::new();
  for reference in repo.references()?.all()? {
    let name = reference?.name().to_owned();
    let mirrored = MIRRORED_NAMESPACES
      .iter()
      .any(|namespace| name.as_bstr().starts_with_str(namespace));
    if mirrored && !fetched.contains(name.as_bstr()) {
      stale.push(name);
    }
  }
  repo.edit_references(stale.into_iter().map(|name| {
    RefEdit {
      change: Change::Delete {
        expected: PreviousValue::Any,
        log:      RefLog::AndReference,
      },
      name,
      deref: false,
    }
  }))?;
  Ok(())
}

fn resolve_ref(repo: &Repository, git_ref: &str) -> Result<(String, ObjectId)> {
  let id = repo.find_reference(git_ref)?.peel_to_commit()?.id;
  Ok((id.to_string(), id))
}

fn parse_commit(repo: &Repository, hash: &str) -> Result<ObjectId> {
  let id = ObjectId::from_hex(hash.as_bytes()).map_err(|error| {
    CiError::Validation(format!("Invalid commit SHA '{hash}': {error}"))
  })?;
  repo.find_commit(id).map_err(|error| {
    CiError::NotFound(format!(
      "Commit {hash} not reachable on origin (fetched branches and \
       pull/merge-request refs): {error}"
    ))
  })?;
  Ok(id)
}

/// Force the worktree, index, and a detached `HEAD` to `commit`, removing
/// tracked files the previous checkout had and `commit` lacks.
fn checkout(repo: &Repository, commit: ObjectId) -> Result<()> {
  // gix rewrites every file on checkout and every poll lands here. `HEAD`
  // moves last, so an interrupted checkout is still redone.
  if repo.head_id().is_ok_and(|head| head == commit) {
    return Ok(());
  }

  let workdir = repo.workdir().ok_or_else(|| {
    CiError::Internal(format!(
      "Repository {} has no worktree",
      repo.git_dir().display()
    ))
  })?;
  let tree = repo
    .find_commit(commit)?
    .tree_id()
    .map_err(gix::Error::from)?;
  let mut index = repo.index_from_tree(&tree)?;

  if let Some(previous) = repo.try_index()? {
    for entry in previous.entries() {
      let path = entry.path(&previous);
      if index.entry_by_path(path).is_some() {
        continue;
      }
      match std::fs::remove_file(workdir.join(gix::path::from_bstr(path))) {
        Err(error) if error.kind() != std::io::ErrorKind::NotFound => {
          return Err(error.into());
        },
        _ => {},
      }
    }
  }

  let mut options = repo.checkout_options(
    gix::worktree::stack::state::attributes::Source::IdMapping,
  )?;
  options.overwrite_existing = true;
  gix::worktree::state::checkout(
    &mut index,
    workdir,
    repo.objects.clone().into_arc()?,
    &Discard,
    &Discard,
    &AtomicBool::new(false),
    options,
  )
  .map_err(gix::Error::from)?;
  index
    .write(gix::index::write::Options::default())
    .map_err(gix::Error::from)?;

  repo.edit_reference(RefEdit {
    change: Change::Update {
      log:      LogChange::default(),
      expected: PreviousValue::Any,
      new:      gix::refs::Target::Object(commit),
    },
    name:   FullName::try_from("HEAD").map_err(gix::Error::from_error)?,
    deref:  false,
  })?;
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
  pub ref_time:    i64,
}

pub fn retain_newest_tag(refs: &mut Vec<DiscoveredRef>) {
  let newest_tag = refs
    .iter()
    .filter(|git_ref| git_ref.kind == RefKind::Tag)
    .max_by(|a, b| {
      a.ref_time
        .cmp(&b.ref_time)
        .then_with(|| a.name.cmp(&b.name))
    })
    .cloned();
  refs.retain(|git_ref| git_ref.kind == RefKind::Branch);
  refs.extend(newest_tag);
}

fn clone_or_open_and_fetch(
  url: &str,
  work_dir: &Path,
  project_name: &str,
  interrupt: &AtomicBool,
) -> Result<(PathBuf, Repository)> {
  let repo_path = work_dir.join(project_name);
  if !repo_path.exists() {
    gix::init(&repo_path)?;
  }
  let repo = open(&repo_path)?;
  fetch(&repo, &clone_url(url), interrupt)?;
  Ok((repo_path, repo))
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
  let (_repo_path, repo) = clone_or_open_and_fetch(
    url,
    work_dir,
    project_name,
    &AtomicBool::new(false),
  )?;
  let mut refs = Vec::new();

  if let Some(pattern) = branch_pattern {
    for reference in repo.references()?.prefixed("refs/remotes/origin/")? {
      let mut reference = reference?;
      let name = reference
        .name()
        .as_bstr()
        .strip_prefix(b"refs/remotes/origin/")
        .unwrap_or_default()
        .to_str_lossy()
        .into_owned();
      if name == "HEAD" || !glob_matches(pattern, &name) {
        continue;
      }
      let commit = reference.peel_to_commit()?;
      refs.push(DiscoveredRef {
        kind: RefKind::Branch,
        name,
        commit_hash: commit.id.to_string(),
        ref_time: commit.time()?.seconds,
      });
    }
  }

  if let Some(pattern) = tag_pattern {
    for reference in repo.references()?.prefixed("refs/tags/")? {
      let mut reference = reference?;
      let name = reference
        .name()
        .as_bstr()
        .strip_prefix(b"refs/tags/")
        .unwrap_or_default()
        .to_str_lossy()
        .into_owned();
      if !glob_matches(pattern, &name) {
        continue;
      }
      let tagger_time = reference
        .try_id()
        .and_then(|id| id.object().ok()?.try_into_tag().ok())
        .and_then(|tag| Some(tag.tagger().ok()??.time().ok()?.seconds));
      let commit = reference.peel_to_commit()?;
      let ref_time = match tagger_time {
        Some(time) => time,
        None => commit.time()?.seconds,
      };
      refs.push(DiscoveredRef {
        kind: RefKind::Tag,
        name,
        commit_hash: commit.id.to_string(),
        ref_time,
      });
    }
  }

  refs.sort_by(|a, b| a.kind.cmp(&b.kind).then_with(|| a.name.cmp(&b.name)));
  refs.dedup_by(|a, b| a.kind == b.kind && a.name == b.name);
  Ok(refs)
}

/// Resolve the commit a source evaluation tracks, honoring the URL's `rev` and
/// `ref` attributes before falling back to the default branch.
fn resolve_source_commit(
  repo: &Repository,
  url: &str,
  branch: Option<&str>,
) -> Result<(String, ObjectId)> {
  // Resolve commit from remote refs (which are always up-to-date after fetch).
  // When no branch is specified, use the remote's default branch.
  if let Some(rev) = source_attribute(url, "rev") {
    let oid = parse_commit(repo, &rev)?;
    Ok((oid.to_string(), oid))
  } else {
    let git_ref = match (branch, source_attribute(url, "ref")) {
      (Some(branch), _) => format!("refs/remotes/origin/{branch}"),
      (None, Some(source_ref)) if source_ref.starts_with("refs/") => source_ref,
      (None, Some(source_ref)) => format!("refs/remotes/origin/{source_ref}"),
      (None, None) => DEFAULT_BRANCH_REF.to_owned(),
    };
    resolve_ref(repo, &git_ref).map_err(|error| {
      CiError::NotFound(format!("Git ref '{git_ref}' not found: {error}"))
    })
  }
}

fn contains_commit(
  repo: &Repository,
  tip: ObjectId,
  commit: &str,
) -> Result<bool> {
  let commit =
    ObjectId::from_hex(commit.as_bytes()).map_err(gix::Error::from)?;
  if commit == tip {
    return Ok(true);
  }
  // Fetching never deletes objects, so a missing commit was never fetched and
  // cannot be in the history of any fetched ref.
  if !repo.has_object(commit) {
    return Ok(false);
  }
  let mut graph = repo.revision_graph(None);
  let bases = gix::revision::plumbing::merge_base(tip, &[commit], &mut graph)
    .map_err(gix::Error::from)?;
  Ok(bases.is_some_and(|bases| bases.contains(&commit)))
}

/// Whether `commit` is `tip` or one of its ancestors in the checkout at
/// `repo_path`.
///
/// # Errors
///
/// Returns an error if the repository cannot be opened or either hash is
/// invalid.
pub fn history_contains(
  repo_path: &Path,
  tip: &str,
  commit: &str,
) -> Result<bool> {
  let repo = open(repo_path)?;
  let tip = ObjectId::from_hex(tip.as_bytes()).map_err(gix::Error::from)?;
  contains_commit(&repo, tip, commit)
}

/// The first line of the message of commit `hash` in the checkout at
/// `repo_path`.
///
/// # Errors
///
/// Returns an error if the repository cannot be opened or the commit is
/// missing.
pub fn commit_subject(repo_path: &Path, hash: &str) -> Result<String> {
  let repo = open(repo_path)?;
  let commit = repo.find_commit(parse_commit(&repo, hash)?)?;
  let message = commit.message().map_err(gix::Error::from)?;
  Ok(message.summary().to_str_lossy().into_owned())
}

/// Whether the repository URL pins a revision, which no branch can rewrite.
#[must_use]
pub fn pins_revision(url: &str) -> bool {
  source_attribute(url, "rev").is_some()
}

/// Fetch origin and report whether `commit` is still in the history of
/// `branch`, or of the source `clone_or_fetch` resolves when `branch` is
/// `None`. Setting `interrupt` aborts the fetch.
///
/// # Errors
///
/// Returns an error if the fetch fails or the ref does not resolve.
pub fn source_contains_commit(
  url: &str,
  work_dir: &Path,
  project_name: &str,
  branch: Option<&str>,
  commit: &str,
  interrupt: &AtomicBool,
) -> Result<bool> {
  let (_repo_path, repo) =
    clone_or_open_and_fetch(url, work_dir, project_name, interrupt)?;
  let tip = match branch {
    Some(branch) => {
      let git_ref = format!("refs/remotes/origin/{branch}");
      resolve_ref(&repo, &git_ref).map_err(|error| {
        CiError::NotFound(format!("Git ref '{git_ref}' not found: {error}"))
      })?
    },
    None => resolve_source_commit(&repo, url, None)?,
  };
  contains_commit(&repo, tip.1, commit)
}

/// Clone or fetch a repository. Returns (`repo_path`, `commit_hash`).
///
/// If `branch` is `Some`, resolve `refs/remotes/origin/<branch>` instead of
/// the remote's default branch.
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
  let (repo_path, repo) = clone_or_open_and_fetch(
    url,
    work_dir,
    project_name,
    &AtomicBool::new(false),
  )?;
  let (hash, oid) = resolve_source_commit(&repo, url, branch)?;

  // The requested ref may differ from the remote's default branch, including
  // on a fresh clone, so always align the checkout with the resolved commit.
  checkout(&repo, oid)?;
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
  let (repo_path, repo) = clone_or_open_and_fetch(
    url,
    work_dir,
    project_name,
    &AtomicBool::new(false),
  )?;
  let (hash, oid) = resolve_ref(&repo, &git_ref).map_err(|e| {
    CiError::NotFound(format!("Git ref '{git_ref}' not found: {e}"))
  })?;
  checkout(&repo, oid)?;
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
  let (repo_path, repo) = clone_or_open_and_fetch(
    url,
    work_dir,
    project_name,
    &AtomicBool::new(false),
  )?;
  checkout(&repo, parse_commit(&repo, commit_sha)?)?;
  Ok(repo_path)
}
