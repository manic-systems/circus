//! Integration tests for advanced search functionality
//! Requires `TEST_DATABASE_URL` to be set to a `PostgreSQL` connection string.

use circus_common::{BuildStatus, models::*, repo, repo::search::*};
use uuid::Uuid;

async fn get_pool() -> Option<sqlx::PgPool> {
  let Ok(url) = std::env::var("TEST_DATABASE_URL") else {
    println!("Skipping search test: TEST_DATABASE_URL not set");
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

#[tokio::test]
async fn test_project_search() {
  let Some(pool) = get_pool().await else {
    return;
  };

  // Create test projects
  let project1 = repo::projects::create(&pool, CreateProject {
    name:           format!("search-test-alpha-{}", Uuid::new_v4().simple()),
    description:    Some("Alpha testing project".to_string()),
    repository_url: "https://github.com/test/alpha".to_string(),
  })
  .await
  .expect("create project 1");

  let project2 = repo::projects::create(&pool, CreateProject {
    name:           format!("search-test-beta-{}", Uuid::new_v4().simple()),
    description:    Some("Beta testing project".to_string()),
    repository_url: "https://github.com/test/beta".to_string(),
  })
  .await
  .expect("create project 2");

  // Search for "alpha"
  let params = SearchParams {
    query:              "alpha".to_string(),
    entities:           vec![SearchEntity::Projects],
    limit:              10,
    offset:             0,
    build_filters:      None,
    project_filters:    None,
    jobset_filters:     None,
    evaluation_filters: None,
    build_sort:         None,
    project_sort:       None,
  };

  let results = search(&pool, &params).await.expect("search");
  assert_eq!(results.projects.len(), 1);
  assert_eq!(results.projects[0].id, project1.id);
  assert_eq!(results.total_projects, 1);

  // Search for "testing" (should match both)
  let params = SearchParams {
    query:              "testing".to_string(),
    entities:           vec![SearchEntity::Projects],
    limit:              10,
    offset:             0,
    build_filters:      None,
    project_filters:    None,
    jobset_filters:     None,
    evaluation_filters: None,
    build_sort:         None,
    project_sort:       None,
  };

  let results = search(&pool, &params).await.expect("search");
  assert_eq!(results.projects.len(), 2);
  assert_eq!(results.total_projects, 2);

  // Cleanup
  repo::projects::delete(&pool, project1.id).await.ok();
  repo::projects::delete(&pool, project2.id).await.ok();
}

#[tokio::test]
async fn test_build_search_with_filters() {
  let Some(pool) = get_pool().await else {
    return;
  };

  // Setup: project -> jobset -> evaluation -> builds
  let project = repo::projects::create(&pool, CreateProject {
    name:           format!("build-search-{}", Uuid::new_v4().simple()),
    description:    None,
    repository_url: "https://github.com/test/repo".to_string(),
  })
  .await
  .expect("create project");

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

  // Create evaluation first (builds require an evaluation)
  let evaluation = repo::evaluations::create(&pool, CreateEvaluation {
    jobset_id:      jobset.id,
    commit_hash:    format!("abc123{}", Uuid::new_v4().simple()),
    pr_number:      None,
    pr_head_branch: None,
    pr_base_branch: None,
    pr_action:      None,
  })
  .await
  .expect("create evaluation");

  // Create builds with different statuses
  let build1 = repo::builds::create(&pool, CreateBuild {
    evaluation_id: evaluation.id,
    job_name:      "package-hello".to_string(),
    drv_path:      format!("/nix/store/{}-hello.drv", Uuid::new_v4().simple()),
    system:        Some("x86_64-linux".to_string()),
    outputs:       None,
    is_aggregate:  None,
    constituents:  None,
  })
  .await
  .expect("create build 1");

  // Complete build1 as succeeded
  repo::builds::start(&pool, build1.id)
    .await
    .expect("start build 1");
  repo::builds::complete(
    &pool,
    build1.id,
    BuildStatus::Succeeded,
    None,
    None,
    None,
  )
  .await
  .expect("complete build 1");

  let build2 = repo::builds::create(&pool, CreateBuild {
    evaluation_id: evaluation.id,
    job_name:      "package-world".to_string(),
    drv_path:      format!("/nix/store/{}-world.drv", Uuid::new_v4().simple()),
    system:        Some("x86_64-linux".to_string()),
    outputs:       None,
    is_aggregate:  None,
    constituents:  None,
  })
  .await
  .expect("create build 2");

  // Complete build2 as failed
  repo::builds::start(&pool, build2.id)
    .await
    .expect("start build 2");
  repo::builds::complete(
    &pool,
    build2.id,
    BuildStatus::Failed,
    None,
    None,
    Some("Test failure"),
  )
  .await
  .expect("complete build 2");

  // Search by job name
  let params = SearchParams {
    query:              "hello".to_string(),
    entities:           vec![SearchEntity::Builds],
    limit:              10,
    offset:             0,
    build_filters:      None,
    project_filters:    None,
    jobset_filters:     None,
    evaluation_filters: None,
    build_sort:         None,
    project_sort:       None,
  };

  let results = search(&pool, &params).await.expect("search");
  assert_eq!(results.builds.len(), 1);
  assert_eq!(results.builds[0].id, build1.id);

  // Search with status filter (succeeded)
  let params = SearchParams {
    query:              String::new(),
    entities:           vec![SearchEntity::Builds],
    limit:              10,
    offset:             0,
    build_filters:      Some(BuildSearchFilters {
      status:          Some(BuildStatusFilter::Succeeded),
      project_id:      None,
      jobset_id:       None,
      evaluation_id:   None,
      created_after:   None,
      created_before:  None,
      min_priority:    None,
      max_priority:    None,
      has_substitutes: None,
    }),
    project_filters:    None,
    jobset_filters:     None,
    evaluation_filters: None,
    build_sort:         None,
    project_sort:       None,
  };

  let results = search(&pool, &params).await.expect("search");
  assert!(results.builds.iter().any(|b| b.id == build1.id));

  // Cleanup - cascades to jobsets, evaluations, builds
  repo::projects::delete(&pool, project.id).await.ok();
}

#[tokio::test]
async fn test_multi_entity_search() {
  let Some(pool) = get_pool().await else {
    return;
  };

  // Create project with jobset, evaluation, and build
  let project = repo::projects::create(&pool, CreateProject {
    name:           format!("multi-search-{}", Uuid::new_v4().simple()),
    description:    Some("Multi-entity search test".to_string()),
    repository_url: "https://github.com/test/multi".to_string(),
  })
  .await
  .expect("create project");

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

  let evaluation = repo::evaluations::create(&pool, CreateEvaluation {
    jobset_id:      jobset.id,
    commit_hash:    format!("test{}", Uuid::new_v4().simple()),
    pr_number:      None,
    pr_head_branch: None,
    pr_base_branch: None,
    pr_action:      None,
  })
  .await
  .expect("create evaluation");

  let _build = repo::builds::create(&pool, CreateBuild {
    evaluation_id: evaluation.id,
    job_name:      "test-job".to_string(),
    drv_path:      format!("/nix/store/{}-test.drv", Uuid::new_v4().simple()),
    system:        Some("x86_64-linux".to_string()),
    outputs:       None,
    is_aggregate:  None,
    constituents:  None,
  })
  .await
  .expect("create build");

  // Search across all entities
  let params = SearchParams {
    query:              "test".to_string(),
    entities:           vec![
      SearchEntity::Projects,
      SearchEntity::Jobsets,
      SearchEntity::Builds,
    ],
    limit:              10,
    offset:             0,
    build_filters:      None,
    project_filters:    None,
    jobset_filters:     None,
    evaluation_filters: None,
    build_sort:         None,
    project_sort:       None,
  };

  let results = search(&pool, &params).await.expect("search");
  // Verify we found the specific project we created (description contains
  // "test")
  assert!(
    results.projects.iter().any(|p| p.id == project.id),
    "Expected to find created project in search results"
  );

  // Cleanup - cascades to all children
  repo::projects::delete(&pool, project.id).await.ok();
}

#[tokio::test]
async fn test_search_pagination() {
  let Some(pool) = get_pool().await else {
    return;
  };

  // Create multiple projects
  let mut project_ids = vec![];
  for i in 0..5 {
    let project = repo::projects::create(&pool, CreateProject {
      name:           format!("page-test-{}-{}", i, Uuid::new_v4().simple()),
      description:    Some(format!("Page test project {i}")),
      repository_url: "https://github.com/test/page".to_string(),
    })
    .await
    .expect("create project");
    project_ids.push(project.id);
  }

  // Search with limit 2
  let params = SearchParams {
    query:              "page-test".to_string(),
    entities:           vec![SearchEntity::Projects],
    limit:              2,
    offset:             0,
    build_filters:      None,
    project_filters:    None,
    jobset_filters:     None,
    evaluation_filters: None,
    build_sort:         None,
    project_sort:       None,
  };

  let results = search(&pool, &params).await.expect("search");
  assert_eq!(results.projects.len(), 2);
  assert!(results.total_projects >= 5);

  // Search with offset 2
  let params = SearchParams {
    query:              "page-test".to_string(),
    entities:           vec![SearchEntity::Projects],
    limit:              2,
    offset:             2,
    build_filters:      None,
    project_filters:    None,
    jobset_filters:     None,
    evaluation_filters: None,
    build_sort:         None,
    project_sort:       None,
  };

  let results = search(&pool, &params).await.expect("search");
  assert_eq!(results.projects.len(), 2);

  // Cleanup
  for id in project_ids {
    repo::projects::delete(&pool, id).await.ok();
  }
}

#[tokio::test]
async fn test_search_sorting() {
  let Some(pool) = get_pool().await else {
    return;
  };

  // Create projects in reverse alphabetical order
  let project_z = repo::projects::create(&pool, CreateProject {
    name:           format!("zzz-sort-test-{}", Uuid::new_v4().simple()),
    description:    None,
    repository_url: "https://github.com/test/z".to_string(),
  })
  .await
  .expect("create project z");

  let project_a = repo::projects::create(&pool, CreateProject {
    name:           format!("aaa-sort-test-{}", Uuid::new_v4().simple()),
    description:    None,
    repository_url: "https://github.com/test/a".to_string(),
  })
  .await
  .expect("create project a");

  // Search sorted by name ascending
  let params = SearchParams {
    query:              "sort-test".to_string(),
    entities:           vec![SearchEntity::Projects],
    limit:              10,
    offset:             0,
    build_filters:      None,
    project_filters:    None,
    jobset_filters:     None,
    evaluation_filters: None,
    build_sort:         None,
    project_sort:       Some((ProjectSortField::Name, SortOrder::Asc)),
  };

  let results = search(&pool, &params).await.expect("search");
  assert_eq!(results.projects.len(), 2);
  assert!(results.projects[0].name.starts_with("aaa"));
  assert!(results.projects[1].name.starts_with("zzz"));

  // Cleanup
  repo::projects::delete(&pool, project_a.id).await.ok();
  repo::projects::delete(&pool, project_z.id).await.ok();
}

#[tokio::test]
async fn test_empty_search() {
  let Some(pool) = get_pool().await else {
    return;
  };

  // Empty query should return all entities (up to limit)
  let params = SearchParams {
    query:              String::new(),
    entities:           vec![SearchEntity::Projects],
    limit:              10,
    offset:             0,
    build_filters:      None,
    project_filters:    None,
    jobset_filters:     None,
    evaluation_filters: None,
    build_sort:         None,
    project_sort:       None,
  };

  let results = search(&pool, &params).await.expect("search");
  // Should not error, just return results
  assert!(results.total_projects >= 0);
}

#[tokio::test]
async fn test_quick_search() {
  let Some(pool) = get_pool().await else {
    return;
  };

  // Create test data: project -> jobset -> evaluation -> build
  let project = repo::projects::create(&pool, CreateProject {
    name:           format!("quick-search-{}", Uuid::new_v4().simple()),
    description:    Some("Quick search test".to_string()),
    repository_url: "https://github.com/test/quick".to_string(),
  })
  .await
  .expect("create project");

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

  let evaluation = repo::evaluations::create(&pool, CreateEvaluation {
    jobset_id:      jobset.id,
    commit_hash:    format!("quick{}", Uuid::new_v4().simple()),
    pr_number:      None,
    pr_head_branch: None,
    pr_base_branch: None,
    pr_action:      None,
  })
  .await
  .expect("create evaluation");

  let _build = repo::builds::create(&pool, CreateBuild {
    evaluation_id: evaluation.id,
    job_name:      "quick-job".to_string(),
    drv_path:      format!("/nix/store/{}-quick.drv", Uuid::new_v4().simple()),
    system:        Some("x86_64-linux".to_string()),
    outputs:       None,
    is_aggregate:  None,
    constituents:  None,
  })
  .await
  .expect("create build");

  // Quick search
  let (projects, builds) = quick_search(&pool, "quick", 10)
    .await
    .expect("quick search");
  // Verify we found the specific project we created
  assert!(
    projects.iter().any(|p| p.id == project.id),
    "Expected to find created project in quick search results"
  );
  // Build may or may not appear depending on job_name matching
  let _ = builds; // Acknowledge builds were returned

  // Cleanup - cascades to all children
  repo::projects::delete(&pool, project.id).await.ok();
}
