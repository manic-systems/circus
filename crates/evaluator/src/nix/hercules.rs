//! Jobs from a flake's `herculesCI` attribute, Hercules CI's job interface.

use circus_common::repository::RepositoryCoordinates;
use serde::Serialize;

/// Selected by setting a flake jobset's `nix_expression` to this.
pub const EXPRESSION: &str = "herculesCI";

pub const NIX: &str = include_str!("hercules.nix");

/// Argument name for hercules.nix, kept distinct so evix does not auto-pass it
/// to deeper functions.
pub const ARGUMENT: &str = "circusHerculesJson";

/// Job name of the marker derivation carrying schedule specs in
/// meta.description.
pub const SCHEDULE_MARKER: &str = "__circusSchedules";

/// The checked-out ref, as Hercules exposes it to `herculesCI` functions.
#[derive(Debug, Clone, Copy, Default)]
pub struct GitRef<'a> {
  pub branch:   Option<&'a str>,
  pub tag:      Option<&'a str>,
  /// Evaluate this `onSchedule` job instead of `onPush`.
  pub schedule: Option<&'a str>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Context<'a> {
  flake_ref:       &'a str,
  schedule:        Option<&'a str>,
  schedule_marker: &'static str,
  primary_repo:    PrimaryRepo<'a>,
  #[serde(rename = "herculesCI")]
  hercules_ci:     HerculesCi,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct PrimaryRepo<'a> {
  #[serde(rename = "ref")]
  git_ref:         String,
  branch:          Option<&'a str>,
  tag:             Option<&'a str>,
  rev:             &'a str,
  short_rev:       &'a str,
  web_url:         Option<String>,
  remote_http_url: Option<&'a str>,
  remote_ssh_url:  Option<&'a str>,
  forge_type:      Option<String>,
  owner:           Option<String>,
  name:            Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct HerculesCi {
  api_base_url: Option<String>,
}

/// JSON for [`ARGUMENT`], describing the commit being evaluated.
///
/// # Errors
///
/// Returns an error if serialization fails.
pub fn context_json(
  flake_ref: &str,
  repository_url: &str,
  rev: &str,
  git_ref: GitRef<'_>,
) -> serde_json::Result<String> {
  let coordinates = repository_url.parse::<RepositoryCoordinates>().ok();
  let http = repository_url.starts_with("https://")
    || repository_url.starts_with("http://");
  let full_ref = match (git_ref.branch, git_ref.tag) {
    (_, Some(tag)) => format!("refs/tags/{tag}"),
    (Some(branch), None) => format!("refs/heads/{branch}"),
    (None, None) => rev.to_owned(),
  };
  serde_json::to_string(&Context {
    flake_ref,
    schedule: git_ref.schedule,
    schedule_marker: SCHEDULE_MARKER,
    primary_repo: PrimaryRepo {
      git_ref: full_ref,
      branch: git_ref.branch,
      tag: git_ref.tag,
      rev,
      short_rev: rev.get(..7).unwrap_or(rev),
      web_url: http.then(|| {
        repository_url
          .trim_end_matches('/')
          .trim_end_matches(".git")
          .to_owned()
      }),
      remote_http_url: http.then_some(repository_url),
      remote_ssh_url: (!http).then_some(repository_url),
      forge_type: coordinates.as_ref().map(|c| c.site.clone()),
      owner: coordinates.as_ref().map(|c| c.owner.clone()),
      name: coordinates.map(|c| c.repo),
    },
    hercules_ci: HerculesCi { api_base_url: None },
  })
}
