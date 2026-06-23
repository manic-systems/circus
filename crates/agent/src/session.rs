//! Connect to the runner, register, and run the per-connection event loop.
//!
//! capnp-rpc on the agent side is two halves:
//!
//! 1. The agent calls `runner.register(info, builder)` once after connecting.
//!    The `builder` is a capability we host so the runner can push work into
//!    us. The `session` capability we get back is for outbound heartbeats.
//!
//! 2. Concurrent with that, we accept whatever `Builder.assign` calls the
//!    runner makes. Each `assign` spawns one `crate::build::run` task; the
//!    completion is reported via the `result` sink the runner passed in.

use std::{
  collections::HashMap,
  fmt::Write,
  path::{Path, PathBuf},
  sync::{
    Arc,
    atomic::{AtomicBool, AtomicU32, Ordering},
  },
  time::{Duration, Instant},
};

use capnp::capability::Promise;
use capnp_rpc::{RpcSystem, rpc_twoparty_capnp, twoparty};
use circus_proto::{
  PROTO_VERSION,
  agent_info,
  agent_session,
  builder,
  heartbeat,
  log_sink,
  output_sink,
  pressure_state,
  result_sink,
  runner,
};
use color_eyre::eyre::{Context as _, bail, eyre};
use parking_lot::Mutex;
use tokio::net::TcpStream;
use tokio_util::{
  compat::{TokioAsyncReadCompatExt as _, TokioAsyncWriteCompatExt as _},
  sync::CancellationToken,
};
use uuid::Uuid;

use crate::{
  build,
  config::{Agent, EphemeralConfig, TlsConfig},
  psi,
  sandbox::NixTool,
};

/// Open a connection and run it to completion.
///
/// Returns when the runner disconnects or the local builder side fails.
/// The caller (`main`) implements reconnect with backoff.
///
/// # Errors
///
/// Network or RPC errors. Connection-time errors (`connect`, `register`)
/// are bubbled; mid-stream errors land in tracing and end the function.
#[expect(
  clippy::future_not_send,
  reason = "capnp futures are not Send; agent uses a single-threaded runtime"
)]
pub async fn run_once(cfg: &Agent, machine_id: Uuid) -> color_eyre::Result<()> {
  let (host, port, want_tls) = parse_endpoint(&cfg.runner_url)?;
  tracing::info!(host = %host, port, want_tls, "dialing runner");
  let socket = TcpStream::connect((host.as_str(), port))
    .await
    .with_context(|| format!("connect to runner {host}:{port}"))?;
  let _ = socket.set_nodelay(true);

  let want_tls = want_tls || cfg.tls.is_some();

  // Branch on TLS at the type level: keep both arms inside the RPC system
  // by erasing through `Box<dyn>` and the tokio-util compat adapters.
  let mut rpc = if want_tls {
    let default_tls = TlsConfig::default();
    let tls = cfg.tls.as_ref().unwrap_or(&default_tls);
    let connector = crate::tls::build_client_connector(tls)?;
    let server_name = rustls::pki_types::ServerName::try_from(host.clone())
      .map_err(|e| eyre!("invalid server name {host}: {e}"))?;
    let stream = connector.connect(server_name, socket).await?;
    let (rh, wh) = tokio::io::split(stream);
    let network = twoparty::VatNetwork::new(
      rh.compat(),
      wh.compat_write(),
      rpc_twoparty_capnp::Side::Client,
      capnp::message::ReaderOptions::default(),
    );
    RpcSystem::new(Box::new(network), None)
  } else {
    let (read_half, write_half) = socket.into_split();
    let network = twoparty::VatNetwork::new(
      read_half.compat(),
      write_half.compat_write(),
      rpc_twoparty_capnp::Side::Client,
      capnp::message::ReaderOptions::default(),
    );
    RpcSystem::new(Box::new(network), None)
  };

  let runner_cap: runner::Client =
    rpc.bootstrap(rpc_twoparty_capnp::Side::Server);
  let disconnector = rpc.get_disconnector();

  let lifecycle = cfg
    .ephemeral
    .as_ref()
    .map(|e| Arc::new(Lifecycle::new(e.max_builds)));

  let local_builder: builder::Client = capnp_rpc::new_client(BuilderImpl::new(
    cfg.max_jobs,
    cfg.cores,
    machine_id,
    runner_cap.clone(),
    cfg.rootless,
    lifecycle.clone(),
  ));

  let mut rpc_join = tokio::task::spawn_local(async move {
    if let Err(e) = rpc.await {
      tracing::warn!("rpc system ended: {e}");
    }
  });

  verify_runner_version(&runner_cap).await?;
  let session = register(&runner_cap, cfg, machine_id, local_builder).await?;
  tracing::info!("registered with runner");

  let heartbeat_join = spawn_heartbeat(
    session,
    Duration::from_secs(cfg.heartbeat_interval_secs.max(1)),
    cfg.work_dir.clone(),
  );

  // Ephemeral: a monitor drains and exits on the limits. Persistent: run until
  // the connection ends.
  if let (Some(eph), Some(lc)) = (cfg.ephemeral.as_ref(), lifecycle.as_ref()) {
    let quit = CancellationToken::new();
    let monitor = tokio::task::spawn_local(ephemeral_monitor(
      eph.clone(),
      Arc::clone(lc),
      quit.clone(),
    ));
    tokio::select! {
      _ = &mut rpc_join => {
        tracing::info!("connection ended before ephemeral limits");
      },
      () = quit.cancelled() => {
        drain_inflight().await;
      },
    }
    monitor.abort();
  } else {
    let _ = rpc_join.await;
  }

  heartbeat_join.abort();
  let _ = disconnector.await;
  Ok(())
}

