//! capnp-rpc TCP server. Accept loop, bootstrap capability, register
//! handler, and the per-connection dispatch pump.
//!
//! Threading: this entire module runs inside a `tokio::task::LocalSet`
//! pinned to one thread. Capnp-rpc capabilities are `Rc`-backed and
//! `!Send`, so the connection task is the only place they exist. The
//! scheduler (on the multi-threaded runtime) hands off work via
//! `mpsc::UnboundedSender<DispatchCommand>` channels held in
//! [`super::pool::AgentMeta`].

use std::{
  collections::HashMap,
  net::SocketAddr,
  sync::{Arc, Weak},
};

use capnp::capability::Promise;
use capnp_rpc::{RpcSystem, rpc_twoparty_capnp, twoparty};
use circus_common::{BuildStatus, PgPool, repo};
use circus_proto::{
  PROTO_VERSION,
  agent_session,
  build_assignment,
  builder,
  drv_sink,
  limits,
  log_sink,
  result_sink,
  runner,
};
use color_eyre::eyre::{Context as _, bail};
use sha2::{Digest as _, Sha256};
use subtle::ConstantTimeEq as _;
use tokio::{
  net::TcpListener,
  sync::{Semaphore, mpsc, oneshot},
};
use tokio_rustls::TlsAcceptor;
use tokio_util::compat::{
  TokioAsyncReadCompatExt as _,
  TokioAsyncWriteCompatExt as _,
};
use uuid::Uuid;
use x509_parser::prelude::FromDer;

use super::{
  AgentPool,
  log_sink::LogSinkImpl,
  output_sink::OutputSinkImpl,
  pool::{AgentMeta, DispatchCommand, DispatchResult, EffectContext},
  result_sink::{BuildOutcomeKind, ResultSinkImpl},
  session::SessionImpl,
};
use crate::rpc::pool::MAX_AGENT_MAX_JOBS;

#[derive(Clone)]
pub struct ServerConfig {
  pub bind:               SocketAddr,
  /// SHA-256 hex digests of accepted bearer tokens. Empty = reject all.
  pub token_hashes:       Vec<String>,
  pub max_connections:    usize,
  /// Optional TLS. `None` means plain TCP.
  pub tls:                Option<TlsState>,
  /// Optional S3 presigner. `None` disables the presigned-upload path;
  /// agents that request a presigned URL get a per-entry error in the
  /// response.
  pub presigner:          Option<Arc<super::s3::Presigner>>,
  /// How long presigned PUT URLs are valid for. Defaults to one hour.
  pub presign_expiry:     std::time::Duration,
  /// Wire compression advertised to agents for the presigned-upload path.
  /// Must match `CacheUploadConfig::compression` so the S3 key suffix and
  /// the narinfo `Compression:` field agree. Defaults to `"zstd"`.
  pub upload_compression: String,
  /// Path to the Ed25519 signing key (Nix format
  /// `<key-name>:<base64-secret>`). When set, narinfo records are signed
  /// before persistence so cache fetchers see a trust-rooted entry.
  pub signing_key_file:   Option<std::path::PathBuf>,
  /// Cache forwarded to agents so they can substitute drv closures.
  pub cache_substituter:  Option<String>,
  /// Key forwarded to agents so they can substitute drv closures.
  pub cache_public_key:   Option<String>,
  /// Public Circus HTTP API base URL exposed to effects.
  pub api_base_url:       String,
  /// OIDC verifier. `None` disables the OIDC auth path.
  pub oidc:               Option<Arc<super::oidc::OidcVerifier>>,
  /// Mints `GitToken` secrets for effects that request one.
  pub github_app:         Option<Arc<crate::github_app::GithubApp>>,
  active_uploads: Arc<parking_lot::Mutex<HashMap<UploadKey, ExpectedUpload>>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct UploadKey {
  machine_id: Uuid,
  build_id:   Uuid,
  store_path: String,
}

#[derive(Debug, Clone)]
struct ExpectedUpload {
  nar_hash:    String,
  nar_size:    u64,
  compression: String,
  nar_path:    String,
}

#[derive(Debug, Clone, Copy)]
struct RegisteredAgent {
  machine_id:    Uuid,
  connection_id: Uuid,
}

#[derive(Clone)]
pub struct TlsState {
  pub acceptor: TlsAcceptor,
  /// If true, the registering agent's `name` must equal the CN extracted
  /// from the verified client certificate. Only meaningful when
  /// `RpcTlsConfig.client_ca` was set (mTLS).
  pub pin_cn:   bool,
}

impl ServerConfig {
  /// Build a `ServerConfig` from the user-facing `RpcConfig`. TLS material
  /// is loaded here; failure prevents the listener from starting.
  ///
  /// # Errors
  ///
  /// Returns the underlying error if TLS files are missing or invalid.
  pub fn from_user(cfg: &circus_config::RpcConfig) -> color_eyre::Result<Self> {
    let bind: SocketAddr = cfg
      .bind
      .parse()
      .with_context(|| format!("parse bind {}", cfg.bind))?;
    validate_token_hashes(&cfg.auth_tokens)?;
    let tls = match &cfg.tls {
      None => None,
      Some(tcfg) => {
        Some(TlsState {
          acceptor: super::tls::build_acceptor(tcfg)?,
          pin_cn:   tcfg.pin_cn,
        })
      },
    };
    if cfg.cache_public_key.is_some() && cfg.cache_substituter.is_none() {
      tracing::warn!(
        "[queue_runner.rpc] cache_public_key is set without \
         cache_substituter, this has no effect"
      );
    }

    let oidc = match &cfg.oidc {
      None => None,
      Some(o) => Some(Arc::new(super::oidc::OidcVerifier::new(o)?)),
    };

    Ok(Self {
      bind,
      token_hashes: cfg.auth_tokens.clone(),
      max_connections: cfg.max_connections,
      tls,
      presigner: None,
      presign_expiry: std::time::Duration::from_secs(cfg.presign_expiry_secs),
      upload_compression: "zstd".to_owned(),
      signing_key_file: None,
      cache_substituter: cfg.cache_substituter.clone(),
      cache_public_key: cfg.cache_public_key.clone(),
      api_base_url: cfg.api_base_url.clone().unwrap_or_default(),
      oidc,
      github_app: None,
      active_uploads: Arc::new(parking_lot::Mutex::new(HashMap::new())),
    })
  }

  #[must_use]
  pub fn with_github_app(
    mut self,
    app: Option<Arc<crate::github_app::GithubApp>>,
  ) -> Self {
    self.github_app = app;
    self
  }

  /// Attach the runner's narinfo signing key. When set, the runner
  /// signs every persisted narinfo (matches the SSH-path behaviour
  /// where signing is done by `nix store sign` after copy).
  #[must_use]
  pub fn with_signing_key(
    mut self,
    key_file: Option<std::path::PathBuf>,
  ) -> Self {
    self.signing_key_file = key_file;
    self
  }

  /// Attach an S3 presigner derived from the runner's
  /// `[cache_upload]` config. Returns `Self` unchanged when the cache
  /// config does not point at an S3 bucket.
  #[must_use]
  pub fn with_presigner_from(
    mut self,
    cache_cfg: &circus_config::CacheUploadConfig,
  ) -> Self {
    if let Some(uri) = &cache_cfg.store_uri
      && let Some(s3_cfg) = &cache_cfg.s3
      && let Some(p) = super::s3::Presigner::from_config(uri, s3_cfg)
    {
      self.presigner = Some(Arc::new(p));
      self.upload_compression.clone_from(&cache_cfg.compression);
    }
    self
  }

