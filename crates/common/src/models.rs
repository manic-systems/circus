//! Data models for CI

use chrono::{DateTime, Utc};
pub use circus_types::{
  AuthKind,
  BinaryCacheUpstream,
  BinaryCacheUpstreams,
  ForgeType,
  InputType,
  NotificationType,
};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use uuid::Uuid;

use crate::roles::{GlobalRole, ProjectRole};

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct Project {
  pub id:              Uuid,
  pub name:            String,
  pub description:     Option<String>,
  pub repository_url:  String,
  pub cache_enabled:   bool,
  pub cache_url:       Option<String>,
  pub cache_upstreams: sqlx::types::Json<BinaryCacheUpstreams>,
  pub created_at:      DateTime<Utc>,
  pub updated_at:      DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct Jobset {
  pub id:                Uuid,
  pub project_id:        Uuid,
  pub name:              String,
  pub nix_expression:    String,
  pub enabled:           bool,
  pub flake_mode:        bool,
  pub check_interval:    i32,
  pub trigger_mode:      JobsetTriggerMode,
  pub branch:            Option<String>,
  pub branch_pattern:    Option<String>,
  pub tag_pattern:       Option<String>,
  pub scheduling_shares: i32,
  pub created_at:        DateTime<Utc>,
  pub updated_at:        DateTime<Utc>,
  pub state:             JobsetState,
  pub last_checked_at:   Option<DateTime<Utc>>,
  pub keep_nr:           i32,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct Evaluation {
  pub id:              Uuid,
  pub jobset_id:       Uuid,
  pub commit_hash:     String,
  pub evaluation_time: DateTime<Utc>,
  pub status:          EvaluationStatus,
  pub error_message:   Option<String>,
  pub inputs_hash:     Option<String>,
  pub trigger_kind:    EvaluationTriggerKind,
  pub hidden:          bool,
  pub pr_number:       Option<i32>,
  pub pr_head_branch:  Option<String>,
  pub pr_base_branch:  Option<String>,
  pub pr_action:       Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, sqlx::Type)]
#[serde(rename_all = "lowercase")]
#[sqlx(type_name = "text", rename_all = "lowercase")]
pub enum EvaluationStatus {
  Pending,
  Running,
  Completed,
  Failed,
  Cancelled,
  TimedOut,
}

impl EvaluationStatus {
  #[must_use]
  pub const fn badge(&self) -> (&'static str, &'static str) {
    match self {
      Self::Completed => ("Completed", "completed"),
      Self::Failed => ("Failed", "failed"),
      Self::Cancelled => ("Cancelled", "cancelled"),
      Self::TimedOut => ("Timed out", "timed-out"),
      Self::Running => ("Running", "running"),
      Self::Pending => ("Pending", "pending"),
    }
  }
}

#[derive(
  Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, sqlx::Type, Default,
)]
#[serde(rename_all = "snake_case")]
#[sqlx(type_name = "varchar", rename_all = "snake_case")]
pub enum EvaluationTriggerKind {
  #[default]
  SourceChange,
  Manual,
  Interval,
}

/// Jobset scheduling state (Hydra-compatible).
///
/// - `Disabled`: Jobset will not be evaluated
/// - `Enabled`: Normal operation, evaluated according to its trigger mode
/// - `OneShot`: Evaluated once, then automatically set to Disabled
/// - `OneAtATime`: Only one build can run at a time for this jobset
#[derive(
  Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, sqlx::Type, Default,
)]
#[serde(rename_all = "snake_case")]
#[sqlx(type_name = "varchar", rename_all = "snake_case")]
pub enum JobsetState {
  Disabled,
  #[default]
  Enabled,
  OneShot,
  OneAtATime,
}

impl JobsetState {
  /// Returns true if this jobset state allows evaluation.
  ///
  /// # Returns
  ///
  /// `true` when the jobset is [`Enabled`], [`OneShot`], or [`OneAtATime`].
  #[must_use]
  pub const fn is_evaluable(&self) -> bool {
    matches!(self, Self::Enabled | Self::OneShot | Self::OneAtATime)
  }