/// Wait for in-flight builds to finish before disconnecting (new work is
/// already refused). Bounded; anything still running when the grace expires is
/// recovered by the runner's orphan sweeper.
async fn drain_inflight() {
  const DRAIN_GRACE: Duration = Duration::from_mins(5);
  let deadline = Instant::now() + DRAIN_GRACE;
  let mut ticker = tokio::time::interval(Duration::from_secs(1));
  loop {
    let running = JOB_COUNTER.load(Ordering::Relaxed);
    if running == 0 {
      tracing::info!("ephemeral: drained; disconnecting");
      return;
    }
    if Instant::now() >= deadline {
      tracing::warn!(
        running,
        "ephemeral: drain grace expired with builds still running; \
         disconnecting anyway (runner will requeue)"
      );
      return;
    }
    ticker.tick().await;
  }
}

fn parse_endpoint(url: &str) -> color_eyre::Result<(String, u16, bool)> {
  let has_scheme = url.contains("://");
  let normalized = if has_scheme {
    url.to_owned()
  } else {
    format!("circus://{url}")
  };
  let parsed = url::Url::parse(&normalized)
    .with_context(|| format!("invalid runner_url: {url}"))?;
  let scheme = parsed.scheme();
  let tls = matches!(scheme, "circus+tls");
  if !matches!(scheme, "circus" | "circus+tls") {
    bail!("unsupported runner_url scheme: {scheme}");
  }
  let host = parsed
    .host_str()
    .ok_or_else(|| eyre!("missing host in runner_url"))?
    .to_owned();
  let port = parsed
    .port()
    .ok_or_else(|| eyre!("missing port in runner_url"))?;
  Ok((host, port, tls))
}

async fn verify_runner_version(
  runner_cap: &runner::Client,
) -> color_eyre::Result<()> {
  #![expect(
    clippy::future_not_send,
    reason = "capnp futures are not Send; agent uses a single-threaded runtime"
  )]
  let response = runner_cap
    .version_request()
    .send()
    .promise
    .await
    .context("version")?;
  let payload = response.get().context("version response")?;
  let proto = payload.get_proto()?.to_str()?;
  if proto != PROTO_VERSION {
    bail!("proto mismatch: runner={proto} agent={PROTO_VERSION}");
  }
  Ok(())
}

