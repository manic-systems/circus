use std::{
  ffi::OsString,
  path::{Path, PathBuf},
  time::Duration,
};

use circus_common::{CiError, error::Result};
use tokio::{
  io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
  process::Command,
  task::JoinHandle,
};

const MAX_LOG_SIZE: usize = 100 * 1024 * 1024; // 100MB

/// SSH options for every remote build: use only the configured identity (no
/// agent or `~/.ssh` keys), never prompt, and fail fast on a dead host.
const SSH_HARDENING_OPTS: &[&str] = &[
  "-o",
  "IdentitiesOnly=yes",
  "-o",
  "IdentityAgent=none",
  "-o",
  "BatchMode=yes",
  "-o",
  "ConnectTimeout=30",
];

/// Run a nix build on a remote builder via SSH.
///
/// With `public_host_key` set, the key is pinned via a throwaway `known_hosts`
/// and `StrictHostKeyChecking=yes`; without it, the connection falls back to
/// `accept-new` (see `ssh_require_host_key`).
///
/// # Errors
///
/// Returns error if the `known_hosts` write fails, or the build fails or times
/// out.
#[tracing::instrument(
  skip(work_dir, live_log_path, public_host_key),
  fields(drv_path, store_uri, host_key_pinned = public_host_key.is_some())
)]
pub async fn run_nix_build_remote(
  drv_path: &str,
  work_dir: &Path,
  timeout: Duration,
  store_uri: &str,
  ssh_key_file: Option<&str>,
  public_host_key: Option<&str>,
  live_log_path: Option<&Path>,
  extra_args: &[String],
) -> Result<BuildResult> {
  let mut args = common_nix_build_args(drv_path, extra_args);
  args.splice(args.len() - 1..args.len() - 1, [
    "--store".into(),
    store_uri.into(),
  ]);

  // Hold the guard until the build returns; it must outlive the ssh process.
  let known_hosts = if let Some(key) = public_host_key {
    Some(write_known_hosts(store_uri, key)?)
  } else {
    tracing::warn!(
      store_uri,
      "remote builder has no pinned public_host_key; connecting with \
       StrictHostKeyChecking=accept-new (trust on first use)"
    );
    None
  };

  let ssh_opts = build_ssh_opts(ssh_key_file, known_hosts.as_ref())?;

  let result = run_nix_build_command(
    args,
    work_dir,
    timeout,
    live_log_path,
    "remote nix build",
    |cmd| {
      cmd.env("NIX_SSHOPTS", ssh_opts);
    },
  )
  .await;

  drop(known_hosts);
  result
}

/// Assemble the `NIX_SSHOPTS` string, pinning the host key when `known_hosts`
/// is present and falling back to `accept-new` otherwise.
fn build_ssh_opts(
  ssh_key_file: Option<&str>,
  known_hosts: Option<&tempfile::NamedTempFile>,
) -> Result<String> {
  let mut opts: Vec<String> = SSH_HARDENING_OPTS
    .iter()
    .map(|s| (*s).to_string())
    .collect();

  if let Some(key_file) = ssh_key_file {
    // NIX_SSHOPTS is whitespace-split with no quoting
    if key_file.contains(char::is_whitespace) {
      return Err(CiError::Build(format!(
        "ssh_key_file contains whitespace, unrepresentable in NIX_SSHOPTS: \
         {key_file}"
      )));
    }
    opts.push("-i".into());
    opts.push(key_file.into());
  }

  opts.push("-o".into());
  if let Some(file) = known_hosts {
    let path = file.path().display().to_string();
    if path.contains(char::is_whitespace) {
      return Err(CiError::Build(format!(
        "known_hosts temp path contains whitespace, unrepresentable in \
         NIX_SSHOPTS: {path}"
      )));
    }
    opts.push("StrictHostKeyChecking=yes".into());
    opts.push("-o".into());
    opts.push(format!("UserKnownHostsFile={path}"));
  } else {
    opts.push("StrictHostKeyChecking=accept-new".into());
  }

  Ok(opts.join(" "))
}

