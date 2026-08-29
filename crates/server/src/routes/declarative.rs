use circus_common::{Jobset, Project};
use uuid::Uuid;

use crate::{error::ApiError, state::AppState};

#[must_use]
pub const fn project_is_mutable(state: &AppState, project: &Project) -> bool {
  !project.managed_declaratively
    || match project.allow_runtime_mutation {
      Some(allow) => allow,
      None => state.config.declarative.allow_runtime_mutation,
    }
}

pub async fn require_project_mutable(
  state: &AppState,
  project_id: Uuid,
) -> Result<Project, ApiError> {
  let project =
    circus_common::repo::projects::get(&state.pool, project_id).await?;
  if !project_is_mutable(state, &project) {
    return Err(ApiError(circus_common::CiError::Conflict(
      "Project is managed by declarative configuration".to_string(),
    )));
  }
  Ok(project)
}

pub async fn require_jobset_mutable(
  state: &AppState,
  jobset_id: Uuid,
) -> Result<Jobset, ApiError> {
  let jobset =
    circus_common::repo::jobsets::get(&state.pool, jobset_id).await?;
  require_project_mutable(state, jobset.project_id).await?;
  Ok(jobset)
}