  /// # Returns
  ///
  /// Returns the database string representation of this state.
  #[must_use]
  pub const fn as_str(&self) -> &'static str {
    match self {
      Self::Disabled => "disabled",
      Self::Enabled => "enabled",
      Self::OneShot => "one_shot",
      Self::OneAtATime => "one_at_a_time",
    }
  }

  /// Parses a state string from declarative config.
  /// Unrecognised values default to `Enabled`.
  #[must_use]
  pub fn from_config_str(s: &str) -> Self {
    match s {
      "disabled" => Self::Disabled,
      "one_shot" => Self::OneShot,
      "one_at_a_time" => Self::OneAtATime,
      _ => Self::Enabled,
    }
  }
}

/// How a jobset enters the evaluator.
#[derive(
  Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, sqlx::Type, Default,
)]
#[serde(rename_all = "snake_case")]
#[sqlx(type_name = "varchar", rename_all = "snake_case")]
pub enum JobsetTriggerMode {
  /// Rebuild when a source/manual trigger or polling discovers new inputs.
  #[default]
  SourceChange,
  /// Rebuild on the jobset interval, even when inputs did not change.
  Interval,
}

impl JobsetTriggerMode {
  /// # Returns
  ///
  /// Returns true when webhook/manual pending evaluations should be accepted.
  #[must_use]
  pub const fn accepts_source_triggers(&self) -> bool {
    matches!(self, Self::SourceChange)
  }

  /// # Returns
  ///
  /// Returns the database string representation of this trigger mode.
  #[must_use]
  pub const fn as_str(&self) -> &'static str {
    match self {
      Self::SourceChange => "source_change",
      Self::Interval => "interval",
    }
  }

  /// Parses a trigger mode from declarative config.
  /// Unrecognised values default to `SourceChange`.
  #[must_use]
  pub fn from_config_str(s: &str) -> Self {
    match s {
      "interval" => Self::Interval,
      _ => Self::SourceChange,
    }
  }
}

/// Job-name prefix marking an intermediate dependency build synthesized by the
/// evaluator from the derivation graph, as opposed to a top-level jobset job.
/// These builds are internal scheduling artifacts and are excluded from
/// user-facing notifications (commit statuses, webhooks, ...).
pub const DEPENDENCY_JOB_PREFIX: &str = "drv:";

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
#[expect(
  clippy::struct_excessive_bools,
  reason = "Build is a database row matching a well-known schema; the bools \
            represent orthogonal flags"
)]
pub struct Build {
  pub id:                         Uuid,
  pub evaluation_id:              Uuid,
  pub job_name:                   String,
  pub drv_path:                   String,
  pub status:                     BuildStatus,
  pub started_at:                 Option<DateTime<Utc>>,
  pub completed_at:               Option<DateTime<Utc>>,
  pub log_path:                   Option<String>,
  pub build_output_path:          Option<String>,
  pub error_message:              Option<String>,
  pub system:                     Option<String>,
  pub priority:                   i32,
  pub retry_count:                i32,
  pub max_retries:                i32,
  pub notification_pending_since: Option<DateTime<Utc>>,
  pub created_at:                 DateTime<Utc>,
  pub outputs:                    Option<serde_json::Value>,
  pub is_aggregate:               bool,
  pub constituents:               Option<serde_json::Value>,
  pub builder_id:                 Option<Uuid>,
  pub agent_machine_id:           Option<Uuid>,
  pub signed:                     bool,
  pub keep:                       bool,
  pub is_fod:                     bool,
  pub fod_hash:                   Option<String>,
  pub meta_description:           Option<String>,
  pub meta_license:               Option<String>,
  pub meta_homepage:              Option<String>,
  pub meta_maintainers:           Option<String>,
  /// Features the derivation declares via `requiredSystemFeatures`. Empty
  /// list = build is feature-agnostic. Populated by the evaluator from the
  /// drv JSON; consumed by the scheduler to gate which agent or SSH builder
  /// is eligible.
  #[serde(default)]
  pub required_features:          Vec<String>,
  /// `requiredSystemFeatures` unioned over the drvs the venue will actually
  /// build. [`None`] means this was not yet computed.
  #[serde(default)]
  pub effective_features:         Option<Vec<String>>,
}

impl Build {
  /// The effective features, else the job drv's own `required_features`.
  #[must_use]
  pub fn scheduling_features(&self) -> &[String] {
    self
      .effective_features
      .as_deref()
      .unwrap_or(&self.required_features)
  }

