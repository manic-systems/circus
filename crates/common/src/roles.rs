//! Role constants and validation for circus

/// Global role - full system access
pub const ROLE_ADMIN: &str = "admin";

/// Global role - view only
pub const ROLE_READ_ONLY: &str = "read-only";

/// Global role - can create projects
pub const ROLE_CREATE_PROJECTS: &str = "create-projects";

/// Global role - can evaluate jobsets
pub const ROLE_EVAL_JOBSET: &str = "eval-jobset";

/// Global role - can cancel builds
pub const ROLE_CANCEL_BUILD: &str = "cancel-build";

/// Global role - can restart jobs
pub const ROLE_RESTART_JOBS: &str = "restart-jobs";

/// Global role - can bump jobs to front of queue
pub const ROLE_BUMP_TO_FRONT: &str = "bump-to-front";

/// Project role - full project access
pub const PROJECT_ROLE_ADMIN: &str = "admin";

/// Project role - can manage project settings and builds
pub const PROJECT_ROLE_MAINTAINER: &str = "maintainer";

/// Project role - basic project access
pub const PROJECT_ROLE_MEMBER: &str = "member";

/// All valid global roles
pub const VALID_ROLES: &[&str] = &[
  ROLE_ADMIN,
  ROLE_READ_ONLY,
  ROLE_CREATE_PROJECTS,
  ROLE_EVAL_JOBSET,
  ROLE_CANCEL_BUILD,
  ROLE_RESTART_JOBS,
  ROLE_BUMP_TO_FRONT,
];

/// All valid project roles
pub const VALID_PROJECT_ROLES: &[&str] = &[
  PROJECT_ROLE_ADMIN,
  PROJECT_ROLE_MAINTAINER,
  PROJECT_ROLE_MEMBER,
];

/// Check if a global role is valid
#[must_use]
pub fn is_valid_role(role: &str) -> bool {
  VALID_ROLES.contains(&role)
}

/// Check if a project role is valid
#[must_use]
pub fn is_valid_project_role(role: &str) -> bool {
  VALID_PROJECT_ROLES.contains(&role)
}

/// Get the highest project role (for permission checks)
#[must_use]
pub fn project_role_level(role: &str) -> i32 {
  match role {
    PROJECT_ROLE_ADMIN => 3,
    PROJECT_ROLE_MAINTAINER => 2,
    PROJECT_ROLE_MEMBER => 1,
    _ => 0,
  }
}

/// Check if user has required project permission
/// Higher level roles automatically have lower level permissions
#[must_use]
pub fn has_project_permission(user_role: &str, required: &str) -> bool {
  let user_level = project_role_level(user_role);
  let required_level = project_role_level(required);
  user_level >= required_level
}
