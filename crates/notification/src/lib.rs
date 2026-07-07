//! Notification dispatch for build events.
//!
//! Delivery is driven by per-project [`notification_configs`] rows, merged with
//! the global `[notifications]` config as a fallback for any channel a project
//! has not configured. The same [`NotificationChannel`] objects are used on
//! both the immediate path and the persistent retry-queue path, so behavior is
//! identical regardless of `enable_retry_queue`.

mod channel;
mod event;

use std::{
  sync::OnceLock,
  time::{Duration, SystemTime, UNIX_EPOCH},
};

pub use channel::NotificationChannel;
use circus_common::{
  models::{Build, BuildStatus, NotificationType, Project},
  repo,
};
use circus_config::NotificationsConfig;
pub use event::BuildEvent;
use reqwest::Url;
use sqlx::PgPool;
use tracing::{error, info, warn};

/// Shared HTTP client for all notification dispatches.
/// Avoids recreating connection pools on every build completion. A bounded
/// timeout keeps a misbehaving webhook target from pinning a worker
/// indefinitely; the retry queue handles transient timeouts via its own
/// backoff.
pub(crate) fn http_client() -> &'static reqwest::Client {
  static CLIENT: OnceLock<reqwest::Client> = OnceLock::new();
  CLIENT.get_or_init(|| {
    reqwest::Client::builder()
      .connect_timeout(Duration::from_secs(10))
      .timeout(Duration::from_secs(30))
      .build()
      .unwrap_or_else(|_| reqwest::Client::new())
  })
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

fn unix_now_secs() -> u64 {
  SystemTime::now()
    .duration_since(UNIX_EPOCH)
    .map_or(0, |duration| duration.as_secs())
}

/// Log when the GitHub rate budget runs low and sleep when it is critical,
/// mirroring Hydra's adaptive backoff thresholds (2000 warn / 1000 sleep).
pub(crate) async fn apply_github_rate_limit(
  rate_limit: Option<RateLimitState>,
) {
  let Some(rate_limit) = rate_limit else {
    return;
  };
  let now = unix_now_secs();
  if rate_limit.remaining <= 2000 {
    let seconds_until_reset = rate_limit.reset_at.saturating_sub(now);
    info!(
      "GitHub rate limit: {}/{}, resets in {}s",
      rate_limit.remaining, rate_limit.limit, seconds_until_reset
    );
  }
  if rate_limit.remaining <= 1000 {
    let delay = calculate_delay(&rate_limit, now);
    warn!(
      "GitHub rate limit critical: {}/{}, sleeping {}s",
      rate_limit.remaining, rate_limit.limit, delay
    );
    tokio::time::sleep(Duration::from_secs(delay)).await;
  }
}

/// Resolve the effective notification channels for a project: per-project
/// configs take precedence, and the global config fills in any channel kind the
/// project has not configured.
async fn resolve_channels(
  pool: Option<&PgPool>,
  build: &Build,
  project: &Project,
  commit_hash: &str,
  config: &NotificationsConfig,
  encryption_key: Option<&str>,
) -> Vec<NotificationChannel> {
  let event = BuildEvent::from_build(build, project, commit_hash);
  let mut channels: Vec<NotificationChannel> = Vec::new();
  let mut configured_types: Vec<NotificationType> = Vec::new();

  // Per-project channels (decrypted from the database).
  if let Some(pool) = pool {
    match repo::notification_configs::list_for_project(pool, project.id).await {
      Ok(rows) => {
        for row in rows {
          match NotificationChannel::from_stored(
            row.notification_type,
            &row.config,
            encryption_key,
          ) {
            Ok(channel) => {
              configured_types.push(channel.notification_type());
              channels.push(channel);
            },
            Err(e) => {
              warn!(
                project = %project.name,
                notification_type = %row.notification_type,
                "Skipping invalid per-project notification config: {e}"
              );
            },
          }
        }
      },
      Err(e) => {
        error!(
          project = %project.name,
          "Failed to load per-project notification configs: {e}"
        );
      },
    }
  }

  // Global fallback for any channel kind the project did not configure.
  for channel in global_fallback_channels(config, &event) {
    if !configured_types.contains(&channel.notification_type()) {
      channels.push(channel);
    }
  }

  channels
}

