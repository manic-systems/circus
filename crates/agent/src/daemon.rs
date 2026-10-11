//! Client for the Nix daemon worker protocol, spoken to `nix-daemon --stdio`.
//!
//! Only what one build needs: the handshake, `BuildPathsWithResults`, and
//! `QueryDerivationOutputMap`. Log frames are re-encoded as the `@nix {...}`
//! lines `--log-format internal-json` prints, so build logs keep their shape.
//! The byte layout follows Nix's `worker-protocol.cc` and `daemon.cc`.

use std::io;

use serde_json::{Value, json};
use tokio::{
  io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt},
  sync::mpsc,
};

const WORKER_MAGIC_1: u64 = 0x6E69_7863;
const WORKER_MAGIC_2: u64 = 0x6478_696F;

const CLIENT_MINOR: u64 = 38;
const MIN_MINOR: u64 = 34;

const OP_QUERY_DERIVATION_OUTPUT_MAP: u64 = 41;
const OP_BUILD_PATHS_WITH_RESULTS: u64 = 46;

const STDERR_NEXT: u64 = 0x6F6C_6D67;
const STDERR_READ: u64 = 0x6461_7461;
const STDERR_WRITE: u64 = 0x6461_7416;
pub const STDERR_LAST: u64 = 0x616C_7473;
const STDERR_ERROR: u64 = 0x6378_7470;
pub const STDERR_START_ACTIVITY: u64 = 0x5354_5254;
const STDERR_STOP_ACTIVITY: u64 = 0x5354_4F50;
const STDERR_RESULT: u64 = 0x5253_4C54;

const MAX_STRING_LEN: u64 = 64 << 20;

#[derive(Debug, thiserror::Error)]
pub enum DaemonError {
  #[error("nix-daemon connection failed")]
  Io(#[source] io::Error),
  #[error("nix-daemon sent {0}")]
  Protocol(&'static str),
  #[error(
    "nix-daemon speaks protocol 1.{0}, but building needs 1.{MIN_MINOR} or \
     newer"
  )]
  TooOld(u64),
  #[error("{0}")]
  Remote(String),
}

impl From<io::Error> for DaemonError {
  fn from(error: io::Error) -> Self {
    Self::Io(error)
  }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BuildStatus {
  Built,
  Substituted,
  AlreadyValid,
  PermanentFailure,
  InputRejected,
  OutputRejected,
  TransientFailure,
  CachedFailure,
  TimedOut,
  MiscFailure,
  DependencyFailed,
  LogLimitExceeded,
  NotDeterministic,
  ResolvesToAlreadyValid,
  NoSubstituters,
}

impl TryFrom<u64> for BuildStatus {
  type Error = DaemonError;

  fn try_from(value: u64) -> Result<Self, DaemonError> {
    Ok(match value {
      0 => Self::Built,
      1 => Self::Substituted,
      2 => Self::AlreadyValid,
      3 => Self::PermanentFailure,
      4 => Self::InputRejected,
      5 => Self::OutputRejected,
      6 => Self::TransientFailure,
      7 => Self::CachedFailure,
      8 => Self::TimedOut,
      9 => Self::MiscFailure,
      10 => Self::DependencyFailed,
      11 => Self::LogLimitExceeded,
      12 => Self::NotDeterministic,
      13 => Self::ResolvesToAlreadyValid,
      14 => Self::NoSubstituters,
      _ => return Err(DaemonError::Protocol("an unknown build status")),
    })
  }
}

impl BuildStatus {
  #[must_use]
  pub const fn succeeded(self) -> bool {
    matches!(
      self,
      Self::Built
        | Self::Substituted
        | Self::AlreadyValid
        | Self::ResolvesToAlreadyValid
    )
  }

