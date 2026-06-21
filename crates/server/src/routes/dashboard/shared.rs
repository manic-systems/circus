//! View models, formatting helpers, status-badge mappings, and per-request
//! auth helpers shared across all dashboard handlers. Everything here is
//! `pub(super)` so sibling modules (auth, admin, pages, ...) can use them
//! without re-exporting them at the dashboard module's external surface.

use std::convert::Infallible;

use askama::Template;
use axum::{
  extract::FromRequestParts,
  http::{Extensions, StatusCode, request::Parts},
  response::{Html, IntoResponse, Redirect, Response},
};
use circus_common::models::{
  ApiKey,
  Build,
  BuildStatus,
  Evaluation,
  EvaluationStatus,
  User,
};
use circus_config::{Config, PageAccessLevel, ServerConfig, UiConfig};
use circus_proto::nix_log::{self, LogLine};
use subtle::ConstantTimeEq;
use uuid::Uuid;

use crate::{
  permissions::{self, Permission, UiPermissions},
  state::{AppState, CsrfToken},
};

#[derive(Clone)]
pub(super) struct UiTemplateConfig {
  pub(super) brand_name:     String,
  pub(super) brand_subtitle: String,
  pub(super) logo_url:       String,
  pub(super) has_logo:       bool,
  pub(super) favicon_url:    String,
  pub(super) has_favicon:    bool,
  pub(super) has_custom_css: bool,
}

impl UiTemplateConfig {
  pub(super) fn from_config(config: &UiConfig) -> Self {
    let logo_url = config.logo_url.clone().unwrap_or_default();
    let favicon_url = config.favicon_url.clone().unwrap_or_default();
    Self {
      brand_name: config.brand_name.clone(),
      brand_subtitle: config.brand_subtitle.clone(),
      has_logo: !logo_url.is_empty(),
      logo_url,
      has_favicon: !favicon_url.is_empty(),
      favicon_url,
      has_custom_css: config.custom_css.is_some(),
    }
  }
}

#[derive(Template)]
#[template(path = "private.html")]
pub(super) struct PrivateTemplate {
  pub(super) ui:        UiTemplateConfig,
  pub(super) is_admin:  bool,
  pub(super) auth_name: String,
}

pub(super) trait RenderExt: Template {
  #[expect(
    clippy::result_large_err,
    reason = "dashboard handlers return axum Response directly"
  )]
  fn render_html_or_500(&self) -> Result<Html<String>, Response> {
    self.render().map(Html).map_err(|error| {
      (
        StatusCode::INTERNAL_SERVER_ERROR,
        format!("Template error: {error}"),
      )
        .into_response()
    })
  }
}

impl<T: Template> RenderExt for T {}

pub(super) struct Pagination {
  pub(super) page:        i64,
  pub(super) total_pages: i64,
  pub(super) has_prev:    bool,
  pub(super) has_next:    bool,
  pub(super) prev_offset: i64,
  pub(super) next_offset: i64,
}

impl Pagination {
  #[must_use]
  pub(super) fn new(total: i64, offset: i64, limit: i64) -> Self {
    let limit = limit.max(1);
    Self {
      page:        offset / limit + 1,
      total_pages: (total + limit - 1) / limit,
      has_prev:    offset > 0,
      has_next:    offset + limit < total,
      prev_offset: (offset - limit).max(0),
      next_offset: offset + limit,
    }
  }
}

// View models (pre-formatted for templates)

pub(super) struct BuildView {
  pub(super) id:            Uuid,
  pub(super) job_name:      String,
  pub(super) project_id:    Option<Uuid>,
  pub(super) project_name:  String,
  pub(super) jobset_id:     Option<Uuid>,
  pub(super) jobset_name:   String,
  pub(super) status_text:   String,
  pub(super) status_class:  String,
  pub(super) system:        String,
  pub(super) created_at:    String,
  pub(super) started_at:    String,
  pub(super) completed_at:  String,
  pub(super) duration:      String,
  /// Unix epoch seconds for the build start, when running.
  pub(super) started_epoch: Option<i64>,
  pub(super) priority:      i32,
  pub(super) is_aggregate:  bool,
  pub(super) signed:        bool,
  pub(super) drv_path:      String,
  pub(super) output_path:   String,
  pub(super) error_message: String,
  pub(super) error_lines:   Vec<BuildErrorLine>,
  pub(super) has_log:       bool,
}

