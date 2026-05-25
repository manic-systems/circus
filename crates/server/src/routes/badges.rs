use axum::{
  Router,
  extract::{Path, State},
  http::StatusCode,
  response::{IntoResponse, Response},
  routing::get,
};

use crate::{error::ApiError, state::AppState};

async fn build_badge(
  State(state): State<AppState>,
  Path((project_name, jobset_name, job_name)): Path<(String, String, String)>,
) -> Result<Response, ApiError> {
  // Find the project
  let project =
    fc_common::repo::projects::get_by_name(&state.pool, &project_name)
      .await
      .map_err(ApiError)?;

  // Find the jobset
  let jobsets = fc_common::repo::jobsets::list_for_project(
    &state.pool,
    project.id,
    1000,
    0,
  )
  .await
  .map_err(ApiError)?;

  let jobset = jobsets.iter().find(|j| j.name == jobset_name);
  let Some(jobset) = jobset else {
    return Ok(shield_svg("build", "not found", "#9f9f9f").into_response());
  };

  // Get latest evaluation
  let eval = fc_common::repo::evaluations::get_latest(&state.pool, jobset.id)
    .await
    .map_err(ApiError)?;

  let Some(eval) = eval else {
    return Ok(
      shield_svg("build", "no evaluations", "#9f9f9f").into_response(),
    );
  };

  // Find the build for this job
  let builds =
    fc_common::repo::builds::list_for_evaluation(&state.pool, eval.id)
      .await
      .map_err(ApiError)?;

  let build = builds.iter().find(|b| b.job_name == job_name);

  let (label, color) = build.map_or(("not found", "#9f9f9f"), |b| {
    match b.status {
      fc_common::BuildStatus::Succeeded => ("passing", "#4c1"),
      fc_common::BuildStatus::Failed => ("failing", "#e05d44"),
      fc_common::BuildStatus::Running => ("building", "#dfb317"),
      fc_common::BuildStatus::Pending => ("queued", "#dfb317"),
      fc_common::BuildStatus::Cancelled => ("cancelled", "#9f9f9f"),
      fc_common::BuildStatus::DependencyFailed => ("dep failed", "#e05d44"),
      fc_common::BuildStatus::Aborted => ("aborted", "#9f9f9f"),
      fc_common::BuildStatus::FailedWithOutput => ("failed output", "#e05d44"),
      fc_common::BuildStatus::Timeout => ("timeout", "#e05d44"),
      fc_common::BuildStatus::CachedFailure => ("cached fail", "#e05d44"),
      fc_common::BuildStatus::UnsupportedSystem => ("unsupported", "#9f9f9f"),
      fc_common::BuildStatus::LogLimitExceeded => ("log limit", "#e05d44"),
      fc_common::BuildStatus::NarSizeLimitExceeded => ("nar limit", "#e05d44"),
      fc_common::BuildStatus::NonDeterministic => ("non-det", "#e05d44"),
    }
  });

  Ok(
    (
      StatusCode::OK,
      [
        ("content-type", "image/svg+xml"),
        ("cache-control", "no-cache, no-store, must-revalidate"),
      ],
      shield_svg("build", label, color),
    )
      .into_response(),
  )
}

/// Latest successful build redirect
async fn latest_build(
  State(state): State<AppState>,
  Path((project_name, jobset_name, job_name)): Path<(String, String, String)>,
) -> Result<Response, ApiError> {
  let project =
    fc_common::repo::projects::get_by_name(&state.pool, &project_name)
      .await
      .map_err(ApiError)?;

  let jobsets = fc_common::repo::jobsets::list_for_project(
    &state.pool,
    project.id,
    1000,
    0,
  )
  .await
  .map_err(ApiError)?;

  let jobset = jobsets.iter().find(|j| j.name == jobset_name);
  let Some(jobset) = jobset else {
    return Ok((StatusCode::NOT_FOUND, "Jobset not found").into_response());
  };

  let eval = fc_common::repo::evaluations::get_latest(&state.pool, jobset.id)
    .await
    .map_err(ApiError)?;

  let Some(eval) = eval else {
    return Ok((StatusCode::NOT_FOUND, "No evaluations found").into_response());
  };

  let builds =
    fc_common::repo::builds::list_for_evaluation(&state.pool, eval.id)
      .await
      .map_err(ApiError)?;

  let build = builds.iter().find(|b| b.job_name == job_name);
  build.map_or_else(
    || Ok((StatusCode::NOT_FOUND, "Build not found").into_response()),
    |b| Ok(axum::Json(b.clone()).into_response()),
  )
}

