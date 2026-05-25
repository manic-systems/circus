//! Notification dispatch for build events

use std::{
  sync::OnceLock,
  time::{SystemTime, UNIX_EPOCH},
};

use sqlx::PgPool;
use tracing::{error, info, warn};

use crate::{
  config::{EmailConfig, NotificationsConfig},
  models::{Build, BuildStatus, Project},
  repo,
};

/// Shared HTTP client for all notification dispatches.
/// Avoids recreating connection pools on every build completion.
fn http_client() -> &'static reqwest::Client {
  static CLIENT: OnceLock<reqwest::Client> = OnceLock::new();
  CLIENT.get_or_init(reqwest::Client::new)
}

#[derive(Debug, Clone, Copy)]
pub struct RateLimitState {
  pub limit:     u64,
  pub remaining: u64,
  pub reset_at:  u64,
}

#[must_use]
pub fn extract_rate_limit_from_headers(
  headers: &reqwest::header::HeaderMap,
) -> Option<RateLimitState> {
  let limit = headers
    .get("X-RateLimit-Limit")?
    .to_str()
    .ok()?
    .parse()
    .ok()?;
  let remaining = headers
    .get("X-RateLimit-Remaining")?
    .to_str()
    .ok()?
    .parse()
    .ok()?;
  let reset_at = headers
    .get("X-RateLimit-Reset")?
    .to_str()
    .ok()?
    .parse()
    .ok()?;
  Some(RateLimitState {
    limit,
    remaining,
    reset_at,
  })
}

#[must_use]
pub fn calculate_delay(state: &RateLimitState, now: u64) -> u64 {
  let seconds_until_reset = state.reset_at.saturating_sub(now).max(1);
  let consumed = state.limit.saturating_sub(state.remaining);
  let delay = (consumed * 5) / seconds_until_reset;
  delay.max(1)
}

/// Dispatch all configured notifications for a completed build.
/// If retry queue is enabled, enqueues tasks; otherwise sends immediately.
pub async fn dispatch_build_finished(
  pool: Option<&PgPool>,
  build: &Build,
  project: &Project,
  commit_hash: &str,
  config: &NotificationsConfig,
) {
  // If retry queue is enabled and pool is available, enqueue tasks
  if config.enable_retry_queue
    && let Some(pool) = pool
  {
    enqueue_notifications(pool, build, project, commit_hash, config).await;
    return;
  }

  // Otherwise, send immediately (legacy fire-and-forget behavior)
  send_notifications_immediate(build, project, commit_hash, config).await;
}

/// Enqueue notification tasks for reliable delivery with retry
async fn enqueue_notifications(
  pool: &PgPool,
  build: &Build,
  project: &Project,
  commit_hash: &str,
  config: &NotificationsConfig,
) {
  let max_attempts = config.max_retry_attempts;

  // 1. Generic webhook notification
  if let Some(ref url) = config.webhook_url {
    let payload = serde_json::json!({
      "type": "webhook",
      "url": url,
      "build_id": build.id,
      "build_status": build.status,
      "build_job": build.job_name,
      "build_drv": build.drv_path,
      "build_output": build.build_output_path,
      "project_name": project.name,
      "project_url": project.repository_url,
      "commit_hash": commit_hash,
    });

    if let Err(e) =
      repo::notification_tasks::create(pool, "webhook", payload, max_attempts)
        .await
    {
      error!(build_id = %build.id, "Failed to enqueue webhook notification: {e}");
    }
  }

  // 2. GitHub commit status
  if let Some(ref token) = config.github_token
    && project.repository_url.contains("github.com")
  {
    let payload = serde_json::json!({
      "type": "github_status",
      "token": token,
      "repository_url": project.repository_url,
      "commit_hash": commit_hash,
      "build_id": build.id,
      "build_status": build.status,
      "build_job": build.job_name,
    });

    if let Err(e) = repo::notification_tasks::create(
      pool,
      "github_status",
      payload,
      max_attempts,
    )
    .await
    {
      error!(build_id = %build.id, "Failed to enqueue GitHub status notification: {e}");
    }
  }

  // 3. Gitea/Forgejo commit status
  if let (Some(url), Some(token)) = (&config.gitea_url, &config.gitea_token) {
    let payload = serde_json::json!({
      "type": "gitea_status",
      "base_url": url,
      "token": token,
      "repository_url": project.repository_url,
      "commit_hash": commit_hash,
      "build_id": build.id,
      "build_status": build.status,
      "build_job": build.job_name,
    });

    if let Err(e) = repo::notification_tasks::create(
      pool,
      "gitea_status",
      payload,
      max_attempts,
    )
    .await
    {
      error!(build_id = %build.id, "Failed to enqueue Gitea status notification: {e}");
    }
  }

  // 4. GitLab commit status
  if let (Some(url), Some(token)) = (&config.gitlab_url, &config.gitlab_token) {
    let payload = serde_json::json!({
      "type": "gitlab_status",
      "base_url": url,
      "token": token,
      "repository_url": project.repository_url,
      "commit_hash": commit_hash,
      "build_id": build.id,
      "build_status": build.status,
      "build_job": build.job_name,
    });

    if let Err(e) = repo::notification_tasks::create(
      pool,
      "gitlab_status",
      payload,
      max_attempts,
    )
    .await
    {
      error!(build_id = %build.id, "Failed to enqueue GitLab status notification: {e}");
    }
  }

  // 5. Slack notification
  let is_failure = !build.status.is_success();
  if let Some(ref slack_config) = config.slack
    && (!slack_config.on_failure_only || is_failure)
  {
    let payload = serde_json::json!({
      "type": "slack",
      "webhook_url": slack_config.webhook_url,
      "build_id": build.id,
      "build_status": build.status,
      "build_job": build.job_name,
      "project_name": project.name,
      "commit_hash": commit_hash,
    });

    if let Err(e) =
      repo::notification_tasks::create(pool, "slack", payload, max_attempts)
        .await
    {
      error!(build_id = %build.id, "Failed to enqueue Slack notification: {e}");
    }
  }

  // 6. Email notification
  if let Some(ref email_config) = config.email
    && (!email_config.on_failure_only || is_failure)
  {
    let payload = serde_json::json!({
      "type": "email",
      "config": email_config,
      "build_id": build.id,
      "build_status": build.status,
      "build_job": build.job_name,
      "build_drv": build.drv_path,
      "build_output": build.build_output_path,
      "project_name": project.name,
    });

    if let Err(e) =
      repo::notification_tasks::create(pool, "email", payload, max_attempts)
        .await
    {
      error!(build_id = %build.id, "Failed to enqueue email notification: {e}");
    }
  }
}

