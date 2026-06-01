//! Per-request CSRF helpers used by every form-posting handler in the
//! dashboard.

use axum::{
  http::{Extensions, StatusCode},
  response::{IntoResponse, Response},
};

pub(super) fn csrf_from(extensions: &Extensions) -> String {
  extensions
    .get::<crate::state::CsrfToken>()
    .map(|t| t.0.clone())
    .unwrap_or_default()
}

#[expect(
  clippy::result_large_err,
  reason = "dashboard handlers return axum Response directly; boxing would \
            add noise at call sites"
)]
pub(super) fn check_csrf(
  extensions: &Extensions,
  submitted: &str,
) -> Result<(), Response> {
  use subtle::ConstantTimeEq;
  let expected = csrf_from(extensions);
  if expected.is_empty()
    || expected.as_bytes().ct_eq(submitted.as_bytes()).unwrap_u8() != 1
  {
    return Err(
      (StatusCode::FORBIDDEN, "Invalid or missing CSRF token").into_response(),
    );
  }
  Ok(())
}
