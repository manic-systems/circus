use std::{
  collections::VecDeque,
  sync::Arc,
  time::{Duration, Instant},
};

use circus_common::{config::GhaConfig, models::Build};
use color_eyre::eyre::{Context as _, bail};
use serde::Serialize;
use sqlx::PgPool;
use tokio_util::sync::CancellationToken;

use crate::{
  dispatch::{supports_required_features, trusted_build_github_repository},
  rpc::AgentPool,
};

const GITHUB_API_VERSION: &str = "2026-03-10";

pub struct Autoscaler {
  cfg:            GhaConfig,
  token:          String,
  http:           reqwest::Client,
  pool:           PgPool,
  agent_pool:     Arc<AgentPool>,
  inflight:       VecDeque<InflightLaunch>,
  last_scale_up:  Option<Instant>,
  shutdown_token: CancellationToken,
}

#[derive(Debug)]
struct InflightLaunch {
  launched_at: Instant,
}

#[derive(Serialize)]
struct WorkflowDispatch<'a> {
  #[serde(rename = "ref")]
  ref_name: &'a str,
  inputs:   WorkflowInputs<'a>,
}

#[derive(Serialize)]
struct WorkflowInputs<'a> {
  runner_url:         String,
  oidc_audience:      &'a str,
  systems:            String,
  supported_features: String,
  mandatory_features: String,
  max_jobs:           String,
  cores:              String,
  speed_factor:       String,
  agent_name:         String,
}

#[derive(Debug)]
struct Demand {
  eligible_pending: usize,
  live_capacity:    u32,
}

impl Autoscaler {
  /// # Errors
  ///
  /// Returns an error if the GitHub token cannot be loaded or the HTTP client
  /// cannot be constructed.
  pub async fn new(
    cfg: GhaConfig,
    pool: PgPool,
    agent_pool: Arc<AgentPool>,
    shutdown_token: CancellationToken,
  ) -> color_eyre::Result<Self> {
    let token = load_token(&cfg).await?;
    let http = reqwest::Client::builder()
      .user_agent("circus-queue-runner")
      .build()
      .context("build GitHub Actions autoscaler HTTP client")?;
    Ok(Self {
      cfg,
      token,
      http,
      pool,
      agent_pool,
      inflight: VecDeque::new(),
      last_scale_up: None,
      shutdown_token,
    })
  }

  pub async fn run(mut self) {
    tracing::info!(
      repository = %self.cfg.repository,
      workflow = %self.cfg.workflow,
      ref_name = %self.cfg.ref_name,
      systems = ?self.cfg.systems,
      "GitHub Actions autoscaler started"
    );
    let mut interval =
      tokio::time::interval(Duration::from_secs(self.cfg.poll_interval_secs));
    loop {
      tokio::select! {
        () = self.shutdown_token.cancelled() => {
          tracing::info!("GitHub Actions autoscaler stopped");
          return;
        }
        _ = interval.tick() => {
          if let Err(e) = self.tick().await {
            tracing::warn!("GitHub Actions autoscaler tick failed: {e}");
          }
        }
      }
    }
  }

  async fn tick(&mut self) -> color_eyre::Result<()> {
    self.prune_inflight();
    if self.in_cooldown() {
      return Ok(());
    }
    let demand = self.demand().await?;
    let pending = demand.eligible_pending as u32;
    let available = demand
      .live_capacity
      .saturating_add(self.inflight.len() as u32);
    let deficit = pending.saturating_sub(available);
    let remaining_inflight = self
      .cfg
      .max_inflight
      .saturating_sub(self.inflight.len() as u32);
    let launches = deficit.min(remaining_inflight);
    if launches == 0 {
      return Ok(());
    }

    for _ in 0..launches {
      self.dispatch_workflow().await?;
      self.inflight.push_back(InflightLaunch {
        launched_at: Instant::now(),
      });
      self.last_scale_up = Some(Instant::now());
    }
    Ok(())
  }

  fn prune_inflight(&mut self) {
    let ttl = Duration::from_secs(self.cfg.inflight_ttl_secs);
    while self
      .inflight
      .front()
      .is_some_and(|launch| launch.launched_at.elapsed() >= ttl)
    {
      self.inflight.pop_front();
    }
  }

  fn in_cooldown(&self) -> bool {
    self.last_scale_up.is_some_and(|last| {
      last.elapsed() < Duration::from_secs(self.cfg.scale_up_cooldown_secs)
    })
  }

  async fn demand(&self) -> color_eyre::Result<Demand> {
    let builds = pending_builds_for_systems(&self.pool, &self.cfg.systems)
      .await
      .context("load pending builds for GHA autoscaler")?;
    let mut eligible_pending = 0usize;
    for build in builds {
      if !supports_required_features(
        build.scheduling_features(),
        &self.cfg.supported_features,
        &self.cfg.mandatory_features,
      ) {
        continue;
      }
      let trusted_repo =
        trusted_build_github_repository(&self.pool, &build).await;
      if trusted_repo.as_ref().and_then(|repo| repo.as_deref())
        == Some(self.cfg.repository.as_str())
      {
        eligible_pending += 1;
      }
    }

    let live_capacity = self
      .agent_pool
      .snapshot_all()
      .into_iter()
      .filter(|agent| {
        agent.ephemeral
          && agent.auth_kind == "oidc"
          && agent.oidc_repository.as_deref()
            == Some(self.cfg.repository.as_str())
          && self
            .cfg
            .systems
            .iter()
            .any(|system| agent.systems.iter().any(|s| s == system))
          && supports_required_features(
            &self.cfg.mandatory_features,
            &agent.supported_features,
            &agent.mandatory_features,
          )
      })
      .map(|agent| agent.max_jobs.saturating_sub(agent.current_jobs))
      .sum();

    Ok(Demand {
      eligible_pending,
      live_capacity,
    })
  }