async fn register(
  runner_cap: &runner::Client,
  cfg: &Agent,
  machine_id: Uuid,
  local_builder: builder::Client,
) -> color_eyre::Result<agent_session::Client> {
  #![expect(
    clippy::future_not_send,
    reason = "capnp futures are not Send; agent uses a single-threaded runtime"
  )]
  let mut req = runner_cap.register_request();
  let mut params = req.get();
  fill_info(params.reborrow().init_info(), cfg, machine_id);
  params.set_builder(local_builder);
  let response = req.send().promise.await.context("register")?;
  let session = response.get().context("register response")?.get_session()?;
  Ok(session)
}

fn fill_info(mut info: agent_info::Builder<'_>, cfg: &Agent, machine_id: Uuid) {
  let hostname = read_hostname();
  info.set_hostname(hostname.as_str());
  info.set_name(cfg.name.as_str());
  info.set_machine_id(machine_id.to_string().as_str());
  info.set_speed_factor(cfg.speed_factor);
  info.set_cpu_count(num_cpus() as u32);
  info.set_max_jobs(cfg.max_jobs);
  info.set_proto_version(PROTO_VERSION);
  info.set_auth_token(cfg.auth_token.as_str());
  info.set_ephemeral(cfg.ephemeral.is_some());

  {
    let mut sys = info.reborrow().init_systems(cfg.systems.len() as u32);
    for (i, s) in cfg.systems.iter().enumerate() {
      sys.set(i as u32, s.as_str());
    }
  }
  {
    let mut feats = info
      .reborrow()
      .init_supported_features(cfg.supported_features.len() as u32);
    for (i, s) in cfg.supported_features.iter().enumerate() {
      feats.set(i as u32, s.as_str());
    }
  }
  {
    let mut feats = info
      .reborrow()
      .init_mandatory_features(cfg.mandatory_features.len() as u32);
    for (i, s) in cfg.mandatory_features.iter().enumerate() {
      feats.set(i as u32, s.as_str());
    }
  }
}

/// Best-effort hostname read.
fn read_hostname() -> String {
  sysinfo::System::host_name().unwrap_or_else(|| "unknown".into())
}

fn num_cpus() -> usize {
  std::thread::available_parallelism().map_or(1, std::num::NonZeroUsize::get)
}

fn spawn_heartbeat(
  session: agent_session::Client,
  interval: Duration,
  work_dir: PathBuf,
) -> tokio::task::JoinHandle<()> {
  tokio::task::spawn_local(async move {
    let mut ticker = tokio::time::interval(interval);
    ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    loop {
      ticker.tick().await;
      if let Err(e) = send_heartbeat(&session, &work_dir).await {
        tracing::warn!("heartbeat failed: {e}; ending loop");
        break;
      }
    }
  })
}

async fn send_heartbeat(
  session: &agent_session::Client,
  work_dir: &Path,
) -> Result<(), capnp::Error> {
  #![expect(
    clippy::future_not_send,
    reason = "capnp futures are not Send; agent uses a single-threaded runtime"
  )]
  let mut req = session.heartbeat_request();
  let mut ping: heartbeat::Builder<'_> = req.get().init_ping();
  let load = read_loadavg();
  ping.set_load1(load.0);
  ping.set_load5(load.1);
  ping.set_load15(load.2);
  ping.set_current_jobs(JOB_COUNTER.load(Ordering::Relaxed));
  let mem = read_meminfo();
  ping.set_mem_total(mem.0);
  ping.set_mem_used(mem.1);
  ping.set_store_free(fs_available_bytes(Path::new("/nix/store")));
  ping.set_build_dir_free(fs_available_bytes(work_dir));

  let snap = psi::read();
  let mut p: pressure_state::Builder = ping.reborrow().init_pressure();
  p.set_cpu_avg10(snap.cpu_avg10);
  p.set_mem_avg10(snap.mem_avg10);
  p.set_io_avg10(snap.io_avg10);
  p.set_cpu_avg60(snap.cpu_avg60);
  p.set_mem_avg60(snap.mem_avg60);
  p.set_io_avg60(snap.io_avg60);

  req.send().promise.await?;
  Ok(())
}

