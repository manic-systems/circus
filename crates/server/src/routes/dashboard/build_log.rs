//! Structured Nix build-log parsing for the dashboard log page.

use std::collections::HashMap;

use cognos::internal::json::{
  self as nix_json,
  Actions,
  Activities,
  Id,
  ResultType,
};

use super::shared::{
  classify_plain_line,
  classify_verbosity,
  display_log_line,
  strip_ansi,
};

#[derive(Clone, Copy, Eq, PartialEq)]
enum Severity {
  Error,
  Warn,
  Notice,
  Info,
  Debug,
  Phase,
}

impl Severity {
  const fn css_class(self) -> &'static str {
    match self {
      Self::Error => "error",
      Self::Warn => "warn",
      Self::Notice => "notice",
      Self::Info => "info",
      Self::Debug => "debug",
      Self::Phase => "phase",
    }
  }
}

impl From<&'static str> for Severity {
  fn from(class: &'static str) -> Self {
    match class {
      "error" => Self::Error,
      "warn" => Self::Warn,
      "notice" => Self::Notice,
      "debug" => Self::Debug,
      "phase" => Self::Phase,
      _ => Self::Info,
    }
  }
}

#[derive(Clone, Copy, Eq, PartialEq)]
enum ActivityStatus {
  Running,
  Success,
  Failed,
  Open,
}

impl ActivityStatus {
  const fn label(self) -> &'static str {
    match self {
      Self::Running => "running",
      Self::Success => "completed",
      Self::Failed => "failed",
      Self::Open => "open",
    }
  }

  const fn css_class(self) -> &'static str {
    match self {
      Self::Running | Self::Open => "running",
      Self::Success => "success",
      Self::Failed => "failed",
    }
  }
}

#[derive(Clone, Copy)]
enum ActivityKind {
  Unknown,
  CopyPath,
  FileTransfer,
  Realise,
  CopyPaths,
  Builds,
  Build,
  OptimiseStore,
  VerifyPath,
  Substitute,
  QueryPathInfo,
  PostBuildHook,
  BuildWaiting,
  FetchTree,
}

impl ActivityKind {
  const fn label(self) -> &'static str {
    match self {
      Self::Unknown => "activity",
      Self::CopyPath => "copy path",
      Self::FileTransfer => "file transfer",
      Self::Realise => "realise",
      Self::CopyPaths => "copy paths",
      Self::Builds => "build set",
      Self::Build => "build",
      Self::OptimiseStore => "optimise store",
      Self::VerifyPath => "verify path",
      Self::Substitute => "substitute",
      Self::QueryPathInfo => "query path",
      Self::PostBuildHook => "post-build hook",
      Self::BuildWaiting => "waiting",
      Self::FetchTree => "fetch tree",
    }
  }
}

impl From<Activities> for ActivityKind {
  fn from(activity: Activities) -> Self {
    match activity {
      Activities::Unknown => Self::Unknown,
      Activities::CopyPath => Self::CopyPath,
      Activities::FileTransfer => Self::FileTransfer,
      Activities::Realise => Self::Realise,
      Activities::CopyPaths => Self::CopyPaths,
      Activities::Builds => Self::Builds,
      Activities::Build => Self::Build,
      Activities::OptimiseStore => Self::OptimiseStore,
      Activities::VerifyPath => Self::VerifyPath,
      Activities::Substitute => Self::Substitute,
      Activities::QueryPathInfo => Self::QueryPathInfo,
      Activities::PostBuildHook => Self::PostBuildHook,
      Activities::BuildWaiting => Self::BuildWaiting,
      Activities::FetchTree => Self::FetchTree,
    }
  }
}

#[derive(Clone, Copy, Eq, PartialEq)]
enum BuildLogLineKind {
  Message,
  Phase,
}

pub(super) struct BuildLogSummaryView {
  pub(super) visible_lines:  usize,
  pub(super) activity_count: usize,
  pub(super) error_count:    usize,
  pub(super) warning_count:  usize,
}

pub(super) struct BuildLogActivityView {
  pub(super) depth:      usize,
  kind:                  ActivityKind,
  pub(super) label:      String,
  pub(super) detail:     String,
  status:                ActivityStatus,
  pub(super) line_count: usize,
  pub(super) line_range: String,
  first_line:            Option<usize>,
  last_line:             Option<usize>,
}

