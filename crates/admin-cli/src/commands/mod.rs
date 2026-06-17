use color_eyre::eyre::{Context, Result, bail, eyre};
use serde_json::{Map, Value, json};

use crate::{
  ApiKeyCommand,
  BuildCommand,
  BuilderCommand,
  ConfigCommand,
  EvaluationCommand,
  NotificationCommand,
  PinnedOutputCommand,
  ProjectCommand,
  UserCommand,
  client::{
    ApiClient,
    insert_optional,
    insert_optional_i32,
    insert_optional_vec,
    push_optional,
    with_query,
  },
  output::{
    field,
    items,
    print_builder_sessions,
    print_builders,
    print_builds,
    print_json,
    print_page,
    print_pinned_outputs,
    print_table,
    print_users,
    short,
  },
};

pub async fn health(api: &ApiClient, json_output: bool) -> Result<()> {
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

pub async fn status(api: &ApiClient, json_output: bool) -> Result<()> {
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

pub async fn projects(
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

pub async fn api_keys(
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

pub async fn users(
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

pub async fn update_user_enabled(
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

pub async fn builders(
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

pub async fn update_builder_enabled(
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

pub async fn builds(
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

pub async fn build_action(
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

pub async fn evaluations(
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

pub async fn notifications(
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

pub async fn pinned_outputs(
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

pub async fn audit(
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

pub async fn config(
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
