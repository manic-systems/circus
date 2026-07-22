//! Rootless sandbox for direct-store Nix builds.

use std::{
  collections::BTreeMap,
  ffi::{OsStr, OsString},
  io,
  path::{Path, PathBuf},
  process::Stdio,
};
#[cfg(target_os = "linux")]
use std::{
  fs,
  os::{
    fd::OwnedFd,
    unix::{
      fs::{PermissionsExt as _, symlink},
      process::CommandExt,
    },
  },
};

#[cfg(target_os = "linux")]
use nix::{
  fcntl::OFlag,
  mount::{MntFlags, MsFlags, mount, umount2},
  sched::{CloneFlags, unshare},
  sys::{
    statvfs::{FsFlags, statvfs},
    wait::{WaitStatus, waitpid},
  },
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
const EFFECT_HELPER_ARG: &str = "--circus-effect-sandbox";
const NIX_ENV: &str = "CIRCUS_AGENT_NIX";
pub const DATA_DIR_ENV: &str = "CIRCUS_AGENT_DATA_DIR";

#[cfg(target_os = "linux")]
const BUILD_DEV_NODES: [&str; 6] =
  ["full", "null", "random", "tty", "urandom", "zero"];

#[derive(Clone, Copy)]
pub(crate) enum NixTool {
  Nix,
  NixStore,
}

#[derive(Clone, Copy)]
pub(crate) struct EffectSandboxOptions<'a> {
  pub rootless:      bool,
  pub build_dir:     &'a Path,
  pub secrets_file:  &'a Path,
  pub fs_root:       Option<&'a Path>,
  pub default_shell: &'a Path,
  pub default_env:   Option<&'a Path>,
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
    rootless_wrap_command(&target)
  } else {
    Ok(target)
  }
}

