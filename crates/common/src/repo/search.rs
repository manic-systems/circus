//! Advanced search functionality for circus

use sqlx::{PgPool, Postgres, QueryBuilder};
use uuid::Uuid;

use crate::{
  error::Result,
  models::{Build, Evaluation, Jobset, Project},
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
  LastEvaluation,
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
}

/// Search filters for builds
#[derive(Debug, Clone, Default)]
pub struct BuildSearchFilters {
  pub status:          Option<BuildStatusFilter>,
  pub project_id:      Option<Uuid>,
  pub jobset_id:       Option<Uuid>,
  pub evaluation_id:   Option<Uuid>,
  pub created_after:   Option<chrono::DateTime<chrono::Utc>>,
  pub created_before:  Option<chrono::DateTime<chrono::Utc>>,
  pub min_priority:    Option<i32>,
  pub max_priority:    Option<i32>,
  pub has_substitutes: Option<bool>,
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
    }
  }
}

impl BuildSearchFilters {
  fn apply_to(&self, builder: &mut QueryBuilder<Postgres>) {
    if let Some(status) = self.status {
      builder.push(" AND status = ");
      builder.push_bind(status.as_str());
    }
    if let Some(project_id) = self.project_id {
      builder.push(" AND project_id = ");
      builder.push_bind(project_id);
    }
    if let Some(jobset_id) = self.jobset_id {
      builder.push(" AND jobset_id = ");
      builder.push_bind(jobset_id);
    }
    if let Some(evaluation_id) = self.evaluation_id {
      builder.push(" AND evaluation_id = ");
      builder.push_bind(evaluation_id);
    }
    if let Some(after) = self.created_after {
      builder.push(" AND created_at >= ");
      builder.push_bind(after);
    }
    if let Some(before) = self.created_before {
      builder.push(" AND created_at <= ");
      builder.push_bind(before);
    }
    if let Some(min) = self.min_priority {
      builder.push(" AND priority >= ");
      builder.push_bind(min);
    }
    if let Some(max) = self.max_priority {
      builder.push(" AND priority <= ");
      builder.push_bind(max);
    }
    if let Some(has) = self.has_substitutes {
      builder.push(" AND has_substitutes = ");
      builder.push_bind(has);
    }
  }
}

impl JobsetSearchFilters {
  fn apply_to(&self, builder: &mut QueryBuilder<Postgres>) {
    if let Some(project_id) = self.project_id {
      builder.push(" AND project_id = ");
      builder.push_bind(project_id);
    }
    if let Some(enabled) = self.enabled {
      builder.push(" AND enabled = ");
      builder.push_bind(enabled);
    }
    if let Some(flake_mode) = self.flake_mode {
      builder.push(" AND flake_mode = ");
      builder.push_bind(flake_mode);
    }
  }
}

impl EvaluationSearchFilters {
  fn apply_to(&self, builder: &mut QueryBuilder<Postgres>) {
    if let Some(project_id) = self.project_id {
      builder.push(" AND project_id = ");
      builder.push_bind(project_id);
    }
    if let Some(jobset_id) = self.jobset_id {
      builder.push(" AND jobset_id = ");
      builder.push_bind(jobset_id);
    }
    if let Some(has_builds) = self.has_builds {
      if has_builds {
        builder.push(
          " AND EXISTS (SELECT 1 FROM builds WHERE builds.evaluation_id = \
           evaluations.id)",
        );
      } else {
        builder.push(
          " AND NOT EXISTS (SELECT 1 FROM builds WHERE builds.evaluation_id = \
           evaluations.id)",
        );
      }
    }
    if let Some(after) = self.finished_after {
      builder.push(" AND finished_at >= ");
      builder.push_bind(after);
    }
    if let Some(before) = self.finished_before {
      builder.push(" AND finished_at <= ");
      builder.push_bind(before);
    }
  }
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
  let pattern = if params.query.is_empty() {
    "%".to_string()
  } else {
    format!("%{}%", params.query)
  };

