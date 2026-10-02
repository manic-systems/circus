//! Forge coordinates parsed from a project's repository URL.

use std::str::FromStr;

/// Owner, name and Hercules-style project path of a hosted repository.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RepositoryCoordinates {
  /// `github`, `gitlab`, or the host name of any other forge.
  pub site:         String,
  /// `<site>/<owner>/<repo>`, with GitLab subgroups kept in between.
  pub project_path: String,
  pub owner:        String,
  pub repo:         String,
}

/// The URL is a local path or has no `owner/repo` part.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("repository URL does not identify an owner and repository")]
pub struct UnidentifiableRepository;

impl FromStr for RepositoryCoordinates {
  type Err = UnidentifiableRepository;

  fn from_str(raw: &str) -> Result<Self, Self::Err> {
    let raw = raw.trim();
    let (host, path) = if let Ok(url) = url::Url::parse(raw) {
      let host = url.host_str().ok_or(UnidentifiableRepository)?;
      (host.to_ascii_lowercase(), url.path().to_owned())
    } else {
      let (prefix, path) =
        raw.split_once(':').ok_or(UnidentifiableRepository)?;
      (
        prefix
          .rsplit_once('@')
          .map_or(prefix, |(_, host)| host)
          .to_ascii_lowercase(),
        path.to_owned(),
      )
    };
    let path = path
      .trim_start_matches('/')
      .trim_end_matches('/')
      .trim_end_matches(".git");
    let parts = path
      .split('/')
      .filter(|part| !part.is_empty())
      .collect::<Vec<_>>();
    let [owner, .., repo] = parts.as_slice() else {
      return Err(UnidentifiableRepository);
    };
    let site = match host.as_str() {
      "github.com" => "github",
      "gitlab.com" => "gitlab",
      _ => host.as_str(),
    };
    Ok(Self {
      project_path: format!("{site}/{}", parts.join("/")),
      site:         site.to_owned(),
      owner:        (*owner).to_owned(),
      repo:         (*repo).to_owned(),
    })
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn coordinates_build_hercules_project_paths() {
    let github = "https://github.com/owner/repo.git"
      .parse::<RepositoryCoordinates>()
      .expect("valid GitHub repository URL");
    assert_eq!(github.project_path, "github/owner/repo");
    assert_eq!(github.site, "github");
    assert_eq!(github.owner, "owner");
    assert_eq!(github.repo, "repo");

    let gitlab = "git@gitlab.com:group/subgroup/repo.git"
      .parse::<RepositoryCoordinates>()
      .expect("valid GitLab repository URL");
    assert_eq!(gitlab.project_path, "gitlab/group/subgroup/repo");
    assert_eq!(gitlab.owner, "group");
    assert_eq!(gitlab.repo, "repo");

    assert_eq!(
      "/srv/repos/infra".parse::<RepositoryCoordinates>(),
      Err(UnidentifiableRepository)
    );
  }
}