fn rootless_nix_tool(tool: NixTool) -> color_eyre::Result<PathBuf> {
  if !cfg!(target_os = "linux") {
    return Err(color_eyre::eyre::eyre!(
      "rootless agent mode is only supported on Linux"
    ));
  }
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

#[cfg(target_os = "linux")]
fn rootless_wrap_command(target: &Command) -> io::Result<Command> {
  helper_command(target)
}

#[cfg(not(target_os = "linux"))]
fn rootless_wrap_command(_target: &Command) -> io::Result<Command> {
  Err(io::Error::new(
    io::ErrorKind::Unsupported,
    "rootless agent mode is only supported on Linux",
  ))
}

#[cfg(target_os = "linux")]
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

pub(crate) fn effect_command(
  opts: EffectSandboxOptions<'_>,
  program: &Path,
  args: &[String],
  env: &BTreeMap<String, String>,
) -> io::Result<Command> {
  let mut cmd = Command::new(std::env::current_exe()?);
  cmd
    .arg(EFFECT_HELPER_ARG)
    .arg(if opts.rootless { "1" } else { "0" })
    .arg(opts.build_dir)
    .arg(opts.secrets_file)
    .arg(if opts.fs_root.is_some() { "1" } else { "0" });
  if let Some(fs_root) = opts.fs_root {
    cmd.arg(fs_root);
  }
  cmd
    .arg(opts.default_shell)
    .arg(if opts.default_env.is_some() { "1" } else { "0" });
  if let Some(default_env) = opts.default_env {
    cmd.arg(default_env);
  }
  cmd
    .arg("--")
    .arg(program)
    .args(args)
    .env_clear()
    .envs(env)
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
  let marker = args.next();
  if marker.as_deref() == Some(OsStr::new(EFFECT_HELPER_ARG)) {
    return run_effect_helper(args).map(Some);
  }
  if marker.as_deref() != Some(OsStr::new(HELPER_ARG)) {
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
  #[cfg(not(target_os = "linux"))]
  {
    let _ = cmd;
    Err(color_eyre::eyre::eyre!(
      "rootless agent mode is only supported on Linux"
    ))
  }
  #[cfg(target_os = "linux")]
  {
    run_sandboxed_command(cmd).map(Some)
  }
}

#[cfg(not(target_os = "linux"))]
fn run_effect_helper(
  _args: impl Iterator<Item = OsString>,
) -> color_eyre::Result<i32> {
  Err(color_eyre::eyre::eyre!(
    "effects are only supported on Linux"
  ))
}

#[cfg(target_os = "linux")]
fn run_effect_helper(
  mut args: impl Iterator<Item = OsString>,
) -> color_eyre::Result<i32> {
  let rootless = match args.next().as_deref() {
    Some(v) if v == OsStr::new("1") => true,
    Some(v) if v == OsStr::new("0") => false,
    _ => return Err(color_eyre::Report::new(Error::MissingCommand)),
  };
  let build_dir = next_path(&mut args)?;
  let secrets_file = next_path(&mut args)?;
  let fs_root = match args.next().as_deref() {
    Some(v) if v == OsStr::new("1") => Some(next_path(&mut args)?),
    Some(v) if v == OsStr::new("0") => None,
    _ => return Err(color_eyre::Report::new(Error::MissingCommand)),
  };
  let default_shell = next_path(&mut args)?;
  let default_env = match args.next().as_deref() {
    Some(v) if v == OsStr::new("1") => Some(next_path(&mut args)?),
    Some(v) if v == OsStr::new("0") => None,
    _ => return Err(color_eyre::Report::new(Error::MissingCommand)),
  };
  if args.next().as_deref() != Some(OsStr::new("--")) {
    return Err(color_eyre::Report::new(Error::MissingCommand));
  }
  let program = args
    .next()
    .ok_or_else(|| color_eyre::Report::new(Error::MissingCommand))?;
  let mut cmd = Command::new(program);
  cmd.args(args);
  run_effect_sandboxed_command(cmd, EffectSandboxOptions {
    rootless,
    build_dir: &build_dir,
    secrets_file: &secrets_file,
    fs_root: fs_root.as_deref(),
    default_shell: &default_shell,
    default_env: default_env.as_deref(),
  })
}

#[cfg(target_os = "linux")]
fn next_path(
  args: &mut impl Iterator<Item = OsString>,
) -> color_eyre::Result<PathBuf> {
  args
    .next()
    .map(PathBuf::from)
    .filter(|path| path.is_absolute())
    .ok_or_else(|| color_eyre::Report::new(Error::MissingCommand))
}

/// Validate the rootless environment once at startup. User namespaces must
/// be available and `CIRCUS_AGENT_NIX` must resolve inside the sandbox
/// store.
///
/// # Errors
///
/// Returns an error describing the failing requirement.
pub fn preflight() -> color_eyre::Result<()> {
  if !cfg!(target_os = "linux") {
    return Err(color_eyre::eyre::eyre!(
      "rootless agent mode is only supported on Linux"
    ));
  }
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

#[cfg(target_os = "linux")]
struct SandboxPaths {
  local_nixdir: PathBuf,
  local_tmp:    tempfile::TempDir,
  newroot:      tempfile::TempDir,
}

#[cfg(target_os = "linux")]
struct EffectSandboxPaths {
  rootless:     bool,
  local_nixdir: Option<PathBuf>,
  build_dir:    PathBuf,
  secrets_file: PathBuf,
  newroot:      tempfile::TempDir,
}

#[cfg(target_os = "linux")]
struct SyncPipes {
  child_rx:  OwnedFd,
  child_tx:  OwnedFd,
  parent_rx: OwnedFd,
  parent_tx: OwnedFd,
}

#[cfg(target_os = "linux")]
fn run_sandboxed_command(cmd: Command) -> color_eyre::Result<i32> {
  let paths = prepare_paths()?;
  let pipes = sync_pipes()?;

  // SAFETY: after fork, the child branch only performs namespace setup and
  // then execs the requested command. On setup failure it writes to stderr and
  // exits with `_exit`, avoiding inherited async runtime cleanup.
  match unsafe { fork() }? {
    ForkResult::Parent { child } => parent_handshake(child, pipes),
    ForkResult::Child => child_enter_and_exec(cmd, pipes, &paths),
  }
}

#[cfg(target_os = "linux")]
fn run_effect_sandboxed_command(
  cmd: Command,
  opts: EffectSandboxOptions<'_>,
) -> color_eyre::Result<i32> {
  let paths = prepare_effect_paths(&opts)?;
  let pipes = sync_pipes()?;

  // SAFETY: the child only establishes its namespaces and execs the effect.
  match unsafe { fork() }? {
    ForkResult::Parent { child } => parent_handshake(child, pipes),
    ForkResult::Child => child_enter_effect_and_exec(cmd, pipes, &paths),
  }
}

#[cfg(target_os = "linux")]
fn sync_pipes() -> nix::Result<SyncPipes> {
  let (child_rx, child_tx) = pipe2(OFlag::O_CLOEXEC)?;
  let (parent_rx, parent_tx) = pipe2(OFlag::O_CLOEXEC)?;
  Ok(SyncPipes {
    child_rx,
    child_tx,
    parent_rx,
    parent_tx,
  })
}

/// `$CIRCUS_AGENT_DATA_DIR` when set, else `$XDG_DATA_HOME/circus-agent`.
#[cfg(target_os = "linux")]
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

#[cfg(target_os = "linux")]
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
    "dev/pts",
    "etc",
    "etc/ssl/certs",
    ".oldroot",
  ] {
    fs::create_dir_all(newroot.path().join(dir))?;
  }
  for dev in BUILD_DEV_NODES {
    touch(newroot.path().join("dev").join(dev))?;
  }
  std::os::unix::fs::symlink("pts/ptmx", newroot.path().join("dev/ptmx"))?;

  Ok(SandboxPaths {
    local_nixdir,
    local_tmp,
    newroot,
  })
}

#[cfg(target_os = "linux")]
fn prepare_effect_paths(
  opts: &EffectSandboxOptions<'_>,
) -> color_eyre::Result<EffectSandboxPaths> {
  if !opts.build_dir.is_dir() || !opts.secrets_file.is_file() {
    return Err(color_eyre::Report::new(Error::MissingCommand));
  }
  let newroot = tempfile::Builder::new()
    .prefix("circus-effect-root-")
    .tempdir_in("/tmp")?;
  for dir in [
    "nix/store",
    "nix/var/nix/db",
    "build",
    "secrets",
    "proc",
    "dev",
    "etc",
    "etc/ssl/certs",
    "bin",
    "usr/bin",
    ".oldroot",
  ] {
    fs::create_dir_all(newroot.path().join(dir))?;
  }
  for dev in ["null", "zero", "random", "urandom"] {
    touch(newroot.path().join("dev").join(dev))?;
  }
  touch(newroot.path().join("secrets/secrets.json"))?;

  if let Some(fs_root) = opts.fs_root {
    copy_effect_fs_root(fs_root, newroot.path())?;
  }
  let shell = newroot.path().join("bin/sh");
  if fs::symlink_metadata(&shell).is_err() {
    symlink(opts.default_shell, shell)?;
  }
  if let Some(default_env) = opts.default_env {
    let env = newroot.path().join("usr/bin/env");
    if fs::symlink_metadata(&env).is_err() {
      symlink(default_env, env)?;
    }
  }

  Ok(EffectSandboxPaths {
    rootless: opts.rootless,
    local_nixdir: opts.rootless.then(data_dir).transpose()?,
    build_dir: opts.build_dir.to_path_buf(),
    secrets_file: opts.secrets_file.to_path_buf(),
    newroot,
  })
}

#[cfg(target_os = "linux")]
fn copy_effect_fs_root(source: &Path, destination: &Path) -> io::Result<()> {
  const RESERVED: &[&str] =
    &["nix", "build", "secrets", "proc", "dev", "sys", ".oldroot"];
  if !source.is_dir() {
    return Err(io::Error::new(
      io::ErrorKind::InvalidInput,
      "effect fs root is not a directory",
    ));
  }
  for entry in fs::read_dir(source)? {
    let entry = entry?;
    let name = entry.file_name();
    if RESERVED.iter().any(|reserved| name == OsStr::new(reserved)) {
      return Err(io::Error::new(
        io::ErrorKind::InvalidInput,
        "effect fs root contains a reserved top-level path",
      ));
    }
    copy_tree_entry(&entry.path(), &destination.join(name))?;
  }
  Ok(())
}

#[cfg(target_os = "linux")]
fn copy_tree_entry(source: &Path, destination: &Path) -> io::Result<()> {
  let metadata = fs::symlink_metadata(source)?;
  if metadata.file_type().is_symlink() {
    symlink(fs::read_link(source)?, destination)?;
  } else if metadata.is_dir() {
    fs::create_dir_all(destination)?;
    for entry in fs::read_dir(source)? {
      let entry = entry?;
      copy_tree_entry(&entry.path(), &destination.join(entry.file_name()))?;
    }
    fs::set_permissions(
      destination,
      fs::Permissions::from_mode(metadata.permissions().mode()),
    )?;
  } else if metadata.is_file() {
    fs::copy(source, destination)?;
    fs::set_permissions(
      destination,
      fs::Permissions::from_mode(metadata.permissions().mode()),
    )?;
  } else {
    return Err(io::Error::new(
      io::ErrorKind::InvalidInput,
      "effect fs root contains an unsupported file type",
    ));
  }
  Ok(())
}

#[cfg(target_os = "linux")]
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

#[cfg(target_os = "linux")]
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
    set_parent_death_signal_and_verify(libc::SIGTERM)?;
    unshare(CloneFlags::CLONE_NEWUSER | CloneFlags::CLONE_NEWNS)?;
    write_token(&child_tx, *b"6", "announce namespace")?;
    read_token(&parent_rx, *b"7", "parent wrote id maps")?;
    let ca_bundle = HOST_CA_BUNDLES
      .into_iter()
      .find(|bundle| Path::new(bundle).exists());
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
      .env("NIX_CONFIG", "require-drop-supplementary-groups = false")
      .env("TMPDIR", "/tmp");
    if let Some(bundle) = ca_bundle {
      cmd.as_std_mut().env("NIX_SSL_CERT_FILE", bundle);
    }
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

#[cfg(target_os = "linux")]
fn child_enter_effect_and_exec(
  mut cmd: Command,
  pipes: SyncPipes,
  paths: &EffectSandboxPaths,
) -> ! {
  let SyncPipes {
    child_rx,
    child_tx,
    parent_rx,
    parent_tx,
  } = pipes;
  drop(child_rx);
  drop(parent_tx);

  let setup_result = (|| -> color_eyre::Result<()> {
    set_parent_death_signal_and_verify(libc::SIGKILL)?;
    unshare(CloneFlags::CLONE_NEWUSER | CloneFlags::CLONE_NEWNS)?;
    write_token(&child_tx, *b"6", "announce namespace")?;
    read_token(&parent_rx, *b"7", "parent wrote id maps")?;
    unshare(CloneFlags::CLONE_NEWPID)?;
    Ok(())
  })();
  drop(child_tx);
  drop(parent_rx);

  if let Err(e) = setup_result {
    print_effect_setup_error(&e);
    // SAFETY: this is the forked child after setup failed before exec.
    unsafe { libc::_exit(127) };
  }

  let death_pipes = match sync_pipes() {
    Ok(pipes) => pipes,
    Err(e) => {
      print_effect_setup_error(&color_eyre::Report::from(e));
      // SAFETY: namespace setup failed before exec.
      unsafe { libc::_exit(127) };
    },
  };

  // SAFETY: both branches only wait/exec and use `_exit` on failure.
  match unsafe { fork() } {
    Ok(ForkResult::Parent { child }) => {
      let code = match parent_death_handshake_and_wait(child, death_pipes) {
        Ok(code) => code,
        Err(e) => {
          print_effect_setup_error(&e);
          127
        },
      };
      // SAFETY: this intermediate namespace process must not run inherited
      // async runtime destructors.
      unsafe { libc::_exit(code) };
    },
    Ok(ForkResult::Child) => {
      let child_result = (|| -> color_eyre::Result<()> {
        arm_parent_death_signal(death_pipes)?;
        setup_effect_pivot_root(paths)?;
        chdir("/build")?;
        // SAFETY: this process is about to exec and the constant mask only
        // narrows permissions on files the effect creates.
        unsafe { libc::umask(0o077) };
        let e = cmd.as_std_mut().exec();
        Err(e.into())
      })();
      if let Err(e) = child_result {
        print_effect_setup_error(&e);
      }
    },
    Err(e) => {
      print_effect_setup_error(&color_eyre::Report::from(e));
    },
  }
  // SAFETY: the namespace init failed before exec and must not run inherited
  // async runtime destructors.
  unsafe { libc::_exit(127) };
}

#[cfg(target_os = "linux")]
fn parent_death_handshake_and_wait(
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

  read_token(&child_rx, *b"8", "child armed parent-death signal")?;
  write_token(&parent_tx, *b"9", "acknowledge live namespace parent")?;
  drop(child_rx);
  drop(parent_tx);

  Ok(match waitpid(child, None)? {
    WaitStatus::Exited(_, code) => code,
    WaitStatus::Signaled(_, signal, _) => 128 + signal as i32,
    _ => 127,
  })
}

#[cfg(target_os = "linux")]
fn arm_parent_death_signal(pipes: SyncPipes) -> color_eyre::Result<()> {
  let SyncPipes {
    child_rx,
    child_tx,
    parent_rx,
    parent_tx,
  } = pipes;
  drop(child_rx);
  drop(parent_tx);

  set_parent_death_signal(libc::SIGKILL)?;
  write_token(&child_tx, *b"8", "announce armed parent-death signal")?;
  read_token(&parent_rx, *b"9", "namespace parent still alive")?;
  Ok(())
}

#[cfg(target_os = "linux")]
#[expect(
  clippy::print_stderr,
  reason = "inside a child proc with stderr piping to the effect log"
)]
fn print_effect_setup_error(error: &color_eyre::Report) {
  eprintln!("agent effect sandbox setup failed: {error:?}");
}

