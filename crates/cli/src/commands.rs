use std::path::PathBuf;

use circus_common::{BuildStatus, EvaluationStatus, roles::GlobalRole};
use pound::Parse;
use reqwest::Url;
use uuid::Uuid;

/// CLI for Circus servers
#[derive(Parse)]
#[pound(name = "circusctl", help_if_empty)]
pub(super) struct Cli {
  /// Server base URL.
  #[pound(long, global, env = "CIRCUS_URL", default = "http://localhost:3000")]
  pub(super) url: Url,

  /// API key.
  #[pound(long, global, env = "CIRCUS_API_KEY")]
  pub(super) api_key: Option<String>,

  /// Print raw JSON responses instead of tables.
  #[pound(long, global)]
  pub(super) json: bool,

  #[pound(subcommand)]
  pub(super) command: Command,
}

#[derive(Parse)]
pub(super) struct Page {
  /// Most rows to return.
  #[pound(long, default = 50, min = 1)]
  pub(super) limit:  u32,
  /// Rows to skip.
  #[pound(long, default = 0)]
  pub(super) offset: u32,
}

impl Page {
  pub(super) fn query(&self) -> Vec<(&'static str, String)> {
    vec![
      ("limit", self.limit.to_string()),
      ("offset", self.offset.to_string()),
    ]
  }
}

#[derive(Parse)]
pub(super) enum Command {
  /// Show public health status.
  Health,
  /// Manage projects.
  #[pound(alias = "project")]
  Projects {
    #[pound(subcommand)]
    command: ProjectCommand,
  },
  /// Manage jobsets.
  #[pound(alias = "jobset")]
  Jobsets {
    #[pound(subcommand)]
    command: JobsetCommand,
  },
  /// Inspect or control builds.
  #[pound(alias = "build")]
  Builds {
    #[pound(subcommand)]
    command: BuildCommand,
  },
  /// Inspect queue state.
  Queue,
  /// Print a build log.
  #[pound(alias = "logs")]
  Log { build_id: Uuid },
  /// Search projects, jobsets, evaluations, and builds.
  Search {
    query: String,
    /// Most results to return.
    #[pound(long, default = 20, min = 1)]
    limit: u32,
  },
  /// Manage channels.
  #[pound(alias = "channel")]
  Channels {
    #[pound(subcommand)]
    command: ChannelCommand,
  },
  /// Manage news items.
  News {
    #[pound(subcommand)]
    command: NewsCommand,
  },
  /// Administrative commands.
  Admin {
    #[pound(subcommand)]
    command: AdminCommand,
  },
  /// Manage database migrations.
  Migrate {
    #[pound(subcommand)]
    command: circus_common::migrate_cli::Commands,
  },
  /// Inspect or trigger evaluations.
  #[pound(alias = ["evals", "evaluation"])]
  Evaluations {
    #[pound(subcommand)]
    command: EvaluationCommand,
  },
}

#[derive(Parse)]
pub(super) enum AdminCommand {
  /// Show admin system status.
  Status,
  /// Manage API keys.
  ApiKeys {
    #[pound(subcommand)]
    command: ApiKeyCommand,
  },
  /// Manage users.
  Users {
    #[pound(subcommand)]
    command: UserCommand,
  },
  /// Manage builder agent sessions.
  Builders {
    #[pound(subcommand)]
    command: BuilderCommand,
  },
  /// Inspect or retry notification tasks.
  Notifications {
    #[pound(subcommand)]
    command: NotificationCommand,
  },
  /// Clear Circus's `PostgreSQL` failed-path cache skip records.
  FailedPathsCache {
    #[pound(subcommand)]
    command: FailedPathsCacheCommand,
  },
  /// Inspect or unpin kept build outputs.
  PinnedOutputs {
    #[pound(subcommand)]
    command: PinnedOutputCommand,
  },
  /// Show the audit log.
  Audit {
    #[pound(flatten)]
    page: Page,
  },
  /// Read or update server config.
  Config {
    #[pound(subcommand)]
    command: ConfigCommand,
  },
}

#[derive(Parse)]
pub(super) enum ProjectCommand {
  /// List projects.
  #[pound(alias = "ls")]
  List {
    #[pound(flatten)]
    page: Page,
  },
  /// Create a project.
  Create {
    #[pound(long)]
    name:           String,
    #[pound(long)]
    repository_url: String,
    #[pound(long)]
    description:    Option<String>,
  },
  /// Delete a project.
  #[pound(alias = "rm")]
  Delete { id: Uuid },
}

#[derive(Parse)]
pub(super) enum JobsetCommand {
  /// List jobsets for a project.
  #[pound(alias = "ls")]
  List {
    project_id: Uuid,
    #[pound(flatten)]
    page:       Page,
  },
  /// Create a jobset under a project.
  Create {
    project_id:        Uuid,
    #[pound(long)]
    name:              String,
    #[pound(long)]
    nix_expression:    String,
    #[pound(long)]
    enabled:           Option<bool>,
    #[pound(long)]
    flake_mode:        Option<bool>,
    /// Seconds between checks for new commits.
    #[pound(long, min = 1)]
    check_interval:    Option<i32>,
    #[pound(long)]
    only_build_latest: Option<bool>,
    /// Seconds before a build is killed, overriding the global timeout.
    #[pound(long, min = 1)]
    build_timeout:     Option<i32>,
    /// Git pathspecs that must match at least one changed path. Repeat it
    /// or separate pathspecs with commas.
    #[pound(long, delimiter = ',')]
    path_filter:       Vec<String>,
  },
  /// Show a jobset.
  Show { project_id: Uuid, id: Uuid },
  /// Delete a jobset.
  #[pound(alias = "rm")]
  Delete { project_id: Uuid, id: Uuid },
  /// List jobset inputs.
  Inputs { project_id: Uuid, jobset_id: Uuid },
}