  fn forget_uploads_for(&self, machine_id: Uuid, build_id: Uuid) {
    self.active_uploads.lock().retain(|key, _| {
      key.machine_id != machine_id || key.build_id != build_id
    });
  }
}

/// Run the accept loop on the current `LocalSet`. The caller is
/// responsible for spawning the runtime + `LocalSet`.
///
/// # Errors
///
/// Returns the underlying error if the listener cannot be bound.
pub async fn serve(
  cfg: ServerConfig,
  pool: Arc<AgentPool>,
  db_pool: PgPool,
) -> color_eyre::Result<()> {
  let listener = TcpListener::bind(cfg.bind)
    .await
    .with_context(|| format!("bind {}", cfg.bind))?;
  tracing::info!(addr = %cfg.bind, tls = cfg.tls.is_some(), "circus-rpc listening");

  if cfg.presigner.is_some() && cfg.signing_key_file.is_none() {
    tracing::warn!(
      "[queue_runner] presigned uploads are enabled without [signing] \
       key_file; uploaded NARs are persisted unsigned and the cache will \
       never serve them"
    );
  }

  let cfg = Arc::new(cfg);
  let connection_permits = Arc::new(Semaphore::new(cfg.max_connections));
  #[expect(clippy::infinite_loop, reason = "intentional accept loop")]
  loop {
    let (socket, peer) = match listener.accept().await {
      Ok(p) => p,
      Err(e) => {
        tracing::warn!("accept error: {e}");
        continue;
      },
    };
    let pool = Arc::clone(&pool);
    let db_pool = db_pool.clone();
    let cfg = Arc::clone(&cfg);
    let permits = Arc::clone(&connection_permits);
    tokio::task::spawn_local(async move {
      let Ok(_permit) = permits.try_acquire_owned() else {
        tracing::warn!(
          ?peer,
          "rpc connection rejected: max_connections reached"
        );
        return;
      };
      match serve_one(socket, peer, cfg, pool, db_pool).await {
        Err(SessionEnd::Registered(e)) => {
          tracing::warn!(?peer, "rpc session ended: {e}");
        },
        Err(SessionEnd::Unregistered(e)) => {
          tracing::debug!(?peer, "rpc session ended before register: {e}");
        },
        Ok(()) => {},
      }
    });
  }
}

enum SessionEnd {
  Registered(color_eyre::Report),
  Unregistered(color_eyre::Report),
}

#[expect(clippy::future_not_send, reason = "capnp future")]
async fn serve_one(
  socket: tokio::net::TcpStream,
  peer: SocketAddr,
  cfg: Arc<ServerConfig>,
  pool: Arc<AgentPool>,
  db_pool: PgPool,
) -> Result<(), SessionEnd> {
  let _ = socket.set_nodelay(true);
  tracing::info!(?peer, "incoming rpc connection");

  let registered_machine: Arc<parking_lot::Mutex<Option<RegisteredAgent>>> =
    Arc::new(parking_lot::Mutex::new(None));
  let rpc_result = run_rpc(
    socket,
    Arc::clone(&cfg),
    Arc::clone(&pool),
    db_pool.clone(),
    Arc::clone(&registered_machine),
  )
  .await;

  let registered = *registered_machine.lock();
  if let Some(registered) = registered {
    let machine_id = registered.machine_id;
    if pool
      .remove_if_connection(&machine_id, registered.connection_id)
      .is_some()
    {
      if let Err(e) =
        repo::builder_sessions::mark_disconnected(&db_pool, machine_id).await
      {
        tracing::warn!(%machine_id, "failed to mark disconnected: {e}");
      }
      tracing::info!(%machine_id, "agent connection closed");
    } else {
      tracing::debug!(
        %machine_id,
        connection_id = %registered.connection_id,
        "stale agent connection closed after replacement"
      );
    }
  }
  rpc_result.map_err(|e| {
    if registered.is_some() {
      SessionEnd::Registered(e)
    } else {
      SessionEnd::Unregistered(e)
    }
  })
}

#[expect(clippy::future_not_send, reason = "capnp future")]
async fn run_rpc(
  socket: tokio::net::TcpStream,
  cfg: Arc<ServerConfig>,
  pool: Arc<AgentPool>,
  db_pool: PgPool,
  registered_machine: Arc<parking_lot::Mutex<Option<RegisteredAgent>>>,
) -> color_eyre::Result<()> {
  if let Some(tls) = cfg.tls.as_ref() {
    let stream = tls.acceptor.clone().accept(socket).await?;
    let peer_cert = extract_peer_cert_identity(&stream);
    let (rh, wh) = tokio::io::split(stream);
    let network = twoparty::VatNetwork::new(
      rh.compat(),
      wh.compat_write(),
      rpc_twoparty_capnp::Side::Server,
      capnp::message::ReaderOptions::default(),
    );
    let runner_impl = RunnerImpl {
      cfg: Arc::clone(&cfg),
      pool,
      db_pool,
      registered_machine,
      peer_cert,
    };
    let runner_cap: runner::Client = capnp_rpc::new_client(runner_impl);
    let rpc = RpcSystem::new(Box::new(network), Some(runner_cap.client));
    rpc.await?;
  } else {
    let (read_half, write_half) = socket.into_split();
    let network = twoparty::VatNetwork::new(
      read_half.compat(),
      write_half.compat_write(),
      rpc_twoparty_capnp::Side::Server,
      capnp::message::ReaderOptions::default(),
    );
    let runner_impl = RunnerImpl {
      cfg: Arc::clone(&cfg),
      pool,
      db_pool,
      registered_machine,
      peer_cert: PeerCertIdentity::default(),
    };
    let runner_cap: runner::Client = capnp_rpc::new_client(runner_impl);
    let rpc = RpcSystem::new(Box::new(network), Some(runner_cap.client));
    rpc.await?;
  }
  Ok(())
}

#[derive(Clone, Default)]
struct PeerCertIdentity {
  presented: bool,
  name:      Option<String>,
}

/// Extract the pinning name from the peer's verified client certificate.
///
/// rustls gives us the DER-encoded certificate chain, and we only need the
/// leaf name. Prefer DNS SANs and fall back to the Subject CN.
fn extract_peer_cert_identity(
  stream: &tokio_rustls::server::TlsStream<tokio::net::TcpStream>,
) -> PeerCertIdentity {
  let (_, server_conn) = stream.get_ref();
  let Some(peer) = server_conn.peer_certificates().and_then(|c| c.first())
  else {
    return PeerCertIdentity::default();
  };
  PeerCertIdentity {
    presented: true,
    name:      parse_cert_name(peer.as_ref()),
  }
}

fn parse_cert_name(der: &[u8]) -> Option<String> {
  let (_, cert) =
    x509_parser::certificate::X509Certificate::from_der(der).ok()?;
  if let Ok(Some(san)) = cert.subject_alternative_name() {
    for name in &san.value.general_names {
      if let x509_parser::extensions::GeneralName::DNSName(dns) = name {
        return Some((*dns).to_owned());
      }
    }
  }
  for attr in cert.subject().iter_attributes() {
    if attr.attr_type().to_id_string() == "2.5.4.3"
      && let Ok(val) = attr.attr_value().as_str()
    {
      return Some(val.to_owned());
    }
  }
  None
}

struct RunnerImpl {
  cfg:                Arc<ServerConfig>,
  pool:               Arc<AgentPool>,
  db_pool:            PgPool,
  registered_machine: Arc<parking_lot::Mutex<Option<RegisteredAgent>>>,
  /// Client certificate identity, if one was presented.
  peer_cert:          PeerCertIdentity,
}

#[allow(refining_impl_trait_internal, refining_impl_trait_reachable)]
impl runner::Server for RunnerImpl {
  fn register(
    self: capnp::capability::Rc<Self>,
    params: runner::RegisterParams,
    mut results: runner::RegisterResults,
  ) -> Promise<(), capnp::Error> {
    let cfg = Arc::clone(&self.cfg);
    let pool = Arc::clone(&self.pool);
    let db_pool = self.db_pool.clone();
    let registered_slot = Arc::clone(&self.registered_machine);
    let peer_cert = self.peer_cert.clone();
    Promise::from_future(async move {
      let pr = params.get()?;
      let info = pr.get_info()?;
      let builder_cap: builder::Client = pr.get_builder()?;

      let machine_id_str = info.get_machine_id()?.to_str()?;
      let machine_id = Uuid::parse_str(machine_id_str)
        .map_err(|e| capnp::Error::failed(format!("bad machine_id: {e}")))?;
      let name = info.get_name()?.to_str()?.to_owned();
      let hostname = info.get_hostname()?.to_str()?.to_owned();
      let proto = info.get_proto_version()?.to_str()?;
      if proto != PROTO_VERSION {
        return Err(capnp::Error::failed(format!(
          "proto mismatch: agent={proto} runner={PROTO_VERSION}"
        )));
      }
      let token = info.get_auth_token()?.to_str()?;
      validate_text_len(
        "agent.auth_token",
        token,
        1,
        limits::MAX_AUTH_TOKEN_LEN,
      )?;
      // Bearer token first (cheap, constant-time); fall through to OIDC so a
      // JWT presented in the same field is verified against the issuer's JWKS.
      let (auth_kind, oidc_identity) = if verify_token(&cfg.token_hashes, token)
      {
        (circus_common::models::AuthKind::Token, None)
      } else if let Some(verifier) = cfg.oidc.as_ref() {
        match verifier.verify(token).await {
          Ok(id) => {
            tracing::info!(name = %name, repository = %id.repository, subject = %id.subject, "agent authenticated via OIDC");
            (circus_common::models::AuthKind::Oidc, Some(id))
          },
          Err(e) => {
            tracing::warn!(name = %name, "OIDC auth failed: {e}");
            return Err(capnp::Error::failed("auth failed".into()));
          },
        }
      } else {
        tracing::warn!(name = %name, "bad auth token from agent");
        return Err(capnp::Error::failed("auth failed".into()));
      };

      if cfg.tls.as_ref().is_some_and(|t| t.pin_cn) && peer_cert.presented {
        match peer_cert.name.as_deref() {
          Some(cert_name) if cert_name == name => {},
          Some(cert_name) => {
            tracing::warn!(name = %name, cert_name, "cert name does not match agent name");
            return Err(capnp::Error::failed("cert/name mismatch".into()));
          },
          None => {
            tracing::warn!(name = %name, "client cert has no name to pin");
            return Err(capnp::Error::failed(
              "client cert has no pinned name".into(),
            ));
          },
        }
      }

      validate_text_len("agent.name", &name, 1, limits::MAX_AGENT_NAME_LEN)?;
      validate_text_len(
        "agent.hostname",
        &hostname,
        1,
        limits::MAX_HOSTNAME_LEN,
      )?;

      let systems = read_bounded_text_list(
        info.get_systems()?,
        "systems",
        limits::MAX_SYSTEMS,
        limits::MAX_FEATURE_LEN,
      )?;
      let supported = read_bounded_text_list(
        info.get_supported_features()?,
        "supported_features",
        limits::MAX_FEATURES,
        limits::MAX_FEATURE_LEN,
      )?;
      let mandatory = read_bounded_text_list(
        info.get_mandatory_features()?,
        "mandatory_features",
        limits::MAX_FEATURES,
        limits::MAX_FEATURE_LEN,
      )?;
      let speed = info.get_speed_factor();
      let cpu = info.get_cpu_count();
      let maxj = info.get_max_jobs();

      // An OIDC identity is always ephemeral.
      let ephemeral = info.get_ephemeral()
        || auth_kind == circus_common::models::AuthKind::Oidc;
      let requested_effects = info.get_effects();
      let effects = effect_capability_enabled(
        requested_effects,
        ephemeral,
        cfg.cache_substituter.as_deref(),
        cfg.cache_public_key.as_deref(),
      );
      if requested_effects && !effects {
        tracing::warn!(
          name = %name,
          ephemeral,
          cache_substituter = configured_nonempty(
            cfg.cache_substituter.as_deref()
          ),
          cache_public_key = configured_nonempty(
            cfg.cache_public_key.as_deref()
          ),
          "Ignoring advertised Effects capability because prerequisites are \
           missing"
        );
      }
      validate_agent_capacity(&systems, speed, cpu, maxj)?;

      let connection_id = Uuid::new_v4();
      {
        let mut slot = registered_slot.lock();
        if slot.is_some() {
          return Err(capnp::Error::failed(
            "connection is already registered".into(),
          ));
        }
        *slot = Some(RegisteredAgent {
          machine_id,
          connection_id,
        });
      }

      if let Err(e) = repo::builder_sessions::register(
        &db_pool,
        repo::builder_sessions::RegisterSession {
          machine_id,
          name: &name,
          hostname: &hostname,
          systems: &systems,
          supported_features: &supported,
          mandatory_features: &mandatory,
          speed_factor: speed,
          cpu_count: cpu as i32,
          max_jobs: maxj as i32,
          proto_version: PROTO_VERSION,
          ephemeral,
          auth_kind: auth_kind.as_str(),
        },
      )
      .await
      {
        *registered_slot.lock() = None;
        tracing::warn!("upsert builder_session: {e}");
        return Err(capnp::Error::failed(format!(
          "builder session upsert failed: {e}"
        )));
      }

      let (tx, rx) = mpsc::unbounded_channel::<DispatchCommand>();
      let meta = Arc::new(AgentMeta::new(
        machine_id,
        connection_id,
        name.clone(),
        hostname,
        systems,
        supported,
        mandatory,
        speed,
        cpu,
        maxj,
        ephemeral,
        effects,
        auth_kind,
        oidc_identity.as_ref().map(|id| id.repository.clone()),
        oidc_identity.as_ref().map(|id| id.subject.clone()),
        tx,
      ));
      if let Some(previous) = pool.insert(Arc::clone(&meta)) {
        tracing::warn!(
          name = %name,
          %machine_id,
          old_connection_id = %previous.connection_id,
          %connection_id,
          "agent registration replaced an existing live connection"
        );
      }
      tracing::info!(name = %name, ?machine_id, "agent registered");

      tokio::task::spawn_local(run_dispatch_pump(
        builder_cap,
        Arc::downgrade(&meta),
        Arc::clone(&cfg),
        db_pool.clone(),
        rx,
      ));

      let session_impl = SessionImpl {
        machine_id,
        pool: Arc::clone(&pool),
        db_pool: db_pool.clone(),
      };
      let session_cap: agent_session::Client =
        capnp_rpc::new_client(session_impl);
      results.get().set_session(session_cap);
      Ok(())
    })
  }

