//! Tests for the queue runner.
//! Nix log parsing tests require no external binaries.
//! Database tests require `TEST_DATABASE_URL`.
#![expect(
  clippy::unwrap_used,
  clippy::expect_used,
  clippy::print_stdout,
  reason = "Fine in tests"
)]

// Nix log line parsing

#[test]
fn test_parse_nix_log_start() {
  let line =
    r#"@nix {"action":"start","derivation":"/nix/store/abc-hello.drv"}"#;
  let result = circus_queue_runner::builder::parse_nix_log_line(line);
  assert!(result.is_some());
  let (action, drv) = result.unwrap();
  assert_eq!(action, "start");
  assert_eq!(drv, "/nix/store/abc-hello.drv");
}

#[test]
fn test_parse_nix_log_stop() {
  let line =
    r#"@nix {"action":"stop","derivation":"/nix/store/abc-hello.drv"}"#;
  let result = circus_queue_runner::builder::parse_nix_log_line(line);
  assert!(result.is_some());
  let (action, drv) = result.unwrap();
  assert_eq!(action, "stop");
  assert_eq!(drv, "/nix/store/abc-hello.drv");
}

#[test]
fn test_parse_nix_log_unknown_action() {
  let line = r#"@nix {"action":"msg","msg":"building..."}"#;
  let result = circus_queue_runner::builder::parse_nix_log_line(line);
  assert!(result.is_none());
}

#[test]
fn test_parse_nix_log_not_nix_prefix() {
  let line = "building '/nix/store/abc-hello.drv'...";
  let result = circus_queue_runner::builder::parse_nix_log_line(line);
  assert!(result.is_none());
}

#[test]
fn test_parse_nix_log_invalid_json() {
  let line = "@nix {invalid json}";
  let result = circus_queue_runner::builder::parse_nix_log_line(line);
  assert!(result.is_none());
}

#[test]
fn test_parse_nix_log_no_derivation_field() {
  let line = r#"@nix {"action":"start","type":"build"}"#;
  let result = circus_queue_runner::builder::parse_nix_log_line(line);
  assert!(result.is_none());
}

#[test]
fn test_parse_nix_log_empty_line() {
  let result = circus_queue_runner::builder::parse_nix_log_line("");
  assert!(result.is_none());
}

// WorkerPool drain

#[tokio::test]
async fn test_worker_pool_drain_stops_dispatch() {
  // Create a minimal worker pool
  let Ok(url) = std::env::var("TEST_DATABASE_URL") else {
    println!("Skipping: TEST_DATABASE_URL not set");
    return;
  };

  let pool = sqlx::postgres::PgPoolOptions::new()
    .max_connections(1)
    .connect(&url)
    .await
    .expect("failed to connect");

  let hot_config = std::sync::Arc::new(tokio::sync::RwLock::new(
    circus_common::config::HotConfig {
      poll_interval:        std::time::Duration::from_secs(1),
      build_timeout:        std::time::Duration::from_mins(1),
      notifications_config: circus_common::config::NotificationsConfig::default(
      ),
      failed_paths_ttl:     0,
      scheduling_strategy:
        circus_common::config::BuilderSchedulingStrategy::default(),
      psi_threshold:        None,
      psi_check_timeout:    std::time::Duration::from_secs(5),
      extra_nix_build_args: Vec::new(),
    },
  ));
  let worker_pool = circus_queue_runner::worker::WorkerPool::new(
    pool,
    2,
    std::env::temp_dir(),
    std::path::PathBuf::from("/nix/store"),
    hot_config,
    circus_common::config::LogConfig::default(),
    circus_common::config::GcConfig::default(),
    circus_common::config::SigningConfig::default(),
    circus_common::config::CacheUploadConfig::default(),
    None,
    circus_queue_runner::rpc::AgentPool::new(),
    std::time::Duration::from_mins(1),
  );

  // Drain should not panic
  worker_pool.drain();

  // After drain, dispatching should be a no-op (build won't start)
  // We can't easily test this without a real build, but at least verify drain
  // doesn't crash
}

// Per-build cancellation