  let mut query_builder: QueryBuilder<Postgres> =
    QueryBuilder::new("SELECT * FROM projects WHERE (name ILIKE ");
  query_builder.push_bind(&pattern);
  query_builder.push(" OR description ILIKE ");
  query_builder.push_bind(&pattern);
  query_builder.push(")");

  // Apply filters
  if let Some(ref filters) = params.project_filters {
    if let Some(after) = filters.created_after {
      query_builder.push(" AND created_at >= ");
      query_builder.push_bind(after);
    }
    if let Some(before) = filters.created_before {
      query_builder.push(" AND created_at <= ");
      query_builder.push_bind(before);
    }
    if let Some(has_jobsets) = filters.has_jobsets {
      if has_jobsets {
        query_builder.push(
          " AND EXISTS (SELECT 1 FROM jobsets WHERE jobsets.project_id = \
           projects.id)",
        );
      } else {
        query_builder.push(
          " AND NOT EXISTS (SELECT 1 FROM jobsets WHERE jobsets.project_id = \
           projects.id)",
        );
      }
    }
  }

  // Get total count
  let (total,): (i64,) = if pattern == "%" {
    sqlx::query_as("SELECT COUNT(*) FROM projects")
      .fetch_one(pool)
      .await?
  } else {
    sqlx::query_as(
      "SELECT COUNT(*) FROM projects WHERE name ILIKE $1 OR description ILIKE \
       $1",
    )
    .bind(&pattern)
    .fetch_one(pool)
    .await?
  };

  // Apply sorting
  query_builder.push(" ORDER BY ");
  if let Some((field, order)) = &params.project_sort {
    let field_str = match field {
      ProjectSortField::Name => "name",
      ProjectSortField::CreatedAt => "created_at",
      ProjectSortField::LastEvaluation => "last_evaluation_at",
    };
    let order_str = match order {
      SortOrder::Asc => "ASC",
      SortOrder::Desc => "DESC",
    };
    query_builder.push(field_str);
    query_builder.push(" ");
    query_builder.push(order_str);
  } else {
    query_builder.push("name ASC");
  }

  // Apply pagination
  query_builder.push(" LIMIT ");
  query_builder.push_bind(params.limit);
  query_builder.push(" OFFSET ");
  query_builder.push_bind(params.offset);

  let projects = query_builder
    .build_query_as::<Project>()
    .fetch_all(pool)
    .await?;

  Ok((projects, total))
}

/// Search jobsets with filters
async fn search_jobsets(
  pool: &PgPool,
  params: &SearchParams,
) -> Result<(Vec<Jobset>, i64)> {
  let pattern = if params.query.is_empty() {
    "%".to_string()
  } else {
    format!("%{}%", params.query)
  };

  let mut query_builder: QueryBuilder<Postgres> =
    QueryBuilder::new("SELECT * FROM jobsets WHERE name ILIKE ");
  query_builder.push_bind(&pattern);

  // Apply filters
  if let Some(ref filters) = params.jobset_filters {
    filters.apply_to(&mut query_builder);
  }

  // Count query mirrors the data query filters.
  let mut count_builder: QueryBuilder<Postgres> =
    QueryBuilder::new("SELECT COUNT(*) FROM jobsets WHERE name ILIKE ");
  count_builder.push_bind(&pattern);
  if let Some(ref filters) = params.jobset_filters {
    filters.apply_to(&mut count_builder);
  }
  let (total,): (i64,) = count_builder.build_query_as().fetch_one(pool).await?;

  // Apply sorting
  query_builder.push(" ORDER BY name ASC LIMIT ");
  query_builder.push_bind(params.limit);
  query_builder.push(" OFFSET ");
  query_builder.push_bind(params.offset);

  let jobsets = query_builder
    .build_query_as::<Jobset>()
    .fetch_all(pool)
    .await?;

  Ok((jobsets, total))
}