impl BuildLogActivityView {
  pub(super) const fn kind(&self) -> &'static str {
    self.kind.label()
  }

  pub(super) const fn status(&self) -> &'static str {
    self.status.label()
  }

  pub(super) const fn status_class(&self) -> &'static str {
    self.status.css_class()
  }

  pub(super) const fn is_failed(&self) -> bool {
    matches!(self.status, ActivityStatus::Failed)
  }
}

pub(super) struct BuildLogLineView {
  pub(super) number: usize,
  level:             Severity,
  kind:              BuildLogLineKind,
  pub(super) text:   String,
}

impl BuildLogLineView {
  pub(super) const fn level_class(&self) -> &'static str {
    self.level.css_class()
  }

  pub(super) const fn is_phase(&self) -> bool {
    matches!(self.kind, BuildLogLineKind::Phase)
  }
}

pub(super) struct BuildLogView {
  pub(super) summary:    BuildLogSummaryView,
  pub(super) activities: Vec<BuildLogActivityView>,
  pub(super) lines:      Vec<BuildLogLineView>,
}

fn field_as_display(value: &serde_json::Value) -> Option<String> {
  match value {
    serde_json::Value::String(s) => Some(s.clone()),
    serde_json::Value::Number(n) => Some(n.to_string()),
    serde_json::Value::Bool(b) => Some(b.to_string()),
    _ => None,
  }
}

fn first_field(fields: &[serde_json::Value]) -> String {
  fields.iter().find_map(field_as_display).unwrap_or_default()
}

fn field_at(fields: &[serde_json::Value], index: usize) -> String {
  fields
    .get(index)
    .and_then(field_as_display)
    .unwrap_or_default()
}

fn activity_depth(
  activities: &[BuildLogActivityView],
  activity_index: &HashMap<Id, usize>,
  parent: Id,
) -> usize {
  if parent == 0 {
    return 0;
  }
  activity_index
    .get(&parent)
    .map_or(0, |idx| activities[*idx].depth.saturating_add(1))
    .min(6)
}

fn activity_label(text: &str, fields: &[serde_json::Value]) -> String {
  let text = strip_ansi(text).trim().to_string();
  if text.is_empty() {
    first_field(fields)
  } else {
    text
  }
}

fn activity_detail(
  kind: ActivityKind,
  fields: &[serde_json::Value],
  label: &str,
) -> String {
  let detail = match kind {
    ActivityKind::Builds | ActivityKind::Unknown => String::new(),
    ActivityKind::Build => field_at(fields, 0),
    ActivityKind::Substitute => {
      let cache = field_at(fields, 1);
      if cache.starts_with("http://") || cache.starts_with("https://") {
        format!("from {cache}")
      } else {
        field_at(fields, 0)
      }
    },
    _ => first_field(fields),
  };

  if detail == label {
    String::new()
  } else {
    detail
  }
}

fn record_activity_line(
  activities: &mut [BuildLogActivityView],
  idx: usize,
  line_number: usize,
) {
  let activity = &mut activities[idx];
  activity.line_count += 1;
  if activity.first_line.is_none() {
    activity.first_line = Some(line_number);
  }
  activity.last_line = Some(line_number);
  activity.line_range = match (activity.first_line, activity.last_line) {
    (Some(first), Some(last)) if first == last => format!("line {first}"),
    (Some(first), Some(last)) => format!("lines {first}-{last}"),
    _ => String::new(),
  };
}