#[tokio::test]
async fn test_active_builds_registry_cancellation() {
  use std::sync::Arc;

  use dashmap::DashMap;
  use tokio_util::sync::CancellationToken;

  let active_builds: circus_queue_runner::worker::ActiveBuilds =
    Arc::new(DashMap::new());

  let id1 = uuid::Uuid::new_v4();
  let id2 = uuid::Uuid::new_v4();
  let id3 = uuid::Uuid::new_v4();

  let token1 = CancellationToken::new();
  let token2 = CancellationToken::new();
  let token3 = CancellationToken::new();

  active_builds.insert(id1, token1.clone());
  active_builds.insert(id2, token2.clone());
  active_builds.insert(id3, token3.clone());

  assert_eq!(active_builds.len(), 3);
  assert!(!token1.is_cancelled());
  assert!(!token2.is_cancelled());
  assert!(!token3.is_cancelled());

  // Simulate cancel checker finding id1 and id2 cancelled in DB
  if let Some((_, token)) = active_builds.remove(&id1) {
    token.cancel();
  }
  if let Some((_, token)) = active_builds.remove(&id2) {
    token.cancel();
  }

  assert!(token1.is_cancelled());
  assert!(token2.is_cancelled());
  assert!(!token3.is_cancelled());
  assert_eq!(active_builds.len(), 1);
  assert!(active_builds.contains_key(&id3));
}

#[tokio::test]
async fn test_cancellation_token_aborts_select() {
  use tokio_util::sync::CancellationToken;

  let token = CancellationToken::new();
  let token_clone = token.clone();

  // Simulate a long-running build
  let build_future = async {
    tokio::time::sleep(std::time::Duration::from_mins(1)).await;
    "completed"
  };

  // Cancel after 50ms
  tokio::spawn(async move {
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    token_clone.cancel();
  });

  let start = std::time::Instant::now();
  let result = tokio::select! {
      val = build_future => val,
      () = token.cancelled() => "cancelled",
  };

  assert_eq!(result, "cancelled");
  // Should complete in well under a second (the 60s "build" was aborted)
  assert!(start.elapsed() < std::time::Duration::from_secs(1));
}

#[tokio::test]
async fn test_worker_pool_active_builds_cancel() {
  let Ok(url) = std::env::var("TEST_DATABASE_URL") else {
    println!("Skipping: TEST_DATABASE_URL not set");
    return;
  };

  let pool = sqlx::postgres::PgPoolOptions::new()
    .max_connections(1)
    .connect(&url)
    .await
    .expect("failed to connect");

  let hot_config = std::sync::Arc::new(tokio::sync::RwLock::new(
    circus_common::config::HotConfig {
      poll_interval:        std::time::Duration::from_secs(1),
      build_timeout:        std::time::Duration::from_mins(1),
      notifications_config: circus_common::config::NotificationsConfig::default(
      ),
      failed_paths_ttl:     0,
      scheduling_strategy:
        circus_common::config::BuilderSchedulingStrategy::default(),
      psi_threshold:        None,
      psi_check_timeout:    std::time::Duration::from_secs(5),
      extra_nix_build_args: Vec::new(),
    },
  ));
  let worker_pool = circus_queue_runner::worker::WorkerPool::new(
    pool,
    2,
    std::env::temp_dir(),
    std::path::PathBuf::from("/nix/store"),
    hot_config,
    circus_common::config::LogConfig::default(),
    circus_common::config::GcConfig::default(),
    circus_common::config::SigningConfig::default(),
    circus_common::config::CacheUploadConfig::default(),
    None,
    circus_queue_runner::rpc::AgentPool::new(),
    std::time::Duration::from_mins(1),
  );

  // Active builds map should start empty
  assert!(worker_pool.active_builds().is_empty());

  // Manually insert a token (simulating what dispatch does internally)
  let build_id = uuid::Uuid::new_v4();
  let token = tokio_util::sync::CancellationToken::new();
  worker_pool.active_builds().insert(build_id, token.clone());

  assert_eq!(worker_pool.active_builds().len(), 1);
  assert!(worker_pool.active_builds().contains_key(&build_id));
  assert!(!token.is_cancelled());

  // Simulate cancel checker removing and triggering the token
  if let Some((_, t)) = worker_pool.active_builds().remove(&build_id) {
    t.cancel();
  }

  assert!(token.is_cancelled());
  assert!(worker_pool.active_builds().is_empty());
}

// Fair-share scheduling

