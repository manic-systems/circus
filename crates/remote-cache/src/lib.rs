//! A Bazel remote-execution API cache, the action cache and CAS without
//! execution. Each listener speaks HTTP/2 with prior knowledge, or TLS that
//! requires a client certificate.

mod digest;
mod grpc;
mod metrics;
mod proto;
mod service;
mod store;

use std::{
  collections::HashMap,
  convert::Infallible,
  ffi::OsString,
  path::PathBuf,
  sync::Arc,
};

use circus_config::{Config, RemoteCacheConfig, RemoteCacheTls};
use clap::Parser;
use color_eyre::eyre::{Context as _, eyre};
use hyper_util::rt::{TokioExecutor, TokioIo};
use rustls::{
  RootCertStore,
  ServerConfig,
  pki_types::{
    CertificateDer,
    CertificateRevocationListDer,
    PrivateKeyDer,
    pem::PemObject as _,
  },
  server::WebPkiClientVerifier,
};
use tokio::{
  io::{AsyncRead, AsyncWrite},
  net::TcpListener,
};
use tokio_rustls::TlsAcceptor;

use crate::{
  metrics::Counters,
  service::{Instance, Service},
  store::Store,
};

#[derive(Parser)]
#[command(name = "circus-remote-cache")]
#[command(about = "REAPI remote cache: action cache and CAS")]
struct Cli {
  #[arg(short, long)]
  config: Option<PathBuf>,
}

/// Run the remote cache until SIGINT or SIGTERM.
///
/// # Errors
///
/// Returns an error when configuration, a store, TLS material or a listener
/// is unusable.
pub fn run() -> color_eyre::Result<()> {
  run_from(std::env::args_os())
}

/// Run the remote cache with explicit argv values.
///
/// # Errors
///
/// Returns an error when configuration, a store, TLS material or a listener
/// is unusable.
pub fn run_from<I, T>(args: I) -> color_eyre::Result<()>
where
  I: IntoIterator<Item = T>,
  T: Into<OsString> + Clone,
{
  color_eyre::install()?;
  circus_common::install_crypto_provider()?;

  let cli = Cli::parse_from(args);
  let config = Config::load(cli.config.as_deref())?;

  tokio::runtime::Builder::new_multi_thread()
    .enable_all()
    .build()?
    .block_on(async move {
      circus_common::init_tracing(&config.tracing);
      start(config.remote_cache).await?;
      shutdown_signal().await?;
      tracing::info!("remote cache shutting down");
      Ok(())
    })
}

async fn shutdown_signal() -> std::io::Result<()> {
  let mut terminate =
    tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())?;
  tokio::select! {
    interrupted = tokio::signal::ctrl_c() => interrupted,
    _ = terminate.recv() => Ok(()),
  }
}

/// Opens every instance's stores and starts every listener in the
/// background.
async fn start(config: RemoteCacheConfig) -> color_eyre::Result<()> {
  config.validate()?;

  let RemoteCacheConfig {
    root,
    instances: wanted,
    listeners,
    metrics_bind,
  } = config;

  let instances = tokio::task::spawn_blocking(move || {
    wanted
      .into_iter()
      .map(|instance| {
        let dir = root.join(&instance.name);
        let opened = Arc::new(Instance {
          cas:     Store::open(dir.join("cas"), instance.cas_max_bytes)?,
          ac:      Store::open(dir.join("ac"), instance.ac_max_bytes)?,
          metrics: Counters::default(),
        });
        Ok((instance.name, opened))
      })
      .collect::<std::io::Result<HashMap<_, _>>>()
      .wrap_err_with(|| format!("open remote cache under {}", root.display()))
  })
  .await??;

  for listener in &listeners {
    let acceptor = listener.tls.as_ref().map(acceptor).transpose()?;
    let tcp = TcpListener::bind(&listener.bind).await.wrap_err_with(|| {
      format!("bind remote cache listener {}", listener.bind)
    })?;

    tracing::info!(
      bind = listener.bind,
      tls = acceptor.is_some(),
      writable = listener.writable,
      "remote cache listening"
    );

    let served = instances
      .iter()
      .filter(|(name, _)| {
        listener.instances.is_empty() || listener.instances.contains(name)
      })
      .map(|(name, instance)| (name.clone(), Arc::clone(instance)))
      .collect();

    let service = Arc::new(Service {
      instances: served,
      writable:  listener.writable,
    });

    tokio::spawn(accept(tcp, acceptor, service));
  }

  if let Some(bind) = metrics_bind {
    let tcp = TcpListener::bind(&bind)
      .await
      .wrap_err_with(|| format!("bind remote cache metrics {bind}"))?;
    tracing::info!(bind, "remote cache metrics listening");
    tokio::spawn(metrics::serve(
      tcp,
      Arc::new(instances.into_iter().collect()),
    ));
  }

  Ok(())
}

