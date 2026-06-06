//! Rootless sandbox for direct-store Nix builds.

use std::{
  ffi::{OsStr, OsString},
  fs,
  io,
  os::{fd::OwnedFd, unix::process::CommandExt},
  path::{Path, PathBuf},
  process::Stdio,
};

use nix::{
  fcntl::OFlag,
  mount::{MntFlags, MsFlags, mount, umount2},
  sched::{CloneFlags, unshare},
  sys::wait::{WaitStatus, waitpid},
  unistd::{
    ForkResult,
    Gid,
    Uid,
    chdir,
    fork,
    getppid,
    pipe2,
    pivot_root,
    read,
    write,
  },
};
use tokio::process::Command;

const HELPER_ARG: &str = "--circus-sandbox";
const NIX_ENV: &str = "CIRCUS_AGENT_NIX";
pub const DATA_DIR_ENV: &str = "CIRCUS_AGENT_DATA_DIR";

#[derive(Clone, Copy)]
pub(crate) enum NixTool {
  Nix,
  NixStore,
}

impl NixTool {
  const fn name(self) -> &'static str {
    match self {
      Self::Nix => "nix",
      Self::NixStore => "nix-store",
    }
  }

  /// The tool a binary's filename names, if any.
  fn from_filename(file: &OsStr) -> Option<Self> {
    [Self::Nix, Self::NixStore]
      .into_iter()
      .find(|t| file == OsStr::new(t.name()))
  }
}

#[derive(Debug)]
pub enum Error {
  Io {
    op:     &'static str,
    source: io::Error,
  },
  PipeClosed(&'static str),
  BadHandshake(&'static str),
  MissingCommand,
  MissingNixEnv,
  BadNixEnv,
  NoHomeDir,
}

impl std::fmt::Display for Error {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    let msg = match self {
      Self::Io { op, source } => {
        return write!(f, "sandbox handshake io ({op}): {source}");
      },
      Self::PipeClosed(phase) => {
        return write!(f, "sandbox handshake pipe closed early ({phase})");
      },
      Self::BadHandshake(phase) => {
        return write!(f, "bad sandbox handshake: {phase}");
      },
      Self::MissingCommand => "sandbox helper missing command",
      Self::MissingNixEnv => "CIRCUS_AGENT_NIX is not set",
      Self::BadNixEnv => {
        "CIRCUS_AGENT_NIX must point to a `nix` or `nix-store` binary"
      },
      Self::NoHomeDir => "couldn't find home dir",
    };
    write!(f, "{msg}")
  }
}

impl std::error::Error for Error {
  fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
    match self {
      Self::Io { source, .. } => Some(source),
      _ => None,
    }
  }
}

pub(crate) fn nix_command(
  rootless: bool,
  tool: NixTool,
) -> color_eyre::Result<Command> {
  let program = if rootless {
    rootless_nix_tool(tool)?
  } else {
    PathBuf::from(tool.name())
  };
  Ok(Command::new(program))
}

pub(crate) fn wrap_command(
  rootless: bool,
  target: Command,
) -> io::Result<Command> {
  if rootless {
    helper_command(&target)
  } else {
    Ok(target)
  }
}

fn rootless_nix_tool(tool: NixTool) -> color_eyre::Result<PathBuf> {
  let nix = std::env::var_os(NIX_ENV)
    .ok_or_else(|| color_eyre::Report::new(Error::MissingNixEnv))?;
  sibling_tool(PathBuf::from(nix), tool)
    .ok_or_else(|| color_eyre::Report::new(Error::BadNixEnv))
}

/// Resolve the requested tool next to whichever of the two binaries
/// `CIRCUS_AGENT_NIX` names. Returns `None` when it names neither.
fn sibling_tool(nix: PathBuf, tool: NixTool) -> Option<PathBuf> {
  match NixTool::from_filename(nix.file_name()?)? {
    found if found.name() == tool.name() => Some(nix),
    _ => Some(nix.with_file_name(tool.name())),
  }
}

fn helper_command(target: &Command) -> io::Result<Command> {
  let mut cmd = Command::new(std::env::current_exe()?);
  cmd
    .arg(HELPER_ARG)
    .arg("--")
    .arg(target.as_std().get_program())
    .args(target.as_std().get_args())
    .stdout(Stdio::piped())
    .stderr(Stdio::piped())
    .kill_on_drop(true);
  Ok(cmd)
}

