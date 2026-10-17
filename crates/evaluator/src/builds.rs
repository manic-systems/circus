use std::{
  collections::{HashMap, HashSet},
  future,
  path::Path,
  time::Duration,
};

use circus_common::{
  PgPool,
  models::{
    Build,
    CreateBuild,
    DEPENDENCY_JOB_PREFIX,
    EvaluationStatus,
    JobsetInput,
  },
  repo,
  systems::system_allowed,
};
use data_encoding::HEXLOWER;
use futures::stream::{self, StreamExt};
use tokio::process::Command;
use uuid::Uuid;

use crate::{memory::MemoryLimit, nix::NixJob};

async fn read_required_features(
  drv_path: &str,
  memory_limit: MemoryLimit,
) -> Vec<String> {
  let mut command =
    circus_nix::derivation::required_features_command(&[drv_path.to_owned()]);
  command.kill_on_drop(true);
  if memory_limit.apply_to(&mut command).is_err() {
    return Vec::new();
  }
  let Ok(output) = command.output().await else {
    return Vec::new();
  };
  if !output.status.success() {
    return Vec::new();
  }
  serde_json::from_slice(&output.stdout)
    .map(|value| circus_nix::derivation::union_required_features(&value))
    .unwrap_or_default()
}

#[derive(Debug, Clone)]
struct DerivationInfo {
  system:            Option<String>,
  outputs:           Option<HashMap<String, String>>,
  input_drvs:        Option<HashMap<String, serde_json::Value>>,
  required_features: Vec<String>,
}

fn parse_derivation_infos(
  store_dir: &Path,
  value: &serde_json::Value,
) -> HashMap<String, DerivationInfo> {
  let Some(derivations) = value
    .get("derivations")
    .and_then(serde_json::Value::as_object)
    .or_else(|| value.as_object())
  else {
    return HashMap::new();
  };

  derivations
    .iter()
    .map(|(drv_path, drv_val)| {
      let drv_path = circus_nix::derivation::store_path(store_dir, drv_path);
      let system = circus_nix::derivation::drv_system(drv_val);
      let outputs = circus_nix::derivation::drv_outputs(store_dir, drv_val);
      let input_drvs =
        circus_nix::derivation::drv_input_drvs(store_dir, drv_val);
      let required_features =
        circus_nix::derivation::drv_required_features(drv_val);

      (drv_path, DerivationInfo {
        system,
        outputs,
        input_drvs,
        required_features,
      })
    })
    .collect()
}

async fn show_derivations(
  drv_paths: &[String],
  memory_limit: MemoryLimit,
) -> HashMap<String, DerivationInfo> {
  let mut derivations = HashMap::new();
  let store_dir = match circus_nix::derivation::store_dir().await {
    Ok(dir) => dir,
    Err(error) => {
      tracing::warn!(%error, "failed to read the nix store directory");
      return derivations;
    },
  };

  for chunk in drv_paths.chunks(1024) {
    let mut command = Command::new("nix");
    command
      .args(["derivation", "show"])
      .args(chunk)
      .kill_on_drop(true);
    if let Err(error) = memory_limit.apply_to(&mut command) {
      tracing::warn!(%error, "failed to apply evaluator memory limit");
      return derivations;
    }
    let output = match command.output().await {
      Ok(output) => output,
      Err(error) => {
        tracing::warn!(%error, "failed to run nix derivation show");
        continue;
      },
    };
    if !output.status.success() {
      tracing::warn!(
        stderr = %String::from_utf8_lossy(&output.stderr),
        "nix derivation show failed"
      );
      continue;
    }
    if let Ok(value) =
      serde_json::from_slice::<serde_json::Value>(&output.stdout)
    {
      derivations.extend(parse_derivation_infos(store_dir, &value));
    }
  }
  derivations
}

