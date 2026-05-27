//! Declarative bootstrap: upsert projects, jobsets, API keys and users from
//! config.
//!
//! Called once on server startup to reconcile declarative configuration
//! with database state. Uses upsert semantics so repeated runs are idempotent.

use std::collections::HashMap;

use sha2::{Digest, Sha256};
use sqlx::PgPool;
use uuid::Uuid;

use crate::{
  config::{DeclarativeConfig, DeclarativeWebhook},
  error::Result,
  models::{CreateJobset, CreateProject, JobsetState},
  repo,
};

/// Expand path with environment variables and home directory.
/// Supports ${VAR}, $VAR, and ~ for home directory.
fn expand_path(path: &str) -> String {
  let expanded = if path.starts_with('~') {
    std::env::var_os("HOME").map_or_else(
      || path.to_string(),
      |home| path.replacen('~', &home.to_string_lossy(), 1),
    )
  } else {
    path.to_string()
  };

  // Expand ${VAR} and $VAR patterns
  let mut result = expanded;
  while let Some(start) = result.find("${") {
    if let Some(end) = result[start..].find('}') {
      let var_name = &result[start + 2..start + end];
      let replacement = std::env::var(var_name).unwrap_or_default();
      result = format!(
        "{}{}{}",
        &result[..start],
        replacement,
        &result[start + end + 1..]
      );
    } else {
      break;
    }
  }
  result
}

/// Resolve secret for a webhook from inline value or file.
fn resolve_webhook_secret(webhook: &DeclarativeWebhook) -> Option<String> {
  webhook.secret.as_ref().map_or_else(
    || {
      webhook.secret_file.as_ref().and_then(|file| {
        let expanded = expand_path(file);
        match std::fs::read_to_string(&expanded) {
          Ok(s) => Some(s.trim().to_string()),
          Err(e) => {
            tracing::warn!(
              forge_type = %webhook.forge_type,
              file = %expanded,
              "Failed to read webhook secret file: {e}"
            );
            None
          },
        }
      })
    },
    |secret| Some(secret.clone()),
  )
}