/// Build channels from the global `[notifications]` config. These apply only
/// when a project has not configured the corresponding channel kind.
fn global_fallback_channels(
  config: &NotificationsConfig,
  event: &BuildEvent,
) -> Vec<NotificationChannel> {
  use channel::{
    GiteaStatusChannel,
    GithubStatusChannel,
    GitlabStatusChannel,
    SlackChannel,
    WebhookChannel,
  };

  let mut out = Vec::new();
  if let Some(url) = &config.webhook_url {
    out.push(NotificationChannel::Webhook(WebhookChannel {
      url: url.clone(),
      ..Default::default()
    }));
  }
  if let Some(token) = &config.github_token
    && event.project_url.contains("github.com")
  {
    out.push(NotificationChannel::GithubStatus(GithubStatusChannel {
      token: token.clone(),
    }));
  }
  if let (Some(base_url), Some(token)) =
    (&config.gitea_url, &config.gitea_token)
  {
    out.push(NotificationChannel::GiteaStatus(GiteaStatusChannel {
      base_url: base_url.clone(),
      token:    token.clone(),
    }));
  }
  if let (Some(base_url), Some(token)) =
    (&config.gitlab_url, &config.gitlab_token)
  {
    out.push(NotificationChannel::GitlabStatus(GitlabStatusChannel {
      base_url: base_url.clone(),
      token:    token.clone(),
    }));
  }
  if let Some(slack) = &config.slack {
    out.push(NotificationChannel::Slack(SlackChannel {
      webhook_url:     slack.webhook_url.clone(),
      on_failure_only: slack.on_failure_only,
    }));
  }
  if let Some(email) = &config.email {
    out.push(NotificationChannel::Email(email.clone()));
  }
  out
}

/// Deliver the applicable channels for an event, either immediately or by
/// enqueuing retry-queue tasks. `commit_status_only` restricts delivery to the
/// forge commit-status channels (used on build-created/started transitions).
async fn dispatch(
  pool: Option<&PgPool>,
  build: &Build,
  project: &Project,
  commit_hash: &str,
  config: &NotificationsConfig,
  encryption_key: Option<&str>,
  commit_status_only: bool,
) {
  let event = BuildEvent::from_build(build, project, commit_hash);

  // Intermediate dependency builds are internal scheduling artifacts; their
  // status is rolled up into the top-level job. Forwarding them would pollute
  // commit statuses (and every other channel) with one entry per dependency.
  if event.is_dependency() {
    return;
  }

  let channels =
    resolve_channels(pool, build, project, commit_hash, config, encryption_key)
      .await;

  // The retry queue persists secrets in the task payload, so it requires an
  // encryption key. Without one, fall back to immediate delivery rather than
  // writing plaintext secrets to the database.
  let use_queue =
    config.enable_retry_queue && pool.is_some() && encryption_key.is_some();
  if config.enable_retry_queue && encryption_key.is_none() {
    warn!(
      build_id = %build.id,
      "Notification retry queue is enabled but \
       server.webhook_secret_encryption_key is unset; delivering immediately"
    );
  }

  for channel in &channels {
    if commit_status_only && !channel.is_commit_status() {
      continue;
    }
    if !channel.applies_to(&event) {
      continue;
    }

    match (use_queue, pool) {
      (true, Some(pool)) => {
        enqueue(
          pool,
          channel,
          &event,
          config.max_retry_attempts,
          encryption_key,
        )
        .await
      },
      (false, Some(pool)) => {
        if channel.is_commit_status() {
          match stale_commit_status_event(pool, &event).await {
            Ok(true) => continue,
            Ok(false) => (),
            Err(e) => {
              warn!(
                  build_id = %build.id,
                  notification_type = %channel.notification_type(),
                  "Skipping commit status, freshness check failed: {e}"
              );
              continue;
            },
          }
        }

        if let Err(e) = channel.deliver(&event).await {
          error!(
              build_id = %build.id,
              notification_type = %channel.notification_type(),
              "Notification delivery failed: {e}"
          )
        }
      },
      (_, None) => {
        if channel.is_commit_status() {
          warn!(
              build_id = %build.id,
              notification_type = %channel.notification_type(),
              "Delivering commit status without freshness check"
          );
        }

        if let Err(e) = channel.deliver(&event).await {
          error!(
              build_id = %build.id,
              notification_type = %channel.notification_type(),
              "Notification delivery failed: {e}"
          )
        }
      },
    }
  }
}

