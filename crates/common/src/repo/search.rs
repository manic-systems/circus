//! Advanced search over projects, jobsets, evaluations, and builds. Optional
//! filters are NULL-guarded predicates and the sorts are CASE ladders, so
//! every query stays static and generated.

use circus_codegen::queries::search as q;
use uuid::Uuid;

use crate::{
  db::PgPool,
  error::{CiError, Result},
  models::{Build, BuildStatus, Evaluation, Jobset, Project},
};

/// Search entity types
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SearchEntity {
  Projects,
  Jobsets,
  Evaluations,
  Builds,
}

/// Sort order for search results
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SortOrder {
  Asc,
  Desc,
}

/// Sort field for builds
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BuildSortField {
  CreatedAt,
  JobName,
  Status,
  Priority,
}

/// Sort field for projects
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProjectSortField {
  Name,
  CreatedAt,
}

/// Build status filter
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BuildStatusFilter {
  Pending,
  Running,
  Succeeded,
  Failed,
  Cancelled,
  DependencyFailed,
  Aborted,
  FailedWithOutput,
  Timeout,
  CachedFailure,
  UnsupportedSystem,
  LogLimitExceeded,
  NarSizeLimitExceeded,
  NonDeterministic,
  OomKilled,
}

impl BuildStatusFilter {
  const fn as_str(self) -> &'static str {
    match self {
      Self::Pending => "pending",
      Self::Running => "running",
      Self::Succeeded => "succeeded",
      Self::Failed => "failed",
      Self::Cancelled => "cancelled",
      Self::DependencyFailed => "dependency_failed",
      Self::Aborted => "aborted",
      Self::FailedWithOutput => "failed_with_output",
      Self::Timeout => "timeout",
      Self::CachedFailure => "cached_failure",
      Self::UnsupportedSystem => "unsupported_system",
      Self::LogLimitExceeded => "log_limit_exceeded",
      Self::NarSizeLimitExceeded => "nar_size_limit_exceeded",
      Self::NonDeterministic => "non_deterministic",
      Self::OomKilled => "oom_killed",
    }
  }
}

/// Search filters for builds
#[derive(Debug, Clone, Default)]
pub struct BuildSearchFilters {
  pub status:         Option<BuildStatusFilter>,
  pub project_id:     Option<Uuid>,
  pub jobset_id:      Option<Uuid>,
  pub evaluation_id:  Option<Uuid>,
  pub created_after:  Option<chrono::DateTime<chrono::Utc>>,
  pub created_before: Option<chrono::DateTime<chrono::Utc>>,
  pub min_priority:   Option<i32>,
  pub max_priority:   Option<i32>,
}

/// Search filters for projects
#[derive(Debug, Clone, Default)]
pub struct ProjectSearchFilters {
  pub created_after:  Option<chrono::DateTime<chrono::Utc>>,
  pub created_before: Option<chrono::DateTime<chrono::Utc>>,
  pub has_jobsets:    Option<bool>,
}

/// Search filters for jobsets
#[derive(Debug, Clone, Default)]
pub struct JobsetSearchFilters {
  pub project_id: Option<Uuid>,
  pub enabled:    Option<bool>,
  pub flake_mode: Option<bool>,
}

/// Search filters for evaluations
#[derive(Debug, Clone, Default)]
pub struct EvaluationSearchFilters {
  pub project_id:      Option<Uuid>,
  pub jobset_id:       Option<Uuid>,
  pub has_builds:      Option<bool>,
  pub finished_after:  Option<chrono::DateTime<chrono::Utc>>,
  pub finished_before: Option<chrono::DateTime<chrono::Utc>>,
}

/// Search parameters
#[derive(Debug, Clone)]
pub struct SearchParams {
  pub query:              String,
  pub entities:           Vec<SearchEntity>,
  pub limit:              i64,
  pub offset:             i64,
  pub build_filters:      Option<BuildSearchFilters>,
  pub project_filters:    Option<ProjectSearchFilters>,
  pub jobset_filters:     Option<JobsetSearchFilters>,
  pub evaluation_filters: Option<EvaluationSearchFilters>,
  pub build_sort:         Option<(BuildSortField, SortOrder)>,
  pub project_sort:       Option<(ProjectSortField, SortOrder)>,
}