#[cfg(target_os = "linux")]
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

#[cfg(target_os = "linux")]
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

#[cfg(target_os = "linux")]
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

#[cfg(target_os = "linux")]
fn bind(
  source: impl AsRef<Path>,
  target: impl AsRef<Path>,
) -> color_eyre::Result<()> {
  let source = source.as_ref();
  let target = target.as_ref();
  mount(
    Some(source),
    target,
    None::<&str>,
    MsFlags::MS_BIND | MsFlags::MS_REC,
    None::<&str>,
  )
  .map_err(|error| {
    color_eyre::eyre::eyre!(
      "bind mount {} at {}: {error}",
      source.display(),
      target.display()
    )
  })?;
  Ok(())
}

#[cfg(target_os = "linux")]
fn bind_readonly(
  source: impl AsRef<Path>,
  target: impl AsRef<Path>,
) -> color_eyre::Result<()> {
  bind(source, target.as_ref())?;
  let fs_flags = statvfs(target.as_ref())?.flags();
  let mut remount_flags =
    MsFlags::MS_BIND | MsFlags::MS_REMOUNT | MsFlags::MS_RDONLY;
  for (present, mount_flag) in [
    (FsFlags::ST_NOSUID, MsFlags::MS_NOSUID),
    (FsFlags::ST_NODEV, MsFlags::MS_NODEV),
    (FsFlags::ST_NOEXEC, MsFlags::MS_NOEXEC),
    (FsFlags::ST_SYNCHRONOUS, MsFlags::MS_SYNCHRONOUS),
    (FsFlags::ST_MANDLOCK, MsFlags::MS_MANDLOCK),
    (FsFlags::ST_NOATIME, MsFlags::MS_NOATIME),
    (FsFlags::ST_NODIRATIME, MsFlags::MS_NODIRATIME),
    (FsFlags::ST_RELATIME, MsFlags::MS_RELATIME),
  ] {
    if fs_flags.contains(present) {
      remount_flags.insert(mount_flag);
    }
  }
  mount(
    None::<&Path>,
    target.as_ref(),
    None::<&str>,
    remount_flags,
    None::<&str>,
  )
  .map_err(|error| {
    color_eyre::eyre::eyre!(
      "remount {} read-only with inherited flags {fs_flags:?}: {error}",
      target.as_ref().display()
    )
  })?;
  Ok(())
}

