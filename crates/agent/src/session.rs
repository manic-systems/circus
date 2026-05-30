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
  sync::{
    Arc,
    atomic::{AtomicU32, Ordering},
  },
  time::Duration,
};

use anyhow::Context as _;
use capnp::capability::Promise;
use capnp_rpc::{RpcSystem, rpc_twoparty_capnp, twoparty};
use circus_proto::{
  PROTO_VERSION,
  agent_info,
  agent_session,
  builder,
  heartbeat,
  log_sink,
  pressure_state,
  result_sink,
  runner,
};
use parking_lot::Mutex;
use tokio::net::TcpStream;
use tokio_util::{
  compat::{TokioAsyncReadCompatExt as _, TokioAsyncWriteCompatExt as _},
  sync::CancellationToken,
};
use uuid::Uuid;

use crate::{build, config::Agent, psi};

/// Open a connection and run it to completion.
///
/// Returns when the runner disconnects or the local builder side fails.
/// The caller (`main`) implements reconnect with backoff.
///
/// # Errors
/// Network or RPC errors. Connection-time errors (`connect`, `register`)
/// are bubbled; mid-stream errors land in tracing and end the function.
pub async fn run_once(cfg: &Agent, machine_id: Uuid) -> anyhow::Result<()> {
  let (host, port, want_tls) = parse_endpoint(&cfg.runner_url)?;
  tracing::info!(host = %host, port, want_tls, "dialing runner");
  let socket = TcpStream::connect((host.as_str(), port))
    .await
    .with_context(|| format!("connect to runner {host}:{port}"))?;
  socket.set_nodelay(true).ok();

  let want_tls = want_tls || cfg.tls.is_some();

  // Branch on TLS at the type level: keep both arms inside the RPC system
  // by erasing through `Box<dyn>` and the tokio-util compat adapters.
  let mut rpc = if want_tls {
    let tls = cfg.tls.as_ref().ok_or_else(|| {
      anyhow::anyhow!("circus+tls:// requested but [agent.tls] not configured")
    })?;
    let connector = crate::tls::build_client_connector(tls)?;
    let server_name = rustls::pki_types::ServerName::try_from(host.clone())
      .map_err(|e| anyhow::anyhow!("invalid server name {host}: {e}"))?;
    let stream = connector.connect(server_name, socket).await?;
    let (rh, wh) = tokio::io::split(stream);
    let network = twoparty::VatNetwork::new(
      rh.compat(),
      wh.compat_write(),
      rpc_twoparty_capnp::Side::Client,
      Default::default(),
    );
    RpcSystem::new(Box::new(network), None)
  } else {
    let (read_half, write_half) = socket.into_split();
    let network = twoparty::VatNetwork::new(
      read_half.compat(),
      write_half.compat_write(),
      rpc_twoparty_capnp::Side::Client,
      Default::default(),
    );
    RpcSystem::new(Box::new(network), None)
  };

  let runner_cap: runner::Client =
    rpc.bootstrap(rpc_twoparty_capnp::Side::Server);
  let disconnector = rpc.get_disconnector();

  let local_builder: builder::Client = capnp_rpc::new_client(BuilderImpl::new(
    cfg.max_jobs,
    machine_id,
    runner_cap.clone(),
  ));

  let rpc_join = tokio::task::spawn_local(async move {
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
  );

  rpc_join.await.ok();
  heartbeat_join.abort();
  let _ = disconnector.await;
  Ok(())
}

fn parse_endpoint(url: &str) -> anyhow::Result<(String, u16, bool)> {
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
    return Err(anyhow::anyhow!("unsupported runner_url scheme: {scheme}"));
  }
  let host = parsed
    .host_str()
    .ok_or_else(|| anyhow::anyhow!("missing host in runner_url"))?
    .to_owned();
  let port = parsed
    .port()
    .ok_or_else(|| anyhow::anyhow!("missing port in runner_url"))?;
  Ok((host, port, tls))
}

async fn verify_runner_version(
  runner_cap: &runner::Client,
) -> anyhow::Result<()> {
  let response = runner_cap
    .version_request()
    .send()
    .promise
    .await
    .context("version")?;
  let payload = response.get().context("version response")?;
  let proto = payload.get_proto()?.to_str()?;
  if proto != PROTO_VERSION {
    return Err(anyhow::anyhow!(
      "proto mismatch: runner={proto} agent={PROTO_VERSION}"
    ));
  }
  Ok(())
}

