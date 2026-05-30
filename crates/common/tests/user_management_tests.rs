//! Integration tests for user management - CRUD, authentication, and
//! relationships. Requires `TEST_DATABASE_URL` to be set to a `PostgreSQL`
//! connection string.
#![expect(
  clippy::unwrap_used,
  clippy::expect_used,
  clippy::print_stdout,
  reason = "Fine in tests"
)]

use circus_common::{models::*, repo};
use uuid::Uuid;

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

#[tokio::test]
async fn test_user_crud() {
  let Some(pool) = get_pool().await else {
    return;
  };

  let username = format!("test-user-{}", Uuid::new_v4().simple());
  let email = format!("{username}@example.com");

  // Create user
  let user = repo::users::create(
    &pool,
    &CreateUser {
      username:  username.clone(),
      email:     email.clone(),
      full_name: Some("Test User".to_string()),
      password:  "secure_password_123".to_string(),
      role:      Some("admin".to_string()),
    },
    None,
  )
  .await
  .expect("create user");

  assert_eq!(user.username, username);
  assert_eq!(user.email, email);
  assert_eq!(user.full_name.as_deref(), Some("Test User"));
  assert_eq!(user.role, "admin");
  assert!(user.enabled);
  assert!(user.password_hash.is_some());

  // Get by ID
  let fetched = repo::users::get(&pool, user.id).await.expect("get user");
  assert_eq!(fetched.id, user.id);
  assert_eq!(fetched.username, username);

  // Get by username
  let by_username = repo::users::get_by_username(&pool, &username)
    .await
    .expect("get by username")
    .expect("user should exist");
  assert_eq!(by_username.id, user.id);

  // Get by email
  let by_email = repo::users::get_by_email(&pool, &email)
    .await
    .expect("get by email")
    .expect("user should exist");
  assert_eq!(by_email.id, user.id);

  // List users
  let users = repo::users::list(&pool, 100, 0).await.expect("list users");
  assert!(users.iter().any(|u| u.id == user.id));

  // Count users
  let count = repo::users::count(&pool).await.expect("count users");
  assert!(count > 0);

  // Update email
  let new_email = format!("updated-{email}");
  let updated = repo::users::update_email(&pool, user.id, &new_email, None)
    .await
    .expect("update email");
  assert_eq!(updated.email, new_email);

  // Update full name
  repo::users::update_full_name(&pool, user.id, Some("Updated Name"))
    .await
    .expect("update full name");
  let updated = repo::users::get(&pool, user.id).await.expect("get updated");
  assert_eq!(updated.full_name.as_deref(), Some("Updated Name"));

  // Update role
  repo::users::update_role(&pool, user.id, "read-only")
    .await
    .expect("update role");
  let updated = repo::users::get(&pool, user.id).await.expect("get updated");
  assert_eq!(updated.role, "read-only");

  // Disable user
  repo::users::set_enabled(&pool, user.id, false)
    .await
    .expect("disable user");
  let updated = repo::users::get(&pool, user.id).await.expect("get updated");
  assert!(!updated.enabled);

  // Enable user
  repo::users::set_enabled(&pool, user.id, true)
    .await
    .expect("enable user");
  let updated = repo::users::get(&pool, user.id).await.expect("get updated");
  assert!(updated.enabled);

  // Set public dashboard
  repo::users::set_public_dashboard(&pool, user.id, true)
    .await
    .expect("set public dashboard");
  let updated = repo::users::get(&pool, user.id).await.expect("get updated");
  assert!(updated.public_dashboard);

  // Delete user
  repo::users::delete(&pool, user.id)
    .await
    .expect("delete user");

  // Verify deleted
  let result = repo::users::get(&pool, user.id).await;
  assert!(matches!(result, Err(circus_common::CiError::NotFound(_))));
}

