//! Prometheus counters for each instance, served as text over plain
//! HTTP/1.1, since scrapers do not speak HTTP/2 with prior knowledge.

use std::{
  collections::BTreeMap,
  convert::Infallible,
  fmt::Write as _,
  sync::{
    Arc,
    atomic::{AtomicU64, Ordering},
  },
};

use bytes::Bytes;
use http::{Response, StatusCode, header};
use http_body_util::Full;
use hyper_util::rt::TokioIo;
use tokio::net::TcpListener;

use crate::{service::Instance, store::Store};

#[derive(Default)]
pub struct Counters {
  pub ac_hits:    AtomicU64,
  pub ac_misses:  AtomicU64,
  pub cas_hits:   AtomicU64,
  pub cas_misses: AtomicU64,
  /// Bytes as they crossed the wire, compressed or not.
  pub wire_in:    AtomicU64,
  pub wire_out:   AtomicU64,
  /// Bytes of blobs and action results, uncompressed.
  pub data_in:    AtomicU64,
  pub data_out:   AtomicU64,
}

/// Serves `/metrics` for `instances` on `listener` until the process ends.
pub async fn serve(
  listener: TcpListener,
  instances: Arc<BTreeMap<String, Arc<Instance>>>,
) {
  #[expect(clippy::infinite_loop, reason = "intentional accept loop")]
  loop {
    let stream = match listener.accept().await {
      Ok((stream, _)) => stream,
      Err(error) => {
        tracing::warn!("remote cache metrics accept failed: {error}");
        continue;
      },
    };

    let instances = Arc::clone(&instances);
    tokio::spawn(async move {
      let handler = hyper::service::service_fn(move |request| {
        let response = if request.uri().path() == "/metrics" {
          Response::builder()
            .header(header::CONTENT_TYPE, "text/plain; version=0.0.4")
            .body(Full::new(Bytes::from(render(&instances))))
        } else {
          Response::builder()
            .status(StatusCode::NOT_FOUND)
            .body(Full::default())
        };
        async move { Ok::<_, Infallible>(response.unwrap_or_default()) }
      });

      let served = hyper::server::conn::http1::Builder::new()
        .serve_connection(TokioIo::new(stream), handler)
        .await;

      if let Err(error) = served {
        tracing::debug!("remote cache metrics connection ended: {error}");
      }
    });
  }
}

fn render(instances: &BTreeMap<String, Arc<Instance>>) -> String {
  let mut out = String::new();

  let counter = |out: &mut String, name, help, rows: &[(String, u64)]| {
    family(out, name, "counter", help, rows);
  };
  let gauge = |out: &mut String, name, help, rows: &[(String, u64)]| {
    family(out, name, "gauge", help, rows);
  };
  let per_instance = |pick: fn(&Counters) -> &AtomicU64, result: &str| {
    instances
      .iter()
      .map(|(name, instance)| {
        (
          format!("reapi_instance=\"{}\"{result}", escape(name)),
          pick(&instance.metrics).load(Ordering::Relaxed),
        )
      })
      .collect::<Vec<_>>()
  };
  let per_store = |pick: fn(&Store) -> u64| {
    instances
      .iter()
      .flat_map(|(name, instance)| {
        [("cas", &instance.cas), ("ac", &instance.ac)].map(|(kind, store)| {
          (
            format!("reapi_instance=\"{}\",store=\"{kind}\"", escape(name)),
            pick(store),
          )
        })
      })
      .collect::<Vec<_>>()
  };

  let ac = [
    per_instance(|counters| &counters.ac_hits, ",result=\"hit\""),
    per_instance(|counters| &counters.ac_misses, ",result=\"miss\""),
  ]
  .concat();
  counter(
    &mut out,
    "circus_remote_cache_action_cache_lookups_total",
    "Action cache lookups by result",
    &ac,
  );

  let cas = [
    per_instance(|counters| &counters.cas_hits, ",result=\"hit\""),
    per_instance(|counters| &counters.cas_misses, ",result=\"miss\""),
  ]
  .concat();
  counter(
    &mut out,
    "circus_remote_cache_cas_lookups_total",
    "CAS blob lookups by result",
    &cas,
  );

  let wire = [
    per_instance(|counters| &counters.wire_in, ",direction=\"in\""),
    per_instance(|counters| &counters.wire_out, ",direction=\"out\""),
  ]
  .concat();
  counter(
    &mut out,
    "circus_remote_cache_wire_bytes_total",
    "Bytes sent and received, as transferred",
    &wire,
  );

  let data = [
    per_instance(|counters| &counters.data_in, ",direction=\"in\""),
    per_instance(|counters| &counters.data_out, ",direction=\"out\""),
  ]
  .concat();
  counter(
    &mut out,
    "circus_remote_cache_data_bytes_total",
    "Blob and action result bytes sent and received, uncompressed",
    &data,
  );

  counter(
    &mut out,
    "circus_remote_cache_evictions_total",
    "Objects evicted past the byte budget",
    &per_store(|store| store.stats().evictions),
  );
  counter(
    &mut out,
    "circus_remote_cache_evicted_bytes_total",
    "Bytes evicted past the byte budget",
    &per_store(|store| store.stats().evicted_bytes),
  );
  gauge(
    &mut out,
    "circus_remote_cache_stored_bytes",
    "Bytes held",
    &per_store(|store| store.stats().bytes),
  );
  gauge(
    &mut out,
    "circus_remote_cache_stored_objects",
    "Objects held",
    &per_store(|store| store.stats().objects),
  );
  gauge(
    &mut out,
    "circus_remote_cache_budget_bytes",
    "Byte budget past which the least recently used are evicted",
    &per_store(Store::budget),
  );

  out
}

fn family(
  out: &mut String,
  name: &str,
  kind: &str,
  help: &str,
  rows: &[(String, u64)],
) {
  // Writing to a String cannot fail.
  let _ = writeln!(out, "# HELP {name} {help}\n# TYPE {name} {kind}");
  for (labels, value) in rows {
    let _ = writeln!(out, "{name}{{{labels}}} {value}");
  }
}

fn escape(label: &str) -> String {
  label
    .replace('\\', "\\\\")
    .replace('"', "\\\"")
    .replace('\n', "\\n")
}
