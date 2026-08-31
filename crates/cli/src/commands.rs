use std::path::PathBuf;

use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(about = "CLI for Circus servers")]
pub(super) struct Cli {
  /// Server base URL. Defaults to `CIRCUS_URL` or <http://localhost:3000>.
  #[arg(long, global = true)]
  pub(super) url: Option<String>,

  /// API key. Defaults to `CIRCUS_API_KEY`.
  #[arg(long, global = true)]
  pub(super) api_key: Option<String>,

  /// Print raw JSON responses instead of tables.
  #[arg(long, global = true)]
  pub(super) json: bool,

  #[command(subcommand)]
  pub(super) command: Command,
}

#[derive(Subcommand)]
pub(super) enum Command {
  /// Show public health status.
  Health,
  /// Manage projects.
  Projects {
    #[command(subcommand)]
    command: ProjectCommand,
  },
  /// Manage jobsets.
  Jobsets {
    #[command(subcommand)]
    command: JobsetCommand,
  },
  /// Inspect or control builds.
  Builds {
    #[command(subcommand)]
    command: BuildCommand,
  },
  /// Inspect queue state.
  Queue,
  /// Read build logs.
  Logs {
    #[command(subcommand)]
    command: LogCommand,
  },
  /// Search projects, jobsets, evaluations, and builds.
  Search {
    query: String,
    #[arg(long, default_value_t = 20)]
    limit: u32,
  },
  /// Manage channels.
  Channels {
    #[command(subcommand)]
    command: ChannelCommand,
  },
  /// Manage news items.
  News {
    #[command(subcommand)]
    command: NewsCommand,
  },
  /// Administrative commands.
  Admin {
    #[command(subcommand)]
    command: AdminCommand,
  },
  /// Manage database migrations.
  Migrate {
    #[command(subcommand)]
    command: circus_common::migrate_cli::Commands,
  },
  /// Inspect or trigger evaluations.
  Evaluations {
    #[command(subcommand)]
    command: EvaluationCommand,
  },
}

#[derive(Subcommand)]
pub(super) enum AdminCommand {
  /// Show admin system status.
  Status,
  /// Manage API keys.
  ApiKeys {
    #[command(subcommand)]
    command: ApiKeyCommand,
  },
  /// Manage users.
  Users {
    #[command(subcommand)]
    command: UserCommand,
  },
  /// Manage remote builders and agent sessions.
  Builders {
    #[command(subcommand)]
    command: BuilderCommand,
  },
  /// Inspect or retry notification tasks.
  Notifications {
    #[command(subcommand)]
    command: NotificationCommand,
  },
  /// Inspect or unpin kept build outputs.
  PinnedOutputs {
    #[command(subcommand)]
    command: PinnedOutputCommand,
  },
  /// Show the audit log.
  Audit {
    #[arg(long, default_value_t = 50)]
    limit:  u32,
    #[arg(long, default_value_t = 0)]
    offset: u32,
  },
  /// Read or update server config.
  Config {
    #[command(subcommand)]
    command: ConfigCommand,
  },
}

#[derive(Subcommand)]
pub(super) enum ProjectCommand {
  /// List projects.
  List {
    #[arg(long, default_value_t = 50)]
    limit:  u32,
    #[arg(long, default_value_t = 0)]
    offset: u32,
  },
  /// Create a project.
  Create {
    #[arg(long)]
    name:           String,
    #[arg(long)]
    repository_url: String,
    #[arg(long)]
    description:    Option<String>,
  },
  /// Delete a project by id.
  Delete { id: String },
}

