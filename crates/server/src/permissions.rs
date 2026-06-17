//! Capability-based authorisation. Every gateable action in the server
//! maps to one [`Permission`] variant, and every check - API handler,
//! dashboard mutation, template gating - flows through this module. The
//! role a permission resolves to lives only in [`Permission::role`], so the
//! server-side enforcement and the UI's button-gating cannot drift apart.

use axum::http::{Extensions, StatusCode};
use circus_common::{
  models::{ApiKey, User},
  roles::GlobalRole,
};

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
  #[must_use]
  pub const fn role(self) -> GlobalRole {
    match self {
      Self::Admin => GlobalRole::Admin,
      Self::BumpToFront => GlobalRole::BumpToFront,
      Self::CancelBuild => GlobalRole::CancelBuild,
      Self::RestartJobs => GlobalRole::RestartJobs,
      Self::CreateProjects => GlobalRole::CreateProjects,
      Self::EvalJobset => GlobalRole::EvalJobset,
    }
  }
}

fn session_role(extensions: &Extensions) -> Option<GlobalRole> {
  if let Some(user) = extensions.get::<User>() {
    return Some(user.role);
  }
  extensions.get::<ApiKey>().map(|k| k.role)
}

fn role_grants(role: GlobalRole, permission: Permission) -> bool {
  role == GlobalRole::Admin || role == permission.role()
}

/// Whether the authenticated session may exercise `permission`. Admin
/// sessions satisfy every check; anonymous sessions satisfy none.
#[must_use]
pub fn check(extensions: &Extensions, permission: Permission) -> bool {
  session_role(extensions).is_some_and(|role| role_grants(role, permission))
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
  match session_role(extensions) {
    None => Err(StatusCode::UNAUTHORIZED),
    Some(role) if role_grants(role, permission) => Ok(()),
    Some(_) => Err(StatusCode::FORBIDDEN),
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
