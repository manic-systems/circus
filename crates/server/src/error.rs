use axum::{
  http::StatusCode,
  response::{IntoResponse, Response},
};
use circus_common::CiError;
use serde_json::json;

pub struct ApiError(pub CiError);

impl From<CiError> for ApiError {
  fn from(err: CiError) -> Self {
    Self(err)
  }
}

impl IntoResponse for ApiError {
  fn into_response(self) -> Response {
    let (status, code, message) = match &self.0 {
      CiError::NotFound(msg) => {
        (StatusCode::NOT_FOUND, "NOT_FOUND", msg.clone())
      },
      CiError::Validation(msg) => {
        (StatusCode::BAD_REQUEST, "VALIDATION_ERROR", msg.clone())
      },
      CiError::Conflict(msg) => (StatusCode::CONFLICT, "CONFLICT", msg.clone()),
      CiError::Timeout(msg) => {
        (StatusCode::REQUEST_TIMEOUT, "TIMEOUT", msg.clone())
      },
      CiError::Unauthorized(msg) => {
        (StatusCode::UNAUTHORIZED, "UNAUTHORIZED", msg.clone())
      },
      CiError::Forbidden(msg) => {
        (StatusCode::FORBIDDEN, "FORBIDDEN", msg.clone())
      },
      CiError::NixEval(msg) => {
        let is_disk_full =
          msg.to_lowercase().contains("no space left on device")
            || msg.to_lowercase().contains("disk full")
            || msg.to_lowercase().contains("enospc")
            || msg.to_lowercase().contains("cannot create directory")
            || msg.to_lowercase().contains("sqlite");

        if is_disk_full {
          (
            StatusCode::INSUFFICIENT_STORAGE,
            "DISK_FULL",
            format!(
              "{msg}\n\nDISK SPACE ISSUE DETECTED:\nThe server has run out of \
               disk space. Please free up space:\n- Run `nix-collect-garbage \
               -d` to clean the Nix store\n- Clear the evaluator work \
               directory: `rm -rf /tmp/circus-evaluator/*`\n- Clear build \
               logs if configured"
            ),
          )
        } else {
          (
            StatusCode::UNPROCESSABLE_ENTITY,
            "NIX_EVAL_ERROR",
            msg.clone(),
          )
        }
      },
      CiError::DiskSpace(msg) => {
        (
          StatusCode::INSUFFICIENT_STORAGE,
          "DISK_FULL",
          format!(
            "{msg}\n\nDISK SPACE ISSUE:\nThe server is running low on disk \
             space. Please free up space:\n- Run `nix-collect-garbage -d` to \
             clean the Nix store\n- Clear the evaluator work directory\n- \
             Clear build logs if configured"
          ),
        )
      },
      CiError::Build(msg) => {
        (StatusCode::UNPROCESSABLE_ENTITY, "BUILD_ERROR", msg.clone())
      },
      CiError::Config(msg) => {
        (
          StatusCode::INTERNAL_SERVER_ERROR,
          "CONFIG_ERROR",
          msg.clone(),
        )
      },
      CiError::Database(e) => {
        tracing::error!(error = %e, "Database error in API handler");

        if e.to_string().contains("disk") || e.to_string().contains("space") {
          (
            StatusCode::INSUFFICIENT_STORAGE,
            "DISK_FULL",
            format!(
              "Database error: {e}\n\nDISK SPACE ISSUE:\nThe server is \
               running low on disk space."
            ),
          )
        } else {
          (
            StatusCode::INTERNAL_SERVER_ERROR,
            "DATABASE_ERROR",
            "Internal database error".to_string(),
          )
        }
      },
      CiError::Git(e) => {
        tracing::error!(error = %e, "Git error in API handler");
        (
          StatusCode::INTERNAL_SERVER_ERROR,
          "GIT_ERROR",
          format!("Git operation failed: {e}"),
        )
      },
      CiError::Serialization(e) => {
        tracing::error!(error = %e, "Serialization error in API handler");
        (
          StatusCode::INTERNAL_SERVER_ERROR,
          "SERIALIZATION_ERROR",
          format!("Data serialization error: {e}"),
        )
      },
      CiError::Io(e) => {
        tracing::error!(error = %e, "IO error in API handler");

        let msg = e.to_string();
        if msg.contains("No space left on device")
          || msg.contains("disk full")
          || msg.contains("ENOSPC")
        {
          (
            StatusCode::INSUFFICIENT_STORAGE,
            "DISK_FULL",
            format!(
              "IO error: {msg}\n\nDISK SPACE ISSUE DETECTED:\nThe server has \
               run out of disk space. Please free up space:\n- Run \
               `nix-collect-garbage -d` to clean the Nix store\n- Clear the \
               evaluator work directory: `rm -rf /tmp/circus-evaluator/*`\n- \
               Clear build logs if configured"
            ),
          )
        } else {
          (
            StatusCode::INTERNAL_SERVER_ERROR,
            "IO_ERROR",
            format!("IO error: {e}"),
          )
        }
      },
      CiError::Internal(msg) => {
        tracing::error!(message = %msg, "Internal error in API handler");

        if msg.to_lowercase().contains("disk")
          || msg.to_lowercase().contains("space")
          || msg.to_lowercase().contains("storage")
        {
          (
            StatusCode::INSUFFICIENT_STORAGE,
            "DISK_FULL",
            format!(
              "{msg}\n\nDISK SPACE ISSUE:\nThe server is running low on disk \
               space. Please free up space."
            ),
          )
        } else {
          (
            StatusCode::INTERNAL_SERVER_ERROR,
            "INTERNAL_ERROR",
            msg.clone(),
          )
        }
      },
    };

    if status.is_server_error() || status == StatusCode::INSUFFICIENT_STORAGE {
      tracing::warn!(
          status = %status,
          code = code,
          "API error response: {}",
          message
      );
    }

    let body = axum::Json(json!({ "error": message, "error_code": code }));
    (status, body).into_response()
  }
}
