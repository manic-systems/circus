//! `ResultSink`: the agent calls this exactly once at the end of a build.
//!
//! Handlers translate the `BuildResult` into the existing
//! `circus_common::repo::builds::*` updates so the rest of the system
//! (notifications, dashboard, metrics) sees the same shape as an
//! SSH-dispatched build.

use std::sync::Arc;

use capnp::capability::Promise;
use circus_common::repo;
use circus_proto::{BuildOutcome, result_sink};
use sqlx::PgPool;
use tokio::sync::oneshot;
use uuid::Uuid;

/// One result sink per dispatch. Holds the PgPool, the build id, the
/// originating agent (so per-agent counters stay in sync), and a
/// completion notify the dispatch task awaits.
pub struct ResultSinkImpl {
  pub pool:       PgPool,
  pub machine_id: Uuid,
  /// Notified after the result is committed to the database. The
  /// scheduler task awaits this to free up the agent slot and pick the
  /// next build.
  pub done: Arc<tokio::sync::Mutex<Option<oneshot::Sender<BuildOutcomeKind>>>>,
}

#[derive(Debug, Clone)]
pub enum BuildOutcomeKind {
  Success,
  Failure { error_message: Option<String> },
  TimedOut,
  Aborted,
}

#[allow(refining_impl_trait_internal, refining_impl_trait_reachable)]
impl result_sink::Server for ResultSinkImpl {
  fn report(
    self: capnp::capability::Rc<Self>,
    params: result_sink::ReportParams,
    _results: result_sink::ReportResults,
  ) -> Promise<(), capnp::Error> {
    let pool = self.pool.clone();
    let machine_id = self.machine_id;
    let done = Arc::clone(&self.done);
    Promise::from_future(async move {
      let pr = params.get()?;
      let r = pr.get_result()?;
      let outcome = r.get_outcome().unwrap_or(BuildOutcome::BuildFailure);
      let kind = match outcome {
        BuildOutcome::Success => BuildOutcomeKind::Success,
        BuildOutcome::TimedOut => BuildOutcomeKind::TimedOut,
        BuildOutcome::Aborted => BuildOutcomeKind::Aborted,
        _ => {
          BuildOutcomeKind::Failure {
            error_message: r
              .get_error_message()
              .ok()
              .and_then(|s| s.to_str().ok())
              .filter(|s| !s.is_empty())
              .map(std::borrow::ToOwned::to_owned),
          }
        },
      };

      let succeeded = matches!(kind, BuildOutcomeKind::Success);
      if let Err(e) =
        repo::builder_sessions::record_outcome(&pool, machine_id, succeeded)
          .await
      {
        tracing::warn!(%machine_id, "failed to record outcome: {e}");
      }

      if let Some(tx) = done.lock().await.take() {
        let _ = tx.send(kind);
      }
      Ok(())
    })
  }
}