/// Run in the sandbox when the hidden `--circus-sandbox` flag is used.
///
/// # Errors
///
/// Returns an error when the helper command is malformed, or when sandbox setup
/// fails before the build command can report an exit status.
pub fn maybe_run_helper(
  mut args: impl Iterator<Item = OsString>,
) -> color_eyre::Result<Option<i32>> {
  let _exe = args.next();
  if args.next().as_deref() != Some(OsStr::new(HELPER_ARG)) {
    return Ok(None);
  }
  if args.next().as_deref() != Some(OsStr::new("--")) {
    return Err(color_eyre::Report::new(Error::MissingCommand));
  }
  let program = args
    .next()
    .ok_or_else(|| color_eyre::Report::new(Error::MissingCommand))?;
  let mut cmd = Command::new(program);
  cmd.args(args);
  run_sandboxed_command(cmd).map(Some)
}

/// Validate the rootless environment once at startup. User namespaces must
/// be available and `CIRCUS_AGENT_NIX` must resolve inside the sandbox
/// store.
///
/// # Errors
///
/// Returns an error describing the failing requirement.
pub fn preflight() -> color_eyre::Result<()> {
  let nix = rootless_nix_tool(NixTool::Nix)?;
  let out = std::process::Command::new(std::env::current_exe()?)
    .arg(HELPER_ARG)
    .arg("--")
    .arg(&nix)
    .arg("--version")
    .output()?;
  if out.status.success() {
    Ok(())
  } else {
    Err(color_eyre::eyre::eyre!(
      "rootless preflight failed, either user namespaces unavailable, or \
       {NIX_ENV} does not resolve inside the sandbox store: {}",
      String::from_utf8_lossy(&out.stderr).trim()
    ))
  }
}

struct SandboxPaths {
  local_nixdir: PathBuf,
  local_tmp:    tempfile::TempDir,
  newroot:      tempfile::TempDir,
}

struct SyncPipes {
  child_rx:  OwnedFd,
  child_tx:  OwnedFd,
  parent_rx: OwnedFd,
  parent_tx: OwnedFd,
}

fn run_sandboxed_command(cmd: Command) -> color_eyre::Result<i32> {
  let paths = prepare_paths()?;
  let (child_rx, child_tx) = pipe2(OFlag::O_CLOEXEC)?;
  let (parent_rx, parent_tx) = pipe2(OFlag::O_CLOEXEC)?;
  let pipes = SyncPipes {
    child_rx,
    child_tx,
    parent_rx,
    parent_tx,
  };

  // SAFETY: after fork, the child branch only performs namespace setup and
  // then execs the requested command. On setup failure it writes to stderr and
  // exits with `_exit`, avoiding inherited async runtime cleanup.
  match unsafe { fork() }? {
    ForkResult::Parent { child } => parent_handshake(child, pipes),
    ForkResult::Child => child_enter_and_exec(cmd, pipes, &paths),
  }
}

/// `$CIRCUS_AGENT_DATA_DIR` when set, else `$XDG_DATA_HOME/circus-agent`.
fn data_dir() -> color_eyre::Result<PathBuf> {
  if let Some(dir) = std::env::var_os(DATA_DIR_ENV)
    .map(PathBuf::from)
    .filter(|d| d.is_absolute())
  {
    return Ok(dir);
  }
  if let Some(dir) = std::env::var_os("XDG_DATA_HOME")
    .map(PathBuf::from)
    .filter(|d| d.is_absolute())
  {
    return Ok(dir.join("circus-agent"));
  }
  Ok(
    std::env::home_dir()
      .ok_or(Error::NoHomeDir)?
      .join(".local")
      .join("share")
      .join("circus-agent"),
  )
}

fn prepare_paths() -> color_eyre::Result<SandboxPaths> {
  let local_nixdir = data_dir()?;
  for path in [
    local_nixdir.join("store"),
    local_nixdir.join("var").join("nix").join("db"),
    local_nixdir.join("var").join("log").join("nix"),
    local_nixdir.join("etc").join("nix"),
    local_nixdir.join("tmp"),
  ] {
    fs::create_dir_all(path)?;
  }

  // Host /tmp is often a small tmpfs on the shared machines this mode targets.
  let local_tmp = tempfile::Builder::new()
    .prefix("build-")
    .tempdir_in(local_nixdir.join("tmp"))?;
  let newroot = tempfile::Builder::new()
    .prefix("circus-bigtop-")
    .tempdir_in("/tmp")?;
  for dir in [
    "nix/store",
    "nix/var/nix/db",
    "nix/var/log/nix",
    "nix/etc/nix",
    "tmp",
    "proc",
    "dev",
    "etc",
    "etc/ssl/certs",
    ".oldroot",
  ] {
    fs::create_dir_all(newroot.path().join(dir))?;
  }
  for dev in ["null", "zero", "random", "urandom"] {
    touch(newroot.path().join("dev").join(dev))?;
  }

  Ok(SandboxPaths {
    local_nixdir,
    local_tmp,
    newroot,
  })
}