fn read_loadavg() -> (f32, f32, f32) {
  let load = sysinfo::System::load_average();
  (load.one as f32, load.five as f32, load.fifteen as f32)
}

fn read_meminfo() -> (u64, u64) {
  let mut system = sysinfo::System::new();
  system.refresh_memory();
  let total = system.total_memory();
  (total, total.saturating_sub(system.available_memory()))
}

fn fs_available_bytes(path: &Path) -> u64 {
  nix::sys::statvfs::statvfs(path).map_or(0, |stat| {
    #[cfg(target_os = "macos")]
    {
      u64::from(stat.blocks_available()).saturating_mul(stat.fragment_size())
    }
    #[cfg(not(target_os = "macos"))]
    {
      stat.blocks_available().saturating_mul(stat.fragment_size())
    }
  })
}

/// Process-global counter for concurrent builds. Bumped on `assign`,
/// dropped on result. Exposed in heartbeats.
static JOB_COUNTER: AtomicU32 = AtomicU32::new(0);

/// Shared lifecycle state for an ephemeral session; `None` for persistent
/// agents. The builder records activity, the monitor reads it.
struct Lifecycle {
  /// Builds that have reported a result this session.
  completed:   AtomicU32,
  /// Builds accepted this session, capped at `max_builds`.
  accepted:    AtomicU32,
  /// Exit after this many accepted builds. `None` = unbounded.
  max_builds:  Option<u32>,
  /// Set on exit so `assign` refuses further work while draining.
  draining:    AtomicBool,
  /// Last assign/completion time; seeded to connect time so an idle agent
  /// still hits the idle limit.
  last_active: Mutex<Instant>,
}

impl Lifecycle {
  fn new(max_builds: Option<u32>) -> Self {
    Self {
      completed: AtomicU32::new(0),
      accepted: AtomicU32::new(0),
      max_builds,
      draining: AtomicBool::new(false),
      last_active: Mutex::new(Instant::now()),
    }
  }

  fn touch(&self) {
    *self.last_active.lock() = Instant::now();
  }

  fn is_draining(&self) -> bool {
    self.draining.load(Ordering::Relaxed)
  }

  /// Reserve one build against `max_builds`, draining at the cap so the
  /// agent never accepts one past it.
  fn reserve_build(&self) -> bool {
    if self.is_draining() {
      return false;
    }
    let Some(max) = self.max_builds else {
      return true;
    };
    let prev = self.accepted.fetch_add(1, Ordering::AcqRel);
    if prev >= max {
      self.accepted.fetch_sub(1, Ordering::AcqRel);
      self.draining.store(true, Ordering::Relaxed);
      return false;
    }
    if prev + 1 >= max {
      self.draining.store(true, Ordering::Relaxed);
    }
    true
  }
}