/// Bootstrap declarative configuration into the database.
///
/// This function is idempotent: running it multiple times with the same config
/// produces the same database state. It upserts (insert or update) all
/// configured projects, jobsets, API keys, and users.
///
/// # Errors
///
/// Returns error if database operations fail.
pub async fn run(pool: &PgPool, config: &DeclarativeConfig) -> Result<()> {
  if config.projects.is_empty()
    && config.api_keys.is_empty()
    && config.users.is_empty()
    && config.remote_builders.is_empty()
  {
    return Ok(());
  }

  let n_projects = config.projects.len();
  let n_jobsets: usize = config.projects.iter().map(|p| p.jobsets.len()).sum();
  let n_keys = config.api_keys.len();
  let n_users = config.users.len();
  let n_builders = config.remote_builders.len();

  tracing::info!(
    projects = n_projects,
    jobsets = n_jobsets,
    api_keys = n_keys,
    users = n_users,
    remote_builders = n_builders,
    "Bootstrapping declarative configuration"
  );

  // Upsert projects and their jobsets
  for decl_project in &config.projects {
    let project = repo::projects::upsert(pool, CreateProject {
      name:           decl_project.name.clone(),
      repository_url: decl_project.repository_url.clone(),
      description:    decl_project.description.clone(),
    })
    .await?;

    tracing::info!(
        project = %project.name,
        id = %project.id,
        "Upserted declarative project"
    );

    for decl_jobset in &decl_project.jobsets {
      let state = decl_jobset
        .state
        .as_deref()
        .map(JobsetState::from_config_str);

      let jobset = repo::jobsets::upsert(pool, CreateJobset {
        project_id: project.id,
        name: decl_jobset.name.clone(),
        nix_expression: decl_jobset.nix_expression.clone(),
        enabled: Some(decl_jobset.enabled),
        flake_mode: Some(decl_jobset.flake_mode),
        check_interval: Some(decl_jobset.check_interval),
        branch: decl_jobset.branch.clone(),
        scheduling_shares: Some(decl_jobset.scheduling_shares),
        state,
        keep_nr: decl_jobset.keep_nr,
      })
      .await?;

      tracing::info!(
          project = %project.name,
          jobset = %jobset.name,
          "Upserted declarative jobset"
      );

      // Sync jobset inputs
      if !decl_jobset.inputs.is_empty() {
        repo::jobset_inputs::sync_for_jobset(
          pool,
          jobset.id,
          &decl_jobset.inputs,
        )
        .await?;
        tracing::info!(
            project = %project.name,
            jobset = %jobset.name,
            inputs = decl_jobset.inputs.len(),
            "Synced declarative jobset inputs"
        );
      }
    }

    // Build jobset name -> ID map for channel resolution
    let jobset_map: HashMap<String, Uuid> = {
      let jobsets =
        repo::jobsets::list_for_project(pool, project.id, 1000, 0).await?;
      jobsets.into_iter().map(|j| (j.name, j.id)).collect()
    };

    // Sync notifications
    if !decl_project.notifications.is_empty() {
      repo::notification_configs::sync_for_project(
        pool,
        project.id,
        &decl_project.notifications,
      )
      .await?;
      tracing::info!(
          project = %project.name,
          notifications = decl_project.notifications.len(),
          "Synced declarative notifications"
      );
    }

    // Sync webhooks
    if !decl_project.webhooks.is_empty() {
      repo::webhook_configs::sync_for_project(
        pool,
        project.id,
        &decl_project.webhooks,
        resolve_webhook_secret,
      )
      .await?;
      tracing::info!(
          project = %project.name,
          webhooks = decl_project.webhooks.len(),
          "Synced declarative webhooks"
      );
    }

    // Sync channels
    if !decl_project.channels.is_empty() {
      repo::channels::sync_for_project(
        pool,
        project.id,
        &decl_project.channels,
        |name| jobset_map.get(name).copied(),
      )
      .await?;
      tracing::info!(
          project = %project.name,
          channels = decl_project.channels.len(),
          "Synced declarative channels"
      );
    }
  }

  // Upsert API keys
  for decl_key in &config.api_keys {
    // Resolve key from inline or file
    let key = decl_key.key.as_ref().map_or_else(
      || {
        decl_key.key_file.as_ref().and_then(|file| {
          let expanded = expand_path(file);
          match std::fs::read_to_string(&expanded) {
            Ok(k) => Some(k.trim().to_string()),
            Err(e) => {
              tracing::warn!(
                name = %decl_key.name,
                file = %expanded,
                "Failed to read API key file: {e}"
              );
              None
            },
          }
        })
      },
      |k| Some(k.clone()),
    );

    let Some(key) = key else {
      tracing::warn!(
        name = %decl_key.name,
        "Declarative API key has no key or key_file set, skipping"
      );
      continue;
    };

    let mut hasher = Sha256::new();
    hasher.update(key.as_bytes());
    let key_hash = hex::encode(hasher.finalize());

    let api_key =
      repo::api_keys::upsert(pool, &decl_key.name, &key_hash, &decl_key.role)
        .await?;

    tracing::info!(
        name = %api_key.name,
        role = %api_key.role,
        "Upserted declarative API key"
    );
  }

  // Upsert users
  for decl_user in &config.users {
    // Resolve password from inline or file
    let password = decl_user.password.as_ref().map_or_else(
      || {
        decl_user.password_file.as_ref().and_then(|file| {
          let expanded = expand_path(file);
          match std::fs::read_to_string(&expanded) {
            Ok(p) => Some(p.trim().to_string()),
            Err(e) => {
              tracing::warn!(
                username = %decl_user.username,
                file = %expanded,
                "Failed to read password file: {e}"
              );
              None
            },
          }
        })
      },
      |p| Some(p.clone()),
    );

    // Check if user exists
    let existing =
      repo::users::get_by_username(pool, &decl_user.username).await?;

    if let Some(user) = existing {
      // Update existing user
      let update = crate::models::UpdateUser {
        email: Some(decl_user.email.clone()),
        full_name: decl_user.full_name.clone(),
        password,
        role: Some(decl_user.role.clone()),
        enabled: Some(decl_user.enabled),
        public_dashboard: None,
      };
      if let Err(e) = repo::users::update(pool, user.id, &update).await {
        tracing::warn!(
          username = %decl_user.username,
          "Failed to update declarative user: {e}"
        );
      } else {
        tracing::info!(
          username = %decl_user.username,
          "Updated declarative user"
        );
      }
    } else if let Some(pwd) = password {
      // Create new user
      let create = crate::models::CreateUser {
        username:  decl_user.username.clone(),
        email:     decl_user.email.clone(),
        full_name: decl_user.full_name.clone(),
        password:  pwd,
        role:      Some(decl_user.role.clone()),
      };
      match repo::users::create(pool, &create).await {
        Ok(user) => {
          tracing::info!(
            username = %user.username,
            "Created declarative user"
          );
          // Set enabled status if false (users are enabled by default)
          if !decl_user.enabled
            && let Err(e) = repo::users::set_enabled(pool, user.id, false).await
          {
            tracing::warn!(
              username = %user.username,
              "Failed to disable declarative user: {e}"
            );
          }
        },
        Err(e) => {
          tracing::warn!(
            username = %decl_user.username,
            "Failed to create declarative user: {e}"
          );
        },
      }
    } else {
      tracing::warn!(
        username = %decl_user.username,
        "Declarative user has no password set, skipping creation"
      );
    }
  }

  // Sync remote builders
  if !config.remote_builders.is_empty() {
    repo::remote_builders::sync_all(pool, &config.remote_builders).await?;
    tracing::info!(
      builders = config.remote_builders.len(),
      "Synced declarative remote builders"
    );
  }

  // Build username -> user ID map for project member resolution
  let user_map: HashMap<String, Uuid> = {
    // Get all users (use large limit to get all)
    let users = repo::users::list(pool, 10000, 0).await?;
    users.into_iter().map(|u| (u.username, u.id)).collect()
  };

  // Sync project members (now that users exist)
  for decl_project in &config.projects {
    if decl_project.members.is_empty() {
      continue;
    }

    // Get project by name (already exists from earlier upsert)
    if let Ok(project) =
      repo::projects::get_by_name(pool, &decl_project.name).await
    {
      repo::project_members::sync_for_project(
        pool,
        project.id,
        &decl_project.members,
        |username| user_map.get(username).copied(),
      )
      .await?;
      tracing::info!(
          project = %project.name,
          members = decl_project.members.len(),
          "Synced declarative project members"
      );
    }
  }

  // Wake the evaluator so it picks up newly bootstrapped jobsets immediately
  // instead of waiting for the next poll interval.
  if let Err(e) = sqlx::query("SELECT pg_notify($1, '')")
    .bind(crate::pg_notify::CHANNEL_JOBSETS_CHANGED)
    .execute(pool)
    .await
  {
    tracing::warn!("Failed to notify evaluator after bootstrap: {e}");
  }

  tracing::info!("Declarative bootstrap complete");
  Ok(())
}