fn parent_handshake(
  child: nix::unistd::Pid,
  pipes: SyncPipes,
) -> color_eyre::Result<i32> {
  let SyncPipes {
    child_rx,
    child_tx,
    parent_rx,
    parent_tx,
  } = pipes;
  drop(child_tx);
  drop(parent_rx);

  read_token(&child_rx, *b"6", "child entered namespace")?;
  write_id_maps(child)?;
  write_token(&parent_tx, *b"7", "release child")?;

  Ok(match waitpid(child, None)? {
    WaitStatus::Exited(_, code) => code,
    WaitStatus::Signaled(_, sig, _) => 128 + sig as i32,
    _ => 127,
  })
}

fn child_enter_and_exec(
  mut cmd: Command,
  pipes: SyncPipes,
  paths: &SandboxPaths,
) -> ! {
  let SyncPipes {
    child_rx,
    child_tx,
    parent_rx,
    parent_tx,
  } = pipes;
  drop(child_rx);
  drop(parent_tx);

  let child_result = (|| -> color_eyre::Result<()> {
    set_parent_death_signal()?;
    unshare(CloneFlags::CLONE_NEWUSER | CloneFlags::CLONE_NEWNS)?;
    write_token(&child_tx, *b"6", "announce namespace")?;
    read_token(&parent_rx, *b"7", "parent wrote id maps")?;
    setup_pivot_root(paths)?;

    // The inherited environment references host paths that no longer exist
    // after the pivot, and anything sensitive in the agent's environment
    // would leak into builds.
    let bindir = PathBuf::from(cmd.as_std().get_program())
      .parent()
      .map_or_else(OsString::new, |p| p.as_os_str().to_owned());
    cmd
      .as_std_mut()
      .env_clear()
      .env("PATH", bindir)
      .env("HOME", "/tmp")
      .env("USER", "root")
      .env("NIX_REMOTE", "local")
      .env("NIX_CONF_DIR", "/nix/etc/nix")
      .env("TMPDIR", "/tmp");
    let e = cmd.as_std_mut().exec();
    Err(e.into())
  })();

  #[expect(
    clippy::print_stderr,
    reason = "inside a child proc with stderr piping to build log"
  )]
  if let Err(e) = child_result {
    eprintln!("agent sandbox setup failed: {e:?}");
  }

  // SAFETY: this is the forked child after setup failed before exec. Use
  // `_exit` to avoid running inherited async/runtime destructors.
  unsafe { libc::_exit(127) };
}

fn write_token(
  fd: &OwnedFd,
  token: [u8; 1],
  phase: &'static str,
) -> color_eyre::Result<()> {
  let mut token = token.as_slice();
  while !token.is_empty() {
    let n = write(fd, token).map_err(|e| {
      Error::Io {
        op:     phase,
        source: io::Error::from_raw_os_error(e as i32),
      }
    })?;
    if n == 0 {
      return Err(color_eyre::Report::new(Error::PipeClosed(phase)));
    }
    token = &token[n..];
  }
  Ok(())
}

fn read_token(
  fd: &OwnedFd,
  token: [u8; 1],
  phase: &'static str,
) -> color_eyre::Result<()> {
  let mut buf = [0; 1];
  let mut tail = buf.as_mut_slice();
  while !tail.is_empty() {
    let n = read(fd, tail).map_err(|e| {
      Error::Io {
        op:     phase,
        source: io::Error::from_raw_os_error(e as i32),
      }
    })?;
    if n == 0 {
      return Err(color_eyre::Report::new(Error::PipeClosed(phase)));
    }
    tail = &mut tail[n..];
  }
  if buf != token {
    return Err(color_eyre::Report::new(Error::BadHandshake(phase)));
  }
  Ok(())
}

fn touch(path: impl AsRef<Path>) -> color_eyre::Result<()> {
  if let Some(parent) = path.as_ref().parent() {
    fs::create_dir_all(parent)?;
  }
  fs::OpenOptions::new()
    .create(true)
    .append(true)
    .open(path)?;
  Ok(())
}

fn bind(
  source: impl AsRef<Path>,
  target: impl AsRef<Path>,
) -> color_eyre::Result<()> {
  mount(
    Some(source.as_ref()),
    target.as_ref(),
    None::<&str>,
    MsFlags::MS_BIND | MsFlags::MS_REC,
    None::<&str>,
  )?;
  Ok(())
}