/// Queue page build info with elapsed time and builder details
pub(super) struct QueueBuildView {
  pub(super) id:            Uuid,
  pub(super) job_name:      String,
  pub(super) project_id:    Option<Uuid>,
  pub(super) project_name:  String,
  pub(super) jobset_id:     Option<Uuid>,
  pub(super) jobset_name:   String,
  pub(super) system:        String,
  pub(super) created_at:    String,
  pub(super) started_at:    String,
  pub(super) elapsed:       String,
  /// Unix epoch seconds for the build start. None when the build has not
  /// started; populated for running builds so the browser can tick a live
  /// elapsed counter without polling.
  pub(super) started_epoch: Option<i64>,
  pub(super) priority:      i32,
  pub(super) builder_name:  Option<String>,
  pub(super) queue_pos:     i64,
}

pub(super) struct EvalView {
  pub(super) id:            Uuid,
  pub(super) commit_hash:   String,
  pub(super) commit_short:  String,
  pub(super) status_text:   String,
  pub(super) status_class:  String,
  pub(super) time:          String,
  pub(super) error_message: Option<String>,
  pub(super) hidden:        bool,
  pub(super) jobset_name:   String,
  pub(super) project_name:  String,
}

pub(super) struct EvalSummaryView {
  pub(super) id:           Uuid,
  pub(super) commit_short: String,
  pub(super) status_text:  String,
  pub(super) status_class: String,
  pub(super) time:         String,
  pub(super) succeeded:    i64,
  pub(super) failed:       i64,
  pub(super) pending:      i64,
  pub(super) hidden:       bool,
}

pub(super) struct JobStatusColumn {
  pub(super) eval_id: Uuid,
  pub(super) label:   String,
  pub(super) title:   String,
}

pub(super) struct JobStatusCell {
  pub(super) href:         String,
  pub(super) status_text:  String,
  pub(super) status_class: String,
}

pub(super) struct JobStatusRow {
  pub(super) job_name:  String,
  pub(super) is_active: bool,
  pub(super) cells:     Vec<JobStatusCell>,
}

#[derive(Clone, Copy)]
pub(super) enum DashboardPage {
  Home,
  Projects,
  Project,
  Jobset,
  JobsetJobs,
  Evaluations,
  Evaluation,
  Builds,
  Build,
  Queue,
  Channels,
  Channel,
  News,
  Starred,
  Metrics,
  Caches,
  CacheDetail,
  CacheNars,
}

pub(super) struct DashboardContext {
  pub(super) is_admin:         bool,
  pub(super) is_authenticated: bool,
  pub(super) auth_name:        String,
  pub(super) csrf_token:       String,
  pub(super) permissions:      UiPermissions,
  pub(super) viewer_user_id:   Option<Uuid>,
}

impl DashboardContext {
  #[must_use]
  pub(super) fn from_extensions(extensions: &Extensions) -> Self {
    Self {
      is_admin:         is_admin(extensions),
      is_authenticated: is_authenticated(extensions),
      auth_name:        auth_name(extensions),
      csrf_token:       extensions
        .get::<CsrfToken>()
        .map(|t| t.0.clone())
        .unwrap_or_default(),
      permissions:      UiPermissions::from_extensions(extensions),
      viewer_user_id:   extensions
        .get::<User>()
        .map(|user| user.id)
        .or_else(|| extensions.get::<ApiKey>().and_then(|key| key.user_id)),
    }
  }