/// Enqueue commit status notifications for GitHub/GitLab/Gitea/Forgejo.
///
/// # Errors
///
/// Logs database errors if task creation fails.
async fn enqueue_commit_status_notification(
  pool: &PgPool,
  build: &Build,
  project: &Project,
  commit_hash: &str,
  config: &NotificationsConfig,
) {
  let max_attempts = config.max_retry_attempts;

  // GitHub commit status
  if let Some(ref token) = config.github_token
    && project.repository_url.contains("github.com")
  {
    let payload = serde_json::json!({
      "type": "github_status",
      "token": token,
      "repository_url": project.repository_url,
      "commit_hash": commit_hash,
      "build_id": build.id,
      "build_status": build.status,
      "build_job": build.job_name,
    });

    if let Err(e) = repo::notification_tasks::create(
      pool,
      "github_status",
      payload,
      max_attempts,
    )
    .await
    {
      error!(build_id = %build.id, "Failed to enqueue GitHub status notification: {e}");
    }
  }

  // Gitea/Forgejo commit status
  if let (Some(url), Some(token)) = (&config.gitea_url, &config.gitea_token) {
    let payload = serde_json::json!({
      "type": "gitea_status",
      "base_url": url,
      "token": token,
      "repository_url": project.repository_url,
      "commit_hash": commit_hash,
      "build_id": build.id,
      "build_status": build.status,
      "build_job": build.job_name,
    });

    if let Err(e) = repo::notification_tasks::create(
      pool,
      "gitea_status",
      payload,
      max_attempts,
    )
    .await
    {
      error!(build_id = %build.id, "Failed to enqueue Gitea status notification: {e}");
    }
  }

  // GitLab commit status
  if let (Some(url), Some(token)) = (&config.gitlab_url, &config.gitlab_token) {
    let payload = serde_json::json!({
      "type": "gitlab_status",
      "base_url": url,
      "token": token,
      "repository_url": project.repository_url,
      "commit_hash": commit_hash,
      "build_id": build.id,
      "build_status": build.status,
      "build_job": build.job_name,
    });

    if let Err(e) = repo::notification_tasks::create(
      pool,
      "gitlab_status",
      payload,
      max_attempts,
    )
    .await
    {
      error!(build_id = %build.id, "Failed to enqueue GitLab status notification: {e}");
    }
  }
}

/// Dispatch commit status notification when a build is created (pending state).
///
/// # Errors
///
/// Logs database errors if task creation fails.
pub async fn dispatch_build_created(
  pool: &PgPool,
  build: &Build,
  project: &Project,
  commit_hash: &str,
  config: &NotificationsConfig,
) {
  if !config.enable_retry_queue {
    return;
  }

  enqueue_commit_status_notification(pool, build, project, commit_hash, config)
    .await;
  info!(
    build_id = %build.id,
    job = %build.job_name,
    status = %build.status,
    "Enqueued commit status notification for build creation"
  );
}