#[tokio::test]
async fn test_fair_share_scheduling() {
  let Ok(url) = std::env::var("TEST_DATABASE_URL") else {
    println!("Skipping: TEST_DATABASE_URL not set");
    return;
  };

  let pool = sqlx::postgres::PgPoolOptions::new()
    .max_connections(5)
    .connect(&url)
    .await
    .expect("failed to connect");

  sqlx::migrate!("../common/migrations")
    .run(&pool)
    .await
    .expect("migration failed");

  // Create two projects with different scheduling shares
  let project_hi = circus_common::repo::projects::create(
    &pool,
    circus_common::models::CreateProject {
      name:           format!("fair-hi-{}", uuid::Uuid::new_v4()),
      description:    None,
      repository_url: "https://github.com/test/repo".to_string(),
    },
  )
  .await
  .expect("create project hi");

  let project_lo = circus_common::repo::projects::create(
    &pool,
    circus_common::models::CreateProject {
      name:           format!("fair-lo-{}", uuid::Uuid::new_v4()),
      description:    None,
      repository_url: "https://github.com/test/repo".to_string(),
    },
  )
  .await
  .expect("create project lo");

  // High-share jobset (200 shares) and low-share jobset (100 shares)
  let jobset_hi = circus_common::repo::jobsets::create(
    &pool,
    circus_common::models::CreateJobset {
      project_id:        project_hi.id,
      name:              "main".to_string(),
      nix_expression:    "packages".to_string(),
      enabled:           None,
      flake_mode:        None,
      check_interval:    None,
      branch:            None,
      scheduling_shares: Some(200),
      state:             None,
      keep_nr:           None,
    },
  )
  .await
  .expect("create jobset hi");

  let jobset_lo = circus_common::repo::jobsets::create(
    &pool,
    circus_common::models::CreateJobset {
      project_id:        project_lo.id,
      name:              "main".to_string(),
      nix_expression:    "packages".to_string(),
      enabled:           None,
      flake_mode:        None,
      check_interval:    None,
      branch:            None,
      scheduling_shares: Some(100),
      state:             None,
      keep_nr:           None,
    },
  )
  .await
  .expect("create jobset lo");

  let eval_hi = circus_common::repo::evaluations::create(
    &pool,
    circus_common::models::CreateEvaluation {
      jobset_id:      jobset_hi.id,
      commit_hash:    format!("hi{}", uuid::Uuid::new_v4().simple()),
      pr_number:      None,
      pr_head_branch: None,
      pr_base_branch: None,
      pr_action:      None,
    },
  )
  .await
  .expect("create eval hi");

  let eval_lo = circus_common::repo::evaluations::create(
    &pool,
    circus_common::models::CreateEvaluation {
      jobset_id:      jobset_lo.id,
      commit_hash:    format!("lo{}", uuid::Uuid::new_v4().simple()),
      pr_number:      None,
      pr_head_branch: None,
      pr_base_branch: None,
      pr_action:      None,
    },
  )
  .await
  .expect("create eval lo");

  // Create pending builds: 2 for hi-share, 2 for lo-share
  let drv_hi_1 =
    format!("/nix/store/{}-hi1.drv", uuid::Uuid::new_v4().simple());
  let drv_hi_2 =
    format!("/nix/store/{}-hi2.drv", uuid::Uuid::new_v4().simple());
  let drv_lo_1 =
    format!("/nix/store/{}-lo1.drv", uuid::Uuid::new_v4().simple());
  let drv_lo_2 =
    format!("/nix/store/{}-lo2.drv", uuid::Uuid::new_v4().simple());

  circus_common::repo::builds::create(
    &pool,
    circus_common::models::CreateBuild {
      evaluation_id: eval_hi.id,
      job_name: "hi-build-1".to_string(),
      drv_path: drv_hi_1,
      system: Some("x86_64-linux".to_string()),
      outputs: None,
      is_aggregate: None,
      constituents: None,
      is_fod: None,
      fod_hash: None,
      ..Default::default()
    },
  )
  .await
  .expect("create hi build 1");

  circus_common::repo::builds::create(
    &pool,
    circus_common::models::CreateBuild {
      evaluation_id: eval_hi.id,
      job_name: "hi-build-2".to_string(),
      drv_path: drv_hi_2,
      system: Some("x86_64-linux".to_string()),
      outputs: None,
      is_aggregate: None,
      constituents: None,
      is_fod: None,
      fod_hash: None,
      ..Default::default()
    },
  )
  .await
  .expect("create hi build 2");

  circus_common::repo::builds::create(
    &pool,
    circus_common::models::CreateBuild {
      evaluation_id: eval_lo.id,
      job_name: "lo-build-1".to_string(),
      drv_path: drv_lo_1,
      system: Some("x86_64-linux".to_string()),
      outputs: None,
      is_aggregate: None,
      constituents: None,
      is_fod: None,
      fod_hash: None,
      ..Default::default()
    },
  )
  .await
  .expect("create lo build 1");

  circus_common::repo::builds::create(
    &pool,
    circus_common::models::CreateBuild {
      evaluation_id: eval_lo.id,
      job_name: "lo-build-2".to_string(),
      drv_path: drv_lo_2,
      system: Some("x86_64-linux".to_string()),
      outputs: None,
      is_aggregate: None,
      constituents: None,
      is_fod: None,
      fod_hash: None,
      ..Default::default()
    },
  )
  .await
  .expect("create lo build 2");

  // With no running builds, hi-share jobset (200) should come first due to
  // higher share fraction (200/300 > 100/300)
  let pending = circus_common::repo::builds::list_pending(&pool, 10, 4)
    .await
    .expect("list pending cold start");
  assert!(
    pending.len() >= 4,
    "expected at least 4 pending builds, got {}",
    pending.len()
  );

  // The first builds should belong to the high-share jobset
  let first_eval = pending[0].evaluation_id;
  assert_eq!(
    first_eval, eval_hi.id,
    "high-share jobset should be scheduled first on cold start"
  );

  // Now simulate: start 2 builds from the high-share jobset to make it
  // over-served (2 running out of 4 workers, but only 66% of shares)
  let hi_build_ids: Vec<_> = pending
    .iter()
    .filter(|b| b.evaluation_id == eval_hi.id)
    .map(|b| b.id)
    .take(2)
    .collect();

  for &id in &hi_build_ids {
    circus_common::repo::builds::start(&pool, id)
      .await
      .expect("start hi build");
  }

  // Re-query: lo-share jobset should now be prioritized because it is
  // underserved (0 running vs its fair share)
  let pending2 = circus_common::repo::builds::list_pending(&pool, 10, 4)
    .await
    .expect("list pending after running");

  assert!(
    !pending2.is_empty(),
    "expected pending builds from lo-share jobset"
  );
  assert_eq!(
    pending2[0].evaluation_id, eval_lo.id,
    "underserved lo-share jobset should be scheduled first when hi-share is \
     over-served"
  );

  // Clean up
  let _ = circus_common::repo::projects::delete(&pool, project_hi.id).await;
  let _ = circus_common::repo::projects::delete(&pool, project_lo.id).await;
}