#[tokio::test]
async fn test_user_authentication() {
  let Some(pool) = get_pool().await else {
    return;
  };

  let username = format!("auth-test-{}", Uuid::new_v4().simple());
  let password = "my_secret_password";

  // Create user
  let user = repo::users::create(
    &pool,
    &CreateUser {
      username:  username.clone(),
      email:     format!("{username}@example.com"),
      full_name: None,
      password:  password.to_string(),
      role:      None,
    },
    None,
  )
  .await
  .expect("create user");

  // Authenticate with correct credentials
  let auth_result = repo::users::authenticate(&pool, &LoginCredentials {
    username: username.clone(),
    password: password.to_string(),
  })
  .await;
  assert!(auth_result.is_ok());
  let auth_user = auth_result.unwrap();
  assert_eq!(auth_user.id, user.id);
  assert!(auth_user.last_login_at.is_some());

  // Authenticate with wrong password
  let wrong_auth = repo::users::authenticate(&pool, &LoginCredentials {
    username: username.clone(),
    password: "wrong_password".to_string(),
  })
  .await;
  assert!(matches!(
    wrong_auth,
    Err(circus_common::CiError::Unauthorized(_))
  ));

  // Authenticate with wrong username
  let wrong_user = repo::users::authenticate(&pool, &LoginCredentials {
    username: "nonexistent".to_string(),
    password: password.to_string(),
  })
  .await;
  assert!(matches!(
    wrong_user,
    Err(circus_common::CiError::Unauthorized(_))
  ));

  // Authenticate disabled user
  repo::users::set_enabled(&pool, user.id, false)
    .await
    .expect("disable user");
  let disabled_auth = repo::users::authenticate(&pool, &LoginCredentials {
    username: username.clone(),
    password: password.to_string(),
  })
  .await;
  assert!(matches!(
    disabled_auth,
    Err(circus_common::CiError::Unauthorized(_))
  ));

  // Cleanup
  let _ = repo::users::delete(&pool, user.id).await;
}

#[tokio::test]
async fn test_password_hashing() {
  use circus_common::repo::users::{hash_password, verify_password};

  let password = "test_password_123";

  // Hash password
  let hash = hash_password(password).expect("hash password");
  assert!(!hash.is_empty());
  assert_ne!(hash, password);

  // Verify correct password
  let verified = verify_password(password, &hash).expect("verify password");
  assert!(verified);

  // Verify wrong password
  let wrong = verify_password("wrong_password", &hash).expect("verify wrong");
  assert!(!wrong);

  // Hash is different each time (due to salt)
  let hash2 = hash_password(password).expect("hash again");
  assert_ne!(hash, hash2);

  // But both verify correctly
  assert!(verify_password(password, &hash2).expect("verify hash2"));
}

#[tokio::test]
async fn test_user_unique_constraints() {
  let Some(pool) = get_pool().await else {
    return;
  };

  let username = format!("unique-{}", Uuid::new_v4().simple());
  let email = format!("{username}@example.com");

  // Create first user
  let _ = repo::users::create(
    &pool,
    &CreateUser {
      username:  username.clone(),
      email:     email.clone(),
      full_name: None,
      password:  "password".to_string(),
      role:      None,
    },
    None,
  )
  .await
  .expect("create first user");

  // Try to create with same username
  let result = repo::users::create(
    &pool,
    &CreateUser {
      username:  username.clone(),
      email:     format!("other-{email}"),
      full_name: None,
      password:  "password".to_string(),
      role:      None,
    },
    None,
  )
  .await;
  assert!(matches!(result, Err(circus_common::CiError::Conflict(_))));

  // Try to create with same email
  let result = repo::users::create(
    &pool,
    &CreateUser {
      username:  format!("other-{username}"),
      email:     email.clone(),
      full_name: None,
      password:  "password".to_string(),
      role:      None,
    },
    None,
  )
  .await;
  assert!(matches!(result, Err(circus_common::CiError::Conflict(_))));

  // Cleanup
  let user = repo::users::get_by_username(&pool, &username)
    .await
    .unwrap()
    .unwrap();
  let _ = repo::users::delete(&pool, user.id).await;
}

#[tokio::test]
async fn test_oauth_user_creation() {
  let Some(pool) = get_pool().await else {
    return;
  };

  let username = format!("oauth-user-{}", Uuid::new_v4().simple());
  let email = format!("{username}@github.com");
  let oauth_provider_id = format!("github_{}", Uuid::new_v4().simple());

  // Create OAuth user
  let user = repo::users::upsert_oauth_user(
    &pool,
    &username,
    Some(email.as_str()),
    UserType::Github,
    &oauth_provider_id,
    None,
  )
  .await
  .expect("create OAuth user");

  assert!(user.username.contains(&username));
  assert_eq!(user.user_type, UserType::Github);
  assert!(user.password_hash.is_none()); // OAuth users have no password

  // Update same user (should not create duplicate)
  let updated = repo::users::upsert_oauth_user(
    &pool,
    &username,
    Some(email.as_str()),
    UserType::Github,
    &oauth_provider_id,
    None,
  )
  .await
  .expect("update OAuth user");

  assert_eq!(updated.id, user.id);

  // Cleanup
  let _ = repo::users::delete(&pool, user.id).await;
}

