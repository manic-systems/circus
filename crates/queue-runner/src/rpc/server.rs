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
  collections::{HashMap, HashSet},
  net::SocketAddr,
  sync::{
    Arc,
    atomic::{AtomicU32, Ordering},
  },
  time::Instant,
};

use capnp::capability::Promise;
use capnp_rpc::{RpcSystem, rpc_twoparty_capnp, twoparty};
use circus_proto::{
  PROTO_VERSION,
  agent_session,
  builder,
  limits,
  log_sink,
  result_sink,
  runner,
};
use color_eyre::eyre::{Context as _, eyre};
use parking_lot::RwLock;
use sha2::{Digest as _, Sha256};
use sqlx::PgPool;
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
  pool::{AgentMeta, DispatchCommand, DispatchResult, HeartbeatSnapshot},
  result_sink::{BuildOutcomeKind, ResultSinkImpl},
  session::SessionImpl,
};

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
  /// Returns the underlying error if TLS files are missing or invalid.
  pub fn from_user(
    cfg: &circus_common::config::RpcConfig,
  ) -> color_eyre::Result<Self> {
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
      active_uploads: Arc::new(parking_lot::Mutex::new(HashMap::new())),
    })
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
    cache_cfg: &circus_common::config::CacheUploadConfig,
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
      if let Err(e) = serve_one(socket, peer, cfg, pool, db_pool).await {
        tracing::warn!(?peer, "rpc session ended: {e}");
      }
    });
  }
}