/// Dispatch commit status notification when a build starts (running state).
///
/// # Errors
///
/// Logs database errors if task creation fails.
pub async fn dispatch_build_started(
  pool: &PgPool,
  build: &Build,
  project: &Project,
  commit_hash: &str,
  config: &NotificationsConfig,
) {
  if !config.enable_retry_queue {
    return;
  }

  enqueue_commit_status_notification(pool, build, project, commit_hash, config)
    .await;
  info!(
    build_id = %build.id,
    job = %build.job_name,
    status = %build.status,
    "Enqueued commit status notification for build start"
  );
}

/// Send notifications immediately.
/// This is the "legacy" fire-and-forget behavior.
async fn send_notifications_immediate(
  build: &Build,
  project: &Project,
  commit_hash: &str,
  config: &NotificationsConfig,
) {
  // 1. Generic webhook notification
  if let Some(ref url) = config.webhook_url {
    webhook_notification(url, build, project, commit_hash).await;
  }

  // 2. GitHub commit status
  if let Some(ref token) = config.github_token
    && project.repository_url.contains("github.com")
  {
    set_github_status(token, &project.repository_url, commit_hash, build).await;
  }

  // 3. Gitea/Forgejo commit status
  if let (Some(url), Some(token)) = (&config.gitea_url, &config.gitea_token) {
    set_gitea_status(url, token, &project.repository_url, commit_hash, build)
      .await;
  }

  // 4. GitLab commit status
  if let (Some(url), Some(token)) = (&config.gitlab_url, &config.gitlab_token) {
    set_gitlab_status(url, token, &project.repository_url, commit_hash, build)
      .await;
  }

  // 5. Email notification
  let is_failure = !build.status.is_success();
  if let Some(ref email_config) = config.email
    && (!email_config.on_failure_only || is_failure)
  {
    send_email_notification(email_config, build, project).await;
  }
}

async fn webhook_notification(
  url: &str,
  build: &Build,
  project: &Project,
  commit_hash: &str,
) {
  let status_str = match build.status {
    BuildStatus::Succeeded | BuildStatus::CachedFailure => "success",
    BuildStatus::Failed
    | BuildStatus::DependencyFailed
    | BuildStatus::FailedWithOutput
    | BuildStatus::Timeout
    | BuildStatus::LogLimitExceeded
    | BuildStatus::NarSizeLimitExceeded
    | BuildStatus::NonDeterministic => "failure",
    BuildStatus::Cancelled => "cancelled",
    BuildStatus::Aborted => "aborted",
    BuildStatus::UnsupportedSystem => "skipped",
    BuildStatus::Pending | BuildStatus::Running => "pending",
  };

  let payload = serde_json::json!({
    "build_id":     build.id,
    "build_status": status_str,
    "build_job":    build.job_name,
    "build_drv":    build.drv_path,
    "build_output": build.build_output_path.as_deref().unwrap_or(""),
    "project_name": project.name,
    "project_url":  project.repository_url,
    "commit_hash":  commit_hash,
  });

  match http_client().post(url).json(&payload).send().await {
    Ok(resp) if resp.status().is_success() => {
      info!(build_id = %build.id, "Webhook notification sent");
    },
    Ok(resp) => {
      warn!(
        build_id = %build.id,
        status = %resp.status(),
        "Webhook notification rejected"
      );
    },
    Err(e) => error!(build_id = %build.id, "Webhook notification failed: {e}"),
  }
}