async fn register(
  runner_cap: &runner::Client,
  cfg: &Agent,
  machine_id: Uuid,
  local_builder: builder::Client,
) -> anyhow::Result<agent_session::Client> {
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

/// Best-effort hostname read. Falls back to the configured agent name
/// when /etc/hostname is unavailable; the runner only treats the field
/// as a display label, not as identity.
fn read_hostname() -> String {
  std::fs::read_to_string("/etc/hostname")
    .map(|s| s.trim().to_owned())
    .ok()
    .filter(|s| !s.is_empty())
    .unwrap_or_else(|| "unknown".into())
}

fn num_cpus() -> usize {
  std::thread::available_parallelism()
    .map(std::num::NonZeroUsize::get)
    .unwrap_or(1)
}

fn spawn_heartbeat(
  session: agent_session::Client,
  interval: Duration,
) -> tokio::task::JoinHandle<()> {
  tokio::task::spawn_local(async move {
    let mut ticker = tokio::time::interval(interval);
    ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    loop {
      ticker.tick().await;
      if let Err(e) = send_heartbeat(&session).await {
        tracing::warn!("heartbeat failed: {e}; ending loop");
        break;
      }
    }
  })
}

async fn send_heartbeat(
  session: &agent_session::Client,
) -> Result<(), capnp::Error> {
  let mut req = session.heartbeat_request();
  let mut ping: heartbeat::Builder<'_> = req.get().init_ping();
  let load = read_loadavg();
  ping.set_load1(load.0);
  ping.set_load5(load.1);
  ping.set_load15(load.2);
  ping.set_current_jobs(JOB_COUNTER.load(Ordering::Relaxed));

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
  let Ok(s) = std::fs::read_to_string("/proc/loadavg") else {
    return (0.0, 0.0, 0.0);
  };
  let mut it = s.split_whitespace();
  let a = it.next().and_then(|v| v.parse().ok()).unwrap_or(0.0);
  let b = it.next().and_then(|v| v.parse().ok()).unwrap_or(0.0);
  let c = it.next().and_then(|v| v.parse().ok()).unwrap_or(0.0);
  (a, b, c)
}

/// Process-global counter for concurrent builds. Bumped on `assign`,
/// dropped on result. Exposed in heartbeats.
static JOB_COUNTER: AtomicU32 = AtomicU32::new(0);

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
  machine_id: String,
  /// Runner capability, used to request presigned URLs and notify the
  /// runner of upload completion. Cloning a capnp client is cheap.
  runner_cap: runner::Client,
  /// build_id -> CancellationToken. Inserted by `assign`, removed by
  /// the per-build task at completion, signalled by `abort`.
  running:    Mutex<HashMap<Uuid, CancellationToken>>,
}

impl BuilderImpl {
  fn new(max_jobs: u32, machine_id: Uuid, runner_cap: runner::Client) -> Self {
    Self {
      inner: Arc::new(BuilderInner {
        max_jobs,
        machine_id: machine_id.to_string(),
        runner_cap,
        running: Mutex::new(HashMap::new()),
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
      let log: log_sink::Client = pr.get_log()?;
      let result: result_sink::Client = pr.get_result()?;

      let cancel = CancellationToken::new();
      {
        let mut g = inner.running.lock();
        if g.len() as u32 >= inner.max_jobs {
          return Err(capnp::Error::failed(
            "agent at max_jobs; refusing assignment".into(),
          ));
        }
        g.insert(build_id, cancel.clone());
      }
      JOB_COUNTER.fetch_add(1, Ordering::Relaxed);

      let inner_for_task = Arc::clone(&inner);
      tokio::task::spawn_local(async move {
        let mut outcome = build::run(
          build::BuildOptions {
            drv_path: &drv_path,
            max_log_size,
            max_silent_time: Duration::from_secs(max_silent_time.into()),
            build_timeout: Duration::from_secs(build_timeout.into()),
            extra_args: extra,
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
          )
          .await
          {
            Ok(stats) => {
              local.upload_time_ms = stats.elapsed_ms;
              if !stats.failures.is_empty() {
                let mut msg = String::from("upload failures: ");
                for (path, why) in &stats.failures {
                  use std::fmt::Write as _;
                  let _ = write!(msg, "[{path} -> {why}] ");
                }
                if !local.error_message.is_empty() {
                  local.error_message.push('\n');
                }
                local.error_message.push_str(msg.trim_end());
                local.outcome = circus_proto::BuildOutcome::UploadFailure;
                local.exit_code = 1;
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
              local.error_message.push_str(&format!("upload: {e}"));
            },
          }
        }

        if let Err(e) = report_result(&result, outcome).await {
          tracing::warn!(%build_id, "result sink failed: {e}");
        }
        JOB_COUNTER.fetch_sub(1, Ordering::Relaxed);
        inner_for_task.running.lock().remove(&build_id);
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
        if let Some(tok) = inner.running.lock().get(&id).cloned() {
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
  outcome: anyhow::Result<build::LocalResult>,
) -> Result<(), capnp::Error> {
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