  /// Whether this is an intermediate dependency build synthesized from the
  /// derivation graph rather than a top-level jobset job. See
  /// [`DEPENDENCY_JOB_PREFIX`].
  #[must_use]
  pub fn is_dependency(&self) -> bool {
    self.job_name.starts_with(DEPENDENCY_JOB_PREFIX)
  }
}

#[derive(
  Debug,
  Clone,
  Copy,
  Serialize,
  Deserialize,
  PartialEq,
  Eq,
  num_enum::IntoPrimitive,
  num_enum::TryFromPrimitive,
)]
#[repr(i32)]
#[serde(rename_all = "snake_case")]
pub enum BuildStatus {
  Pending              = 0,
  Running              = 1,
  Succeeded            = 2,
  Failed               = 3,
  DependencyFailed     = 4,
  Aborted              = 5,
  Cancelled            = 6,
  FailedWithOutput     = 7,
  Timeout              = 8,
  CachedFailure        = 9,
  UnsupportedSystem    = 10,
  LogLimitExceeded     = 11,
  NarSizeLimitExceeded = 12,
  NonDeterministic     = 13,
  OomKilled            = 14,
}

impl BuildStatus {
  /// # Returns
  ///
  /// Returns true if the build has completed (not pending or running).
  #[must_use]
  pub const fn is_finished(&self) -> bool {
    !matches!(self, Self::Pending | Self::Running)
  }

  /// # Returns
  ///
  /// Returns true if the build succeeded.
  /// Note: Does NOT include `CachedFailure` - a cached failure is still a
  /// failure.
  #[must_use]
  pub const fn is_success(&self) -> bool {
    matches!(self, Self::Succeeded)
  }

  /// # Returns
  ///
  /// Returns true if the build completed without needing a retry.
  /// This includes both successful builds and cached failures.
  #[must_use]
  pub const fn is_terminal(&self) -> bool {
    matches!(
      self,
      Self::Succeeded
        | Self::Failed
        | Self::CachedFailure
        | Self::DependencyFailed
        | Self::Aborted
        | Self::Cancelled
        | Self::FailedWithOutput
        | Self::Timeout
        | Self::UnsupportedSystem
        | Self::LogLimitExceeded
        | Self::NarSizeLimitExceeded
        | Self::NonDeterministic
        | Self::OomKilled
    )
  }

  /// # Returns
  ///
  /// Returns the database integer representation of this status.
  /// Note: This uses an internal numbering scheme (0-14), not Hydra exit codes.
  #[must_use]
  pub fn as_i32(&self) -> i32 {
    (*self).into()
  }

  /// Converts a database integer to `BuildStatus`.
  /// This is the inverse of `as_i32()` for reading from the database.
  #[must_use]
  pub fn from_i32(code: i32) -> Option<Self> {
    Self::try_from(code).ok()
  }

