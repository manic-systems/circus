#[cfg(unix)] use std::os::unix::process::CommandExt as _;
use std::{env, io};

use circus_config::EvaluatorConfig;
use tokio::process::Command;

const DEFAULT_EVIX_MEMORY_MB: usize = 4096;
const WORKER_LIMIT_ENV: &str =
  "CIRCUS_EVALUATOR_WORKER_MEMORY_LIMIT_MB";

#[derive(Debug, Clone, Copy)]
pub(crate) struct MemoryLimit(Option<u64>);

impl MemoryLimit {
  pub(crate) const fn new(limit_mb: Option<u64>) -> Self {
    Self(limit_mb)
  }

  pub(crate) fn evix_mb(self) -> io::Result<usize> {
    self.0.map_or(Ok(DEFAULT_EVIX_MEMORY_MB), |limit| {
      usize::try_from(limit)
        .map_err(|_| io::Error::other("evaluator memory limit is too large"))
    })
  }

  pub(crate) fn apply_to(self, command: &mut Command) -> io::Result<()> {
    let Some(limit_mb) = self.0 else {
      return Ok(());
    };

    #[cfg(unix)]
    {
      let limit = bytes(limit_mb)?;
      // SAFETY: the closure only calls async-signal-safe resource-limit
      // syscalls.
      unsafe {
        command.pre_exec(move || set_address_space_limit(limit));
      }
      Ok(())
    }

    #[cfg(not(unix))]
    Err(io::Error::new(
      io::ErrorKind::Unsupported,
      "evaluator memory limits require Unix",
    ))
  }

  /// Export the limit inherited by evix worker re-executions.
  ///
  /// # Safety
  ///
  /// The process must still be single-threaded because environment mutation
  /// is not thread-safe on every supported platform.
  pub(crate) unsafe fn export_worker_env(self) {
    if let Some(limit_mb) = self.0 {
      // SAFETY: guaranteed by the caller.
      unsafe { env::set_var(WORKER_LIMIT_ENV, limit_mb.to_string()) };
    } else {
      // SAFETY: guaranteed by the caller.
      unsafe { env::remove_var(WORKER_LIMIT_ENV) };
    }
  }
}

impl From<&EvaluatorConfig> for MemoryLimit {
  fn from(config: &EvaluatorConfig) -> Self {
    Self::new(config.memory_limit_mb)
  }
}

#[cfg(unix)]
fn bytes(limit_mb: u64) -> io::Result<libc::rlim_t> {
  limit_mb
    .checked_mul(1024 * 1024)
    .and_then(|value| libc::rlim_t::try_from(value).ok())
    .ok_or_else(|| io::Error::other("evaluator memory limit is too large"))
}

pub(crate) fn limit_evix_worker_from_env() -> io::Result<()> {
  let Some(value) = env::var_os(WORKER_LIMIT_ENV) else {
    return Ok(());
  };
  let value = value.to_string_lossy();
  let limit_mb = value.parse::<u64>().map_err(|error| {
    io::Error::new(
      io::ErrorKind::InvalidInput,
      format!("invalid {WORKER_LIMIT_ENV} value {value:?}: {error}"),
    )
  })?;

  #[cfg(unix)]
  return set_address_space_limit(bytes(limit_mb)?);

  #[cfg(not(unix))]
  Err(io::Error::new(
    io::ErrorKind::Unsupported,
    "evaluator memory limits require Unix",
  ))
}

#[cfg(unix)]
fn set_address_space_limit(limit: libc::rlim_t) -> io::Result<()> {
  let mut resource_limit = libc::rlimit {
    rlim_cur: 0,
    rlim_max: 0,
  };
  // SAFETY: resource_limit points to writable storage for a valid rlimit.
  if unsafe { libc::getrlimit(libc::RLIMIT_AS, &mut resource_limit) } != 0 {
    return Err(io::Error::last_os_error());
  }
  resource_limit.rlim_cur = limit.min(resource_limit.rlim_max);
  // SAFETY: resource_limit contains the existing hard limit and a soft limit
  // no greater than it.
  if unsafe { libc::setrlimit(libc::RLIMIT_AS, &resource_limit) } == 0 {
    Ok(())
  } else {
    Err(io::Error::last_os_error())
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[cfg(unix)]
  #[test]
  fn converts_mib_to_bytes() {
    assert_eq!(bytes(512).unwrap(), 512 * 1024 * 1024);
    assert!(bytes(u64::MAX).is_err());
  }

  #[test]
  fn evix_keeps_legacy_default_without_a_hard_limit() {
    assert_eq!(MemoryLimit::new(None).evix_mb().unwrap(), 4096);
    assert_eq!(MemoryLimit::new(Some(512)).evix_mb().unwrap(), 512);
  }

  #[cfg(unix)]
  #[tokio::test]
  async fn command_receives_address_space_limit() {
    let mut command = Command::new("sh");
    command.args(["-c", "ulimit -v"]);
    MemoryLimit::new(Some(64)).apply_to(&mut command).unwrap();

    let output = command.output().await.unwrap();
    assert!(output.status.success());
    assert_eq!(String::from_utf8_lossy(&output.stdout).trim(), "65536");
  }
}
