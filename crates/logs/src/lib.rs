//! Logging and OpenTelemetry tracing configuration for Circus daemons.

use std::time::Duration;

use opentelemetry::trace::TracerProvider as _;
use opentelemetry_otlp::WithExportConfig as _;
use opentelemetry_sdk::{
  Resource,
  trace::{Sampler, SdkTracer, SdkTracerProvider},
};
use serde::{Deserialize, Serialize};
use tracing::Subscriber;
use tracing_subscriber::{
  EnvFilter,
  fmt,
  layer::SubscriberExt as _,
  registry::LookupSpan,
  util::{SubscriberInitExt as _, TryInitError},
};

const DEFAULT_OTLP_ENDPOINT: &str = "http://localhost:4317";
const SHUTDOWN_TIMEOUT: Duration = Duration::from_secs(5);

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct TracingConfig {
  pub level:           String,
  pub format:          String,
  pub show_targets:    bool,
  pub show_timestamps: bool,
  pub otlp:            OtlpConfig,
}

impl Default for TracingConfig {
  fn default() -> Self {
    Self {
      level:           "info".to_string(),
      format:          "compact".to_string(),
      show_targets:    true,
      show_timestamps: true,
      otlp:            OtlpConfig::default(),
    }
  }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct OtlpConfig {
  pub enabled:      bool,
  pub endpoint:     String,
  pub service_name: Option<String>,
  pub sample_ratio: f64,
}

impl Default for OtlpConfig {
  fn default() -> Self {
    Self {
      enabled:      false,
      endpoint:     DEFAULT_OTLP_ENDPOINT.to_owned(),
      service_name: None,
      sample_ratio: 1.0,
    }
  }
}

impl TracingConfig {
  /// Validate the optional OTLP exporter configuration.
  ///
  /// # Errors
  ///
  /// Returns an error when enabled OTLP settings cannot describe a valid
  /// exporter.
  pub fn validate(&self) -> Result<(), TracingError> {
    if !self.otlp.enabled {
      return Ok(());
    }
    if self.otlp.endpoint.trim().is_empty() {
      return Err(TracingError::InvalidConfig(
        "tracing.otlp.endpoint cannot be empty".to_owned(),
      ));
    }
    if self
      .otlp
      .service_name
      .as_deref()
      .is_some_and(|name| name.trim().is_empty())
    {
      return Err(TracingError::InvalidConfig(
        "tracing.otlp.service_name cannot be empty".to_owned(),
      ));
    }
    if !self.otlp.sample_ratio.is_finite()
      || !(0.0..=1.0).contains(&self.otlp.sample_ratio)
    {
      return Err(TracingError::InvalidConfig(format!(
        "tracing.otlp.sample_ratio must be between 0.0 and 1.0, got {}",
        self.otlp.sample_ratio
      )));
    }
    Ok(())
  }
}

#[derive(Debug, thiserror::Error)]
pub enum TracingError {
  #[error("{0}")]
  InvalidConfig(String),

  #[error("failed to create OTLP trace exporter: {0}")]
  Exporter(#[from] opentelemetry_otlp::ExporterBuildError),

  #[error("failed to install tracing subscriber: {0}")]
  Subscriber(#[from] TryInitError),
}

/// Owns the OTLP provider and flushes queued spans before the runtime exits.
#[must_use = "dropping this guard shuts down OpenTelemetry tracing"]
pub struct TracingGuard {
  provider: Option<SdkTracerProvider>,
}

impl Drop for TracingGuard {
  fn drop(&mut self) {
    let Some(provider) = self.provider.take() else {
      return;
    };
    if let Err(error) = provider.shutdown_with_timeout(SHUTDOWN_TIMEOUT) {
      tracing::warn!(%error, "failed to flush OpenTelemetry spans");
    }
  }
}

/// Initialize local logging and the optional OTLP trace exporter.
///
/// `default_service_name` identifies the daemon when no explicit
/// `tracing.otlp.service_name` is configured. `RUST_LOG` overrides the
/// configured level.
///
/// # Errors
///
/// Returns an error when the OTLP configuration, exporter, or global tracing
/// subscriber cannot be initialized.
pub fn init_tracing(
  config: &TracingConfig,
  default_service_name: &'static str,
) -> Result<TracingGuard, TracingError> {
  config.validate()?;
  let provider = build_provider(config, default_service_name)?;
  let tracer = provider.as_ref().map(|provider| provider.tracer("circus"));
  let env_filter = EnvFilter::try_from_default_env()
    .unwrap_or_else(|_| EnvFilter::new(&config.level));
  install_formatted_subscriber(config, env_filter, tracer)?;

  Ok(TracingGuard { provider })
}

fn install_formatted_subscriber(
  config: &TracingConfig,
  env_filter: EnvFilter,
  tracer: Option<SdkTracer>,
) -> Result<(), TryInitError> {
  match config.format.as_str() {
    "json" => {
      let builder = fmt()
        .json()
        .with_target(config.show_targets)
        .with_env_filter(env_filter);
      if config.show_timestamps {
        install_subscriber(builder.finish(), tracer)
      } else {
        install_subscriber(builder.without_time().finish(), tracer)
      }
    },
    "full" => {
      let builder = fmt()
        .with_target(config.show_targets)
        .with_env_filter(env_filter);
      if config.show_timestamps {
        install_subscriber(builder.finish(), tracer)
      } else {
        install_subscriber(builder.without_time().finish(), tracer)
      }
    },
    _ => {
      let builder = fmt()
        .compact()
        .with_target(config.show_targets)
        .with_env_filter(env_filter);
      if config.show_timestamps {
        install_subscriber(builder.finish(), tracer)
      } else {
        install_subscriber(builder.without_time().finish(), tracer)
      }
    },
  }
}

fn build_provider(
  config: &TracingConfig,
  default_service_name: &'static str,
) -> Result<Option<SdkTracerProvider>, TracingError> {
  if !config.otlp.enabled {
    return Ok(None);
  }
  let service_name = config
    .otlp
    .service_name
    .clone()
    .unwrap_or_else(|| default_service_name.to_owned());
  let exporter = opentelemetry_otlp::SpanExporter::builder()
    .with_tonic()
    .with_endpoint(config.otlp.endpoint.clone())
    .build()?;
  let sampler = Sampler::ParentBased(Box::new(Sampler::TraceIdRatioBased(
    config.otlp.sample_ratio,
  )));
  let resource = Resource::builder().with_service_name(service_name).build();
  let provider = SdkTracerProvider::builder()
    .with_batch_exporter(exporter)
    .with_sampler(sampler)
    .with_resource(resource)
    .build();
  Ok(Some(provider))
}

fn install_subscriber<S>(
  subscriber: S,
  tracer: Option<SdkTracer>,
) -> Result<(), TryInitError>
where
  S: Subscriber + for<'span> LookupSpan<'span> + Send + Sync + 'static,
{
  let otlp =
    tracer.map(|tracer| tracing_opentelemetry::layer().with_tracer(tracer));
  subscriber.with(otlp).try_init()
}
