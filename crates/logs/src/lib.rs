//! Logging and OpenTelemetry tracing configuration for Circus daemons.

use std::{fmt as std_fmt, time::Duration};

use opentelemetry::trace::TracerProvider as _;
use opentelemetry_otlp::{WithExportConfig as _, WithTonicConfig as _};
use opentelemetry_sdk::{
  Resource,
  trace::{Sampler, SdkTracer, SdkTracerProvider},
};
use serde::{
  Deserialize,
  Serialize,
  de::{self, Deserializer, Visitor},
};
use tracing_subscriber::{
  EnvFilter,
  fmt,
  layer::{Layer as _, SubscriberExt as _},
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
  #[serde(deserialize_with = "deserialize_sample_ratio")]
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

fn deserialize_sample_ratio<'de, D>(deserializer: D) -> Result<f64, D::Error>
where
  D: Deserializer<'de>,
{
  struct SampleRatio;

  impl Visitor<'_> for SampleRatio {
    type Value = f64;

    fn expecting(
      &self,
      formatter: &mut std_fmt::Formatter<'_>,
    ) -> std_fmt::Result {
      formatter.write_str("a floating-point or integer sampling ratio")
    }

    fn visit_f64<E>(self, value: f64) -> Result<Self::Value, E>
    where
      E: de::Error,
    {
      Ok(value)
    }

    fn visit_i64<E>(self, value: i64) -> Result<Self::Value, E>
    where
      E: de::Error,
    {
      Ok(value as f64)
    }

    fn visit_u64<E>(self, value: u64) -> Result<Self::Value, E>
    where
      E: de::Error,
    {
      Ok(value as f64)
    }
  }

  deserializer.deserialize_any(SampleRatio)
}

impl TracingConfig {
  /// Validate the optional OTLP exporter configuration.
  ///
  /// # Errors
  ///
  /// Returns an error when enabled OTLP settings cannot describe a valid
  /// exporter.
  pub fn validate(&self) -> Result<(), TracingError> {
    EnvFilter::try_new(&self.level).map_err(|error| {
      TracingError::InvalidConfig(format!("tracing.level is invalid: {error}"))
    })?;
    if !self.otlp.enabled {
      return Ok(());
    }
    let endpoint = url::Url::parse(&self.otlp.endpoint).map_err(|error| {
      TracingError::InvalidConfig(format!(
        "tracing.otlp.endpoint must be an absolute HTTP(S) URL: {error}"
      ))
    })?;
    if !matches!(endpoint.scheme(), "http" | "https")
      || endpoint.host().is_none()
    {
      return Err(TracingError::InvalidConfig(
        "tracing.otlp.endpoint must be an absolute HTTP(S) URL".to_owned(),
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

  #[error("invalid OTLP header: {0}")]
  Header(String),

  #[error("failed to install tracing subscriber: {0}")]
  Subscriber(#[from] TryInitError),
}

/// Owns the OTLP provider and flushes queued spans before the runtime exits.
#[must_use = "call shutdown before the Tokio runtime exits"]
pub struct TracingGuard {
  provider: Option<SdkTracerProvider>,
}

impl TracingGuard {
  /// Flush queued spans without blocking a Tokio worker thread.
  pub async fn shutdown(mut self) {
    let Some(provider) = self.provider.take() else {
      return;
    };
    match tokio::task::spawn_blocking(move || flush_provider(provider)).await {
      Ok(()) => {},
      Err(error) => {
        tracing::warn!(%error, "OpenTelemetry shutdown task failed");
      },
    }
  }
}

impl Drop for TracingGuard {
  fn drop(&mut self) {
    let Some(provider) = self.provider.take() else {
      return;
    };
    tracing::warn!(
      "TracingGuard dropped without shutdown; flushing OTLP spans on fallback \
       path"
    );
    if let Ok(runtime) = tokio::runtime::Handle::try_current() {
      runtime.spawn_blocking(move || flush_provider(provider));
    } else {
      flush_provider(provider);
    }
  }
}

fn flush_provider(provider: SdkTracerProvider) {
  if let Err(error) = provider.shutdown_with_timeout(SHUTDOWN_TIMEOUT) {
    tracing::warn!(%error, "failed to flush OpenTelemetry spans");
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
  let env_filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| {
    EnvFilter::try_new(&config.level).expect("validated above")
  });
  install_formatted_subscriber(config, env_filter, tracer)?;

  Ok(TracingGuard { provider })
}

fn install_formatted_subscriber(
  config: &TracingConfig,
  env_filter: EnvFilter,
  tracer: Option<SdkTracer>,
) -> Result<(), TryInitError> {
  let fmt_layer = match (config.format.as_str(), config.show_timestamps) {
    ("json", true) => {
      fmt::layer()
        .json()
        .with_target(config.show_targets)
        .with_filter(env_filter)
        .boxed()
    },
    ("json", false) => {
      fmt::layer()
        .json()
        .without_time()
        .with_target(config.show_targets)
        .with_filter(env_filter)
        .boxed()
    },
    ("full", true) => {
      fmt::layer()
        .with_target(config.show_targets)
        .with_filter(env_filter)
        .boxed()
    },
    ("full", false) => {
      fmt::layer()
        .without_time()
        .with_target(config.show_targets)
        .with_filter(env_filter)
        .boxed()
    },
    (_, true) => {
      fmt::layer()
        .compact()
        .with_target(config.show_targets)
        .with_filter(env_filter)
        .boxed()
    },
    (_, false) => {
      fmt::layer()
        .compact()
        .without_time()
        .with_target(config.show_targets)
        .with_filter(env_filter)
        .boxed()
    },
  };
  let otlp =
    tracer.map(|tracer| tracing_opentelemetry::layer().with_tracer(tracer));
  tracing_subscriber::registry()
    .with(fmt_layer)
    .with(otlp)
    .try_init()
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
    .with_metadata(otlp_metadata()?)
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

fn otlp_metadata() -> Result<tonic::metadata::MetadataMap, TracingError> {
  use tonic::metadata::{MetadataKey, MetadataMap, MetadataValue};

  let mut metadata = MetadataMap::new();
  for headers in [
    std::env::var("OTEL_EXPORTER_OTLP_HEADERS").ok(),
    std::env::var("OTEL_EXPORTER_OTLP_TRACES_HEADERS").ok(),
  ]
  .into_iter()
  .flatten()
  {
    for header in headers.split(',') {
      let (key, value) = header.split_once('=').ok_or_else(|| {
        TracingError::Header(format!("expected key=value, got {header:?}"))
      })?;
      let key =
        MetadataKey::from_bytes(key.trim().as_bytes()).map_err(|error| {
          TracingError::Header(format!("invalid name {key:?}: {error}"))
        })?;
      let value = urlencoding::decode(value).map_err(|error| {
        TracingError::Header(format!("invalid value for {key:?}: {error}"))
      })?;
      let value = MetadataValue::try_from(value.as_ref()).map_err(|error| {
        TracingError::Header(format!("invalid value for {key:?}: {error}"))
      })?;
      metadata.insert(key, value);
    }
  }
  Ok(metadata)
}