/// Outputs neither in the local store nor on a substituter. Paths the checks
/// cannot vouch for are treated as invalid so the derivation still gets
/// enqueued.
async fn invalid_output_paths<'a>(
  derivations: impl Iterator<Item = &'a DerivationInfo>,
  substituters: &[String],
  memory_limit: MemoryLimit,
) -> HashSet<String> {
  let paths = derivations
    .filter_map(|info| info.outputs.as_ref())
    .flat_map(|outputs| outputs.values().cloned())
    .collect::<HashSet<String>>()
    .into_iter()
    .collect::<Vec<String>>();

  let mut invalid = HashSet::new();
  for chunk in paths.chunks(1024) {
    let mut command = Command::new("nix-store");
    command
      .args(["--check-validity", "--print-invalid"])
      .args(chunk)
      .kill_on_drop(true);
    let output = if memory_limit.apply_to(&mut command).is_ok() {
      command.output().await.ok()
    } else {
      None
    };
    match output {
      Some(output) if output.status.success() => {
        invalid.extend(
          String::from_utf8_lossy(&output.stdout)
            .lines()
            .map(str::to_owned),
        );
      },
      _ => invalid.extend(chunk.iter().cloned()),
    }
  }

  for path in substitutable(&invalid, substituters).await {
    invalid.remove(&path);
  }
  invalid
}

/// The HTTP binary caches this host's Nix substitutes from.
async fn http_substituters(memory_limit: MemoryLimit) -> Vec<String> {
  let mut command = Command::new("nix");
  command
    .args(["config", "show", "substituters"])
    .kill_on_drop(true);
  if memory_limit.apply_to(&mut command).is_err() {
    return Vec::new();
  }

  match command.output().await {
    Ok(output) if output.status.success() => {
      String::from_utf8_lossy(&output.stdout)
        .split_whitespace()
        .filter(|url| url.starts_with("http://") || url.starts_with("https://"))
        // Query parameters are store settings like `priority`.
        .map(|url| url.split_once('?').map_or(url, |(base, _)| base))
        .map(|url| url.trim_end_matches('/').to_owned())
        .collect()
    },
    Ok(output) => {
      tracing::warn!(
        stderr = %String::from_utf8_lossy(&output.stderr).trim(),
        "Failed to read Nix substituters"
      );
      Vec::new()
    },
    Err(error) => {
      tracing::warn!("Failed to read Nix substituters: {error}");
      Vec::new()
    },
  }
}

/// Paths some substituter already serves. An agent fetches these instead of
/// building them, so they need no build of their own.
async fn substitutable(
  paths: &HashSet<String>,
  substituters: &[String],
) -> Vec<String> {
  if paths.is_empty() || substituters.is_empty() {
    return Vec::new();
  }

  let client = match circus_http::api()
    .connect_timeout(Duration::from_secs(5))
    .timeout(Duration::from_secs(10))
    .build()
  {
    Ok(client) => client,
    Err(error) => {
      tracing::warn!("Failed to build the substituter client: {error}");
      return Vec::new();
    },
  };

  stream::iter(paths)
    .map(|path| {
      let client = &client;
      async move {
        let hash = Path::new(path)
          .file_name()
          .and_then(|name| name.to_str())
          .and_then(|name| name.get(..32))?;
        for substituter in substituters {
          let found = client
            .head(format!("{substituter}/{hash}.narinfo"))
            .send()
            .await
            .is_ok_and(|response| response.status().is_success());
          if found {
            return Some(path.clone());
          }
        }
        None
      }
    })
    .buffer_unordered(32)
    .filter_map(future::ready)
    .collect()
    .await
}

fn should_enqueue_derivation(
  info: &DerivationInfo,
  invalid_outputs: &HashSet<String>,
) -> bool {
  let Some(outputs) = &info.outputs else {
    return true;
  };
  outputs
    .values()
    .any(|output| invalid_outputs.contains(output))
}

fn dependency_job_name(drv_path: &str) -> String {
  let basename = Path::new(drv_path)
    .file_name()
    .and_then(|name| name.to_str())
    .unwrap_or(drv_path)
    .trim_end_matches(".drv");
  format!("{DEPENDENCY_JOB_PREFIX}{basename}")
}