async fn set_github_status(
  token: &str,
  repo_url: &str,
  commit: &str,
  build: &Build,
) {
  // Parse owner/repo from URL
  let Some((owner, repo)) = parse_github_repo(repo_url) else {
    warn!("Cannot parse GitHub owner/repo from {repo_url}");
    return;
  };

  let (state, description) = match build.status {
    BuildStatus::Succeeded | BuildStatus::CachedFailure => {
      ("success", "Build succeeded")
    },
    BuildStatus::Failed
    | BuildStatus::DependencyFailed
    | BuildStatus::FailedWithOutput
    | BuildStatus::NonDeterministic => ("failure", "Build failed"),
    BuildStatus::Running => ("pending", "Build in progress"),
    BuildStatus::Pending => ("pending", "Build queued"),
    BuildStatus::Cancelled => ("error", "Build cancelled"),
    BuildStatus::Aborted => ("error", "Build aborted"),
    BuildStatus::Timeout => ("error", "Build timed out"),
    BuildStatus::UnsupportedSystem => ("error", "Unsupported system"),
    BuildStatus::LogLimitExceeded => ("error", "Log limit exceeded"),
    BuildStatus::NarSizeLimitExceeded => ("error", "NAR size limit exceeded"),
  };

  let url =
    format!("https://api.github.com/repos/{owner}/{repo}/statuses/{commit}");
  let body = serde_json::json!({
      "state": state,
      "description": description,
      "context": format!("fc/{}", build.job_name),
  });

  match http_client()
    .post(&url)
    .header("Authorization", format!("token {token}"))
    .header("User-Agent", "fc-ci")
    .header("Accept", "application/vnd.github+json")
    .json(&body)
    .send()
    .await
  {
    Ok(resp) => {
      let is_success = resp.status().is_success();
      let status = resp.status();

      // Extract rate limit state from response headers before consuming body
      let rate_limit = extract_rate_limit_from_headers(resp.headers());

      if is_success {
        info!(build_id = %build.id, "Set GitHub commit status: {state}");
      } else {
        let text = resp.text().await.unwrap_or_default();
        warn!("GitHub status API returned {status}: {text}");
      }

      // Handle rate limiting based on extracted state
      if let Some(rate_limit) = rate_limit {
        let now = SystemTime::now()
          .duration_since(UNIX_EPOCH)
          .unwrap()
          .as_secs();

        // Log when approaching limit (Hydra threshold: 2000)
        if rate_limit.remaining <= 2000 {
          let seconds_until_reset = rate_limit.reset_at.saturating_sub(now);
          info!(
            "GitHub rate limit: {}/{}, resets in {}s",
            rate_limit.remaining, rate_limit.limit, seconds_until_reset
          );
        }

        // Sleep when critical (Hydra threshold: 1000)
        if rate_limit.remaining <= 1000 {
          let delay = calculate_delay(&rate_limit, now);
          warn!(
            "GitHub rate limit critical: {}/{}, sleeping {}s",
            rate_limit.remaining, rate_limit.limit, delay
          );
          tokio::time::sleep(std::time::Duration::from_secs(delay)).await;
        }
      }
    },
    Err(e) => error!("GitHub status API request failed: {e}"),
  }
}

async fn set_gitea_status(
  base_url: &str,
  token: &str,
  repo_url: &str,
  commit: &str,
  build: &Build,
) {
  // Parse owner/repo from URL (try to extract from the gitea URL)
  let Some((owner, repo)) = parse_gitea_repo(repo_url, base_url) else {
    warn!("Cannot parse Gitea owner/repo from {repo_url}");
    return;
  };

  let (state, description) = match build.status {
    BuildStatus::Succeeded | BuildStatus::CachedFailure => {
      ("success", "Build succeeded")
    },
    BuildStatus::Failed
    | BuildStatus::DependencyFailed
    | BuildStatus::FailedWithOutput
    | BuildStatus::NonDeterministic => ("failure", "Build failed"),
    BuildStatus::Running => ("pending", "Build in progress"),
    BuildStatus::Pending => ("pending", "Build queued"),
    BuildStatus::Cancelled => ("error", "Build cancelled"),
    BuildStatus::Aborted => ("error", "Build aborted"),
    BuildStatus::Timeout => ("error", "Build timed out"),
    BuildStatus::UnsupportedSystem => ("error", "Unsupported system"),
    BuildStatus::LogLimitExceeded => ("error", "Log limit exceeded"),
    BuildStatus::NarSizeLimitExceeded => ("error", "NAR size limit exceeded"),
  };

  let url = format!("{base_url}/api/v1/repos/{owner}/{repo}/statuses/{commit}");
  let body = serde_json::json!({
      "state": state,
      "description": description,
      "context": format!("fc/{}", build.job_name),
  });

  match http_client()
    .post(&url)
    .header("Authorization", format!("token {token}"))
    .json(&body)
    .send()
    .await
  {
    Ok(resp) => {
      if resp.status().is_success() {
        info!(build_id = %build.id, "Set Gitea commit status: {state}");
      } else {
        let status = resp.status();
        let text = resp.text().await.unwrap_or_default();
        warn!("Gitea status API returned {status}: {text}");
      }
    },
    Err(e) => error!("Gitea status API request failed: {e}"),
  }
}