  fn version(
    self: capnp::capability::Rc<Self>,
    _params: runner::VersionParams,
    mut results: runner::VersionResults,
  ) -> Promise<(), capnp::Error> {
    let mut r = results.get();
    r.set_proto(PROTO_VERSION);
    r.set_server(circus_common::version::long());
    Promise::ok(())
  }

  fn request_presigned_urls(
    self: capnp::capability::Rc<Self>,
    params: runner::RequestPresignedUrlsParams,
    mut results: runner::RequestPresignedUrlsResults,
  ) -> Promise<(), capnp::Error> {
    Promise::from_future(async move {
      let pr = params.get()?;
      let machine_id =
        parse_uuid_param(pr.get_machine_id()?.to_str()?, "machine_id")?;
      let build_id =
        parse_uuid_param(pr.get_build_id()?.to_str()?, "build_id")?;
      let req_list = pr.get_request()?;
      if req_list.len() > limits::MAX_PRESIGNED_URL_REQUESTS {
        return Err(capnp::Error::failed(format!(
          "too many presigned URL requests: {} > {}",
          req_list.len(),
          limits::MAX_PRESIGNED_URL_REQUESTS
        )));
      }
      let presigner = self.cfg.presigner.clone();
      let expiry = self.cfg.presign_expiry;
      let compression = self.cfg.upload_compression.clone();
      validate_upload_compression(&compression)?;

      let registered = *self.registered_machine.lock();
      if registered.map(|r| r.machine_id) != Some(machine_id) {
        return Err(capnp::Error::failed(
          "machine_id does not match registered session".into(),
        ));
      }
      let Some(meta) = self.pool.get(&machine_id) else {
        return Err(capnp::Error::failed(
          "registered agent is not in the live pool".into(),
        ));
      };
      if !meta.active_builds.read().contains(&build_id) {
        return Err(capnp::Error::failed(
          "build_id is not active for this agent".into(),
        ));
      }

      // Only the build drv outputs may be uploaded, else an agent could sign
      // a narinfo for an arbitrary input-addressed path.
      let drv_outputs: std::collections::HashSet<String> =
        match circus_common::repo::builds::get(&self.db_pool, build_id).await {
          Ok(build) => {
            match crate::dispatch::try_read_drv_outputs(&build.drv_path).await {
              Ok(outputs) if !outputs.is_empty() => {
                outputs.into_iter().collect()
              },
              Ok(_) => {
                return Err(capnp::Error::failed(format!(
                  "derivation {} has no queryable outputs for upload \
                   authorization",
                  build.drv_path
                )));
              },
              Err(e) => {
                return Err(capnp::Error::failed(format!(
                  "cannot query derivation outputs for upload authorization: \
                   {e}"
                )));
              },
            }
          },
          Err(e) => {
            return Err(capnp::Error::failed(format!(
              "cannot resolve build {build_id} for upload authorization: {e}"
            )));
          },
        };

      let mut out = results.get().init_responses(req_list.len());
      for (i, req) in req_list.iter().enumerate() {
        let store_path = req.get_store_path()?.to_str()?.to_owned();
        let nar_hash = req.get_nar_hash()?.to_str()?.to_owned();
        let nar_size = req.get_nar_size();
        let mut slot = out.reborrow().get(i as u32);
        slot.set_store_path(store_path.as_str());
        slot.set_compression(compression.as_str());
        if let Err(e) = validate_store_path(&store_path)
          .and_then(|()| validate_hash_text("nar_hash", &nar_hash))
        {
          let msg = e.to_string();
          slot.set_error_message(msg.as_str());
          continue;
        }
        if !drv_outputs.contains(&store_path) {
          slot.set_error_message(
            "store path is not an output of this build's derivation",
          );
          continue;
        }
        let Some(p) = presigner.as_ref() else {
          slot.set_error_message("runner has no S3 presigner configured");
          continue;
        };
        // An agent-chosen key could overwrite another path's NAR.
        let ext = compression_ext(&compression);
        let key = format!("nar/{}.{ext}", Uuid::new_v4().simple());
        let url = p.presign_put(&key, expiry);
        slot.set_nar_url(url.as_str());
        slot.set_nar_path(key.as_str());
        self.cfg.active_uploads.lock().insert(
          UploadKey {
            machine_id,
            build_id,
            store_path: store_path.clone(),
          },
          ExpectedUpload {
            nar_hash,
            nar_size,
            compression: compression.clone(),
            nar_path: key,
          },
        );
      }
      Ok(())
    })
  }