/// Detect whether a derivation is a fixed-output derivation from the output
/// list of its `.drv`.
///
/// # Returns
///
/// Returns `(is_fod, fod_hash)`.
fn detect_fod(drv_path: &str) -> (bool, Option<String>) {
  let Ok(content) = std::fs::read_to_string(drv_path) else {
    return (false, None);
  };
  // ATerm: Derive([("out","<path>","<algo>","<hash>")],...), one output only.
  let Some((outputs, _)) = content
    .strip_prefix("Derive([(")
    .and_then(|rest| rest.split_once(")],"))
  else {
    return (false, None);
  };
  match outputs.split(',').collect::<Vec<_>>().as_slice() {
    ["\"out\"", _path, algo, hash] if *algo != "\"\"" && *hash != "\"\"" => {
      (true, Some(hash.trim_matches('"').to_string()))
    },
    _ => (false, None),
  }
}

/// Resolve the dependency edges to insert for `jobs`, deduplicated. Jobs can
/// alias one drv path (`packages.default`), so edges are anchored to each
/// job's own build id rather than the drv-keyed map, which only retains the
/// last build per drv.
fn resolve_dependency_pairs(
  jobs: &[NixJob],
  build_ids: &[Uuid],
  drv_to_build: &HashMap<String, Uuid>,
  name_to_build: &HashMap<String, Uuid>,
) -> Vec<(Uuid, Uuid)> {
  let mut seen = HashSet::new();
  let mut pairs = Vec::new();
  for (job, &build_id) in jobs.iter().zip(build_ids) {
    if let Some(input_drvs) = &job.input_drvs {
      for dep_drv in input_drvs.keys() {
        if let Some(&dep_build_id) = drv_to_build.get(dep_drv)
          && dep_build_id != build_id
          && seen.insert((build_id, dep_build_id))
        {
          pairs.push((build_id, dep_build_id));
        }
      }
    }

    if let Some(constituents) = &job.constituents {
      for constituent_name in constituents {
        if let Some(&dep_build_id) = name_to_build.get(constituent_name)
          && dep_build_id != build_id
          && seen.insert((build_id, dep_build_id))
        {
          pairs.push((build_id, dep_build_id));
        }
      }
    }
  }
  pairs
}

/// Records an evaluation's builds batch by batch while it is still running.
///
/// Each batch commits together with every dependency build and edge it needs,
/// so the queue runner never sees a build before the edges that hold it back.
/// Aggregates wait for [`BuildStream::complete`] because their constituents can
/// arrive in any later batch.
pub(crate) struct BuildStream<'a> {
  pool:            &'a PgPool,
  eval_id:         Uuid,
  memory_limit:    MemoryLimit,
  allowed_systems: Option<&'a HashSet<String>>,
  drv_to_build:    HashMap<String, Uuid>,
  name_to_build:   HashMap<String, Uuid>,
  name_to_drv:     HashMap<String, String>,
  dependencies:    HashMap<String, Uuid>,
  present:         HashSet<String>,
  touched:         HashSet<Uuid>,
  substituters:    Vec<String>,
  aggregates:      Vec<NixJob>,
  dropped:         usize,
}

impl<'a> BuildStream<'a> {
  /// An orphaned evaluation re-runs under the same id, so builds its earlier
  /// attempt recorded are picked up instead of inserted again.
  pub(crate) async fn open(
    pool: &'a PgPool,
    eval_id: Uuid,
    memory_limit: MemoryLimit,
    allowed_systems: Option<&'a HashSet<String>>,
  ) -> color_eyre::Result<Self> {
    let mut drv_to_build = HashMap::new();
    let mut name_to_build = HashMap::new();
    let mut name_to_drv = HashMap::new();
    let mut dependencies = HashMap::new();
    for build in repo::builds::list_for_evaluation(pool, eval_id).await? {
      if build.job_name.starts_with(DEPENDENCY_JOB_PREFIX) {
        dependencies.insert(build.drv_path.clone(), build.id);
      }
      drv_to_build.insert(build.drv_path.clone(), build.id);
      name_to_build.insert(build.job_name.clone(), build.id);
      name_to_drv.insert(build.job_name, build.drv_path);
    }

    Ok(Self {
      pool,
      eval_id,
      memory_limit,
      allowed_systems,
      drv_to_build,
      name_to_build,
      name_to_drv,
      dependencies,
      present: HashSet::new(),
      touched: HashSet::new(),
      substituters: http_substituters(memory_limit).await,
      aggregates: Vec::new(),
      dropped: 0,
    })
  }