// Database-dependent tests

#[tokio::test]
async fn test_atomic_build_claiming() {
  let Ok(url) = std::env::var("TEST_DATABASE_URL") else {
    println!("Skipping: TEST_DATABASE_URL not set");
    return;
  };

  let pool = sqlx::postgres::PgPoolOptions::new()
    .max_connections(5)
    .connect(&url)
    .await
    .expect("failed to connect");

  sqlx::migrate!("../common/migrations")
    .run(&pool)
    .await
    .expect("migration failed");

  // Create a project -> jobset -> evaluation -> build chain
  let project = circus_common::repo::projects::create(
    &pool,
    circus_common::models::CreateProject {
      name:           format!("runner-test-{}", uuid::Uuid::new_v4()),
      description:    None,
      repository_url: "https://github.com/test/repo".to_string(),
    },
  )
  .await
  .expect("create project");

  let jobset = circus_common::repo::jobsets::create(
    &pool,
    circus_common::models::CreateJobset {
      project_id:        project.id,
      name:              "main".to_string(),
      nix_expression:    "packages".to_string(),
      enabled:           None,
      flake_mode:        None,
      check_interval:    None,
      branch:            None,
      scheduling_shares: None,
      state:             None,
      keep_nr:           None,
    },
  )
  .await
  .expect("create jobset");

  let eval = circus_common::repo::evaluations::create(
    &pool,
    circus_common::models::CreateEvaluation {
      jobset_id:      jobset.id,
      commit_hash:    "abcdef1234567890abcdef1234567890abcdef12".to_string(),
      pr_number:      None,
      pr_head_branch: None,
      pr_base_branch: None,
      pr_action:      None,
    },
  )
  .await
  .expect("create eval");

  let build = circus_common::repo::builds::create(
    &pool,
    circus_common::models::CreateBuild {
      evaluation_id: eval.id,
      job_name: "test-build".to_string(),
      drv_path: "/nix/store/test-runner-test.drv".to_string(),
      system: Some("x86_64-linux".to_string()),
      outputs: None,
      is_aggregate: None,
      constituents: None,
      is_fod: None,
      fod_hash: None,
      ..Default::default()
    },
  )
  .await
  .expect("create build");

  assert_eq!(build.status, circus_common::models::BuildStatus::Pending);

  // First claim should succeed
  let claimed = circus_common::repo::builds::start(&pool, build.id)
    .await
    .expect("start build");
  assert!(claimed.is_some());

  // Second claim should return None (already claimed)
  let claimed2 = circus_common::repo::builds::start(&pool, build.id)
    .await
    .expect("start build again");
  assert!(claimed2.is_none());

  // Clean up
  let _ = circus_common::repo::projects::delete(&pool, project.id).await;
}

