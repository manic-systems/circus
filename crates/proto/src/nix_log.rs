//! Parsing for nix `--log-format internal-json` (`@nix {...}`) output, shared
//! by the agent and server.

use serde_json::Value;

const BUILD_LOG_LINE: i64 = 101;
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
