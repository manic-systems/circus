use circus_config::EmailConfig;
use lettre::{
  AsyncSmtpTransport,
  AsyncTransport,
  Message,
  Tokio1Executor,
  message::{Mailbox, header::ContentType},
  transport::smtp::authentication::Credentials,
};
use tracing::{info, warn};

use super::{
  GiteaStatusChannel,
  GithubStatusChannel,
  GitlabStatusChannel,
  Notifier,
  RESERVED_HEADERS,
  SIGNATURE_HEADER,
  SlackChannel,
  WebhookChannel,
  sign_body,
};
use crate::{
  BuildEvent,
  http_client,
  parse_gitea_repo,
  parse_github_repo,
  parse_gitlab_project,
};

impl Notifier for WebhookChannel {
  fn applies_to(&self, event: &BuildEvent) -> bool {
    !self.on_failure_only || event.is_failure()
  }

  async fn deliver(&self, event: &BuildEvent) -> Result<(), String> {
    let payload = serde_json::json!({
      "build_id":     event.build_id,
      "build_status": event.generic_status(),
      "build_job":    event.job_name,
      "build_drv":    event.drv_path,
      "build_output": event.build_output.as_deref().unwrap_or(""),
      "project_name": event.project_name,
      "project_url":  event.project_url,
      "commit_hash":  event.commit_hash,
    });
    let body = serde_json::to_vec(&payload)
      .map_err(|e| format!("Failed to serialize webhook payload: {e}"))?;

    let mut req = http_client()
      .post(&self.url)
      .header("Content-Type", "application/json");

    if let Some(secret) = &self.secret {
      req = req.header(SIGNATURE_HEADER, sign_body(secret, &body)?);
    }
    for (name, value) in &self.headers {
      if RESERVED_HEADERS
        .iter()
        .any(|r| r.eq_ignore_ascii_case(name))
      {
        warn!(header = %name, "Ignoring reserved webhook header override");
        continue;
      }
      req = req.header(name, value);
    }

    let resp = req
      .body(body)
      .send()
      .await
      .map_err(|e| format!("Webhook request failed: {e}"))?;
    if resp.status().is_success() {
      info!(build_id = %event.build_id, "Webhook notification sent");
      Ok(())
    } else {
      Err(format!("Webhook returned status: {}", resp.status()))
    }
  }
}

impl Notifier for GithubStatusChannel {
  fn applies_to(&self, _event: &BuildEvent) -> bool {
    true
  }

  async fn deliver(&self, event: &BuildEvent) -> Result<(), String> {
    let (owner, repo) =
      parse_github_repo(&event.project_url).ok_or_else(|| {
        format!("Cannot parse GitHub repo from {}", event.project_url)
      })?;
    let (state, description) = event.github_state();
    let url = format!(
      "https://api.github.com/repos/{owner}/{repo}/statuses/{}",
      event.commit_hash
    );
    let body = serde_json::json!({
      "state": state,
      "description": description,
      "context": format!("circus/{}", event.job_name),
    });

    let resp = http_client()
      .post(&url)
      .header("Authorization", format!("token {}", self.token))
      .header("User-Agent", "circus")
      .header("Accept", "application/vnd.github+json")
      .json(&body)
      .send()
      .await
      .map_err(|e| format!("GitHub API request failed: {e}"))?;

    let status = resp.status();
    let rate_limit =
      super::super::extract_rate_limit_from_headers(resp.headers());
    if !status.is_success() {
      let text = resp.text().await.unwrap_or_default();
      return Err(format!("GitHub API returned {status}: {text}"));
    }
    info!(build_id = %event.build_id, "Set GitHub commit status: {state}");
    super::super::apply_github_rate_limit(rate_limit).await;
    Ok(())
  }
}

impl Notifier for GiteaStatusChannel {
  fn applies_to(&self, _event: &BuildEvent) -> bool {
    true
  }

  async fn deliver(&self, event: &BuildEvent) -> Result<(), String> {
    let (owner, repo) = parse_gitea_repo(&event.project_url, &self.base_url)
      .ok_or_else(|| {
        format!("Cannot parse Gitea repo from {}", event.project_url)
      })?;
    let (state, description) = event.github_state();
    let url = format!(
      "{}/api/v1/repos/{owner}/{repo}/statuses/{}",
      self.base_url.trim_end_matches('/'),
      event.commit_hash
    );
    let body = serde_json::json!({
      "state": state,
      "description": description,
      "context": format!("circus/{}", event.job_name),
    });

    let resp = http_client()
      .post(&url)
      .header("Authorization", format!("token {}", self.token))
      .json(&body)
      .send()
      .await
      .map_err(|e| format!("Gitea API request failed: {e}"))?;
    if resp.status().is_success() {
      info!(build_id = %event.build_id, "Set Gitea commit status: {state}");
      Ok(())
    } else {
      let status = resp.status();
      let text = resp.text().await.unwrap_or_default();
      Err(format!("Gitea API returned {status}: {text}"))
    }
  }
}