  fn notify_upload_complete(
    self: capnp::capability::Rc<Self>,
    params: runner::NotifyUploadCompleteParams,
    _results: runner::NotifyUploadCompleteResults,
  ) -> Promise<(), capnp::Error> {
    let db_pool = self.db_pool.clone();
    let self_cfg = Arc::clone(&self.cfg);
    Promise::from_future(async move {
      let pr = params.get()?;
      let machine_id =
        parse_uuid_param(pr.get_machine_id()?.to_str()?, "machine_id")?;
      let build_id =
        parse_uuid_param(pr.get_build_id()?.to_str()?, "build_id")?;
      let info = pr.get_nar_info()?;
      let store_path = info.get_store_path()?.to_str()?.to_owned();
      let nar_hash = info.get_nar_hash()?.to_str()?.to_owned();
      let nar_size = info.get_nar_size();
      let file_hash = info.get_file_hash()?.to_str()?.to_owned();
      let file_size = info.get_file_size();
      let compression = info.get_compression()?.to_str()?.to_owned();
      let url = info.get_url()?.to_str()?.to_owned();
      let deriver = {
        let s = info.get_deriver()?.to_str()?;
        (!s.is_empty()).then(|| s.to_owned())
      };
      let references: Vec<String> = info
        .get_references()?
        .iter()
        .map(|t| -> Result<String, capnp::Error> {
          Ok(t?.to_str()?.to_owned())
        })
        .collect::<Result<_, _>>()?;
      let ca = {
        let s = info.get_ca()?.to_str()?;
        (!s.is_empty()).then(|| s.to_owned())
      };
      validate_store_path(&store_path)?;
      validate_hash_text("nar_hash", &nar_hash)?;
      if !file_hash.is_empty() {
        validate_hash_text("file_hash", &file_hash)?;
      }
      validate_upload_compression(&compression)?;
      for reference in &references {
        validate_store_path(reference)?;
      }
      if let Some(deriver) = &deriver {
        validate_store_path(deriver)?;
        if std::path::Path::new(deriver)
          .extension()
          .is_none_or(|extension| extension != "drv")
        {
          return Err(capnp::Error::failed(format!(
            "deriver is not a derivation: {deriver}"
          )));
        }
      }
      if let Some(ca) = &ca {
        validate_text_len("ca", ca, 1, limits::MAX_HASH_LEN)?;
        if !ca.bytes().all(|byte| byte.is_ascii_graphic()) {
          return Err(capnp::Error::failed("ca contains invalid bytes".into()));
        }
      }

      let registered = *self.registered_machine.lock();
      if registered.map(|r| r.machine_id) != Some(machine_id) {
        return Err(capnp::Error::failed(
          "machine_id does not match registered session".into(),
        ));
      }
      let key = UploadKey {
        machine_id,
        build_id,
        store_path: store_path.clone(),
      };
      let Some(expected) = self_cfg.active_uploads.lock().get(&key).cloned()
      else {
        return Err(capnp::Error::failed(
          "upload was not presigned for this session/build/path".into(),
        ));
      };
      if expected.nar_hash != nar_hash
        || expected.nar_size != nar_size
        || expected.compression != compression
        || expected.nar_path != url
      {
        return Err(capnp::Error::failed(
          "narinfo does not match presigned upload".into(),
        ));
      }
      let Some(presigner) = self_cfg.presigner.as_ref() else {
        return Err(capnp::Error::failed(
          "runner has no S3 presigner configured".into(),
        ));
      };
      let get_url =
        presigner.presign_get(&expected.nar_path, self_cfg.presign_expiry);
      let verified = tokio::spawn(super::upload_verify::verify(
        super::upload_verify::VerifyRequest {
          get_url,
          compression: compression.clone(),
          nar_hash: nar_hash.clone(),
          nar_size,
          file_hash: (!file_hash.is_empty()).then(|| file_hash.clone()),
          file_size: (file_size > 0).then_some(file_size),
          references: references.clone(),
        },
      ))
      .await
      .map_err(|e| capnp::Error::failed(format!("upload verification panicked: {e}")))?
      .map_err(|e| {
        tracing::warn!(%machine_id, %build_id, %store_path, "uploaded NAR verification failed: {e}");
        capnp::Error::failed(format!("uploaded NAR verification failed: {e}"))
      })?;

      tracing::info!(
        %machine_id,
        %build_id,
        %store_path,
        nar_size = verified.nar_size as i64,
        file_size = verified.file_size as i64,
        %compression,
        "verified uploaded NAR"
      );

      let file_hash_opt = Some(verified.file_hash.as_str());
      let file_size_opt =
        (compression != "none").then_some(verified.file_size as i64);
      let project_id = repo::builds::project_id_for_build(&db_pool, build_id)
        .await
        .map_err(|e| {
          capnp::Error::failed(format!("failed to resolve build project: {e}"))
        })?;

      // Sign over the canonical Nix fingerprint (store path, nar hash, nar
      // size, refs) with the nar hash in sha256 base32. Never persist an
      // agent signature, only a runner-minted one is servable.
      let signed_sig = if let Some(key_file) = &self_cfg.signing_key_file {
        match sign_fingerprint(
          key_file,
          &store_path,
          &verified.nar_hash,
          verified.nar_size as i64,
          &references,
        )
        .await
        {
          Ok(sig) => Some(sig),
          Err(e) => {
            tracing::warn!(%store_path, "narinfo signing failed: {e}");
            None
          },
        }
      } else {
        None
      };

      if let Err(e) = circus_common::repo::narinfo_cache::upsert(
        &db_pool,
        circus_common::repo::narinfo_cache::UpsertNarInfo {
          store_path: &store_path,
          nar_hash: &verified.nar_hash,
          nar_size: verified.nar_size as i64,
          file_hash: file_hash_opt,
          file_size: file_size_opt,
          compression: &compression,
          url: &url,
          deriver: deriver.as_deref(),
          references: &references,
          sig: signed_sig.as_deref(),
          ca: ca.as_deref(),
          build_id: Some(build_id),
          project_id,
        },
      )
      .await
      {
        return Err(capnp::Error::failed(format!(
          "failed to persist narinfo: {e}"
        )));
      }
      self_cfg.active_uploads.lock().remove(&key);
      Ok(())
    })
  }

