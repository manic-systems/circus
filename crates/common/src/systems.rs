//! Resolve `evaluator.systems` into the set of systems an instance builds.

use std::{collections::HashSet, hash::BuildHasher};

use circus_config::{EvaluatorConfig, EvaluatorSystems};

use crate::PgPool;

/// Resolve `evaluator.systems` into the set of systems to keep, [`None`]
/// meaning keep everything. Note that auto mode fails open when no agent is
/// connected.
pub async fn resolve_allowed_systems(
  pool: &PgPool,
  config: &EvaluatorConfig,
) -> Option<HashSet<String>> {
  match &config.systems {
    None => None,
    Some(EvaluatorSystems::List(list)) => Some(list.iter().cloned().collect()),
    Some(EvaluatorSystems::Keyword(_)) => {
      match crate::repo::builder_sessions::list_connected(pool).await {
        Ok(sessions) => {
          let systems = sessions
            .into_iter()
            .flat_map(|session| session.systems)
            .collect::<HashSet<String>>();
          if systems.is_empty() {
            tracing::warn!(
              "No connected agents advertise systems, keeping all jobs"
            );
            None
          } else {
            Some(systems)
          }
        },
        Err(error) => {
          tracing::warn!(%error, "Failed to list connected agents, keeping all jobs");
          None
        },
      }
    },
  }
}

#[must_use]
pub fn system_allowed<S: BuildHasher>(
  system: Option<&str>,
  allowed: Option<&HashSet<String, S>>,
) -> bool {
  match (system, allowed) {
    (Some(system), Some(allowed)) => allowed.contains(system),
    _ => true,
  }
}