#[expect(clippy::future_not_send, reason = "capnp future")]
async fn serve_one(
  socket: tokio::net::TcpStream,
  peer: SocketAddr,
  cfg: Arc<ServerConfig>,
  pool: Arc<AgentPool>,
  db_pool: PgPool,
) -> color_eyre::Result<()> {
  let _ = socket.set_nodelay(true);
  tracing::info!(?peer, "incoming rpc connection");

  let registered_machine: Arc<parking_lot::Mutex<Option<RegisteredAgent>>> =
    Arc::new(parking_lot::Mutex::new(None));
  let registered_for_cleanup = Arc::clone(&registered_machine);

  let rpc_result = if let Some(tls) = cfg.tls.as_ref() {
    let stream = tls.acceptor.clone().accept(socket).await?;
    let pinned_cn = extract_peer_cn(&stream);
    let (rh, wh) = tokio::io::split(stream);
    let network = twoparty::VatNetwork::new(
      rh.compat(),
      wh.compat_write(),
      rpc_twoparty_capnp::Side::Server,
      capnp::message::ReaderOptions::default(),
    );
    let runner_impl = RunnerImpl {
      cfg: Arc::clone(&cfg),
      pool: Arc::clone(&pool),
      db_pool: db_pool.clone(),
      registered_machine: Arc::clone(&registered_machine),
      pinned_cn,
    };
    let runner_cap: runner::Client = capnp_rpc::new_client(runner_impl);
    let rpc = RpcSystem::new(Box::new(network), Some(runner_cap.client));
    rpc.await
  } else {
    let (read_half, write_half) = socket.into_split();
    let network = twoparty::VatNetwork::new(
      read_half.compat(),
      write_half.compat_write(),
      rpc_twoparty_capnp::Side::Server,
      capnp::message::ReaderOptions::default(),
    );
    let runner_impl = RunnerImpl {
      cfg:                Arc::clone(&cfg),
      pool:               Arc::clone(&pool),
      db_pool:            db_pool.clone(),
      registered_machine: Arc::clone(&registered_machine),
      pinned_cn:          None,
    };
    let runner_cap: runner::Client = capnp_rpc::new_client(runner_impl);
    let rpc = RpcSystem::new(Box::new(network), Some(runner_cap.client));
    rpc.await
  };

  let registered = *registered_for_cleanup.lock();
  if let Some(registered) = registered {
    let machine_id = registered.machine_id;
    if pool
      .remove_if_connection(&machine_id, registered.connection_id)
      .is_some()
    {
      if let Err(e) = mark_disconnected(&db_pool, machine_id).await {
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
  rpc_result?;
  Ok(())
}

/// Extract the Common Name from the peer's verified client certificate.
///
/// rustls hands us the DER-encoded certificate chain; we only need the CN
/// of the leaf. We do a minimal X.509 walk rather than pulling in a full
/// parser: the CN is in the Subject sequence under OID 2.5.4.3.
fn extract_peer_cn(
  stream: &tokio_rustls::server::TlsStream<tokio::net::TcpStream>,
) -> Option<String> {
  let (_, server_conn) = stream.get_ref();
  let peer = server_conn.peer_certificates()?.first()?;
  parse_cn(peer.as_ref())
}

fn parse_cn(der: &[u8]) -> Option<String> {
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
  /// CN extracted from the peer certificate when mTLS is enforced; `None`
  /// when TLS is off or `client_ca` was not set.
  pinned_cn:          Option<String>,
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
    let pinned_cn = self.pinned_cn.clone();
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
      if !verify_token(&cfg.token_hashes, token) {
        tracing::warn!(name = %name, "bad auth token from agent");
        return Err(capnp::Error::failed("auth failed".into()));
      }

      // CN pinning: only enforced when mTLS extracted a CN. With
      // `pin_cn = false` operators can use a per-tenant CA where the CN
      // is the tenant identifier rather than the agent name.
      if let Some(cn) = pinned_cn.as_ref()
        && cfg.tls.as_ref().is_some_and(|t| t.pin_cn)
        && cn != &name
      {
        tracing::warn!(name = %name, cn = %cn, "cert CN does not match agent name");
        return Err(capnp::Error::failed("CN/name mismatch".into()));
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

      if let Err(e) = upsert_session(
        &db_pool,
        machine_id,
        &name,
        &hostname,
        &systems,
        &supported,
        &mandatory,
        speed,
        cpu as i32,
        maxj as i32,
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
      let meta = Arc::new(AgentMeta {
        machine_id,
        connection_id,
        name: name.clone(),
        hostname,
        systems,
        supported_features: supported,
        mandatory_features: mandatory,
        speed_factor: speed,
        cpu_count: cpu,
        max_jobs: maxj,
        current_jobs: Arc::new(AtomicU32::new(0)),
        active_builds: RwLock::new(HashSet::new()),
        heartbeat: RwLock::new(HeartbeatSnapshot::default()),
        registered_at: Instant::now(),
        tx,
      });
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
        Arc::clone(&meta),
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
    r.set_server(env!("CARGO_PKG_VERSION"));
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
        let Some(p) = presigner.as_ref() else {
          slot.set_error_message("runner has no S3 presigner configured");
          continue;
        };
        // S3 key shape: nar/<sha256-base32 from nar_hash>.<ext>, where the
        // extension is derived from the configured compression so the key
        // suffix matches the actual encoding. Nix clients use the narinfo
        // `Compression:` field to decompress, but operators and S3-level
        // tooling rely on the extension being accurate.
        let ext = compression_ext(&compression);
        let key = format!("nar/{}.{}", hash_key_segment(&nar_hash), ext);
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
      let nar_size = info.get_nar_size() as i64;
      let file_hash = info.get_file_hash()?.to_str()?.to_owned();
      let file_size = info.get_file_size() as i64;
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
      let sig_in = {
        let s = info.get_sig()?.to_str()?;
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
        || expected.nar_size != info.get_nar_size()
        || expected.compression != compression
        || expected.nar_path != url
      {
        return Err(capnp::Error::failed(
          "narinfo does not match presigned upload".into(),
        ));
      }

      tracing::info!(
        %machine_id,
        %build_id,
        %store_path,
        nar_size,
        file_size,
        %compression,
        "agent reported upload complete"
      );

      let file_hash_opt = (!file_hash.is_empty()).then_some(file_hash.as_str());
      let file_size_opt =
        (compression != "none" && file_size > 0).then_some(file_size);

      // Sign the narinfo on the runner side. The fingerprint is the
      // canonical Nix narinfo signing input:
      // `<storePath>;<narHash>;<narSize>;<references>` (refs comma-
      // joined). When a signing key is configured we replace whatever
      // the agent sent (typically empty) with our own signature.
      let signed_sig = if let Some(key_file) = &self_cfg.signing_key_file {
        match sign_fingerprint(
          key_file,
          &store_path,
          &nar_hash,
          nar_size,
          &references,
        )
        .await
        {
          Ok(sig) => Some(sig),
          Err(e) => {
            tracing::warn!(%store_path, "narinfo signing failed: {e}");
            sig_in
          },
        }
      } else {
        sig_in
      };

      if let Err(e) = circus_common::repo::narinfo_cache::upsert(
        &db_pool,
        circus_common::repo::narinfo_cache::UpsertNarInfo {
          store_path: &store_path,
          nar_hash: &nar_hash,
          nar_size,
          file_hash: file_hash_opt,
          file_size: file_size_opt,
          compression: &compression,
          url: &url,
          deriver: deriver.as_deref(),
          references: &references,
          sig: signed_sig.as_deref(),
          ca: ca.as_deref(),
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
}

/// Sign a narinfo fingerprint with the Nix-format signing key on disk.
///
/// Nix key files are one line: `<key-name>:<base64 secret>`. The secret
/// is a 64-byte concatenation of the Ed25519 seed and public key (the
/// canonical libsodium "secret key" layout). Output is
/// `<key-name>:<base64 signature>`.
async fn sign_fingerprint(
  key_file: &std::path::Path,
  store_path: &str,
  nar_hash: &str,
  nar_size: i64,
  references: &[String],
) -> color_eyre::Result<String> {
  use base64::{Engine as _, engine::general_purpose::STANDARD as B64};
  use ring::signature::Ed25519KeyPair;
  let raw = tokio::fs::read_to_string(key_file)
    .await
    .with_context(|| format!("read signing key {}", key_file.display()))?;
  let raw = raw.trim();
  let (name, secret_b64) = raw
    .split_once(':')
    .ok_or_else(|| eyre!("signing key not in `name:base64` form"))?;
  let secret = B64
    .decode(secret_b64)
    .with_context(|| "signing key base64 decode")?;
  if secret.len() != 64 {
    return Err(eyre!("signing key has {} bytes, expected 64", secret.len()));
  }
  // libsodium layout is `seed (32) || public key (32)`. ring wants the
  // seed alone.
  let key = Ed25519KeyPair::from_seed_unchecked(&secret[..32])
    .map_err(|e| eyre!("ring rejected key seed: {e}"))?;
  let fingerprint = format!(
    "{store_path};{nar_hash};{nar_size};{}",
    references.join(",")
  );
  let sig = key.sign(fingerprint.as_bytes());
  Ok(format!("{name}:{}", B64.encode(sig.as_ref())))
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

/// Extract a URL/S3-key safe segment from a NAR hash.
///
/// Normal Nix hashes use base32 (`sha256:...`) or SRI base64
/// (`sha256-...`). A compromised agent can send arbitrary text here, so
/// never copy the string into an object key without sanitising it first.
fn hash_key_segment(h: &str) -> String {
  for prefix in ["sha256:", "sha256-"] {
    if let Some(rest) = h.strip_prefix(prefix) {
      let cleaned = sanitise_key_segment(rest.trim_end_matches('='));
      if !cleaned.is_empty() {
        return cleaned;
      }
    }
  }
  let cleaned = sanitise_key_segment(h);
  if cleaned.is_empty() {
    hex::encode(Sha256::digest(h.as_bytes()))
  } else {
    cleaned
  }
}

fn sanitise_key_segment(value: &str) -> String {
  value
    .chars()
    .filter_map(|c| {
      match c {
        'A'..='Z' | 'a'..='z' | '0'..='9' | '-' | '_' | '.' => Some(c),
        '/' | '+' => Some('_'),
        _ => None,
      }
    })
    .collect()
}

/// Pull from the dispatch channel forever, sending each command through
/// the held builder capability.
#[expect(clippy::future_not_send, reason = "capnp future")]
async fn run_dispatch_pump(
  builder_cap: builder::Client,
  meta: Arc<AgentMeta>,
  cfg: Arc<ServerConfig>,
  db_pool: PgPool,
  mut rx: mpsc::UnboundedReceiver<DispatchCommand>,
) {
  while let Some(cmd) = rx.recv().await {
    meta.current_jobs.fetch_add(1, Ordering::Relaxed);
    let machine_id = meta.machine_id;
    let pool = db_pool.clone();
    let cfg = Arc::clone(&cfg);
    let builder_cap = builder_cap.clone();
    let current_jobs_counter = Arc::clone(&meta.current_jobs);
    let meta_for_task = Arc::clone(&meta);

    tokio::task::spawn_local(async move {
      let outcome = dispatch_one(
        &builder_cap,
        &cmd,
        &pool,
        &cfg,
        machine_id,
        &meta_for_task,
      )
      .await;
      current_jobs_counter.fetch_sub(1, Ordering::Relaxed);
      let _ = cmd.completion.send(outcome);
      tracing::debug!(%machine_id, build_id = %cmd.build_id, "dispatch finished");
    });
  }
}

#[expect(clippy::future_not_send, reason = "capnp future")]
async fn dispatch_one(
  builder_cap: &builder::Client,
  cmd: &DispatchCommand,
  pool: &PgPool,
  cfg: &ServerConfig,
  machine_id: Uuid,
  meta: &AgentMeta,
) -> DispatchResult {
  meta.active_builds.write().insert(cmd.build_id);
  let (done_tx, done_rx) = oneshot::channel::<BuildOutcomeKind>();
  let log_sink_impl = LogSinkImpl::new(cmd.log_path.clone(), cmd.max_log_size);
  let log_cap: log_sink::Client = capnp_rpc::new_client(log_sink_impl);
  let result_sink_impl = ResultSinkImpl {
    pool: pool.clone(),
    machine_id,
    done: Arc::new(tokio::sync::Mutex::new(Some(done_tx))),
  };
  let result_cap: result_sink::Client = capnp_rpc::new_client(result_sink_impl);

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
    }
    p.set_log(log_cap);
    p.set_result(result_cap);
  }

  if let Err(e) = req.send().promise.await {
    tracing::warn!(build_id = %cmd.build_id, "assign call failed: {e}");
    meta.active_builds.write().remove(&cmd.build_id);
    cfg.forget_uploads_for(machine_id, cmd.build_id);
    return DispatchResult::Disconnected;
  }

  let out = match done_rx.await {
    Ok(BuildOutcomeKind::Success) => DispatchResult::Succeeded,
    Ok(BuildOutcomeKind::TimedOut) => DispatchResult::TimedOut,
    Ok(BuildOutcomeKind::Aborted) => DispatchResult::Aborted,
    Ok(BuildOutcomeKind::Failure { error_message }) => {
      DispatchResult::Failed(error_message.unwrap_or_default())
    },
    Err(_) => DispatchResult::Disconnected,
  };
  meta.active_builds.write().remove(&cmd.build_id);
  cfg.forget_uploads_for(machine_id, cmd.build_id);
  out
}

fn parse_uuid_param(value: &str, name: &str) -> Result<Uuid, capnp::Error> {
  Uuid::parse_str(value)
    .map_err(|e| capnp::Error::failed(format!("bad {name}: {e}")))
}

fn read_bounded_text_list(
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
  if cpu == 0 {
    return Err(capnp::Error::failed(
      "agent cpu_count must be greater than 0".into(),
    ));
  }
  if max_jobs == 0 {
    return Err(capnp::Error::failed(
      "agent max_jobs must be greater than 0".into(),
    ));
  }
  Ok(())
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
    || !circus_common::validate::is_valid_store_path(path, "/nix/store")
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
      return Err(eyre!(
        "queue_runner.rpc.auth_tokens[{idx}] must decode to 32 bytes, got {}",
        decoded.len()
      ));
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

#[expect(clippy::too_many_arguments, reason = "DB upsert needs all fields")]
async fn upsert_session(
  pool: &PgPool,
  machine_id: Uuid,
  name: &str,
  hostname: &str,
  systems: &[String],
  supported: &[String],
  mandatory: &[String],
  speed: f32,
  cpu: i32,
  max_jobs: i32,
) -> Result<(), sqlx::Error> {
  sqlx::query(
    "INSERT INTO builder_sessions (machine_id, name, hostname, systems, \
     supported_features, mandatory_features, speed_factor, cpu_count, \
     max_jobs, proto_version, connected, last_seen, updated_at) VALUES ($1, \
     $2, $3, $4, $5, $6, $7, $8, $9, $10, TRUE, NOW(), NOW()) ON CONFLICT \
     (machine_id) DO UPDATE SET name = EXCLUDED.name, hostname = \
     EXCLUDED.hostname, systems = EXCLUDED.systems, supported_features = \
     EXCLUDED.supported_features, mandatory_features = \
     EXCLUDED.mandatory_features, speed_factor = EXCLUDED.speed_factor, \
     cpu_count = EXCLUDED.cpu_count, max_jobs = EXCLUDED.max_jobs, \
     proto_version = EXCLUDED.proto_version, connected = TRUE, last_seen = \
     NOW(), updated_at = NOW()",
  )
  .bind(machine_id)
  .bind(name)
  .bind(hostname)
  .bind(systems)
  .bind(supported)
  .bind(mandatory)
  .bind(speed)
  .bind(cpu)
  .bind(max_jobs)
  .bind(PROTO_VERSION)
  .execute(pool)
  .await?;
  Ok(())
}

async fn mark_disconnected(
  pool: &PgPool,
  machine_id: Uuid,
) -> Result<(), sqlx::Error> {
  sqlx::query(
    "UPDATE builder_sessions SET connected = FALSE, updated_at = NOW() WHERE \
     machine_id = $1",
  )
  .bind(machine_id)
  .execute(pool)
  .await?;
  Ok(())
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn verify_token_accepts_configured_sha256_digest() {
    let token = "correct horse battery staple";
    let digest = hex::encode(Sha256::digest(token.as_bytes()));
    assert!(verify_token(&[digest], token));
  }

  #[test]
  fn verify_token_rejects_invalid_or_different_digest() {
    let digest = hex::encode(Sha256::digest(b"other"));
    assert!(!verify_token(&["not-hex".into(), digest], "token"));
  }

  #[test]
  fn hash_key_segment_never_preserves_path_separators() {
    assert_eq!(hash_key_segment("sha256:abc/def+ghi="), "abc_def_ghi");
    assert_eq!(hash_key_segment("../../evil"), ".._.._evil");
  }

  #[test]
  fn validate_store_path_rejects_prefix_only_and_traversal() {
    assert!(validate_store_path("/nix/store/abc123-package").is_ok());
    assert!(validate_store_path("/nix/store/").is_err());
    assert!(validate_store_path("/nix/store/abc..def").is_err());
    assert!(validate_store_path("/tmp/not-store").is_err());
  }
}
