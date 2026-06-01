#![expect(
  clippy::print_stdout,
  reason = "circus-admin is a CLI and stdout is its user interface"
)]

use std::path::PathBuf;

use clap::{Parser, Subcommand};
use color_eyre::{
  Result,
  eyre::{Context, bail, eyre},
};
use comfy_table::Table;
use reqwest::Method;
use serde_json::{Map, Value, json};

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

struct ApiClient {
  client:   reqwest::Client,
  base_url: reqwest::Url,
  api_key:  Option<String>,
}

impl ApiClient {
  fn new(base_url: &str, api_key: Option<String>) -> Result<Self> {
    let mut normalized = base_url.trim().to_string();
    if !normalized.ends_with('/') {
      normalized.push('/');
    }
    let base_url = reqwest::Url::parse(&normalized)
      .with_context(|| format!("invalid Circus URL: {base_url}"))?;
    Ok(Self {
      client: reqwest::Client::new(),
      base_url,
      api_key,
    })
  }

  async fn get(&self, path: &str, auth_required: bool) -> Result<Value> {
    self.send(Method::GET, path, None, auth_required).await
  }

  async fn post(
    &self,
    path: &str,
    body: Value,
    auth_required: bool,
  ) -> Result<Value> {
    self
      .send(Method::POST, path, Some(body), auth_required)
      .await
  }

  async fn put(
    &self,
    path: &str,
    body: Value,
    auth_required: bool,
  ) -> Result<Value> {
    self
      .send(Method::PUT, path, Some(body), auth_required)
      .await
  }

  async fn delete(&self, path: &str, auth_required: bool) -> Result<Value> {
    self.send(Method::DELETE, path, None, auth_required).await
  }

  async fn send(
    &self,
    method: Method,
    path: &str,
    body: Option<Value>,
    auth_required: bool,
  ) -> Result<Value> {
    if auth_required && self.api_key.is_none() {
      bail!(
        "this command requires an API key; pass --api-key or set \
         CIRCUS_API_KEY"
      );
    }

    let url = self.endpoint(path)?;
    let mut request = self.client.request(method.clone(), url.clone());
    if let Some(api_key) = &self.api_key {
      request = request.bearer_auth(api_key);
    }
    if let Some(body) = body {
      request = request.json(&body);
    }

    let response = request
      .send()
      .await
      .with_context(|| format!("request failed: {method} {url}"))?;
    let status = response.status();
    let text = response
      .text()
      .await
      .with_context(|| format!("reading response body for {method} {url}"))?;

    if !status.is_success() {
      bail!(
        "{} {} failed with {}: {}",
        method,
        url,
        status,
        response_error(&text)
      );
    }

    if text.trim().is_empty() {
      return Ok(json!({}));
    }
    serde_json::from_str(&text).map_or_else(|_| Ok(json!({ "body": text })), Ok)
  }

  fn endpoint(&self, path: &str) -> Result<reqwest::Url> {
    self
      .base_url
      .join(path.trim_start_matches('/'))
      .with_context(|| format!("invalid endpoint path: {path}"))
  }
}