  /// Record a batch of jobs.
  ///
  /// # Returns
  ///
  /// The builds created, or `None` once the evaluation is no longer running.
  pub(crate) async fn insert(
    &mut self,
    jobs: Vec<NixJob>,
  ) -> color_eyre::Result<Option<Vec<Build>>> {
    let jobs = self.allowed(jobs);
    let (aggregates, jobs): (Vec<_>, Vec<_>) =
      jobs.into_iter().partition(|job| job.constituents.is_some());
    self.aggregates.extend(aggregates);
    self.persist(jobs, false).await
  }

  /// Record the aggregates and mark the evaluation completed.
  ///
  /// # Returns
  ///
  /// `false` if the evaluation stopped running first.
  pub(crate) async fn complete(mut self) -> color_eyre::Result<bool> {
    let aggregates = std::mem::take(&mut self.aggregates);
    let completed = self.persist(aggregates, true).await?.is_some();
    if self.dropped > 0 {
      tracing::info!(
        dropped = self.dropped,
        "Dropped jobs for systems outside evaluator.systems"
      );
    }
    Ok(completed)
  }

  fn allowed(&mut self, jobs: Vec<NixJob>) -> Vec<NixJob> {
    let total = jobs.len();
    let jobs = jobs
      .into_iter()
      .filter(|job| system_allowed(job.system.as_deref(), self.allowed_systems))
      .collect::<Vec<_>>();
    self.dropped += total - jobs.len();
    jobs
  }

  async fn expand(
    &mut self,
    jobs: Vec<NixJob>,
  ) -> (Vec<NixJob>, HashMap<String, DerivationInfo>) {
    let top_level_drvs = jobs
      .iter()
      .map(|job| job.drv_path.clone())
      .collect::<Vec<_>>();
    let mut derivations =
      show_derivations(&top_level_drvs, self.memory_limit).await;
    let mut included = top_level_drvs.into_iter().collect::<HashSet<_>>();
    let mut frontier = jobs
      .iter()
      .filter_map(|job| job.input_drvs.as_ref())
      .flat_map(|input_drvs| input_drvs.keys().cloned())
      .collect::<Vec<_>>();
    let mut expanded = jobs;

    loop {
      frontier.retain(|drv_path| {
        !included.contains(drv_path)
          && !self.drv_to_build.contains_key(drv_path)
          && !self.present.contains(drv_path)
      });
      frontier.sort_unstable();
      frontier.dedup();
      if frontier.is_empty() {
        break;
      }

      let shown = show_derivations(&frontier, self.memory_limit).await;
      let invalid_outputs = invalid_output_paths(
        shown.values(),
        &self.substituters,
        self.memory_limit,
      )
      .await;
      let mut next = Vec::new();
      for drv_path in std::mem::take(&mut frontier) {
        let Some(info) = shown.get(&drv_path) else {
          continue;
        };
        if !should_enqueue_derivation(info, &invalid_outputs) {
          self.present.insert(drv_path);
          continue;
        }

        included.insert(drv_path.clone());
        if let Some(input_drvs) = &info.input_drvs {
          next.extend(input_drvs.keys().cloned());
        }
        expanded.push(NixJob {
          name: dependency_job_name(&drv_path),
          drv_path,
          system: info.system.clone(),
          outputs: info.outputs.clone(),
          input_drvs: info.input_drvs.clone(),
          constituents: None,
          meta: crate::nix::NixMeta::default(),
        });
      }
      derivations.extend(shown);
      frontier = next;
    }

    (expanded, derivations)
  }