#[derive(Subcommand)]
pub(super) enum JobsetCommand {
  /// List jobsets for a project.
  List {
    project_id: String,
    #[arg(long, default_value_t = 50)]
    limit:      u32,
    #[arg(long, default_value_t = 0)]
    offset:     u32,
  },
  /// Create a jobset under a project.
  Create {
    project_id:        String,
    #[arg(long)]
    name:              String,
    #[arg(long)]
    nix_expression:    String,
    #[arg(long)]
    enabled:           Option<bool>,
    #[arg(long)]
    flake_mode:        Option<bool>,
    #[arg(long)]
    check_interval:    Option<i32>,
    #[arg(long)]
    only_build_latest: Option<bool>,
    /// Git pathspecs that must match at least one changed path.
    #[arg(long, value_delimiter = ',', num_args = 1..)]
    path_filter:       Option<Vec<String>>,
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

#[derive(Subcommand)]
pub(super) enum LogCommand {
  /// Print a build log.
  Show { build_id: String },
}

#[derive(Subcommand)]
pub(super) enum ChannelCommand {
  /// List channels.
  List,
  /// Show one channel.
  Show { id: String },
  /// Delete one channel.
  Delete { id: String },
}

#[derive(Subcommand)]
pub(super) enum NewsCommand {
  /// List news items.
  List,
  /// Create a news item.
  Create {
    #[arg(long)]
    title:   String,
    #[arg(long)]
    content: String,
  },
  /// Delete a news item.
  Delete { id: String },
}

#[derive(Subcommand)]
pub(super) enum ApiKeyCommand {
  /// List API keys.
  List,
  /// Create an API key and print the one-time secret.
  Create {
    #[arg(long)]
    name: String,
    #[arg(long)]
    role: Option<String>,
  },
  /// Revoke an API key by id.
  Revoke { id: String },
}

#[derive(Subcommand)]
pub(super) enum UserCommand {
  /// List users.
  List {
    #[arg(long, default_value_t = 50)]
    limit:  u32,
    #[arg(long, default_value_t = 0)]
    offset: u32,
  },
  /// Create a local user.
  Create {
    #[arg(long)]
    username:  String,
    #[arg(long)]
    email:     String,
    #[arg(long)]
    password:  String,
    #[arg(long)]
    role:      Option<String>,
    #[arg(long)]
    full_name: Option<String>,
  },
  /// Set a user's role.
  SetRole {
    id:   String,
    #[arg(long)]
    role: String,
  },
  /// Enable a user.
  Enable { id: String },
  /// Disable a user.
  Disable { id: String },
  /// Delete a user by id.
  Delete { id: String },
}

#[derive(Subcommand)]
pub(super) enum BuilderCommand {
  /// List builder agent sessions.
  Sessions {
    #[arg(long)]
    connected: bool,
  },
  /// Show one builder agent session by machine id.
  Session { machine_id: String },
}

#[derive(Subcommand)]
pub(super) enum BuildCommand {
  /// List builds.
  List {
    #[arg(long)]
    status:   Option<String>,
    #[arg(long)]
    system:   Option<String>,
    #[arg(long)]
    job_name: Option<String>,
    #[arg(long, default_value_t = 50)]
    limit:    u32,
    #[arg(long, default_value_t = 0)]
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
    #[arg(long)]
    value: bool,
  },
}

#[derive(Subcommand)]
pub(super) enum EvaluationCommand {
  /// List evaluations.
  List {
    #[arg(long)]
    jobset_id: Option<String>,
    #[arg(long)]
    status:    Option<String>,
    #[arg(long, default_value_t = 50)]
    limit:     u32,
    #[arg(long, default_value_t = 0)]
    offset:    u32,
  },
  /// Trigger an evaluation for a jobset and commit.
  Trigger {
    #[arg(long)]
    jobset_id:      String,
    #[arg(long)]
    commit_hash:    String,
    #[arg(long)]
    pr_number:      Option<i32>,
    #[arg(long)]
    pr_head_branch: Option<String>,
    #[arg(long)]
    pr_base_branch: Option<String>,
    #[arg(long)]
    pr_action:      Option<String>,
  },
}

#[derive(Subcommand)]
pub(super) enum NotificationCommand {
  /// List recent notification tasks.
  List,
  /// Retry a failed notification task.
  Retry { id: String },
}

#[derive(Subcommand)]
pub(super) enum PinnedOutputCommand {
  /// List pinned build outputs.
  List {
    #[arg(long, default_value_t = 100)]
    limit:  u32,
    #[arg(long, default_value_t = 0)]
    offset: u32,
  },
  /// Clear the keep flag for a build and make its outputs GC-eligible.
  Unpin { build_id: String },
}

#[derive(Subcommand)]
pub(super) enum ConfigCommand {
  /// Print the current server config body.
  Get,
  /// Replace the server config body from a local TOML file.
  Apply { file: PathBuf },
}
