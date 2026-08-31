//! Declarative bootstrap: upsert projects, jobsets, API keys and users from
//! config.
//!
//! Called once on server startup to reconcile declarative configuration
//! with database state. Uses upsert semantics so repeated runs are idempotent.

use std::collections::HashMap;

use circus_config::{
  DeclarativeApiKey,
  DeclarativeConfig,
  DeclarativeProject,
  DeclarativeUser,
  DeclarativeWebhook,
};
use sha2::{Digest, Sha256};

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

const fn should_reconcile(authoritative: bool, is_empty: bool) -> bool {
  authoritative || !is_empty
}

async fn sync_declarative_project(
  pool: &PgPool,
  decl_project: &DeclarativeProject,
  allow_runtime_mutation: bool,
  webhook_secret_encryption_key: Option<&str>,
) -> Result<()> {
  let authoritative = !allow_runtime_mutation;
  let project = repo::projects::upsert_declarative(
    pool,
    CreateProject {
      name:            decl_project.name.clone(),
      repository_url:  decl_project.repository_url.clone(),
      description:     decl_project.description.clone(),
      cache_enabled:   decl_project.cache_enabled,
      cache_url:       decl_project.cache_url.clone(),
      cache_upstreams: crate::models::BinaryCacheUpstreams(
        decl_project.cache_upstreams.clone(),
      ),
    },
    decl_project.allow_runtime_mutation,
  )
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
      path_filters: Some(decl_jobset.path_filters.clone()),
    })
    .await?;

    if should_reconcile(authoritative, decl_jobset.inputs.is_empty()) {
      repo::jobset_inputs::sync_for_jobset(
        pool,
        jobset.id,
        &decl_jobset.inputs,
      )
      .await?;
    }
  }

  if authoritative {
    let names = decl_project
      .jobsets
      .iter()
      .map(|jobset| jobset.name.as_str())
      .collect::<Vec<_>>();
    repo::jobsets::delete_except(pool, project.id, &names).await?;
  }

  let jobset_map = repo::jobsets::list_for_project(pool, project.id, 1000, 0)
    .await?
    .into_iter()
    .map(|jobset| (jobset.name, jobset.id))
    .collect::<HashMap<_, _>>();

  if should_reconcile(authoritative, decl_project.notifications.is_empty()) {
    repo::notification_configs::sync_for_project(
      pool,
      project.id,
      &decl_project.notifications,
    )
    .await?;
  }
  if should_reconcile(authoritative, decl_project.webhooks.is_empty()) {
    repo::webhook_configs::sync_for_project(
      pool,
      project.id,
      &decl_project.webhooks,
      resolve_webhook_secret,
      webhook_secret_encryption_key,
    )
    .await?;
  }
  if should_reconcile(authoritative, decl_project.channels.is_empty()) {
    repo::channels::sync_for_project(
      pool,
      project.id,
      &decl_project.channels,
      |name| jobset_map.get(name).copied(),
    )
    .await?;
  }
  Ok(())
}

async fn sync_declarative_projects(
  pool: &PgPool,
  config: &DeclarativeConfig,
  webhook_secret_encryption_key: Option<&str>,
) -> Result<()> {
  for project in &config.projects {
    let allow_runtime_mutation = project
      .allow_runtime_mutation
      .unwrap_or(config.allow_runtime_mutation);
    sync_declarative_project(
      pool,
      project,
      allow_runtime_mutation,
      webhook_secret_encryption_key,
    )
    .await?;
  }
  let names = config
    .projects
    .iter()
    .map(|project| project.name.as_str())
    .collect::<Vec<_>>();
  repo::projects::delete_declarative_except(
    pool,
    &names,
    config.allow_runtime_mutation,
  )
  .await?;
  Ok(())
}

async fn sync_project_members(
  pool: &PgPool,
  config: &DeclarativeConfig,
) -> Result<()> {
  let users = repo::users::list(pool, 10000, 0).await?;
  let user_map = users
    .into_iter()
    .map(|user| (user.username, user.id))
    .collect::<HashMap<_, _>>();
  for declaration in &config.projects {
    let authoritative = !declaration
      .allow_runtime_mutation
      .unwrap_or(config.allow_runtime_mutation);
    if !should_reconcile(authoritative, declaration.members.is_empty()) {
      continue;
    }
    let project = repo::projects::get_by_name(pool, &declaration.name).await?;
    repo::project_members::sync_for_project(
      pool,
      project.id,
      &declaration.members,
      |username| user_map.get(username).copied(),
    )
    .await?;
  }
  Ok(())
}

