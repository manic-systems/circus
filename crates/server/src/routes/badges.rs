use axum::{
  Router,
  extract::{Path, State},
  http::StatusCode,
  response::{IntoResponse, Response},
  routing::get,
};
use badgelib::{Badge, Color};

use crate::{error::ApiError, state::AppState};

/// Wrap a generated SVG in a 200 response with the correct content-type
/// and cache headers. Used for every successful badge return so callers
/// never need to remember the headers.
fn svg_response(svg: String) -> Response {
  (
    StatusCode::OK,
    [
      ("content-type", "image/svg+xml"),
      ("cache-control", "no-cache, no-store, must-revalidate"),
    ],
    svg,
  )
    .into_response()
}

async fn build_badge(
  State(state): State<AppState>,
  Path((project_name, jobset_name, job_name)): Path<(String, String, String)>,
) -> Result<Response, ApiError> {
  // Find the project
  let project =
    circus_common::repo::projects::get_by_name(&state.pool, &project_name)
      .await?;

  // Find the jobset
  let jobsets = circus_common::repo::jobsets::list_for_project(
    &state.pool,
    project.id,
    1000,
    0,
  )
  .await?;

  let jobset = jobsets.iter().find(|j| j.name == jobset_name);
  let Some(jobset) = jobset else {
    return Ok(svg_response(shield_svg("build", "not found", "#9f9f9f")));
  };

  // Get latest evaluation
  let eval =
    circus_common::repo::evaluations::get_latest(&state.pool, jobset.id)
      .await?;

  let Some(eval) = eval else {
    return Ok(svg_response(shield_svg(
      "build",
      "no evaluations",
      "#9f9f9f",
    )));
  };

  // Find the build for this job
  let builds =
    circus_common::repo::builds::list_for_evaluation(&state.pool, eval.id)
      .await?;

  let build = builds.iter().find(|b| b.job_name == job_name);

  let (label, color) = build.map_or(("not found", "#9f9f9f"), |b| {
    match b.status {
      circus_common::BuildStatus::Succeeded => ("passing", "#4c1"),
      circus_common::BuildStatus::Failed => ("failing", "#e05d44"),
      circus_common::BuildStatus::Running => ("building", "#dfb317"),
      circus_common::BuildStatus::Pending => ("queued", "#dfb317"),
      circus_common::BuildStatus::Cancelled => ("cancelled", "#9f9f9f"),
      circus_common::BuildStatus::DependencyFailed => ("dep failed", "#e05d44"),
      circus_common::BuildStatus::Aborted => ("aborted", "#9f9f9f"),
      circus_common::BuildStatus::FailedWithOutput => {
        ("failed output", "#e05d44")
      },
      circus_common::BuildStatus::Timeout => ("timeout", "#e05d44"),
      circus_common::BuildStatus::CachedFailure => ("cached fail", "#e05d44"),
      circus_common::BuildStatus::UnsupportedSystem => {
        ("unsupported", "#9f9f9f")
      },
      circus_common::BuildStatus::LogLimitExceeded => ("log limit", "#e05d44"),
      circus_common::BuildStatus::NarSizeLimitExceeded => {
        ("nar limit", "#e05d44")
      },
      circus_common::BuildStatus::NonDeterministic => ("non-det", "#e05d44"),
      circus_common::BuildStatus::OomKilled => ("oom killed", "#e05d44"),
    }
  });

  Ok(svg_response(shield_svg("build", label, color)))
}

/// Latest successful build redirect
async fn latest_build(
  State(state): State<AppState>,
  Path((project_name, jobset_name, job_name)): Path<(String, String, String)>,
) -> Result<Response, ApiError> {
  let project =
    circus_common::repo::projects::get_by_name(&state.pool, &project_name)
      .await?;

  let jobsets = circus_common::repo::jobsets::list_for_project(
    &state.pool,
    project.id,
    1000,
    0,
  )
  .await?;

  let jobset = jobsets.iter().find(|j| j.name == jobset_name);
  let Some(jobset) = jobset else {
    return Ok((StatusCode::NOT_FOUND, "Jobset not found").into_response());
  };

  let eval =
    circus_common::repo::evaluations::get_latest(&state.pool, jobset.id)
      .await?;

  let Some(eval) = eval else {
    return Ok((StatusCode::NOT_FOUND, "No evaluations found").into_response());
  };

  let builds =
    circus_common::repo::builds::list_for_evaluation(&state.pool, eval.id)
      .await?;

  let build = builds.iter().find(|b| b.job_name == job_name);
  build.map_or_else(
    || Ok((StatusCode::NOT_FOUND, "Build not found").into_response()),
    |b| Ok(axum::Json(b.clone()).into_response()),
  )
}

fn shield_svg(subject: &str, status: &str, color: &str) -> String {
  Badge::new()
    .label(subject)
    .label_color(Color::Hex("555".into()))
    .value(status)
    .value_color(Color::Hex(color.trim_start_matches('#').into()))
    .to_svg()
}

pub fn router() -> Router<AppState> {
  Router::new()
    .route("/job/{project}/{jobset}/{job}/shield", get(build_badge))
    // Hydra-compatible alias
    .route("/job/{project}/{jobset}/{job}/badge", get(build_badge))
    .route("/job/{project}/{jobset}/{job}/latest", get(latest_build))
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn test_shield_svg_is_valid_svg() {
    let color = "#4c1";
    let svg = shield_svg("build", "passing", color);
    let color = Color::Hex(color.trim_start_matches('#').into()).to_css();
    assert!(svg.starts_with("<svg xmlns="));
    assert!(svg.ends_with("</svg>"));
    assert!(svg.contains("build"));
    assert!(svg.contains("passing"));
    assert!(svg.contains(&color));
  }

  #[test]
  fn test_shield_svg_escapes_text() {
    let svg = shield_svg("<build>", "passing & cached", "#4c1");
    assert!(svg.contains("&lt;build&gt;"));
    assert!(svg.contains("passing &amp; cached"));
  }

  #[test]
  fn test_shield_svg_different_statuses() {
    for (status, color) in [
      ("passing", "#4c1"),
      ("failing", "#e05d44"),
      ("building", "#dfb317"),
      ("not found", "#9f9f9f"),
    ] {
      let svg = shield_svg("build", status, color);
      let css_color = Color::Hex(color.trim_start_matches('#').into()).to_css();
      assert!(svg.contains(status), "SVG should contain status '{status}'");
      assert!(
        svg.contains(&css_color),
        "SVG should contain color '{color}'"
      );
    }
  }
}