  #[expect(
    clippy::result_large_err,
    reason = "dashboard handlers return axum Response directly"
  )]
  pub(super) fn check_csrf(&self, submitted: &str) -> Result<(), Response> {
    if self.csrf_token.is_empty()
      || self
        .csrf_token
        .as_bytes()
        .ct_eq(submitted.as_bytes())
        .unwrap_u8()
        != 1
    {
      return Err(
        (StatusCode::FORBIDDEN, "Invalid or missing CSRF token")
          .into_response(),
      );
    }
    Ok(())
  }

  pub(super) const fn require_permission(
    &self,
    permission: Permission,
  ) -> Result<(), StatusCode> {
    let granted = match permission {
      Permission::Admin => self.permissions.admin,
      Permission::BumpToFront => self.permissions.bump_to_front,
      Permission::CancelBuild => self.permissions.cancel_build,
      Permission::RestartJobs => self.permissions.restart_jobs,
      Permission::CreateProjects => self.permissions.create_projects,
      Permission::EvalJobset => self.permissions.eval_jobset,
    };
    if granted {
      Ok(())
    } else if self.is_authenticated {
      Err(StatusCode::FORBIDDEN)
    } else {
      Err(StatusCode::UNAUTHORIZED)
    }
  }
}

impl FromRequestParts<AppState> for DashboardContext {
  type Rejection = Infallible;

  async fn from_request_parts(
    parts: &mut Parts,
    _state: &AppState,
  ) -> Result<Self, Self::Rejection> {
    Ok(Self::from_extensions(&parts.extensions))
  }
}

impl DashboardPage {
  const fn access(self, config: &ServerConfig) -> PageAccessLevel {
    let pages = &config.page_access;
    match self {
      Self::Home => pages.home,
      Self::Projects => pages.projects,
      Self::Project => pages.project,
      Self::Jobset => pages.jobset,
      Self::JobsetJobs => pages.jobset_jobs,
      Self::Evaluations => pages.evaluations,
      Self::Evaluation => pages.evaluation,
      Self::Builds => pages.builds,
      Self::Build => pages.build,
      Self::Queue => pages.queue,
      Self::Channels => pages.channels,
      Self::Channel => pages.channel,
      Self::News => pages.news,
      Self::Starred => pages.starred,
      Self::Metrics => pages.metrics,
      // Cache observability surfaces are admin-only and not configurable via
      // page_access; there is no first-class cache entity to expose publicly.
      Self::Caches | Self::CacheDetail | Self::CacheNars => {
        PageAccessLevel::Admin
      },
    }
  }
}

/// Format a byte count as a human-readable binary-unit string (e.g. `1.4 GiB`).
/// Negative inputs are clamped to zero.
#[must_use]
pub(super) fn format_bytes(bytes: i64) -> String {
  const UNITS: [&str; 6] = ["B", "KiB", "MiB", "GiB", "TiB", "PiB"];
  let mut value = bytes.max(0) as f64;
  let mut unit = 0;
  while value >= 1024.0 && unit < UNITS.len() - 1 {
    value /= 1024.0;
    unit += 1;
  }
  if unit == 0 {
    format!("{} {}", value as i64, UNITS[unit])
  } else {
    format!("{value:.1} {}", UNITS[unit])
  }
}

/// The 32-character store-path hash from a `/nix/store/<hash>-<name>` path, or
/// the whole path when it does not match that shape.
#[must_use]
pub(super) fn store_path_hash(store_path: &str) -> String {
  store_path
    .strip_prefix("/nix/store/")
    .and_then(|rest| rest.split_once('-'))
    .map_or_else(|| store_path.to_owned(), |(hash, _name)| hash.to_owned())
}