impl Default for SearchParams {
  fn default() -> Self {
    Self {
      query:              String::new(),
      entities:           vec![SearchEntity::Projects, SearchEntity::Builds],
      limit:              20,
      offset:             0,
      build_filters:      None,
      project_filters:    None,
      jobset_filters:     None,
      evaluation_filters: None,
      build_sort:         None,
      project_sort:       None,
    }
  }
}

/// Search results container
#[derive(Debug, Clone)]
pub struct SearchResults {
  pub projects:          Vec<Project>,
  pub jobsets:           Vec<Jobset>,
  pub evaluations:       Vec<Evaluation>,
  pub builds:            Vec<Build>,
  pub total_projects:    i64,
  pub total_jobsets:     i64,
  pub total_evaluations: i64,
  pub total_builds:      i64,
}

const fn project_sort_key(
  sort: Option<(ProjectSortField, SortOrder)>,
) -> Option<&'static str> {
  match sort {
    None => None,
    Some((ProjectSortField::Name, SortOrder::Asc)) => Some("name_asc"),
    Some((ProjectSortField::Name, SortOrder::Desc)) => Some("name_desc"),
    Some((ProjectSortField::CreatedAt, SortOrder::Asc)) => {
      Some("created_at_asc")
    },
    Some((ProjectSortField::CreatedAt, SortOrder::Desc)) => {
      Some("created_at_desc")
    },
  }
}

const fn build_sort_key(
  sort: Option<(BuildSortField, SortOrder)>,
) -> Option<&'static str> {
  match sort {
    None => None,
    Some((BuildSortField::CreatedAt, SortOrder::Asc)) => Some("created_at_asc"),
    Some((BuildSortField::CreatedAt, SortOrder::Desc)) => {
      Some("created_at_desc")
    },
    Some((BuildSortField::JobName, SortOrder::Asc)) => Some("job_name_asc"),
    Some((BuildSortField::JobName, SortOrder::Desc)) => Some("job_name_desc"),
    Some((BuildSortField::Status, SortOrder::Asc)) => Some("status_asc"),
    Some((BuildSortField::Status, SortOrder::Desc)) => Some("status_desc"),
    Some((BuildSortField::Priority, SortOrder::Asc)) => Some("priority_asc"),
    Some((BuildSortField::Priority, SortOrder::Desc)) => Some("priority_desc"),
  }
}

fn like_pattern(query: &str) -> String {
  if query.is_empty() {
    "%".to_string()
  } else {
    format!("%{query}%")
  }
}

fn parse_build_status(status: &str, id: Uuid) -> Result<BuildStatus> {
  status.parse().map_err(|e| {
    CiError::Internal(format!("build {id} in the database has {e}"))
  })
}

fn project_from_quick_search_row(
  row: q::ProjectQuickSearchRow,
) -> Result<Project> {
  Ok(Project {
    id:              row.id,
    name:            row.name,
    description:     row.description,
    repository_url:  row.repository_url,
    cache_enabled:   row.cache_enabled,
    cache_url:       row.cache_url,
    cache_upstreams: serde_json::from_value(row.cache_upstreams)?,
    created_at:      row.created_at,
    updated_at:      row.updated_at,
  })
}

fn jobset_from_search_row(row: q::JobsetSearchRow) -> Result<Jobset> {
  Ok(Jobset {
    id:                row.id,
    project_id:        row.project_id,
    name:              row.name,
    nix_expression:    row.nix_expression,
    enabled:           row.enabled,
    flake_mode:        row.flake_mode,
    check_interval:    row.check_interval,
    trigger_mode:      row.trigger_mode.parse().map_err(CiError::Internal)?,
    branch:            row.branch,
    branch_pattern:    row.branch_pattern,
    tag_pattern:       row.tag_pattern,
    scheduling_shares: row.scheduling_shares,
    created_at:        row.created_at,
    updated_at:        row.updated_at,
    state:             row.state.parse().map_err(CiError::Internal)?,
    last_checked_at:   row.last_checked_at,
    keep_nr:           row.keep_nr,
    systems:           row.systems,
  })
}