/// Search evaluations with filters
async fn search_evaluations(
  pool: &PgPool,
  params: &SearchParams,
) -> Result<(Vec<Evaluation>, i64)> {
  let mut query_builder: QueryBuilder<Postgres> =
    QueryBuilder::new("SELECT * FROM evaluations WHERE 1=1");

  // Apply filters
  if let Some(ref filters) = params.evaluation_filters {
    filters.apply_to(&mut query_builder);
  }

  // Count query mirrors the data query filters.
  let mut count_builder: QueryBuilder<Postgres> =
    QueryBuilder::new("SELECT COUNT(*) FROM evaluations WHERE 1=1");
  if let Some(ref filters) = params.evaluation_filters {
    filters.apply_to(&mut count_builder);
  }
  let (total,): (i64,) = count_builder.build_query_as().fetch_one(pool).await?;

  // Apply sorting and pagination
  query_builder.push(" ORDER BY created_at DESC LIMIT ");
  query_builder.push_bind(params.limit);
  query_builder.push(" OFFSET ");
  query_builder.push_bind(params.offset);

  let evaluations = query_builder
    .build_query_as::<Evaluation>()
    .fetch_all(pool)
    .await?;

  Ok((evaluations, total))
}

/// Search builds with advanced filters
async fn search_builds(
  pool: &PgPool,
  params: &SearchParams,
) -> Result<(Vec<Build>, i64)> {
  let pattern = if params.query.is_empty() {
    "%".to_string()
  } else {
    format!("%{}%", params.query)
  };

  let mut query_builder: QueryBuilder<Postgres> =
    QueryBuilder::new("SELECT * FROM builds WHERE (job_name ILIKE ");
  query_builder.push_bind(&pattern);
  query_builder.push(" OR drv_path ILIKE ");
  query_builder.push_bind(&pattern);
  query_builder.push(")");

  // Apply filters
  if let Some(ref filters) = params.build_filters {
    filters.apply_to(&mut query_builder);
  }

  // Count query mirrors the data query filters.
  let mut count_builder: QueryBuilder<Postgres> =
    QueryBuilder::new("SELECT COUNT(*) FROM builds WHERE (job_name ILIKE ");
  count_builder.push_bind(&pattern);
  count_builder.push(" OR drv_path ILIKE ");
  count_builder.push_bind(&pattern);
  count_builder.push(")");
  if let Some(ref filters) = params.build_filters {
    filters.apply_to(&mut count_builder);
  }
  let (total,): (i64,) = count_builder.build_query_as().fetch_one(pool).await?;

  // Apply sorting
  query_builder.push(" ORDER BY ");
  if let Some((field, order)) = &params.build_sort {
    let field_str = match field {
      BuildSortField::CreatedAt => "created_at",
      BuildSortField::JobName => "job_name",
      BuildSortField::Status => "status",
      BuildSortField::Priority => "priority",
    };
    let order_str = match order {
      SortOrder::Asc => "ASC",
      SortOrder::Desc => "DESC",
    };
    query_builder.push(field_str);
    query_builder.push(" ");
    query_builder.push(order_str);
  } else {
    query_builder.push("created_at DESC");
  }

  // Apply pagination
  query_builder.push(" LIMIT ");
  query_builder.push_bind(params.limit);
  query_builder.push(" OFFSET ");
  query_builder.push_bind(params.offset);

  let builds = query_builder
    .build_query_as::<Build>()
    .fetch_all(pool)
    .await?;

  Ok((builds, total))
}

/// Quick search - simple text search across entities
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

  let projects = sqlx::query_as::<_, Project>(
    "SELECT * FROM projects WHERE name ILIKE $1 OR description ILIKE $1 ORDER \
     BY name LIMIT $2",
  )
  .bind(&pattern)
  .bind(limit)
  .fetch_all(pool)
  .await?;

  let builds = sqlx::query_as::<_, Build>(
    "SELECT * FROM builds WHERE job_name ILIKE $1 OR drv_path ILIKE $1 ORDER \
     BY created_at DESC LIMIT $2",
  )
  .bind(&pattern)
  .bind(limit)
  .fetch_all(pool)
  .await?;

  Ok((projects, builds))
}