  async fn dispatch_workflow(&self) -> color_eyre::Result<()> {
    let (owner, repo) =
      self.cfg.repository.split_once('/').ok_or_else(|| {
        color_eyre::eyre::eyre!("repository must be owner/repo")
      })?;
    let workflow = urlencoding::encode(&self.cfg.workflow);
    let url = format!(
      "https://api.github.com/repos/{owner}/{repo}/actions/workflows/{workflow}/dispatches"
    );
    let body = WorkflowDispatch {
      ref_name: &self.cfg.ref_name,
      inputs:   WorkflowInputs {
        runner_url:         json_string(&self.cfg.runner_url)?,
        oidc_audience:      &self.cfg.oidc_audience,
        systems:            json_string_array(&self.cfg.systems)?,
        supported_features: json_string_array(&self.cfg.supported_features)?,
        mandatory_features: json_string_array(&self.cfg.mandatory_features)?,
        max_jobs:           self.cfg.max_jobs.to_string(),
        cores:              self.cfg.cores.to_string(),
        speed_factor:       self.cfg.speed_factor.to_string(),
        agent_name:         json_string(&self.cfg.agent_name)?,
      },
    };
    let response = self
      .http
      .post(url)
      .header(reqwest::header::ACCEPT, "application/vnd.github+json")
      .header("X-GitHub-Api-Version", GITHUB_API_VERSION)
      .bearer_auth(&self.token)
      .json(&body)
      .send()
      .await
      .context("dispatch GitHub Actions builder workflow")?;
    let status = response.status();
    if status != reqwest::StatusCode::NO_CONTENT {
      let text = response.text().await.unwrap_or_default();
      bail!("GitHub workflow dispatch returned {status}: {text}");
    }
    tracing::info!(
      repository = %self.cfg.repository,
      workflow = %self.cfg.workflow,
      ref_name = %self.cfg.ref_name,
      "dispatched GitHub Actions builder workflow"
    );
    Ok(())
  }
}

fn json_string(value: &str) -> color_eyre::Result<String> {
  serde_json::to_string(value).context("encode JSON string literal")
}

fn json_string_array(values: &[String]) -> color_eyre::Result<String> {
  serde_json::to_string(values).context("encode JSON string array")
}

async fn load_token(cfg: &GhaConfig) -> color_eyre::Result<String> {
  if let Some(token) = cfg.token.as_deref()
    && !token.trim().is_empty()
  {
    return Ok(token.trim().to_owned());
  }
  let Some(path) = cfg.token_file.as_ref() else {
    bail!("queue_runner.gha requires token or token_file");
  };
  let token = tokio::fs::read_to_string(path)
    .await
    .with_context(|| format!("read GitHub token {}", path.display()))?;
  let token = token.trim();
  if token.is_empty() {
    bail!("GitHub token file {} is empty", path.display());
  }
  Ok(token.to_owned())
}

async fn pending_builds_for_systems(
  pool: &PgPool,
  systems: &[String],
) -> circus_common::error::Result<Vec<Build>> {
  sqlx::query_as::<_, Build>(
    "SELECT * FROM builds WHERE status = 'pending' AND system = ANY($1) ORDER \
     BY priority DESC, created_at ASC LIMIT 512",
  )
  .bind(systems)
  .fetch_all(pool)
  .await
  .map_err(circus_common::CiError::Database)
}

#[cfg(test)]
mod tests {
  use super::*;

  fn test_http_client() -> reqwest::Client {
    let _ = rustls::crypto::ring::default_provider().install_default();
    reqwest::Client::new()
  }

  #[tokio::test]
  async fn prunes_expired_inflight_launches() {
    let mut autoscaler = Autoscaler {
      cfg:            GhaConfig {
        inflight_ttl_secs: 1,
        ..GhaConfig::default()
      },
      token:          "token".into(),
      http:           test_http_client(),
      pool:           PgPool::connect_lazy("postgres://localhost/circus")
        .expect("lazy pool should construct"),
      agent_pool:     AgentPool::new(),
      inflight:       VecDeque::from([InflightLaunch {
        launched_at: Instant::now()
          .checked_sub(Duration::from_secs(2))
          .expect("two seconds before now should be representable"),
      }]),
      last_scale_up:  None,
      shutdown_token: CancellationToken::new(),
    };
    autoscaler.prune_inflight();
    assert!(autoscaler.inflight.is_empty());
  }

  #[tokio::test]
  async fn cooldown_blocks_immediate_second_scale_up() {
    let autoscaler = Autoscaler {
      cfg:            GhaConfig {
        scale_up_cooldown_secs: 30,
        ..GhaConfig::default()
      },
      token:          "token".into(),
      http:           test_http_client(),
      pool:           PgPool::connect_lazy("postgres://localhost/circus")
        .expect("lazy pool should construct"),
      agent_pool:     AgentPool::new(),
      inflight:       VecDeque::new(),
      last_scale_up:  Some(Instant::now()),
      shutdown_token: CancellationToken::new(),
    };
    assert!(autoscaler.in_cooldown());
  }

  #[test]
  fn workflow_inputs_are_json_literals() {
    assert_eq!(
      json_string("circus gha").expect("string literal should encode"),
      "\"circus gha\""
    );
    assert_eq!(
      json_string_array(&["x86_64-linux".into(), "kvm".into()])
        .expect("array literal should encode"),
      "[\"x86_64-linux\",\"kvm\"]"
    );
  }
}