  async fn persist(
    &mut self,
    jobs: Vec<NixJob>,
    finish: bool,
  ) -> color_eyre::Result<Option<Vec<Build>>> {
    // A restart keeps running and succeeded builds, which only stand in for a
    // job whose derivation is unchanged.
    let mut stale = Vec::new();
    let jobs = jobs
      .into_iter()
      .filter(|job| {
        let Some(&id) = self.name_to_build.get(&job.name) else {
          return true;
        };
        if self.name_to_drv.get(&job.name) == Some(&job.drv_path) {
          self.touched.insert(id);
          return false;
        }
        stale.push(id);
        true
      })
      .collect::<Vec<_>>();
    let (expanded, derivations) = self.expand(jobs).await;
    let jobs = self.allowed(expanded);

    let mut builds = Vec::with_capacity(jobs.len());
    for job in &jobs {
      let outputs_json = job
        .outputs
        .as_ref()
        .map(|o| serde_json::to_value(o).unwrap_or_default());
      let constituents_json = job
        .constituents
        .as_ref()
        .map(|c| serde_json::to_value(c).unwrap_or_default());
      let is_aggregate = job.constituents.is_some();

      let (is_fod, fod_hash) = detect_fod(&job.drv_path);
      let required_features = match derivations.get(&job.drv_path) {
        Some(info) => info.required_features.clone(),
        None => read_required_features(&job.drv_path, self.memory_limit).await,
      };
      builds.push(CreateBuild {
        evaluation_id: self.eval_id,
        job_name: job.name.clone(),
        drv_path: job.drv_path.clone(),
        system: job.system.clone(),
        outputs: outputs_json,
        is_aggregate: Some(is_aggregate),
        constituents: constituents_json,
        is_fod: Some(is_fod),
        fod_hash,
        meta_description: job.meta.description.clone(),
        meta_license: job.meta.license.clone(),
        meta_homepage: job.meta.homepage.clone(),
        meta_maintainers: job.meta.maintainers.clone(),
        required_features,
      });
    }

    let mut client = self.pool.get().await?;
    let tx = client.transaction().await?;
    if !repo::evaluations::lock_running(&tx, self.eval_id).await? {
      return Ok(None);
    }

    for &id in &stale {
      repo::builds::delete_in_transaction(&tx, id).await?;
    }
    self
      .drv_to_build
      .retain(|_, build_id| !stale.contains(build_id));
    self
      .dependencies
      .retain(|_, build_id| !stale.contains(build_id));

    let mut created = Vec::with_capacity(builds.len());
    let mut build_ids = Vec::with_capacity(builds.len());
    let mut adopted_ids = HashSet::new();
    for build in builds {
      let adopted = if build.job_name.starts_with(DEPENDENCY_JOB_PREFIX)
        || build.is_aggregate == Some(true)
      {
        None
      } else {
        self.dependencies.remove(&build.drv_path)
      };
      let build = match adopted {
        Some(id) => {
          adopted_ids.insert(id);
          repo::builds::adopt_dependency_in_transaction(&tx, id, &build).await?
        },
        None => repo::builds::create_in_transaction(&tx, build).await?,
      };
      if build.job_name.starts_with(DEPENDENCY_JOB_PREFIX) {
        self.dependencies.insert(build.drv_path.clone(), build.id);
      }
      self.drv_to_build.insert(build.drv_path.clone(), build.id);
      self.name_to_build.insert(build.job_name.clone(), build.id);
      self
        .name_to_drv
        .insert(build.job_name.clone(), build.drv_path.clone());
      self.touched.insert(build.id);
      build_ids.push(build.id);
      created.push(build);
    }

    let pairs = resolve_dependency_pairs(
      &jobs,
      &build_ids,
      &self.drv_to_build,
      &self.name_to_build,
    );
    for (build_id, dep_build_id) in pairs
      .into_iter()
      .filter(|(build_id, _)| !adopted_ids.contains(build_id))
    {
      repo::build_dependencies::create_in_transaction(
        &tx,
        build_id,
        dep_build_id,
      )
      .await?;
    }

    if finish {
      // A restart keeps builds the new attempt may no longer want.
      let kept = self.touched.iter().copied().collect::<Vec<_>>();
      repo::builds::delete_unreached_in_transaction(&tx, self.eval_id, &kept)
        .await?;

      let finished = repo::evaluations::finish_running_in_transaction(
        &tx,
        self.eval_id,
        EvaluationStatus::Completed,
        None,
      )
      .await?;
      if !finished {
        return Err(color_eyre::eyre::eyre!(
          "evaluation {} lost its running state while locked",
          self.eval_id
        ));
      }
    }
    tx.commit().await?;
    Ok(Some(created))
  }
}

