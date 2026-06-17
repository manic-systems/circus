use color_eyre::{
  Result,
  eyre::{Context, bail, eyre},
};
use serde_json::{Map, Value, json};

use crate::{
  app::{
    CommandRunner,
    insert_optional,
    insert_optional_i32,
    insert_optional_vec,
    with_query,
  },
  commands::{
    AdminCommand,
    ApiKeyCommand,
    BuilderCommand,
    ConfigCommand,
    NotificationCommand,
    PinnedOutputCommand,
    UserCommand,
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

impl CommandRunner {
  pub(super) async fn status(&self) -> Result<()> {
    let response = self.api.get("api/v1/admin/system", true).await?;
    if self.json_output {
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

  pub(super) async fn api_keys(&self, command: ApiKeyCommand) -> Result<()> {
    match command {
      ApiKeyCommand::List => {
        let response = self.api.get("api/v1/api-keys", true).await?;
        if self.json_output {
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
        let response = self
          .api
          .post("api/v1/api-keys", Value::Object(body), true)
          .await?;
        if self.json_output {
          return print_json(&response);
        }
        println!(
          "Created API key. Save the secret now; it cannot be recovered."
        );
        print_table(&["ID", "Name", "Role", "Key"], &[vec![
          field(&response, "id"),
          field(&response, "name"),
          field(&response, "role"),
          field(&response, "key"),
        ]]);
      },
      ApiKeyCommand::Revoke { id } => {
        let response = self
          .api
          .delete(&format!("api/v1/api-keys/{id}"), true)
          .await?;
        if self.json_output {
          return print_json(&response);
        }
        println!("Revoked API key {id}");
      },
    }
    Ok(())
  }

  pub(super) async fn users(&self, command: UserCommand) -> Result<()> {
    match command {
      UserCommand::List { limit, offset } => {
        let response = self
          .api
          .get(
            &with_query("api/v1/users", &[
              ("limit", limit.to_string()),
              ("offset", offset.to_string()),
            ]),
            true,
          )
          .await?;
        if self.json_output {
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
        let response = self
          .api
          .post("api/v1/users", Value::Object(body), true)
          .await?;
        if self.json_output {
          return print_json(&response);
        }
        println!("Created user {}", field(&response, "username"));
        print_users(&response);
      },
      UserCommand::SetRole { id, role } => {
        let response = self
          .api
          .put(&format!("api/v1/users/{id}"), json!({ "role": role }), true)
          .await?;
        if self.json_output {
          return print_json(&response);
        }
        println!("Updated user role for {id}");
        print_users(&response);
      },
      UserCommand::Enable { id } => {
        self.update_user_enabled(&id, true).await?;
      },
      UserCommand::Disable { id } => {
        self.update_user_enabled(&id, false).await?;
      },
      UserCommand::Delete { id } => {
        let response =
          self.api.delete(&format!("api/v1/users/{id}"), true).await?;
        if self.json_output {
          return print_json(&response);
        }
        println!("Deleted user {id}");
      },
    }
    Ok(())
  }

  async fn update_user_enabled(&self, id: &str, enabled: bool) -> Result<()> {
    let response = self
      .api
      .put(
        &format!("api/v1/users/{id}"),
        json!({ "enabled": enabled }),
        true,
      )
      .await?;
    if self.json_output {
      return print_json(&response);
    }
    println!("{} user {id}", if enabled { "Enabled" } else { "Disabled" });
    print_users(&response);
    Ok(())
  }

  pub(super) async fn builders(&self, command: BuilderCommand) -> Result<()> {
    match command {
      BuilderCommand::List => {
        let response = self.api.get("api/v1/admin/builders", false).await?;
        if self.json_output {
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
        insert_optional_vec(
          &mut body,
          "supported_features",
          &supported_features,
        );
        insert_optional_vec(
          &mut body,
          "mandatory_features",
          &mandatory_features,
        );
        insert_optional(&mut body, "public_host_key", public_host_key);
        insert_optional(&mut body, "ssh_key_file", ssh_key_file);
        let response = self
          .api
          .post("api/v1/admin/builders", Value::Object(body), true)
          .await?;
        if self.json_output {
          return print_json(&response);
        }
        println!("Registered builder {}", field(&response, "name"));
        print_builders(&response);
      },
      BuilderCommand::Enable { id } => {
        self.update_builder_enabled(&id, true).await?;
      },
      BuilderCommand::Disable { id } => {
        self.update_builder_enabled(&id, false).await?;
      },
      BuilderCommand::Remove { id } => {
        let response = self
          .api
          .delete(&format!("api/v1/admin/builders/{id}"), true)
          .await?;
        if self.json_output {
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
        let response = self.api.get(path, true).await?;
        if self.json_output {
          return print_json(&response);
        }
        print_builder_sessions(&response);
      },
      BuilderCommand::Session { machine_id } => {
        let response = self
          .api
          .get(
            &format!("api/v1/admin/builders/sessions/{machine_id}"),
            true,
          )
          .await?;
        if self.json_output {
          return print_json(&response);
        }
        print_builder_sessions(&response);
      },
    }
    Ok(())
  }

  async fn update_builder_enabled(
    &self,
    id: &str,
    enabled: bool,
  ) -> Result<()> {
    let response = self
      .api
      .put(
        &format!("api/v1/admin/builders/{id}"),
        json!({ "enabled": enabled }),
        true,
      )
      .await?;
    if self.json_output {
      return print_json(&response);
    }
    println!(
      "{} builder {id}",
      if enabled { "Enabled" } else { "Disabled" }
    );
    print_builders(&response);
    Ok(())
  }

  pub(super) async fn admin(&self, command: AdminCommand) -> Result<()> {
    match command {
      AdminCommand::Status => self.status().await,
      AdminCommand::ApiKeys { command } => self.api_keys(command).await,
      AdminCommand::Users { command } => self.users(command).await,
      AdminCommand::Builders { command } => self.builders(command).await,
      AdminCommand::Notifications { command } => {
        self.notifications(command).await
      },
      AdminCommand::PinnedOutputs { command } => {
        self.pinned_outputs(command).await
      },
      AdminCommand::Audit { limit, offset } => self.audit(limit, offset).await,
      AdminCommand::Config { command } => self.config(command).await,
    }
  }

  async fn notifications(&self, command: NotificationCommand) -> Result<()> {
    match command {
      NotificationCommand::List => {
        let response = self
          .api
          .get("api/v1/admin/notification-tasks", true)
          .await?;
        if self.json_output {
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
        let response = self
          .api
          .post(
            &format!("api/v1/admin/notification-tasks/{id}/retry"),
            json!({}),
            true,
          )
          .await?;
        if self.json_output {
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

  async fn pinned_outputs(&self, command: PinnedOutputCommand) -> Result<()> {
    match command {
      PinnedOutputCommand::List { limit, offset } => {
        let response = self
          .api
          .get(
            &with_query("api/v1/admin/pinned-build-products", &[
              ("limit", limit.to_string()),
              ("offset", offset.to_string()),
            ]),
            true,
          )
          .await?;
        if self.json_output {
          return print_json(&response);
        }
        print_pinned_outputs(&response);
        print_page(&response);
      },
      PinnedOutputCommand::Unpin { build_id } => {
        let response = self
          .api
          .post(
            &format!("api/v1/admin/pinned-builds/{build_id}/unpin"),
            json!({}),
            true,
          )
          .await?;
        if self.json_output {
          return print_json(&response);
        }
        println!("Unpinned build {build_id}");
        print_builds(&response);
      },
    }
    Ok(())
  }

  async fn audit(&self, limit: u32, offset: u32) -> Result<()> {
    let response = self
      .api
      .get(
        &with_query("api/v1/admin/audit-log", &[
          ("limit", limit.to_string()),
          ("offset", offset.to_string()),
        ]),
        true,
      )
      .await?;
    if self.json_output {
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

  async fn config(&self, command: ConfigCommand) -> Result<()> {
    match command {
      ConfigCommand::Get => {
        let response = self.api.get("api/v1/admin/config", true).await?;
        if self.json_output {
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
        let response = self
          .api
          .put("api/v1/admin/config", json!({ "contents": contents }), true)
          .await?;
        if self.json_output {
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
}
