//! Configuration loading for Circus.

mod env;
mod load;
mod redact;
mod structs;
mod validation;

pub use circus_logs::TracingConfig;
#[cfg(test)]
pub(crate) use env::{apply_env_vars, parse_env_value, set_nested};
#[cfg(test)] pub(crate) use load::deep_merge;
pub use redact::redact_secrets;
pub use structs::*;
#[cfg(test)]
#[expect(clippy::unwrap_used, reason = "Fine in tests")]
mod tests {
  use std::time::Duration;

  use circus_types::GlobalRole;

  use super::*;

  #[test]
  fn test_default_config() {
    let config = Config::default();
    assert!(config.validate().is_ok());
  }

  #[test]
  fn test_invalid_database_url() {
    let mut config = Config::default();
    config.database.url = "invalid://url".to_string();
    assert!(config.validate().is_err());
  }

  #[test]
  fn test_invalid_port() {
    let mut config = Config::default();
    config.server.port = 0;
    assert!(config.validate().is_err());

    config.server.port = 65535;
    assert!(config.validate().is_ok()); // valid port
  }

  #[test]
  fn test_invalid_connections() {
    let mut config = Config::default();
    config.database.max_connections = 0;
    assert!(config.validate().is_err());

    config.database.max_connections = 10;
    config.database.min_connections = 15;
    assert!(config.validate().is_err());
  }

  #[test]
  fn test_declarative_config_default_is_empty() {
    let config = DeclarativeConfig::default();
    assert!(config.projects.is_empty());
    assert!(config.api_keys.is_empty());
  }

  #[test]
  fn test_declarative_config_deserialization() {
    let toml_str = r#"
            [[projects]]
            name = "my-project"
            repository_url = "https://github.com/test/repo"
            description = "Test project"

            [[projects.jobsets]]
            name = "packages"
            nix_expression = "packages"
            trigger_mode = "interval"

            [[api_keys]]
            name = "admin-key"
            key = "circus_secret_key_123"
            role = "admin"
        "#;
    let config: DeclarativeConfig = toml::from_str(toml_str).unwrap();
    assert_eq!(config.projects.len(), 1);
    assert_eq!(config.projects[0].name, "my-project");
    assert_eq!(config.projects[0].jobsets.len(), 1);
    assert_eq!(config.projects[0].jobsets[0].name, "packages");
    assert!(config.projects[0].jobsets[0].enabled); // default true
    assert!(config.projects[0].jobsets[0].flake_mode); // default true
    assert_eq!(
      config.projects[0].jobsets[0].trigger_mode.as_deref(),
      Some("interval")
    );
    assert_eq!(config.api_keys.len(), 1);
    assert_eq!(config.api_keys[0].role, GlobalRole::Admin);
  }

  #[test]
  fn test_page_access_config_deserialization() {
    let toml_str = r#"
            [page_access]
            evaluations = "authenticated"
            metrics = "admin"
        "#;

    let config: ServerConfig = toml::from_str(toml_str).unwrap();
    // `projects` is not set in the TOML above, so it keeps its default. The
    // secure default is `Authenticated` (the dashboard does not expose the
    // project list anonymously unless an operator opts in).
    assert_eq!(config.page_access.projects, PageAccessLevel::Authenticated);
    assert_eq!(
      config.page_access.evaluations,
      PageAccessLevel::Authenticated
    );
    assert_eq!(config.page_access.metrics, PageAccessLevel::Admin);
  }

  #[test]
  fn test_declarative_config_serialization_roundtrip() {
    let config = DeclarativeConfig {
      projects:        vec![DeclarativeProject {
        name:           "test".to_string(),
        repository_url: "https://example.com/repo".to_string(),
        description:    Some("desc".to_string()),
        cache_enabled:  true,
        cache_url:      None,
        cache_upstreams: Vec::new(),
        jobsets:        vec![DeclarativeJobset {
          name:              "checks".to_string(),
          nix_expression:    "checks".to_string(),
          enabled:           true,
          flake_mode:        true,
          check_interval:    300,
          trigger_mode:      None,
          state:             None,
          branch:            None,
          scheduling_shares: 100,
          keep_nr:           None,
          inputs:            vec![],
        }],
        notifications:  vec![],
        webhooks:       vec![],
        channels:       vec![],
        members:        vec![],
      }],
      api_keys:        vec![DeclarativeApiKey {
        name:     "test-key".to_string(),
        key:      Some("circus_test".to_string()),
        key_file: None,
        role:     GlobalRole::Admin,
      }],
      users:           vec![],
      remote_builders: vec![],
    };

    let json = serde_json::to_string(&config).unwrap();
    let parsed: DeclarativeConfig = serde_json::from_str(&json).unwrap();
    assert_eq!(parsed.projects.len(), 1);
    assert_eq!(parsed.projects[0].jobsets[0].check_interval, 300);
    assert_eq!(parsed.api_keys[0].name, "test-key");
  }

