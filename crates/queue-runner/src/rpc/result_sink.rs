//! `ResultSink`: the agent calls this exactly once at the end of a build.
//!
//! Handlers translate the `BuildResult` into the existing
//! `circus_common::repo::builds::*` updates so the rest of the system
//! (notifications, dashboard, metrics) sees the same shape as an
//! SSH-dispatched build.

use std::sync::Arc;

use capnp::capability::Promise;
use circus_common::{models::BuildStatus, repo};
use circus_proto::{BuildOutcome, result_sink};
use sqlx::PgPool;
use tokio::sync::oneshot;
use uuid::Uuid;

/// One result sink per dispatch. Holds the PgPool, the build id, the
/// originating agent (so per-agent counters stay in sync), and a
/// completion notify the dispatch task awaits.
pub struct ResultSinkImpl {
  pub pool:       PgPool,
  pub build_id:   Uuid,
  pub machine_id: Uuid,
  /// Notified after the result is committed to the database. The
  /// scheduler task awaits this to free up the agent slot and pick the
  /// next build.
  pub done: Arc<tokio::sync::Mutex<Option<oneshot::Sender<BuildOutcomeKind>>>>,
}

#[derive(Debug, Clone, Copy)]
pub enum BuildOutcomeKind {
  Success,
  Failure,
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
    let build_id = self.build_id;
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
        _ => BuildOutcomeKind::Failure,
      };

      let status = match kind {
        BuildOutcomeKind::Success => BuildStatus::Succeeded,
        BuildOutcomeKind::Aborted => BuildStatus::Aborted,
        BuildOutcomeKind::TimedOut => BuildStatus::Timeout,
        BuildOutcomeKind::Failure => BuildStatus::Failed,
      };

      let err_msg = r
        .get_error_message()
        .ok()
        .and_then(|s| s.to_str().ok())
        .filter(|s| !s.is_empty())
        .map(std::borrow::ToOwned::to_owned);
      let primary_output = r
        .get_outputs()
        .ok()
        .and_then(|outs| outs.iter().next())
        .and_then(|o| o.get_path().ok())
        .and_then(|t| t.to_str().ok())
        .map(std::borrow::ToOwned::to_owned);

      // The existing repo layer handles timestamps. The log file is
      // appended to by the LogSink alongside this call; the path was
      // pre-computed at dispatch time and is the canonical
      // `<work_dir>/<build_id>.log`. We rely on the dispatcher to
      // pass the same path via the build row when starting.
      if let Err(e) = repo::builds::complete(
        &pool,
        build_id,
        status,
        None,
        primary_output.as_deref(),
        err_msg.as_deref(),
      )
      .await
      {
        tracing::warn!(%build_id, "failed to complete build: {e}");
      }

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