/// Serialize a channel + event into a retry-queue task (secrets re-encrypted).
async fn enqueue(
  pool: &PgPool,
  channel: &NotificationChannel,
  event: &BuildEvent,
  max_attempts: i32,
  encryption_key: Option<&str>,
) {
  let (notification_type, stored) = match channel.to_stored(encryption_key) {
    Ok(parts) => parts,
    Err(e) => {
      error!("Failed to serialize notification for queue: {e}");
      return;
    },
  };
  let payload = serde_json::json!({
    "notification_type": notification_type,
    "channel": stored,
    "event": event,
  });
  if let Err(e) = repo::notification_tasks::create(
    pool,
    notification_type,
    payload,
    max_attempts,
  )
  .await
  {
    error!(build_id = %event.build_id, "Failed to enqueue {notification_type} notification: {e}");
  }
}

/// Dispatch all configured notifications for a completed build.
pub async fn dispatch_build_finished(
  pool: Option<&PgPool>,
  build: &Build,
  project: &Project,
  commit_hash: &str,
  config: &NotificationsConfig,
  encryption_key: Option<&str>,
) {
  info!(
    build_id = %build.id,
    enable_retry_queue = config.enable_retry_queue,
    pool_present = pool.is_some(),
    "Dispatching build finished notifications"
  );
  dispatch(
    pool,
    build,
    project,
    commit_hash,
    config,
    encryption_key,
    false,
  )
  .await;
}

/// Dispatch commit-status notifications when a build is created (pending).
pub async fn dispatch_build_created(
  pool: &PgPool,
  build: &Build,
  project: &Project,
  commit_hash: &str,
  config: &NotificationsConfig,
  encryption_key: Option<&str>,
) {
  dispatch(
    Some(pool),
    build,
    project,
    commit_hash,
    config,
    encryption_key,
    true,
  )
  .await;
  info!(
    build_id = %build.id,
    job = %build.job_name,
    status = %build.status,
    "Dispatched commit status notification for build creation"
  );
}

/// Dispatch commit-status notifications when a build starts (running).
pub async fn dispatch_build_started(
  pool: &PgPool,
  build: &Build,
  project: &Project,
  commit_hash: &str,
  config: &NotificationsConfig,
  encryption_key: Option<&str>,
) {
  dispatch(
    Some(pool),
    build,
    project,
    commit_hash,
    config,
    encryption_key,
    true,
  )
  .await;
  info!(
    build_id = %build.id,
    job = %build.job_name,
    status = %build.status,
    "Dispatched commit status notification for build start"
  );
}

/// Process a notification task from the retry queue.
///
/// # Errors
///
/// Returns an error string if notification delivery fails, so the caller can
/// record it and schedule a retry.
pub async fn process_notification_task(
  pool: &PgPool,
  task: &circus_common::models::NotificationTask,
  encryption_key: Option<&str>,
) -> Result<(), String> {
  let payload = &task.payload;

  // Current payload format: { notification_type, channel, event }.
  if let (Some(channel_cfg), Some(event)) =
    (payload.get("channel"), payload.get("event"))
  {
    let channel = NotificationChannel::from_stored(
      task.notification_type,
      channel_cfg,
      encryption_key,
    )
    .map_err(|e| format!("Invalid stored notification channel: {e}"))?;
    let event: BuildEvent = serde_json::from_value(event.clone())
      .map_err(|e| format!("Invalid stored build event: {e}"))?;
    if event.is_dependency() {
      return Ok(());
    }
    if channel.is_commit_status()
      && stale_commit_status_event(pool, &event).await?
    {
      info!(
        build_id = %event.build_id,
        notification_type = %task.notification_type,
        status = ?event.status,
        "Skipping stale commit status notification"
      );
      return Ok(());
    }
    return channel.deliver(&event).await;
  }

  Err(format!(
    "Unrecognized notification task payload for type '{}'",
    task.notification_type
  ))
}