#[tokio::main]
async fn main() -> Result<()> {
  color_eyre::install()?;
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

async fn health(api: &ApiClient, json_output: bool) -> Result<()> {
  let response = api.get("health", false).await?;
  if json_output {
    return print_json(&response);
  }

  print_table(&["Status", "Database"], &[vec![
    field(&response, "status"),
    field(&response, "database"),
  ]]);
  let services = response
    .get("services")
    .and_then(Value::as_array)
    .map_or_else(Vec::new, |services| {
      services
        .iter()
        .map(|service| {
          vec![
            field(service, "service"),
            field(service, "healthy"),
            field(service, "seconds_since"),
            field(service, "detail"),
          ]
        })
        .collect()
    });
  if !services.is_empty() {
    println!();
    print_table(
      &["Service", "Healthy", "Seconds Since", "Detail"],
      &services,
    );
  }
  Ok(())
}

async fn status(api: &ApiClient, json_output: bool) -> Result<()> {
  let response = api.get("api/v1/admin/system", true).await?;
  if json_output {
    return print_json(&response);
  }

  print_table(
    &[
      "Projects",
      "Jobsets",
      "Evaluations",
      "Pending",
      "Running",
      "Completed",
      "Failed",
      "Builders",
      "Channels",
    ],
    &[vec![
      field(&response, "projects_count"),
      field(&response, "jobsets_count"),
      field(&response, "evaluations_count"),
      field(&response, "builds_pending"),
      field(&response, "builds_running"),
      field(&response, "builds_completed"),
      field(&response, "builds_failed"),
      field(&response, "remote_builders"),
      field(&response, "channels_count"),
    ]],
  );
  Ok(())
}

async fn projects(
  api: &ApiClient,
  json_output: bool,
  command: ProjectCommand,
) -> Result<()> {
  match command {
    ProjectCommand::List { limit, offset } => {
      let response = api
        .get(
          &with_query("api/v1/projects", &[
            ("limit", limit.to_string()),
            ("offset", offset.to_string()),
          ]),
          false,
        )
        .await?;
      if json_output {
        return print_json(&response);
      }
      print_table(
        &["ID", "Name", "Repository", "Description"],
        &items(&response)
          .iter()
          .map(|project| {
            vec![
              field(project, "id"),
              field(project, "name"),
              field(project, "repository_url"),
              field(project, "description"),
            ]
          })
          .collect::<Vec<_>>(),
      );
      print_page(&response);
    },
    ProjectCommand::Create {
      name,
      repository_url,
      description,
    } => {
      let mut body = Map::new();
      body.insert("name".to_string(), Value::String(name));
      body.insert("repository_url".to_string(), Value::String(repository_url));
      insert_optional(&mut body, "description", description);
      let response = api
        .post("api/v1/projects", Value::Object(body), true)
        .await?;
      if json_output {
        return print_json(&response);
      }
      println!("Created project {}", field(&response, "name"));
      print_table(&["ID", "Name", "Repository"], &[vec![
        field(&response, "id"),
        field(&response, "name"),
        field(&response, "repository_url"),
      ]]);
    },
    ProjectCommand::Delete { id } => {
      let response = api.delete(&format!("api/v1/projects/{id}"), true).await?;
      if json_output {
        return print_json(&response);
      }
      println!("Deleted project {id}");
    },
  }
  Ok(())
}

async fn api_keys(
  api: &ApiClient,
  json_output: bool,
  command: ApiKeyCommand,
) -> Result<()> {
  match command {
    ApiKeyCommand::List => {
      let response = api.get("api/v1/api-keys", true).await?;
      if json_output {
        return print_json(&response);
      }
      print_table(
        &["ID", "Name", "Role", "Created", "Last Used"],
        &items(&response)
          .iter()
          .map(|key| {
            vec![
              field(key, "id"),
              field(key, "name"),
              field(key, "role"),
              field(key, "created_at"),
              field(key, "last_used_at"),
            ]
          })
          .collect::<Vec<_>>(),
      );
    },
    ApiKeyCommand::Create { name, role } => {
      let mut body = Map::new();
      body.insert("name".to_string(), Value::String(name));
      insert_optional(&mut body, "role", role);
      let response = api
        .post("api/v1/api-keys", Value::Object(body), true)
        .await?;
      if json_output {
        return print_json(&response);
      }
      println!("Created API key. Save the secret now; it cannot be recovered.");
      print_table(&["ID", "Name", "Role", "Key"], &[vec![
        field(&response, "id"),
        field(&response, "name"),
        field(&response, "role"),
        field(&response, "key"),
      ]]);
    },
    ApiKeyCommand::Revoke { id } => {
      let response = api.delete(&format!("api/v1/api-keys/{id}"), true).await?;
      if json_output {
        return print_json(&response);
      }
      println!("Revoked API key {id}");
    },
  }
  Ok(())
}

async fn users(
  api: &ApiClient,
  json_output: bool,
  command: UserCommand,
) -> Result<()> {
  match command {
    UserCommand::List { limit, offset } => {
      let response = api
        .get(
          &with_query("api/v1/users", &[
            ("limit", limit.to_string()),
            ("offset", offset.to_string()),
          ]),
          true,
        )
        .await?;
      if json_output {
        return print_json(&response);
      }
      print_users(&response);
    },
    UserCommand::Create {
      username,
      email,
      password,
      role,
      full_name,
    } => {
      let mut body = Map::new();
      body.insert("username".to_string(), Value::String(username));
      body.insert("email".to_string(), Value::String(email));
      body.insert("password".to_string(), Value::String(password));
      insert_optional(&mut body, "role", role);
      insert_optional(&mut body, "full_name", full_name);
      let response =
        api.post("api/v1/users", Value::Object(body), true).await?;
      if json_output {
        return print_json(&response);
      }
      println!("Created user {}", field(&response, "username"));
      print_users(&response);
    },
    UserCommand::SetRole { id, role } => {
      let response = api
        .put(&format!("api/v1/users/{id}"), json!({ "role": role }), true)
        .await?;
      if json_output {
        return print_json(&response);
      }
      println!("Updated user role for {id}");
      print_users(&response);
    },
    UserCommand::Enable { id } => {
      update_user_enabled(api, json_output, &id, true).await?;
    },
    UserCommand::Disable { id } => {
      update_user_enabled(api, json_output, &id, false).await?;
    },
    UserCommand::Delete { id } => {
      let response = api.delete(&format!("api/v1/users/{id}"), true).await?;
      if json_output {
        return print_json(&response);
      }
      println!("Deleted user {id}");
    },
  }
  Ok(())
}

async fn update_user_enabled(
  api: &ApiClient,
  json_output: bool,
  id: &str,
  enabled: bool,
) -> Result<()> {
  let response = api
    .put(
      &format!("api/v1/users/{id}"),
      json!({ "enabled": enabled }),
      true,
    )
    .await?;
  if json_output {
    return print_json(&response);
  }
  println!("{} user {id}", if enabled { "Enabled" } else { "Disabled" });
  print_users(&response);
  Ok(())
}

async fn builders(
  api: &ApiClient,
  json_output: bool,
  command: BuilderCommand,
) -> Result<()> {
  match command {
    BuilderCommand::List => {
      let response = api.get("api/v1/admin/builders", false).await?;
      if json_output {
        return print_json(&response);
      }
      print_builders(&response);
    },
    BuilderCommand::Add {
      name,
      ssh_uri,
      systems,
      max_jobs,
      speed_factor,
      supported_features,
      mandatory_features,
      public_host_key,
      ssh_key_file,
    } => {
      if systems.is_empty() {
        bail!("at least one --systems value is required");
      }
      let mut body = Map::new();
      body.insert("name".to_string(), Value::String(name));
      body.insert("ssh_uri".to_string(), Value::String(ssh_uri));
      body.insert("systems".to_string(), json!(systems));
      insert_optional_i32(&mut body, "max_jobs", max_jobs);
      insert_optional_i32(&mut body, "speed_factor", speed_factor);
      insert_optional_vec(&mut body, "supported_features", &supported_features);
      insert_optional_vec(&mut body, "mandatory_features", &mandatory_features);
      insert_optional(&mut body, "public_host_key", public_host_key);
      insert_optional(&mut body, "ssh_key_file", ssh_key_file);
      let response = api
        .post("api/v1/admin/builders", Value::Object(body), true)
        .await?;
      if json_output {
        return print_json(&response);
      }
      println!("Registered builder {}", field(&response, "name"));
      print_builders(&response);
    },
    BuilderCommand::Enable { id } => {
      update_builder_enabled(api, json_output, &id, true).await?;
    },
    BuilderCommand::Disable { id } => {
      update_builder_enabled(api, json_output, &id, false).await?;
    },
    BuilderCommand::Remove { id } => {
      let response = api
        .delete(&format!("api/v1/admin/builders/{id}"), true)
        .await?;
      if json_output {
        return print_json(&response);
      }
      println!("Removed builder {id}");
    },
    BuilderCommand::Sessions { connected } => {
      let path = if connected {
        "api/v1/admin/builders/sessions/connected"
      } else {
        "api/v1/admin/builders/sessions"
      };
      let response = api.get(path, true).await?;
      if json_output {
        return print_json(&response);
      }
      print_builder_sessions(&response);
    },
    BuilderCommand::Session { machine_id } => {
      let response = api
        .get(
          &format!("api/v1/admin/builders/sessions/{machine_id}"),
          true,
        )
        .await?;
      if json_output {
        return print_json(&response);
      }
      print_builder_sessions(&response);
    },
  }
  Ok(())
}

async fn update_builder_enabled(
  api: &ApiClient,
  json_output: bool,
  id: &str,
  enabled: bool,
) -> Result<()> {
  let response = api
    .put(
      &format!("api/v1/admin/builders/{id}"),
      json!({ "enabled": enabled }),
      true,
    )
    .await?;
  if json_output {
    return print_json(&response);
  }
  println!(
    "{} builder {id}",
    if enabled { "Enabled" } else { "Disabled" }
  );
  print_builders(&response);
  Ok(())
}

async fn builds(
  api: &ApiClient,
  json_output: bool,
  command: BuildCommand,
) -> Result<()> {
  match command {
    BuildCommand::List {
      status,
      system,
      job_name,
      limit,
      offset,
    } => {
      let mut params =
        vec![("limit", limit.to_string()), ("offset", offset.to_string())];
      push_optional(&mut params, "status", status);
      push_optional(&mut params, "system", system);
      push_optional(&mut params, "job_name", job_name);
      let response = api
        .get(&with_query("api/v1/builds", &params), false)
        .await?;
      if json_output {
        return print_json(&response);
      }
      print_builds(&response);
      print_page(&response);
    },
    BuildCommand::Cancel { id } => {
      build_action(api, json_output, &id, "cancel", "Cancelled").await?;
    },
    BuildCommand::Restart { id } => {
      build_action(api, json_output, &id, "restart", "Restarted").await?;
    },
    BuildCommand::Bump { id } => {
      build_action(api, json_output, &id, "bump", "Bumped").await?;
    },
    BuildCommand::Keep { id, value } => {
      let response = api
        .put(&format!("api/v1/builds/{id}/keep/{value}"), json!({}), true)
        .await?;
      if json_output {
        return print_json(&response);
      }
      println!("Set keep={value} for build {id}");
      print_builds(&response);
    },
  }
  Ok(())
}

async fn build_action(
  api: &ApiClient,
  json_output: bool,
  id: &str,
  action: &str,
  label: &str,
) -> Result<()> {
  let response = api
    .post(&format!("api/v1/builds/{id}/{action}"), json!({}), true)
    .await?;
  if json_output {
    return print_json(&response);
  }
  println!("{label} build {id}");
  print_builds(&response);
  Ok(())
}

async fn evaluations(
  api: &ApiClient,
  json_output: bool,
  command: EvaluationCommand,
) -> Result<()> {
  match command {
    EvaluationCommand::List {
      jobset_id,
      status,
      limit,
      offset,
    } => {
      let mut params =
        vec![("limit", limit.to_string()), ("offset", offset.to_string())];
      push_optional(&mut params, "jobset_id", jobset_id);
      push_optional(&mut params, "status", status);
      let response = api
        .get(&with_query("api/v1/evaluations", &params), false)
        .await?;
      if json_output {
        return print_json(&response);
      }
      print_table(
        &["ID", "Jobset", "Commit", "Status", "Time"],
        &items(&response)
          .iter()
          .map(|evaluation| {
            vec![
              field(evaluation, "id"),
              field(evaluation, "jobset_id"),
              short(&field(evaluation, "commit_hash"), 12),
              field(evaluation, "status"),
              field(evaluation, "evaluation_time"),
            ]
          })
          .collect::<Vec<_>>(),
      );
      print_page(&response);
    },
    EvaluationCommand::Trigger {
      jobset_id,
      commit_hash,
      pr_number,
      pr_head_branch,
      pr_base_branch,
      pr_action,
    } => {
      let mut body = Map::new();
      body.insert("jobset_id".to_string(), Value::String(jobset_id));
      body.insert("commit_hash".to_string(), Value::String(commit_hash));
      insert_optional_i32(&mut body, "pr_number", pr_number);
      insert_optional(&mut body, "pr_head_branch", pr_head_branch);
      insert_optional(&mut body, "pr_base_branch", pr_base_branch);
      insert_optional(&mut body, "pr_action", pr_action);
      let response = api
        .post("api/v1/evaluations/trigger", Value::Object(body), true)
        .await?;
      if json_output {
        return print_json(&response);
      }
      println!("Triggered evaluation {}", field(&response, "id"));
      print_table(&["ID", "Jobset", "Commit", "Status"], &[vec![
        field(&response, "id"),
        field(&response, "jobset_id"),
        short(&field(&response, "commit_hash"), 12),
        field(&response, "status"),
      ]]);
    },
  }
  Ok(())
}

async fn notifications(
  api: &ApiClient,
  json_output: bool,
  command: NotificationCommand,
) -> Result<()> {
  match command {
    NotificationCommand::List => {
      let response = api.get("api/v1/admin/notification-tasks", true).await?;
      if json_output {
        return print_json(&response);
      }
      print_table(
        &["ID", "Type", "Status", "Attempts", "Next Retry", "Error"],
        &items(&response)
          .iter()
          .map(|task| {
            vec![
              field(task, "id"),
              field(task, "notification_type"),
              field(task, "status"),
              format!(
                "{}/{}",
                field(task, "attempts"),
                field(task, "max_attempts")
              ),
              field(task, "next_retry_at"),
              short(&field(task, "last_error"), 80),
            ]
          })
          .collect::<Vec<_>>(),
      );
    },
    NotificationCommand::Retry { id } => {
      let response = api
        .post(
          &format!("api/v1/admin/notification-tasks/{id}/retry"),
          json!({}),
          true,
        )
        .await?;
      if json_output {
        return print_json(&response);
      }
      println!("Retry scheduled for notification task {id}");
      print_table(&["ID", "Type", "Status", "Attempts", "Next Retry"], &[
        vec![
          field(&response, "id"),
          field(&response, "notification_type"),
          field(&response, "status"),
          format!(
            "{}/{}",
            field(&response, "attempts"),
            field(&response, "max_attempts")
          ),
          field(&response, "next_retry_at"),
        ],
      ]);
    },
  }
  Ok(())
}

async fn pinned_outputs(
  api: &ApiClient,
  json_output: bool,
  command: PinnedOutputCommand,
) -> Result<()> {
  match command {
    PinnedOutputCommand::List { limit, offset } => {
      let response = api
        .get(
          &with_query("api/v1/admin/pinned-build-products", &[
            ("limit", limit.to_string()),
            ("offset", offset.to_string()),
          ]),
          true,
        )
        .await?;
      if json_output {
        return print_json(&response);
      }
      print_pinned_outputs(&response);
      print_page(&response);
    },
    PinnedOutputCommand::Unpin { build_id } => {
      let response = api
        .post(
          &format!("api/v1/admin/pinned-builds/{build_id}/unpin"),
          json!({}),
          true,
        )
        .await?;
      if json_output {
        return print_json(&response);
      }
      println!("Unpinned build {build_id}");
      print_builds(&response);
    },
  }
  Ok(())
}

async fn audit(
  api: &ApiClient,
  json_output: bool,
  limit: u32,
  offset: u32,
) -> Result<()> {
  let response = api
    .get(
      &with_query("api/v1/admin/audit-log", &[
        ("limit", limit.to_string()),
        ("offset", offset.to_string()),
      ]),
      true,
    )
    .await?;
  if json_output {
    return print_json(&response);
  }
  print_table(
    &["Time", "Actor", "Action", "Target", "Remote"],
    &items(&response)
      .iter()
      .map(|entry| {
        let actor = format!(
          "{}:{}",
          field(entry, "actor_kind"),
          field(entry, "actor_name")
        );
        let target = format!(
          "{}:{}",
          field(entry, "target_kind"),
          field(entry, "target_id")
        );
        vec![
          field(entry, "occurred_at"),
          actor,
          field(entry, "action"),
          target,
          field(entry, "remote_addr"),
        ]
      })
      .collect::<Vec<_>>(),
  );
  print_page(&response);
  Ok(())
}

async fn config(
  api: &ApiClient,
  json_output: bool,
  command: ConfigCommand,
) -> Result<()> {
  match command {
    ConfigCommand::Get => {
      let response = api.get("api/v1/admin/config", true).await?;
      if json_output {
        return print_json(&response);
      }
      let contents = response
        .get("contents")
        .and_then(Value::as_str)
        .ok_or_else(|| eyre!("config response did not include contents"))?;
      println!("{contents}");
    },
    ConfigCommand::Apply { file } => {
      let contents = std::fs::read_to_string(&file)
        .with_context(|| format!("reading {}", file.display()))?;
      let response = api
        .put("api/v1/admin/config", json!({ "contents": contents }), true)
        .await?;
      if json_output {
        return print_json(&response);
      }
      println!(
        "Updated {} (restart required: {})",
        field(&response, "path"),
        field(&response, "requires_restart")
      );
    },
  }
  Ok(())
}

fn print_users(value: &Value) {
  print_table(
    &["ID", "Username", "Email", "Role", "Enabled", "Type"],
    &items(value)
      .iter()
      .map(|user| {
        vec![
          field(user, "id"),
          field(user, "username"),
          field(user, "email"),
          field(user, "role"),
          field(user, "enabled"),
          field(user, "user_type"),
        ]
      })
      .collect::<Vec<_>>(),
  );
}

fn print_builders(value: &Value) {
  print_table(
    &[
      "ID", "Name", "SSH URI", "Systems", "Jobs", "Speed", "Enabled",
    ],
    &items(value)
      .iter()
      .map(|builder| {
        vec![
          field(builder, "id"),
          field(builder, "name"),
          field(builder, "ssh_uri"),
          field(builder, "systems"),
          field(builder, "max_jobs"),
          field(builder, "speed_factor"),
          field(builder, "enabled"),
        ]
      })
      .collect::<Vec<_>>(),
  );
}

fn print_builder_sessions(value: &Value) {
  print_table(
    &[
      "Machine",
      "Name",
      "Host",
      "Systems",
      "Jobs",
      "Load",
      "Connected",
      "Last Seen",
    ],
    &items(value)
      .iter()
      .map(|session| {
        vec![
          field(session, "machine_id"),
          field(session, "name"),
          field(session, "hostname"),
          field(session, "systems"),
          format!(
            "{}/{}",
            field(session, "current_jobs"),
            field(session, "max_jobs")
          ),
          field(session, "load1"),
          field(session, "connected"),
          field(session, "last_seen"),
        ]
      })
      .collect::<Vec<_>>(),
  );
}

fn print_pinned_outputs(value: &Value) {
  print_table(
    &["Build", "Job", "System", "Status", "Product", "Root"],
    &items(value)
      .iter()
      .map(|product| {
        vec![
          field(product, "build_id"),
          field(product, "job_name"),
          field(product, "system"),
          field(product, "status"),
          field(product, "product_name"),
          short(&field(product, "gc_root_path"), 80),
        ]
      })
      .collect::<Vec<_>>(),
  );
}

fn print_builds(value: &Value) {
  print_table(
    &[
      "ID", "Job", "Status", "System", "Priority", "Keep", "Created",
    ],
    &items(value)
      .iter()
      .map(|build| {
        vec![
          field(build, "id"),
          field(build, "job_name"),
          field(build, "status"),
          field(build, "system"),
          field(build, "priority"),
          field(build, "keep"),
          field(build, "created_at"),
        ]
      })
      .collect::<Vec<_>>(),
  );
}

fn print_json(value: &Value) -> Result<()> {
  println!("{}", serde_json::to_string_pretty(value)?);
  Ok(())
}

fn print_page(value: &Value) {
  if value.get("total").is_some() {
    println!(
      "total={} limit={} offset={}",
      field(value, "total"),
      field(value, "limit"),
      field(value, "offset")
    );
  }
}

fn print_table(headers: &[&str], rows: &[Vec<String>]) {
  if rows.is_empty() {
    println!("No rows.");
    return;
  }

  let mut table = Table::new();
  table.set_header(headers.to_vec());
  for row in rows {
    table.add_row(row.clone());
  }
  println!("{table}");
}

fn items(value: &Value) -> Vec<&Value> {
  if let Some(items) = value.get("items").and_then(Value::as_array) {
    return items.iter().collect();
  }
  if let Some(items) = value.as_array() {
    return items.iter().collect();
  }
  vec![value]
}

fn field(value: &Value, key: &str) -> String {
  value
    .get(key)
    .map_or_else(|| "-".to_string(), value_to_string)
}

fn value_to_string(value: &Value) -> String {
  match value {
    Value::Null => "-".to_string(),
    Value::Bool(v) => v.to_string(),
    Value::Number(v) => v.to_string(),
    Value::String(v) if v.is_empty() => "-".to_string(),
    Value::String(v) => v.clone(),
    Value::Array(values) => {
      values
        .iter()
        .map(value_to_string)
        .collect::<Vec<_>>()
        .join(",")
    },
    Value::Object(_) => {
      serde_json::to_string(value).unwrap_or_else(|_| "-".to_string())
    },
  }
}

fn short(value: &str, max: usize) -> String {
  if value.chars().count() <= max {
    return value.to_string();
  }
  let mut out = value
    .chars()
    .take(max.saturating_sub(3))
    .collect::<String>();
  out.push_str("...");
  out
}

fn with_query(path: &str, params: &[(&str, String)]) -> String {
  let query = params
    .iter()
    .filter(|(_, value)| !value.is_empty())
    .map(|(key, value)| {
      format!("{}={}", key, urlencoding::encode(value).into_owned())
    })
    .collect::<Vec<_>>()
    .join("&");
  if query.is_empty() {
    path.to_string()
  } else {
    format!("{path}?{query}")
  }
}

fn push_optional(
  params: &mut Vec<(&'static str, String)>,
  key: &'static str,
  value: Option<String>,
) {
  if let Some(value) = value {
    params.push((key, value));
  }
}

fn insert_optional(
  map: &mut Map<String, Value>,
  key: &str,
  value: Option<String>,
) {
  if let Some(value) = value {
    map.insert(key.to_string(), Value::String(value));
  }
}

fn insert_optional_i32(
  map: &mut Map<String, Value>,
  key: &str,
  value: Option<i32>,
) {
  if let Some(value) = value {
    map.insert(key.to_string(), json!(value));
  }
}

fn insert_optional_vec(
  map: &mut Map<String, Value>,
  key: &str,
  value: &[String],
) {
  if !value.is_empty() {
    map.insert(key.to_string(), json!(value));
  }
}

fn response_error(text: &str) -> String {
  if text.trim().is_empty() {
    return "empty response body".to_string();
  }
  if let Ok(value) = serde_json::from_str::<Value>(text) {
    if let Some(error) = value.get("error").and_then(Value::as_str) {
      return error.to_string();
    }
    if let Some(message) = value.get("message").and_then(Value::as_str) {
      return message.to_string();
    }
  }
  text.to_string()
}
