//! Configuration loading for Circus.

mod env;
mod load;
mod queue;
mod redact;
mod structs;
mod validation;

pub use circus_logs::TracingConfig;
pub use circus_types::BinaryCacheUpstream;
#[cfg(test)]
pub(crate) use env::{apply_env_vars, parse_env_value, set_nested};
#[cfg(test)] pub(crate) use load::deep_merge;
pub use queue::*;
pub use redact::redact_secrets;
pub use structs::*;

/// Resolve the public substituter URL for a per-project cache.
///
/// An explicit project URL wins. Otherwise derive
/// `<site>/projects/<project>/nix-cache/` from the configured global cache URL,
/// using the global URL only as the public site base; the global cache itself
/// does not need to be enabled for project caches to be usable.
#[must_use]
pub fn project_cache_url(
  global_cache_url: Option<&str>,
  project_name: &str,
  project_cache_url: Option<&str>,
) -> Option<String> {
  if let Some(url) = project_cache_url {
    return Some(url.to_owned());
  }
  let base = global_cache_url?.trim_end_matches('/');
  let site = base.strip_suffix("/nix-cache").unwrap_or(base);
  Some(format!("{site}/projects/{project_name}/nix-cache/"))
}

#[cfg(test)]
#[expect(clippy::unwrap_used, reason = "Fine in tests")]
mod tests {
  use std::{
    env,
    fs,
    time::{Duration, SystemTime, UNIX_EPOCH},
  };

  use circus_types::GlobalRole;

  use super::*;

  #[test]
  fn test_default_config() {
    let config = Config::default();
    assert!(config.validate().is_ok());
    assert!(!config.cache.gc.is_enabled());
  }

  #[test]
  fn cache_gc_policy_loads_from_toml() {
    let config = Config::from_toml_with_defaults(
      r#"
        [cache.gc]
        max_size_bytes = 10737418240
        target_size_bytes = 8589934592
        max_age_days = 45
        cleanup_interval = 900

        [cache_upload]
        store_uri = "s3://circus-cache"

        [cache_upload.s3]
        access_key_id = "test-access-key"
        secret_access_key = "test-secret-key"
      "#,
    )
    .unwrap();

    assert_eq!(config.cache.gc.max_size_bytes, Some(10_737_418_240));
    assert_eq!(config.cache.gc.target_size_bytes, Some(8_589_934_592));
    assert_eq!(config.cache.gc.max_age_days, Some(45));
    assert_eq!(config.cache.gc.cleanup_interval, 900);
  }

  #[test]
  fn cache_gc_rejects_invalid_or_unenforceable_policies() {
    let invalid_target = Config::from_toml_with_defaults(
      r"
        [cache.gc]
        max_size_bytes = 100
        target_size_bytes = 100
      ",
    )
    .unwrap_err()
    .to_string();
    assert!(invalid_target.contains("target_size_bytes must be less"));

    let missing_storage = Config::from_toml_with_defaults(
      r"
        [cache.gc]
        max_age_days = 30
      ",
    )
    .unwrap_err()
    .to_string();
    assert!(missing_storage.contains("requires an S3 cache_upload.store_uri"));
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
  }

  #[test]
  fn test_declarative_config_default_is_empty() {
    let config = DeclarativeConfig::default();
    assert!(!config.allow_runtime_mutation);
    assert!(config.projects.is_empty());
    assert!(config.api_keys.is_empty());
  }

  #[test]
  fn test_declarative_runtime_mutation_can_be_enabled() {
    let config: DeclarativeConfig =
      toml::from_str("allow_runtime_mutation = true").unwrap();
    assert!(config.allow_runtime_mutation);
  }