const fn status_rank(status: BuildStatus) -> u8 {
  match status {
    BuildStatus::Pending => 0,
    BuildStatus::Running => 1,
    _ => 2,
  }
}

async fn stale_commit_status_event(
  pool: &PgPool,
  event: &BuildEvent,
) -> Result<bool, String> {
  let build = repo::builds::get(pool, event.build_id).await.map_err(|e| {
    format!("Failed to load build for notification freshness: {e}")
  })?;
  if build.status == event.status {
    return Ok(false);
  }
  if status_rank(build.status) > status_rank(event.status) {
    return Ok(true);
  }
  Ok(event.status.is_finished() && !build.status.is_finished())
}

/// Validate and encrypt the secret fields of every declarative notification
/// config in place, ahead of [`circus_common::bootstrap::run`].
///
/// The notification repo layer stores config blobs verbatim, so encryption and
/// validation happen here, where the channel types live. This keeps
/// `circus-common` free of any dependency on this crate.
///
/// # Errors
///
/// Returns an error if any declarative notification config is invalid (bad URL,
/// unknown type, ...) or if encryption fails (e.g. a secret is configured but
/// no encryption key is available).
pub fn encrypt_declarative_notifications(
  config: &mut circus_config::DeclarativeConfig,
  encryption_key: Option<&str>,
) -> circus_common::error::Result<()> {
  for project in &mut config.projects {
    for notification in &mut project.notifications {
      notification.config = NotificationChannel::encrypt_into_stored(
        notification.notification_type,
        &notification.config,
        encryption_key,
      )?;
    }
  }
  Ok(())
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ForgeUrl {
  host: String,
  path: String,
}

impl ForgeUrl {
  pub(crate) fn parse(raw: &str) -> Option<Self> {
    let raw = raw.trim();
    if raw.is_empty() {
      return None;
    }

    if let Ok(url) = Url::parse(raw) {
      let host = url.host_str()?.to_ascii_lowercase();
      let path = normalize_repo_path(url.path())?;
      return Some(Self { host, path });
    }

    let (prefix, path) = raw.split_once(':')?;
    let host = prefix.strip_prefix("git@")?.to_ascii_lowercase();
    let path = normalize_repo_path(path)?;
    Some(Self { host, path })
  }

  fn matches_base(&self, base_url: &str) -> bool {
    Url::parse(base_url)
      .ok()
      .and_then(|url| url.host_str().map(str::to_ascii_lowercase))
      .is_some_and(|host| host == self.host)
  }

  fn owner_repo(&self) -> Option<(String, String)> {
    let (owner, repo) = self.path.split_once('/')?;
    Some((owner.to_string(), repo.to_string()))
  }

  fn project_path(&self) -> String {
    self.path.clone()
  }
}

fn normalize_repo_path(path: &str) -> Option<String> {
  let path = path.trim_start_matches('/').trim_end_matches(".git");
  (!path.is_empty()).then(|| path.to_string())
}

pub(crate) fn parse_github_repo(url: &str) -> Option<(String, String)> {
  let url = ForgeUrl::parse(url)?;
  (url.host == "github.com").then_some(())?;
  url.owner_repo()
}

pub(crate) fn parse_gitea_repo(
  repo_url: &str,
  base_url: &str,
) -> Option<(String, String)> {
  let url = ForgeUrl::parse(repo_url)?;
  url.matches_base(base_url).then_some(())?;
  url.owner_repo()
}

pub(crate) fn parse_gitlab_project(
  repo_url: &str,
  base_url: &str,
) -> Option<String> {
  let url = ForgeUrl::parse(repo_url)?;
  url.matches_base(base_url).then_some(())?;
  Some(url.project_path())
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

  #[test]
  fn test_parse_gitlab_project_ssh_nested_group() {
    let result = parse_gitlab_project(
      "git@gitlab.com:group/subgroup/repo.git",
      "https://gitlab.com",
    );
    assert_eq!(result, Some("group/subgroup/repo".to_string()));
  }
}
