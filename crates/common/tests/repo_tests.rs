//! Integration tests for repository CRUD operations.
//! Requires `TEST_DATABASE_URL` to be set to a `PostgreSQL` connection string.
#![expect(
  clippy::unwrap_used,
  clippy::expect_used,
  clippy::print_stdout,
  reason = "Fine in tests"
)]

use std::time::Duration;

use circus_common::{
  ForgeType,
  GlobalRole,
  InputType,
  NotificationType,
  ProjectRole,
  models::*,
  repo,
};
use circus_config::{
  DeclarativeChannel,
  DeclarativeConfig,
  DeclarativeJobsetInput,
  DeclarativeNotification,
  DeclarativeProjectMember,
  DeclarativeRemoteBuilder,
  DeclarativeWebhook,
};

static REPO_TEST_LOCK: tokio::sync::Mutex<()> =
  tokio::sync::Mutex::const_new(());

struct TestPool {
  pool:   circus_common::PgPool,
  _guard: tokio::sync::MutexGuard<'static, ()>,
}

impl std::ops::Deref for TestPool {
  type Target = circus_common::PgPool;

  fn deref(&self) -> &Self::Target {
    &self.pool
  }
}

async fn get_pool() -> Option<TestPool> {
  get_pool_with_size(5).await
}

async fn get_pool_with_size(max_size: usize) -> Option<TestPool> {
  let guard = REPO_TEST_LOCK.lock().await;
  let Ok(url) = std::env::var("TEST_DATABASE_URL") else {
    println!("Skipping repo test: TEST_DATABASE_URL not set");
    return None;
  };
  let url = per_process_database_url(&url)?;

  // Run migrations
  circus_migrations::run_migrations(&url).await.ok()?;

  Some(TestPool {
    pool:   circus_common::db::build_pool(&url, max_size).ok()?,
    _guard: guard,
  })
}

fn per_process_database_url(url: &str) -> Option<String> {
  let mut parsed = url::Url::parse(url).ok()?;
  let dbname = parsed.path().trim_start_matches('/').to_owned();
  parsed.set_path(&format!("/{dbname}_p{}", std::process::id()));
  Some(parsed.to_string())
}

/// Helper: create a project with a unique name.
async fn create_test_project(
  pool: &circus_common::PgPool,
  prefix: &str,
) -> Project {
  repo::projects::create(pool, CreateProject {
    name:            format!("{prefix}-{}", uuid::Uuid::new_v4()),
    description:     Some("Test project".to_string()),
    repository_url:  "https://github.com/test/repo".to_string(),
    cache_enabled:   true,
    cache_url:       None,
    cache_upstreams: BinaryCacheUpstreams::default(),
  })
  .await
  .expect("create project")
}

/// Helper: create a jobset for a project.
async fn create_test_jobset(
  pool: &circus_common::PgPool,
  project_id: uuid::Uuid,
) -> Jobset {
  repo::jobsets::create(pool, CreateJobset {
    project_id,
    name: format!("default-{}", uuid::Uuid::new_v4()),
    nix_expression: "packages".to_string(),
    enabled: Some(true),
    flake_mode: None,
    check_interval: None,
    trigger_mode: None,
    branch: None,
    branch_pattern: None,
    tag_pattern: None,
    scheduling_shares: None,
    state: None,
    keep_nr: None,
    systems: None,
    only_build_latest: None,
    path_filters: None,
  })
  .await
  .expect("create jobset")
}

async fn enable_latest_only(
  pool: &circus_common::PgPool,
  jobset_id: uuid::Uuid,
) -> Jobset {
  repo::jobsets::update(pool, jobset_id, UpdateJobset {
    only_build_latest: Some(true),
    path_filters: None,
    ..Default::default()
  })
  .await
  .expect("enable latest-only")
}

/// Helper: create an evaluation for a jobset.
async fn create_test_eval(
  pool: &circus_common::PgPool,
  jobset_id: uuid::Uuid,
) -> Evaluation {
  repo::evaluations::create(pool, CreateEvaluation {
    jobset_id,
    commit_hash: format!("abc123{}", uuid::Uuid::new_v4().simple()),
    pr_number: None,
    pr_head_branch: None,
    pr_base_branch: None,
    pr_action: None,
  })
  .await
  .expect("create evaluation")
}

async fn enqueue_test_source(
  pool: &circus_common::PgPool,
  jobset_id: uuid::Uuid,
  source_scope: &str,
) -> Evaluation {
  repo::evaluations::enqueue_source(
    pool,
    source_evaluation(jobset_id),
    source_scope,
    repo::evaluations::SourceOrder::Unchecked,
  )
  .await
  .expect("enqueue source evaluation")
}

fn source_evaluation(jobset_id: uuid::Uuid) -> CreateEvaluation {
  CreateEvaluation {
    jobset_id,
    commit_hash: format!("source{}", uuid::Uuid::new_v4().simple()),
    pr_number: None,
    pr_head_branch: None,
    pr_base_branch: None,
    pr_action: None,
  }
}

/// Helper: create a build for an evaluation.
async fn create_test_build(
  pool: &circus_common::PgPool,
  eval_id: uuid::Uuid,
  job_name: &str,
  drv_path: &str,
  system: Option<&str>,
) -> Build {
  repo::builds::create(pool, CreateBuild {
    evaluation_id: eval_id,
    job_name: job_name.to_string(),
    drv_path: drv_path.to_string(),
    system: system.map(std::string::ToString::to_string),
    ..Default::default()
  })
  .await
  .expect("create build")
}

