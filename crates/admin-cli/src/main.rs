#![expect(
  clippy::print_stdout,
  reason = "circus-admin is a CLI and stdout is its user interface"
)]

mod client;
mod commands;
mod output;

use std::path::PathBuf;

use clap::{Parser, Subcommand};
use color_eyre::eyre::{Result, eyre};

use crate::{
  client::ApiClient,
  commands::{
    api_keys,
    audit,
    builders,
    builds,
    config,
    evaluations,
    health,
    notifications,
    pinned_outputs,
    projects,
    status,
    users,
  },
};
#[derive(Parser)]
#[command(name = "circus-admin", about = "Admin CLI for Circus servers")]
struct Cli {
  /// Server base URL. Defaults to `CIRCUS_URL` or <http://localhost:3000>.
  #[arg(long, global = true)]
  url: Option<String>,

  /// API key. Defaults to `CIRCUS_API_KEY`.
  #[arg(long, global = true)]
  api_key: Option<String>,

  /// Print raw JSON responses instead of tables.
  #[arg(long, global = true)]
  json: bool,

  #[command(subcommand)]
  command: Command,
}

#[derive(Subcommand)]
enum Command {
  /// Show public health status.
  Health,
  /// Show admin system status.
  Status,
  /// Manage projects.
  Projects {
    #[command(subcommand)]
    command: ProjectCommand,
  },
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
  /// Manage remote builders.
  Builders {
    #[command(subcommand)]
    command: BuilderCommand,
  },
  /// Inspect or control builds.
  Builds {
    #[command(subcommand)]
    command: BuildCommand,
  },
  /// Inspect or trigger evaluations.
  Evaluations {
    #[command(subcommand)]
    command: EvaluationCommand,
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
  /// Read or update the declarative config file through the API.
  Config {
    #[command(subcommand)]
    command: ConfigCommand,
  },
}

#[derive(Subcommand)]
enum ProjectCommand {
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
enum ApiKeyCommand {
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
enum UserCommand {
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
enum BuilderCommand {
  /// List remote builders.
  List,
  /// Register a remote builder.
  Add {
    #[arg(long)]
    name:               String,
    #[arg(long)]
    ssh_uri:            String,
    #[arg(long, value_delimiter = ',', num_args = 1..)]
    systems:            Vec<String>,
    #[arg(long)]
    max_jobs:           Option<i32>,
    #[arg(long)]
    speed_factor:       Option<i32>,
    #[arg(long, value_delimiter = ',')]
    supported_features: Vec<String>,
    #[arg(long, value_delimiter = ',')]
    mandatory_features: Vec<String>,
    #[arg(long)]
    public_host_key:    Option<String>,
    #[arg(long)]
    ssh_key_file:       Option<String>,
  },
  /// Enable a remote builder.
  Enable { id: String },
  /// Disable a remote builder.
  Disable { id: String },
  /// Remove a remote builder.
  Remove { id: String },
  /// List builder agent sessions.
  Sessions {
    #[arg(long)]
    connected: bool,
  },
  /// Show one builder agent session by machine id.
  Session { machine_id: String },
}

#[derive(Subcommand)]
enum BuildCommand {
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
enum EvaluationCommand {
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
enum NotificationCommand {
  /// List recent notification tasks.
  List,
  /// Retry a failed notification task.
  Retry { id: String },
}

#[derive(Subcommand)]
enum PinnedOutputCommand {
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
enum ConfigCommand {
  /// Print the current server config body.
  Get,
  /// Replace the server config body from a local TOML file.
  Apply { file: PathBuf },
}

#[tokio::main]
async fn main() -> Result<()> {
  color_eyre::install()?;

  // Use ring for tls.
  rustls::crypto::ring::default_provider()
    .install_default()
    .map_err(|_| eyre!("a rustls CryptoProvider is already installed"))?;

  let cli = Cli::parse();
  let base_url = cli
    .url
    .or_else(|| std::env::var("CIRCUS_URL").ok())
    .unwrap_or_else(|| "http://localhost:3000".to_string());
  let api_key = cli.api_key.or_else(|| std::env::var("CIRCUS_API_KEY").ok());
  let api = ApiClient::new(&base_url, api_key)?;

  match cli.command {
    Command::Health => health(&api, cli.json).await,
    Command::Status => status(&api, cli.json).await,
    Command::Projects { command } => projects(&api, cli.json, command).await,
    Command::ApiKeys { command } => api_keys(&api, cli.json, command).await,
    Command::Users { command } => users(&api, cli.json, command).await,
    Command::Builders { command } => builders(&api, cli.json, command).await,
    Command::Builds { command } => builds(&api, cli.json, command).await,
    Command::Evaluations { command } => {
      evaluations(&api, cli.json, command).await
    },
    Command::Notifications { command } => {
      notifications(&api, cli.json, command).await
    },
    Command::PinnedOutputs { command } => {
      pinned_outputs(&api, cli.json, command).await
    },
    Command::Audit { limit, offset } => {
      audit(&api, cli.json, limit, offset).await
    },
    Command::Config { command } => config(&api, cli.json, command).await,
  }
}