/// Nix only probes the Debian path on its own.
#[cfg(target_os = "linux")]
const HOST_CA_BUNDLES: [&str; 2] = [
  "/etc/ssl/certs/ca-certificates.crt",
  "/etc/pki/tls/certs/ca-bundle.crt",
];

#[cfg(target_os = "linux")]
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

/// Nix opens `/dev/ptmx` for the builder console before it forks, and a
/// private `devpts` is one of the few mounts an unprivileged namespace gets.
#[cfg(target_os = "linux")]
fn mount_devpts(target: impl AsRef<Path>) -> color_eyre::Result<()> {
  mount(
    Some("devpts"),
    target.as_ref(),
    Some("devpts"),
    MsFlags::MS_NOSUID | MsFlags::MS_NOEXEC,
    Some("newinstance,ptmxmode=0666,mode=0620"),
  )?;
  Ok(())
}

#[cfg(target_os = "linux")]
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

#[cfg(target_os = "linux")]
fn setup_pivot_root(paths: &SandboxPaths) -> color_eyre::Result<()> {
  make_mounts_private()?;

  let newroot = paths.newroot.path();
  bind(newroot, newroot)?;

  bind(&paths.local_nixdir, newroot.join("nix"))?;
  bind(paths.local_tmp.path(), newroot.join("tmp"))?;
  for dev in BUILD_DEV_NODES {
    bind(Path::new("/dev").join(dev), newroot.join("dev").join(dev))?;
  }
  mount_devpts(newroot.join("dev/pts"))?;
  bind_if_exists("/etc/resolv.conf", newroot.join("etc/resolv.conf"))?;
  bind_if_exists("/etc/hosts", newroot.join("etc/hosts"))?;
  bind_if_exists("/etc/nsswitch.conf", newroot.join("etc/nsswitch.conf"))?;
  bind_if_exists("/etc/ssl/certs", newroot.join("etc/ssl/certs"))?;
  // RHEL keeps the bundle here and /etc/ssl/certs only symlinks into it.
  bind_if_exists("/etc/pki", newroot.join("etc/pki"))?;

  // Bind the host /proc rather than mounting a fresh procfs as it needs
  // CAP_SYS_ADMIN over the PID namespace it exposes, which this sandbox does
  // not own.
  bind("/proc", newroot.join("proc"))?;

  pivot_root(newroot, &newroot.join(".oldroot"))?;
  chdir("/")?;
  umount2("/.oldroot", MntFlags::MNT_DETACH)?;
  fs::remove_dir("/.oldroot")?;
  Ok(())
}

