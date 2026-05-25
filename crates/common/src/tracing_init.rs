//! Tracing initialization helper for all circus daemons.

use tracing_subscriber::{EnvFilter, fmt};

use crate::config::TracingConfig;

/// Initialize the global tracing subscriber based on configuration.
///
/// Respects `RUST_LOG` environment variable as an override. If `RUST_LOG` is
/// not set, falls back to the configured level.
pub fn init_tracing(config: &TracingConfig) {
  let env_filter = EnvFilter::try_from_default_env()
    .unwrap_or_else(|_| EnvFilter::new(&config.level));

  match config.format.as_str() {
    "json" => {
      let builder = fmt()
        .json()
        .with_target(config.show_targets)
        .with_env_filter(env_filter);
      if config.show_timestamps {
        builder.init();
      } else {
        builder.without_time().init();
      }
    },
    "full" => {
      let builder = fmt()
        .with_target(config.show_targets)
        .with_env_filter(env_filter);
      if config.show_timestamps {
        builder.init();
      } else {
        builder.without_time().init();
      }
    },
    _ => {
      // "compact" or any other value
      let builder = fmt()
        .compact()
        .with_target(config.show_targets)
        .with_env_filter(env_filter);
      if config.show_timestamps {
        builder.init();
      } else {
        builder.without_time().init();
      }
    },
  }
}
