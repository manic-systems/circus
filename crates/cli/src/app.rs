use std::ffi::OsString;

use clap::Parser;
use color_eyre::Result;
use serde_json::{Map, Value, json};

use crate::{
  client::ApiClient,
  commands::{
    BuildCommand,
    ChannelCommand,
    Cli,
    Command,
    EvaluationCommand,
    JobsetCommand,
    LogCommand,
    NewsCommand,
    ProjectCommand,
  },
  output::{
    array_items,
    field,
    items,
    print_builds,
    print_json,
    print_page,
    print_table,
    search_rows,
    short,
  },
};

/// Run the Circus API client CLI.
///
/// # Errors
///
/// Returns an error when argument parsing, API requests, or output rendering
/// fail.
pub async fn run() -> Result<()> {
  run_from(std::env::args_os().collect()).await
}

/// Run the Circus API client CLI with explicit argv values.
///
/// # Errors
///
/// Returns an error when argument parsing, API requests, or output rendering
/// fail.
pub async fn run_from(args: Vec<OsString>) -> Result<()> {
  let cli = Cli::parse_from(args);
  let base_url = cli
    .url
    .or_else(|| std::env::var("CIRCUS_URL").ok())
    .unwrap_or_else(|| "http://localhost:3000".to_string());
  let api_key = cli.api_key.or_else(|| std::env::var("CIRCUS_API_KEY").ok());
  let runner = CommandRunner {
    api:         ApiClient::new(&base_url, api_key)?,
    json_output: cli.json,
  };

  runner.run(cli.command).await
}

pub(super) struct CommandRunner {
  pub(super) api:         ApiClient,
  pub(super) json_output: bool,
}

impl CommandRunner {
  async fn run(&self, command: Command) -> Result<()> {
    match command {
      Command::Health => self.health().await,
      Command::Projects { command } => self.projects(command).await,
      Command::Jobsets { command } => self.jobsets(command).await,
      Command::Builds { command } => self.builds(command).await,
      Command::Queue => self.queue().await,
      Command::Logs { command } => self.logs(command).await,
      Command::Search { query, limit } => self.search(&query, limit).await,
      Command::Channels { command } => self.channels(command).await,
      Command::News { command } => self.news(command).await,
      Command::Admin { command } => self.admin(command).await,
      Command::Migrate { command } => {
        circus_common::migrate_cli::run_command(command).await
      },
      Command::Evaluations { command } => self.evaluations(command).await,
    }
  }