/// Write a builder's recorded host key to a throwaway `known_hosts` file.
///
/// The stored value may be a full `known_hosts` line (`<host> <type> <b64>`)
/// or a bare key (`<type> <b64>`); in the latter case the host pattern is
/// derived from `store_uri` so SSH matches it against the connection.
fn write_known_hosts(
  store_uri: &str,
  public_host_key: &str,
) -> Result<tempfile::NamedTempFile> {
  use std::io::Write as _;

  let key = public_host_key.trim();
  let line = if host_pattern_prefix(key) {
    // Already begins with a host pattern; use verbatim.
    format!("{key}\n")
  } else {
    let host = ssh_host_from_store_uri(store_uri);
    format!("{host} {key}\n")
  };

  let mut file = tempfile::Builder::new()
    .prefix("circus-known-hosts-")
    .tempfile()
    .map_err(|e| {
      CiError::Build(format!("Failed to create known_hosts file: {e}"))
    })?;
  file.write_all(line.as_bytes()).map_err(|e| {
    CiError::Build(format!("Failed to write known_hosts file: {e}"))
  })?;
  file.flush().map_err(|e| {
    CiError::Build(format!("Failed to flush known_hosts file: {e}"))
  })?;
  Ok(file)
}

/// True if the stored host key already starts with a host pattern rather than
/// a bare key type (`ssh-ed25519`, `ecdsa-...`, `ssh-rsa`, `sk-...`).
fn host_pattern_prefix(key: &str) -> bool {
  let first = key.split_whitespace().next().unwrap_or("");
  !(first.starts_with("ssh-")
    || first.starts_with("ecdsa-")
    || first.starts_with("sk-"))
}

/// Extract the host (with port in the `[host]:port` form SSH expects) from a
/// `ssh://`/`ssh-ng://`/`user@host:port` store URI for use as a `known_hosts`
/// pattern.
fn ssh_host_from_store_uri(store_uri: &str) -> String {
  let after_scheme = store_uri
    .strip_prefix("ssh://")
    .or_else(|| store_uri.strip_prefix("ssh-ng://"))
    .unwrap_or(store_uri);
  let after_user = after_scheme
    .split_once('@')
    .map_or(after_scheme, |(_, rest)| rest);
  // Drop any query string / store params.
  let host_port = after_user.split(['?', '/']).next().unwrap_or(after_user);
  if host_port.starts_with('[') {
    return host_port.to_string();
  }
  match host_port.rsplit_once(':') {
    Some((host, port))
      if !port.is_empty() && port.chars().all(|c| c.is_ascii_digit()) =>
    {
      format!("[{host}]:{port}")
    },
    _ => host_port.to_string(),
  }
}

pub struct BuildResult {
  pub success:              bool,
  pub exit_code:            Option<i32>,
  pub stdout:               String,
  pub stderr:               String,
  pub output_paths:         Vec<String>,
  pub sub_steps:            Vec<SubStep>,
  pub cache_upload_handled: bool,
}

/// A sub-step parsed from nix's internal JSON log format.
pub struct SubStep {
  pub drv_path:     String,
  pub completed_at: Option<chrono::DateTime<chrono::Utc>>,
  pub success:      bool,
}

/// Parse a single nix internal JSON log line (`@nix {...}`).
///
/// # Returns
///
/// Returns `Some(action, drv_path)` if the line contains a derivation action.
#[must_use]
pub fn parse_nix_log_line(line: &str) -> Option<(&'static str, String)> {
  let json_str = line.strip_prefix("@nix ")?.trim();
  let parsed: serde_json::Value = serde_json::from_str(json_str).ok()?;
  let action = parsed.get("action")?.as_str()?;
  let drv = parsed.get("derivation")?.as_str()?.to_string();

  match action {
    "start" => Some(("start", drv)),
    "stop" => Some(("stop", drv)),
    _ => None,
  }
}

/// Run `nix build` for a derivation path.
/// If `live_log_path` is provided, build output is streamed to that file
/// incrementally.
///
/// # Errors
///
/// Returns error if nix build command fails or times out.
#[tracing::instrument(skip(work_dir, live_log_path), fields(drv_path))]
pub async fn run_nix_build(
  drv_path: &str,
  work_dir: &Path,
  timeout: Duration,
  live_log_path: Option<&Path>,
  extra_args: &[String],
) -> Result<BuildResult> {
  run_nix_build_command(
    common_nix_build_args(drv_path, extra_args),
    work_dir,
    timeout,
    live_log_path,
    "nix build",
    |_| {},
  )
  .await
}