  fn fetch_drv_closure(
    self: capnp::capability::Rc<Self>,
    params: runner::FetchDrvClosureParams,
    _results: runner::FetchDrvClosureResults,
  ) -> Promise<(), capnp::Error> {
    let db_pool = self.db_pool.clone();
    Promise::from_future(async move {
      let pr = params.get()?;
      let machine_id =
        parse_uuid_param(pr.get_machine_id()?.to_str()?, "machine_id")?;
      let build_id =
        parse_uuid_param(pr.get_build_id()?.to_str()?, "build_id")?;
      let sink = pr.get_sink()?;

      let registered = *self.registered_machine.lock();
      if registered.map(|r| r.machine_id) != Some(machine_id) {
        return Err(capnp::Error::failed(
          "machine_id does not match registered session".into(),
        ));
      }
      let Some(meta) = self.pool.get(&machine_id) else {
        return Err(capnp::Error::failed(
          "registered agent is not in the live pool".into(),
        ));
      };
      // Scoped to the assigned build, else any authenticated agent could
      // read arbitrary paths out of the runner's store.
      if !meta.active_builds.read().contains(&build_id) {
        return Err(capnp::Error::failed(
          "build_id is not active for this agent".into(),
        ));
      }
      let build = circus_common::repo::builds::get(&db_pool, build_id)
        .await
        .map_err(|e| {
          capnp::Error::failed(format!("cannot load build {build_id}: {e}"))
        })?;

      export_drv_closure(&build.drv_path, sink).await.map_err(|e| {
        tracing::warn!(%machine_id, %build_id, "drv closure export failed: {e}");
        capnp::Error::failed(format!("drv closure export failed: {e}"))
      })
    })
  }
}

/// Stream `nix-store --export` of a derivation's closure into an agent's sink.
#[expect(clippy::future_not_send, reason = "capnp future")]
async fn export_drv_closure(
  drv_path: &str,
  sink: drv_sink::Client,
) -> color_eyre::Result<()> {
  use tokio::io::AsyncReadExt as _;

  let closure = crate::dispatch::drv_requisites(drv_path).await?;
  if closure.is_empty() {
    return Err(color_eyre::eyre::eyre!(
      "derivation {drv_path} has an empty closure"
    ));
  }

  let mut child = tokio::process::Command::new("nix-store")
    .arg("--export")
    .args(&closure)
    .stdin(std::process::Stdio::null())
    .stdout(std::process::Stdio::piped())
    .kill_on_drop(true)
    .spawn()?;
  let mut stdout = child
    .stdout
    .take()
    .ok_or_else(|| color_eyre::eyre::eyre!("export stdout missing"))?;

  let mut buf = vec![0u8; 1024 * 1024];
  let mut stream_err = None;
  loop {
    let n = match stdout.read(&mut buf).await {
      Ok(0) => break,
      Ok(n) => n,
      Err(e) => {
        stream_err =
          Some(color_eyre::eyre::eyre!("read nix-store --export: {e}"));
        break;
      },
    };
    let mut req = sink.write_request();
    req.get().set_chunk(&buf[..n]);
    if let Err(e) = req.send().promise.await {
      stream_err = Some(color_eyre::eyre::eyre!("stream drv closure: {e}"));
      break;
    }
  }

  // Always close so the agent reaps its import child, even on a short read.
  let close_res = sink.close_request().send().promise.await;
  if let Some(e) = stream_err {
    return Err(e);
  }
  close_res?;

  let status = child.wait().await?;
  if !status.success() {
    return Err(color_eyre::eyre::eyre!(
      "nix-store --export exited with {status}"
    ));
  }
  Ok(())
}

/// Sign a narinfo fingerprint with the on-disk Nix signing key, returning
/// `<key-name>:<base64 signature>`.
async fn sign_fingerprint(
  key_file: &std::path::Path,
  store_path: &str,
  nar_hash: &str,
  nar_size: i64,
  references: &[String],
) -> color_eyre::Result<String> {
  let key = circus_common::narinfo_signing::read_signing_key(key_file).await?;
  Ok(circus_common::narinfo_signing::sign_narinfo(
    &key, store_path, nar_hash, nar_size, references,
  ))
}

/// Map a compression algorithm name to the conventional NAR file extension.
///
/// Nix uses `nar/<hash>.nar.<ext>` in its binary cache layout. The
/// extension is purely cosmetic for Nix clients (they use the narinfo
/// `Compression:` field), but operators and S3 tooling rely on it being
/// accurate.
fn compression_ext(compression: &str) -> &'static str {
  match compression {
    "zstd" => "nar.zst",
    "xz" => "nar.xz",
    "gzip" | "gz" => "nar.gz",
    "bzip2" | "bz2" => "nar.bz2",
    _ => "nar",
  }
}

/// The meta is [`Weak`] so that removing the agent drops the sender for `rx`.
#[expect(clippy::future_not_send, reason = "capnp future")]
async fn run_dispatch_pump(
  builder_cap: builder::Client,
  meta: Weak<AgentMeta>,
  cfg: Arc<ServerConfig>,
  db_pool: PgPool,
  mut rx: mpsc::UnboundedReceiver<DispatchCommand>,
) {
  while let Some(mut cmd) = rx.recv().await {
    let Some(meta) = meta.upgrade() else {
      break;
    };
    let machine_id = meta.machine_id;
    let pool = db_pool.clone();
    let cfg = Arc::clone(&cfg);
    let builder_cap = builder_cap.clone();
    let meta_for_task = Arc::clone(&meta);

    tokio::task::spawn_local(async move {
      let outcome = dispatch_one(
        &builder_cap,
        &mut cmd,
        &pool,
        &cfg,
        machine_id,
        &meta_for_task,
      )
      .await;
      let build_id = cmd.build_id;
      let _ = cmd.completion.send(outcome);
      drop(cmd.reservation);
      tracing::debug!(%machine_id, %build_id, "dispatch finished");
    });
  }
}

