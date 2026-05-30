//! Input validation utilities for circus

use std::sync::LazyLock;

use regex::Regex;

/// Username validation: 3-32 chars, alphanumeric + underscore + hyphen
static USERNAME_REGEX: LazyLock<Regex> = LazyLock::new(|| {
  #[expect(
    clippy::expect_used,
    reason = "static regex initializer - panic on invalid regex is intentional"
  )]
  {
    Regex::new(r"^[a-zA-Z0-9_-]{3,32}$")
      .expect("Invalid username regex pattern")
  }
});

/// Validation errors
#[derive(Debug, Clone)]
pub struct ValidationError {
  pub field:   String,
  pub message: String,
}

impl std::fmt::Display for ValidationError {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    write!(f, "{}: {}", self.field, self.message)
  }
}

impl std::error::Error for ValidationError {}

/// Validate username format
/// Requirements:
/// - 3-32 characters
/// - Alphanumeric, underscore, hyphen only
///
/// # Errors
///
/// Returns error if username format is invalid.
pub fn validate_username(username: &str) -> Result<(), ValidationError> {
  if username.is_empty() {
    return Err(ValidationError {
      field:   "username".to_string(),
      message: "Username is required".to_string(),
    });
  }

  if !USERNAME_REGEX.is_match(username) {
    return Err(ValidationError {
      field:   "username".to_string(),
      message: "Username must be 3-32 characters and contain only letters, \
                numbers, underscores, and hyphens"
        .to_string(),
    });
  }

  Ok(())
}

/// Validate email address.
///
/// When `regex` is `None`, only structural checks are applied: non-empty,
/// max 255 characters, and must contain `@`. When a `Regex` is provided it
/// is used instead of the structural check, allowing operators to enforce a
/// stricter or organisation-specific pattern (e.g. requiring a real TLD).
///
/// # Errors
///
/// Returns error if the email fails validation.
pub fn validate_email(
  email: &str,
  regex: Option<&Regex>,
) -> Result<(), ValidationError> {
  if email.is_empty() {
    return Err(ValidationError {
      field:   "email".to_string(),
      message: "Email is required".to_string(),
    });
  }

  if email.len() > 255 {
    return Err(ValidationError {
      field:   "email".to_string(),
      message: "Email must be 255 characters or less".to_string(),
    });
  }

  match regex {
    Some(re) => {
      if !re.is_match(email) {
        return Err(ValidationError {
          field:   "email".to_string(),
          message: "Invalid email format".to_string(),
        });
      }
    },
    None => {
      if !email.contains('@') {
        return Err(ValidationError {
          field:   "email".to_string(),
          message: "Email must contain '@'".to_string(),
        });
      }
    },
  }

  Ok(())
}

/// Validate password strength
/// Requirements:
/// - At least 12 characters
/// - At least one uppercase letter
/// - At least one lowercase letter
/// - At least one number
/// - At least one special character
///
/// # Errors
///
/// Returns error if password does not meet requirements.
pub fn validate_password(password: &str) -> Result<(), ValidationError> {
  if password.len() < 12 {
    return Err(ValidationError {
      field:   "password".to_string(),
      message: "Password must be at least 12 characters".to_string(),
    });
  }

  let has_upper = password.chars().any(|c| c.is_ascii_uppercase());
  let has_lower = password.chars().any(|c| c.is_ascii_lowercase());
  let has_digit = password.chars().any(|c| c.is_ascii_digit());
  let has_special = password.chars().any(|c| !c.is_ascii_alphanumeric());

  if !has_upper {
    return Err(ValidationError {
      field:   "password".to_string(),
      message: "Password must contain at least one uppercase letter"
        .to_string(),
    });
  }

  if !has_lower {
    return Err(ValidationError {
      field:   "password".to_string(),
      message: "Password must contain at least one lowercase letter"
        .to_string(),
    });
  }

  if !has_digit {
    return Err(ValidationError {
      field:   "password".to_string(),
      message: "Password must contain at least one number".to_string(),
    });
  }

  if !has_special {
    return Err(ValidationError {
      field:   "password".to_string(),
      message: "Password must contain at least one special character"
        .to_string(),
    });
  }

  Ok(())
}

/// Validate role against allowed roles
///
/// # Errors
///
/// Returns error if role is not in the allowed list.
pub fn validate_role(
  role: &str,
  allowed: &[&str],
) -> Result<(), ValidationError> {
  if role.is_empty() {
    return Err(ValidationError {
      field:   "role".to_string(),
      message: "Role is required".to_string(),
    });
  }

  if !allowed.contains(&role) {
    return Err(ValidationError {
      field:   "role".to_string(),
      message: format!("Invalid role. Must be one of: {}", allowed.join(", ")),
    });
  }

  Ok(())
}

/// Validate full name (optional field)
/// - Max 255 characters
/// - Must not contain control characters
///
/// # Errors
///
/// Returns error if full name contains invalid characters or is too long.
pub fn validate_full_name(name: &str) -> Result<(), ValidationError> {
  if name.len() > 255 {
    return Err(ValidationError {
      field:   "full_name".to_string(),
      message: "Full name must be 255 characters or less".to_string(),
    });
  }

  if name.chars().any(char::is_control) {
    return Err(ValidationError {
      field:   "full_name".to_string(),
      message: "Full name cannot contain control characters".to_string(),
    });
  }

  Ok(())
}

/// Validate job name
/// Requirements:
/// - 1-255 characters
/// - Alphanumeric + common path characters
///
/// # Errors
///
/// Returns error if job name format is invalid.
pub fn validate_job_name(name: &str) -> Result<(), ValidationError> {
  if name.is_empty() {
    return Err(ValidationError {
      field:   "job_name".to_string(),
      message: "Job name is required".to_string(),
    });
  }

  if name.len() > 255 {
    return Err(ValidationError {
      field:   "job_name".to_string(),
      message: "Job name must be 255 characters or less".to_string(),
    });
  }

  // Allow alphanumeric, hyphen, underscore, dot, and path separators
  let valid_chars: std::collections::HashSet<char> =
    "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789_-./"
      .chars()
      .collect();

  if !name.chars().all(|c| valid_chars.contains(&c)) {
    return Err(ValidationError {
      field:   "job_name".to_string(),
      message: "Job name contains invalid characters".to_string(),
    });
  }

  Ok(())
}