async fn sync_api_keys(
  pool: &PgPool,
  declarations: &[DeclarativeApiKey],
) -> Result<()> {
  for declaration in declarations {
    let key = resolve_secret(
      declaration.key.as_deref(),
      declaration.key_file.as_deref(),
      |file, error| {
        tracing::warn!(
          name = %declaration.name,
          file,
          "Failed to read API key file: {error}"
        );
      },
    );
    let Some(key) = key else {
      tracing::warn!(
        name = %declaration.name,
        "Declarative API key has no key or key_file set, skipping"
      );
      continue;
    };

    let mut hasher = Sha256::new();
    hasher.update(key.as_bytes());
    let key_hash = hex::encode(hasher.finalize());
    let api_key = repo::api_keys::upsert(
      pool,
      &declaration.name,
      &key_hash,
      declaration.role,
    )
    .await?;
    tracing::info!(
      name = %api_key.name,
      role = %api_key.role,
      "Upserted declarative API key"
    );
  }
  Ok(())
}

async fn sync_user(pool: &PgPool, declaration: &DeclarativeUser) -> Result<()> {
  let password = resolve_secret(
    declaration.password.as_deref(),
    declaration.password_file.as_deref(),
    |file, error| {
      tracing::warn!(
        username = %declaration.username,
        file,
        "Failed to read password file: {error}"
      );
    },
  );
  let existing =
    repo::users::get_by_username(pool, &declaration.username).await?;

  if let Some(user) = existing {
    let update = crate::models::UpdateUser {
      email: Some(declaration.email.clone()),
      full_name: declaration.full_name.clone(),
      password,
      role: Some(declaration.role),
      enabled: Some(declaration.enabled),
      public_dashboard: None,
    };
    if let Err(error) = repo::users::update(pool, user.id, &update, None).await
    {
      tracing::warn!(
        username = %declaration.username,
        "Failed to update declarative user: {error}"
      );
    } else {
      tracing::info!(
        username = %declaration.username,
        "Updated declarative user"
      );
    }
  } else if let Some(password) = password {
    let create = crate::models::CreateUser {
      username: declaration.username.clone(),
      email: declaration.email.clone(),
      full_name: declaration.full_name.clone(),
      password,
      role: Some(declaration.role),
    };
    match repo::users::create(pool, &create, None).await {
      Ok(user) => {
        tracing::info!(username = %user.username, "Created declarative user");
        if !declaration.enabled
          && let Err(error) =
            repo::users::set_enabled(pool, user.id, false).await
        {
          tracing::warn!(
            username = %user.username,
            "Failed to disable declarative user: {error}"
          );
        }
      },
      Err(error) => {
        tracing::warn!(
          username = %declaration.username,
          "Failed to create declarative user: {error}"
        );
      },
    }
  } else {
    tracing::warn!(
      username = %declaration.username,
      "Declarative user has no password set, skipping creation"
    );
  }
  Ok(())
}

async fn sync_users(
  pool: &PgPool,
  declarations: &[DeclarativeUser],
) -> Result<()> {
  for declaration in declarations {
    sync_user(pool, declaration).await?;
  }
  Ok(())
}

async fn notify_evaluator(pool: &PgPool) {
  if let Err(error) =
    crate::pg_notify::notify(pool, crate::pg_notify::CHANNEL_JOBSETS_CHANGED)
      .await
  {
    tracing::warn!("Failed to notify evaluator after bootstrap: {error}");
  }
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
  let n_projects = config.projects.len();
  let n_jobsets: usize = config.projects.iter().map(|p| p.jobsets.len()).sum();
  let n_keys = config.api_keys.len();
  let n_users = config.users.len();

  tracing::info!(
    projects = n_projects,
    jobsets = n_jobsets,
    api_keys = n_keys,
    users = n_users,
    "Bootstrapping declarative configuration"
  );

  sync_declarative_projects(pool, config, webhook_secret_encryption_key)
    .await?;

  sync_api_keys(pool, &config.api_keys).await?;
  sync_users(pool, &config.users).await?;
  sync_project_members(pool, config).await?;
  notify_evaluator(pool).await;

  tracing::info!("Declarative bootstrap complete");
  Ok(())
}
