use std::{
  net::{IpAddr, Ipv4Addr, SocketAddr},
  path::PathBuf,
  process::Command,
};

use circus_common::{
  PgPool,
  models::{
    BinaryCacheUpstreams,
    BuildStatus,
    CreateBuild,
    CreateChannel,
    CreateEvaluation,
    CreateJobset,
    CreateNewsItem,
    CreateProject,
    CreateStarredJob,
    CreateUser,
    EvaluationStatus,
  },
  repo,
  roles::GlobalRole,
};
use circus_config::Config;
use color_eyre::eyre::{Result, WrapErr, bail};

const USERNAME: &str = "preview";
const PASSWORD: &str = "Preview-password-1";

/// A throwaway Postgres cluster under a temp dir, stopped and deleted on drop.
struct Cluster {
  dir: PathBuf,
}

impl Cluster {
  fn start() -> Result<Self> {
    let dir = std::env::temp_dir()
      .join(format!("circus-preview-{}", std::process::id()));
    let cluster = Self { dir };
    let data = cluster.dir.join("data");

    exec(
      Command::new("initdb")
        .args(["--auth=trust", "--username=circus", "--no-sync", "-D"])
        .arg(&data),
    )?;
    exec(
      Command::new("pg_ctl")
        .arg("-D")
        .arg(&data)
        .arg("-o")
        .arg(format!("-c listen_addresses= -k {}", cluster.dir.display()))
        .args(["-w", "-l"])
        .arg(cluster.dir.join("postgres.log"))
        .arg("start"),
    )?;
    exec(
      Command::new("createdb")
        .arg("-h")
        .arg(&cluster.dir)
        .args(["-U", "circus", "circus"]),
    )?;

    Ok(cluster)
  }

  fn url(&self) -> String {
    format!(
      "postgres:///circus?host={}&user=circus&sslmode=disable",
      self.dir.display()
    )
  }
}

impl Drop for Cluster {
  fn drop(&mut self) {
    let _stopped = Command::new("pg_ctl")
      .arg("-D")
      .arg(self.dir.join("data"))
      .args(["-m", "fast", "-w", "stop"])
      .output();
    let _removed = std::fs::remove_dir_all(&self.dir);
  }
}

fn exec(command: &mut Command) -> Result<()> {
  let program = command.get_program().to_string_lossy().into_owned();
  let output = command.output().wrap_err_with(|| {
    format!("{program} not found, run inside `nix develop`")
  })?;

  if !output.status.success() {
    bail!(
      "{program} failed: {}",
      String::from_utf8_lossy(&output.stderr).trim()
    );
  }

  Ok(())
}

pub async fn run(host: IpAddr, port: u16) -> Result<()> {
  #![expect(clippy::print_stdout, reason = "xtask CLI output is intentional")]
  circus_common::install_crypto_provider()?;

  let cluster = Cluster::start()?;
  let url = cluster.url();
  circus_migrations::run_migrations(&url).await?;
  let pool = circus_common::db::build_pool(&url, 4)?;
  seed(&pool).await?;

  let mut config = Config::default();
  config.database.url = url;
  config.ui.enabled = true;

  let addr = SocketAddr::new(host, port);
  println!(
    "serving Circus preview at http://{addr}/ as {USERNAME} / {PASSWORD}"
  );
  circus_server::cli::serve(config, &addr.to_string()).await
}

pub const fn default_host() -> IpAddr {
  IpAddr::V4(Ipv4Addr::LOCALHOST)
}

