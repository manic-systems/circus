//! View models, formatting helpers, status-badge mappings, and per-request
//! auth helpers shared across all dashboard handlers. Everything here is
//! `pub(super)` so sibling modules (auth, admin, pages, ...) can use them
//! without re-exporting them at the dashboard module's external surface.

use axum::{
  http::Extensions,
  response::{IntoResponse, Redirect, Response},
};
use circus_common::{
  config::{PageAccessLevel, ServerConfig},
  models::{ApiKey, Build, BuildStatus, Evaluation, EvaluationStatus, User},
};
use circus_proto::nix_log::{self, LogLine};
use uuid::Uuid;

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
    }
  }
}

pub(super) fn enforce_page_access(
  config: &ServerConfig,
  extensions: &Extensions,
  page: DashboardPage,
) -> Result<(), Response> {
  #![expect(
    clippy::result_large_err,
    reason = "Dashboard handlers return axum Response directly; boxing would \
              add noise at every call site"
  )]

  let allowed = match page.access(config) {
    PageAccessLevel::Public => true,
    PageAccessLevel::Authenticated => is_authenticated(extensions),
    PageAccessLevel::Admin => is_admin(extensions),
  };
  if allowed {
    return Ok(());
  }

  let target = if is_authenticated(extensions) {
    "/"
  } else {
    "/login"
  };
  Err(Redirect::to(target).into_response())
}

pub(super) struct ProjectSummaryView {
  pub(super) id:               Uuid,
  pub(super) name:             String,
  pub(super) jobset_count:     i64,
  pub(super) last_eval_status: String,
  pub(super) last_eval_class:  String,
  pub(super) last_eval_time:   String,
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

/// Strip ANSI/CSI escape sequences.
fn strip_ansi(s: &str) -> String {
  let mut out = String::with_capacity(s.len());
  let mut chars = s.chars().peekable();
  while let Some(c) = chars.next() {
    if c == '\u{1b}' && chars.peek() == Some(&'[') {
      chars.next();
      for esc in chars.by_ref() {
        if esc.is_ascii_alphabetic() {
          break;
        }
      }
    } else {
      out.push(c);
    }
  }
  out
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

pub(super) fn build_view(b: &Build) -> BuildView {
  let (text, class) = status_badge(b.status);
  BuildView {
    id:            b.id,
    job_name:      b.job_name.clone(),
    project_id:    None,
    project_name:  String::new(),
    jobset_id:     None,
    jobset_name:   String::new(),
    status_text:   text,
    status_class:  class,
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

pub(super) fn eval_view(e: &Evaluation) -> EvalView {
  let (text, class) = eval_badge(&e.status);
  let short = if e.commit_hash.len() > 12 {
    e.commit_hash[..12].to_string()
  } else {
    e.commit_hash.clone()
  };
  EvalView {
    id:            e.id,
    commit_hash:   e.commit_hash.clone(),
    commit_short:  short,
    status_text:   text,
    status_class:  class,
    time:          e.evaluation_time.format("%Y-%m-%d %H:%M").to_string(),
    error_message: e.error_message.clone(),
    hidden:        e.hidden,
    jobset_name:   String::new(),
    project_name:  String::new(),
  }
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
  match s {
    BuildStatus::Succeeded => ("Succeeded".into(), "succeeded".into()),
    BuildStatus::Failed => ("Failed".into(), "failed".into()),
    BuildStatus::Running => ("Running".into(), "running".into()),
    BuildStatus::Pending => ("Pending".into(), "pending".into()),
    BuildStatus::Cancelled => ("Cancelled".into(), "cancelled".into()),
    BuildStatus::DependencyFailed => {
      ("Dependency Failed".into(), "failed".into())
    },
    BuildStatus::Aborted => ("Aborted".into(), "aborted".into()),
    BuildStatus::FailedWithOutput => {
      ("Failed w/ Output".into(), "failed".into())
    },
    BuildStatus::Timeout => ("Timeout".into(), "failed".into()),
    BuildStatus::CachedFailure => ("Cached Failure".into(), "failed".into()),
    BuildStatus::UnsupportedSystem => {
      ("Unsupported System".into(), "skipped".into())
    },
    BuildStatus::LogLimitExceeded => ("Log Limit".into(), "failed".into()),
    BuildStatus::NarSizeLimitExceeded => {
      ("NAR Size Limit".into(), "failed".into())
    },
    BuildStatus::NonDeterministic => {
      ("Non-deterministic".into(), "failed".into())
    },
  }
}

pub(super) fn eval_badge(s: &EvaluationStatus) -> (String, String) {
  match s {
    EvaluationStatus::Completed => ("Completed".into(), "completed".into()),
    EvaluationStatus::Failed => ("Failed".into(), "failed".into()),
    EvaluationStatus::Running => ("Running".into(), "running".into()),
    EvaluationStatus::Pending => ("Pending".into(), "pending".into()),
  }
}

pub(super) fn is_admin(extensions: &Extensions) -> bool {
  if let Some(user) = extensions.get::<User>() {
    return user.role == "admin";
  }
  extensions
    .get::<ApiKey>()
    .is_some_and(|k| k.role == "admin")
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