#[expect(clippy::future_not_send, reason = "capnp future")]
async fn dispatch_one(
  builder_cap: &builder::Client,
  cmd: &mut DispatchCommand,
  pool: &PgPool,
  cfg: &ServerConfig,
  machine_id: Uuid,
  meta: &AgentMeta,
) -> DispatchResult {
  meta.active_builds.write().insert(cmd.build_id);
  let task_token = if cmd.effect.is_some() {
    match repo::effect_task_tokens::issue(pool, cmd.build_id, cmd.attempt).await
    {
      Ok(token) => token,
      Err(e) => {
        let out = DispatchResult::Failed(format!(
          "could not issue the effect task token: {e}"
        ));
        if reconcile_effect_result(pool, cmd, machine_id, &out, false).await {
          meta.active_builds.write().remove(&cmd.build_id);
        }
        cfg.forget_uploads_for(machine_id, cmd.build_id);
        return out;
      },
    }
  } else {
    String::new()
  };
  let git_token = match effect_git_token(pool, cfg, cmd).await {
    Ok(token) => token,
    Err(e) => {
      let out = DispatchResult::Failed(format!(
        "could not mint the effect's GitToken: {e}"
      ));
      if reconcile_effect_result(pool, cmd, machine_id, &out, false).await {
        meta.active_builds.write().remove(&cmd.build_id);
      }
      cfg.forget_uploads_for(machine_id, cmd.build_id);
      return out;
    },
  };
  let (done_tx, mut done_rx) = oneshot::channel::<BuildOutcomeKind>();
  let log_sink_impl = LogSinkImpl::new(cmd.log_path.clone(), cmd.max_log_size);
  let log_cap: log_sink::Client = capnp_rpc::new_client(log_sink_impl);
  let result_sink_impl = ResultSinkImpl {
    pool: pool.clone(),
    machine_id,
    done: Arc::new(tokio::sync::Mutex::new(Some(done_tx))),
  };
  let result_cap: result_sink::Client = capnp_rpc::new_client(result_sink_impl);

  // Give the agent a sink to stream its output closure into so the runner can
  // serve this build locally.
  let output_cap = (cmd.effect.is_none() && cmd.presigned_upload.is_none())
    .then(|| {
      capnp_rpc::new_client(OutputSinkImpl::new(cmd.build_id.to_string()))
    });

  let mut req = builder_cap.assign_request();
  {
    let mut p = req.get();
    {
      let mut job = p.reborrow().init_job();
      let build_id_str = cmd.build_id.to_string();
      job.set_build_id(build_id_str.as_str());
      job.set_drv_path(cmd.drv_path.as_str());
      // Where the agent substitutes the drv closure from.
      if let Some(url) = cfg.cache_substituter.as_deref() {
        job.set_cache_substituter(url);
      }
      if let Some(key) = cfg.cache_public_key.as_deref() {
        job.set_cache_public_key(key);
      }
      job.set_max_log_size(cmd.max_log_size);
      job.set_max_silent_time(cmd.max_silent_time);
      job.set_build_timeout(cmd.build_timeout);
      let mut args = job
        .reborrow()
        .init_extra_nix_args(cmd.extra_args.len() as u32);
      for (i, a) in cmd.extra_args.iter().enumerate() {
        args.set(i as u32, a.as_str());
      }
      if let Some(upload) = cmd.presigned_upload.as_ref() {
        let mut opts = job.reborrow().init_presigned_upload();
        opts.set_upload_debug_info(false);
        opts.set_compression(upload.compression.as_str());
        opts.set_compression_level(0);
        opts.set_fail_build_on_upload_error(upload.fail_build_on_upload_error);
      }
      if let Some(effect) = cmd.effect.as_ref() {
        set_effect_options(
          &mut job,
          effect,
          &cfg.api_base_url,
          &task_token,
          &git_token,
        );
      }
    }
    p.set_log(log_cap);
    p.set_result(result_cap);
    if let Some(output_cap) = output_cap {
      p.set_output(output_cap);
    }
  }

  if let Err(e) = req.send().promise.await {
    tracing::warn!(build_id = %cmd.build_id, "assign call failed: {e}");
    if quarantine_ambiguous_effect(pool, cmd, machine_id).await {
      meta.active_builds.write().remove(&cmd.build_id);
    }
    cfg.forget_uploads_for(machine_id, cmd.build_id);
    return if e.kind == capnp::ErrorKind::Disconnected {
      DispatchResult::Disconnected
    } else {
      DispatchResult::Refused(e.to_string())
    };
  }

  let (outcome, abort_requested) = tokio::select! {
    outcome = &mut done_rx => (outcome, false),
    _ = &mut cmd.abort => {
      tracing::info!(
        build_id = %cmd.build_id,
        "requesting agent abort after scheduler cancellation"
      );
      if let Err(e) = request_agent_abort(builder_cap, cmd.build_id).await {
        tracing::warn!(
          build_id = %cmd.build_id,
          "agent abort call failed: {e}"
        );
        if quarantine_ambiguous_effect(pool, cmd, machine_id).await {
          meta.active_builds.write().remove(&cmd.build_id);
        }
        cfg.forget_uploads_for(machine_id, cmd.build_id);
        return DispatchResult::Disconnected;
      }
      (done_rx.await, true)
    },
  };

  let out = match outcome {
    Ok(BuildOutcomeKind::Success { error_message }) => {
      DispatchResult::Succeeded { error_message }
    },
    Ok(BuildOutcomeKind::TimedOut) => DispatchResult::TimedOut,
    Ok(BuildOutcomeKind::Aborted) => DispatchResult::Aborted,
    Ok(BuildOutcomeKind::OomKilled { error_message }) => {
      DispatchResult::OomKilled(error_message.unwrap_or_default())
    },
    Ok(BuildOutcomeKind::Failure { error_message }) => {
      DispatchResult::Failed(error_message.unwrap_or_default())
    },
    Err(_) => DispatchResult::Disconnected,
  };
  let may_release_active =
    reconcile_effect_result(pool, cmd, machine_id, &out, abort_requested).await;
  if may_release_active {
    meta.active_builds.write().remove(&cmd.build_id);
  }
  cfg.forget_uploads_for(machine_id, cmd.build_id);
  out
}

async fn reconcile_effect_result(
  pool: &PgPool,
  cmd: &DispatchCommand,
  machine_id: Uuid,
  outcome: &DispatchResult,
  abort_requested: bool,
) -> bool {
  if cmd.effect.is_none() {
    return true;
  }

  if matches!(outcome, DispatchResult::Disconnected) {
    return quarantine_ambiguous_effect(pool, cmd, machine_id).await;
  }

  if matches!(outcome, DispatchResult::Aborted) {
    return match repo::builds::acknowledge_effect_stopped(
      pool,
      cmd.build_id,
      machine_id,
      cmd.attempt,
    )
    .await
    {
      Ok(true) => true,
      Ok(false) => {
        match repo::builds::record_assigned_effect_outcome(
          pool,
          cmd.build_id,
          machine_id,
          cmd.attempt,
          BuildStatus::Aborted,
          Some("effect aborted"),
        )
        .await
        {
          Ok(true) => true,
          Ok(false) => {
            effect_state_allows_connection_release(
              pool,
              cmd.build_id,
              machine_id,
              cmd.attempt,
            )
            .await
          },
          Err(e) => {
            tracing::error!(
              build_id = %cmd.build_id,
              "failed to persist unexpected Effect abort outcome: {e}"
            );
            quarantine_ambiguous_effect(pool, cmd, machine_id).await
          },
        }
      },
      Err(e) => {
        tracing::error!(
          build_id = %cmd.build_id,
          "failed to persist cancelled effect acknowledgement: {e}; \
           quarantining"
        );
        quarantine_ambiguous_effect(pool, cmd, machine_id).await
      },
    };
  }

  let Some((status, error_message)) = known_effect_outcome(outcome) else {
    return true;
  };
  if abort_requested {
    tracing::warn!(
      build_id = %cmd.build_id,
      actual_status = status.as_db_str(),
      "Effect returned a terminal result after its abort was requested"
    );
  }
  match repo::builds::record_assigned_effect_outcome(
    pool,
    cmd.build_id,
    machine_id,
    cmd.attempt,
    status,
    error_message,
  )
  .await
  {
    Ok(true) if abort_requested => {
      match repo::builds::release_terminal_effect_attempt(
        pool,
        cmd.build_id,
        cmd.attempt,
      )
      .await
      {
        Ok(true) => true,
        Ok(false) => {
          tracing::error!(
            build_id = %cmd.build_id,
            attempt = cmd.attempt,
            "Effect result was persisted after cancellation, but its worker \
             barrier was not released"
          );
          false
        },
        Err(e) => {
          tracing::error!(
            build_id = %cmd.build_id,
            attempt = cmd.attempt,
            "failed to release cancelled worker's terminal Effect barrier: \
             {e}"
          );
          false
        },
      }
    },
    Ok(true) => true,
    Ok(false) => {
      effect_state_allows_connection_release(
        pool,
        cmd.build_id,
        machine_id,
        cmd.attempt,
      )
      .await
    },
    Err(e) => {
      tracing::error!(
        build_id = %cmd.build_id,
        "failed to persist Effect result after cancellation: {e}; retaining \
         agent-active barrier"
      );
      quarantine_ambiguous_effect(pool, cmd, machine_id).await
    },
  }
}