/// Cancel `quit` once an exit condition is reached (max builds, lifetime, or
/// idle), setting `draining` first so no further work is accepted.
async fn ephemeral_monitor(
  eph: EphemeralConfig,
  lifecycle: Arc<Lifecycle>,
  quit: CancellationToken,
) {
  let start = Instant::now();
  let mut ticker = tokio::time::interval(Duration::from_secs(1));
  ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
  loop {
    ticker.tick().await;

    let lifetime_reached = eph
      .max_lifetime_secs
      .is_some_and(|max| start.elapsed() >= Duration::from_secs(max));
    let running = JOB_COUNTER.load(Ordering::Relaxed);

    if lifetime_reached {
      tracing::info!(
        running,
        "ephemeral: max_lifetime reached; draining and exiting"
      );
      break;
    }

    // The build-count and idle limits only apply once idle.
    if running > 0 {
      continue;
    }

    if eph
      .max_builds
      .is_some_and(|max| lifecycle.completed.load(Ordering::Relaxed) >= max)
    {
      tracing::info!("ephemeral: max_builds reached; exiting");
      break;
    }

    let idle = lifecycle.last_active.lock().elapsed();
    if idle >= Duration::from_secs(eph.max_idle_secs) {
      tracing::info!(
        idle_secs = idle.as_secs(),
        "ephemeral: idle limit reached; exiting"
      );
      break;
    }
  }

  lifecycle.draining.store(true, Ordering::Relaxed);
  quit.cancel();
}

/// The `Builder` capability we expose to the runner.
///
/// Each `assign` spawns a build task and reports the result via the
/// `ResultSink` the runner gave us. `abort(build_id)` signals the
/// per-build [`CancellationToken`] stored here; the build task selects
/// on it and SIGTERMs the child immediately.
struct BuilderImpl {
  inner: Arc<BuilderInner>,
}

struct BuilderInner {
  max_jobs:   u32,
  cores:      u32,
  machine_id: String,
  /// Runner capability, used to request presigned URLs and notify the
  /// runner of upload completion. Cloning a capnp client is cheap.
  runner_cap: runner::Client,
  /// `build_id` -> `CancellationToken`. Inserted by `assign`, removed by
  /// the per-build task at completion, signalled by `abort`.
  running:    Mutex<HashMap<Uuid, CancellationToken>>,
  /// Indicates whether the builder will use rootless, sandboxed Nix.
  rootless:   bool,
  /// Ephemeral session lifecycle; `None` for persistent agents.
  lifecycle:  Option<Arc<Lifecycle>>,
}

impl BuilderImpl {
  #[expect(
    clippy::arc_with_non_send_sync,
    reason = "BuilderInner is intentionally !Send + !Sync; the agent runs on \
              a single-threaded tokio runtime so an Arc is never actually \
              shared across threads"
  )]
  fn new(
    max_jobs: u32,
    cores: u32,
    machine_id: Uuid,
    runner_cap: runner::Client,
    rootless: bool,
    lifecycle: Option<Arc<Lifecycle>>,
  ) -> Self {
    Self {
      inner: Arc::new(BuilderInner {
        max_jobs,
        cores,
        machine_id: machine_id.to_string(),
        runner_cap,
        running: Mutex::new(HashMap::new()),
        rootless,
        lifecycle,
      }),
    }
  }
}