  const fn db_str(self) -> &'static str {
    match self {
      Self::Pending => "pending",
      Self::Running => "running",
      Self::Succeeded => "succeeded",
      Self::Failed => "failed",
      Self::DependencyFailed => "dependency_failed",
      Self::Aborted => "aborted",
      Self::Cancelled => "cancelled",
      Self::FailedWithOutput => "failed_with_output",
      Self::Timeout => "timeout",
      Self::CachedFailure => "cached_failure",
      Self::UnsupportedSystem => "unsupported_system",
      Self::LogLimitExceeded => "log_limit_exceeded",
      Self::NarSizeLimitExceeded => "nar_size_limit_exceeded",
      Self::NonDeterministic => "non_deterministic",
      Self::OomKilled => "oom_killed",
    }
  }

  fn from_db_str(status: &str) -> Option<Self> {
    match status {
      "pending" => Some(Self::Pending),
      "running" => Some(Self::Running),
      "succeeded" => Some(Self::Succeeded),
      "failed" => Some(Self::Failed),
      "dependency_failed" => Some(Self::DependencyFailed),
      "aborted" => Some(Self::Aborted),
      "cancelled" => Some(Self::Cancelled),
      "failed_with_output" => Some(Self::FailedWithOutput),
      "timeout" => Some(Self::Timeout),
      "cached_failure" => Some(Self::CachedFailure),
      "unsupported_system" => Some(Self::UnsupportedSystem),
      "log_limit_exceeded" => Some(Self::LogLimitExceeded),
      "nar_size_limit_exceeded" => Some(Self::NarSizeLimitExceeded),
      "non_deterministic" => Some(Self::NonDeterministic),
      "oom_killed" => Some(Self::OomKilled),
      _ => None,
    }
  }

  /// Converts a Hydra-compatible exit code to a `BuildStatus`.
  /// Note: These codes follow Hydra's conventions and differ from
  /// `as_i32/from_i32`.
  #[must_use]
  pub const fn from_exit_code(exit_code: i32) -> Self {
    match exit_code {
      0 => Self::Succeeded,
      2 => Self::DependencyFailed,
      3 | 5 => Self::Aborted, // 5 is obsolete in Hydra, treat as aborted
      4 => Self::Cancelled,
      6 => Self::FailedWithOutput,
      7 => Self::Timeout,
      8 => Self::CachedFailure,
      9 => Self::UnsupportedSystem,
      10 => Self::LogLimitExceeded,
      11 => Self::NarSizeLimitExceeded,
      12 => Self::NonDeterministic,
      -9 => Self::OomKilled,
      _ => Self::Failed,
    }
  }

  #[must_use]
  pub const fn badge(self) -> (&'static str, &'static str) {
    match self {
      Self::Succeeded => ("Succeeded", "succeeded"),
      Self::Failed => ("Failed", "failed"),
      Self::Running => ("Running", "running"),
      Self::Pending => ("Pending", "pending"),
      Self::Cancelled => ("Cancelled", "cancelled"),
      Self::DependencyFailed => ("Dependency Failed", "failed"),
      Self::Aborted => ("Aborted", "aborted"),
      Self::FailedWithOutput => ("Failed w/ Output", "failed"),
      Self::Timeout => ("Timeout", "failed"),
      Self::CachedFailure => ("Cached Failure", "failed"),
      Self::UnsupportedSystem => ("Unsupported System", "skipped"),
      Self::LogLimitExceeded => ("Log Limit", "failed"),
      Self::NarSizeLimitExceeded => ("NAR Size Limit", "failed"),
      Self::NonDeterministic => ("Non-deterministic", "failed"),
      Self::OomKilled => ("OOM Killed", "failed"),
    }
  }
}

impl std::fmt::Display for BuildStatus {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    let s = match self {
      Self::Pending => "pending",
      Self::Running => "running",
      Self::Succeeded => "succeeded",
      Self::Failed => "failed",
      Self::DependencyFailed => "dependency failed",
      Self::Aborted => "aborted",
      Self::Cancelled => "cancelled",
      Self::FailedWithOutput => "failed with output",
      Self::Timeout => "timeout",
      Self::CachedFailure => "cached failure",
      Self::UnsupportedSystem => "unsupported system",
      Self::LogLimitExceeded => "log limit exceeded",
      Self::NarSizeLimitExceeded => "nar size limit exceeded",
      Self::NonDeterministic => "non-deterministic",
      Self::OomKilled => "oom killed",
    };
    write!(f, "{s}")
  }
}

impl sqlx::Type<sqlx::Postgres> for BuildStatus {
  fn type_info() -> sqlx::postgres::PgTypeInfo {
    <String as sqlx::Type<sqlx::Postgres>>::type_info()
  }

  fn compatible(ty: &sqlx::postgres::PgTypeInfo) -> bool {
    <String as sqlx::Type<sqlx::Postgres>>::compatible(ty)
  }
}

impl sqlx::Encode<'_, sqlx::Postgres> for BuildStatus {
  fn encode_by_ref(
    &self,
    buf: &mut sqlx::postgres::PgArgumentBuffer,
  ) -> Result<sqlx::encode::IsNull, sqlx::error::BoxDynError> {
    <&str as sqlx::Encode<sqlx::Postgres>>::encode(self.db_str(), buf)
  }
}

impl<'r> sqlx::Decode<'r, sqlx::Postgres> for BuildStatus {
  fn decode(
    value: sqlx::postgres::PgValueRef<'r>,
  ) -> Result<Self, sqlx::error::BoxDynError> {
    let status = <&str as sqlx::Decode<sqlx::Postgres>>::decode(value)?;
    Self::from_db_str(status)
      .ok_or_else(|| {
        std::io::Error::new(
          std::io::ErrorKind::InvalidData,
          format!("unknown build status '{status}'"),
        )
      })
      .map_err(Into::into)
  }
}