async fn effect_state_allows_connection_release(
  pool: &PgPool,
  build_id: Uuid,
  machine_id: Uuid,
  attempt: i32,
) -> bool {
  match repo::builds::get(pool, build_id).await {
    Ok(build)
      if build.agent_machine_id != Some(machine_id)
        || build.retry_count != attempt =>
    {
      true
    },
    Ok(build)
      if build.status.is_terminal()
        && !matches!(
          build.status,
          BuildStatus::Cancelled | BuildStatus::Running | BuildStatus::Pending
        ) =>
    {
      true
    },
    Ok(build) => {
      tracing::error!(
        %build_id,
        status = build.status.as_db_str(),
        agent_machine_id = ?build.agent_machine_id,
        "Effect connection no longer owns an updateable assignment; retaining \
         agent-active barrier"
      );
      false
    },
    Err(e) => {
      tracing::error!(
        %build_id,
        "failed to inspect Effect assignment after no-op update: {e}; \
         retaining agent-active barrier"
      );
      false
    },
  }
}

fn known_effect_outcome(
  outcome: &DispatchResult,
) -> Option<(BuildStatus, Option<&str>)> {
  match outcome {
    DispatchResult::Succeeded { error_message } => {
      Some((BuildStatus::Succeeded, error_message.as_deref()))
    },
    DispatchResult::Failed(error_message) => {
      Some((BuildStatus::Failed, Some(error_message)))
    },
    DispatchResult::TimedOut => {
      Some((BuildStatus::Timeout, Some("effect timed out")))
    },
    DispatchResult::OomKilled(error_message) => {
      Some((BuildStatus::OomKilled, Some(error_message)))
    },
    DispatchResult::Aborted
    | DispatchResult::Disconnected
    | DispatchResult::Refused(_) => None,
  }
}

async fn quarantine_ambiguous_effect(
  pool: &PgPool,
  cmd: &DispatchCommand,
  machine_id: Uuid,
) -> bool {
  if cmd.effect.is_none() {
    return true;
  }
  match repo::builds::quarantine_effect(
    pool,
    cmd.build_id,
    machine_id,
    cmd.attempt,
    circus_common::EFFECT_OUTCOME_UNKNOWN_ERROR,
  )
  .await
  {
    Ok(true) => true,
    Ok(false) => {
      if effect_state_allows_connection_release(
        pool,
        cmd.build_id,
        machine_id,
        cmd.attempt,
      )
      .await
      {
        tracing::info!(
          build_id = %cmd.build_id,
          "Ignoring late Effect disconnect after ownership was released"
        );
        true
      } else {
        tracing::error!(
          build_id = %cmd.build_id,
          "could not quarantine ambiguous effect state; retaining \
           agent-active barrier"
        );
        false
      }
    },
    Err(e) => {
      tracing::error!(
        build_id = %cmd.build_id,
        "failed to quarantine ambiguous effect: {e}; retaining agent-active barrier"
      );
      false
    },
  }
}

#[expect(clippy::future_not_send, reason = "capnp future")]
async fn request_agent_abort(
  builder_cap: &builder::Client,
  build_id: Uuid,
) -> Result<(), capnp::Error> {
  let mut abort = builder_cap.abort_request();
  let build_id = build_id.to_string();
  abort.get().set_build_id(build_id.as_str());
  abort.send().promise.await?;
  Ok(())
}

/// An empty token when the effect did not ask for one.
async fn effect_git_token(
  pool: &PgPool,
  cfg: &ServerConfig,
  cmd: &DispatchCommand,
) -> color_eyre::Result<String> {
  let Some(effect) = cmd.effect.as_ref() else {
    return Ok(String::new());
  };
  if !repo::effect_git_token_requests::requested(pool, cmd.build_id).await? {
    return Ok(String::new());
  }
  if !effect.project_path.starts_with("github/") {
    color_eyre::eyre::bail!("GitToken is only minted for GitHub repositories");
  }
  let app = cfg.github_app.as_ref().ok_or_else(|| {
    color_eyre::eyre::eyre!("queue_runner.github_app is not configured")
  })?;
  app.repository_token(&effect.owner, &effect.repo).await
}

fn set_effect_options(
  job: &mut build_assignment::Builder<'_>,
  effect: &EffectContext,
  api_base_url: &str,
  task_token: &str,
  git_token: &str,
) {
  let mut opts = job.reborrow().init_effect();
  opts.set_project_id(effect.project_id.as_str());
  opts.set_project_path(effect.project_path.as_str());
  opts.set_api_base_url(api_base_url);
  opts.set_owner(effect.owner.as_str());
  opts.set_repo(effect.repo.as_str());
  opts.set_branch(effect.branch.as_str());
  opts.set_tag(effect.tag.as_str());
  opts.set_is_default_branch(effect.is_default_branch);
  opts.set_task_token(task_token);
  opts.set_git_token(git_token);
}

fn parse_uuid_param(value: &str, name: &str) -> Result<Uuid, capnp::Error> {
  Uuid::parse_str(value)
    .map_err(|e| capnp::Error::failed(format!("bad {name}: {e}")))
}

pub(crate) fn read_bounded_text_list(
  list: capnp::text_list::Reader<'_>,
  field: &str,
  max_items: u32,
  max_item_len: usize,
) -> Result<Vec<String>, capnp::Error> {
  if list.len() > max_items {
    return Err(capnp::Error::failed(format!(
      "{field} has too many entries: {} > {max_items}",
      list.len()
    )));
  }
  list
    .iter()
    .enumerate()
    .map(|(idx, t)| -> Result<String, capnp::Error> {
      let value = t?.to_str()?.to_owned();
      let item_field = format!("{field}[{idx}]");
      validate_text_len(&item_field, &value, 1, max_item_len)?;
      Ok(value)
    })
    .collect()
}

fn validate_agent_capacity(
  systems: &[String],
  speed: f32,
  cpu: u32,
  max_jobs: u32,
) -> Result<(), capnp::Error> {
  if systems.is_empty() {
    return Err(capnp::Error::failed(
      "agent must advertise at least one system".into(),
    ));
  }
  if !speed.is_finite() || speed <= 0.0 {
    return Err(capnp::Error::failed(format!(
      "agent speed_factor must be finite and positive, got {speed}"
    )));
  }
  if cpu == 0 || i32::try_from(cpu).is_err() {
    return Err(capnp::Error::failed(format!(
      "agent cpu_count must be between 1 and {}, got {cpu}",
      i32::MAX
    )));
  }
  if max_jobs == 0 {
    return Err(capnp::Error::failed(
      "agent max_jobs must be greater than 0".into(),
    ));
  }
  if max_jobs > MAX_AGENT_MAX_JOBS {
    return Err(capnp::Error::failed(format!(
      "agent max_jobs {max_jobs} exceeds cap {MAX_AGENT_MAX_JOBS}",
    )));
  }
  Ok(())
}

fn configured_nonempty(value: Option<&str>) -> bool {
  value.is_some_and(|value| !value.trim().is_empty())
}