fn evaluation_from_search_row(
  row: q::EvaluationSearchRow,
) -> Result<Evaluation> {
  Ok(Evaluation {
    id:              row.id,
    jobset_id:       row.jobset_id,
    commit_hash:     row.commit_hash,
    evaluation_time: row.evaluation_time,
    status:          row.status.parse().map_err(CiError::Internal)?,
    error_message:   row.error_message,
    inputs_hash:     row.inputs_hash,
    trigger_kind:    row.trigger_kind.parse().map_err(CiError::Internal)?,
    hidden:          row.hidden,
    pr_number:       row.pr_number,
    pr_head_branch:  row.pr_head_branch,
    pr_base_branch:  row.pr_base_branch,
    pr_action:       row.pr_action,
  })
}

fn build_from_quick_search_row(row: q::BuildQuickSearchRow) -> Result<Build> {
  Ok(Build {
    id:                         row.id,
    evaluation_id:              row.evaluation_id,
    job_name:                   row.job_name,
    drv_path:                   row.drv_path,
    status:                     parse_build_status(&row.status, row.id)?,
    started_at:                 row.started_at,
    completed_at:               row.completed_at,
    log_path:                   row.log_path,
    build_output_path:          row.build_output_path,
    error_message:              row.error_message,
    system:                     row.system,
    priority:                   row.priority,
    retry_count:                row.retry_count,
    max_retries:                row.max_retries,
    notification_pending_since: row.notification_pending_since,
    created_at:                 row.created_at,
    outputs:                    row.outputs,
    is_aggregate:               row.is_aggregate,
    constituents:               row.constituents,
    builder_id:                 row.builder_id,
    agent_machine_id:           row.agent_machine_id,
    signed:                     row.signed,
    keep:                       row.keep,
    is_fod:                     row.is_fod,
    fod_hash:                   row.fod_hash,
    meta_description:           row.meta_description,
    meta_license:               row.meta_license,
    meta_homepage:              row.meta_homepage,
    meta_maintainers:           row.meta_maintainers,
    required_features:          row.required_features,
    effective_features:         row.effective_features,
  })
}

/// Execute a comprehensive search across all entities
///
/// # Errors
///
/// Returns error if database query fails.
pub async fn search(
  pool: &PgPool,
  params: &SearchParams,
) -> Result<SearchResults> {
  let mut results = SearchResults {
    projects:          vec![],
    jobsets:           vec![],
    evaluations:       vec![],
    builds:            vec![],
    total_projects:    0,
    total_jobsets:     0,
    total_evaluations: 0,
    total_builds:      0,
  };

  for entity in &params.entities {
    match entity {
      SearchEntity::Projects => {
        let (projects, total) = search_projects(pool, params).await?;
        results.projects = projects;
        results.total_projects = total;
      },
      SearchEntity::Jobsets => {
        let (jobsets, total) = search_jobsets(pool, params).await?;
        results.jobsets = jobsets;
        results.total_jobsets = total;
      },
      SearchEntity::Evaluations => {
        let (evaluations, total) = search_evaluations(pool, params).await?;
        results.evaluations = evaluations;
        results.total_evaluations = total;
      },
      SearchEntity::Builds => {
        let (builds, total) = search_builds(pool, params).await?;
        results.builds = builds;
        results.total_builds = total;
      },
    }
  }

  Ok(results)
}

/// Search projects with filters
async fn search_projects(
  pool: &PgPool,
  params: &SearchParams,
) -> Result<(Vec<Project>, i64)> {
  let pattern = like_pattern(&params.query);
  let f = params.project_filters.clone().unwrap_or_default();
  let sort = project_sort_key(params.project_sort);

  let client = pool.get().await?;
  let rows = q::search_projects()
    .bind(
      &client,
      &pattern,
      &f.created_after,
      &f.created_before,
      &f.has_jobsets,
      &sort,
      &params.limit,
      &params.offset,
    )
    .all()
    .await?;
  let total = q::count_projects()
    .bind(
      &client,
      &pattern,
      &f.created_after,
      &f.created_before,
      &f.has_jobsets,
    )
    .one()
    .await?;

  let projects = rows
    .into_iter()
    .map(project_from_quick_search_row)
    .collect::<Result<_>>()?;
  Ok((projects, total))
}