  /// The exit status `nix-store --realise` gives for this result.
  #[must_use]
  pub const fn exit_status(self) -> i32 {
    match self {
      Self::Built
      | Self::Substituted
      | Self::AlreadyValid
      | Self::ResolvesToAlreadyValid => 0,
      Self::PermanentFailure | Self::TransientFailure => 100,
      Self::TimedOut => 101,
      Self::OutputRejected => 102,
      Self::NotDeterministic => 104,
      Self::InputRejected
      | Self::CachedFailure
      | Self::MiscFailure
      | Self::DependencyFailed
      | Self::LogLimitExceeded
      | Self::NoSubstituters => 1,
    }
  }
}

#[derive(Clone, Copy)]
pub enum BuildMode {
  Normal = 0,
  Repair = 1,
  Check  = 2,
}

pub struct BuildResult {
  pub status: BuildStatus,
  pub error:  String,
}

pub struct Connection<R, W> {
  reader: R,
  writer: W,
  minor:  u64,
  log:    mpsc::Sender<Result<String, String>>,
}

impl<R: AsyncRead + Unpin, W: AsyncWrite + Unpin> Connection<R, W> {
  /// # Errors
  ///
  /// Returns an error if the daemon is unreachable, uses another protocol, or
  /// is too old to report build results.
  pub async fn handshake(
    reader: R,
    writer: W,
    log: mpsc::Sender<Result<String, String>>,
  ) -> Result<Self, DaemonError> {
    let mut conn = Self {
      reader,
      writer,
      minor: 0,
      log,
    };
    conn.write_u64(WORKER_MAGIC_1).await?;
    conn.write_u64(0x100 | CLIENT_MINOR).await?;
    conn.writer.flush().await?;

    if conn.read_u64().await? != WORKER_MAGIC_2 {
      return Err(DaemonError::Protocol("a bad handshake magic"));
    }
    let daemon_version = conn.read_u64().await?;
    if daemon_version >> 8 != 1 {
      return Err(DaemonError::Protocol("an unsupported major version"));
    }
    conn.minor = (daemon_version & 0xFF).min(CLIENT_MINOR);
    if conn.minor < MIN_MINOR {
      return Err(DaemonError::TooOld(conn.minor));
    }

    if conn.minor >= 38 {
      conn.write_u64(0).await?;
      conn.writer.flush().await?;
      conn.skip_strings().await?;
    }
    // CPU affinity and reserveSpace, both obsolete.
    conn.write_u64(0).await?;
    conn.write_u64(0).await?;
    conn.writer.flush().await?;

    conn.read_string().await?;
    if conn.minor >= 35 {
      conn.read_u64().await?;
    }
    conn.finish_work().await?;
    Ok(conn)
  }

  /// # Errors
  ///
  /// Returns an error if the connection breaks or the daemon refuses the
  /// request. A failed build is an [`Ok`] result with a failing status.
  pub async fn build(
    &mut self,
    drv_path: &str,
    mode: BuildMode,
  ) -> Result<BuildResult, DaemonError> {
    self.write_u64(OP_BUILD_PATHS_WITH_RESULTS).await?;
    self.write_u64(1).await?;
    self.write_string(&format!("{drv_path}!*")).await?;
    self.write_u64(mode as u64).await?;
    self.writer.flush().await?;
    self.finish_work().await?;

    if self.read_u64().await? != 1 {
      return Err(DaemonError::Protocol("a result count other than one"));
    }
    self.read_string().await?;
    let status = BuildStatus::try_from(self.read_u64().await?)?;
    let error = self.read_string().await?;
    for _ in 0..4 {
      self.read_u64().await?;
    }
    if self.minor >= 37 {
      for _ in 0..2 {
        if self.read_u64().await? == 1 {
          self.read_u64().await?;
        }
      }
    }
    // builtOutputs is empty in check mode, so [`Self::output_paths`] asks
    // instead.
    for _ in 0..self.read_u64().await? {
      self.read_string().await?;
      self.read_string().await?;
    }
    Ok(BuildResult { status, error })
  }

  /// # Errors
  ///
  /// Returns an error if the connection breaks or the daemon refuses the
  /// query.
  pub async fn output_paths(
    &mut self,
    drv_path: &str,
  ) -> Result<Vec<(String, String)>, DaemonError> {
    self.write_u64(OP_QUERY_DERIVATION_OUTPUT_MAP).await?;
    self.write_string(drv_path).await?;
    self.writer.flush().await?;
    self.finish_work().await?;

    let mut outputs = Vec::new();
    for _ in 0..self.read_u64().await? {
      let name = self.read_string().await?;
      let path = self.read_string().await?;
      if !path.is_empty() {
        outputs.push((name, path));
      }
    }
    Ok(outputs)
  }