  async fn health(&self) -> Result<()> {
    let response = self.api.get("health", false).await?;
    if self.json_output {
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

  async fn projects(&self, command: ProjectCommand) -> Result<()> {
    match command {
      ProjectCommand::List { limit, offset } => {
        let response = self
          .api
          .get(
            &with_query("api/v1/projects", &[
              ("limit", limit.to_string()),
              ("offset", offset.to_string()),
            ]),
            false,
          )
          .await?;
        if self.json_output {
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
        body
          .insert("repository_url".to_string(), Value::String(repository_url));
        insert_optional(&mut body, "description", description);
        let response = self
          .api
          .post("api/v1/projects", Value::Object(body), true)
          .await?;
        if self.json_output {
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
        let response = self
          .api
          .delete(&format!("api/v1/projects/{id}"), true)
          .await?;
        if self.json_output {
          return print_json(&response);
        }
        println!("Deleted project {id}");
      },
    }
    Ok(())
  }

  async fn jobsets(&self, command: JobsetCommand) -> Result<()> {
    match command {
      JobsetCommand::List {
        project_id,
        limit,
        offset,
      } => {
        let response = self
          .api
          .get(
            &with_query(&format!("api/v1/projects/{project_id}/jobsets"), &[
              ("limit", limit.to_string()),
              ("offset", offset.to_string()),
            ]),
            false,
          )
          .await?;
        if self.json_output {
          return print_json(&response);
        }
        print_table(
          &["ID", "Name", "Expr", "Enabled", "Flake", "Interval"],
          &items(&response)
            .iter()
            .map(|jobset| {
              vec![
                field(jobset, "id"),
                field(jobset, "name"),
                field(jobset, "nix_expression"),
                field(jobset, "enabled"),
                field(jobset, "flake_mode"),
                field(jobset, "check_interval"),
              ]
            })
            .collect::<Vec<_>>(),
        );
        print_page(&response);
      },
      JobsetCommand::Create {
        project_id,
        name,
        nix_expression,
        enabled,
        flake_mode,
        check_interval,
      } => {
        let mut body = Map::new();
        body.insert("name".to_string(), Value::String(name));
        body
          .insert("nix_expression".to_string(), Value::String(nix_expression));
        insert_optional_bool(&mut body, "enabled", enabled);
        insert_optional_bool(&mut body, "flake_mode", flake_mode);
        insert_optional_i32(&mut body, "check_interval", check_interval);
        let response = self
          .api
          .post(
            &format!("api/v1/projects/{project_id}/jobsets"),
            Value::Object(body),
            true,
          )
          .await?;
        if self.json_output {
          return print_json(&response);
        }
        print_table(&["ID", "Name", "Expr"], &[vec![
          field(&response, "id"),
          field(&response, "name"),
          field(&response, "nix_expression"),
        ]]);
      },
      JobsetCommand::Show { project_id, id } => {
        let response = self
          .api
          .get(&format!("api/v1/projects/{project_id}/jobsets/{id}"), false)
          .await?;
        if self.json_output {
          return print_json(&response);
        }
        print_table(&["ID", "Name", "Expr", "Enabled", "State"], &[vec![
          field(&response, "id"),
          field(&response, "name"),
          field(&response, "nix_expression"),
          field(&response, "enabled"),
          field(&response, "state"),
        ]]);
      },
      JobsetCommand::Delete { project_id, id } => {
        let response = self
          .api
          .delete(&format!("api/v1/projects/{project_id}/jobsets/{id}"), true)
          .await?;
        if self.json_output {
          return print_json(&response);
        }
        println!("Deleted jobset {id}");
      },
      JobsetCommand::Inputs {
        project_id,
        jobset_id,
      } => {
        let response = self
          .api
          .get(
            &format!("api/v1/projects/{project_id}/jobsets/{jobset_id}/inputs"),
            false,
          )
          .await?;
        if self.json_output {
          return print_json(&response);
        }
        print_table(
          &["ID", "Name", "Type", "Value", "Revision"],
          &array_items(&response)
            .iter()
            .map(|input| {
              vec![
                field(input, "id"),
                field(input, "name"),
                field(input, "input_type"),
                field(input, "value"),
                field(input, "revision"),
              ]
            })
            .collect::<Vec<_>>(),
        );
      },
    }
    Ok(())
  }

  async fn builds(&self, command: BuildCommand) -> Result<()> {
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
        let response = self
          .api
          .get(&with_query("api/v1/builds", &params), false)
          .await?;
        if self.json_output {
          return print_json(&response);
        }
        print_builds(&response);
        print_page(&response);
      },
      BuildCommand::Cancel { id } => {
        self.build_action(&id, "cancel", "Cancelled").await?;
      },
      BuildCommand::Restart { id } => {
        self.build_action(&id, "restart", "Restarted").await?;
      },
      BuildCommand::Bump { id } => {
        self.build_action(&id, "bump", "Bumped").await?;
      },
      BuildCommand::Keep { id, value } => {
        let response = self
          .api
          .put(&format!("api/v1/builds/{id}/keep/{value}"), json!({}), true)
          .await?;
        if self.json_output {
          return print_json(&response);
        }
        println!("Set keep={value} for build {id}");
        print_builds(&response);
      },
    }
    Ok(())
  }

  async fn build_action(
    &self,
    id: &str,
    action: &str,
    label: &str,
  ) -> Result<()> {
    let response = self
      .api
      .post(&format!("api/v1/builds/{id}/{action}"), json!({}), true)
      .await?;
    if self.json_output {
      return print_json(&response);
    }
    println!("{label} build {id}");
    print_builds(&response);
    Ok(())
  }

  async fn queue(&self) -> Result<()> {
    let response = self.api.get("api/v1/operator/overview", false).await?;
    if self.json_output {
      return print_json(&response);
    }
    print_table(&["Pending", "Running", "Workers"], &[vec![
      field(&response, "pending_builds"),
      field(&response, "running_builds"),
      format!(
        "{}/{}",
        field(&response, "worker_online"),
        field(&response, "worker_total")
      ),
    ]]);
    println!();
    print_table(
      &["System", "Queued"],
      &array_items(response.get("queue_by_system").unwrap_or(&Value::Null))
        .iter()
        .map(|item| vec![field(item, "system"), field(item, "count")])
        .collect::<Vec<_>>(),
    );
    Ok(())
  }

  async fn logs(&self, command: LogCommand) -> Result<()> {
    match command {
      LogCommand::Show { build_id } => {
        let response = self
          .api
          .get(&format!("api/v1/builds/{build_id}/log"), false)
          .await?;
        if self.json_output {
          return print_json(&response);
        }
        println!("{}", field(&response, "body"));
      },
    }
    Ok(())
  }

  async fn search(&self, query: &str, limit: u32) -> Result<()> {
    let response = self
      .api
      .get(
        &with_query("api/v1/search", &[
          ("q", query.to_string()),
          ("limit", limit.to_string()),
          ("entities", "projects".to_string()),
          ("entities", "jobsets".to_string()),
          ("entities", "evaluations".to_string()),
          ("entities", "builds".to_string()),
        ]),
        false,
      )
      .await?;
    if self.json_output {
      return print_json(&response);
    }
    print_table(&["Type", "ID", "Name"], &search_rows(&response));
    Ok(())
  }

  async fn channels(&self, command: ChannelCommand) -> Result<()> {
    match command {
      ChannelCommand::List => {
        let response = self.api.get("api/v1/channels", false).await?;
        if self.json_output {
          return print_json(&response);
        }
        print_table(
          &["ID", "Name", "Project", "Jobset", "Evaluation"],
          &array_items(&response)
            .iter()
            .map(|channel| {
              vec![
                field(channel, "id"),
                field(channel, "name"),
                field(channel, "project_id"),
                field(channel, "jobset_id"),
                field(channel, "current_evaluation_id"),
              ]
            })
            .collect::<Vec<_>>(),
        );
      },
      ChannelCommand::Show { id } => {
        let response = self
          .api
          .get(&format!("api/v1/channels/{id}"), false)
          .await?;
        if self.json_output {
          return print_json(&response);
        }
        print_table(&["ID", "Name", "Project", "Jobset", "Evaluation"], &[
          vec![
            field(&response, "id"),
            field(&response, "name"),
            field(&response, "project_id"),
            field(&response, "jobset_id"),
            field(&response, "current_evaluation_id"),
          ],
        ]);
      },
      ChannelCommand::Delete { id } => {
        let response = self
          .api
          .delete(&format!("api/v1/channels/{id}"), true)
          .await?;
        if self.json_output {
          return print_json(&response);
        }
        println!("Deleted channel {id}");
      },
    }
    Ok(())
  }

  async fn news(&self, command: NewsCommand) -> Result<()> {
    match command {
      NewsCommand::List => {
        let response = self.api.get("api/v1/news", false).await?;
        if self.json_output {
          return print_json(&response);
        }
        print_table(
          &["ID", "Title", "Created"],
          &array_items(&response)
            .iter()
            .map(|item| {
              vec![
                field(item, "id"),
                field(item, "title"),
                field(item, "created_at"),
              ]
            })
            .collect::<Vec<_>>(),
        );
      },
      NewsCommand::Create { title, content } => {
        let response = self
          .api
          .post(
            "api/v1/news",
            json!({ "title": title, "content": content }),
            true,
          )
          .await?;
        if self.json_output {
          return print_json(&response);
        }
        println!("Created news item {}", field(&response, "id"));
      },
      NewsCommand::Delete { id } => {
        let response =
          self.api.delete(&format!("api/v1/news/{id}"), true).await?;
        if self.json_output {
          return print_json(&response);
        }
        println!("Deleted news item {id}");
      },
    }
    Ok(())
  }

  async fn evaluations(&self, command: EvaluationCommand) -> Result<()> {
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
        let response = self
          .api
          .get(&with_query("api/v1/evaluations", &params), false)
          .await?;
        if self.json_output {
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
        let response = self
          .api
          .post("api/v1/evaluations/trigger", Value::Object(body), true)
          .await?;
        if self.json_output {
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
}

pub(super) fn with_query(path: &str, params: &[(&str, String)]) -> String {
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

pub(super) fn insert_optional(
  map: &mut Map<String, Value>,
  key: &str,
  value: Option<String>,
) {
  if let Some(value) = value {
    map.insert(key.to_string(), Value::String(value));
  }
}

pub(super) fn insert_optional_i32(
  map: &mut Map<String, Value>,
  key: &str,
  value: Option<i32>,
) {
  if let Some(value) = value {
    map.insert(key.to_string(), json!(value));
  }
}

fn insert_optional_bool(
  map: &mut Map<String, Value>,
  key: &str,
  value: Option<bool>,
) {
  if let Some(value) = value {
    map.insert(key.to_string(), Value::Bool(value));
  }
}

pub(super) fn insert_optional_vec(
  map: &mut Map<String, Value>,
  key: &str,
  value: &[String],
) {
  if !value.is_empty() {
    map.insert(key.to_string(), json!(value));
  }
}