#[tokio::test]
async fn test_orphan_build_reset() {
  let Ok(url) = std::env::var("TEST_DATABASE_URL") else {
    println!("Skipping: TEST_DATABASE_URL not set");
    return;
  };

  let pool = sqlx::postgres::PgPoolOptions::new()
    .max_connections(5)
    .connect(&url)
    .await
    .expect("failed to connect");

  sqlx::migrate!("../common/migrations")
    .run(&pool)
    .await
    .expect("migration failed");

  let project = circus_common::repo::projects::create(
    &pool,
    circus_common::models::CreateProject {
      name:           format!("orphan-test-{}", uuid::Uuid::new_v4()),
      description:    None,
      repository_url: "https://github.com/test/repo".to_string(),
    },
  )
  .await
  .expect("create project");

  let jobset = circus_common::repo::jobsets::create(
    &pool,
    circus_common::models::CreateJobset {
      project_id:        project.id,
      name:              "main".to_string(),
      nix_expression:    "packages".to_string(),
      enabled:           None,
      flake_mode:        None,
      check_interval:    None,
      branch:            None,
      scheduling_shares: None,
      state:             None,
      keep_nr:           None,
    },
  )
  .await
  .expect("create jobset");

  let eval = circus_common::repo::evaluations::create(
    &pool,
    circus_common::models::CreateEvaluation {
      jobset_id:      jobset.id,
      commit_hash:    "1234567890abcdef1234567890abcdef12345678".to_string(),
      pr_number:      None,
      pr_head_branch: None,
      pr_base_branch: None,
      pr_action:      None,
    },
  )
  .await
  .expect("create eval");

  // Create a build and mark it running
  let build = circus_common::repo::builds::create(
    &pool,
    circus_common::models::CreateBuild {
      evaluation_id: eval.id,
      job_name: "orphan-build".to_string(),
      drv_path: "/nix/store/test-orphan.drv".to_string(),
      system: None,
      outputs: None,
      is_aggregate: None,
      constituents: None,
      is_fod: None,
      fod_hash: None,
      ..Default::default()
    },
  )
  .await
  .expect("create build");

  let _ = circus_common::repo::builds::start(&pool, build.id).await;

  // Simulate the build being stuck for a while by manually backdating
  // started_at
  // Truly a genius way to test.
  sqlx::query(
    "UPDATE builds SET started_at = NOW() - INTERVAL '10 minutes' WHERE id = \
     $1",
  )
  .bind(build.id)
  .execute(&pool)
  .await
  .expect("backdate build");

  // Reset orphaned builds (older than 5 minutes)
  let count = circus_common::repo::builds::reset_orphaned(&pool, 300)
    .await
    .expect("reset orphaned");
  assert!(count >= 1, "should have reset at least 1 orphaned build");

  // Verify build is pending again
  let reset_build = circus_common::repo::builds::get(&pool, build.id)
    .await
    .expect("get build");
  assert_eq!(
    reset_build.status,
    circus_common::models::BuildStatus::Pending
  );

  // Clean up
  let _ = circus_common::repo::projects::delete(&pool, project.id).await;
}

