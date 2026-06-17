use circus_types::validation::{
  validate_binary_cache_upstream,
  validate_cache_url,
};
use color_eyre::eyre::{self, WrapErr, bail};

use crate::{Config, DatabaseConfig};

fn validate_shared(result: Result<(), String>) -> eyre::Result<()> {
  result.map_err(|error| eyre::eyre!(error))
}

impl DatabaseConfig {
  /// Validate database configuration.
  ///
  /// # Errors
  ///
  /// Returns error if configuration is invalid.
  pub fn validate(&self) -> eyre::Result<()> {
    if self.url.is_empty() {
      bail!("Database URL cannot be empty");
    }

    if !self.url.starts_with("postgresql://")
      && !self.url.starts_with("postgres://")
    {
      bail!("Database URL must start with postgresql:// or postgres://");
    }

    if self.max_connections == 0 {
      bail!("Max database connections must be greater than 0");
    }

    if self.min_connections > self.max_connections {
      bail!("Min database connections cannot exceed max connections");
    }

    Ok(())
  }
}

impl Config {
  /// Validate all configuration sections.
  ///
  /// # Errors
  ///
  /// Returns error if any configuration section is invalid.
  pub fn validate(&self) -> eyre::Result<()> {
    // Validate database URL
    if self.database.url.is_empty() {
      bail!("Database URL cannot be empty");
    }

    if !self.database.url.starts_with("postgresql://")
      && !self.database.url.starts_with("postgres://")
    {
      bail!("Database URL must start with postgresql:// or postgres://");
    }

    // Validate connection pool settings
    if self.database.max_connections == 0 {
      bail!("Max database connections must be greater than 0");
    }

    if self.database.min_connections > self.database.max_connections {
      bail!("Min database connections cannot exceed max connections");
    }

    // Validate server settings
    if self.server.port == 0 {
      bail!("Server port must be greater than 0");
    }

    // Validate evaluator settings
    if self.evaluator.poll_interval == 0 {
      bail!("Evaluator poll interval must be greater than 0");
    }

    if let Some(url) = self.cache.cache_url.as_deref() {
      validate_shared(validate_cache_url(url, "cache.cache_url"))?;
    }
    for (idx, upstream) in self.cache.upstreams.iter().enumerate() {
      validate_shared(validate_binary_cache_upstream(
        upstream,
        &format!("cache.upstreams[{idx}]"),
      ))?;
    }
    for (project_idx, project) in self.declarative.projects.iter().enumerate() {
      if let Some(url) = project.cache_url.as_deref() {
        validate_shared(validate_cache_url(
          url,
          &format!("declarative.projects[{project_idx}].cache_url"),
        ))?;
      }
      for (upstream_idx, upstream) in project.cache_upstreams.iter().enumerate()
      {
        validate_shared(validate_binary_cache_upstream(
          upstream,
          &format!(
            "declarative.projects[{project_idx}].\
             cache_upstreams[{upstream_idx}]"
          ),
        ))?;
      }
    }

    // Validate queue runner settings
    if let Some(t) = self.queue_runner.psi_threshold
      && !(0.0..=100.0).contains(&t)
    {
      bail!("queue_runner.psi_threshold must be in [0.0, 100.0], got {t}");
    }
    if self.queue_runner.psi_check_timeout == 0 {
      bail!("queue_runner.psi_check_timeout must be greater than 0 seconds");
    }
    if let Some(rpc) = self.queue_runner.rpc.as_ref() {
      if rpc.max_connections == 0 {
        bail!("queue_runner.rpc.max_connections must be greater than 0");
      }
      if rpc.heartbeat_ttl_secs == 0 {
        bail!("queue_runner.rpc.heartbeat_ttl_secs must be greater than 0");
      }
      if rpc.presign_expiry_secs == 0 {
        bail!("queue_runner.rpc.presign_expiry_secs must be greater than 0");
      }
      for (idx, token_hash) in rpc.auth_tokens.iter().enumerate() {
        let decoded = hex::decode(token_hash).wrap_err_with(|| {
          format!("queue_runner.rpc.auth_tokens[{idx}] must be SHA-256 hex")
        })?;
        if decoded.len() != 32 {
          bail!(
            "queue_runner.rpc.auth_tokens[{idx}] must decode to 32 bytes, got \
             {}",
            decoded.len()
          );
        }
      }
      if let Some(oidc) = rpc.oidc.as_ref() {
        if !oidc.issuer.starts_with("https://") {
          bail!("queue_runner.rpc.oidc.issuer must be an https URL");
        }
        if oidc.audiences.is_empty() {
          bail!(
            "queue_runner.rpc.oidc.audiences must list at least one audience"
          );
        }
        if oidc.allowed_repositories.is_empty() {
          bail!(
            "queue_runner.rpc.oidc.allowed_repositories must list at least \
             one repository"
          );
        }
      }
      if rpc.tls.is_none()
        && (rpc.oidc.is_some() || !rpc.auth_tokens.is_empty())
      {
        if !rpc.allow_plaintext {
          bail!(
            "queue_runner.rpc.tls is required when auth_tokens or oidc are \
             set. Set queue_runner.rpc.allow_plaintext = true to accept \
             credentials over plain TCP on a trusted network."
          );
        }
        tracing::warn!(
          "queue_runner.rpc accepts credentials over plain TCP \
           (allow_plaintext = true)"
        );
      }
    }
    for (idx, pool) in self.queue_runner.ephemeral_pools.iter().enumerate() {
      if self
        .queue_runner
        .rpc
        .as_ref()
        .is_none_or(|rpc| rpc.oidc.is_none())
      {
        bail!(
          "queue_runner.ephemeral_pools[{idx}] requires queue_runner.rpc.oidc"
        );
      }
      let gha = &pool.github_actions;
      if pool.name.trim().is_empty() {
        bail!("queue_runner.ephemeral_pools[{idx}].name cannot be empty");
      }
      if pool.allowed_build_repositories.is_empty() {
        bail!(
          "queue_runner.ephemeral_pools[{idx}].allowed_build_repositories \
           must list at least one repository"
        );
      }
      for (repo_idx, repo) in pool.allowed_build_repositories.iter().enumerate()
      {
        if repo.split_once('/').is_none() {
          bail!(
            "queue_runner.ephemeral_pools[{idx}].\
             allowed_build_repositories[{repo_idx}] must be owner/repo"
          );
        }
      }
      if gha.workflow_repository.split_once('/').is_none() {
        bail!(
          "queue_runner.ephemeral_pools[{idx}].github_actions.\
           workflow_repository must be owner/repo"
        );
      }
      if gha.workflow.trim().is_empty() {
        bail!(
          "queue_runner.ephemeral_pools[{idx}].github_actions.workflow cannot \
           be empty"
        );
      }
      if gha.ref_name.trim().is_empty() {
        bail!(
          "queue_runner.ephemeral_pools[{idx}].github_actions.ref_name cannot \
           be empty"
        );
      }
      if gha.token.is_none() && gha.token_file.is_none() {
        bail!(
          "queue_runner.ephemeral_pools[{idx}].github_actions requires token \
           or token_file"
        );
      }
      if gha.runner_url.trim().is_empty() {
        bail!(
          "queue_runner.ephemeral_pools[{idx}].github_actions.runner_url \
           cannot be empty"
        );
      }
      if !gha.runner_url.starts_with("circus+tls://") {
        bail!(
          "queue_runner.ephemeral_pools[{idx}].github_actions.runner_url must \
           use circus+tls://. The dispatched agent sends its OIDC token over \
           the internet and must not use plaintext."
        );
      }
      if gha.oidc_audience.trim().is_empty() {
        bail!(
          "queue_runner.ephemeral_pools[{idx}].github_actions.oidc_audience \
           cannot be empty"
        );
      }
      if gha.agent_binary_url.trim().is_empty() {
        bail!(
          "queue_runner.ephemeral_pools[{idx}].github_actions.\
           agent_binary_url cannot be empty"
        );
      }
      if let Some(rpc) = self.queue_runner.rpc.as_ref()
        && let Some(oidc) = rpc.oidc.as_ref()
        && !oidc.audiences.iter().any(|aud| aud == &gha.oidc_audience)
      {
        bail!(
          "queue_runner.ephemeral_pools[{idx}].github_actions.oidc_audience \
           must be listed in queue_runner.rpc.oidc.audiences"
        );
      }
      if let Some(rpc) = self.queue_runner.rpc.as_ref()
        && let Some(oidc) = rpc.oidc.as_ref()
        && !oidc
          .allowed_repositories
          .iter()
          .any(|repo| repo == &gha.workflow_repository)
      {
        bail!(
          "queue_runner.ephemeral_pools[{idx}].github_actions.\
           workflow_repository must be listed in \
           queue_runner.rpc.oidc.allowed_repositories"
        );
      }
      if let Some(rpc) = self.queue_runner.rpc.as_ref()
        && let Some(oidc) = rpc.oidc.as_ref()
        && oidc.allowed_subjects.is_empty()
        && oidc.allowed_subject_prefixes.is_empty()
        && oidc.allowed_workflow_refs.is_empty()
      {
        bail!(
          "queue_runner.ephemeral_pools[{idx}] requires at least one OIDC \
           subject or workflow_ref restriction"
        );
      }
      if self
        .queue_runner
        .rpc
        .as_ref()
        .and_then(|rpc| rpc.cache_substituter.as_ref())
        .is_none()
      {
        bail!(
          "queue_runner.ephemeral_pools[{idx}] requires \
           queue_runner.rpc.cache_substituter so fresh CI agents can realise \
           assigned derivations"
        );
      }
      if self
        .queue_runner
        .rpc
        .as_ref()
        .and_then(|rpc| rpc.cache_public_key.as_ref())
        .is_none()
      {
        bail!(
          "queue_runner.ephemeral_pools[{idx}] requires \
           queue_runner.rpc.cache_public_key for the derivation substituter"
        );
      }
      if pool.systems.is_empty() {
        bail!(
          "queue_runner.ephemeral_pools[{idx}].systems must list at least one \
           system"
        );
      }
      if pool.max_jobs == 0 {
        bail!(
          "queue_runner.ephemeral_pools[{idx}].max_jobs must be greater than 0"
        );
      }
      if pool.speed_factor <= 0.0 {
        bail!(
          "queue_runner.ephemeral_pools[{idx}].speed_factor must be greater \
           than 0"
        );
      }
      if pool.max_inflight == 0 {
        bail!(
          "queue_runner.ephemeral_pools[{idx}].max_inflight must be greater \
           than 0"
        );
      }
      if pool.inflight_ttl_secs == 0 || pool.poll_interval_secs == 0 {
        bail!(
          "queue_runner.ephemeral_pools[{idx}] inflight_ttl_secs and \
           poll_interval_secs must be greater than 0"
        );
      }
    }
    if !matches!(
      self.cache_upload.compression.as_str(),
      "zstd" | "xz" | "gzip" | "none"
    ) {
      bail!(
        "cache_upload.compression must be one of zstd, xz, gzip, none; got {}",
        self.cache_upload.compression
      );
    }

    // Validate LDAP settings
    if let Some(ldap) = self.server.ldap.as_ref() {
      if ldap.url.is_empty() {
        bail!("server.ldap.url cannot be empty");
      }
      if ldap.base_dn.is_empty() {
        bail!("server.ldap.base_dn cannot be empty");
      }
      if ldap.bind_dn_template.is_empty() {
        bail!("server.ldap.bind_dn_template cannot be empty");
      }
      if !ldap.bind_dn_template.contains("{username}") {
        bail!(
          "server.ldap.bind_dn_template must contain the literal \
           '{{username}}' placeholder"
        );
      }
    }

    // Validate GC config
    if self.gc.enabled && self.gc.gc_roots_dir.as_os_str().is_empty() {
      bail!("GC roots directory cannot be empty when GC is enabled");
    }

    // Validate log config
    if self.logs.log_dir.as_os_str().is_empty() {
      bail!("Log directory cannot be empty");
    }

    // OAuth: when GitHub OAuth is configured, a client secret must be
    // available (inline or via file).
    if let Some(ref github) = self.oauth.github
      && github.client_secret.is_empty()
      && github.client_secret_file.is_none()
    {
      bail!("oauth.github requires client_secret or client_secret_file");
    }

    if let Some(ref slack) = self.notifications.slack
      && slack.webhook_url.is_empty()
      && slack.webhook_url_file.is_none()
    {
      bail!("notifications.slack requires webhook_url or webhook_url_file");
    }

    Ok(())
  }
}