#[tokio::test]
async fn composite_operations_work_with_one_pool_connection() {
  let Some(pool) = get_pool_with_size(1).await else {
    return;
  };

  let (project_id, user_id, oauth_user_id) =
    tokio::time::timeout(Duration::from_secs(10), async {
      let project = create_test_project(&pool, "single-connection").await;
      let jobset = create_test_jobset(&pool, project.id).await;
      let evaluation = create_test_eval(&pool, jobset.id).await;

      let builder_name = format!("builder-{}", uuid::Uuid::new_v4());
      repo::remote_builders::sync_all(&pool, &[DeclarativeRemoteBuilder {
        name:               builder_name,
        ssh_uri:            "ssh://builder.example".to_string(),
        systems:            vec!["x86_64-linux".to_string()],
        max_jobs:           1,
        speed_factor:       1,
        supported_features: Vec::new(),
        mandatory_features: Vec::new(),
        ssh_key_file:       None,
        public_host_key:    None,
        enabled:            true,
      }])
      .await?;

      repo::jobset_inputs::sync_for_jobset(&pool, jobset.id, &[
        DeclarativeJobsetInput {
          name:       "message".to_string(),
          input_type: InputType::String,
          value:      "hello".to_string(),
          revision:   None,
        },
      ])
      .await?;

      repo::channels::sync_for_project(
        &pool,
        project.id,
        &[DeclarativeChannel {
          name:        "latest".to_string(),
          jobset_name: jobset.name.clone(),
        }],
        |name| (name == jobset.name).then_some(jobset.id),
      )
      .await?;

      repo::notification_configs::sync_for_project(&pool, project.id, &[
        DeclarativeNotification {
          notification_type: NotificationType::Email,
          config:            serde_json::json!({}),
          enabled:           true,
        },
      ])
      .await?;

      repo::webhook_configs::sync_for_project(
        &pool,
        project.id,
        &[DeclarativeWebhook {
          forge_type:  ForgeType::Github,
          secret:      None,
          secret_file: None,
          enabled:     true,
        }],
        |_| None,
        None,
      )
      .await?;

      let username =
        format!("single-{}", &uuid::Uuid::new_v4().simple().to_string()[..8]);
      let user = repo::users::create(
        &pool,
        &CreateUser {
          username:  username.clone(),
          email:     format!("{username}@example.com"),
          full_name: None,
          password:  "Secure_password_123".to_string(),
          role:      Some(GlobalRole::ReadOnly),
        },
        None,
      )
      .await?;
      repo::project_members::sync_for_project(
        &pool,
        project.id,
        &[DeclarativeProjectMember {
          username: username.clone(),
          role:     ProjectRole::Member,
        }],
        |candidate| (candidate == username).then_some(user.id),
      )
      .await?;

      let provider_id =
        uuid::Uuid::new_v4().simple().to_string()[..8].to_string();
      let oauth = repo::users::upsert_oauth_user(
        &pool,
        "oauth",
        Some(&format!("oauth-{provider_id}@example.com")),
        UserType::Github,
        &provider_id,
        None,
      )
      .await?;
      let updated_oauth = repo::users::upsert_oauth_user(
        &pool,
        "oauth",
        Some(&format!("updated-oauth-{provider_id}@example.com")),
        UserType::Github,
        &provider_id,
        None,
      )
      .await?;
      assert_eq!(updated_oauth.id, oauth.id);

      let completed = create_test_build(
        &pool,
        evaluation.id,
        "completed",
        &format!("/nix/store/{}.drv", uuid::Uuid::new_v4().simple()),
        None,
      )
      .await;
      repo::builds::complete(
        &pool,
        completed.id,
        BuildStatus::Succeeded,
        None,
        None,
        None,
      )
      .await?;
      repo::channels::auto_promote_if_complete(&pool, jobset.id, evaluation.id)
        .await?;

      let channels =
        repo::channels::list_for_project(&pool, project.id).await?;
      assert_eq!(channels[0].current_evaluation_id, Some(evaluation.id));

      repo::builds::restart(&pool, completed.id).await?;

      Ok::<_, circus_common::CiError>((project.id, user.id, oauth.id))
    })
    .await
    .expect("composite operation deadlocked")
    .expect("composite operation failed");

  repo::projects::delete(&pool, project_id)
    .await
    .expect("delete project");
  repo::users::delete(&pool, user_id)
    .await
    .expect("delete user");
  repo::users::delete(&pool, oauth_user_id)
    .await
    .expect("delete OAuth user");
}

#[tokio::test]
async fn runtime_enum_values_are_accepted_by_database_constraints() {
  let Some(pool) = get_pool().await else {
    return;
  };

  let project = create_test_project(&pool, "runtime-enums").await;
  let jobset = create_test_jobset(&pool, project.id).await;
  let evaluation = create_test_eval(&pool, jobset.id).await;
  let build = create_test_build(
    &pool,
    evaluation.id,
    "oom",
    &format!("/nix/store/{}.drv", uuid::Uuid::new_v4().simple()),
    None,
  )
  .await;

  let build = repo::builds::complete(
    &pool,
    build.id,
    BuildStatus::OomKilled,
    None,
    None,
    Some("out of memory"),
  )
  .await
  .expect("persist OOM status");
  assert_eq!(build.status, BuildStatus::OomKilled);

  for notification_type in
    [NotificationType::ForgejoStatus, NotificationType::Slack]
  {
    let task = repo::notification_tasks::create(
      &pool,
      notification_type,
      serde_json::json!({}),
      1,
    )
    .await
    .expect("persist notification task");
    assert_eq!(task.notification_type, notification_type);
  }

  repo::projects::delete(&pool, project.id)
    .await
    .expect("delete project");
}

// CRUD and lifecycle tests

#[tokio::test]
async fn test_project_crud() {
  let Some(pool) = get_pool().await else {
    return;
  };

  // Create
  let project = create_test_project(&pool, "crud").await;
  assert!(!project.name.is_empty());
  assert_eq!(project.description.as_deref(), Some("Test project"));

  // Get
  let fetched = repo::projects::get(&pool, project.id)
    .await
    .expect("get project");
  assert_eq!(fetched.name, project.name);

  // Get by name
  let by_name = repo::projects::get_by_name(&pool, &project.name)
    .await
    .expect("get by name");
  assert_eq!(by_name.id, project.id);

  // Update
  let updated = repo::projects::update(&pool, project.id, UpdateProject {
    name:            None,
    description:     Some("Updated description".to_string()),
    repository_url:  None,
    cache_enabled:   None,
    cache_url:       None,
    cache_upstreams: None,
  })
  .await
  .expect("update project");
  assert_eq!(updated.description.as_deref(), Some("Updated description"));

  // List
  let projects = repo::projects::list(&pool, 100, 0)
    .await
    .expect("list projects");
  assert!(projects.iter().any(|p| p.id == project.id));

  // Delete
  repo::projects::delete(&pool, project.id)
    .await
    .expect("delete project");

  // Verify deleted
  let result = repo::projects::get(&pool, project.id).await;
  assert!(result.is_err());
}

#[tokio::test]
async fn test_project_unique_constraint() {
  let Some(pool) = get_pool().await else {
    return;
  };

  let name = format!("unique-test-{}", uuid::Uuid::new_v4());

  let _project = repo::projects::create(&pool, CreateProject {
    name:            name.clone(),
    description:     None,
    repository_url:  "https://github.com/test/repo".to_string(),
    cache_enabled:   true,
    cache_url:       None,
    cache_upstreams: BinaryCacheUpstreams::default(),
  })
  .await
  .expect("create first project");

  // Creating with same name should fail with Conflict
  let result = repo::projects::create(&pool, CreateProject {
    name,
    description: None,
    repository_url: "https://github.com/test/repo2".to_string(),
    cache_enabled: true,
    cache_url: None,
    cache_upstreams: BinaryCacheUpstreams::default(),
  })
  .await;

  assert!(matches!(result, Err(circus_common::CiError::Conflict(_))));
}