/// Search jobsets with filters
async fn search_jobsets(
  pool: &PgPool,
  params: &SearchParams,
) -> Result<(Vec<Jobset>, i64)> {
  let pattern = like_pattern(&params.query);
  let f = params.jobset_filters.clone().unwrap_or_default();

  let client = pool.get().await?;
  let rows = q::search_jobsets()
    .bind(
      &client,
      &pattern,
      &f.project_id,
      &f.enabled,
      &f.flake_mode,
      &params.limit,
      &params.offset,
    )
    .all()
    .await?;
  let total = q::count_jobsets()
    .bind(&client, &pattern, &f.project_id, &f.enabled, &f.flake_mode)
    .one()
    .await?;

  let jobsets = rows
    .into_iter()
    .map(jobset_from_search_row)
    .collect::<Result<_>>()?;
  Ok((jobsets, total))
}

/// Search evaluations with filters
async fn search_evaluations(
  pool: &PgPool,
  params: &SearchParams,
) -> Result<(Vec<Evaluation>, i64)> {
  let f = params.evaluation_filters.clone().unwrap_or_default();

  let client = pool.get().await?;
  let rows = q::search_evaluations()
    .bind(
      &client,
      &f.project_id,
      &f.jobset_id,
      &f.has_builds,
      &f.finished_after,
      &f.finished_before,
      &params.limit,
      &params.offset,
    )
    .all()
    .await?;
  let total = q::count_evaluations()
    .bind(
      &client,
      &f.project_id,
      &f.jobset_id,
      &f.has_builds,
      &f.finished_after,
      &f.finished_before,
    )
    .one()
    .await?;

  let evaluations = rows
    .into_iter()
    .map(evaluation_from_search_row)
    .collect::<Result<_>>()?;
  Ok((evaluations, total))
}

/// Search builds with advanced filters
async fn search_builds(
  pool: &PgPool,
  params: &SearchParams,
) -> Result<(Vec<Build>, i64)> {
  let pattern = like_pattern(&params.query);
  let f = params.build_filters.clone().unwrap_or_default();
  let status = f.status.map(BuildStatusFilter::as_str);
  let sort = build_sort_key(params.build_sort);

  let client = pool.get().await?;
  let rows = q::search_builds()
    .bind(
      &client,
      &pattern,
      &status,
      &f.project_id,
      &f.jobset_id,
      &f.evaluation_id,
      &f.created_after,
      &f.created_before,
      &f.min_priority,
      &f.max_priority,
      &sort,
      &params.limit,
      &params.offset,
    )
    .all()
    .await?;
  let total = q::count_builds()
    .bind(
      &client,
      &pattern,
      &status,
      &f.project_id,
      &f.jobset_id,
      &f.evaluation_id,
      &f.created_after,
      &f.created_before,
      &f.min_priority,
      &f.max_priority,
    )
    .one()
    .await?;

  let builds = rows
    .into_iter()
    .map(build_from_quick_search_row)
    .collect::<Result<_>>()?;
  Ok((builds, total))
}

/// Quick search, a simple text match across projects and builds.
///
/// # Errors
///
/// Returns error if database query fails.
pub async fn quick_search(
  pool: &PgPool,
  query: &str,
  limit: i64,
) -> Result<(Vec<Project>, Vec<Build>)> {
  let pattern = format!("%{query}%");
  let client = pool.get().await?;

  let project_rows = q::quick_projects()
    .bind(&client, &pattern, &limit)
    .all()
    .await?;
  let build_rows = q::quick_builds()
    .bind(&client, &pattern, &limit)
    .all()
    .await?;

  let projects = project_rows
    .into_iter()
    .map(project_from_quick_search_row)
    .collect::<Result<_>>()?;
  let builds = build_rows
    .into_iter()
    .map(build_from_quick_search_row)
    .collect::<Result<_>>()?;

  Ok((projects, builds))
}