fn shield_svg(subject: &str, status: &str, color: &str) -> String {
  use std::fmt::Write;

  let subject_width = subject.len() * 7 + 10;
  let status_width = status.len() * 7 + 10;
  let total_width = subject_width + status_width;
  let subject_x = subject_width / 2;
  let status_x = subject_width + status_width / 2;

  let mut svg = String::with_capacity(768);
  let _ = writeln!(
    svg,
    "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{total_width}\" \
     height=\"20\">"
  );
  svg.push_str("  <linearGradient id=\"b\" x2=\"0\" y2=\"100%\">\n");
  svg.push_str(
    "    <stop offset=\"0\" stop-color=\"#bbb\" stop-opacity=\".1\"/>\n",
  );
  svg.push_str("    <stop offset=\"1\" stop-opacity=\".1\"/>\n");
  svg.push_str("  </linearGradient>\n");
  svg.push_str("  <mask id=\"a\">\n");
  let _ = writeln!(
    svg,
    "    <rect width=\"{total_width}\" height=\"20\" rx=\"3\" fill=\"#fff\"/>"
  );
  svg.push_str("  </mask>\n");
  svg.push_str("  <g mask=\"url(#a)\">\n");
  let _ = writeln!(
    svg,
    "    <rect width=\"{subject_width}\" height=\"20\" fill=\"#555\"/>"
  );
  let _ = writeln!(
    svg,
    "    <rect x=\"{subject_width}\" width=\"{status_width}\" height=\"20\" \
     fill=\"{color}\"/>"
  );
  let _ = writeln!(
    svg,
    "    <rect width=\"{total_width}\" height=\"20\" fill=\"url(#b)\"/>"
  );
  svg.push_str("  </g>\n");
  svg.push_str(
    "  <g fill=\"#fff\" text-anchor=\"middle\" font-family=\"DejaVu \
     Sans,Verdana,Geneva,sans-serif\" font-size=\"11\">\n",
  );
  let _ = writeln!(
    svg,
    "    <text x=\"{subject_x}\" y=\"15\" fill=\"#010101\" \
     fill-opacity=\".3\">{subject}</text>"
  );
  let _ =
    writeln!(svg, "    <text x=\"{subject_x}\" y=\"14\">{subject}</text>");
  let _ = writeln!(
    svg,
    "    <text x=\"{status_x}\" y=\"15\" fill=\"#010101\" \
     fill-opacity=\".3\">{status}</text>"
  );
  let _ = writeln!(svg, "    <text x=\"{status_x}\" y=\"14\">{status}</text>");
  svg.push_str("  </g>\n");
  svg.push_str("</svg>");
  svg
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
    let svg = shield_svg("build", "passing", "#4c1");
    assert!(svg.starts_with("<svg xmlns="));
    assert!(svg.ends_with("</svg>"));
    assert!(svg.contains("build"));
    assert!(svg.contains("passing"));
    assert!(svg.contains("#4c1"));
  }

  #[test]
  fn test_shield_svg_dimensions() {
    let svg = shield_svg("build", "failing", "#e05d44");
    // "build" = 5 chars * 7 + 10 = 45
    // "failing" = 7 chars * 7 + 10 = 59
    // total = 104
    assert!(svg.contains("width=\"104\""));
  }

  #[test]
  fn test_shield_svg_text_positions() {
    let svg = shield_svg("ci", "ok", "#4c1");
    // "ci" = 2*7+10 = 24, subject_x = 12
    // "ok" = 2*7+10 = 24, status_x = 24 + 12 = 36
    assert!(svg.contains("x=\"12\""));
    assert!(svg.contains("x=\"36\""));
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
      assert!(svg.contains(status), "SVG should contain status '{status}'");
      assert!(svg.contains(color), "SVG should contain color '{color}'");
    }
  }
}