#[cfg(target_os = "linux")]
fn setup_effect_pivot_root(
  paths: &EffectSandboxPaths,
) -> color_eyre::Result<()> {
  make_mounts_private()?;

  let newroot = paths.newroot.path();
  bind(newroot, newroot)?;
  if paths.rootless {
    bind(
      paths.local_nixdir.as_ref().ok_or(Error::MissingCommand)?,
      newroot.join("nix"),
    )?;
  } else {
    bind_readonly("/nix/store", newroot.join("nix/store"))?;
    bind_readonly("/nix/var/nix/db", newroot.join("nix/var/nix/db"))?;
  }
  bind(&paths.build_dir, newroot.join("build"))?;
  bind_readonly(&paths.secrets_file, newroot.join("secrets/secrets.json"))?;
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
    MsFlags::MS_NOSUID | MsFlags::MS_NODEV | MsFlags::MS_NOEXEC,
    // Kernels before 7.2 reject fresh procfs without subset=pid when systemd
    // hides entries.
    Some("subset=pid"),
  )?;
  pivot_root(newroot, &newroot.join(".oldroot"))?;
  chdir("/")?;
  umount2("/.oldroot", MntFlags::MNT_DETACH)?;
  fs::remove_dir("/.oldroot")?;
  Ok(())
}