  /// A closed log channel means the build is being torn down, and the frames
  /// still have to be read to keep the stream in sync.
  async fn finish_work(&mut self) -> Result<(), DaemonError> {
    loop {
      let line = match self.read_u64().await? {
        STDERR_LAST => return Ok(()),
        STDERR_ERROR => return Err(self.read_error().await?),
        STDERR_NEXT => {
          let text = self.read_string().await?;
          for line in text.trim_end_matches('\n').lines() {
            let _ = self.log.send(Ok(line.to_owned())).await;
          }
          continue;
        },
        STDERR_START_ACTIVITY => {
          let id = self.read_u64().await?;
          let level = self.read_u64().await?;
          let kind = self.read_u64().await?;
          let text = self.read_string().await?;
          let fields = self.read_fields().await?;
          let parent = self.read_u64().await?;
          let mut entry = json!({
            "action": "start",
            "id": id,
            "level": level,
            "parent": parent,
            "text": text,
            "type": kind,
          });
          if !fields.is_empty() {
            entry["fields"] = Value::Array(fields);
          }
          entry
        },
        STDERR_STOP_ACTIVITY => {
          json!({ "action": "stop", "id": self.read_u64().await? })
        },
        STDERR_RESULT => {
          let id = self.read_u64().await?;
          let kind = self.read_u64().await?;
          let fields = self.read_fields().await?;
          json!({ "action": "result", "fields": fields, "id": id, "type": kind })
        },
        STDERR_READ | STDERR_WRITE => {
          return Err(DaemonError::Protocol("a data frame during a build"));
        },
        _ => return Err(DaemonError::Protocol("an unknown stderr frame")),
      };
      let _ = self.log.send(Ok(format!("@nix {line}"))).await;
    }
  }

  async fn read_error(&mut self) -> Result<DaemonError, DaemonError> {
    if self.read_string().await? != "Error" {
      return Err(DaemonError::Protocol("a malformed error"));
    }
    self.read_u64().await?;
    self.read_string().await?;
    let mut message = self.read_string().await?;
    if self.read_u64().await? != 0 {
      return Err(DaemonError::Protocol("an error with a position"));
    }
    for _ in 0..self.read_u64().await? {
      if self.read_u64().await? != 0 {
        return Err(DaemonError::Protocol("an error trace with a position"));
      }
      message.push('\n');
      message.push_str(&self.read_string().await?);
    }
    Ok(DaemonError::Remote(message))
  }

  async fn read_fields(&mut self) -> Result<Vec<Value>, DaemonError> {
    let mut fields = Vec::new();
    for _ in 0..self.read_u64().await? {
      fields.push(match self.read_u64().await? {
        0 => Value::from(self.read_u64().await?),
        1 => Value::from(self.read_string().await?),
        _ => return Err(DaemonError::Protocol("an unknown field type")),
      });
    }
    Ok(fields)
  }

  async fn skip_strings(&mut self) -> Result<(), DaemonError> {
    for _ in 0..self.read_u64().await? {
      self.read_string().await?;
    }
    Ok(())
  }

  async fn read_u64(&mut self) -> Result<u64, DaemonError> {
    Ok(self.reader.read_u64_le().await?)
  }

  /// Nix strings are bytes, and build logs are not always UTF-8.
  async fn read_string(&mut self) -> Result<String, DaemonError> {
    let len = self.read_u64().await?;
    if len > MAX_STRING_LEN {
      return Err(DaemonError::Protocol("an oversized string"));
    }
    let mut bytes = vec![0; len.next_multiple_of(8) as usize];
    self.reader.read_exact(&mut bytes).await?;
    let (text, padding) = bytes.split_at(len as usize);
    if padding.iter().any(|byte| *byte != 0) {
      return Err(DaemonError::Protocol("nonzero string padding"));
    }
    Ok(String::from_utf8_lossy(text).into_owned())
  }

  async fn write_u64(&mut self, value: u64) -> Result<(), DaemonError> {
    Ok(self.writer.write_u64_le(value).await?)
  }

  async fn write_string(&mut self, value: &str) -> Result<(), DaemonError> {
    let len = value.len() as u64;
    self.write_u64(len).await?;
    self.writer.write_all(value.as_bytes()).await?;
    let padding = (len.next_multiple_of(8) - len) as usize;
    self.writer.write_all(&[0; 8][..padding]).await?;
    Ok(())
  }
}