#[allow(refining_impl_trait_internal)]
impl builder::Server for BuilderImpl {
  fn assign(
    self: capnp::capability::Rc<Self>,
    params: builder::AssignParams,
    _results: builder::AssignResults,
  ) -> Promise<(), capnp::Error> {
    let inner = Arc::clone(&self.inner);
    Promise::from_future(async move {
      let pr = params.get()?;
      let job = pr.get_job()?;
      let build_id_str = job.get_build_id()?.to_str()?.to_owned();
      let build_id = Uuid::parse_str(&build_id_str)
        .map_err(|e| capnp::Error::failed(format!("bad build_id: {e}")))?;
      let drv_path = job.get_drv_path()?.to_str()?.to_owned();
      let cache_substituter = job.get_cache_substituter()?.to_str()?.to_owned();
      let cache_public_key = job.get_cache_public_key()?.to_str()?.to_owned();
      let max_log_size = job.get_max_log_size();
      let max_silent_time = job.get_max_silent_time();
      let build_timeout = job.get_build_timeout();
      let extra: Vec<String> = job
        .get_extra_nix_args()?
        .iter()
        .map(|s| -> Result<String, capnp::Error> {
          Ok(s?.to_str()?.to_owned())
        })
        .collect::<Result<_, _>>()?;
      // Optional presigned-upload opts. If the runner passes
      // PresignedUploadOpts, the agent does the binary-cache push
      // itself before reporting BuildResult.
      let presign_compression: Option<String> = {
        let opts = job.get_presigned_upload()?;
        if opts.has_compression() {
          let c = opts.get_compression()?.to_str()?;
          if c.is_empty() {
            None
          } else {
            Some(c.to_owned())
          }
        } else {
          None
        }
      };
      let fail_build_on_upload_error = job
        .get_presigned_upload()
        .is_ok_and(
          circus_proto::presigned_upload_opts::Reader::get_fail_build_on_upload_error,
        );
      let log: log_sink::Client = pr.get_log()?;
      let result: result_sink::Client = pr.get_result()?;
      let output = if pr.has_output() {
        Some(pr.get_output()?)
      } else {
        None
      };

      // Draining: refuse like the max-jobs case so the runner reschedules.
      if inner.lifecycle.as_ref().is_some_and(|l| l.is_draining()) {
        return Err(capnp::Error::failed(
          "agent draining; not accepting new builds".into(),
        ));
      }

      let cancel = CancellationToken::new();
      {
        let mut g = inner.running.lock();
        if g.len() as u32 >= inner.max_jobs {
          return Err(capnp::Error::failed(
            "agent at max_jobs; refusing assignment".into(),
          ));
        }
        if g.contains_key(&build_id) {
          return Err(capnp::Error::failed(format!(
            "build_id {build_id} is already running"
          )));
        }
        if let Some(l) = inner.lifecycle.as_ref()
          && !l.reserve_build()
        {
          return Err(capnp::Error::failed(
            "agent reached max_builds; draining".into(),
          ));
        }
        g.insert(build_id, cancel.clone());
      }
      JOB_COUNTER.fetch_add(1, Ordering::Relaxed);
      if let Some(lc) = &inner.lifecycle {
        lc.touch();
      }

      let inner_for_task = Arc::clone(&inner);
      tokio::task::spawn_local(async move {
        let mut outcome = build::run(
          build::BuildOptions {
            drv_path: &drv_path,
            max_log_size,
            max_silent_time: Duration::from_secs(max_silent_time.into()),
            build_timeout: Duration::from_secs(build_timeout.into()),
            cores: inner_for_task.cores,
            extra_args: extra,
            cache_substituter,
            cache_public_key,
            rootless: inner_for_task.rootless,
          },
          log,
          cancel,
        )
        .await;

        // Presigned upload (best-effort, mirrors Hydra). Only run when
        // the build succeeded and the runner asked for it. Failures land
        // in error_message but do not flip the BuildOutcome: the build
        // bytes are correct on this host even if the push failed.
        if let (Some(compression), Ok(ref mut local)) =
          (presign_compression, outcome.as_mut())
          && matches!(local.outcome, circus_proto::BuildOutcome::Success)
          && !local.outputs.is_empty()
        {
          match crate::upload::upload_all(
            &inner_for_task.runner_cap,
            &inner_for_task.machine_id,
            &build_id_str,
            &compression,
            &local.outputs,
            inner_for_task.rootless,
          )
          .await
          {
            Ok(stats) => {
              local.upload_time_ms = stats.elapsed_ms;
              if !stats.failures.is_empty() {
                let mut msg = String::from("upload failures: ");
                for (path, why) in &stats.failures {
                  let _ = write!(msg, "[{path} -> {why}] ");
                }
                if !local.error_message.is_empty() {
                  local.error_message.push('\n');
                }
                local.error_message.push_str(msg.trim_end());
                if fail_build_on_upload_error {
                  local.outcome = circus_proto::BuildOutcome::UploadFailure;
                  local.exit_code = 1;
                }
              }
              tracing::info!(
                %build_id,
                ok = stats.successes.len(),
                fail = stats.failures.len(),
                elapsed_ms = stats.elapsed_ms,
                "presigned upload finished"
              );
            },
            Err(e) => {
              tracing::warn!(%build_id, "presigned upload errored: {e}");
              if !local.error_message.is_empty() {
                local.error_message.push('\n');
              }
              let _ = write!(local.error_message, "upload: {e}");
              if fail_build_on_upload_error {
                local.outcome = circus_proto::BuildOutcome::UploadFailure;
                local.exit_code = 1;
              }
            },
          }
        }

        // Best-effort, like the S3 upload above
        if let (Some(sink), Ok(local)) = (&output, outcome.as_ref())
          && matches!(local.outcome, circus_proto::BuildOutcome::Success)
          && !local.outputs.is_empty()
        {
          let paths = local
            .outputs
            .iter()
            .map(|o| o.path.clone())
            .collect::<Vec<String>>();
          if let Err(e) =
            export_outputs_to_sink(sink, &paths, inner_for_task.rootless).await
          {
            tracing::warn!(
              %build_id,
              "output closure transfer to runner failed: {e}"
            );
          }
        }

        match &outcome {
          Ok(r) if matches!(r.outcome, circus_proto::BuildOutcome::Success) => {
            tracing::info!(
              %build_id,
              build_time_ms = r.build_time_ms,
              "build succeeded"
            );
          },
          Ok(r) => {
            tracing::warn!(
              %build_id,
              outcome = ?r.outcome,
              exit_code = r.exit_code,
              error = %r.error_message,
              "build failed"
            );
          },
          Err(e) => tracing::error!(%build_id, "build run errored: {e}"),
        }

        if let Err(e) = report_result(&result, outcome).await {
          tracing::warn!(%build_id, "result sink failed: {e}");
        }
        JOB_COUNTER.fetch_sub(1, Ordering::Relaxed);
        inner_for_task.running.lock().remove(&build_id);
        if let Some(lc) = &inner_for_task.lifecycle {
          lc.completed.fetch_add(1, Ordering::Relaxed);
          lc.touch();
        }
      });
      Ok(())
    })
  }