#[tokio::test]
async fn test_jobset_crud() {
  let Some(pool) = get_pool().await else {
    return;
  };

  let project = create_test_project(&pool, "jobset").await;

  // Create jobset
  let jobset = repo::jobsets::create(&pool, CreateJobset {
    project_id:        project.id,
    name:              "default".to_string(),
    nix_expression:    "packages".to_string(),
    enabled:           Some(true),
    flake_mode:        None,
    check_interval:    None,
    trigger_mode:      None,
    branch:            None,
    branch_pattern:    None,
    tag_pattern:       None,
    scheduling_shares: None,
    state:             None,
    keep_nr:           None,
    systems:           None,
    only_build_latest: None,
    path_filters:      Some(vec!["packages/**".to_string()]),
  })
  .await
  .expect("create jobset");

  assert_eq!(jobset.name, "default");
  assert!(jobset.enabled);
  assert_eq!(jobset.path_filters, ["packages/**"]);

  // Get
  let fetched = repo::jobsets::get(&pool, jobset.id)
    .await
    .expect("get jobset");
  assert_eq!(fetched.project_id, project.id);

  // List for project
  let jobsets = repo::jobsets::list_for_project(&pool, project.id, 100, 0)
    .await
    .expect("list jobsets");
  assert_eq!(jobsets.len(), 1);

  // Update
  let updated = repo::jobsets::update(&pool, jobset.id, UpdateJobset {
    name:              None,
    nix_expression:    Some("checks".to_string()),
    enabled:           Some(false),
    flake_mode:        None,
    check_interval:    None,
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
  .await
  .expect("update jobset");
  assert_eq!(updated.nix_expression, "checks");
  assert!(!updated.enabled);
  assert_eq!(updated.path_filters, ["packages/**"]);

  // Delete
  repo::jobsets::delete(&pool, jobset.id)
    .await
    .expect("delete jobset");

  // Cleanup
  let _ = repo::projects::delete(&pool, project.id).await;
}

#[tokio::test]
async fn test_interval_evaluations_can_repeat_commit() {
  let Some(pool) = get_pool().await else {
    return;
  };

  let project = create_test_project(&pool, "interval-eval").await;
  let jobset = repo::jobsets::create(&pool, CreateJobset {
    project_id:        project.id,
    name:              "interval".to_string(),
    nix_expression:    "packages".to_string(),
    enabled:           Some(true),
    flake_mode:        None,
    check_interval:    Some(60),
    trigger_mode:      Some(JobsetTriggerMode::Interval),
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
  .await
  .expect("create interval jobset");

  let commit_hash = format!("abc123{}", uuid::Uuid::new_v4().simple());
  let first = repo::evaluations::create_interval(&pool, CreateEvaluation {
    jobset_id:      jobset.id,
    commit_hash:    commit_hash.clone(),
    pr_number:      None,
    pr_head_branch: None,
    pr_base_branch: None,
    pr_action:      None,
  })
  .await
  .expect("create first interval evaluation");
  let second = repo::evaluations::create_interval(&pool, CreateEvaluation {
    jobset_id:      jobset.id,
    commit_hash:    commit_hash.clone(),
    pr_number:      None,
    pr_head_branch: None,
    pr_base_branch: None,
    pr_action:      None,
  })
  .await
  .expect("create second interval evaluation");

  assert_ne!(first.id, second.id);
  assert_eq!(first.trigger_kind, EvaluationTriggerKind::Interval);
  assert_eq!(second.trigger_kind, EvaluationTriggerKind::Interval);
  assert_eq!(first.status, EvaluationStatus::Running);
  assert_eq!(second.status, EvaluationStatus::Running);

  let source_commit = commit_hash.clone();
  let source_result = repo::evaluations::create(&pool, CreateEvaluation {
    jobset_id:      jobset.id,
    commit_hash:    source_commit.clone(),
    pr_number:      None,
    pr_head_branch: None,
    pr_base_branch: None,
    pr_action:      None,
  })
  .await;
  assert!(source_result.is_ok());

  let manual_duplicate =
    repo::evaluations::create_manual(&pool, CreateEvaluation {
      jobset_id:      jobset.id,
      commit_hash:    source_commit,
      pr_number:      None,
      pr_head_branch: None,
      pr_base_branch: None,
      pr_action:      None,
    })
    .await;
  assert!(matches!(
    manual_duplicate,
    Err(circus_common::CiError::Conflict(_))
  ));
}

#[tokio::test]
async fn test_source_evaluations_keep_every_revision_by_default() {
  let Some(pool) = get_pool().await else {
    return;
  };

  let project = create_test_project(&pool, "keep-source-revisions").await;
  let jobset = create_test_jobset(&pool, project.id).await;
  let old = enqueue_test_source(&pool, jobset.id, "tags").await;
  let new = enqueue_test_source(&pool, jobset.id, "tags").await;

  assert_eq!(
    repo::evaluations::get(&pool, old.id).await.unwrap().status,
    EvaluationStatus::Pending
  );
  assert_eq!(new.status, EvaluationStatus::Pending);
  assert_eq!(
    new.source_base_commit.as_deref(),
    Some(old.commit_hash.as_str())
  );
  let _ = repo::projects::delete(&pool, project.id).await;
}

#[tokio::test]
async fn test_latest_only_cancels_active_work_but_keeps_successes() {
  let Some(pool) = get_pool().await else {
    return;
  };

  let project = create_test_project(&pool, "latest-source-revision").await;
  let jobset = create_test_jobset(&pool, project.id).await;
  enable_latest_only(&pool, jobset.id).await;
  let old = enqueue_test_source(&pool, jobset.id, "tags").await;
  repo::evaluations::update_status(
    &pool,
    old.id,
    EvaluationStatus::Running,
    None,
  )
  .await
  .expect("start old evaluation");
  let cancelled_build = create_test_build(
    &pool,
    old.id,
    "pending",
    &format!("/nix/store/{}-pending.drv", uuid::Uuid::new_v4().simple()),
    None,
  )
  .await;
  let successful_build = create_test_build(
    &pool,
    old.id,
    "succeeded",
    &format!("/nix/store/{}-succeeded.drv", uuid::Uuid::new_v4().simple()),
    None,
  )
  .await;
  repo::builds::complete(
    &pool,
    successful_build.id,
    BuildStatus::Succeeded,
    None,
    Some("/nix/store/succeeded"),
    None,
  )
  .await
  .expect("complete successful build");

  let replacement = enqueue_test_source(&pool, jobset.id, "tags").await;

  let old = repo::evaluations::get(&pool, old.id).await.unwrap();
  assert_eq!(old.status, EvaluationStatus::Cancelled);
  assert_eq!(old.superseded_by, Some(replacement.id));
  assert_eq!(
    repo::builds::get(&pool, cancelled_build.id)
      .await
      .unwrap()
      .status,
    BuildStatus::Cancelled
  );
  assert_eq!(
    repo::builds::get(&pool, successful_build.id)
      .await
      .unwrap()
      .status,
    BuildStatus::Succeeded
  );
  let _ = repo::projects::delete(&pool, project.id).await;
}

#[tokio::test]
async fn test_latest_only_is_scoped_and_protects_manual_evaluations() {
  let Some(pool) = get_pool().await else {
    return;
  };

  let project = create_test_project(&pool, "latest-source-scope").await;
  let jobset = create_test_jobset(&pool, project.id).await;
  enable_latest_only(&pool, jobset.id).await;
  let legacy = create_test_eval(&pool, jobset.id).await;
  let main = enqueue_test_source(&pool, jobset.id, "branch:main").await;
  let release = enqueue_test_source(&pool, jobset.id, "branch:release").await;
  let manual = repo::evaluations::create_manual(&pool, CreateEvaluation {
    jobset_id:      jobset.id,
    commit_hash:    format!("manual{}", uuid::Uuid::new_v4().simple()),
    pr_number:      None,
    pr_head_branch: None,
    pr_base_branch: None,
    pr_action:      None,
  })
  .await
  .expect("enqueue manual evaluation");

  enqueue_test_source(&pool, jobset.id, "branch:main").await;

  assert_eq!(
    repo::evaluations::get(&pool, main.id).await.unwrap().status,
    EvaluationStatus::Cancelled
  );
  assert_eq!(
    repo::evaluations::get(&pool, release.id)
      .await
      .unwrap()
      .status,
    EvaluationStatus::Pending
  );
  assert_eq!(
    repo::evaluations::get(&pool, manual.id)
      .await
      .unwrap()
      .status,
    EvaluationStatus::Pending
  );
  assert_eq!(
    repo::evaluations::get(&pool, legacy.id)
      .await
      .unwrap()
      .status,
    EvaluationStatus::Pending
  );
  let _ = repo::projects::delete(&pool, project.id).await;
}

#[tokio::test]
async fn test_latest_only_rejects_out_of_order_source_updates() {
  let Some(pool) = get_pool().await else {
    return;
  };

  let project = create_test_project(&pool, "ordered-source-update").await;
  let jobset = create_test_jobset(&pool, project.id).await;
  let current = enqueue_test_source(&pool, jobset.id, "branch:main").await;
  enable_latest_only(&pool, jobset.id).await;

  let stale = repo::evaluations::enqueue_source(
    &pool,
    source_evaluation(jobset.id),
    "branch:main",
    repo::evaluations::SourceOrder::After("older-than-current"),
  )
  .await;

  assert!(matches!(stale, Err(circus_common::CiError::Conflict(_))));
  assert_eq!(
    repo::evaluations::get(&pool, current.id)
      .await
      .unwrap()
      .status,
    EvaluationStatus::Pending
  );
  let _ = repo::projects::delete(&pool, project.id).await;
}

#[tokio::test]
async fn test_hidden_evaluations_are_filtered_from_default_lists() {
  let Some(pool) = get_pool().await else {
    return;
  };

  let project = create_test_project(&pool, "hidden-eval").await;
  let jobset = create_test_jobset(&pool, project.id).await;
  let eval = create_test_eval(&pool, jobset.id).await;

  repo::evaluations::set_hidden(&pool, eval.id, true)
    .await
    .expect("hide evaluation");

  let visible =
    repo::evaluations::list_filtered(&pool, Some(jobset.id), None, 10, 0)
      .await
      .expect("list visible evaluations");
  assert!(!visible.iter().any(|e| e.id == eval.id));

  let with_hidden = repo::evaluations::list_filtered_with_visibility(
    &pool,
    Some(jobset.id),
    None,
    10,
    0,
    true,
  )
  .await
  .expect("list evaluations including hidden");
  assert!(with_hidden.iter().any(|e| e.id == eval.id && e.hidden));

  assert!(
    repo::evaluations::get_visible(&pool, eval.id, false)
      .await
      .is_err()
  );
  assert!(
    repo::evaluations::get_visible(&pool, eval.id, true)
      .await
      .is_ok()
  );

  let _ = repo::projects::delete(&pool, project.id).await;
}

#[tokio::test]
async fn test_evaluation_and_build_lifecycle() {
  let Some(pool) = get_pool().await else {
    return;
  };

  // Set up project and jobset
  let project = create_test_project(&pool, "eval").await;
  let jobset = create_test_jobset(&pool, project.id).await;

  // Create evaluation
  let eval = repo::evaluations::create(&pool, CreateEvaluation {
    jobset_id:      jobset.id,
    commit_hash:    "abc123def456".to_string(),
    pr_number:      None,
    pr_head_branch: None,
    pr_base_branch: None,
    pr_action:      None,
  })
  .await
  .expect("create evaluation");

  assert_eq!(eval.commit_hash, "abc123def456");

  // Update status
  let updated = repo::evaluations::update_status(
    &pool,
    eval.id,
    EvaluationStatus::Running,
    None,
  )
  .await
  .expect("update evaluation status");
  assert!(matches!(updated.status, EvaluationStatus::Running));

  // get_latest only reports completed evaluations
  let completed = repo::evaluations::update_status(
    &pool,
    eval.id,
    EvaluationStatus::Completed,
    None,
  )
  .await
  .expect("complete evaluation");
  assert!(matches!(completed.status, EvaluationStatus::Completed));

  let latest = repo::evaluations::get_latest(&pool, jobset.id)
    .await
    .expect("get latest");
  assert!(latest.is_some());
  assert_eq!(latest.unwrap().id, eval.id);

  // Create build
  let build = create_test_build(
    &pool,
    eval.id,
    "hello",
    "/nix/store/abc.drv",
    Some("x86_64-linux"),
  )
  .await;
  assert_eq!(build.job_name, "hello");
  assert_eq!(build.system.as_deref(), Some("x86_64-linux"));

  // List pending
  let pending = repo::builds::list_pending(&pool, 10, 4)
    .await
    .expect("list pending");
  assert!(pending.iter().any(|b| b.id == build.id));

  // Start build
  let started = repo::builds::start(&pool, build.id)
    .await
    .expect("start build");
  assert!(started.is_some());

  // Second start should return None (already claimed)
  let second = repo::builds::start(&pool, build.id)
    .await
    .expect("second start");
  assert!(second.is_none());

  // Complete build
  let completed = repo::builds::complete(
    &pool,
    build.id,
    BuildStatus::Succeeded,
    None,
    Some("/nix/store/output"),
    None,
  )
  .await
  .expect("complete build");
  assert!(matches!(completed.status, BuildStatus::Succeeded));

  // Create build step
  let step = repo::build_steps::create(&pool, CreateBuildStep {
    build_id:    build.id,
    step_number: 1,
    command:     "nix build".to_string(),
  })
  .await
  .expect("create build step");

  // Complete build step
  let completed_step =
    repo::build_steps::complete(&pool, step.id, 0, Some("output"), None)
      .await
      .expect("complete build step");
  assert_eq!(completed_step.exit_code, Some(0));

  // Create build product
  let product = repo::build_products::create(&pool, CreateBuildProduct {
    build_id:     build.id,
    name:         "hello".to_string(),
    path:         "/nix/store/output".to_string(),
    sha256_hash:  Some("sha256-abc".to_string()),
    file_size:    Some(1024),
    content_type: None,
    is_directory: true,
  })
  .await
  .expect("create build product");
  assert_eq!(product.file_size, Some(1024));

  // List build products
  let products = repo::build_products::list_for_build(&pool, build.id)
    .await
    .expect("list products");
  assert_eq!(products.len(), 1);

  // List build steps
  let steps = repo::build_steps::list_for_build(&pool, build.id)
    .await
    .expect("list steps");
  assert_eq!(steps.len(), 1);

  // Test filtered list
  let filtered =
    repo::builds::list_filtered(&pool, Some(eval.id), None, None, None, 50, 0)
      .await
      .expect("list filtered");
  assert!(filtered.iter().any(|b| b.id == build.id));

  // Get stats
  let stats = repo::builds::get_stats(&pool).await.expect("get stats");
  assert!(stats.total_builds.unwrap_or(0) > 0);

  // List recent
  let recent = repo::builds::list_recent(&pool, 10)
    .await
    .expect("list recent");
  assert!(!recent.is_empty());

  // Cleanup
  let _ = repo::projects::delete(&pool, project.id).await;
}

#[tokio::test]
async fn test_restarting_one_shot_evaluation_resets_its_attempt() {
  let Some(pool) = get_pool().await else {
    return;
  };

  let project = create_test_project(&pool, "restart-one-shot").await;
  let jobset = repo::jobsets::create(&pool, CreateJobset {
    project_id:        project.id,
    name:              format!("one-shot-{}", uuid::Uuid::new_v4()),
    nix_expression:    "packages".to_string(),
    enabled:           Some(true),
    flake_mode:        None,
    check_interval:    None,
    trigger_mode:      None,
    branch:            None,
    branch_pattern:    None,
    tag_pattern:       None,
    scheduling_shares: None,
    state:             Some(JobsetState::OneShot),
    keep_nr:           None,
    systems:           None,
    only_build_latest: None,
    path_filters:      None,
  })
  .await
  .expect("create one-shot jobset");
  let eval = create_test_eval(&pool, jobset.id).await;
  repo::evaluations::update_status(
    &pool,
    eval.id,
    EvaluationStatus::Failed,
    Some("failed"),
  )
  .await
  .expect("fail evaluation");
  let client = pool.get().await.expect("get backdating client");
  client
    .execute(
      "UPDATE evaluations SET evaluation_time = NOW() - INTERVAL '1 hour' \
       WHERE id = $1",
      &[&eval.id],
    )
    .await
    .expect("backdate evaluation");
  repo::jobsets::mark_one_shot_complete(&pool, jobset.id)
    .await
    .expect("complete one-shot");

  let restarted = repo::evaluations::restart(&pool, eval.id)
    .await
    .expect("restart evaluation")
    .expect("failed evaluation can restart");
  let restarted_jobset = repo::jobsets::get(&pool, jobset.id)
    .await
    .expect("get restarted jobset");

  assert_eq!(restarted.status, EvaluationStatus::Pending);
  assert_eq!(restarted.trigger_kind, EvaluationTriggerKind::Manual);
  assert!(restarted.source_scope.is_none());
  assert!(restarted.evaluation_time > eval.evaluation_time);
  assert!(restarted_jobset.enabled);
  assert_eq!(restarted_jobset.state, JobsetState::OneShot);
}

#[tokio::test]
async fn test_restart_rejects_a_manually_disabled_jobset() {
  let Some(pool) = get_pool().await else {
    return;
  };

  let project = create_test_project(&pool, "restart-disabled").await;
  let jobset = create_test_jobset(&pool, project.id).await;
  let eval = create_test_eval(&pool, jobset.id).await;
  repo::evaluations::update_status(
    &pool,
    eval.id,
    EvaluationStatus::Failed,
    Some("failed"),
  )
  .await
  .expect("fail evaluation");
  let client = pool.get().await.expect("get disabling client");
  client
    .execute("UPDATE jobsets SET enabled = false WHERE id = $1", &[
      &jobset.id,
    ])
    .await
    .expect("disable jobset");

  assert!(
    repo::evaluations::restart(&pool, eval.id)
      .await
      .expect("attempt restart")
      .is_none()
  );
  assert_eq!(
    repo::evaluations::get(&pool, eval.id)
      .await
      .expect("get failed evaluation")
      .status,
    EvaluationStatus::Failed
  );
}

#[tokio::test]
async fn test_cancellation_cannot_interleave_build_persistence() {
  let Some(pool) = get_pool().await else {
    return;
  };

  let project = create_test_project(&pool, "cancel-persistence").await;
  let jobset = create_test_jobset(&pool, project.id).await;
  let eval = create_test_eval(&pool, jobset.id).await;
  repo::evaluations::update_status(
    &pool,
    eval.id,
    EvaluationStatus::Running,
    None,
  )
  .await
  .expect("start evaluation");

  let mut tx_client = pool.get().await.expect("get result client");
  let tx = tx_client
    .transaction()
    .await
    .expect("begin result transaction");
  assert!(
    repo::evaluations::lock_running(&tx, eval.id)
      .await
      .expect("lock evaluation")
  );
  let cancel_pool = pool.clone();
  let mut cancel = tokio::spawn(async move {
    repo::evaluations::cancel(&cancel_pool, eval.id).await
  });
  assert!(
    tokio::time::timeout(std::time::Duration::from_millis(25), &mut cancel)
      .await
      .is_err(),
    "cancellation must wait for the result transaction"
  );
  repo::builds::create_in_transaction(&tx, CreateBuild {
    evaluation_id: eval.id,
    job_name: "build".to_string(),
    drv_path: format!("/nix/store/{}.drv", uuid::Uuid::new_v4()),
    ..Default::default()
  })
  .await
  .expect("persist build");
  assert!(
    repo::evaluations::finish_running_in_transaction(
      &tx,
      eval.id,
      EvaluationStatus::Completed,
      None,
    )
    .await
    .expect("finish evaluation")
  );
  tx.commit().await.expect("commit result transaction");

  assert!(
    cancel
      .await
      .expect("join cancellation")
      .expect("cancel evaluation")
      .is_none()
  );
  assert_eq!(
    repo::builds::count_filtered(&pool, Some(eval.id), None, None, None)
      .await
      .expect("count persisted builds"),
    1
  );
  assert_eq!(
    repo::evaluations::get(&pool, eval.id)
      .await
      .expect("get completed evaluation")
      .status,
    EvaluationStatus::Completed
  );

  let cancelled = create_test_eval(&pool, jobset.id).await;
  repo::evaluations::update_status(
    &pool,
    cancelled.id,
    EvaluationStatus::Running,
    None,
  )
  .await
  .expect("start cancelled evaluation");
  assert!(
    repo::evaluations::cancel(&pool, cancelled.id)
      .await
      .expect("cancel before persistence")
      .is_some()
  );
  let mut cancelled_client = pool.get().await.expect("get cancelled client");
  let cancelled_tx = cancelled_client
    .transaction()
    .await
    .expect("begin cancelled transaction");
  assert!(
    !repo::evaluations::lock_running(&cancelled_tx, cancelled.id)
      .await
      .expect("check cancelled evaluation")
  );
  assert_eq!(
    repo::builds::count_filtered(&pool, Some(cancelled.id), None, None, None)
      .await
      .expect("count cancelled builds"),
    0
  );

  // Otherwise, this pending build leaks into sibling tests' `list_pending`
  // assertions.
  let _ = repo::projects::delete(&pool, project.id).await;
}

#[tokio::test]
async fn test_start_blocks_duplicate_running_drv_path() {
  let Some(pool) = get_pool().await else {
    return;
  };

  let project = create_test_project(&pool, "duplicate-drv").await;
  let jobset = create_test_jobset(&pool, project.id).await;
  let eval = create_test_eval(&pool, jobset.id).await;
  let drv = format!("/nix/store/{}.drv", uuid::Uuid::new_v4().simple());

  let first =
    create_test_build(&pool, eval.id, "first", &drv, Some("x86_64-linux"))
      .await;
  let second =
    create_test_build(&pool, eval.id, "second", &drv, Some("x86_64-linux"))
      .await;

  let claimed_first = repo::builds::start(&pool, first.id)
    .await
    .expect("start first build");
  assert!(claimed_first.is_some());

  let pending = repo::builds::list_pending(&pool, 10, 4)
    .await
    .expect("list pending");
  assert!(!pending.iter().any(|build| build.id == second.id));

  let claimed_second = repo::builds::start(&pool, second.id)
    .await
    .expect("start second build");
  assert!(claimed_second.is_none());

  let second = repo::builds::get(&pool, second.id)
    .await
    .expect("reload second build");
  assert!(matches!(second.status, BuildStatus::Pending));

  repo::builds::complete(
    &pool,
    first.id,
    BuildStatus::Succeeded,
    None,
    None,
    None,
  )
  .await
  .expect("complete first build");

  let claimed_second = repo::builds::start(&pool, second.id)
    .await
    .expect("start second build after first completed");
  assert!(claimed_second.is_some());

  // Cleanup
  let _ = repo::projects::delete(&pool, project.id).await;
}

#[tokio::test]
async fn test_not_found_errors() {
  let Some(pool) = get_pool().await else {
    return;
  };

  let fake_id = uuid::Uuid::new_v4();

  assert!(matches!(
    repo::projects::get(&pool, fake_id).await,
    Err(circus_common::CiError::NotFound(_))
  ));

  assert!(matches!(
    repo::jobsets::get(&pool, fake_id).await,
    Err(circus_common::CiError::NotFound(_))
  ));

  assert!(matches!(
    repo::evaluations::get(&pool, fake_id).await,
    Err(circus_common::CiError::NotFound(_))
  ));

  assert!(matches!(
    repo::builds::get(&pool, fake_id).await,
    Err(circus_common::CiError::NotFound(_))
  ));
}

// Batch operations and edge cases

#[tokio::test]
async fn test_batch_get_completed_by_drv_paths() {
  let Some(pool) = get_pool().await else {
    return;
  };

  let project = create_test_project(&pool, "batch-drv").await;
  let jobset = create_test_jobset(&pool, project.id).await;
  let eval = create_test_eval(&pool, jobset.id).await;

  let drv1 = format!("/nix/store/{}.drv", uuid::Uuid::new_v4().simple());
  let drv2 = format!("/nix/store/{}.drv", uuid::Uuid::new_v4().simple());
  let drv_missing = format!("/nix/store/{}.drv", uuid::Uuid::new_v4().simple());

  let b1 =
    create_test_build(&pool, eval.id, "pkg1", &drv1, Some("x86_64-linux"))
      .await;
  let b2 =
    create_test_build(&pool, eval.id, "pkg2", &drv2, Some("x86_64-linux"))
      .await;

  // Start and complete both
  repo::builds::start(&pool, b1.id).await.unwrap();
  repo::builds::complete(
    &pool,
    b1.id,
    BuildStatus::Succeeded,
    None,
    None,
    None,
  )
  .await
  .unwrap();
  repo::builds::start(&pool, b2.id).await.unwrap();
  repo::builds::complete(
    &pool,
    b2.id,
    BuildStatus::Succeeded,
    None,
    None,
    None,
  )
  .await
  .unwrap();

  // Batch query
  let results = repo::builds::get_completed_by_drv_paths(&pool, &[
    drv1.clone(),
    drv2.clone(),
    drv_missing.clone(),
  ])
  .await
  .expect("batch get");

  assert!(results.contains_key(&drv1));
  assert!(results.contains_key(&drv2));
  assert!(!results.contains_key(&drv_missing));
  assert_eq!(results.len(), 2);

  // Empty input
  let empty = repo::builds::get_completed_by_drv_paths(&pool, &[])
    .await
    .expect("empty batch");
  assert!(empty.is_empty());

  // Cleanup
  let _ = repo::projects::delete(&pool, project.id).await;
}

#[tokio::test]
async fn test_batch_check_deps_for_builds() {
  let Some(pool) = get_pool().await else {
    return;
  };

  let project = create_test_project(&pool, "batch-deps").await;
  let jobset = create_test_jobset(&pool, project.id).await;
  let eval = create_test_eval(&pool, jobset.id).await;

  // Create dep (will be completed) and dependent (pending)
  let dep_drv = format!("/nix/store/{}.drv", uuid::Uuid::new_v4().simple());
  let main_drv = format!("/nix/store/{}.drv", uuid::Uuid::new_v4().simple());
  let standalone_drv =
    format!("/nix/store/{}.drv", uuid::Uuid::new_v4().simple());

  let dep_build =
    create_test_build(&pool, eval.id, "dep", &dep_drv, None).await;
  let main_build =
    create_test_build(&pool, eval.id, "main", &main_drv, None).await;
  let standalone =
    create_test_build(&pool, eval.id, "standalone", &standalone_drv, None)
      .await;

  // Create dependency: main depends on dep
  repo::build_dependencies::create(&pool, main_build.id, dep_build.id)
    .await
    .expect("create dep");

  // Before dep is completed, main should have incomplete deps
  let results = repo::build_dependencies::check_deps_for_builds(&pool, &[
    main_build.id,
    standalone.id,
  ])
  .await
  .expect("batch check deps");

  assert!(!results[&main_build.id]); // dep not completed
  assert!(results[&standalone.id]); // no deps

  // Now complete the dep
  repo::builds::start(&pool, dep_build.id).await.unwrap();
  repo::builds::complete(
    &pool,
    dep_build.id,
    BuildStatus::Succeeded,
    None,
    None,
    None,
  )
  .await
  .unwrap();

  // Recheck
  let results = repo::build_dependencies::check_deps_for_builds(&pool, &[
    main_build.id,
    standalone.id,
  ])
  .await
  .expect("batch check deps after complete");

  assert!(results[&main_build.id]); // dep now completed
  assert!(results[&standalone.id]);

  // Empty input
  let empty = repo::build_dependencies::check_deps_for_builds(&pool, &[])
    .await
    .expect("empty check");
  assert!(empty.is_empty());

  // Cleanup
  let _ = repo::projects::delete(&pool, project.id).await;
}

#[tokio::test]
async fn test_list_pending_prioritizes_dependency_ready_builds() {
  let Some(pool) = get_pool().await else {
    return;
  };

  let project = create_test_project(&pool, "ready-deps").await;
  let jobset = create_test_jobset(&pool, project.id).await;
  let eval = create_test_eval(&pool, jobset.id).await;

  let parent_drv = format!("/nix/store/{}.drv", uuid::Uuid::new_v4().simple());
  let dependency_drv =
    format!("/nix/store/{}.drv", uuid::Uuid::new_v4().simple());

  let parent =
    create_test_build(&pool, eval.id, "parent", &parent_drv, None).await;
  let dependency =
    create_test_build(&pool, eval.id, "dependency", &dependency_drv, None)
      .await;
  repo::build_dependencies::create(&pool, parent.id, dependency.id)
    .await
    .expect("create dependency edge");
  let bumped = repo::builds::bump_priority(&pool, parent.id, 1)
    .await
    .expect("bump blocked parent priority");
  assert!(bumped.is_some());

  let pending = repo::builds::list_pending(&pool, 1, 1)
    .await
    .expect("list pending");

  assert_eq!(pending.len(), 1);
  assert_eq!(pending[0].id, dependency.id);

  let _ = repo::projects::delete(&pool, project.id).await;
}

#[tokio::test]
async fn test_list_filtered_with_system_filter() {
  let Some(pool) = get_pool().await else {
    return;
  };

  let project = create_test_project(&pool, "filter-sys").await;
  let jobset = create_test_jobset(&pool, project.id).await;
  let eval = create_test_eval(&pool, jobset.id).await;

  let drv_x86 = format!("/nix/store/{}.drv", uuid::Uuid::new_v4().simple());
  let drv_arm = format!("/nix/store/{}.drv", uuid::Uuid::new_v4().simple());

  create_test_build(&pool, eval.id, "x86-pkg", &drv_x86, Some("x86_64-linux"))
    .await;
  create_test_build(&pool, eval.id, "arm-pkg", &drv_arm, Some("aarch64-linux"))
    .await;

  // Filter by x86_64-linux
  let x86_builds = repo::builds::list_filtered(
    &pool,
    Some(eval.id),
    None,
    Some("x86_64-linux"),
    None,
    50,
    0,
  )
  .await
  .expect("filter x86");
  assert!(
    x86_builds
      .iter()
      .all(|b| b.system.as_deref() == Some("x86_64-linux"))
  );
  assert!(!x86_builds.is_empty());

  // Filter by aarch64-linux
  let arm_builds = repo::builds::list_filtered(
    &pool,
    Some(eval.id),
    None,
    Some("aarch64-linux"),
    None,
    50,
    0,
  )
  .await
  .expect("filter arm");
  assert!(
    arm_builds
      .iter()
      .all(|b| b.system.as_deref() == Some("aarch64-linux"))
  );
  assert!(!arm_builds.is_empty());

  // Count
  let x86_count = repo::builds::count_filtered(
    &pool,
    Some(eval.id),
    None,
    Some("x86_64-linux"),
    None,
  )
  .await
  .expect("count x86");
  assert_eq!(x86_count, x86_builds.len() as i64);

  // Cleanup
  let _ = repo::projects::delete(&pool, project.id).await;
}

#[tokio::test]
async fn test_list_filtered_with_job_name_filter() {
  let Some(pool) = get_pool().await else {
    return;
  };

  let project = create_test_project(&pool, "filter-job").await;
  let jobset = create_test_jobset(&pool, project.id).await;
  let eval = create_test_eval(&pool, jobset.id).await;

  let drv1 = format!("/nix/store/{}.drv", uuid::Uuid::new_v4().simple());
  let drv2 = format!("/nix/store/{}.drv", uuid::Uuid::new_v4().simple());
  let drv3 = format!("/nix/store/{}.drv", uuid::Uuid::new_v4().simple());

  create_test_build(&pool, eval.id, "hello-world", &drv1, None).await;
  create_test_build(&pool, eval.id, "hello-lib", &drv2, None).await;
  create_test_build(&pool, eval.id, "goodbye", &drv3, None).await;

  // ILIKE filter should match both hello-world and hello-lib
  let hello_builds = repo::builds::list_filtered(
    &pool,
    Some(eval.id),
    None,
    None,
    Some("hello"),
    50,
    0,
  )
  .await
  .expect("filter hello");
  assert_eq!(hello_builds.len(), 2);
  assert!(hello_builds.iter().all(|b| b.job_name.contains("hello")));

  // "goodbye" should only match one
  let goodbye_builds = repo::builds::list_filtered(
    &pool,
    Some(eval.id),
    None,
    None,
    Some("goodbye"),
    50,
    0,
  )
  .await
  .expect("filter goodbye");
  assert_eq!(goodbye_builds.len(), 1);

  // Count matches
  let count = repo::builds::count_filtered(
    &pool,
    Some(eval.id),
    None,
    None,
    Some("hello"),
  )
  .await
  .expect("count hello");
  assert_eq!(count, 2);

  // Cleanup
  let _ = repo::projects::delete(&pool, project.id).await;
}

#[tokio::test]
async fn test_reset_orphaned() {
  let Some(pool) = get_pool().await else {
    return;
  };

  let project = create_test_project(&pool, "orphan").await;
  let jobset = create_test_jobset(&pool, project.id).await;
  let eval = create_test_eval(&pool, jobset.id).await;

  // Create and start a build, then set started_at far in the past to simulate
  // orphan
  let drv = format!("/nix/store/{}.drv", uuid::Uuid::new_v4().simple());
  let build =
    create_test_build(&pool, eval.id, "orphan-test", &drv, None).await;
  repo::builds::start(&pool, build.id).await.unwrap();

  // Set started_at to 2 hours ago to make it look orphaned
  let client = pool.get().await.unwrap();
  client
    .execute(
      "UPDATE builds SET started_at = NOW() - INTERVAL '2 hours' WHERE id = $1",
      &[&build.id],
    )
    .await
    .unwrap();

  // Reset orphaned with 1 hour threshold
  let reset_count = repo::builds::reset_orphaned(&pool, 3600)
    .await
    .expect("reset orphaned");
  assert!(reset_count >= 1);

  // Verify the build is back to pending
  let build = repo::builds::get(&pool, build.id).await.expect("get build");
  assert!(matches!(build.status, BuildStatus::Pending));
  assert!(build.started_at.is_none());

  // Cleanup
  let _ = repo::projects::delete(&pool, project.id).await;
}

#[tokio::test]
async fn test_reset_orphaned_excludes_active_builds() {
  let Some(pool) = get_pool().await else {
    return;
  };

  let project = create_test_project(&pool, "orphan-active").await;
  let jobset = create_test_jobset(&pool, project.id).await;
  let eval = create_test_eval(&pool, jobset.id).await;

  let active_drv = format!("/nix/store/{}.drv", uuid::Uuid::new_v4().simple());
  let orphan_drv = format!("/nix/store/{}.drv", uuid::Uuid::new_v4().simple());
  let active =
    create_test_build(&pool, eval.id, "active", &active_drv, None).await;
  let orphan =
    create_test_build(&pool, eval.id, "orphan", &orphan_drv, None).await;

  repo::builds::start(&pool, active.id).await.unwrap();
  repo::builds::start(&pool, orphan.id).await.unwrap();

  let old_ids = vec![active.id, orphan.id];
  let client = pool.get().await.unwrap();
  client
    .execute(
      "UPDATE builds SET started_at = NOW() - INTERVAL '2 hours' WHERE id = \
       ANY($1)",
      &[&old_ids],
    )
    .await
    .unwrap();

  let reset_count =
    repo::builds::reset_orphaned_excluding(&pool, 3600, &[active.id])
      .await
      .expect("reset orphaned excluding active");
  assert!(reset_count >= 1);

  let active = repo::builds::get(&pool, active.id)
    .await
    .expect("reload active build");
  let orphan = repo::builds::get(&pool, orphan.id)
    .await
    .expect("reload orphan build");
  assert!(matches!(active.status, BuildStatus::Running));
  assert!(active.started_at.is_some());
  assert!(matches!(orphan.status, BuildStatus::Pending));
  assert!(orphan.started_at.is_none());

  let reset_count = repo::builds::reset_orphaned(&pool, 3600)
    .await
    .expect("reset remaining orphaned build");
  assert!(reset_count >= 1);

  let active = repo::builds::get(&pool, active.id)
    .await
    .expect("reload active build after full reset");
  assert!(matches!(active.status, BuildStatus::Pending));
  assert!(active.started_at.is_none());

  // Cleanup
  let _ = repo::projects::delete(&pool, project.id).await;
}

#[tokio::test]
async fn test_build_cancel_cascade() {
  let Some(pool) = get_pool_with_size(1).await else {
    return;
  };

  let project = create_test_project(&pool, "cancel-cascade").await;
  let jobset = create_test_jobset(&pool, project.id).await;
  let eval = create_test_eval(&pool, jobset.id).await;

  let drv1 = format!("/nix/store/{}.drv", uuid::Uuid::new_v4().simple());
  let drv2 = format!("/nix/store/{}.drv", uuid::Uuid::new_v4().simple());

  let parent = create_test_build(&pool, eval.id, "parent", &drv1, None).await;
  let child = create_test_build(&pool, eval.id, "child", &drv2, None).await;

  // child depends on parent
  repo::build_dependencies::create(&pool, child.id, parent.id)
    .await
    .expect("create dep");

  // Cancel parent should cascade to child
  let cancelled = tokio::time::timeout(
    Duration::from_secs(2),
    repo::builds::cancel_cascade(&pool, parent.id),
  )
  .await
  .expect("cancel cascade deadlocked")
  .expect("cancel cascade");

  assert!(!cancelled.is_empty());

  // Both should be cancelled
  let parent = repo::builds::get(&pool, parent.id).await.unwrap();
  let child = repo::builds::get(&pool, child.id).await.unwrap();
  assert!(matches!(parent.status, BuildStatus::Cancelled));
  assert!(matches!(child.status, BuildStatus::Cancelled));

  // Cleanup
  let _ = repo::projects::delete(&pool, project.id).await;
}

#[tokio::test]
async fn test_dedup_by_drv_path() {
  let Some(pool) = get_pool().await else {
    return;
  };

  let project = create_test_project(&pool, "dedup").await;
  let jobset = create_test_jobset(&pool, project.id).await;
  let eval = create_test_eval(&pool, jobset.id).await;

  let drv = format!("/nix/store/{}.drv", uuid::Uuid::new_v4().simple());

  let build = create_test_build(&pool, eval.id, "dedup-pkg", &drv, None).await;

  // Complete it
  repo::builds::start(&pool, build.id).await.unwrap();
  repo::builds::complete(
    &pool,
    build.id,
    BuildStatus::Succeeded,
    None,
    None,
    None,
  )
  .await
  .unwrap();

  // Check single dedup
  let existing = repo::builds::get_completed_by_drv_path(&pool, &drv)
    .await
    .expect("dedup check");
  assert!(existing.is_some());
  assert_eq!(existing.unwrap().id, build.id);

  // Check batch dedup
  let batch =
    repo::builds::get_completed_by_drv_paths(&pool, std::slice::from_ref(&drv))
      .await
      .expect("batch dedup");
  assert!(batch.contains_key(&drv));

  // Cleanup
  let _ = repo::projects::delete(&pool, project.id).await;
}

#[tokio::test]
async fn test_build_outputs_crud() {
  let Some(pool) = get_pool().await else {
    return;
  };

  // Create project, jobset, evaluation, build
  let project = create_test_project(&pool, "test-project").await;
  let jobset = create_test_jobset(&pool, project.id).await;
  let eval = create_test_eval(&pool, jobset.id).await;
  let build =
    create_test_build(&pool, eval.id, "test-job", "/nix/store/test.drv", None)
      .await;

  // Create outputs
  let _out1 = repo::build_outputs::create(
    &pool,
    build.id,
    "out",
    Some("/nix/store/abc-result"),
  )
  .await
  .expect("create output 1");

  let _out2 = repo::build_outputs::create(
    &pool,
    build.id,
    "dev",
    Some("/nix/store/def-result-dev"),
  )
  .await
  .expect("create output 2");

  // List outputs for build
  let outputs = repo::build_outputs::list_for_build(&pool, build.id)
    .await
    .expect("list outputs");
  assert_eq!(outputs.len(), 2);
  assert_eq!(outputs[0].name, "dev"); // Alphabetical order
  assert_eq!(outputs[1].name, "out");

  // Find by path
  let found = repo::build_outputs::find_by_path(&pool, "/nix/store/abc-result")
    .await
    .expect("find by path");
  assert_eq!(found.len(), 1);
  assert_eq!(found[0].build, build.id);
  assert_eq!(found[0].name, "out");

  // Cleanup
  let _ = repo::projects::delete(&pool, project.id).await;
}

#[tokio::test]
async fn test_build_outputs_cascade_delete() {
  let Some(pool) = get_pool().await else {
    return;
  };

  let project = create_test_project(&pool, "test-project").await;
  let jobset = create_test_jobset(&pool, project.id).await;
  let eval = create_test_eval(&pool, jobset.id).await;
  let build =
    create_test_build(&pool, eval.id, "test-job", "/nix/store/test.drv", None)
      .await;

  repo::build_outputs::create(&pool, build.id, "out", Some("/nix/store/abc"))
    .await
    .expect("create output");

  // Delete build
  repo::builds::delete(&pool, build.id)
    .await
    .expect("delete build");

  // Verify outputs cascade deleted
  let outputs = repo::build_outputs::list_for_build(&pool, build.id)
    .await
    .expect("list outputs after delete");
  assert_eq!(outputs.len(), 0);

  // Cleanup
  let _ = repo::projects::delete(&pool, project.id).await;
}

#[tokio::test]
async fn test_declarative_projects_are_reconciled_authoritatively() {
  let Some(pool) = get_pool().await else {
    return;
  };

  let suffix = uuid::Uuid::new_v4();
  let kept_name = format!("decl-kept-{suffix}");
  let removed_name = format!("decl-removed-{suffix}");
  let unmanaged = create_test_project(&pool, "unmanaged").await;
  let initial: DeclarativeConfig = toml::from_str(&format!(
    r#"
      [[projects]]
      name = "{kept_name}"
      repository_url = "https://example.com/kept"

      [[projects.jobsets]]
      name = "keep"
      nix_expression = "packages"

      [[projects.jobsets]]
      name = "remove"
      nix_expression = "checks"

      [[projects]]
      name = "{removed_name}"
      repository_url = "https://example.com/removed"
    "#
  ))
  .expect("parse initial declarative config");

  circus_common::bootstrap::run(&pool, &initial, None)
    .await
    .expect("apply initial declarative config");
  let kept = repo::projects::get_by_name(&pool, &kept_name)
    .await
    .expect("get declarative project");
  assert!(kept.managed_declaratively);
  assert_eq!(
    repo::jobsets::list_for_project(&pool, kept.id, 10, 0)
      .await
      .expect("list initial jobsets")
      .len(),
    2
  );

  let updated: DeclarativeConfig = toml::from_str(&format!(
    r#"
      [[projects]]
      name = "{kept_name}"
      repository_url = "https://example.com/kept"

      [[projects.jobsets]]
      name = "keep"
      nix_expression = "packages"
    "#
  ))
  .expect("parse updated declarative config");
  circus_common::bootstrap::run(&pool, &updated, None)
    .await
    .expect("reconcile declarative config");

  assert!(
    repo::projects::get_by_name(&pool, &removed_name)
      .await
      .is_err()
  );
  assert!(repo::projects::get(&pool, unmanaged.id).await.is_ok());
  assert_eq!(
    repo::jobsets::list_for_project(&pool, kept.id, 10, 0)
      .await
      .expect("list reconciled jobsets")
      .iter()
      .map(|jobset| jobset.name.as_str())
      .collect::<Vec<_>>(),
    ["keep"]
  );

  repo::projects::delete(&pool, kept.id)
    .await
    .expect("delete declarative project");
  repo::projects::delete(&pool, unmanaged.id)
    .await
    .expect("delete unmanaged project");
}
