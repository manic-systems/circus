use circus_config::UiConfig;

use super::super::templates::UiTemplateConfig;

pub(super) fn ui() -> UiTemplateConfig {
  let config = UiConfig {
    brand_name: "Circus Preview".into(),
    brand_subtitle: "Fixture-backed frontend".into(),
    ..UiConfig::default()
  };
  UiTemplateConfig::from_config(&config)
}