  #[test]
  fn test_declarative_config_deserialization() {
    let toml_str = r#"
            [[projects]]
            name = "my-project"
            repository_url = "https://github.com/test/repo"
            description = "Test project"
            allow_runtime_mutation = true

            [[projects.jobsets]]
            name = "packages"
            nix_expression = "packages"
            trigger_mode = "source_change"
            only_build_latest = true
            path_filters = ["packages/**", "flake.nix"]
            systems = ["x86_64-linux"]

            [[api_keys]]
            name = "admin-key"
            key = "circus_secret_key_123"
            role = "admin"
        "#;
    let config: DeclarativeConfig = toml::from_str(toml_str).unwrap();
    assert_eq!(config.projects.len(), 1);
    assert_eq!(config.projects[0].name, "my-project");
    assert_eq!(config.projects[0].allow_runtime_mutation, Some(true));
    assert_eq!(config.projects[0].jobsets.len(), 1);
    assert_eq!(config.projects[0].jobsets[0].name, "packages");
    assert!(config.projects[0].jobsets[0].enabled); // default true
    assert!(config.projects[0].jobsets[0].flake_mode); // default true
    assert_eq!(
      config.projects[0].jobsets[0].trigger_mode.as_deref(),
      Some("source_change")
    );
    assert!(config.projects[0].jobsets[0].only_build_latest);
    assert_eq!(config.projects[0].jobsets[0].path_filters, [
      "packages/**",
      "flake.nix"
    ]);
    assert_eq!(
      config.projects[0].jobsets[0].systems.as_deref(),
      Some(["x86_64-linux".to_string()].as_slice())
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
      allow_runtime_mutation: false,
      projects:               vec![DeclarativeProject {
        name:                   "test".to_string(),
        repository_url:         "https://example.com/repo".to_string(),
        description:            Some("desc".to_string()),
        allow_runtime_mutation: None,
        cache_enabled:          true,
        cache_url:              None,
        cache_upstreams:        Vec::new(),
        jobsets:                vec![DeclarativeJobset {
          name:              "checks".to_string(),
          nix_expression:    "checks".to_string(),
          enabled:           true,
          flake_mode:        true,
          check_interval:    300,
          trigger_mode:      None,
          state:             None,
          branch:            None,
          branch_pattern:    None,
          tag_pattern:       None,
          scheduling_shares: 100,
          keep_nr:           None,
          systems:           Some(vec!["x86_64-linux".to_string()]),
          only_build_latest: false,
          path_filters:      Vec::new(),
          inputs:            vec![],
        }],
        notifications:          vec![],
        webhooks:               vec![],
        channels:               vec![],
        members:                vec![],
      }],
      api_keys:               vec![DeclarativeApiKey {
        name:     "test-key".to_string(),
        key:      Some("circus_test".to_string()),
        key_file: None,
        role:     GlobalRole::Admin,
      }],
      users:                  vec![],
    };

    let json = serde_json::to_string(&config).unwrap();
    let parsed: DeclarativeConfig = serde_json::from_str(&json).unwrap();
    assert_eq!(parsed.projects.len(), 1);
    assert_eq!(parsed.projects[0].jobsets[0].check_interval, 300);
    assert_eq!(
      parsed.projects[0].jobsets[0].systems.as_deref(),
      Some(["x86_64-linux".to_string()].as_slice())
    );
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
  fn evaluator_allowed_uris_load_from_toml() {
    let toml_str = r#"
      [evaluator]
      restrict_eval = true
      allowed_uris = ["https://releases.nixos.org", "https://github.com"]
    "#;

    let mut table = toml::Value::try_from(Config::default()).unwrap();
    let file_table: toml::Value = toml::from_str(toml_str).unwrap();
    deep_merge(&mut table, file_table);

    let config: Config = table.try_into().unwrap();
    assert!(config.evaluator.restrict_eval);
    assert_eq!(config.evaluator.allowed_uris, vec![
      "https://releases.nixos.org",
      "https://github.com"
    ]);
  }

  #[test]
  fn evaluator_memory_limit_loads_from_toml() {
    let toml_str = r"
      [evaluator]
      memory_limit_mb = 3072
    ";

    let mut table = toml::Value::try_from(Config::default()).unwrap();
    let file_table: toml::Value = toml::from_str(toml_str).unwrap();
    deep_merge(&mut table, file_table);

    let config: Config = table.try_into().unwrap();
    assert_eq!(config.evaluator.memory_limit_mb, Some(3072));
  }

  #[test]
  fn evaluator_memory_limit_rejects_invalid_values() {
    let mut config = Config::default();
    config.evaluator.memory_limit_mb = Some(0);
    assert!(config.validate().is_err());

    config.evaluator.memory_limit_mb = Some(u64::MAX);
    assert!(config.validate().is_err());
  }

  #[test]
  fn load_requires_explicit_config_path() {
    let old = env::var_os("CIRCUS_CONFIG_FILE");
    // SAFETY: tests in this module run single-threaded with respect to this
    // env var; no other thread reads or writes CIRCUS_CONFIG_FILE concurrently.
    unsafe {
      env::remove_var("CIRCUS_CONFIG_FILE");
    }

    let err = Config::load(None).unwrap_err().to_string();
    assert!(err.contains("configuration file is required"));

    if let Some(value) = old {
      // SAFETY: see above; restoring the original value, still single-threaded.
      unsafe {
        env::set_var("CIRCUS_CONFIG_FILE", value);
      }
    }
  }

  #[test]
  fn load_reads_explicit_config_path() {
    let path = env::temp_dir().join(format!(
      "circus-config-test-{}.toml",
      SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos()
    ));
    fs::write(&path, "[server]\nport = 4321\n").unwrap();

    let config = Config::load(Some(&path)).unwrap();
    assert_eq!(config.server.port, 4321);

    let _ = fs::remove_file(path);
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