async fn set_gitlab_status(
  base_url: &str,
  token: &str,
  repo_url: &str,
  commit: &str,
  build: &Build,
) {
  // Parse project path from URL
  let Some(project_path) = parse_gitlab_project(repo_url, base_url) else {
    warn!("Cannot parse GitLab project from {repo_url}");
    return;
  };

  // GitLab uses different state names
  let (state, description) = match build.status {
    BuildStatus::Succeeded | BuildStatus::CachedFailure => {
      ("success", "Build succeeded")
    },
    BuildStatus::Failed
    | BuildStatus::DependencyFailed
    | BuildStatus::FailedWithOutput
    | BuildStatus::NonDeterministic => ("failed", "Build failed"),
    BuildStatus::Running => ("running", "Build in progress"),
    BuildStatus::Pending => ("pending", "Build queued"),
    BuildStatus::Cancelled => ("canceled", "Build cancelled"),
    BuildStatus::Aborted => ("canceled", "Build aborted"),
    BuildStatus::Timeout => ("failed", "Build timed out"),
    BuildStatus::UnsupportedSystem => ("skipped", "Unsupported system"),
    BuildStatus::LogLimitExceeded => ("failed", "Log limit exceeded"),
    BuildStatus::NarSizeLimitExceeded => ("failed", "NAR size limit exceeded"),
  };

  // URL-encode the project path for the API
  let encoded_project = urlencoding::encode(&project_path);
  let url = format!(
    "{}/api/v4/projects/{}/statuses/{}",
    base_url.trim_end_matches('/'),
    encoded_project,
    commit
  );

  let body = serde_json::json!({
      "state": state,
      "description": description,
      "name": format!("fc/{}", build.job_name),
  });

  match http_client()
    .post(&url)
    .header("PRIVATE-TOKEN", token)
    .json(&body)
    .send()
    .await
  {
    Ok(resp) => {
      if resp.status().is_success() {
        info!(build_id = %build.id, "Set GitLab commit status: {state}");
      } else {
        let status = resp.status();
        let text = resp.text().await.unwrap_or_default();
        warn!("GitLab status API returned {status}: {text}");
      }
    },
    Err(e) => error!("GitLab status API request failed: {e}"),
  }
}

fn parse_github_repo(url: &str) -> Option<(String, String)> {
  // Handle https://github.com/owner/repo.git or git@github.com:owner/repo.git
  let url = url.trim_end_matches(".git");
  if let Some(rest) = url.strip_prefix("https://github.com/") {
    let parts: Vec<&str> = rest.splitn(2, '/').collect();
    if parts.len() == 2 {
      return Some((parts[0].to_string(), parts[1].to_string()));
    }
  }
  if let Some(rest) = url.strip_prefix("git@github.com:") {
    let parts: Vec<&str> = rest.splitn(2, '/').collect();
    if parts.len() == 2 {
      return Some((parts[0].to_string(), parts[1].to_string()));
    }
  }
  None
}

fn parse_gitea_repo(
  repo_url: &str,
  base_url: &str,
) -> Option<(String, String)> {
  let url = repo_url.trim_end_matches(".git");
  let base = base_url.trim_end_matches('/');
  if let Some(rest) = url.strip_prefix(&format!("{base}/")) {
    let parts: Vec<&str> = rest.splitn(2, '/').collect();
    if parts.len() == 2 {
      return Some((parts[0].to_string(), parts[1].to_string()));
    }
  }
  None
}

fn parse_gitlab_project(repo_url: &str, base_url: &str) -> Option<String> {
  let url = repo_url.trim_end_matches(".git");
  let base = base_url.trim_end_matches('/');
  if let Some(rest) = url.strip_prefix(&format!("{base}/")) {
    return Some(rest.to_string());
  }
  // Also try without scheme match (e.g., https vs git@)
  if let (Some(at_pos), Some(colon_pos)) = (
    url.find('@'),
    url.find('@').and_then(|p| url[p..].find(':')),
  ) {
    let path = &url[at_pos + colon_pos + 1..];
    return Some(path.to_string());
  }
  None
}

async fn send_email_notification(
  config: &EmailConfig,
  build: &Build,
  project: &Project,
) {
  use lettre::{
    AsyncSmtpTransport,
    AsyncTransport,
    Message,
    Tokio1Executor,
    message::header::ContentType,
    transport::smtp::authentication::Credentials,
  };

  let status_str = match build.status {
    BuildStatus::Succeeded | BuildStatus::CachedFailure => "SUCCESS",
    BuildStatus::Failed
    | BuildStatus::DependencyFailed
    | BuildStatus::FailedWithOutput
    | BuildStatus::Timeout
    | BuildStatus::LogLimitExceeded
    | BuildStatus::NarSizeLimitExceeded
    | BuildStatus::NonDeterministic => "FAILURE",
    BuildStatus::Cancelled => "CANCELLED",
    BuildStatus::Aborted => "ABORTED",
    BuildStatus::UnsupportedSystem => "UNSUPPORTED",
    BuildStatus::Pending | BuildStatus::Running => "PENDING",
  };

  let subject = format!(
    "[FC] {} - {} ({})",
    status_str, build.job_name, project.name
  );

  let body = format!(
    "Build notification from FC CI\n\nProject: {}\nJob: {}\nStatus: \
     {}\nDerivation: {}\nOutput: {}\nBuild ID: {}\n",
    project.name,
    build.job_name,
    status_str,
    build.drv_path,
    build.build_output_path.as_deref().unwrap_or("N/A"),
    build.id,
  );

  for to_addr in &config.to_addresses {
    let email = match Message::builder()
      .from(match config.from_address.parse() {
        Ok(addr) => addr,
        Err(e) => {
          error!("Invalid from address '{}': {e}", config.from_address);
          return;
        },
      })
      .to(match to_addr.parse() {
        Ok(addr) => addr,
        Err(e) => {
          warn!("Invalid to address '{to_addr}': {e}");
          continue;
        },
      })
      .subject(&subject)
      .header(ContentType::TEXT_PLAIN)
      .body(body.clone())
    {
      Ok(e) => e,
      Err(e) => {
        error!("Failed to build email: {e}");
        continue;
      },
    };

    let mut mailer_builder = if config.tls {
      match AsyncSmtpTransport::<Tokio1Executor>::relay(&config.smtp_host) {
        Ok(b) => b.port(config.smtp_port),
        Err(e) => {
          error!("Failed to create SMTP transport: {e}");
          return;
        },
      }
    } else {
      AsyncSmtpTransport::<Tokio1Executor>::builder_dangerous(&config.smtp_host)
        .port(config.smtp_port)
    };

    if let (Some(user), Some(pass)) = (&config.smtp_user, &config.smtp_password)
    {
      mailer_builder = mailer_builder
        .credentials(Credentials::new(user.clone(), pass.clone()));
    }

    let mailer = mailer_builder.build();

    match mailer.send(email).await {
      Ok(_) => {
        info!(build_id = %build.id, to = to_addr, "Email notification sent");
      },
      Err(e) => {
        error!(build_id = %build.id, to = to_addr, "Failed to send email: {e}");
      },
    }
  }
}

