//! Capability-based authorisation. Every gateable action in the server
//! maps to one [`Permission`] variant, and every check - API handler,
//! dashboard mutation, template gating - flows through this module. The
//! role string a permission resolves to (the value stored in
//! `User.role` and `ApiKey.role`) lives only in [`Permission::role_str`],
//! so a typo in a role string is a compile error and the server-side
//! enforcement and the UI's button-gating cannot drift apart.

use axum::http::{Extensions, StatusCode};
use circus_common::models::{ApiKey, User};

use crate::error::ApiError;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Permission {
  Admin,
  BumpToFront,
  CancelBuild,
  RestartJobs,
  CreateProjects,
  EvalJobset,
}

impl Permission {
  /// Role string stored on `User.role` / `ApiKey.role` for this
  /// capability. Anyone whose session role matches this string (or who
  /// holds the `admin` role, which satisfies every check) may exercise
  /// the capability.
  #[must_use]
  pub const fn role_str(self) -> &'static str {
    match self {
      Self::Admin => "admin",
      Self::BumpToFront => "bump-to-front",
      Self::CancelBuild => "cancel-build",
      Self::RestartJobs => "restart-jobs",
      Self::CreateProjects => "create-projects",
      Self::EvalJobset => "eval-jobset",
    }
  }
}

fn session_role(extensions: &Extensions) -> Option<&str> {
  if let Some(user) = extensions.get::<User>() {
    return Some(user.role.as_str());
  }
  extensions.get::<ApiKey>().map(|k| k.role.as_str())
}

/// Whether the authenticated session may exercise `permission`. Admin
/// sessions satisfy every check; anonymous sessions satisfy none.
#[must_use]
pub fn check(extensions: &Extensions, permission: Permission) -> bool {
  let Some(role) = session_role(extensions) else {
    return false;
  };
  role == Permission::Admin.role_str() || role == permission.role_str()
}

/// Reject the request with `UNAUTHORIZED` if the session is anonymous,
/// or `FORBIDDEN` if it is authenticated but lacks `permission`.
///
/// # Errors
///
/// Returns `UNAUTHORIZED` or `FORBIDDEN` per the rules above.
pub fn require(
  extensions: &Extensions,
  permission: Permission,
) -> Result<(), StatusCode> {
  let Some(role) = session_role(extensions) else {
    return Err(StatusCode::UNAUTHORIZED);
  };
  if role == Permission::Admin.role_str() || role == permission.role_str() {
    Ok(())
  } else {
    Err(StatusCode::FORBIDDEN)
  }
}

/// [`require`] wrapped in the JSON API's error shape so callers in
/// `routes/*` can `?` it directly from a handler returning
/// `Result<_, ApiError>`.
///
/// # Errors
///
/// See [`require`].
pub fn require_api(
  extensions: &Extensions,
  permission: Permission,
) -> Result<(), ApiError> {
  require(extensions, permission).map_err(|s| {
    ApiError(if s == StatusCode::FORBIDDEN {
      circus_common::CiError::Forbidden("Insufficient permissions".to_string())
    } else {
      circus_common::CiError::Unauthorized(
        "Authentication required".to_string(),
      )
    })
  })
}

/// Snapshot of every dashboard-visible capability for the current
/// session. Computed once per request and threaded into the template so
/// each page carries a single `permissions` field instead of accumulating
/// one `can_X: bool` per gateable action.
#[expect(
  clippy::struct_excessive_bools,
  reason = "one bool per Permission variant is the point of this snapshot; \
            templates read these fields directly"
)]
pub struct UiPermissions {
  pub admin:           bool,
  pub bump_to_front:   bool,
  pub cancel_build:    bool,
  pub restart_jobs:    bool,
  pub create_projects: bool,
  pub eval_jobset:     bool,
}

impl UiPermissions {
  #[must_use]
  pub fn from_extensions(extensions: &Extensions) -> Self {
    Self {
      admin:           check(extensions, Permission::Admin),
      bump_to_front:   check(extensions, Permission::BumpToFront),
      cancel_build:    check(extensions, Permission::CancelBuild),
      restart_jobs:    check(extensions, Permission::RestartJobs),
      create_projects: check(extensions, Permission::CreateProjects),
      eval_jobset:     check(extensions, Permission::EvalJobset),
    }
  }
}