async fn accept(
  tcp: TcpListener,
  acceptor: Option<TlsAcceptor>,
  service: Arc<Service>,
) {
  #[expect(clippy::infinite_loop, reason = "intentional accept loop")]
  loop {
    let (stream, peer) = match tcp.accept().await {
      Ok(accepted) => accepted,
      Err(error) => {
        tracing::warn!("remote cache accept failed: {error}");
        continue;
      },
    };

    drop(stream.set_nodelay(true));
    let service = Arc::clone(&service);
    let acceptor = acceptor.clone();

    tokio::spawn(async move {
      match acceptor {
        None => serve(stream, service).await,
        Some(acceptor) => {
          match acceptor.accept(stream).await {
            Ok(tls) => serve(tls, service).await,
            Err(error) => {
              tracing::info!(%peer, "remote cache TLS handshake refused: {error}");
            },
          }
        },
      }
    });
  }
}

async fn serve<S>(io: S, service: Arc<Service>)
where
  S: AsyncRead + AsyncWrite + Unpin + Send + 'static,
{
  let handler = hyper::service::service_fn(move |request| {
    let service = Arc::clone(&service);
    async move { Ok::<_, Infallible>(service.handle(request).await) }
  });

  // The 64 KiB default windows cap a stream at 64 KiB per round trip.
  let served = hyper::server::conn::http2::Builder::new(TokioExecutor::new())
    .initial_stream_window_size(4 << 20)
    .initial_connection_window_size(16 << 20)
    .max_frame_size(1 << 20)
    .serve_connection(TokioIo::new(io), handler)
    .await;

  if let Err(error) = served {
    tracing::debug!("remote cache connection ended: {error}");
  }
}

fn acceptor(tls: &RemoteCacheTls) -> color_eyre::Result<TlsAcceptor> {
  let chain = CertificateDer::pem_file_iter(&tls.cert_file)
    .and_then(Iterator::collect::<Result<Vec<_>, _>>)
    .map_err(|error| eyre!("read {}: {error}", tls.cert_file.display()))?;
  let key = PrivateKeyDer::from_pem_file(&tls.key_file)
    .map_err(|error| eyre!("read {}: {error}", tls.key_file.display()))?;

  let mut roots = RootCertStore::empty();
  for certificate in CertificateDer::pem_file_iter(&tls.client_ca)
    .map_err(|error| eyre!("read {}: {error}", tls.client_ca.display()))?
  {
    roots.add(certificate?)?;
  }

  let mut verifier = WebPkiClientVerifier::builder(Arc::new(roots));

  if let Some(crl_file) = &tls.crl_file {
    let crls = CertificateRevocationListDer::pem_file_iter(crl_file)
      .and_then(Iterator::collect::<Result<Vec<_>, _>>)
      .map_err(|error| eyre!("read {}: {error}", crl_file.display()))?;
    verifier = verifier.with_crls(crls);
  }

  let mut config = ServerConfig::builder()
    .with_client_cert_verifier(verifier.build()?)
    .with_single_cert(chain, key)?;

  config.alpn_protocols = vec![b"h2".to_vec()];

  Ok(TlsAcceptor::from(Arc::new(config)))
}