/// Process a notification task from the retry queue
///
/// # Errors
///
/// Returns error if notification delivery fails.
pub async fn process_notification_task(
  task: &crate::models::NotificationTask,
) -> Result<(), String> {
  let task_type = task.notification_type.as_str();
  let payload = &task.payload;

  match task_type {
    "webhook" => {
      let url = payload["url"]
        .as_str()
        .ok_or("Missing url in webhook payload")?;
      let status_str = match payload["build_status"].as_str() {
        Some("succeeded" | "cached_failure") => "success",
        Some("failed") => "failure",
        Some("cancelled") => "cancelled",
        Some("aborted") => "aborted",
        Some("unsupported_system") => "skipped",
        _ => "pending",
      };

      let body = serde_json::json!({
        "build_id": payload["build_id"],
        "build_status": status_str,
        "build_job": payload["build_job"],
        "build_drv": payload["build_drv"],
        "build_output": payload["build_output"],
        "project_name": payload["project_name"],
        "project_url": payload["project_url"],
        "commit_hash": payload["commit_hash"],
      });

      let resp = http_client()
        .post(url)
        .json(&body)
        .send()
        .await
        .map_err(|e| format!("HTTP request failed: {e}"))?;

      if !resp.status().is_success() {
        return Err(format!("Webhook returned status: {}", resp.status()));
      }

      Ok(())
    },
    "github_status" => {
      let token = payload["token"]
        .as_str()
        .ok_or("Missing token in github_status payload")?;
      let repo_url = payload["repository_url"]
        .as_str()
        .ok_or("Missing repository_url")?;
      let commit = payload["commit_hash"]
        .as_str()
        .ok_or("Missing commit_hash")?;
      let job_name =
        payload["build_job"].as_str().ok_or("Missing build_job")?;

      let (owner, repo) = parse_github_repo(repo_url)
        .ok_or_else(|| format!("Cannot parse GitHub repo from {repo_url}"))?;

      let (state, description) = match payload["build_status"].as_str() {
        Some("succeeded" | "cached_failure") => ("success", "Build succeeded"),
        Some("failed") => ("failure", "Build failed"),
        Some("running") => ("pending", "Build in progress"),
        Some("cancelled") => ("error", "Build cancelled"),
        _ => ("pending", "Build queued"),
      };

      let url = format!(
        "https://api.github.com/repos/{owner}/{repo}/statuses/{commit}"
      );
      let body = serde_json::json!({
        "state": state,
        "description": description,
        "context": format!("fc/{job_name}"),
      });

      let resp = http_client()
        .post(&url)
        .header("Authorization", format!("token {token}"))
        .header("User-Agent", "fc-ci")
        .header("Accept", "application/vnd.github+json")
        .json(&body)
        .send()
        .await
        .map_err(|e| format!("GitHub API request failed: {e}"))?;

      if !resp.status().is_success() {
        let status = resp.status();
        let text = resp.text().await.unwrap_or_default();
        return Err(format!("GitHub API returned {status}: {text}"));
      }

      Ok(())
    },
    "gitea_status" => {
      let base_url = payload["base_url"]
        .as_str()
        .ok_or("Missing base_url in gitea_status payload")?;
      let token = payload["token"].as_str().ok_or("Missing token")?;
      let repo_url = payload["repository_url"]
        .as_str()
        .ok_or("Missing repository_url")?;
      let commit = payload["commit_hash"]
        .as_str()
        .ok_or("Missing commit_hash")?;
      let job_name =
        payload["build_job"].as_str().ok_or("Missing build_job")?;

      let (owner, repo) = parse_gitea_repo(repo_url, base_url)
        .ok_or_else(|| format!("Cannot parse Gitea repo from {repo_url}"))?;

      let (state, description) = match payload["build_status"].as_str() {
        Some("succeeded" | "cached_failure") => ("success", "Build succeeded"),
        Some("failed") => ("failure", "Build failed"),
        Some("running") => ("pending", "Build in progress"),
        Some("cancelled") => ("error", "Build cancelled"),
        _ => ("pending", "Build queued"),
      };

      let url =
        format!("{base_url}/api/v1/repos/{owner}/{repo}/statuses/{commit}");
      let body = serde_json::json!({
        "state": state,
        "description": description,
        "context": format!("fc/{job_name}"),
      });

      let resp = http_client()
        .post(&url)
        .header("Authorization", format!("token {token}"))
        .json(&body)
        .send()
        .await
        .map_err(|e| format!("Gitea API request failed: {e}"))?;

      if !resp.status().is_success() {
        let status = resp.status();
        let text = resp.text().await.unwrap_or_default();
        return Err(format!("Gitea API returned {status}: {text}"));
      }

      Ok(())
    },
    "gitlab_status" => {
      let base_url = payload["base_url"]
        .as_str()
        .ok_or("Missing base_url in gitlab_status payload")?;
      let token = payload["token"].as_str().ok_or("Missing token")?;
      let repo_url = payload["repository_url"]
        .as_str()
        .ok_or("Missing repository_url")?;
      let commit = payload["commit_hash"]
        .as_str()
        .ok_or("Missing commit_hash")?;
      let job_name =
        payload["build_job"].as_str().ok_or("Missing build_job")?;

      let project_path =
        parse_gitlab_project(repo_url, base_url).ok_or_else(|| {
          format!("Cannot parse GitLab project from {repo_url}")
        })?;

      let (state, description) = match payload["build_status"].as_str() {
        Some("succeeded" | "cached_failure") => ("success", "Build succeeded"),
        Some("failed") => ("failed", "Build failed"),
        Some("running") => ("running", "Build in progress"),
        Some("cancelled") => ("canceled", "Build cancelled"),
        _ => ("pending", "Build queued"),
      };

      let encoded_project = urlencoding::encode(&project_path);
      let url = format!(
        "{}/api/v4/projects/{}/statuses/{}",
        base_url.trim_end_matches('/'),
        encoded_project,
        commit
      );

      let body = serde_json::json!({
        "state": state,
        "description": description,
        "name": format!("fc/{job_name}"),
      });

      let resp = http_client()
        .post(&url)
        .header("PRIVATE-TOKEN", token)
        .json(&body)
        .send()
        .await
        .map_err(|e| format!("GitLab API request failed: {e}"))?;

      if !resp.status().is_success() {
        let status = resp.status();
        let text = resp.text().await.unwrap_or_default();
        return Err(format!("GitLab API returned {status}: {text}"));
      }

      Ok(())
    },
    "slack" => {
      let webhook_url = payload["webhook_url"]
        .as_str()
        .ok_or("Missing webhook_url in slack payload")?;
      let job = payload["build_job"].as_str().unwrap_or("(unknown)");
      let project = payload["project_name"].as_str().unwrap_or("(unknown)");
      let commit = payload["commit_hash"].as_str().unwrap_or("");
      let status = payload["build_status"].as_str().unwrap_or("unknown");

      let body = serde_json::json!({
        "text": format!("CI: {job} - {status}"),
        "blocks": [{
          "type": "section",
          "text": {
            "type": "mrkdwn",
            "text": format!(
              "*{job}* - *{status}*\nProject: {project} | Commit: `{commit}`"
            ),
          },
        }],
      });

      let resp = http_client()
        .post(webhook_url)
        .json(&body)
        .send()
        .await
        .map_err(|e| format!("Slack webhook request failed: {e}"))?;

      if resp.status().as_u16() == 429 {
        let retry = resp
          .headers()
          .get("retry-after")
          .and_then(|v| v.to_str().ok())
          .unwrap_or("60");
        return Err(format!("Slack rate limited; retry-after={retry}"));
      }
      if !resp.status().is_success() {
        return Err(format!("Slack returned status: {}", resp.status()));
      }
      Ok(())
    },
    "email" => {
      use lettre::{
        AsyncSmtpTransport,
        AsyncTransport,
        Message,
        Tokio1Executor,
        transport::smtp::authentication::Credentials,
      };

      // Email sending is complex, so we'll reuse the existing function
      // by deserializing the config from payload
      let email_config: EmailConfig =
        serde_json::from_value(payload["config"].clone())
          .map_err(|e| format!("Failed to deserialize email config: {e}"))?;

      // Create a minimal Build struct from payload
      let build_id = payload["build_id"]
        .as_str()
        .and_then(|s| uuid::Uuid::parse_str(s).ok())
        .ok_or("Invalid build_id")?;
      let job_name = payload["build_job"]
        .as_str()
        .ok_or("Missing build_job")?
        .to_string();
      let drv_path = payload["build_drv"]
        .as_str()
        .ok_or("Missing build_drv")?
        .to_string();
      let build_output_path =
        payload["build_output"].as_str().map(String::from);

      let status_str = payload["build_status"]
        .as_str()
        .ok_or("Missing build_status")?;
      let status = match status_str {
        "succeeded" => BuildStatus::Succeeded,
        _ => BuildStatus::Failed,
      };

      let project_name = payload["project_name"]
        .as_str()
        .ok_or("Missing project_name")?;

      let status_display = match status {
        BuildStatus::Succeeded => "SUCCESS",
        _ => "FAILURE",
      };

      let subject =
        format!("[FC] {status_display} - {job_name} ({project_name})");
      let body = format!(
        "Build notification from FC CI\n\nProject: {}\nJob: {}\nStatus: \
         {}\nDerivation: {}\nOutput: {}\nBuild ID: {}\n",
        project_name,
        job_name,
        status_display,
        drv_path,
        build_output_path.as_deref().unwrap_or("N/A"),
        build_id,
      );

      for to_addr in &email_config.to_addresses {
        let email = Message::builder()
          .from(
            email_config
              .from_address
              .parse()
              .map_err(|e| format!("Invalid from address: {e}"))?,
          )
          .to(
            to_addr
              .parse()
              .map_err(|e| format!("Invalid to address: {e}"))?,
          )
          .subject(&subject)
          .body(body.clone())
          .map_err(|e| format!("Failed to build email: {e}"))?;

        let mut mailer_builder = if email_config.tls {
          AsyncSmtpTransport::<Tokio1Executor>::relay(&email_config.smtp_host)
            .map_err(|e| format!("Failed to create SMTP transport: {e}"))?
        } else {
          AsyncSmtpTransport::<Tokio1Executor>::builder_dangerous(
            &email_config.smtp_host,
          )
        }
        .port(email_config.smtp_port);

        if let (Some(user), Some(pass)) =
          (&email_config.smtp_user, &email_config.smtp_password)
        {
          mailer_builder = mailer_builder
            .credentials(Credentials::new(user.clone(), pass.clone()));
        }

        let mailer = mailer_builder.build();
        mailer
          .send(email)
          .await
          .map_err(|e| format!("Failed to send email: {e}"))?;
      }

      Ok(())
    },
    _ => Err(format!("Unknown notification type: {task_type}")),
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn test_parse_github_repo_https() {
    let result = parse_github_repo("https://github.com/owner/repo.git");
    assert_eq!(result, Some(("owner".to_string(), "repo".to_string())));

    let result = parse_github_repo("https://github.com/owner/repo");
    assert_eq!(result, Some(("owner".to_string(), "repo".to_string())));
  }

  #[test]
  fn test_parse_github_repo_ssh() {
    let result = parse_github_repo("git@github.com:owner/repo.git");
    assert_eq!(result, Some(("owner".to_string(), "repo".to_string())));
  }

  #[test]
  fn test_parse_github_repo_invalid() {
    assert_eq!(parse_github_repo("https://gitlab.com/owner/repo"), None);
    assert_eq!(parse_github_repo("invalid-url"), None);
  }

  #[test]
  fn test_parse_gitea_repo() {
    let result = parse_gitea_repo(
      "https://gitea.example.com/owner/repo.git",
      "https://gitea.example.com",
    );
    assert_eq!(result, Some(("owner".to_string(), "repo".to_string())));

    let result = parse_gitea_repo(
      "https://gitea.example.com/owner/repo",
      "https://gitea.example.com/",
    );
    assert_eq!(result, Some(("owner".to_string(), "repo".to_string())));
  }

  #[test]
  fn test_parse_gitlab_project() {
    let result = parse_gitlab_project(
      "https://gitlab.com/group/subgroup/repo.git",
      "https://gitlab.com",
    );
    assert_eq!(result, Some("group/subgroup/repo".to_string()));

    let result = parse_gitlab_project(
      "https://gitlab.com/owner/repo",
      "https://gitlab.com/",
    );
    assert_eq!(result, Some("owner/repo".to_string()));
  }

  #[test]
  fn test_parse_gitlab_project_ssh() {
    let result = parse_gitlab_project(
      "git@gitlab.com:group/repo.git",
      "https://gitlab.com",
    );
    assert_eq!(result, Some("group/repo".to_string()));
  }
}
