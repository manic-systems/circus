//! Integration tests for repository CRUD operations.
//! Requires `TEST_DATABASE_URL` to be set to a `PostgreSQL` connection string.

use fc_common::{models::*, repo};

async fn get_pool() -> Option<sqlx::PgPool> {
  let Ok(url) = std::env::var("TEST_DATABASE_URL") else {
    println!("Skipping repo test: TEST_DATABASE_URL not set");
    return None;
  };

  let pool = sqlx::postgres::PgPoolOptions::new()
    .max_connections(5)
    .connect(&url)
    .await
    .ok()?;

  // Run migrations
  sqlx::migrate!("./migrations").run(&pool).await.ok()?;

  Some(pool)
}

/// Helper: create a project with a unique name.
async fn create_test_project(pool: &sqlx::PgPool, prefix: &str) -> Project {
  repo::projects::create(pool, CreateProject {
    name:           format!("{prefix}-{}", uuid::Uuid::new_v4()),
    description:    Some("Test project".to_string()),
    repository_url: "https://github.com/test/repo".to_string(),
  })
  .await
  .expect("create project")
}

/// Helper: create a jobset for a project.
async fn create_test_jobset(
  pool: &sqlx::PgPool,
  project_id: uuid::Uuid,
) -> Jobset {
  repo::jobsets::create(pool, CreateJobset {
    project_id,
    name: format!("default-{}", uuid::Uuid::new_v4()),
    nix_expression: "packages".to_string(),
    enabled: Some(true),
    flake_mode: None,
    check_interval: None,
    branch: None,
    scheduling_shares: None,
    state: None,
    keep_nr: None,
  })
  .await
  .expect("create jobset")
}

/// Helper: create an evaluation for a jobset.
async fn create_test_eval(
  pool: &sqlx::PgPool,
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

/// Helper: create a build for an evaluation.
async fn create_test_build(
  pool: &sqlx::PgPool,
  eval_id: uuid::Uuid,
  job_name: &str,
  drv_path: &str,
  system: Option<&str>,
) -> Build {
  repo::builds::create(pool, CreateBuild {
    evaluation_id: eval_id,
    job_name:      job_name.to_string(),
    drv_path:      drv_path.to_string(),
    system:        system.map(std::string::ToString::to_string),
    outputs:       None,
    is_aggregate:  None,
    constituents:  None,
  })
  .await
  .expect("create build")
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
    name:           None,
    description:    Some("Updated description".to_string()),
    repository_url: None,
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
    name:           name.clone(),
    description:    None,
    repository_url: "https://github.com/test/repo".to_string(),
  })
  .await
  .expect("create first project");

  // Creating with same name should fail with Conflict
  let result = repo::projects::create(&pool, CreateProject {
    name,
    description: None,
    repository_url: "https://github.com/test/repo2".to_string(),
  })
  .await;

  assert!(matches!(result, Err(fc_common::CiError::Conflict(_))));
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
    branch:            None,
    scheduling_shares: None,
    state:             None,
    keep_nr:           None,
  })
  .await
  .expect("create jobset");

  assert_eq!(jobset.name, "default");
  assert!(jobset.enabled);

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
    branch:            None,
    scheduling_shares: None,
    state:             None,
    keep_nr:           None,
  })
  .await
  .expect("update jobset");
  assert_eq!(updated.nix_expression, "checks");
  assert!(!updated.enabled);

  // Delete
  repo::jobsets::delete(&pool, jobset.id)
    .await
    .expect("delete jobset");

  // Cleanup
  repo::projects::delete(&pool, project.id).await.ok();
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

  // Get latest
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
  repo::projects::delete(&pool, project.id).await.ok();
}

#[tokio::test]
async fn test_not_found_errors() {
  let Some(pool) = get_pool().await else {
    return;
  };

  let fake_id = uuid::Uuid::new_v4();

  assert!(matches!(
    repo::projects::get(&pool, fake_id).await,
    Err(fc_common::CiError::NotFound(_))
  ));

  assert!(matches!(
    repo::jobsets::get(&pool, fake_id).await,
    Err(fc_common::CiError::NotFound(_))
  ));

  assert!(matches!(
    repo::evaluations::get(&pool, fake_id).await,
    Err(fc_common::CiError::NotFound(_))
  ));

  assert!(matches!(
    repo::builds::get(&pool, fake_id).await,
    Err(fc_common::CiError::NotFound(_))
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
  repo::projects::delete(&pool, project.id).await.ok();
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
  repo::projects::delete(&pool, project.id).await.ok();
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
  repo::projects::delete(&pool, project.id).await.ok();
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
  repo::projects::delete(&pool, project.id).await.ok();
}

#[tokio::test]
async fn test_reset_orphaned_batch_limit() {
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
  sqlx::query(
    "UPDATE builds SET started_at = NOW() - INTERVAL '2 hours' WHERE id = $1",
  )
  .bind(build.id)
  .execute(&pool)
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
  repo::projects::delete(&pool, project.id).await.ok();
}

#[tokio::test]
async fn test_build_cancel_cascade() {
  let Some(pool) = get_pool().await else {
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
  let cancelled = repo::builds::cancel_cascade(&pool, parent.id)
    .await
    .expect("cancel cascade");

  assert!(!cancelled.is_empty());

  // Both should be cancelled
  let parent = repo::builds::get(&pool, parent.id).await.unwrap();
  let child = repo::builds::get(&pool, child.id).await.unwrap();
  assert!(matches!(parent.status, BuildStatus::Cancelled));
  assert!(matches!(child.status, BuildStatus::Cancelled));

  // Cleanup
  repo::projects::delete(&pool, project.id).await.ok();
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
  repo::projects::delete(&pool, project.id).await.ok();
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
  repo::projects::delete(&pool, project.id).await.ok();
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
  repo::projects::delete(&pool, project.id).await.ok();
}
