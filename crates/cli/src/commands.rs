use std::path::PathBuf;

use pound::Parse;

/// CLI for Circus servers
#[derive(Parse)]
#[pound(name = "circusctl")]
pub(super) struct Cli {
  /// Server base URL. Defaults to `CIRCUS_URL` or <http://localhost:3000>.
  #[pound(long, global)]
  pub(super) url: Option<String>,

  /// API key. Defaults to `CIRCUS_API_KEY`.
  #[pound(long, global)]
  pub(super) api_key: Option<String>,

  /// Print raw JSON responses instead of tables.
  #[pound(long, global)]
  pub(super) json: bool,

  #[pound(subcommand)]
  pub(super) command: Command,
}

#[derive(Parse)]
pub(super) enum Command {
  /// Show public health status.
  Health,
  /// Manage projects.
  Projects {
    #[pound(subcommand)]
    command: ProjectCommand,
  },
  /// Manage jobsets.
  Jobsets {
    #[pound(subcommand)]
    command: JobsetCommand,
  },
  /// Inspect or control builds.
  Builds {
    #[pound(subcommand)]
    command: BuildCommand,
  },
  /// Inspect queue state.
  Queue,
  /// Read build logs.
  Logs {
    #[pound(subcommand)]
    command: LogCommand,
  },
  /// Search projects, jobsets, evaluations, and builds.
  Search {
    query: String,
    #[pound(long, default = "20")]
    limit: u32,
  },
  /// Manage channels.
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
    #[pound(long, default = "50")]
    limit:  u32,
    #[pound(long, default = "0")]
    offset: u32,
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
  List {
    #[pound(long, default = "50")]
    limit:  u32,
    #[pound(long, default = "0")]
    offset: u32,
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
  /// Delete a project by id.
  Delete { id: String },
}

#[derive(Parse)]
pub(super) enum JobsetCommand {
  /// List jobsets for a project.
  List {
    project_id: String,
    #[pound(long, default = "50")]
    limit:      u32,
    #[pound(long, default = "0")]
    offset:     u32,
  },
  /// Create a jobset under a project.
  Create {
    project_id:        String,
    #[pound(long)]
    name:              String,
    #[pound(long)]
    nix_expression:    String,
    #[pound(long)]
    enabled:           Option<bool>,
    #[pound(long)]
    flake_mode:        Option<bool>,
    #[pound(long)]
    check_interval:    Option<i32>,
    #[pound(long)]
    only_build_latest: Option<bool>,
    /// Git pathspecs that must match at least one changed path. Repeat it
    /// or separate pathspecs with commas.
    #[pound(long)]
    path_filter:       Vec<String>,
  },
  /// Show a jobset.
  Show {
    project_id: String,
    id:         String,
  },
  /// Delete a jobset.
  Delete {
    project_id: String,
    id:         String,
  },
  /// List jobset inputs.
  Inputs {
    project_id: String,
    jobset_id:  String,
  },
}

#[derive(Parse)]
pub(super) enum LogCommand {
  /// Print a build log.
  Show { build_id: String },
}

#[derive(Parse)]
pub(super) enum ChannelCommand {
  /// List channels.
  List,
  /// Show one channel.
  Show { id: String },
  /// Delete one channel.
  Delete { id: String },
}

#[derive(Parse)]
pub(super) enum NewsCommand {
  /// List news items.
  List,
  /// Create a news item.
  Create {
    #[pound(long)]
    title:   String,
    #[pound(long)]
    content: String,
  },
  /// Delete a news item.
  Delete { id: String },
}

#[derive(Parse)]
pub(super) enum ApiKeyCommand {
  /// List API keys.
  List,
  /// Create an API key and print the one-time secret.
  Create {
    #[pound(long)]
    name: String,
    #[pound(long)]
    role: Option<String>,
  },
  /// Revoke an API key by id.
  Revoke { id: String },
}

#[derive(Parse)]
pub(super) enum UserCommand {
  /// List users.
  List {
    #[pound(long, default = "50")]
    limit:  u32,
    #[pound(long, default = "0")]
    offset: u32,
  },
  /// Create a local user.
  Create {
    #[pound(long)]
    username:  String,
    #[pound(long)]
    email:     String,
    #[pound(long)]
    password:  String,
    #[pound(long)]
    role:      Option<String>,
    #[pound(long)]
    full_name: Option<String>,
  },
  /// Set a user's role.
  SetRole {
    id:   String,
    #[pound(long)]
    role: String,
  },
  /// Enable a user.
  Enable { id: String },
  /// Disable a user.
  Disable { id: String },
  /// Delete a user by id.
  Delete { id: String },
}

#[derive(Parse)]
pub(super) enum BuilderCommand {
  /// List builder agent sessions.
  Sessions {
    #[pound(long)]
    connected: bool,
  },
  /// Show one builder agent session by machine id.
  Session { machine_id: String },
}

#[derive(Parse)]
pub(super) enum BuildCommand {
  /// List builds.
  List {
    #[pound(long)]
    status:   Option<String>,
    #[pound(long)]
    system:   Option<String>,
    #[pound(long)]
    job_name: Option<String>,
    #[pound(long, default = "50")]
    limit:    u32,
    #[pound(long, default = "0")]
    offset:   u32,
  },
  /// Cancel a build.
  Cancel { id: String },
  /// Restart a build.
  Restart { id: String },
  /// Bump a queued build to the front.
  Bump { id: String },
  /// Set or clear the keep flag.
  Keep {
    id:    String,
    #[pound(long)]
    value: bool,
  },
}

#[derive(Parse)]
pub(super) enum EvaluationCommand {
  /// List evaluations.
  List {
    #[pound(long)]
    jobset_id: Option<String>,
    #[pound(long)]
    status:    Option<String>,
    #[pound(long, default = "50")]
    limit:     u32,
    #[pound(long, default = "0")]
    offset:    u32,
  },
  /// Trigger an evaluation for a jobset and commit.
  Trigger {
    #[pound(long)]
    jobset_id:      String,
    #[pound(long)]
    commit_hash:    String,
    #[pound(long)]
    pr_number:      Option<i32>,
    #[pound(long)]
    pr_head_branch: Option<String>,
    #[pound(long)]
    pr_base_branch: Option<String>,
    #[pound(long)]
    pr_action:      Option<String>,
  },
}

#[derive(Parse)]
pub(super) enum NotificationCommand {
  /// List recent notification tasks.
  List,
  /// Retry a failed notification task.
  Retry { id: String },
}

#[derive(Parse)]
pub(super) enum PinnedOutputCommand {
  /// List pinned build outputs.
  List {
    #[pound(long, default = "100")]
    limit:  u32,
    #[pound(long, default = "0")]
    offset: u32,
  },
  /// Clear the keep flag for a build and make its outputs GC-eligible.
  Unpin { build_id: String },
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