async fn seed(pool: &PgPool) -> Result<()> {
  let user = repo::users::create(
    pool,
    &CreateUser {
      username:  USERNAME.into(),
      email:     "preview@example.invalid".into(),
      full_name: Some("Preview Operator".into()),
      password:  PASSWORD.into(),
      role:      Some(GlobalRole::Admin),
    },
    None,
  )
  .await?;

  repo::news::create(pool, CreateNewsItem {
    title:      "Preview instance".into(),
    content:    "Seeded fixture data for frontend work.".into(),
    created_by: Some(user.id),
  })
  .await?;

  let project = repo::projects::create(pool, CreateProject {
    name:            "circus".into(),
    description:     Some("Nix-native CI control plane".into()),
    repository_url:  "https://github.com/manic-systems/circus".into(),
    cache_enabled:   true,
    cache_url:       None,
    cache_upstreams: BinaryCacheUpstreams::default(),
  })
  .await?;
  let jobset = repo::jobsets::create(pool, CreateJobset {
    project_id:        project.id,
    name:              "packages".into(),
    nix_expression:    "packages".into(),
    enabled:           Some(true),
    flake_mode:        Some(true),
    check_interval:    Some(300),
    trigger_mode:      None,
    branch:            None,
    branch_pattern:    None,
    tag_pattern:       None,
    scheduling_shares: None,
    state:             None,
    keep_nr:           None,
    systems:           None,
    only_build_latest: None,
    path_filters:      None,
  })
  .await?;

  repo::channels::create(pool, CreateChannel {
    project_id: project.id,
    name:       "stable".into(),
    jobset_id:  jobset.id,
  })
  .await?;
  repo::starred_jobs::create(pool, user.id, &CreateStarredJob {
    project_id: project.id,
    jobset_id:  Some(jobset.id),
    job_name:   "packages.x86_64-linux.circus-server".into(),
  })
  .await?;

  let finished =
    evaluation(pool, jobset.id, "3d12890fb4f2", EvaluationStatus::Completed)
      .await?;
  let jobs = [
    ("circus-server", "x86_64-linux", BuildStatus::Succeeded),
    ("circus-server", "aarch64-linux", BuildStatus::Succeeded),
    ("circus-agent", "x86_64-linux", BuildStatus::Succeeded),
    ("circus-agent", "aarch64-darwin", BuildStatus::Failed),
    ("docs", "x86_64-linux", BuildStatus::Succeeded),
  ];
  for (name, system, status) in jobs {
    let build = build(pool, finished, name, system).await?;
    repo::builds::start(pool, build).await?;
    let error =
      (status == BuildStatus::Failed).then_some("builder exited with code 1");
    repo::builds::complete(pool, build, status, None, None, error).await?;
  }

  let running =
    evaluation(pool, jobset.id, "b8bc61777a01", EvaluationStatus::Running)
      .await?;
  let active = build(pool, running, "circus-server", "x86_64-linux").await?;
  repo::builds::start(pool, active).await?;
  build(pool, running, "circus-agent", "x86_64-linux").await?;
  build(pool, running, "docs", "x86_64-linux").await?;

  Ok(())
}

async fn evaluation(
  pool: &PgPool,
  jobset_id: uuid::Uuid,
  commit: &str,
  status: EvaluationStatus,
) -> Result<uuid::Uuid> {
  let evaluation = repo::evaluations::create(pool, CreateEvaluation {
    jobset_id,
    commit_hash: format!("{commit:0<40}"),
    pr_number: None,
    pr_head_branch: None,
    pr_base_branch: None,
    pr_action: None,
  })
  .await?;
  repo::evaluations::update_status(pool, evaluation.id, status, None).await?;
  Ok(evaluation.id)
}

async fn build(
  pool: &PgPool,
  evaluation_id: uuid::Uuid,
  name: &str,
  system: &str,
) -> Result<uuid::Uuid> {
  let hash = format!("{:0>32}", uuid::Uuid::new_v4().simple());
  let out = format!("/nix/store/{}-{name}", &hash[..32]);
  let build = repo::builds::create(pool, CreateBuild {
    evaluation_id,
    job_name: format!("packages.{system}.{name}"),
    drv_path: format!("{out}.drv"),
    system: Some(system.into()),
    outputs: Some(serde_json::json!({ "out": out })),
    ..CreateBuild::default()
  })
  .await?;
  Ok(build.id)
}