pub(super) fn enforce_page_access(
  config: &Config,
  ctx: &DashboardContext,
  page: DashboardPage,
) -> Result<(), Response> {
  #![expect(
    clippy::result_large_err,
    reason = "Dashboard handlers return axum Response directly; boxing would \
              add noise at every call site"
  )]

  let allowed = match page.access(&config.server) {
    PageAccessLevel::Public => true,
    PageAccessLevel::Authenticated => ctx.is_authenticated,
    PageAccessLevel::Admin => ctx.is_admin,
  };
  if allowed {
    return Ok(());
  }

  if ctx.is_authenticated {
    return Err(Redirect::to("/").into_response());
  }
  let tmpl = PrivateTemplate {
    ui:        UiTemplateConfig::from_config(&config.ui),
    is_admin:  ctx.is_admin,
    auth_name: ctx.auth_name.clone(),
  };
  Err(tmpl.render().map_or_else(
    |_| (StatusCode::INTERNAL_SERVER_ERROR, "Template error").into_response(),
    |html| (StatusCode::UNAUTHORIZED, Html(html)).into_response(),
  ))
}

pub(super) struct ProjectSummaryView {
  pub(super) id:               Uuid,
  pub(super) name:             String,
  pub(super) jobset_count:     i64,
  pub(super) last_eval_status: String,
  pub(super) last_eval_class:  String,
  pub(super) last_eval_time:   String,
  pub(super) failing_jobs:     i64,
  pub(super) queued_jobs:      i64,
  pub(super) systems:          String,
  pub(super) updated_at:       String,
}

pub(super) struct QueueSystemView {
  pub(super) system: String,
  pub(super) count:  i64,
}

pub(super) struct WorkerSummaryView {
  pub(super) name:         String,
  pub(super) system:       String,
  pub(super) status_text:  String,
  pub(super) status_class: String,
  pub(super) current_jobs: i32,
  pub(super) max_jobs:     i32,
}

pub(super) struct ApiKeyView {
  pub(super) id:           Uuid,
  pub(super) name:         String,
  pub(super) role:         String,
  pub(super) created_at:   String,
  pub(super) last_used_at: String,
}

pub(super) struct UserView {
  pub(super) id:            Uuid,
  pub(super) username:      String,
  pub(super) email:         String,
  pub(super) role:          String,
  pub(super) user_type:     String,
  pub(super) enabled:       bool,
  pub(super) last_login_at: String,
}

pub(super) struct StarredJobView {
  pub(super) id:              Uuid,
  pub(super) project_id:      Uuid,
  pub(super) project_name:    String,
  pub(super) jobset_id:       Option<Uuid>,
  pub(super) jobset_name:     String,
  pub(super) job_name:        String,
  pub(super) status_text:     String,
  pub(super) status_class:    String,
  pub(super) latest_build_id: Option<Uuid>,
}

/// A single parsed line from a nix build error stream, classed for styling.
pub(super) struct BuildErrorLine {
  pub(super) text:  String,
  pub(super) level: &'static str,
}

fn strip_ansi(s: &str) -> String {
  String::from_utf8_lossy(&strip_ansi_escapes::strip(s)).into_owned()
}

/// Decode a stored `internal-json` build log into plain terminal text.
pub(super) fn decode_build_log(raw: &str) -> String {
  let mut out = String::with_capacity(raw.len());
  for line in raw.lines() {
    match nix_log::parse_line(line) {
      Some(LogLine::Message { text, .. } | LogLine::Output { text }) => {
        out.push_str(&strip_ansi(&text));
        out.push('\n');
      },
      // Plain output is passed through
      None if !nix_log::is_envelope(line) => {
        out.push_str(&strip_ansi(line));
        out.push('\n');
      },
      None => {},
    }
  }
  out
}

/// Parse a build's `error_message` field into displayable lines.
///
/// Queue-runner captures `nix build --log-format=internal-json` output, so the
/// message is typically a stream of `@nix {...json...}` envelopes pasted
/// together with ANSI colour escapes embedded. Rendering it raw produces a
/// wall of text. We extract each envelope's `msg` (falling back to `raw_msg`),
/// strip ANSI codes, and tag a severity class. Anything that isn't a
/// recognisable envelope is preserved as a single line.
pub(super) fn parse_build_error(raw: &str) -> Vec<BuildErrorLine> {
  const fn classify(level: i64) -> &'static str {
    match level {
      0 => "error",
      1 => "warn",
      2 | 3 => "notice",
      _ => "info",
    }
  }

  let mut lines = Vec::new();
  for line in raw.lines() {
    let (text, level) = match nix_log::parse_line(line) {
      Some(LogLine::Message { level, text }) => {
        (strip_ansi(&text).trim().to_string(), classify(level))
      },
      Some(LogLine::Output { .. }) => continue,
      None if nix_log::is_envelope(line) => continue,
      None => {
        (
          // A plain line
          strip_ansi(line).trim().trim_end_matches(':').trim().into(),
          "info",
        )
      },
    };
    if !text.is_empty() {
      lines.push(BuildErrorLine { text, level });
    }
  }
  lines
}

