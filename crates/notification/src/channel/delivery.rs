use circus_config::EmailConfig;
use circus_http::{RequestExt as _, ResponseExt as _, guard::resolve_public};
use circus_logs::ErrorChain;
use lettre::{
  AsyncSmtpTransport,
  AsyncTransport,
  Message,
  Tokio1Executor,
  message::{Mailbox, header::ContentType},
  transport::smtp::{
    authentication::Credentials,
    client::{Tls, TlsParameters},
  },
};
use reqwest::StatusCode;
use tracing::{info, warn};

use super::{
  GiteaStatusChannel,
  GithubStatusChannel,
  GitlabStatusChannel,
  Notifier,
  Origin,
  RESERVED_HEADERS,
  SIGNATURE_HEADER,
  SlackChannel,
  WebhookChannel,
  sign_body,
};
use crate::{
  BuildEvent,
  parse_gitea_repo,
  parse_github_repo,
  parse_gitlab_project,
};

impl Notifier for WebhookChannel {
  fn applies_to(&self, event: &BuildEvent) -> bool {
    !self.on_failure_only || event.is_failure()
  }

  async fn deliver(
    &self,
    event: &BuildEvent,
    origin: Origin,
  ) -> Result<(), String> {
    let http = crate::http_client(origin);
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

    let mut req = http
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

    req
      .body(body)
      .send_checked()
      .await
      .map_err(|e| format!("Webhook failed: {}", ErrorChain(&e)))?;
    info!(build_id = %event.build_id, "Webhook notification sent");
    Ok(())
  }
}

impl Notifier for GithubStatusChannel {
  fn applies_to(&self, _event: &BuildEvent) -> bool {
    true
  }

  async fn deliver(
    &self,
    event: &BuildEvent,
    origin: Origin,
  ) -> Result<(), String> {
    let http = crate::http_client(origin);
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

    let resp = http
      .post(&url)
      .bearer_auth(&self.token)
      .header("Accept", "application/vnd.github+json")
      .header("X-GitHub-Api-Version", circus_http::GITHUB_API_VERSION)
      .json(&body)
      .send_checked()
      .await
      .map_err(|e| format!("GitHub status failed: {}", ErrorChain(&e)))?;
    let rate_limit =
      super::super::extract_rate_limit_from_headers(resp.headers());
    info!(build_id = %event.build_id, "Set GitHub commit status: {state}");
    super::super::apply_github_rate_limit(rate_limit).await;
    Ok(())
  }
}

impl Notifier for GiteaStatusChannel {
  fn applies_to(&self, _event: &BuildEvent) -> bool {
    true
  }

  async fn deliver(
    &self,
    event: &BuildEvent,
    origin: Origin,
  ) -> Result<(), String> {
    let http = crate::http_client(origin);
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

    http
      .post(&url)
      .header("Authorization", format!("token {}", self.token))
      .json(&body)
      .send_checked()
      .await
      .map_err(|e| format!("Gitea status failed: {}", ErrorChain(&e)))?;
    info!(build_id = %event.build_id, "Set Gitea commit status: {state}");
    Ok(())
  }
}

impl Notifier for GitlabStatusChannel {
  fn applies_to(&self, _event: &BuildEvent) -> bool {
    true
  }

  async fn deliver(
    &self,
    event: &BuildEvent,
    origin: Origin,
  ) -> Result<(), String> {
    let http = crate::http_client(origin);
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

    http
      .post(&url)
      .header("PRIVATE-TOKEN", &self.token)
      .json(&body)
      .send_checked()
      .await
      .map_err(|e| format!("GitLab status failed: {}", ErrorChain(&e)))?;
    info!(build_id = %event.build_id, "Set GitLab commit status: {state}");
    Ok(())
  }
}

impl Notifier for SlackChannel {
  fn applies_to(&self, event: &BuildEvent) -> bool {
    !self.on_failure_only || event.is_failure()
  }

  async fn deliver(
    &self,
    event: &BuildEvent,
    origin: Origin,
  ) -> Result<(), String> {
    let http = crate::http_client(origin);
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

    let resp = http
      .post(&self.webhook_url)
      .json(&body)
      .send_redacted()
      .await
      .map_err(slack_failed)?;

    if resp.status() == StatusCode::TOO_MANY_REQUESTS {
      let retry = resp
        .headers()
        .get("retry-after")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("60");
      return Err(format!("Slack rate limited; retry-after={retry}"));
    }

    resp.checked().await.map_err(slack_failed)?;
    info!(build_id = %event.build_id, "Slack notification sent");
    Ok(())
  }
}

/// The webhook URL is itself the Slack credential, so the error leaves it out.
fn slack_failed(error: circus_http::Error) -> String {
  match error {
    circus_http::Error::Status { status, body, .. } => {
      format!("Slack returned {status}: {body}")
    },
    other => {
      std::error::Error::source(&other).map_or_else(
        || "Slack webhook failed".to_owned(),
        |cause| format!("Slack webhook failed: {}", ErrorChain(cause)),
      )
    },
  }
}

impl Notifier for EmailConfig {
  fn applies_to(&self, event: &BuildEvent) -> bool {
    !self.on_failure_only || event.is_failure()
  }

  async fn deliver(
    &self,
    event: &BuildEvent,
    origin: Origin,
  ) -> Result<(), String> {
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

    let transport = |server: String| {
      let mailer =
        AsyncSmtpTransport::<Tokio1Executor>::builder_dangerous(server)
          .port(self.smtp_port);
      let mailer = if self.tls {
        let tls = TlsParameters::new(self.smtp_host.clone())
          .map_err(|e| format!("Failed to create SMTP transport: {e}"))?;
        mailer.tls(Tls::Wrapper(tls))
      } else {
        mailer
      };
      let mailer = if let (Some(user), Some(pass)) =
        (&self.smtp_user, &self.smtp_password)
      {
        mailer.credentials(Credentials::new(user.clone(), pass.clone()))
      } else {
        mailer
      };

      Ok::<_, String>(mailer.build())
    };

    // lettre resolves the host itself, so project channels connect to an
    // address checked here and keep the name for TLS.
    let mailer = match origin {
      Origin::Instance => transport(self.smtp_host.clone())?,
      Origin::Project => {
        let addrs = resolve_public(&self.smtp_host, self.smtp_port)
          .await
          .map_err(|e| format!("Failed to resolve SMTP host: {e}"))?;
        let mut last_error = String::new();
        let mut reachable = None;

        for addr in addrs {
          let mailer = transport(addr.ip().to_string())?;

          match mailer.test_connection().await {
            Ok(true) => {
              reachable = Some(mailer);
              break;
            },
            Ok(false) => last_error = format!("{addr} did not answer NOOP"),
            Err(e) => last_error = format!("{addr}: {e}"),
          }
        }

        reachable.ok_or_else(|| {
          format!("Failed to connect to SMTP host: {last_error}")
        })?
      },
    };

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