#[tokio::test]
async fn test_get_cancelled_among() {
  let Ok(url) = std::env::var("TEST_DATABASE_URL") else {
    println!("Skipping: TEST_DATABASE_URL not set");
    return;
  };

  let pool = sqlx::postgres::PgPoolOptions::new()
    .max_connections(5)
    .connect(&url)
    .await
    .expect("failed to connect");

  sqlx::migrate!("../common/migrations")
    .run(&pool)
    .await
    .expect("migration failed");

  let project = circus_common::repo::projects::create(
    &pool,
    circus_common::models::CreateProject {
      name:           format!("cancel-among-{}", uuid::Uuid::new_v4()),
      description:    None,
      repository_url: "https://github.com/test/repo".to_string(),
    },
  )
  .await
  .expect("create project");

  let jobset = circus_common::repo::jobsets::create(
    &pool,
    circus_common::models::CreateJobset {
      project_id:        project.id,
      name:              "main".to_string(),
      nix_expression:    "packages".to_string(),
      enabled:           None,
      flake_mode:        None,
      check_interval:    None,
      branch:            None,
      scheduling_shares: None,
      state:             None,
      keep_nr:           None,
    },
  )
  .await
  .expect("create jobset");

  let eval = circus_common::repo::evaluations::create(
    &pool,
    circus_common::models::CreateEvaluation {
      jobset_id:      jobset.id,
      commit_hash:    "aabbccdd1234567890aabbccdd1234567890aabb".to_string(),
      pr_number:      None,
      pr_head_branch: None,
      pr_base_branch: None,
      pr_action:      None,
    },
  )
  .await
  .expect("create eval");

  // Create a pending build
  let build_pending = circus_common::repo::builds::create(
    &pool,
    circus_common::models::CreateBuild {
      evaluation_id: eval.id,
      job_name: "pending-job".to_string(),
      drv_path: format!("/nix/store/{}-pending.drv", uuid::Uuid::new_v4()),
      system: None,
      outputs: None,
      is_aggregate: None,
      constituents: None,
      is_fod: None,
      fod_hash: None,
      ..Default::default()
    },
  )
  .await
  .expect("create pending build");

  // Create a running build
  let build_running = circus_common::repo::builds::create(
    &pool,
    circus_common::models::CreateBuild {
      evaluation_id: eval.id,
      job_name: "running-job".to_string(),
      drv_path: format!("/nix/store/{}-running.drv", uuid::Uuid::new_v4()),
      system: None,
      outputs: None,
      is_aggregate: None,
      constituents: None,
      is_fod: None,
      fod_hash: None,
      ..Default::default()
    },
  )
  .await
  .expect("create running build");
  circus_common::repo::builds::start(&pool, build_running.id)
    .await
    .expect("start running build");

  // Create a cancelled build (start then cancel)
  let build_cancelled = circus_common::repo::builds::create(
    &pool,
    circus_common::models::CreateBuild {
      evaluation_id: eval.id,
      job_name: "cancelled-job".to_string(),
      drv_path: format!("/nix/store/{}-cancelled.drv", uuid::Uuid::new_v4()),
      system: None,
      outputs: None,
      is_aggregate: None,
      constituents: None,
      is_fod: None,
      fod_hash: None,
      ..Default::default()
    },
  )
  .await
  .expect("create cancelled build");
  circus_common::repo::builds::start(&pool, build_cancelled.id)
    .await
    .expect("start cancelled build");
  circus_common::repo::builds::cancel(&pool, build_cancelled.id)
    .await
    .expect("cancel build");

  // Query for cancelled among all three
  let all_ids = vec![build_pending.id, build_running.id, build_cancelled.id];
  let cancelled =
    circus_common::repo::builds::get_cancelled_among(&pool, &all_ids)
      .await
      .expect("get cancelled among");
  assert_eq!(cancelled.len(), 1);
  assert_eq!(cancelled[0], build_cancelled.id);

  // Empty input returns empty
  let empty = circus_common::repo::builds::get_cancelled_among(&pool, &[])
    .await
    .expect("empty query");
  assert!(empty.is_empty());

  // Query with only non-cancelled builds returns empty
  let none_cancelled =
    circus_common::repo::builds::get_cancelled_among(&pool, &[
      build_pending.id,
      build_running.id,
    ])
    .await
    .expect("no cancelled");
  assert!(none_cancelled.is_empty());

  // Clean up
  let _ = circus_common::repo::projects::delete(&pool, project.id).await;
}