pub(super) fn parse_build_log(raw: &str) -> BuildLogView {
  let mut activities = Vec::<BuildLogActivityView>::new();
  let mut activity_index = HashMap::<Id, usize>::new();
  let mut active_stack = Vec::<Id>::new();
  let mut lines = Vec::<BuildLogLineView>::new();
  let mut error_count = 0usize;
  let mut warning_count = 0usize;

  for raw_line in raw.lines() {
    match nix_json::parse_line(raw_line) {
      Some(Actions::Start {
        id,
        parent,
        text,
        activity,
        fields,
        ..
      }) => {
        let kind = ActivityKind::from(activity);
        let label = activity_label(&text, &fields);
        let detail = activity_detail(kind, &fields, &label);
        let depth = activity_depth(&activities, &activity_index, parent);
        activity_index.insert(id, activities.len());
        activities.push(BuildLogActivityView {
          depth,
          kind,
          label: if label.is_empty() {
            kind.label().to_string()
          } else {
            label
          },
          detail,
          status: ActivityStatus::Running,
          line_count: 0,
          line_range: String::new(),
          first_line: None,
          last_line: None,
        });
        active_stack.push(id);
      },
      Some(Actions::Stop { id }) => {
        if let Some(idx) = activity_index.get(&id).copied()
          && activities[idx].status == ActivityStatus::Running
        {
          activities[idx].status = ActivityStatus::Success;
        }
        while let Some(active_id) = active_stack.pop() {
          if active_id == id {
            break;
          }
        }
      },
      Some(Actions::Message {
        level,
        msg,
        raw_msg,
        ..
      }) => {
        let text = display_log_line(raw_msg.as_deref().unwrap_or(&msg));
        if text.is_empty() {
          continue;
        }
        let level = Severity::from(classify_verbosity(level));
        if level == Severity::Error {
          error_count += 1;
          if let Some(idx) = active_stack
            .last()
            .and_then(|id| activity_index.get(id))
            .copied()
          {
            activities[idx].status = ActivityStatus::Failed;
          }
        } else if level == Severity::Warn {
          warning_count += 1;
        }
        if let Some(idx) = active_stack
          .last()
          .and_then(|id| activity_index.get(id))
          .copied()
        {
          record_activity_line(&mut activities, idx, lines.len() + 1);
        }
        lines.push(BuildLogLineView {
          number: lines.len() + 1,
          level,
          kind: BuildLogLineKind::Message,
          text,
        });
      },
      Some(Actions::Result {
        id,
        result_type: ResultType::BuildLogLine | ResultType::PostBuildLogLine,
        fields,
      }) => {
        let Some(text) = fields.first().and_then(serde_json::Value::as_str)
        else {
          continue;
        };
        let text = display_log_line(text);
        if text.is_empty() {
          continue;
        }
        let level = Severity::from(classify_plain_line(&text));
        if level == Severity::Error {
          error_count += 1;
        } else if level == Severity::Warn {
          warning_count += 1;
        }
        if let Some(idx) = activity_index.get(&id).copied() {
          record_activity_line(&mut activities, idx, lines.len() + 1);
          if level == Severity::Error {
            activities[idx].status = ActivityStatus::Failed;
          }
        }
        lines.push(BuildLogLineView {
          number: lines.len() + 1,
          level,
          kind: BuildLogLineKind::Message,
          text,
        });
      },
      Some(Actions::Result {
        result_type: ResultType::SetPhase,
        fields,
        ..
      }) => {
        let phase = first_field(&fields);
        if !phase.is_empty() {
          lines.push(BuildLogLineView {
            number: lines.len() + 1,
            level:  Severity::Phase,
            kind:   BuildLogLineKind::Phase,
            text:   phase,
          });
        }
      },
      None if !raw_line.starts_with("@nix ") => {
        let text = display_log_line(raw_line);
        if text.is_empty() {
          continue;
        }
        let level = Severity::from(classify_plain_line(&text));
        if level == Severity::Error {
          error_count += 1;
        } else if level == Severity::Warn {
          warning_count += 1;
        }
        lines.push(BuildLogLineView {
          number: lines.len() + 1,
          level,
          kind: BuildLogLineKind::Message,
          text,
        });
      },
      Some(Actions::Result { .. }) | None => {},
    }
  }

  for activity in &mut activities {
    if activity.status == ActivityStatus::Running {
      activity.status = ActivityStatus::Open;
    }
  }

  BuildLogView {
    summary: BuildLogSummaryView {
      visible_lines: lines.len(),
      activity_count: activities.len(),
      error_count,
      warning_count,
    },
    activities,
    lines,
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn parse_build_log_decodes_internal_json() {
    let raw = [
      r#"@nix {"action":"start","id":1}"#,
      r#"@nix {"action":"result","id":1,"type":101,"fields":["cc -c main.c"]}"#,
      r#"@nix {"action":"result","id":1,"type":105,"fields":[0,1]}"#,
      r#"@nix {"action":"msg","level":0,"msg":"error: build failed"}"#,
      "plain stdout line",
      r#"@nix {"action":"stop","id":1}"#,
    ]
    .join("\n");

    let log = parse_build_log(&raw);
    assert_eq!(log.summary.visible_lines, 3);
    assert_eq!(log.summary.error_count, 1);
    assert_eq!(log.lines[0].text, "cc -c main.c");
    assert_eq!(log.lines[1].text, "error: build failed");
    assert_eq!(log.lines[2].text, "plain stdout line");
  }
}