#[cfg(test)]
mod build_status_tests {
  use super::BuildStatus;

  #[test]
  fn build_status_i32_round_trips() {
    for (code, status) in [
      (0, BuildStatus::Pending),
      (1, BuildStatus::Running),
      (2, BuildStatus::Succeeded),
      (3, BuildStatus::Failed),
      (4, BuildStatus::DependencyFailed),
      (5, BuildStatus::Aborted),
      (6, BuildStatus::Cancelled),
      (7, BuildStatus::FailedWithOutput),
      (8, BuildStatus::Timeout),
      (9, BuildStatus::CachedFailure),
      (10, BuildStatus::UnsupportedSystem),
      (11, BuildStatus::LogLimitExceeded),
      (12, BuildStatus::NarSizeLimitExceeded),
      (13, BuildStatus::NonDeterministic),
      (14, BuildStatus::OomKilled),
    ] {
      assert_eq!(status.as_i32(), code);
      assert_eq!(BuildStatus::from_i32(code), Some(status));
    }
  }

  #[test]
  fn build_status_from_i32_preserves_unknown_fallback() {
    assert_eq!(BuildStatus::from_i32(-1), None);
    assert_eq!(BuildStatus::from_i32(15), None);
  }
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct BuildProduct {
  pub id:           Uuid,
  pub build_id:     Uuid,
  pub name:         String,
  pub path:         String,
  pub sha256_hash:  Option<String>,
  pub file_size:    Option<i64>,
  pub content_type: Option<String>,
  pub is_directory: bool,
  pub gc_root_path: Option<String>,
  pub created_at:   DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct BuildStep {
  pub id:           Uuid,
  pub build_id:     Uuid,
  pub step_number:  i32,
  pub command:      String,
  pub output:       Option<String>,
  pub error_output: Option<String>,
  pub started_at:   DateTime<Utc>,
  pub completed_at: Option<DateTime<Utc>>,
  pub exit_code:    Option<i32>,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct BuildDependency {
  pub id:                  Uuid,
  pub build_id:            Uuid,
  pub dependency_build_id: Uuid,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct BuildMetric {
  pub id:           Uuid,
  pub build_id:     Uuid,
  pub metric_name:  String,
  pub metric_value: f64,
  pub unit:         String,
  pub collected_at: DateTime<Utc>,
}

pub mod metric_names {
  pub const BUILD_DURATION_SECONDS: &str = "build_duration_seconds";
  pub const OUTPUT_SIZE_BYTES: &str = "output_size_bytes";
}

pub mod metric_units {
  pub const SECONDS: &str = "seconds";
  pub const BYTES: &str = "bytes";
}

/// Active jobsets joined with project info.
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct ActiveJobset {
  pub id:                Uuid,
  pub project_id:        Uuid,
  pub name:              String,
  pub nix_expression:    String,
  pub enabled:           bool,
  pub flake_mode:        bool,
  pub check_interval:    i32,
  pub trigger_mode:      JobsetTriggerMode,
  pub branch:            Option<String>,
  pub branch_pattern:    Option<String>,
  pub tag_pattern:       Option<String>,
  pub scheduling_shares: i32,
  pub created_at:        DateTime<Utc>,
  pub updated_at:        DateTime<Utc>,
  pub state:             JobsetState,
  pub last_checked_at:   Option<DateTime<Utc>>,
  pub keep_nr:           i32,
  pub project_name:      String,
  pub repository_url:    String,
}

/// Build statistics from the `build_stats` view.
#[derive(Debug, Clone, Serialize, Deserialize, FromRow, Default)]
pub struct BuildStats {
  pub total_builds:         Option<i64>,
  pub completed_builds:     Option<i64>,
  pub failed_builds:        Option<i64>,
  pub running_builds:       Option<i64>,
  pub pending_builds:       Option<i64>,
  pub avg_duration_seconds: Option<f64>,
}

/// API key for authentication.
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct ApiKey {
  pub id:           Uuid,
  pub name:         String,
  pub key_hash:     String,
  pub role:         GlobalRole,
  pub user_id:      Option<Uuid>,
  pub created_at:   DateTime<Utc>,
  pub last_used_at: Option<DateTime<Utc>>,
}

/// Webhook configuration for a project.
///
/// `secret_hash` is a legacy column name. New values are encrypted webhook
/// secrets: GitHub/Gitea/Forgejo need the original HMAC key, and GitLab needs
/// the original bearer token, so this cannot be a one-way hash.
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct WebhookConfig {
  pub id:          Uuid,
  pub project_id:  Uuid,
  pub forge_type:  ForgeType,
  /// Encrypted webhook secret. See struct docs.
  #[serde(skip_serializing)]
  pub secret_hash: Option<String>,
  pub enabled:     bool,
  pub created_at:  DateTime<Utc>,
}

/// Notification configuration for a project.
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct NotificationConfig {
  pub id:                Uuid,
  pub project_id:        Uuid,
  pub notification_type: NotificationType,
  pub config:            serde_json::Value,
  pub enabled:           bool,
  pub created_at:        DateTime<Utc>,
}

/// Jobset input definition.
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct JobsetInput {
  pub id:         Uuid,
  pub jobset_id:  Uuid,
  pub name:       String,
  pub input_type: InputType,
  pub value:      String,
  pub revision:   Option<String>,
  pub created_at: DateTime<Utc>,
}

/// Tracks the latest "good" evaluation for a jobset.
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct Channel {
  pub id:                    Uuid,
  pub project_id:            Uuid,
  pub name:                  String,
  pub jobset_id:             Uuid,
  pub current_evaluation_id: Option<Uuid>,
  pub created_at:            DateTime<Utc>,
  pub updated_at:            DateTime<Utc>,
}

/// Remote builder for multi-machine / multi-arch builds.
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct RemoteBuilder {
  pub id:                   Uuid,
  pub name:                 String,
  pub ssh_uri:              String,
  pub systems:              Vec<String>,
  pub max_jobs:             i32,
  pub speed_factor:         i32,
  pub supported_features:   Vec<String>,
  pub mandatory_features:   Vec<String>,
  pub enabled:              bool,
  pub public_host_key:      Option<String>,
  #[serde(skip_serializing)]
  pub ssh_key_file:         Option<String>,
  pub created_at:           DateTime<Utc>,
  pub consecutive_failures: i32,
  pub disabled_until:       Option<DateTime<Utc>>,
  pub last_failure:         Option<DateTime<Utc>>,
  pub cpu_cores:            Option<i32>,
}

/// Parameters for creating or updating a remote builder.
#[derive(Debug, Clone)]
pub struct RemoteBuilderParams<'a> {
  pub name:               &'a str,
  pub ssh_uri:            &'a str,
  pub systems:            &'a [String],
  pub max_jobs:           i32,
  pub speed_factor:       i32,
  pub supported_features: &'a [String],
  pub mandatory_features: &'a [String],
  pub enabled:            bool,
  pub public_host_key:    Option<&'a str>,
  pub ssh_key_file:       Option<&'a str>,
}

/// User account for authentication and personalization
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct User {
  pub id:               Uuid,
  pub username:         String,
  pub email:            String,
  pub full_name:        Option<String>,
  #[serde(skip_serializing)]
  pub password_hash:    Option<String>,
  pub user_type:        UserType,
  pub role:             GlobalRole,
  pub enabled:          bool,
  pub email_verified:   bool,
  pub public_dashboard: bool,
  pub created_at:       DateTime<Utc>,
  pub updated_at:       DateTime<Utc>,
  pub last_login_at:    Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, sqlx::Type)]
#[sqlx(type_name = "varchar", rename_all = "lowercase")]
pub enum UserType {
  Local,
  Github,
  Google,
  Ldap,
}

/// Starred job for personalized dashboard
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct StarredJob {
  pub id:         Uuid,
  pub user_id:    Uuid,
  pub project_id: Uuid,
  pub jobset_id:  Option<Uuid>,
  pub job_name:   String,
  pub created_at: DateTime<Utc>,
}

/// Normalized build output (Hydra-compatible)
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct BuildOutput {
  pub build: Uuid,
  pub name:  String,
  pub path:  Option<String>,
}

/// Project membership for per-project permissions
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct ProjectMember {
  pub id:         Uuid,
  pub project_id: Uuid,
  pub user_id:    Uuid,
  pub role:       ProjectRole,
  pub created_at: DateTime<Utc>,
}

/// User session for persistent authentication
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct UserSession {
  pub id:                 Uuid,
  pub user_id:            Uuid,
  pub session_token_hash: String,
  pub expires_at:         DateTime<Utc>,
  pub created_at:         DateTime<Utc>,
  pub last_used_at:       Option<DateTime<Utc>>,
}

/// Notification task for reliable delivery with retry
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct NotificationTask {
  pub id:                Uuid,
  pub notification_type: NotificationType,
  pub payload:           serde_json::Value,
  pub status:            NotificationTaskStatus,
  pub attempts:          i32,
  pub max_attempts:      i32,
  pub next_retry_at:     DateTime<Utc>,
  pub last_error:        Option<String>,
  pub created_at:        DateTime<Utc>,
  pub completed_at:      Option<DateTime<Utc>>,
}

#[derive(
  Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, sqlx::Type,
)]
#[serde(rename_all = "lowercase")]
#[sqlx(type_name = "varchar", rename_all = "lowercase")]
pub enum NotificationTaskStatus {
  Pending,
  Running,
  Completed,
  Failed,
}