fn common_nix_build_args(drv_path: &str, extra: &[String]) -> Vec<OsString> {
  // Build every output of the derivation via the "^*" selector. A bare ".drv"
  // path is treated by `nix build` as a store path to realise (a no-op that
  // emits the .drv path rather than building it), so the outputs would never
  // be produced and --print-out-paths would not emit real output paths.
  let installable = format!("{drv_path}^*");
  let defaults: [&str; 11] = [
    "build",
    "--no-link",
    "--print-out-paths",
    "--log-format",
    "internal-json",
    "--option",
    "sandbox",
    "true",
    "--max-build-log-size",
    "104857600",
    installable.as_str(),
  ];
  // Operator-supplied args go before the installable so they remain valid
  // overrides (nix's `--option` resolution lets later flags win), while the
  // installable stays in last position where `nix build` requires it.
  let mut args: Vec<OsString> =
    Vec::with_capacity(defaults.len() + extra.len());
  for s in &defaults[..defaults.len() - 1] {
    args.push(OsString::from(*s));
  }
  for s in extra {
    args.push(OsString::from(s));
  }
  args.push(OsString::from(installable));
  args
}

async fn run_nix_build_command(
  args: Vec<OsString>,
  work_dir: &Path,
  timeout: Duration,
  live_log_path: Option<&Path>,
  operation: &'static str,
  configure: impl FnOnce(&mut Command),
) -> Result<BuildResult> {
  let result = tokio::time::timeout(timeout, async {
    let mut cmd = Command::new("nix");
    cmd
      .args(args)
      .current_dir(work_dir)
      .kill_on_drop(true)
      .stdout(std::process::Stdio::piped())
      .stderr(std::process::Stdio::piped());
    configure(&mut cmd);

    let mut child = cmd
      .spawn()
      .map_err(|e| CiError::Build(format!("Failed to run {operation}: {e}")))?;

    let stdout_handle = child.stdout.take();
    let stderr_handle = child.stderr.take();

    let stdout_task = read_stdout(stdout_handle);
    let stderr_task =
      read_stderr(stderr_handle, live_log_path.map(Path::to_path_buf));

    let stdout_buf = join_output(stdout_task, "stdout reader").await?;
    let (stderr_buf, sub_steps) =
      join_output(stderr_task, "stderr reader").await?;

    let status = child.wait().await.map_err(|e| {
      CiError::Build(format!("Failed to wait for {operation}: {e}"))
    })?;

    let output_paths: Vec<String> = stdout_buf
      .lines()
      .map(|s| s.trim().to_string())
      .filter(|s| !s.is_empty())
      .collect();

    Ok::<_, CiError>(BuildResult {
      success: status.success(),
      exit_code: status.code(),
      stdout: stdout_buf,
      stderr: stderr_buf,
      output_paths,
      sub_steps,
      cache_upload_handled: false,
    })
  })
  .await;

  result.unwrap_or_else(|_| {
    Err(CiError::Timeout(format!(
      "{operation} timed out after {timeout:?}"
    )))
  })
}

fn read_stdout(
  stdout: Option<tokio::process::ChildStdout>,
) -> JoinHandle<Result<String>> {
  tokio::spawn(async move {
    let mut buf = String::new();
    if let Some(stdout) = stdout {
      let mut reader = BufReader::new(stdout);
      let mut line = String::new();
      while reader.read_line(&mut line).await.map_err(|e| {
        CiError::Build(format!("Failed to read nix stdout: {e}"))
      })?
        > 0
      {
        buf.push_str(&line);
        line.clear();
      }
    }
    Ok(buf)
  })
}

fn read_stderr(
  stderr: Option<tokio::process::ChildStderr>,
  live_log_path: Option<PathBuf>,
) -> JoinHandle<Result<(String, Vec<SubStep>)>> {
  tokio::spawn(async move {
    let mut buf = String::new();
    let mut steps: Vec<SubStep> = Vec::new();
    let mut log_file = if let Some(ref path) = live_log_path {
      match tokio::fs::File::create(path).await {
        Ok(file) => Some(file),
        Err(e) => {
          tracing::warn!(
            path = %path.display(),
            "Failed to create live build log: {e}"
          );
          None
        },
      }
    } else {
      None
    };
    let mut logged_write_error = false;

    if let Some(stderr) = stderr {
      let mut reader = BufReader::new(stderr);
      let mut line = String::new();
      while reader.read_line(&mut line).await.map_err(|e| {
        CiError::Build(format!("Failed to read nix stderr: {e}"))
      })?
        > 0
      {
        if let Some(ref mut file) = log_file
          && let Err(e) = write_live_log_line(file, &line).await
          && !logged_write_error
        {
          tracing::warn!("Failed to write live build log: {e}");
          logged_write_error = true;
        }

        if let Some((action, drv_path)) = parse_nix_log_line(&line) {
          update_sub_steps(&mut steps, action, drv_path);
        }

        if buf.len() < MAX_LOG_SIZE {
          buf.push_str(&line);
        }
        line.clear();
      }
    }

    Ok((buf, steps))
  })
}

