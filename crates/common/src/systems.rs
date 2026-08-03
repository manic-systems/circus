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

/// Intersect the instance allowlist with a jobset list, [`None`] meaning no
/// restriction from either side.
#[must_use]
pub fn restrict_to_jobset<S: BuildHasher + Default>(
  instance: Option<HashSet<String, S>>,
  jobset: Option<&[String]>,
) -> Option<HashSet<String, S>> {
  match (instance, jobset) {
    (instance, None) => instance,
    (None, Some(list)) => Some(list.iter().cloned().collect()),
    (Some(set), Some(list)) => {
      Some(
        list
          .iter()
          .filter(|system| set.contains(*system))
          .cloned()
          .collect(),
      )
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

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn jobset_list_intersects_with_instance_allowlist() {
    let instance = Some(HashSet::from(["x86_64-linux".to_owned()]));
    let jobset = ["x86_64-linux".to_owned(), "aarch64-darwin".to_owned()];

    let effective =
      restrict_to_jobset(instance, Some(&jobset)).expect("restriction present");
    assert_eq!(effective, HashSet::from(["x86_64-linux".to_owned()]));
    let unmatched = ["aarch64-darwin".to_owned()];
    assert!(
      restrict_to_jobset(
        Some(HashSet::from(["x86_64-linux".to_owned()])),
        Some(&unmatched),
      )
      .expect("restriction present")
      .is_empty()
    );
    assert!(
      restrict_to_jobset(Option::<HashSet<String>>::None, None).is_none()
    );
  }
}