fn bind_if_exists(
  source: impl AsRef<Path>,
  target: impl AsRef<Path>,
) -> color_eyre::Result<()> {
  let source = source.as_ref();
  let target = target.as_ref();
  // Only materialize the target when the source exists.
  if source.exists() {
    if source.is_dir() {
      fs::create_dir_all(target)?;
    } else {
      touch(target)?;
    }
    bind(source, target)?;
  }
  Ok(())
}

fn make_mounts_private() -> color_eyre::Result<()> {
  mount::<str, str, str, str>(
    None,
    "/",
    None,
    MsFlags::MS_REC | MsFlags::MS_PRIVATE,
    None,
  )?;
  Ok(())
}

fn setup_pivot_root(paths: &SandboxPaths) -> color_eyre::Result<()> {
  make_mounts_private()?;

  let newroot = paths.newroot.path();
  bind(newroot, newroot)?;

  bind(&paths.local_nixdir, newroot.join("nix"))?;
  bind(paths.local_tmp.path(), newroot.join("tmp"))?;
  bind("/dev/null", newroot.join("dev/null"))?;
  bind("/dev/zero", newroot.join("dev/zero"))?;
  bind("/dev/random", newroot.join("dev/random"))?;
  bind("/dev/urandom", newroot.join("dev/urandom"))?;
  bind_if_exists("/etc/resolv.conf", newroot.join("etc/resolv.conf"))?;
  bind_if_exists("/etc/hosts", newroot.join("etc/hosts"))?;
  bind_if_exists("/etc/nsswitch.conf", newroot.join("etc/nsswitch.conf"))?;
  bind_if_exists("/etc/ssl/certs", newroot.join("etc/ssl/certs"))?;

  mount(
    Some("proc"),
    newroot.join("proc").as_path(),
    Some("proc"),
    MsFlags::empty(),
    None::<&str>,
  )?;

  pivot_root(newroot, &newroot.join(".oldroot"))?;
  chdir("/")?;
  umount2("/.oldroot", MntFlags::MNT_DETACH)?;
  fs::remove_dir("/.oldroot")?;
  Ok(())
}

fn write_id_maps(child: nix::unistd::Pid) -> color_eyre::Result<()> {
  let base = PathBuf::from("/proc").join(child.to_string());
  fs::write(base.join("setgroups"), "deny\n")?;
  fs::write(
    base.join("uid_map"),
    format!("0 {} 1\n", Uid::current().as_raw()),
  )?;
  fs::write(
    base.join("gid_map"),
    format!("0 {} 1\n", Gid::current().as_raw()),
  )?;
  Ok(())
}

fn set_parent_death_signal() -> color_eyre::Result<()> {
  // SAFETY: prctl is called in the freshly forked child before exec, with a
  // constant operation and signal number.
  let rc = unsafe { libc::prctl(libc::PR_SET_PDEATHSIG, libc::SIGTERM) };
  if rc != 0 {
    return Err(io::Error::last_os_error().into());
  }
  if getppid().as_raw() == 1 {
    // SAFETY: this is the forked child and its supervisor is already gone.
    unsafe { libc::_exit(127) };
  }
  Ok(())
}

#[cfg(test)]
mod tests {
  use super::*;

  fn dispatch(argv: &[&str]) -> color_eyre::Result<Option<i32>> {
    maybe_run_helper(argv.iter().copied().map(OsString::from))
  }

  #[test]
  fn helper_only_dispatches_well_formed_marker() {
    // Normal invocation
    assert!(matches!(
      dispatch(&["circus-agent", "--config", "/etc/a.toml"]),
      Ok(None)
    ));
    // The marker without `--`, or with nothing after it, is malformed.
    assert!(dispatch(&["circus-agent", HELPER_ARG, "nix-store"]).is_err());
    assert!(dispatch(&["circus-agent", HELPER_ARG, "--"]).is_err());
  }

  #[test]
  fn sibling_tool_resolves_either_and_rejects_the_rest() {
    let nix = PathBuf::from("/nix/store/abc-nix/bin/nix");
    assert_eq!(sibling_tool(nix.clone(), NixTool::Nix), Some(nix.clone()));
    assert_eq!(
      sibling_tool(nix, NixTool::NixStore),
      Some(PathBuf::from("/nix/store/abc-nix/bin/nix-store"))
    );
    assert_eq!(
      sibling_tool(PathBuf::from("/bin/nix-daemon"), NixTool::Nix),
      None
    );
  }
}
