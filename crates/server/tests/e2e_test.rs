//! End-to-end integration test.
//! Requires `TEST_DATABASE_URL` to be set.
//! Tests the full flow: create project -> jobset -> evaluation -> builds.
//!
//! Nix-dependent steps are skipped if nix is not available.

use axum::{
  body::Body,
  http::{Request, StatusCode},
};
use fc_common::models::*;
use tower::ServiceExt;

async fn get_pool() -> Option<sqlx::PgPool> {
  let Ok(url) = std::env::var("TEST_DATABASE_URL") else {
    println!("Skipping E2E test: TEST_DATABASE_URL not set");
    return None;
  };

  let pool = sqlx::postgres::PgPoolOptions::new()
    .max_connections(5)
    .connect(&url)
    .await
    .ok()?;

  sqlx::migrate!("../common/migrations")
    .run(&pool)
    .await
    .ok()?;

  Some(pool)
}

#[tokio::test]
async fn test_e2e_project_eval_build_flow() {
  let Some(pool) = get_pool().await else {
    return;
  };

  // 1. Create a project
  let project_name = format!("e2e-test-{}", uuid::Uuid::new_v4());
  let project = fc_common::repo::projects::create(&pool, CreateProject {
    name:           project_name.clone(),
    description:    Some("E2E test project".to_string()),
    repository_url: "https://github.com/test/e2e".to_string(),
  })
  .await
  .expect("create project");

  assert_eq!(project.name, project_name);

  // 2. Create a jobset
  let jobset = fc_common::repo::jobsets::create(&pool, CreateJobset {
    project_id:        project.id,
    name:              "default".to_string(),
    nix_expression:    "packages".to_string(),
    enabled:           Some(true),
    flake_mode:        Some(true),
    check_interval:    Some(300),
    branch:            None,
    scheduling_shares: None,
    state:             None,
    keep_nr:           None,
  })
  .await
  .expect("create jobset");

  assert_eq!(jobset.project_id, project.id);
  assert!(jobset.enabled);

  // 3. Verify active jobsets include our new one
  let active = fc_common::repo::jobsets::list_active(&pool)
    .await
    .expect("list active");
  assert!(
    active.iter().any(|j| j.id == jobset.id),
    "new jobset should be in active list"
  );

  // 4. Create an evaluation
  let eval = fc_common::repo::evaluations::create(&pool, CreateEvaluation {
    jobset_id:      jobset.id,
    commit_hash:    "e2e0000000000000000000000000000000000000".to_string(),
    pr_number:      None,
    pr_head_branch: None,
    pr_base_branch: None,
    pr_action:      None,
  })
  .await
  .expect("create evaluation");

  assert_eq!(eval.jobset_id, jobset.id);
  assert_eq!(eval.status, EvaluationStatus::Pending);

  // 5. Mark evaluation as running
  fc_common::repo::evaluations::update_status(
    &pool,
    eval.id,
    EvaluationStatus::Running,
    None,
  )
  .await
  .expect("update eval status");

  // 6. Create builds as if nix evaluation found jobs
  let build1 = fc_common::repo::builds::create(&pool, CreateBuild {
    evaluation_id: eval.id,
    job_name:      "hello".to_string(),
    drv_path:      "/nix/store/e2e000-hello.drv".to_string(),
    system:        Some("x86_64-linux".to_string()),
    outputs:       Some(serde_json::json!({"out": "/nix/store/e2e000-hello"})),
    is_aggregate:  Some(false),
    constituents:  None,
  })
  .await
  .expect("create build 1");

  let build2 = fc_common::repo::builds::create(&pool, CreateBuild {
    evaluation_id: eval.id,
    job_name:      "world".to_string(),
    drv_path:      "/nix/store/e2e000-world.drv".to_string(),
    system:        Some("x86_64-linux".to_string()),
    outputs:       Some(serde_json::json!({"out": "/nix/store/e2e000-world"})),
    is_aggregate:  Some(false),
    constituents:  None,
  })
  .await
  .expect("create build 2");

  assert_eq!(build1.status, BuildStatus::Pending);
  assert_eq!(build2.status, BuildStatus::Pending);

  // 7. Create build dependency (hello depends on world)
  fc_common::repo::build_dependencies::create(&pool, build1.id, build2.id)
    .await
    .expect("create dependency");

  // 8. Verify dependency check: build1 deps NOT complete (world is still
  //    pending)
  let deps_complete =
    fc_common::repo::build_dependencies::all_deps_completed(&pool, build1.id)
      .await
      .expect("check deps");
  assert!(!deps_complete, "deps should NOT be complete yet");

  // 9. Complete build2 (world)
  fc_common::repo::builds::start(&pool, build2.id)
    .await
    .expect("start build2");
  fc_common::repo::builds::complete(
    &pool,
    build2.id,
    BuildStatus::Succeeded,
    None,
    Some("/nix/store/e2e000-world"),
    None,
  )
  .await
  .expect("complete build2");

  // 10. Now build1 deps should be complete
  let deps_complete =
    fc_common::repo::build_dependencies::all_deps_completed(&pool, build1.id)
      .await
      .expect("check deps again");
  assert!(deps_complete, "deps should be complete after build2 done");

  // 11. Complete build1 (hello)
  fc_common::repo::builds::start(&pool, build1.id)
    .await
    .expect("start build1");

  let step = fc_common::repo::build_steps::create(&pool, CreateBuildStep {
    build_id:    build1.id,
    step_number: 1,
    command:     "nix build /nix/store/e2e000-hello.drv".to_string(),
  })
  .await
  .expect("create step");

  fc_common::repo::build_steps::complete(
    &pool,
    step.id,
    0,
    Some("built!"),
    None,
  )
  .await
  .expect("complete step");

  fc_common::repo::build_products::create(&pool, CreateBuildProduct {
    build_id:     build1.id,
    name:         "out".to_string(),
    path:         "/nix/store/e2e000-hello".to_string(),
    sha256_hash:  Some("abcdef1234567890".to_string()),
    file_size:    Some(12345),
    content_type: None,
    is_directory: true,
  })
  .await
  .expect("create product");

  fc_common::repo::builds::complete(
    &pool,
    build1.id,
    BuildStatus::Succeeded,
    None,
    Some("/nix/store/e2e000-hello"),
    None,
  )
  .await
  .expect("complete build1");

  // 12. Mark evaluation as completed
  fc_common::repo::evaluations::update_status(
    &pool,
    eval.id,
    EvaluationStatus::Completed,
    None,
  )
  .await
  .expect("complete eval");

  // 13. Verify everything is in the expected state
  let final_eval = fc_common::repo::evaluations::get(&pool, eval.id)
    .await
    .expect("get eval");
  assert_eq!(final_eval.status, EvaluationStatus::Completed);

  let final_build1 = fc_common::repo::builds::get(&pool, build1.id)
    .await
    .expect("get build1");
  assert_eq!(final_build1.status, BuildStatus::Succeeded);
  assert_eq!(
    final_build1.build_output_path.as_deref(),
    Some("/nix/store/e2e000-hello")
  );

  let products =
    fc_common::repo::build_products::list_for_build(&pool, build1.id)
      .await
      .expect("list products");
  assert_eq!(products.len(), 1);
  assert_eq!(products[0].name, "out");

  let steps = fc_common::repo::build_steps::list_for_build(&pool, build1.id)
    .await
    .expect("list steps");
  assert_eq!(steps.len(), 1);
  assert_eq!(steps[0].exit_code, Some(0));

  // 14. Verify build stats reflect our changes
  let build_stats = fc_common::repo::builds::get_stats(&pool)
    .await
    .expect("get stats");
  assert!(build_stats.completed_builds.unwrap_or(0) >= 2);

  // 15. Create a channel and verify it works
  let channel = fc_common::repo::channels::create(&pool, CreateChannel {
    project_id: project.id,
    name:       "stable".to_string(),
    jobset_id:  jobset.id,
  })
  .await
  .expect("create channel");

  let channels = fc_common::repo::channels::list_all(&pool)
    .await
    .expect("list channels");
  assert!(channels.iter().any(|c| c.id == channel.id));

  // 16. Test the HTTP API layer
  let config = fc_common::config::Config::default();
  let server_config = config.server.clone();
  let state = fc_server::state::AppState {
    pool: pool.clone(),
    config,
    sessions: std::sync::Arc::new(dashmap::DashMap::new()),
    http_client: reqwest::Client::new(),
    csrf_secret: std::sync::Arc::new([0u8; 32]),
  };
  let app = fc_server::routes::router(state, &server_config);

  // GET /health
  let resp = app
    .clone()
    .oneshot(
      Request::builder()
        .uri("/health")
        .body(Body::empty())
        .unwrap(),
    )
    .await
    .unwrap();
  assert_eq!(resp.status(), StatusCode::OK);

  // GET /api/v1/projects/{id}
  let resp = app
    .clone()
    .oneshot(
      Request::builder()
        .uri(format!("/api/v1/projects/{}", project.id))
        .body(Body::empty())
        .unwrap(),
    )
    .await
    .unwrap();
  assert_eq!(resp.status(), StatusCode::OK);

  // GET /api/v1/builds/{id}
  let resp = app
    .clone()
    .oneshot(
      Request::builder()
        .uri(format!("/api/v1/builds/{}", build1.id))
        .body(Body::empty())
        .unwrap(),
    )
    .await
    .unwrap();
  assert_eq!(resp.status(), StatusCode::OK);

  // GET / (dashboard)
  let resp = app
    .clone()
    .oneshot(Request::builder().uri("/").body(Body::empty()).unwrap())
    .await
    .unwrap();
  assert_eq!(resp.status(), StatusCode::OK);
  let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
    .await
    .unwrap();
  let body_str = String::from_utf8(body.to_vec()).unwrap();
  assert!(body_str.contains("Dashboard"));

  // Clean up
  let _ = fc_common::repo::projects::delete(&pool, project.id).await;
}
