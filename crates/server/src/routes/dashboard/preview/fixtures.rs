use circus_common::models::{BinaryCacheUpstreams, Project};
use circus_config::UiConfig;
use jiff::{SignedDuration, Timestamp};
use uuid::Uuid;

use super::super::templates::UiTemplateConfig;

pub(super) const fn id(n: u128) -> Uuid {
  Uuid::from_u128(n)
}

pub(super) fn ui() -> UiTemplateConfig {
  let config = UiConfig {
    brand_name: "Circus Preview".into(),
    brand_subtitle: "Fixture-backed frontend".into(),
    ..UiConfig::default()
  };
  UiTemplateConfig::from_config(&config)
}

pub(super) fn csrf() -> String {
  "preview-csrf-token".into()
}

pub(super) fn project_fixture() -> Project {
  Project {
    id:                     id(1),
    name:                   "circus".into(),
    description:            Some("Nix-native CI control plane".into()),
    repository_url:         "https://github.com/manic-systems/circus".into(),
    cache_enabled:          true,
    cache_url:              Some("https://cache.example.invalid".into()),
    cache_upstreams:        BinaryCacheUpstreams::default(),
    managed_declaratively:  false,
    allow_runtime_mutation: None,
    created_at:             Timestamp::now()
      - SignedDuration::from_hours(24 * 30),
    updated_at:             Timestamp::now() - SignedDuration::from_mins(5),
  }
}
