//! Parsing for nix `--log-format internal-json` (`@nix {...}`) output, shared
//! by the agent and server.

use serde_json::Value;

const BUILD_LOG_LINE: i64 = 101;
const PROGRESS: i64 = 105;
const SET_EXPECTED: i64 = 106;
const POST_BUILD_LOG_LINE: i64 = 107;

/// A parsed `@nix {...}` line that carries displayable text.
pub enum LogLine {
  Message { level: i64, text: String },
  Output { text: String },
}

#[must_use]
pub fn is_envelope(line: &str) -> bool {
  line.starts_with("@nix ")
}

/// Whether `line` is a progress counter update, which nix emits on every
/// transfer tick and which carries no log text.
#[must_use]
pub fn is_progress(line: &str) -> bool {
  let Some(json) = line.strip_prefix("@nix ") else {
    return false;
  };
  let Ok(v) = serde_json::from_str::<Value>(json.trim()) else {
    return false;
  };
  v.get("action").and_then(Value::as_str) == Some("result")
    && matches!(
      v.get("type").and_then(Value::as_i64),
      Some(PROGRESS | SET_EXPECTED)
    )
}

/// # Returns
///
/// Returns [`None`] if `line` is not an envelope, is malformed, or carries no
/// text.
#[must_use]
pub fn parse_line(line: &str) -> Option<LogLine> {
  let v =
    serde_json::from_str::<Value>(line.strip_prefix("@nix ")?.trim()).ok()?;
  match v.get("action")?.as_str()? {
    "msg" => {
      let text = v
        .get("msg")
        .or_else(|| v.get("raw_msg"))?
        .as_str()?
        .to_owned();
      let level = v.get("level").and_then(Value::as_i64).unwrap_or(3);
      Some(LogLine::Message { level, text })
    },
    "result"
      if matches!(
        v.get("type").and_then(Value::as_i64),
        Some(BUILD_LOG_LINE | POST_BUILD_LOG_LINE)
      ) =>
    {
      let text = v.get("fields")?.get(0)?.as_str()?.to_owned();
      Some(LogLine::Output { text })
    },
    _ => None,
  }
}