  #[test]
  fn test_declarative_config_with_main_config() {
    let config = Config::default();
    assert!(config.declarative.projects.is_empty());
    assert!(config.declarative.api_keys.is_empty());
    let toml_str = toml::to_string_pretty(&config).unwrap();
    let parsed: Config = toml::from_str(&toml_str).unwrap();
    assert!(parsed.declarative.projects.is_empty());
  }

  #[test]
  fn test_declarative_api_key_default_role_is_read_only() {
    let toml_str = r#"
            [[api_keys]]
            name = "default-key"
            key = "circus_test_123"
        "#;
    let config: DeclarativeConfig = toml::from_str(toml_str).unwrap();
    assert_eq!(config.api_keys[0].role, GlobalRole::ReadOnly);
  }

  fn table(m: toml::map::Map<String, toml::Value>) -> toml::Value {
    toml::Value::Table(m)
  }

  #[test]
  fn deep_merge_overrides_scalar() {
    let mut base = table(toml::toml! { [server] port = 3000 });
    deep_merge(&mut base, table(toml::toml! { [server] port = 8080 }));
    assert_eq!(base["server"]["port"].as_integer(), Some(8080));
  }

  #[test]
  fn deep_merge_preserves_unset_keys() {
    let mut base =
      table(toml::toml! { [server] port = 3000 host = "127.0.0.1" });
    deep_merge(&mut base, table(toml::toml! { [server] port = 8080 }));
    assert_eq!(base["server"]["port"].as_integer(), Some(8080));
    assert_eq!(base["server"]["host"].as_str(), Some("127.0.0.1"));
  }

  #[test]
  fn deep_merge_adds_new_keys() {
    let mut base = table(toml::toml! { [server] port = 3000 });
    deep_merge(&mut base, table(toml::toml! { [server] host = "0.0.0.0" }));
    assert_eq!(base["server"]["port"].as_integer(), Some(3000));
    assert_eq!(base["server"]["host"].as_str(), Some("0.0.0.0"));
  }

  #[test]
  fn parse_env_value_bool() {
    assert_eq!(parse_env_value("true"), toml::Value::Boolean(true));
    assert_eq!(parse_env_value("false"), toml::Value::Boolean(false));
    assert_eq!(parse_env_value("yes"), toml::Value::Boolean(true));
    assert_eq!(parse_env_value("no"), toml::Value::Boolean(false));
    assert_eq!(parse_env_value("TRUE"), toml::Value::Boolean(true));
    assert_eq!(parse_env_value("OFF"), toml::Value::Boolean(false));
  }

  #[test]
  fn parse_env_value_integer() {
    assert_eq!(parse_env_value("3000"), toml::Value::Integer(3000));
    assert_eq!(parse_env_value("0"), toml::Value::Integer(0));
  }

  #[test]
  fn parse_env_value_float() {
    assert_eq!(parse_env_value("1.5"), toml::Value::Float(1.5));
  }