  fn abort(
    self: capnp::capability::Rc<Self>,
    params: builder::AbortParams,
    _results: builder::AbortResults,
  ) -> Promise<(), capnp::Error> {
    let inner = Arc::clone(&self.inner);
    Promise::from_future(async move {
      let pr = params.get()?;
      let id_str = pr.get_build_id()?.to_str()?;
      if let Ok(id) = Uuid::parse_str(id_str) {
        let value = inner.running.lock().get(&id).cloned();
        if let Some(tok) = value {
          tok.cancel();
          tracing::info!(%id, "aborting build per runner request");
        } else {
          tracing::warn!(%id, "abort for unknown build_id; ignoring");
        }
      }
      Ok(())
    })
  }

  fn shutdown(
    self: capnp::capability::Rc<Self>,
    params: builder::ShutdownParams,
    _results: builder::ShutdownResults,
  ) -> Promise<(), capnp::Error> {
    if let Ok(p) = params.get()
      && let Ok(reason) = p.get_reason()
      && let Ok(reason_str) = reason.to_str()
    {
      tracing::info!(reason = reason_str, "shutdown requested by runner");
    }
    // Cancel every in-flight build so they wrap up quickly. The
    // supervisor loop in `main` reconnects after the connection drops.
    let inner = Arc::clone(&self.inner);
    Promise::from_future(async move {
      for (_, tok) in inner.running.lock().drain() {
        tok.cancel();
      }
      Ok(())
    })
  }
}