#[cfg(target_os = "linux")]
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

#[cfg(target_os = "linux")]
fn set_parent_death_signal(signal: libc::c_int) -> color_eyre::Result<()> {
  // SAFETY: prctl is called in the freshly forked child before exec, with a
  // validated signal number.
  let rc = unsafe { libc::prctl(libc::PR_SET_PDEATHSIG, signal) };
  if rc != 0 {
    return Err(io::Error::last_os_error().into());
  }
  Ok(())
}

#[cfg(target_os = "linux")]
fn set_parent_death_signal_and_verify(
  signal: libc::c_int,
) -> color_eyre::Result<()> {
  set_parent_death_signal(signal)?;
  if getppid().as_raw() <= 1 {
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

  #[cfg(target_os = "linux")]
  #[test]
  fn effect_sandbox_never_stages_the_privileged_nix_daemon_socket() {
    let task = tempfile::tempdir().expect("task dir");
    let build_dir = task.path().join("build");
    let secrets_file = task.path().join("secrets.json");
    fs::create_dir(&build_dir).expect("build dir");
    fs::write(&secrets_file, b"{}").expect("secrets file");
    let paths = prepare_effect_paths(&EffectSandboxOptions {
      rootless:      false,
      build_dir:     &build_dir,
      secrets_file:  &secrets_file,
      fs_root:       None,
      default_shell: Path::new("/bin/sh"),
      default_env:   None,
    })
    .expect("prepare effect sandbox");

    assert!(paths.newroot.path().join("secrets/secrets.json").is_file());
    assert!(
      !paths
        .newroot
        .path()
        .join("nix/var/nix/daemon-socket/socket")
        .exists()
    );
    let staged_nix_state =
      fs::read_dir(paths.newroot.path().join("nix/var/nix"))
        .expect("staged Nix state")
        .map(|entry| entry.expect("staged Nix entry").file_name())
        .collect::<Vec<_>>();
    assert_eq!(staged_nix_state, [OsString::from("db")]);
  }
}