async fn write_live_log_line(
  file: &mut tokio::fs::File,
  line: &str,
) -> std::io::Result<()> {
  file.write_all(line.as_bytes()).await?;
  file.flush().await
}

fn update_sub_steps(steps: &mut Vec<SubStep>, action: &str, drv_path: String) {
  match action {
    "start" => {
      steps.push(SubStep {
        drv_path,
        completed_at: None,
        success: false,
      });
    },
    "stop" => {
      if let Some(step) = steps.iter_mut().rfind(|s| s.drv_path == drv_path) {
        step.completed_at = Some(chrono::Utc::now());
        step.success = true;
      }
    },
    _ => {},
  }
}

async fn join_output<T>(task: JoinHandle<Result<T>>, label: &str) -> Result<T> {
  task
    .await
    .map_err(|e| CiError::Build(format!("Nix {label} task failed: {e}")))?
}

#[cfg(test)]
#[expect(clippy::unwrap_used, reason = "Fine in tests")]
mod tests {
  use super::*;

  #[test]
  fn ssh_opts_pins_host_key_when_present() {
    let kh = write_known_hosts(
      "ssh://builder.example.com",
      "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAITESTKEY",
    )
    .unwrap();
    let opts =
      build_ssh_opts(Some("/var/lib/circus/id_ed25519"), Some(&kh)).unwrap();
    assert!(opts.contains("StrictHostKeyChecking=yes"));
    assert!(opts.contains("UserKnownHostsFile="));
    assert!(opts.contains("IdentitiesOnly=yes"));
    assert!(opts.contains("IdentityAgent=none"));
    assert!(opts.contains("BatchMode=yes"));
    assert!(opts.contains("-i /var/lib/circus/id_ed25519"));
    assert!(!opts.contains("accept-new"));
  }

  #[test]
  fn ssh_opts_fall_back_to_accept_new_without_host_key() {
    let opts = build_ssh_opts(Some("/key"), None).unwrap();
    assert!(opts.contains("StrictHostKeyChecking=accept-new"));
    assert!(!opts.contains("UserKnownHostsFile="));
    assert!(opts.contains("IdentitiesOnly=yes"));
  }

  #[test]
  fn ssh_opts_without_key_file_omits_identity_flag() {
    let opts = build_ssh_opts(None, None).unwrap();
    assert!(!opts.contains("-i "));
    assert!(opts.contains("BatchMode=yes"));
  }

  #[test]
  fn known_hosts_prefixes_bare_key_with_host() {
    let kh = write_known_hosts(
      "ssh://root@builder.example.com:2222",
      "ssh-ed25519 AAAAKEY",
    )
    .unwrap();
    let contents = std::fs::read_to_string(kh.path()).unwrap();
    assert_eq!(contents, "[builder.example.com]:2222 ssh-ed25519 AAAAKEY\n");
  }

  #[test]
  fn known_hosts_keeps_full_line_verbatim() {
    let kh = write_known_hosts(
      "ssh://builder.example.com",
      "builder.example.com ssh-ed25519 AAAAKEY",
    )
    .unwrap();
    let contents = std::fs::read_to_string(kh.path()).unwrap();
    assert_eq!(contents, "builder.example.com ssh-ed25519 AAAAKEY\n");
  }

  #[test]
  fn host_from_store_uri_variants() {
    assert_eq!(
      ssh_host_from_store_uri("ssh://root@host.example:22"),
      "[host.example]:22"
    );
    assert_eq!(
      ssh_host_from_store_uri("ssh-ng://host.example"),
      "host.example"
    );
    assert_eq!(ssh_host_from_store_uri("user@host.example"), "host.example");
    assert_eq!(
      ssh_host_from_store_uri("ssh://host.example?compress=true"),
      "host.example"
    );
    assert_eq!(
      ssh_host_from_store_uri("ssh://[2001:db8::1]:22"),
      "[2001:db8::1]:22"
    );
    assert_eq!(
      ssh_host_from_store_uri("ssh://root@[2001:db8::1]"),
      "[2001:db8::1]"
    );
  }

  #[test]
  fn host_pattern_prefix_detection() {
    assert!(!host_pattern_prefix("ssh-ed25519 AAAA"));
    assert!(!host_pattern_prefix("ecdsa-sha2-nistp256 AAAA"));
    assert!(!host_pattern_prefix("sk-ssh-ed25519@openssh.com AAAA"));
    assert!(host_pattern_prefix("host.example ssh-ed25519 AAAA"));
    assert!(host_pattern_prefix("[host.example]:22 ssh-ed25519 AAAA"));
  }
}