pub(super) fn format_duration(
  started: Option<&chrono::DateTime<chrono::Utc>>,
  completed: Option<&chrono::DateTime<chrono::Utc>>,
) -> String {
  match (started, completed) {
    (Some(s), Some(c)) => {
      let secs = (*c - *s).num_seconds();
      if secs < 0 {
        return String::new();
      }
      let mins = secs / 60;
      let rem = secs % 60;
      if mins > 0 {
        format!("{mins}m {rem}s")
      } else {
        format!("{rem}s")
      }
    },
    _ => String::new(),
  }
}

impl From<&Build> for BuildView {
  fn from(b: &Build) -> Self {
    let (text, class) = b.status.badge();
    Self {
      id:            b.id,
      job_name:      b.job_name.clone(),
      project_id:    None,
      project_name:  String::new(),
      jobset_id:     None,
      jobset_name:   String::new(),
      status_text:   text.to_string(),
      status_class:  class.to_string(),
      system:        b.system.clone().unwrap_or_else(|| "-".to_string()),
      created_at:    b.created_at.format("%Y-%m-%d %H:%M").to_string(),
      started_at:    b
        .started_at
        .map(|t| t.format("%Y-%m-%d %H:%M:%S").to_string())
        .unwrap_or_default(),
      completed_at:  b
        .completed_at
        .map(|t| t.format("%Y-%m-%d %H:%M:%S").to_string())
        .unwrap_or_default(),
      duration:      format_duration(
        b.started_at.as_ref(),
        b.completed_at.as_ref(),
      ),
      // Only expose epoch while running so the client-side ticker stops
      // updating once the build completes.
      started_epoch: if b.completed_at.is_none() {
        b.started_at.map(|t| t.timestamp())
      } else {
        None
      },
      priority:      b.priority,
      is_aggregate:  b.is_aggregate,
      signed:        b.signed,
      drv_path:      b.drv_path.clone(),
      output_path:   b.build_output_path.clone().unwrap_or_default(),
      error_message: b.error_message.clone().unwrap_or_default(),
      error_lines:   b
        .error_message
        .as_deref()
        .map(parse_build_error)
        .unwrap_or_default(),
      has_log:       b.log_path.as_deref().is_some_and(|p| !p.is_empty()),
    }
  }
}

pub(super) fn build_view(b: &Build) -> BuildView {
  BuildView::from(b)
}

pub(super) fn build_view_with_context(
  b: &Build,
  project_id: Uuid,
  project_name: &str,
  jobset_id: Uuid,
  jobset_name: &str,
) -> BuildView {
  let mut v = build_view(b);
  v.project_id = Some(project_id);
  v.project_name = project_name.to_string();
  v.jobset_id = Some(jobset_id);
  v.jobset_name = jobset_name.to_string();
  v
}

impl From<&Evaluation> for EvalView {
  fn from(e: &Evaluation) -> Self {
    let (text, class) = e.status.badge();
    let short = if e.commit_hash.len() > 12 {
      e.commit_hash[..12].to_string()
    } else {
      e.commit_hash.clone()
    };
    Self {
      id:            e.id,
      commit_hash:   e.commit_hash.clone(),
      commit_short:  short,
      status_text:   text.to_string(),
      status_class:  class.to_string(),
      time:          e.evaluation_time.format("%Y-%m-%d %H:%M").to_string(),
      error_message: e.error_message.clone(),
      hidden:        e.hidden,
      jobset_name:   String::new(),
      project_name:  String::new(),
    }
  }
}