/// Compute a deterministic hash over the commit and all jobset inputs.
/// Used for evaluation caching, so skip re-eval when inputs haven't changed.
pub(crate) fn compute_inputs_hash(
  commit_hash: &str,
  inputs: &[JobsetInput],
) -> String {
  use sha2::{Digest, Sha256};

  let mut hasher = Sha256::new();
  hasher.update(commit_hash.as_bytes());

  // Sort inputs by name for deterministic hashing
  let mut sorted_inputs: Vec<&JobsetInput> = inputs.iter().collect();
  sorted_inputs.sort_by_key(|i| &i.name);

  for input in sorted_inputs {
    hasher.update(input.name.as_bytes());
    hasher.update(input.input_type.as_str().as_bytes());
    hasher.update(input.value.as_bytes());
    if let Some(ref rev) = input.revision {
      hasher.update(rev.as_bytes());
    }
  }

  HEXLOWER.encode(&hasher.finalize())
}

#[cfg(test)]
mod tests {
  use std::collections::HashMap;

  use circus_common::{
    PgPool,
    models::{
      BinaryCacheUpstreams,
      CreateBuild,
      CreateProject,
      EvaluationStatus,
    },
    repo,
  };
  use uuid::Uuid;

  use super::{BuildStream, resolve_dependency_pairs};
  use crate::{
    memory::MemoryLimit,
    nix::{NixJob, NixMeta},
  };

  fn job(name: &str, drv_path: &str, input_drvs: &[&str]) -> NixJob {
    NixJob {
      name:         name.to_owned(),
      drv_path:     drv_path.to_owned(),
      system:       None,
      outputs:      None,
      input_drvs:   (!input_drvs.is_empty()).then(|| {
        input_drvs
          .iter()
          .map(|drv| ((*drv).to_owned(), serde_json::Value::Null))
          .collect()
      }),
      constituents: None,
      meta:         NixMeta::default(),
    }
  }

  #[test]
  fn aliased_jobs_get_unique_edges_on_their_own_builds() {
    let jobs = [
      job("packages.x86_64-linux.default", "/drv/pkg.drv", &[
        "/drv/dep.drv",
      ]),
      job("packages.x86_64-linux.pkg", "/drv/pkg.drv", &[
        "/drv/dep.drv",
      ]),
      job("drv:dep", "/drv/dep.drv", &[]),
    ];
    let build_ids = [Uuid::new_v4(), Uuid::new_v4(), Uuid::new_v4()];
    let drv_to_build = HashMap::from([
      ("/drv/pkg.drv".to_owned(), build_ids[1]),
      ("/drv/dep.drv".to_owned(), build_ids[2]),
    ]);
    let name_to_build = jobs
      .iter()
      .zip(build_ids)
      .map(|(job, id)| (job.name.clone(), id))
      .collect();

    let pairs = resolve_dependency_pairs(
      &jobs,
      &build_ids,
      &drv_to_build,
      &name_to_build,
    );

    assert_eq!(pairs, vec![
      (build_ids[0], build_ids[2]),
      (build_ids[1], build_ids[2]),
    ]);
  }

  fn drv(name: &str) -> String {
    format!("/nix/store/{}-{name}.drv", Uuid::new_v4().simple())
  }

  /// A running evaluation on a fresh database, or `None` without
  /// `TEST_DATABASE_URL`.
  async fn running_evaluation() -> Option<(PgPool, Uuid)> {
    let url = std::env::var("TEST_DATABASE_URL").ok()?;
    circus_migrations::run_migrations(&url)
      .await
      .expect("migration failed");
    let pool = circus_common::build_pool(&url, 5).expect("failed to connect");

    let project = repo::projects::create(&pool, CreateProject {
      name:            format!("stream-{}", Uuid::new_v4().simple()),
      description:     None,
      repository_url:  "https://github.com/test/stream".to_owned(),
      cache_enabled:   false,
      cache_url:       None,
      cache_upstreams: BinaryCacheUpstreams::default(),
    })
    .await
    .expect("create project");

    let row = pool
      .get()
      .await
      .expect("pool client")
      .query_one(
        "WITH jobset AS (
           INSERT INTO jobsets (project_id, name, nix_expression)
           VALUES ($1, 'stream', 'packages')
           RETURNING id
         )
         INSERT INTO evaluations (jobset_id, commit_hash, status)
         SELECT id, 'stream', 'running' FROM jobset
         RETURNING id",
        &[&project.id],
      )
      .await
      .expect("create evaluation");