#[derive(Parse)]
pub(super) enum ChannelCommand {
  /// List channels.
  #[pound(alias = "ls")]
  List,
  /// Show one channel.
  Show { id: Uuid },
  /// Delete one channel.
  #[pound(alias = "rm")]
  Delete { id: Uuid },
}

#[derive(Parse)]
pub(super) enum NewsCommand {
  /// List news items.
  #[pound(alias = "ls")]
  List,
  /// Create a news item.
  Create {
    #[pound(long)]
    title:   String,
    #[pound(long)]
    content: String,
  },
  /// Delete a news item.
  #[pound(alias = "rm")]
  Delete { id: Uuid },
}

#[derive(Parse)]
pub(super) enum ApiKeyCommand {
  /// List API keys.
  #[pound(alias = "ls")]
  List,
  /// Create an API key and print the one-time secret.
  Create {
    #[pound(long)]
    name: String,
    #[pound(long)]
    role: Option<GlobalRole>,
  },
  /// Revoke an API key.
  Revoke { id: Uuid },
}

#[derive(Parse)]
pub(super) enum UserCommand {
  /// List users.
  #[pound(alias = "ls")]
  List {
    #[pound(flatten)]
    page: Page,
  },
  /// Create a local user.
  Create {
    #[pound(long)]
    username:  String,
    #[pound(long)]
    email:     String,
    /// Read from the environment to keep it out of shell history.
    #[pound(long, env = "CIRCUS_USER_PASSWORD")]
    password:  String,
    #[pound(long)]
    role:      Option<GlobalRole>,
    #[pound(long)]
    full_name: Option<String>,
  },
  /// Set a user's role.
  SetRole {
    id:   Uuid,
    #[pound(long)]
    role: GlobalRole,
  },
  /// Enable a user.
  Enable { id: Uuid },
  /// Disable a user.
  Disable { id: Uuid },
  /// Delete a user.
  #[pound(alias = "rm")]
  Delete { id: Uuid },
}

#[derive(Parse)]
pub(super) enum BuilderCommand {
  /// List builder agent sessions.
  #[pound(alias = "ls")]
  Sessions {
    /// Only list connected agents.
    #[pound(long)]
    connected: bool,
  },
  /// Show one builder agent session.
  Session { machine_id: Uuid },
}

#[derive(Parse)]
pub(super) enum BuildCommand {
  /// List builds.
  #[pound(alias = "ls")]
  List {
    #[pound(long)]
    status:   Option<BuildStatus>,
    #[pound(long)]
    system:   Option<String>,
    #[pound(long)]
    job_name: Option<String>,
    #[pound(flatten)]
    page:     Page,
  },
  /// Cancel a build.
  Cancel { id: Uuid },
  /// Restart a build.
  Restart { id: Uuid },
  /// Bump a queued build to the front.
  Bump { id: Uuid },
  /// Keep a build's outputs from garbage collection.
  Keep {
    id:    Uuid,
    /// Clear the keep flag instead.
    #[pound(long)]
    clear: bool,
  },
}

#[derive(Parse)]
pub(super) enum EvaluationCommand {
  /// List evaluations.
  #[pound(alias = "ls")]
  List {
    #[pound(long)]
    jobset_id: Option<Uuid>,
    #[pound(long)]
    status:    Option<EvaluationStatus>,
    #[pound(flatten)]
    page:      Page,
  },
  /// Trigger an evaluation for a jobset and commit.
  Trigger {
    #[pound(long)]
    jobset_id:      Uuid,
    #[pound(long, validate = |hash: &String| {
      let hex = hash.bytes().all(|byte| byte.is_ascii_hexdigit());
      let len = (1..=64).contains(&hash.len());
      (hex && len).then_some(()).ok_or("expected 1 to 64 hex characters")
    })]
    commit_hash:    String,
    #[pound(long)]
    pr_number:      Option<i32>,
    #[pound(long, requires = pr_number)]
    pr_head_branch: Option<String>,
    #[pound(long, requires = pr_number)]
    pr_base_branch: Option<String>,
    #[pound(long, requires = pr_number)]
    pr_action:      Option<String>,
  },
}

#[derive(Parse)]
pub(super) enum NotificationCommand {
  /// List recent notification tasks.
  #[pound(alias = "ls")]
  List,
  /// Retry a failed notification task.
  Retry { id: Uuid },
}

#[derive(Parse)]
pub(super) enum PinnedOutputCommand {
  /// List pinned build outputs.
  #[pound(alias = "ls")]
  List {
    #[pound(flatten)]
    page: Page,
  },
  /// Clear the keep flag for a build and make its outputs GC-eligible.
  Unpin { build_id: Uuid },
}

#[derive(Parse)]
pub(super) enum FailedPathsCacheCommand {
  /// Delete all failed-path cache skip records.
  Clear,
}

#[derive(Parse)]
pub(super) enum ConfigCommand {
  /// Print the current server config body.
  Get,
  /// Replace the server config body from a local TOML file.
  Apply { file: PathBuf },
}
