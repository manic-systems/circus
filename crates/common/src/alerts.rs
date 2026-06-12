use std::sync::Arc;

use chrono::Utc;
use sqlx::PgPool;
use tokio::sync::RwLock;
use tracing::info;
use uuid::Uuid;

use crate::{config::AlertConfig, repo::build_metrics};

#[derive(Debug, Clone)]
pub struct AlertState {
  pub last_alert_at: chrono::DateTime<Utc>,
}

impl Default for AlertState {
  fn default() -> Self {
    Self {
      last_alert_at: chrono::DateTime::<Utc>::MIN_UTC,
    }
  }
}

pub struct AlertManager {
  config: AlertConfig,
  state:  Arc<RwLock<AlertState>>,
}

impl std::fmt::Debug for AlertManager {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    f.debug_struct("AlertManager")
      .field("config", &self.config)
      .finish_non_exhaustive()
  }
}

impl AlertManager {
  /// Create an alert manager from config.
  #[must_use]
  pub fn new(config: AlertConfig) -> Self {
    Self {
      config,
      state: Arc::new(RwLock::new(AlertState::default())),
    }
  }

  /// Check if alerts are enabled in the config.
  #[must_use]
  pub const fn is_enabled(&self) -> bool {
    self.config.enabled
  }

  /// Calculate failure rate and dispatch alerts if threshold exceeded.
  ///
  /// # Returns
  ///
  /// Returns the computed failure rate if alerts are enabled.
  pub async fn check_and_alert(
    &self,
    pool: &PgPool,
    project_id: Option<Uuid>,
    jobset_id: Option<Uuid>,
  ) -> Option<f64> {
    if !self.is_enabled() {
      return None;
    }

    let Ok(failure_rate) = build_metrics::calculate_failure_rate(
      pool,
      project_id,
      jobset_id,
      self.config.time_window_minutes,
    )
    .await
    else {
      return None;
    };

    if failure_rate > self.config.error_threshold {
      let mut state = self.state.write().await;
      let time_since_last = (Utc::now() - state.last_alert_at).num_minutes();

      if time_since_last >= self.config.time_window_minutes {
        state.last_alert_at = Utc::now();
        drop(state);
        info!(
          "Alert: failure rate {:.1}% exceeds threshold {:.1}%",
          failure_rate, self.config.error_threshold
        );
        return Some(failure_rate);
      }
    }
    None
  }
}