    Some((pool, row.get(0)))
  }

  async fn open(pool: &PgPool, eval_id: Uuid) -> BuildStream<'_> {
    BuildStream::open(pool, eval_id, MemoryLimit::new(None), None)
      .await
      .expect("open stream")
  }

  async fn seed(
    pool: &PgPool,
    eval_id: Uuid,
    name: &str,
    drv_path: &str,
  ) -> Uuid {
    repo::builds::create(pool, CreateBuild {
      evaluation_id: eval_id,
      job_name: name.to_owned(),
      drv_path: drv_path.to_owned(),
      ..Default::default()
    })
    .await
    .expect("seed build")
    .id
  }

  #[tokio::test]
  async fn later_batches_depend_on_earlier_ones() {
    let Some((pool, eval_id)) = running_evaluation().await else {
      return;
    };
    let lib = drv("lib");
    let mut stream = open(&pool, eval_id).await;

    let first = stream.insert(vec![job("lib", &lib, &[])]).await;
    let lib_id = first.expect("insert").expect("running")[0].id;
    let second = stream
      .insert(vec![job("app", &drv("app"), &[&lib])])
      .await
      .expect("insert")
      .expect("running");

    let deps = repo::build_dependencies::list_for_build(&pool, second[0].id)
      .await
      .expect("list deps");
    assert_eq!(deps.len(), 1);
    assert_eq!(deps[0].dependency_build_id, lib_id);
  }

  #[tokio::test]
  async fn restart_keeps_only_builds_the_new_attempt_reaches() {
    let Some((pool, eval_id)) = running_evaluation().await else {
      return;
    };
    let lib = drv("lib");
    let kept = seed(&pool, eval_id, "lib", &lib).await;
    let kept_dep = seed(&pool, eval_id, "drv:dep", &drv("dep")).await;
    repo::build_dependencies::create(&pool, kept, kept_dep)
      .await
      .expect("seed edge");
    let stale = seed(&pool, eval_id, "app", &drv("app")).await;
    let dropped = seed(&pool, eval_id, "removed", &drv("removed")).await;
    let app = drv("app");
    let mut stream = open(&pool, eval_id).await;

    let created = stream
      .insert(vec![job("lib", &lib, &[]), job("app", &app, &[])])
      .await
      .expect("insert")
      .expect("running");
    assert_eq!(created.len(), 1);
    assert_eq!(created[0].drv_path, app);
    assert!(stream.complete().await.expect("complete"));

    let ids = repo::builds::list_for_evaluation(&pool, eval_id)
      .await
      .expect("list builds")
      .into_iter()
      .map(|build| build.id)
      .collect::<Vec<_>>();
    assert!(ids.contains(&kept));
    assert!(ids.contains(&kept_dep));
    assert!(!ids.contains(&stale));
    assert!(!ids.contains(&dropped));
  }

  #[tokio::test]
  async fn aggregates_wait_for_completion() {
    let Some((pool, eval_id)) = running_evaluation().await else {
      return;
    };
    let mut stream = open(&pool, eval_id).await;
    let aggregate = NixJob {
      constituents: Some(vec!["a".to_owned()]),
      ..job("all", &drv("all"), &[])
    };

    let created = stream
      .insert(vec![aggregate, job("a", &drv("a"), &[])])
      .await
      .expect("insert")
      .expect("running");
    assert_eq!(created.len(), 1);
    assert_eq!(created[0].job_name, "a");

    assert!(stream.complete().await.expect("complete"));

    let builds = repo::builds::list_for_evaluation(&pool, eval_id)
      .await
      .expect("list builds");
    let all = builds
      .iter()
      .find(|build| build.job_name == "all")
      .expect("aggregate created");
    let deps = repo::build_dependencies::list_for_build(&pool, all.id)
      .await
      .expect("list deps");
    assert_eq!(deps[0].dependency_build_id, created[0].id);

    let evaluation = repo::evaluations::get(&pool, eval_id)
      .await
      .expect("get evaluation");
    assert_eq!(evaluation.status, EvaluationStatus::Completed);
  }
}