// Pagination

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PaginationParams {
  pub limit:  Option<i64>,
  pub offset: Option<i64>,
}

impl PaginationParams {
  #[must_use]
  pub fn limit(&self) -> i64 {
    self.limit.unwrap_or(50).clamp(1, 200)
  }

  #[must_use]
  pub fn offset(&self) -> i64 {
    self.offset.unwrap_or(0).max(0)
  }
}

impl Default for PaginationParams {
  fn default() -> Self {
    Self {
      limit:  Some(50),
      offset: Some(0),
    }
  }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PaginatedResponse<T> {
  pub items:  Vec<T>,
  pub total:  i64,
  pub limit:  i64,
  pub offset: i64,
}

// DTO structs for creation and updates

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateProject {
  pub name:            String,
  pub description:     Option<String>,
  pub repository_url:  String,
  #[serde(default = "default_project_cache_enabled")]
  pub cache_enabled:   bool,
  pub cache_url:       Option<String>,
  #[serde(default)]
  pub cache_upstreams: BinaryCacheUpstreams,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateProject {
  pub name:            Option<String>,
  pub description:     Option<String>,
  pub repository_url:  Option<String>,
  pub cache_enabled:   Option<bool>,
  pub cache_url:       Option<String>,
  pub cache_upstreams: Option<BinaryCacheUpstreams>,
}

const fn default_project_cache_enabled() -> bool {
  true
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateJobset {
  pub project_id:        Uuid,
  pub name:              String,
  pub nix_expression:    String,
  pub enabled:           Option<bool>,
  pub flake_mode:        Option<bool>,
  pub check_interval:    Option<i32>,
  pub trigger_mode:      Option<JobsetTriggerMode>,
  pub branch:            Option<String>,
  pub branch_pattern:    Option<String>,
  pub tag_pattern:       Option<String>,
  pub scheduling_shares: Option<i32>,
  pub state:             Option<JobsetState>,
  pub keep_nr:           Option<i32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateJobset {
  pub name:              Option<String>,
  pub nix_expression:    Option<String>,
  pub enabled:           Option<bool>,
  pub flake_mode:        Option<bool>,
  pub check_interval:    Option<i32>,
  pub trigger_mode:      Option<JobsetTriggerMode>,
  pub branch:            Option<String>,
  pub branch_pattern:    Option<String>,
  pub tag_pattern:       Option<String>,
  pub scheduling_shares: Option<i32>,
  pub state:             Option<JobsetState>,
  pub keep_nr:           Option<i32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateEvaluation {
  pub jobset_id:      Uuid,
  pub commit_hash:    String,
  pub pr_number:      Option<i32>,
  pub pr_head_branch: Option<String>,
  pub pr_base_branch: Option<String>,
  pub pr_action:      Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct CreateBuild {
  pub evaluation_id:     Uuid,
  pub job_name:          String,
  pub drv_path:          String,
  pub system:            Option<String>,
  pub outputs:           Option<serde_json::Value>,
  pub is_aggregate:      Option<bool>,
  pub constituents:      Option<serde_json::Value>,
  pub is_fod:            Option<bool>,
  pub fod_hash:          Option<String>,
  /// Free-form `meta.description` from the nix expression.
  pub meta_description:  Option<String>,
  /// `meta.license`, rendered to a string. evix surfaces this as an object;
  /// we flatten to its `fullName` (or `spdxId`) when present.
  pub meta_license:      Option<String>,
  /// `meta.homepage` URL.
  pub meta_homepage:     Option<String>,
  /// Comma-separated list of `meta.maintainers[*].github` (or name) handles.
  pub meta_maintainers:  Option<String>,
  /// `requiredSystemFeatures` from the derivation. Empty = no constraint.
  #[serde(default)]
  pub required_features: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateBuildProduct {
  pub build_id:     Uuid,
  pub name:         String,
  pub path:         String,
  pub sha256_hash:  Option<String>,
  pub file_size:    Option<i64>,
  pub content_type: Option<String>,
  pub is_directory: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateBuildStep {
  pub build_id:    Uuid,
  pub step_number: i32,
  pub command:     String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateWebhookConfig {
  pub project_id: Uuid,
  pub forge_type: ForgeType,
  pub secret:     Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateNotificationConfig {
  pub project_id:        Uuid,
  pub notification_type: NotificationType,
  pub config:            serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateChannel {
  pub project_id: Uuid,
  pub name:       String,
  pub jobset_id:  Uuid,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateChannel {
  pub name:      Option<String>,
  pub jobset_id: Option<Uuid>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateRemoteBuilder {
  pub name:               String,
  pub ssh_uri:            String,
  pub systems:            Vec<String>,
  pub max_jobs:           Option<i32>,
  pub speed_factor:       Option<i32>,
  pub supported_features: Option<Vec<String>>,
  pub mandatory_features: Option<Vec<String>>,
  pub public_host_key:    Option<String>,
  pub ssh_key_file:       Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateRemoteBuilder {
  pub name:               Option<String>,
  pub ssh_uri:            Option<String>,
  pub systems:            Option<Vec<String>>,
  pub max_jobs:           Option<i32>,
  pub speed_factor:       Option<i32>,
  pub supported_features: Option<Vec<String>>,
  pub mandatory_features: Option<Vec<String>>,
  pub enabled:            Option<bool>,
  pub public_host_key:    Option<String>,
  pub ssh_key_file:       Option<String>,
}

/// Summary of system status for the admin API.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemStatus {
  pub projects_count:    i64,
  pub jobsets_count:     i64,
  pub evaluations_count: i64,
  pub builds_pending:    i64,
  pub builds_running:    i64,
  pub builds_completed:  i64,
  pub builds_failed:     i64,
  pub remote_builders:   i64,
  pub channels_count:    i64,
}

// User DTOs

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateUser {
  pub username:  String,
  pub email:     String,
  pub full_name: Option<String>,
  pub password:  String,
  pub role:      Option<GlobalRole>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateUser {
  pub email:            Option<String>,
  pub full_name:        Option<String>,
  pub password:         Option<String>,
  pub role:             Option<GlobalRole>,
  pub enabled:          Option<bool>,
  pub public_dashboard: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoginCredentials {
  pub username: String,
  pub password: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateStarredJob {
  pub project_id: Uuid,
  pub jobset_id:  Option<Uuid>,
  pub job_name:   String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateProjectMember {
  pub user_id: Uuid,
  pub role:    ProjectRole,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateProjectMember {
  pub role: Option<ProjectRole>,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct NewsItem {
  pub id:         Uuid,
  pub title:      String,
  pub content:    String,
  pub created_by: Option<Uuid>,
  pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateNewsItem {
  pub title:      String,
  pub content:    String,
  pub created_by: Option<Uuid>,
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn domain_enums_reject_unknown_values() {
    assert!(serde_json::from_str::<ForgeType>("\"svn\"").is_err());
    assert!(
      serde_json::from_str::<NotificationType>("\"carrier_pigeon\"").is_err()
    );
    assert!(serde_json::from_str::<InputType>("\"path\"").is_err());
  }

  #[test]
  fn auth_kind_rejects_unknown_values() {
    assert!(serde_json::from_str::<AuthKind>("\"token\"").is_ok());
    assert!(serde_json::from_str::<AuthKind>("\"oidc\"").is_ok());
    assert!(serde_json::from_str::<AuthKind>("\"password\"").is_err());
  }
}
