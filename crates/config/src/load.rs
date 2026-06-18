use std::{
  fs,
  path::{Path, PathBuf},
};

use color_eyre::eyre::{self, WrapErr, bail};
use config as config_crate;

use crate::{Config, env::apply_env_vars};

impl Config {
  /// This matches normal file loading semantics without environment overrides,
  /// making it suitable for the admin config editor: partial config files are
  /// expanded before validation and saving.
  ///
  /// # Errors
  ///
  /// Returns an error if TOML parsing, deserialization, or validation fails.
  pub fn from_toml_with_defaults(contents: &str) -> eyre::Result<Self> {
    let settings = config_crate::Config::builder()
      .add_source(config_crate::Config::try_from(&Self::default())?)
      .add_source(config_crate::File::from_str(
        contents,
        config_crate::FileFormat::Toml,
      ));
    let config = settings.build()?.try_deserialize::<Self>()?;
    config.validate()?;
    Ok(config)
  }

  /// Resolve `*_file` secret fields by reading their file contents at startup.
  ///
  /// For `Option<String>` fields the inline value takes precedence; the file
  /// is read only when the inline value is `None`. For required fields
  /// (`database.url`) the file overrides unconditionally since the field
  /// always has a compiled default.
  ///
  /// # Errors
  ///
  /// Returns an error if a configured file path cannot be read or is empty.
  fn resolve_secret_files(&mut self) -> eyre::Result<()> {
    fn read_secret(path: &Path) -> eyre::Result<String> {
      let content = fs::read_to_string(path).wrap_err_with(|| {
        format!("failed to read secret from {}", path.display())
      })?;
      let trimmed = content.trim().to_owned();
      if trimmed.is_empty() {
        bail!("secret file is empty: {}", path.display());
      }
      Ok(trimmed)
    }

    macro_rules! resolve_optional {
      ($field:expr, $file_field:expr) => {
        if $field.is_none() {
          if let Some(ref path) = $file_field {
            $field = Some(read_secret(path)?);
          }
        }
      };
    }

    // database.url: file overrides (url always carries a compiled default)
    if let Some(ref path) = self.database.url_file {
      self.database.url = read_secret(path)?;
    }

    // server
    resolve_optional!(self.server.api_key, self.server.api_key_file);
    resolve_optional!(
      self.server.webhook_secret_encryption_key,
      self.server.webhook_secret_encryption_key_file
    );

    // notifications
    resolve_optional!(
      self.notifications.webhook_url,
      self.notifications.webhook_url_file
    );
    resolve_optional!(
      self.notifications.github_token,
      self.notifications.github_token_file
    );
    resolve_optional!(
      self.notifications.gitea_token,
      self.notifications.gitea_token_file
    );
    resolve_optional!(
      self.notifications.gitlab_token,
      self.notifications.gitlab_token_file
    );

    // email (nested inside notifications)
    if let Some(ref mut email) = self.notifications.email {
      resolve_optional!(email.smtp_password, email.smtp_password_file);
    }

    if let Some(ref mut github) = self.oauth.github
      && github.client_secret.is_empty()
      && let Some(ref path) = github.client_secret_file
    {
      github.client_secret = read_secret(path)?;
    }

    if let Some(ref mut slack) = self.notifications.slack
      && slack.webhook_url.is_empty()
      && let Some(ref path) = slack.webhook_url_file
    {
      slack.webhook_url = read_secret(path)?;
    }

    // s3 (nested inside cache_upload)
    if let Some(ref mut s3) = self.cache_upload.s3 {
      resolve_optional!(s3.secret_access_key, s3.secret_access_key_file);
      resolve_optional!(s3.session_token, s3.session_token_file);
    }

    Ok(())
  }

  /// Load configuration from an explicit file and environment variables.
  ///
  /// Merges three layers (later wins):
  ///
  /// 1. Compiled defaults (`Config::default()`)
  /// 2. TOML config file from `path` or `CIRCUS_CONFIG_FILE`
  /// 3. `CIRCUS_*` environment variables (`__` = nesting separator)
  ///
  /// # Errors
  ///
  /// Returns error if configuration loading or validation fails.
  pub fn load(path: Option<&Path>) -> eyre::Result<Self> {
    let mut table = toml::Value::try_from(Self::default())
      .wrap_err("failed to serialize config defaults")?;

    let config_path = match path {
      Some(path) => Some(path.to_path_buf()),
      None => std::env::var_os("CIRCUS_CONFIG_FILE").map(PathBuf::from),
    }
    .ok_or_else(|| {
      eyre::eyre!(
        "configuration file is required; pass --config or set \
         CIRCUS_CONFIG_FILE"
      )
    })?;

    let contents = fs::read_to_string(&config_path).wrap_err_with(|| {
      format!("failed to read config file {}", config_path.display())
    })?;
    let file_table: toml::Value =
      toml::from_str(&contents).wrap_err("failed to parse config file")?;
    deep_merge(&mut table, file_table);

    apply_env_vars(&mut table, std::env::vars());

    let mut config: Self =
      table.try_into().wrap_err("failed to deserialize config")?;

    config.resolve_secret_files()?;
    config.validate()?;

    Ok(config)
  }
}

/// Recursively merge `overlay` into `base`. Tables merge key-by-key;
/// scalars and arrays are replaced wholesale.
pub fn deep_merge(base: &mut toml::Value, overlay: toml::Value) {
  match (base, overlay) {
    (toml::Value::Table(base_t), toml::Value::Table(over_t)) => {
      for (key, value) in over_t {
        match base_t.entry(key) {
          toml::map::Entry::Occupied(mut e) => {
            deep_merge(e.get_mut(), value);
          },
          toml::map::Entry::Vacant(e) => {
            e.insert(value);
          },
        }
      }
    },
    (base, overlay) => *base = overlay,
  }
}