impl Notifier for GitlabStatusChannel {
  fn applies_to(&self, _event: &BuildEvent) -> bool {
    true
  }

  async fn deliver(&self, event: &BuildEvent) -> Result<(), String> {
    let project_path = parse_gitlab_project(&event.project_url, &self.base_url)
      .ok_or_else(|| {
        format!("Cannot parse GitLab project from {}", event.project_url)
      })?;
    let (state, description) = event.gitlab_state();
    let encoded_project = urlencoding::encode(&project_path);
    let url = format!(
      "{}/api/v4/projects/{}/statuses/{}",
      self.base_url.trim_end_matches('/'),
      encoded_project,
      event.commit_hash
    );
    let body = serde_json::json!({
      "state": state,
      "description": description,
      "name": format!("circus/{}", event.job_name),
    });

    let resp = http_client()
      .post(&url)
      .header("PRIVATE-TOKEN", &self.token)
      .json(&body)
      .send()
      .await
      .map_err(|e| format!("GitLab API request failed: {e}"))?;
    if resp.status().is_success() {
      info!(build_id = %event.build_id, "Set GitLab commit status: {state}");
      Ok(())
    } else {
      let status = resp.status();
      let text = resp.text().await.unwrap_or_default();
      Err(format!("GitLab API returned {status}: {text}"))
    }
  }
}

impl Notifier for SlackChannel {
  fn applies_to(&self, event: &BuildEvent) -> bool {
    !self.on_failure_only || event.is_failure()
  }

  async fn deliver(&self, event: &BuildEvent) -> Result<(), String> {
    let status = event.generic_status();
    let body = serde_json::json!({
      "text": format!("CI: {} - {status}", event.job_name),
      "blocks": [{
        "type": "section",
        "text": {
          "type": "mrkdwn",
          "text": format!(
            "*{}* - *{status}*\nProject: {} | Commit: `{}`",
            event.job_name, event.project_name, event.commit_hash
          ),
        },
      }],
    });

    let resp = http_client()
      .post(&self.webhook_url)
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
    if resp.status().is_success() {
      info!(build_id = %event.build_id, "Slack notification sent");
      Ok(())
    } else {
      Err(format!("Slack returned status: {}", resp.status()))
    }
  }
}

impl Notifier for EmailConfig {
  fn applies_to(&self, event: &BuildEvent) -> bool {
    !self.on_failure_only || event.is_failure()
  }

  async fn deliver(&self, event: &BuildEvent) -> Result<(), String> {
    let status = event.email_status();
    let subject = format!(
      "[circus] {status} - {} ({})",
      event.job_name, event.project_name
    );
    let body = format!(
      "Build notification from circus\n\nProject: {}\nJob: {}\nStatus: \
       {}\nDerivation: {}\nOutput: {}\nBuild ID: {}\n",
      event.project_name,
      event.job_name,
      status,
      event.drv_path,
      event.build_output.as_deref().unwrap_or("N/A"),
      event.build_id,
    );

    let from: Mailbox = self.from_address.parse().map_err(|e| {
      format!("Invalid from address '{}': {e}", self.from_address)
    })?;

    let mailer = if self.tls {
      AsyncSmtpTransport::<Tokio1Executor>::relay(&self.smtp_host)
        .map_err(|e| format!("Failed to create SMTP transport: {e}"))?
    } else {
      AsyncSmtpTransport::<Tokio1Executor>::builder_dangerous(&self.smtp_host)
    }
    .port(self.smtp_port);
    let mailer = if let (Some(user), Some(pass)) =
      (&self.smtp_user, &self.smtp_password)
    {
      mailer.credentials(Credentials::new(user.clone(), pass.clone()))
    } else {
      mailer
    }
    .build();

    for to_addr in &self.to_addresses {
      let to: Mailbox = match to_addr.parse() {
        Ok(addr) => addr,
        Err(e) => {
          warn!("Invalid to address '{to_addr}': {e}");
          continue;
        },
      };
      let email = Message::builder()
        .from(from.clone())
        .to(to)
        .subject(&subject)
        .header(ContentType::TEXT_PLAIN)
        .body(body.clone())
        .map_err(|e| format!("Failed to build email: {e}"))?;
      AsyncTransport::send(&mailer, email)
        .await
        .map_err(|e| format!("Failed to send email to {to_addr}: {e}"))?;
      info!(build_id = %event.build_id, to = to_addr, "Email notification sent");
    }
    Ok(())
  }
}
