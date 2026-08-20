//! Declarative bootstrap: upsert projects, jobsets, API keys and users from
//! config.
//!
//! Called once on server startup to reconcile declarative configuration
//! with database state. Uses upsert semantics so repeated runs are idempotent.

use std::collections::HashMap;

use circus_config::{DeclarativeConfig, DeclarativeWebhook};
use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::{
  db::PgPool,
  error::Result,
  models::{CreateJobset, CreateProject, JobsetState, JobsetTriggerMode},
  repo,
};

fn resolve_secret(
  inline: Option<&str>,
  file: Option<&str>,
  on_file_error: impl Fn(&str, &std::io::Error),
) -> Option<String> {
  if let Some(inline) = inline {
    return Some(inline.to_string());
  }

  let file = file?;
  let expanded = shellexpand::full(file)
    .map_or_else(|_| file.to_string(), std::borrow::Cow::into_owned);
  match std::fs::read_to_string(&expanded) {
    Ok(value) => Some(value.trim().to_string()),
    Err(error) => {
      on_file_error(&expanded, &error);
      None
    },
  }
}

fn resolve_webhook_secret(webhook: &DeclarativeWebhook) -> Option<String> {
  resolve_secret(
    webhook.secret.as_deref(),
    webhook.secret_file.as_deref(),
    |file, error| {
      tracing::warn!(
        forge_type = %webhook.forge_type,
        file,
        "Failed to read webhook secret file: {error}"
      );
    },
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
pub async fn run(
  pool: &PgPool,
  config: &DeclarativeConfig,
  webhook_secret_encryption_key: Option<&str>,
) -> Result<()> {
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
      name:            decl_project.name.clone(),
      repository_url:  decl_project.repository_url.clone(),
      description:     decl_project.description.clone(),
      cache_enabled:   decl_project.cache_enabled,
      cache_url:       decl_project.cache_url.clone(),
      cache_upstreams: crate::models::BinaryCacheUpstreams(
        decl_project.cache_upstreams.clone(),
      ),
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
      let trigger_mode = decl_jobset
        .trigger_mode
        .as_deref()
        .map(JobsetTriggerMode::from_config_str);

      let jobset = repo::jobsets::upsert(pool, CreateJobset {
        project_id: project.id,
        name: decl_jobset.name.clone(),
        nix_expression: decl_jobset.nix_expression.clone(),
        enabled: Some(decl_jobset.enabled),
        flake_mode: Some(decl_jobset.flake_mode),
        check_interval: Some(decl_jobset.check_interval),
        trigger_mode,
        branch: decl_jobset.branch.clone(),
        branch_pattern: decl_jobset.branch_pattern.clone(),
        tag_pattern: decl_jobset.tag_pattern.clone(),
        scheduling_shares: Some(decl_jobset.scheduling_shares),
        state,
        keep_nr: decl_jobset.keep_nr,
        systems: decl_jobset.systems.clone(),
        only_build_latest: Some(decl_jobset.only_build_latest),
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
      // Notification config blobs are already validated and encrypted by the
      // caller (see circus_notification::encrypt_declarative_notifications);
      // store them verbatim here.
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
        webhook_secret_encryption_key,
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
    let key = resolve_secret(
      decl_key.key.as_deref(),
      decl_key.key_file.as_deref(),
      |file, error| {
        tracing::warn!(
          name = %decl_key.name,
          file,
          "Failed to read API key file: {error}"
        );
      },
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
      repo::api_keys::upsert(pool, &decl_key.name, &key_hash, decl_key.role)
        .await?;

    tracing::info!(
        name = %api_key.name,
        role = %api_key.role,
        "Upserted declarative API key"
    );
  }

  // Upsert users
  for decl_user in &config.users {
    let password = resolve_secret(
      decl_user.password.as_deref(),
      decl_user.password_file.as_deref(),
      |file, error| {
        tracing::warn!(
          username = %decl_user.username,
          file,
          "Failed to read password file: {error}"
        );
      },
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
        role: Some(decl_user.role),
        enabled: Some(decl_user.enabled),
        public_dashboard: None,
      };
      if let Err(e) = repo::users::update(pool, user.id, &update, None).await {
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
        role:      Some(decl_user.role),
      };
      match repo::users::create(pool, &create, None).await {
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
  if let Err(e) =
    crate::pg_notify::notify(pool, crate::pg_notify::CHANNEL_JOBSETS_CHANGED)
      .await
  {
    tracing::warn!("Failed to notify evaluator after bootstrap: {e}");
  }

  tracing::info!("Declarative bootstrap complete");
  Ok(())
}