// Starred Jobs Tests

#[tokio::test]
async fn test_starred_jobs_crud() {
  let Some(pool) = get_pool().await else {
    return;
  };

  // Create prerequisite data
  let user = repo::users::create(
    &pool,
    &CreateUser {
      username:  format!("star-user-{}", Uuid::new_v4().simple()),
      email:     format!("star-{}@example.com", Uuid::new_v4().simple()),
      full_name: None,
      password:  "password".to_string(),
      role:      None,
    },
    None,
  )
  .await
  .expect("create user");

  let project = repo::projects::create(&pool, CreateProject {
    name:           format!("star-project-{}", Uuid::new_v4().simple()),
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

  // Star a job
  let starred = repo::starred_jobs::create(&pool, user.id, &CreateStarredJob {
    project_id: project.id,
    jobset_id:  Some(jobset.id),
    job_name:   "hello-world".to_string(),
  })
  .await
  .expect("star job");

  assert_eq!(starred.user_id, user.id);
  assert_eq!(starred.project_id, project.id);
  assert_eq!(starred.jobset_id, Some(jobset.id));
  assert_eq!(starred.job_name, "hello-world");

  // Check is starred
  let is_starred = repo::starred_jobs::is_starred(
    &pool,
    user.id,
    project.id,
    Some(jobset.id),
    "hello-world",
  )
  .await
  .expect("check is starred");
  assert!(is_starred);

  // List starred jobs
  let starred_list = repo::starred_jobs::list_for_user(&pool, user.id, 100, 0)
    .await
    .expect("list starred");
  assert_eq!(starred_list.len(), 1);
  assert_eq!(starred_list[0].id, starred.id);

  // Count starred jobs
  let count = repo::starred_jobs::count_for_user(&pool, user.id)
    .await
    .expect("count starred");
  assert_eq!(count, 1);

  // Can't star same job twice
  let duplicate =
    repo::starred_jobs::create(&pool, user.id, &CreateStarredJob {
      project_id: project.id,
      jobset_id:  Some(jobset.id),
      job_name:   "hello-world".to_string(),
    })
    .await;
  assert!(matches!(
    duplicate,
    Err(circus_common::CiError::Conflict(_))
  ));

  // Delete starred job
  repo::starred_jobs::delete(&pool, starred.id)
    .await
    .expect("unstar job");

  // Verify deleted
  let is_starred = repo::starred_jobs::is_starred(
    &pool,
    user.id,
    project.id,
    Some(jobset.id),
    "hello-world",
  )
  .await
  .expect("check is starred");
  assert!(!is_starred);

  // Cleanup
  let _ = repo::projects::delete(&pool, project.id).await;
  let _ = repo::users::delete(&pool, user.id).await;
}

#[tokio::test]
async fn test_starred_jobs_delete_by_job() {
  let Some(pool) = get_pool().await else {
    return;
  };

  // Setup
  let user = repo::users::create(
    &pool,
    &CreateUser {
      username:  format!("del-user-{}", Uuid::new_v4().simple()),
      email:     format!("del-{}@example.com", Uuid::new_v4().simple()),
      full_name: None,
      password:  "password".to_string(),
      role:      None,
    },
    None,
  )
  .await
  .expect("create user");

  let project = repo::projects::create(&pool, CreateProject {
    name:           format!("del-project-{}", Uuid::new_v4().simple()),
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

  // Star a job
  let _ = repo::starred_jobs::create(&pool, user.id, &CreateStarredJob {
    project_id: project.id,
    jobset_id:  Some(jobset.id),
    job_name:   "test-job".to_string(),
  })
  .await
  .expect("star job");

  // Delete by job details
  repo::starred_jobs::delete_by_job(
    &pool,
    user.id,
    project.id,
    Some(jobset.id),
    "test-job",
  )
  .await
  .expect("delete by job");

  // Verify deleted
  let count = repo::starred_jobs::count_for_user(&pool, user.id)
    .await
    .expect("count");
  assert_eq!(count, 0);

  // Cleanup
  let _ = repo::projects::delete(&pool, project.id).await;
  let _ = repo::users::delete(&pool, user.id).await;
}

// Project Members Tests

#[tokio::test]
async fn test_project_members_crud() {
  let Some(pool) = get_pool().await else {
    return;
  };

  // Setup
  let user = repo::users::create(
    &pool,
    &CreateUser {
      username:  format!("member-user-{}", Uuid::new_v4().simple()),
      email:     format!("member-{}@example.com", Uuid::new_v4().simple()),
      full_name: None,
      password:  "password".to_string(),
      role:      None,
    },
    None,
  )
  .await
  .expect("create user");

  let project = repo::projects::create(&pool, CreateProject {
    name:           format!("member-project-{}", Uuid::new_v4().simple()),
    description:    None,
    repository_url: "https://github.com/test/repo".to_string(),
  })
  .await
  .expect("create project");

  // Add member
  let member =
    repo::project_members::create(&pool, project.id, &CreateProjectMember {
      user_id: user.id,
      role:    "maintainer".to_string(),
    })
    .await
    .expect("add member");

  assert_eq!(member.project_id, project.id);
  assert_eq!(member.user_id, user.id);
  assert_eq!(member.role, "maintainer");

  // Get by ID
  let fetched = repo::project_members::get(&pool, member.id)
    .await
    .expect("get member");
  assert_eq!(fetched.id, member.id);

  // Get by project and user
  let by_ids =
    repo::project_members::get_by_project_and_user(&pool, project.id, user.id)
      .await
      .expect("get by ids")
      .expect("member should exist");
  assert_eq!(by_ids.id, member.id);

  // List for project
  let members = repo::project_members::list_for_project(&pool, project.id)
    .await
    .expect("list members");
  assert_eq!(members.len(), 1);
  assert_eq!(members[0].id, member.id);

  // List for user
  let user_projects = repo::project_members::list_for_user(&pool, user.id)
    .await
    .expect("list user projects");
  assert_eq!(user_projects.len(), 1);
  assert_eq!(user_projects[0].project_id, project.id);

  // Update role
  let updated =
    repo::project_members::update(&pool, member.id, &UpdateProjectMember {
      role: Some("admin".to_string()),
    })
    .await
    .expect("update role");
  assert_eq!(updated.role, "admin");

  // Can't add duplicate member
  let duplicate =
    repo::project_members::create(&pool, project.id, &CreateProjectMember {
      user_id: user.id,
      role:    "member".to_string(),
    })
    .await;
  assert!(matches!(
    duplicate,
    Err(circus_common::CiError::Conflict(_))
  ));

  // Delete member
  repo::project_members::delete(&pool, member.id)
    .await
    .expect("remove member");

  // Verify deleted
  let result = repo::project_members::get(&pool, member.id).await;
  assert!(matches!(result, Err(circus_common::CiError::NotFound(_))));

  // Cleanup
  let _ = repo::projects::delete(&pool, project.id).await;
  let _ = repo::users::delete(&pool, user.id).await;
}

#[tokio::test]
async fn test_project_members_permissions() {
  let Some(pool) = get_pool().await else {
    return;
  };

  // Setup
  let admin_user = repo::users::create(
    &pool,
    &CreateUser {
      username:  format!("admin-user-{}", Uuid::new_v4().simple()),
      email:     format!("admin-{}@example.com", Uuid::new_v4().simple()),
      full_name: None,
      password:  "password".to_string(),
      role:      None,
    },
    None,
  )
  .await
  .expect("create admin user");

  let maintainer_user = repo::users::create(
    &pool,
    &CreateUser {
      username:  format!("maint-user-{}", Uuid::new_v4().simple()),
      email:     format!("maint-{}@example.com", Uuid::new_v4().simple()),
      full_name: None,
      password:  "password".to_string(),
      role:      None,
    },
    None,
  )
  .await
  .expect("create maintainer user");

  let member_user = repo::users::create(
    &pool,
    &CreateUser {
      username:  format!("member-user-{}", Uuid::new_v4().simple()),
      email:     format!("mem-{}@example.com", Uuid::new_v4().simple()),
      full_name: None,
      password:  "password".to_string(),
      role:      None,
    },
    None,
  )
  .await
  .expect("create member user");

  let project = repo::projects::create(&pool, CreateProject {
    name:           format!("perm-project-{}", Uuid::new_v4().simple()),
    description:    None,
    repository_url: "https://github.com/test/repo".to_string(),
  })
  .await
  .expect("create project");

  // Add members with different roles
  repo::project_members::create(&pool, project.id, &CreateProjectMember {
    user_id: admin_user.id,
    role:    "admin".to_string(),
  })
  .await
  .expect("add admin");

  repo::project_members::create(&pool, project.id, &CreateProjectMember {
    user_id: maintainer_user.id,
    role:    "maintainer".to_string(),
  })
  .await
  .expect("add maintainer");

  repo::project_members::create(&pool, project.id, &CreateProjectMember {
    user_id: member_user.id,
    role:    "member".to_string(),
  })
  .await
  .expect("add member");

  // Check permissions - admin has all permissions
  assert!(
    repo::project_members::check_permission(
      &pool,
      project.id,
      admin_user.id,
      "member"
    )
    .await
    .expect("check admin")
  );
  assert!(
    repo::project_members::check_permission(
      &pool,
      project.id,
      admin_user.id,
      "maintainer"
    )
    .await
    .expect("check admin maintainer")
  );
  assert!(
    repo::project_members::check_permission(
      &pool,
      project.id,
      admin_user.id,
      "admin"
    )
    .await
    .expect("check admin admin")
  );

  // Maintainer has member and maintainer permissions
  assert!(
    repo::project_members::check_permission(
      &pool,
      project.id,
      maintainer_user.id,
      "member"
    )
    .await
    .expect("check maintainer member")
  );
  assert!(
    repo::project_members::check_permission(
      &pool,
      project.id,
      maintainer_user.id,
      "maintainer"
    )
    .await
    .expect("check maintainer maintainer")
  );
  assert!(
    !repo::project_members::check_permission(
      &pool,
      project.id,
      maintainer_user.id,
      "admin"
    )
    .await
    .expect("check maintainer admin")
  );

  // Regular member only has member permission
  assert!(
    repo::project_members::check_permission(
      &pool,
      project.id,
      member_user.id,
      "member"
    )
    .await
    .expect("check member")
  );
  assert!(
    !repo::project_members::check_permission(
      &pool,
      project.id,
      member_user.id,
      "maintainer"
    )
    .await
    .expect("check member maintainer")
  );
  assert!(
    !repo::project_members::check_permission(
      &pool,
      project.id,
      member_user.id,
      "admin"
    )
    .await
    .expect("check member admin")
  );

  // Non-member has no permissions
  let non_member = repo::users::create(
    &pool,
    &CreateUser {
      username:  format!("non-member-{}", Uuid::new_v4().simple()),
      email:     format!("non-{}@example.com", Uuid::new_v4().simple()),
      full_name: None,
      password:  "password".to_string(),
      role:      None,
    },
    None,
  )
  .await
  .expect("create non-member");

  assert!(
    !repo::project_members::check_permission(
      &pool,
      project.id,
      non_member.id,
      "member"
    )
    .await
    .expect("check non-member")
  );

  // Cleanup
  let _ = repo::projects::delete(&pool, project.id).await;
  let _ = repo::users::delete(&pool, admin_user.id).await;
  let _ = repo::users::delete(&pool, maintainer_user.id).await;
  let _ = repo::users::delete(&pool, member_user.id).await;
  let _ = repo::users::delete(&pool, non_member.id).await;
}

#[tokio::test]
async fn test_user_not_found_errors() {
  let Some(pool) = get_pool().await else {
    return;
  };

  let fake_id = Uuid::new_v4();

  assert!(matches!(
    repo::users::get(&pool, fake_id).await,
    Err(circus_common::CiError::NotFound(_))
  ));

  assert!(matches!(
    repo::starred_jobs::get(&pool, fake_id).await,
    Err(circus_common::CiError::NotFound(_))
  ));

  assert!(matches!(
    repo::project_members::get(&pool, fake_id).await,
    Err(circus_common::CiError::NotFound(_))
  ));
}