#[must_use]
fn effect_capability_enabled(
  requested: bool,
  ephemeral: bool,
  cache_substituter: Option<&str>,
  cache_public_key: Option<&str>,
) -> bool {
  requested
    && !ephemeral
    && configured_nonempty(cache_substituter)
    && configured_nonempty(cache_public_key)
}

fn validate_text_len(
  field: &str,
  value: &str,
  min: usize,
  max: usize,
) -> Result<(), capnp::Error> {
  let len = value.len();
  if len < min || len > max || value.chars().any(char::is_control) {
    return Err(capnp::Error::failed(format!(
      "{field} length/control validation failed: len={len}, expected \
       {min}..={max}"
    )));
  }
  Ok(())
}

fn validate_store_path(path: &str) -> Result<(), capnp::Error> {
  validate_text_len("store_path", path, 1, limits::MAX_STORE_PATH_LEN)?;
  if path == "/nix/store/"
    || !circus_nix::StorePath::is_valid(path, "/nix/store")
  {
    return Err(capnp::Error::failed(format!("invalid store path: {path}")));
  }
  Ok(())
}

fn validate_hash_text(field: &str, hash: &str) -> Result<(), capnp::Error> {
  validate_text_len(field, hash, 1, limits::MAX_HASH_LEN)
}

fn validate_upload_compression(compression: &str) -> Result<(), capnp::Error> {
  match compression {
    "zstd" | "xz" | "gzip" | "none" => Ok(()),
    other => {
      Err(capnp::Error::failed(format!(
        "unsupported upload compression: {other}"
      )))
    },
  }
}

fn validate_token_hashes(hashes: &[String]) -> color_eyre::Result<()> {
  for (idx, hash) in hashes.iter().enumerate() {
    let decoded = hex::decode(hash.trim()).with_context(|| {
      format!("queue_runner.rpc.auth_tokens[{idx}] is not hex")
    })?;
    if decoded.len() != 32 {
      bail!(
        "queue_runner.rpc.auth_tokens[{idx}] must decode to 32 bytes, got {}",
        decoded.len()
      );
    }
  }
  Ok(())
}

fn verify_token(allowed: &[String], token: &str) -> bool {
  if allowed.is_empty() {
    return false;
  }
  let mut hasher = Sha256::new();
  hasher.update(token.as_bytes());
  let digest = hasher.finalize();
  let mut matched = 0_u8;
  for allowed_hash in allowed {
    if let Ok(decoded) = hex::decode(allowed_hash.trim())
      && decoded.len() == digest.len()
    {
      matched |= decoded.as_slice().ct_eq(&digest[..]).unwrap_u8();
    }
  }
  matched == 1
}

#[cfg(test)]
mod tests {
  use std::{cell::RefCell, rc::Rc};

  use super::*;

  struct RecordingBuilder {
    aborted: Rc<RefCell<Vec<String>>>,
  }

  #[allow(refining_impl_trait_internal)]
  impl builder::Server for RecordingBuilder {
    fn assign(
      self: capnp::capability::Rc<Self>,
      _params: builder::AssignParams,
      _results: builder::AssignResults,
    ) -> Promise<(), capnp::Error> {
      Promise::ok(())
    }

    fn abort(
      self: capnp::capability::Rc<Self>,
      params: builder::AbortParams,
      _results: builder::AbortResults,
    ) -> Promise<(), capnp::Error> {
      let aborted = Rc::clone(&self.aborted);
      Promise::from_future(async move {
        let build_id = params.get()?.get_build_id()?.to_str()?.to_owned();
        aborted.borrow_mut().push(build_id);
        Ok(())
      })
    }

    fn shutdown(
      self: capnp::capability::Rc<Self>,
      _params: builder::ShutdownParams,
      _results: builder::ShutdownResults,
    ) -> Promise<(), capnp::Error> {
      Promise::ok(())
    }
  }

  #[test]
  fn verify_token_accepts_configured_sha256_digest() {
    let token = "correct horse battery staple";
    let digest = hex::encode(Sha256::digest(token.as_bytes()));
    assert!(verify_token(&[digest], token));
  }

  #[test]
  fn effects_capability_requires_persistent_agent_and_complete_cache_trust() {
    assert!(effect_capability_enabled(
      true,
      false,
      Some("https://cache.example"),
      Some("cache.example-1:key"),
    ));
    assert!(!effect_capability_enabled(
      false,
      false,
      Some("https://cache.example"),
      Some("cache.example-1:key"),
    ));
    assert!(!effect_capability_enabled(
      true,
      true,
      Some("https://cache.example"),
      Some("cache.example-1:key"),
    ));
    assert!(!effect_capability_enabled(
      true,
      false,
      Some("  "),
      Some("cache.example-1:key"),
    ));
    assert!(!effect_capability_enabled(
      true,
      false,
      Some("https://cache.example"),
      Some(""),
    ));
  }

  #[test]
  fn success_after_abort_request_is_a_known_terminal_outcome() {
    let result = DispatchResult::Succeeded {
      error_message: Some("completed before abort".into()),
    };
    let (status, message) =
      known_effect_outcome(&result).expect("success has a terminal outcome");
    assert_eq!(status, BuildStatus::Succeeded);
    assert_eq!(message, Some("completed before abort"));
    assert!(known_effect_outcome(&DispatchResult::Aborted).is_none());
  }

  #[tokio::test(flavor = "current_thread")]
  async fn cancellation_sends_builder_abort_for_the_assigned_build() {
    let aborted = Rc::new(RefCell::new(Vec::new()));
    let client: builder::Client = capnp_rpc::new_client(RecordingBuilder {
      aborted: Rc::clone(&aborted),
    });
    let build_id = Uuid::new_v4();

    request_agent_abort(&client, build_id)
      .await
      .expect("abort call succeeds");

    assert_eq!(aborted.borrow().as_slice(), &[build_id.to_string()]);
  }

  #[test]
  fn verify_token_rejects_invalid_or_different_digest() {
    let digest = hex::encode(Sha256::digest(b"other"));
    assert!(!verify_token(&["not-hex".into(), digest], "token"));
  }

  #[test]
  fn validate_store_path_rejects_prefix_only_and_traversal() {
    assert!(validate_store_path("/nix/store/abc123-package").is_ok());
    assert!(validate_store_path("/nix/store/").is_err());
    assert!(validate_store_path("/nix/store/abc..def").is_err());
    assert!(validate_store_path("/tmp/not-store").is_err());
  }

  #[test]
  fn effect_assignment_serializes_the_full_runtime_context() {
    let effect = crate::rpc::pool::EffectContext {
      project_id:        "42c80f70-58a9-44a1-b622-202d738f40af".into(),
      project_path:      "github/acme/infra".into(),
      owner:             "acme".into(),
      repo:              "infra".into(),
      branch:            "main".into(),
      tag:               String::new(),
      is_default_branch: true,
    };
    let mut message = capnp::message::Builder::new_default();
    {
      let mut job = message.init_root::<build_assignment::Builder<'_>>();
      set_effect_options(
        &mut job,
        &effect,
        "https://ci.example.org",
        "token",
        "",
      );
    }

    let job = message
      .get_root_as_reader::<build_assignment::Reader<'_>>()
      .expect("read assignment");
    assert!(job.has_effect());
    let opts = job.get_effect().expect("read effect options");
    assert_eq!(
      opts.get_project_id().expect("project id"),
      effect.project_id
    );
    assert_eq!(
      opts.get_project_path().expect("project path"),
      effect.project_path
    );
    assert_eq!(
      opts.get_api_base_url().expect("API base URL"),
      "https://ci.example.org"
    );
    assert_eq!(opts.get_owner().expect("owner"), effect.owner);
    assert_eq!(opts.get_repo().expect("repo"), effect.repo);
    assert_eq!(opts.get_branch().expect("branch"), effect.branch);
    assert_eq!(opts.get_tag().expect("tag"), effect.tag);
    assert!(opts.get_is_default_branch());
  }
}
