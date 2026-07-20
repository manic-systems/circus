#[cfg(unix)] use std::os::unix::process::CommandExt as _;
use std::{env, io};

use tokio::process::Command;

pub(crate) const WORKER_LIMIT_ENV: &str =
  "CIRCUS_EVALUATOR_WORKER_MEMORY_LIMIT_MB";

#[cfg(unix)]
fn bytes(limit_mb: u64) -> io::Result<libc::rlim_t> {
  limit_mb
    .checked_mul(1024 * 1024)
    .and_then(|value| libc::rlim_t::try_from(value).ok())
    .ok_or_else(|| io::Error::other("evaluator memory limit is too large"))
}

pub(crate) fn limit_command(
  command: &mut Command,
  limit_mb: Option<u64>,
) -> io::Result<()> {
  let Some(limit_mb) = limit_mb else {
    return Ok(());
  };

  #[cfg(unix)]
  {
    let limit = bytes(limit_mb)?;
    // SAFETY: the closure only calls the async-signal-safe setrlimit syscall.
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

  #[cfg(unix)]
  #[tokio::test]
  async fn command_receives_address_space_limit() {
    let mut command = Command::new("sh");
    command.args(["-c", "ulimit -v"]);
    limit_command(&mut command, Some(64)).unwrap();

    let output = command.output().await.unwrap();
    assert!(output.status.success());
    assert_eq!(String::from_utf8_lossy(&output.stdout).trim(), "65536");
  }
}