pub(super) fn eval_view(e: &Evaluation) -> EvalView {
  EvalView::from(e)
}

pub(super) fn eval_view_with_context(
  e: &Evaluation,
  jobset_name: &str,
  project_name: &str,
) -> EvalView {
  let mut v = eval_view(e);
  v.jobset_name = jobset_name.to_string();
  v.project_name = project_name.to_string();
  v
}

pub(super) fn status_badge(s: BuildStatus) -> (String, String) {
  let (text, class) = s.badge();
  (text.to_string(), class.to_string())
}

pub(super) fn eval_badge(s: &EvaluationStatus) -> (String, String) {
  let (text, class) = s.badge();
  (text.to_string(), class.to_string())
}

pub(super) fn is_admin(extensions: &Extensions) -> bool {
  permissions::check(extensions, Permission::Admin)
}

pub(super) fn is_authenticated(extensions: &Extensions) -> bool {
  extensions.get::<User>().is_some() || extensions.get::<ApiKey>().is_some()
}

pub(super) fn auth_name(extensions: &Extensions) -> String {
  if let Some(user) = extensions.get::<User>() {
    return user.username.clone();
  }
  extensions
    .get::<ApiKey>()
    .map(|k| k.name.clone())
    .unwrap_or_default()
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn parse_build_error_extracts_msg_and_classifies_level() {
    let raw = [
      r#"@nix {"action":"msg","level":0,"msg":"error: boom"}"#,
      r#"@nix {"action":"msg","level":3,"msg":"hello"}"#,
    ]
    .join(
      "
",
    );
    let lines = parse_build_error(&raw);
    assert_eq!(lines.len(), 2);
    assert_eq!(lines[0].text, "error: boom");
    assert_eq!(lines[0].level, "error");
    assert_eq!(lines[1].text, "hello");
    assert_eq!(lines[1].level, "notice");
  }

  #[test]
  fn parse_build_error_keeps_plain_lines() {
    let raw =
      ["Error:", r#"@nix {"action":"msg","level":0,"msg":"boom"}"#].join("\n");
    let lines = parse_build_error(&raw);
    assert_eq!(lines.len(), 2);
    assert_eq!(lines[0].text, "Error"); // trailing ':' trimmed
    assert_eq!(lines[0].level, "info");
    assert_eq!(lines[1].text, "boom");
  }

  #[test]
  fn parse_build_error_empty_returns_empty() {
    assert!(parse_build_error("").is_empty());
    assert!(parse_build_error("   ").is_empty());
  }

  #[test]
  fn parse_build_error_skips_non_msg_actions() {
    let raw = [
      r#"@nix {"action":"start","id":1,"text":"x"}"#,
      r#"@nix {"action":"msg","level":1,"msg":"warn line"}"#,
    ]
    .join("\n");
    let lines = parse_build_error(&raw);
    assert_eq!(lines.len(), 1);
    assert_eq!(lines[0].text, "warn line");
    assert_eq!(lines[0].level, "warn");
  }

  #[test]
  fn decode_build_log_actually_decodes() {
    let raw = [
      r#"@nix {"action":"start","id":1}"#,
      r#"@nix {"action":"result","id":1,"type":101,"fields":["cc -c main.c"]}"#,
      r#"@nix {"action":"result","id":1,"type":105,"fields":[0,1]}"#,
      r#"@nix {"action":"msg","level":0,"msg":"error: build failed"}"#,
      "plain stdout line",
      r#"@nix {"action":"stop","id":1}"#,
    ]
    .join("\n");

    // 101 + msg kept
    assert_eq!(
      decode_build_log(&raw),
      "cc -c main.c\nerror: build failed\nplain stdout line\n"
    );

    // ANSI escapes stripped
    assert_eq!(decode_build_log("\x1b[1mbold\x1b[0m"), "bold\n");
  }
}