async fn report_result(
  sink: &result_sink::Client,
  outcome: color_eyre::Result<build::LocalResult>,
) -> Result<(), capnp::Error> {
  #![expect(
    clippy::future_not_send,
    reason = "capnp futures are not Send; agent uses a single-threaded runtime"
  )]
  let mut req = sink.report_request();
  let mut r = req.get().init_result();
  match outcome {
    Ok(local) => {
      r.set_outcome(local.outcome);
      r.set_exit_code(local.exit_code);
      r.set_build_time_ms(local.build_time_ms);
      r.set_upload_time_ms(local.upload_time_ms);
      r.set_error_message(local.error_message.as_str());
      let mut outs = r.reborrow().init_outputs(local.outputs.len() as u32);
      for (i, o) in local.outputs.iter().enumerate() {
        let mut slot = outs.reborrow().get(i as u32);
        slot.set_name(o.name.as_str());
        slot.set_path(o.path.as_str());
      }
    },
    Err(e) => {
      r.set_outcome(circus_proto::BuildOutcome::PreparingFailure);
      r.set_exit_code(-1);
      r.set_error_message(format!("{e}").as_str());
    },
  }
  req.send().promise.await?;
  Ok(())
}

/// Stream the closure of `output_paths` to the runner's `OutputSink`. A
/// successful return means the runner has imported it into its store.
async fn export_outputs_to_sink(
  sink: &output_sink::Client,
  output_paths: &[String],
  rootless: bool,
) -> color_eyre::Result<()> {
  #![expect(
    clippy::future_not_send,
    reason = "capnp futures are not Send; agent uses a single-threaded runtime"
  )]
  use tokio::io::AsyncReadExt as _;

  // Import needs references registered first, so ship the whole closure.
  let closure = query_requisites(output_paths, rootless).await?;
  if closure.is_empty() {
    return Ok(());
  }

  let mut cmd = crate::sandbox::nix_command(rootless, NixTool::NixStore)?;
  cmd
    .arg("--export")
    .args(&closure)
    .stdout(std::process::Stdio::piped())
    .kill_on_drop(true);
  let mut cmd = crate::sandbox::wrap_command(rootless, cmd)?;
  let mut child = cmd.spawn().context("spawn nix-store --export")?;
  let mut stdout = child
    .stdout
    .take()
    .ok_or_else(|| eyre!("export stdout missing"))?;

  let mut buf = vec![0u8; 1024 * 1024];
  let mut stream_err = None;
  loop {
    let n = match stdout.read(&mut buf).await {
      Ok(0) => break,
      Ok(n) => n,
      Err(e) => {
        stream_err = Some(eyre!("read nix-store --export: {e}"));
        break;
      },
    };
    let mut req = sink.write_request();
    req.get().set_chunk(&buf[..n]);
    if let Err(e) = req.send().promise.await {
      stream_err = Some(eyre!("stream output closure: {e}"));
      break;
    }
  }

  // Always close so the runner reaps its import child, even on a short read.
  let close_res = sink.close_request().send().promise.await;
  if let Some(e) = stream_err {
    return Err(e);
  }
  let status = child.wait().await?;
  if !status.success() {
    bail!("nix-store --export exited with {status}");
  }
  close_res
    .map_err(|e| eyre!("runner failed to import output closure: {e}"))?;
  Ok(())
}

async fn query_requisites(
  output_paths: &[String],
  rootless: bool,
) -> color_eyre::Result<Vec<String>> {
  let mut cmd = crate::sandbox::nix_command(rootless, NixTool::NixStore)?;
  cmd
    .arg("--query")
    .arg("--requisites")
    .args(output_paths)
    .stdout(std::process::Stdio::piped());
  let mut cmd = crate::sandbox::wrap_command(rootless, cmd)?;
  let out = cmd
    .output()
    .await
    .context("nix-store --query --requisites")?;
  if !out.status.success() {
    bail!("nix-store --query --requisites exited with {}", out.status);
  }
  Ok(
    String::from_utf8_lossy(&out.stdout)
      .lines()
      .map(str::to_owned)
      .filter(|s| !s.is_empty())
      .collect(),
  )
}