  #[test]
  fn parse_env_value_array() {
    assert_eq!(
      parse_env_value(r#"["https", "file"]"#),
      toml::Value::Array(vec![
        toml::Value::String("https".into()),
        toml::Value::String("file".into()),
      ])
    );
  }

  #[test]
  fn parse_env_value_string() {
    assert_eq!(
      parse_env_value("hello"),
      toml::Value::String("hello".into())
    );
    assert_eq!(
      parse_env_value("postgresql://x"),
      toml::Value::String("postgresql://x".into())
    );
  }

  #[test]
  fn set_nested_single_level() {
    let mut table = toml::Value::Table(toml::map::Map::new());
    set_nested(&mut table, &["port".into()], toml::Value::Integer(8080));
    assert_eq!(table["port"].as_integer(), Some(8080));
  }

  #[test]
  fn set_nested_two_levels() {
    let mut table = toml::Value::Table(toml::map::Map::new());
    set_nested(
      &mut table,
      &["server".into(), "port".into()],
      toml::Value::Integer(8080),
    );
    assert_eq!(table["server"]["port"].as_integer(), Some(8080));
  }

  #[test]
  fn set_nested_creates_intermediate_tables() {
    let mut table = toml::Value::Table(toml::map::Map::new());
    set_nested(
      &mut table,
      &["a".into(), "b".into(), "c".into()],
      toml::Value::Boolean(true),
    );
    assert_eq!(table["a"]["b"]["c"].as_bool(), Some(true));
  }

  #[test]
  fn apply_env_vars_nested_bool_override() {
    let mut val = table(toml::toml! {
      [server]
      require_api_key_for_reads = false
      port = 3000
    });
    apply_env_vars(&mut val, [(
      "CIRCUS_SERVER__REQUIRE_API_KEY_FOR_READS".into(),
      "true".into(),
    )]);
    assert_eq!(
      val["server"]["require_api_key_for_reads"].as_bool(),
      Some(true),
    );
  }

  #[test]
  fn apply_env_vars_skips_config_file() {
    let mut val = toml::Value::Table(toml::map::Map::new());
    apply_env_vars(&mut val, [(
      "CIRCUS_CONFIG_FILE".into(),
      "/some/path".into(),
    )]);
    assert!(val.get("config_file").is_none());
  }

  #[test]
  fn apply_env_vars_skips_empty_values() {
    let mut val = table(toml::toml! { [server] host = "127.0.0.1" });
    apply_env_vars(&mut val, [("CIRCUS_SERVER__HOST".into(), String::new())]);
    assert_eq!(val["server"]["host"].as_str(), Some("127.0.0.1"));
  }

  #[test]
  fn apply_env_vars_vec_string_override() {
    let mut val = toml::Value::try_from(Config::default()).unwrap();
    apply_env_vars(&mut val, [(
      "CIRCUS_SERVER__ALLOWED_URL_SCHEMES".into(),
      r#"["https", "file"]"#.into(),
    )]);

    let config: Config = val.try_into().unwrap();
    assert_eq!(config.server.allowed_url_schemes, ["https", "file"]);
  }

  #[test]
  fn full_config_load_from_toml_and_env() {
    let toml_str = r#"
      [database]
      url = "postgresql://test:test@localhost/circus_test"

      [server]
      port = 3000
      require_api_key_for_reads = false
    "#;

    let mut table = toml::Value::try_from(Config::default()).unwrap();
    let file_table: toml::Value = toml::from_str(toml_str).unwrap();
    deep_merge(&mut table, file_table);

    // Simulate env override of the nested bool
    set_nested(
      &mut table,
      &["server".into(), "require_api_key_for_reads".into()],
      toml::Value::Boolean(true),
    );

    let config: Config = table.try_into().unwrap();
    assert_eq!(
      config.database.url,
      "postgresql://test:test@localhost/circus_test"
    );
    assert_eq!(config.server.port, 3000);
    assert!(config.server.require_api_key_for_reads);
  }

  #[test]
  fn test_unsupported_timeout_config() {
    let mut config = Config::default();
    config.queue_runner.unsupported_timeout = Some(Duration::from_hours(1));

    let toml_str = toml::to_string(&config).unwrap();
    let parsed: Config = toml::from_str(&toml_str).unwrap();
    assert_eq!(
      parsed.queue_runner.unsupported_timeout,
      Some(Duration::from_hours(1))
    );
  }

  #[test]
  fn test_unsupported_timeout_default() {
    let config = Config::default();
    assert_eq!(config.queue_runner.unsupported_timeout, None);
  }

  #[test]
  fn test_unsupported_timeout_various_formats() {
    let mut config = Config::default();
    config.queue_runner.unsupported_timeout = Some(Duration::from_mins(30));
    let toml_str = toml::to_string(&config).unwrap();
    let parsed: Config = toml::from_str(&toml_str).unwrap();
    assert_eq!(
      parsed.queue_runner.unsupported_timeout,
      Some(Duration::from_mins(30))
    );

    let mut config = Config::default();
    config.queue_runner.unsupported_timeout = Some(Duration::from_secs(0));
    let toml_str = toml::to_string(&config).unwrap();
    let parsed: Config = toml::from_str(&toml_str).unwrap();
    assert_eq!(
      parsed.queue_runner.unsupported_timeout,
      Some(Duration::from_secs(0))
    );
  }

  #[test]
  fn test_humantime_serde_parsing() {
    let toml = r#"
workers = 4
poll_interval = 5
build_timeout = 3600
work_dir = "/tmp/circus"
unsupported_timeout = "2h 30m"
    "#;

    let qr_config: QueueRunnerConfig = toml::from_str(toml).unwrap();
    assert_eq!(
      qr_config.unsupported_timeout,
      Some(Duration::from_mins(150))
    );
  }
}

#[cfg(test)]
#[expect(clippy::unwrap_used, reason = "Fine in tests")]
mod humantime_option_test {
  use super::*;

  #[test]
  fn test_option_humantime_missing() {
    let toml = r#"
workers = 4
poll_interval = 5
build_timeout = 3600
work_dir = "/tmp/circus"
        "#;
    let config: QueueRunnerConfig = toml::from_str(toml).unwrap();
    assert_eq!(config.unsupported_timeout, None);
  }
}
