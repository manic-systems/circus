//! Logging configuration and initialization for Circus daemons.

use serde::{Deserialize, Serialize};
use tracing_subscriber::{
  EnvFilter,
  fmt,
  fmt::{format::JsonFields, time::SystemTime},
};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct TracingConfig {
  pub level:           String,
  pub format:          String,
  pub show_targets:    bool,
  pub show_timestamps: bool,
}

impl Default for TracingConfig {
  fn default() -> Self {
    Self {
      level:           "info".to_string(),
      format:          "compact".to_string(),
      show_targets:    true,
      show_timestamps: true,
    }
  }
}

/// Initialize the global tracing subscriber based on configuration.
///
/// Respects `RUST_LOG` environment variable as an override. If `RUST_LOG` is
/// not set, falls back to the configured level.
pub fn init_tracing(config: &TracingConfig) {
  let env_filter = EnvFilter::try_from_default_env()
    .unwrap_or_else(|_| EnvFilter::new(&config.level));

  let format = fmt::format().with_target(config.show_targets);
  // Swapping the timer back keeps one instantiation per format.
  let format = if config.show_timestamps {
    format
  } else {
    format.without_time().with_timer(SystemTime)
  };
  let builder = fmt().with_env_filter(env_filter);

  match config.format.as_str() {
    "json" => {
      builder
        .event_format(format.json())
        .fmt_fields(JsonFields::new())
        .init();
    },
    "full" => builder.event_format(format).init(),
    // "compact" or any other value
    _ => builder.event_format(format.compact()).init(),
  }
}
