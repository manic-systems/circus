//! Advanced search API routes
//!
//! Supports:
//! - Multi-entity search (projects, jobsets, evaluations, builds)
//! - Full-text search with ILIKE matching
//! - Advanced filtering by status, date range, priority
//! - Sorting by multiple fields
//! - Pagination with total counts

use axum::{
  Json,
  Router,
  extract::{Query, State},
  routing::get,
};
use chrono::{DateTime, Utc};
use circus_common::{
  models::{Build, Evaluation, Jobset, Project},
  repo::search::{
    BuildSearchFilters,
    BuildSortField,
    BuildStatusFilter,
    EvaluationSearchFilters,
    JobsetSearchFilters,
    ProjectSearchFilters,
    ProjectSortField,
    SearchEntity,
    SearchParams,
    SortOrder,
    quick_search,
    search as advanced_search,
  },
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{error::ApiError, state::AppState};

/// Request parameters for advanced search
#[derive(Debug, Deserialize)]
struct SearchRequest {
  /// Search query string (searches across names, descriptions, job names, drv
  /// paths)
  #[serde(default)]
  q: String,

  /// Entities to search (projects, jobsets, evaluations, builds)
  /// Default: ["projects", "builds"]
  #[serde(default)]
  entities: Vec<String>,

  /// Maximum results per entity (default: 20, max: 100)
  #[serde(default = "default_limit")]
  limit: i64,

  /// Offset for pagination (default: 0)
  #[serde(default)]
  offset: i64,

  // Build filters
  /// Filter builds by status: pending, running, succeeded, failed, cancelled
  #[serde(rename = "build_status")]
  build_status: Option<String>,

  /// Filter builds by project ID
  #[serde(rename = "build_project")]
  build_project: Option<Uuid>,

  /// Filter builds by jobset ID
  #[serde(rename = "build_jobset")]
  build_jobset: Option<Uuid>,

  /// Filter builds by evaluation ID
  #[serde(rename = "build_evaluation")]
  build_evaluation: Option<Uuid>,

  /// Filter builds created after this date (ISO 8601)
  #[serde(rename = "build_after")]
  build_after: Option<DateTime<Utc>>,

  /// Filter builds created before this date (ISO 8601)
  #[serde(rename = "build_before")]
  build_before: Option<DateTime<Utc>>,

  /// Minimum build priority
  #[serde(rename = "build_min_priority")]
  build_min_priority: Option<i32>,

  /// Maximum build priority
  #[serde(rename = "build_max_priority")]
  build_max_priority: Option<i32>,

  // Project filters
  /// Filter projects created after this date (ISO 8601)
  #[serde(rename = "project_after")]
  project_after: Option<DateTime<Utc>>,

  /// Filter projects created before this date (ISO 8601)
  #[serde(rename = "project_before")]
  project_before: Option<DateTime<Utc>>,

  // Jobset filters
  /// Filter jobsets by project ID
  #[serde(rename = "jobset_project")]
  jobset_project: Option<Uuid>,

  /// Filter jobsets by enabled status
  #[serde(rename = "jobset_enabled")]
  jobset_enabled: Option<bool>,

  /// Filter jobsets by flake mode
  #[serde(rename = "jobset_flake")]
  jobset_flake: Option<bool>,

  // Evaluation filters
  /// Filter evaluations by project ID
  #[serde(rename = "eval_project")]
  eval_project: Option<Uuid>,

  /// Filter evaluations by jobset ID
  #[serde(rename = "eval_jobset")]
  eval_jobset: Option<Uuid>,

  /// Filter evaluations finished after this date (ISO 8601)
  #[serde(rename = "eval_after")]
  eval_after: Option<DateTime<Utc>>,

  /// Filter evaluations finished before this date (ISO 8601)
  #[serde(rename = "eval_before")]
  eval_before: Option<DateTime<Utc>>,

  // Sorting
  /// Sort builds by: `created_at`, `job_name`, status, priority (default:
  /// `created_at`)
  #[serde(rename = "build_sort")]
  build_sort: Option<String>,

  /// Sort order: asc, desc (default: desc for builds, asc for projects)
  #[serde(rename = "order")]
  order: Option<String>,

  /// Sort projects by: name, `created_at` (default: name)
  #[serde(rename = "project_sort")]
  project_sort: Option<String>,
}

const fn default_limit() -> i64 {
  20
}

/// Search results response
#[derive(Debug, Serialize)]
struct SearchResponse {
  projects:          Vec<Project>,
  jobsets:           Vec<Jobset>,
  evaluations:       Vec<Evaluation>,
  builds:            Vec<Build>,
  #[serde(skip_serializing_if = "Option::is_none")]
  total_projects:    Option<i64>,
  #[serde(skip_serializing_if = "Option::is_none")]
  total_jobsets:     Option<i64>,
  #[serde(skip_serializing_if = "Option::is_none")]
  total_evaluations: Option<i64>,
  #[serde(skip_serializing_if = "Option::is_none")]
  total_builds:      Option<i64>,
  #[serde(skip_serializing_if = "Option::is_none")]
  limit:             Option<i64>,
  #[serde(skip_serializing_if = "Option::is_none")]
  offset:            Option<i64>,
}

/// Legacy quick search parameters (for backward compatibility)
#[derive(Debug, Deserialize)]
struct QuickSearchParams {
  q:     String,
  #[serde(default = "default_limit")]
  limit: i64,
}

/// Handle advanced search requests
async fn advanced_search_handler(
  State(state): State<AppState>,
  Query(params): Query<SearchRequest>,
) -> Result<Json<SearchResponse>, ApiError> {
  // Validate and sanitize
  let query = params.q.trim();
  if query.is_empty() || query.len() > 256 {
    // Empty or too long query returns empty results
    return Ok(Json(SearchResponse {
      projects:          vec![],
      jobsets:           vec![],
      evaluations:       vec![],
      builds:            vec![],
      total_projects:    Some(0),
      total_jobsets:     Some(0),
      total_evaluations: Some(0),
      total_builds:      Some(0),
      limit:             Some(params.limit.clamp(1, 100)),
      offset:            Some(params.offset),
    }));
  }

  // Clamp limit to reasonable range
  let limit = params.limit.clamp(1, 100);
  let clamped_offset = params.offset.max(0);

  // Parse entities
  let entities: Vec<SearchEntity> = if params.entities.is_empty() {
    vec![SearchEntity::Projects, SearchEntity::Builds]
  } else {
    params
      .entities
      .iter()
      .filter_map(|e| {
        match e.as_str() {
          "projects" => Some(SearchEntity::Projects),
          "jobsets" => Some(SearchEntity::Jobsets),
          "evaluations" => Some(SearchEntity::Evaluations),
          "builds" => Some(SearchEntity::Builds),
          _ => None,
        }
      })
      .collect()
  };

  // Parse sort order (default: desc for builds, asc for projects)
  let sort_order = match params.order.as_deref() {
    Some("asc") => SortOrder::Asc,
    Some("desc") => SortOrder::Desc,
    _ => {
      if entities.contains(&SearchEntity::Builds)
        && !entities.contains(&SearchEntity::Projects)
      {
        SortOrder::Desc
      } else {
        SortOrder::Asc
      }
    },
  };

  // Parse build sort field
  let build_sort = params.build_sort.as_deref().map(|s| {
    let field = match s {
      "job_name" => BuildSortField::JobName,
      "status" => BuildSortField::Status,
      "priority" => BuildSortField::Priority,
      _ => BuildSortField::CreatedAt,
    };
    (field, sort_order)
  });

  // Parse project sort field
  let project_sort = params.project_sort.as_deref().map(|s| {
    let field = match s {
      "created_at" => ProjectSortField::CreatedAt,
      _ => ProjectSortField::Name,
    };
    (field, sort_order)
  });

  // Build build filters
  let build_filters = if entities.contains(&SearchEntity::Builds) {
    let status = params.build_status.as_deref().and_then(|s| {
      match s {
        "pending" => Some(BuildStatusFilter::Pending),
        "running" => Some(BuildStatusFilter::Running),
        "succeeded" => Some(BuildStatusFilter::Succeeded),
        "failed" => Some(BuildStatusFilter::Failed),
        "cancelled" => Some(BuildStatusFilter::Cancelled),
        "dependency_failed" => Some(BuildStatusFilter::DependencyFailed),
        "aborted" => Some(BuildStatusFilter::Aborted),
        "failed_with_output" => Some(BuildStatusFilter::FailedWithOutput),
        "timeout" => Some(BuildStatusFilter::Timeout),
        "cached_failure" => Some(BuildStatusFilter::CachedFailure),
        "unsupported_system" => Some(BuildStatusFilter::UnsupportedSystem),
        "log_limit_exceeded" => Some(BuildStatusFilter::LogLimitExceeded),
        "nar_size_limit_exceeded" => {
          Some(BuildStatusFilter::NarSizeLimitExceeded)
        },
        "non_deterministic" => Some(BuildStatusFilter::NonDeterministic),
        _ => None,
      }
    });

    Some(BuildSearchFilters {
      status,
      project_id: params.build_project,
      jobset_id: params.build_jobset,
      evaluation_id: params.build_evaluation,
      created_after: params.build_after,
      created_before: params.build_before,
      min_priority: params.build_min_priority,
      max_priority: params.build_max_priority,
      has_substitutes: None, // Not exposed in API yet
    })
  } else {
    None
  };

  // Build project filters
  let project_filters = if entities.contains(&SearchEntity::Projects) {
    Some(ProjectSearchFilters {
      created_after:  params.project_after,
      created_before: params.project_before,
      has_jobsets:    None, // Not exposed in API yet
    })
  } else {
    None
  };

  // Build jobset filters
  let jobset_filters = if entities.contains(&SearchEntity::Jobsets) {
    Some(JobsetSearchFilters {
      project_id: params.jobset_project,
      enabled:    params.jobset_enabled,
      flake_mode: params.jobset_flake,
    })
  } else {
    None
  };

  // Build evaluation filters
  let evaluation_filters = if entities.contains(&SearchEntity::Evaluations) {
    Some(EvaluationSearchFilters {
      project_id:      params.eval_project,
      jobset_id:       params.eval_jobset,
      has_builds:      None, // Not exposed in API yet
      finished_after:  params.eval_after,
      finished_before: params.eval_before,
    })
  } else {
    None
  };

  let search_params = SearchParams {
    query: query.to_string(),
    entities,
    limit,
    offset: clamped_offset,
    build_filters,
    project_filters,
    jobset_filters,
    evaluation_filters,
    build_sort,
    project_sort,
  };

  let results = advanced_search(&state.pool, &search_params)
    .await
    .map_err(ApiError)?;

  Ok(Json(SearchResponse {
    projects:          results.projects,
    jobsets:           results.jobsets,
    evaluations:       results.evaluations,
    builds:            results.builds,
    total_projects:    Some(results.total_projects),
    total_jobsets:     Some(results.total_jobsets),
    total_evaluations: Some(results.total_evaluations),
    total_builds:      Some(results.total_builds),
    limit:             Some(limit),
    offset:            Some(params.offset),
  }))
}

/// Handle quick search (backward compatible simple search)
async fn quick_search_handler(
  State(state): State<AppState>,
  Query(params): Query<QuickSearchParams>,
) -> Result<Json<SearchResponse>, ApiError> {
  let query = params.q.trim();
  if query.is_empty() || query.len() > 256 {
    return Ok(Json(SearchResponse {
      projects:          vec![],
      jobsets:           vec![],
      evaluations:       vec![],
      builds:            vec![],
      total_projects:    None,
      total_jobsets:     None,
      total_evaluations: None,
      total_builds:      None,
      limit:             None,
      offset:            None,
    }));
  }

  let limit = params.limit.clamp(1, 100);

  let (projects, builds) = quick_search(&state.pool, query, limit)
    .await
    .map_err(ApiError)?;

  Ok(Json(SearchResponse {
    projects,
    jobsets: vec![],
    evaluations: vec![],
    builds,
    total_projects: None,
    total_jobsets: None,
    total_evaluations: None,
    total_builds: None,
    limit: Some(limit),
    offset: Some(0),
  }))
}

pub fn router() -> Router<AppState> {
  Router::new()
    .route("/search", get(advanced_search_handler))
    .route("/search/quick", get(quick_search_handler))
}
