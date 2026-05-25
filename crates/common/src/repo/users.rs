//! User repository - CRUD operations and authentication

use sqlx::PgPool;
use uuid::Uuid;

use crate::{
  error::{CiError, Result},
  models::{CreateUser, LoginCredentials, UpdateUser, User, UserType},
  roles::{ROLE_READ_ONLY, VALID_ROLES},
  validation::{
    validate_email,
    validate_full_name,
    validate_password,
    validate_role,
    validate_username,
  },
};

/// Hash a password using argon2id
///
/// # Errors
///
/// Returns error if password hashing fails.
pub fn hash_password(password: &str) -> Result<String> {
  use argon2::{
    Argon2,
    PasswordHasher,
    password_hash::{SaltString, rand_core::OsRng},
  };

  let salt = SaltString::generate(&mut OsRng);
  let argon2 = Argon2::default();
  argon2
    .hash_password(password.as_bytes(), &salt)
    .map(|h| h.to_string())
    .map_err(|e| CiError::Internal(format!("Password hashing failed: {e}")))
}

/// Verify a password against a hash
///
/// # Errors
///
/// Returns error if password hash parsing fails.
pub fn verify_password(password: &str, hash: &str) -> Result<bool> {
  use argon2::{Argon2, PasswordHash, PasswordVerifier};

  let parsed_hash = PasswordHash::new(hash)
    .map_err(|e| CiError::Internal(format!("Invalid password hash: {e}")))?;
  let argon2 = Argon2::default();
  Ok(
    argon2
      .verify_password(password.as_bytes(), &parsed_hash)
      .is_ok(),
  )
}

/// Create a new user with validation
///
/// # Errors
///
/// Returns error if validation fails or database insert fails.
pub async fn create(pool: &PgPool, data: &CreateUser) -> Result<User> {
  // Validate username
  validate_username(&data.username)
    .map_err(|e| CiError::Validation(e.to_string()))?;

  // Validate email
  validate_email(&data.email)
    .map_err(|e| CiError::Validation(e.to_string()))?;

  // Validate password
  validate_password(&data.password)
    .map_err(|e| CiError::Validation(e.to_string()))?;

  // Validate full name if provided
  if let Some(ref name) = data.full_name {
    validate_full_name(name).map_err(|e| CiError::Validation(e.to_string()))?;
  }

  // Validate role
  let role = data.role.as_deref().unwrap_or(ROLE_READ_ONLY);
  validate_role(role, VALID_ROLES)
    .map_err(|e| CiError::Validation(e.to_string()))?;

  let password_hash = hash_password(&data.password)?;

  sqlx::query_as::<_, User>(
    "INSERT INTO users (username, email, full_name, password_hash, role) \
     VALUES ($1, $2, $3, $4, $5) RETURNING *",
  )
  .bind(&data.username)
  .bind(&data.email)
  .bind(&data.full_name)
  .bind(&password_hash)
  .bind(role)
  .fetch_one(pool)
  .await
  .map_err(|e| {
    match &e {
      sqlx::Error::Database(db_err) if db_err.is_unique_violation() => {
        CiError::Conflict("Username or email already exists".to_string())
      },
      _ => CiError::Database(e),
    }
  })
}

/// Authenticate a user with username and password
///
/// # Errors
///
/// Returns error if credentials are invalid or database query fails.
pub async fn authenticate(
  pool: &PgPool,
  creds: &LoginCredentials,
) -> Result<User> {
  let user = sqlx::query_as::<_, User>(
    "SELECT * FROM users WHERE username = $1 AND enabled = true",
  )
  .bind(&creds.username)
  .fetch_one(pool)
  .await
  .map_err(|_| CiError::Unauthorized("Invalid credentials".to_string()))?;

  if let Some(ref hash) = user.password_hash {
    if verify_password(&creds.password, hash)? {
      // Update last login time
      if let Err(e) =
        sqlx::query("UPDATE users SET last_login_at = NOW() WHERE id = $1")
          .bind(user.id)
          .execute(pool)
          .await
      {
        tracing::warn!(user_id = %user.id, "Failed to update last_login_at: {e}");
      }
      Ok(user)
    } else {
      Err(CiError::Unauthorized("Invalid credentials".to_string()))
    }
  } else {
    Err(CiError::Unauthorized(
      "OAuth user - use OAuth login".to_string(),
    ))
  }
}

/// Get a user by ID
///
/// # Errors
///
/// Returns error if database query fails or user not found.
pub async fn get(pool: &PgPool, id: Uuid) -> Result<User> {
  sqlx::query_as::<_, User>("SELECT * FROM users WHERE id = $1")
    .bind(id)
    .fetch_one(pool)
    .await
    .map_err(|e| {
      match e {
        sqlx::Error::RowNotFound => {
          CiError::NotFound(format!("User {id} not found"))
        },
        _ => CiError::Database(e),
      }
    })
}

/// Get a user by username
///
/// # Errors
///
/// Returns error if database query fails.
pub async fn get_by_username(
  pool: &PgPool,
  username: &str,
) -> Result<Option<User>> {
  sqlx::query_as::<_, User>("SELECT * FROM users WHERE username = $1")
    .bind(username)
    .fetch_optional(pool)
    .await
    .map_err(CiError::Database)
}

/// Get a user by email
///
/// # Errors
///
/// Returns error if database query fails.
pub async fn get_by_email(pool: &PgPool, email: &str) -> Result<Option<User>> {
  sqlx::query_as::<_, User>("SELECT * FROM users WHERE email = $1")
    .bind(email)
    .fetch_optional(pool)
    .await
    .map_err(CiError::Database)
}

/// List all users with pagination
///
/// # Errors
///
/// Returns error if database query fails.
pub async fn list(pool: &PgPool, limit: i64, offset: i64) -> Result<Vec<User>> {
  sqlx::query_as::<_, User>(
    "SELECT * FROM users ORDER BY created_at DESC LIMIT $1 OFFSET $2",
  )
  .bind(limit)
  .bind(offset)
  .fetch_all(pool)
  .await
  .map_err(CiError::Database)
}

/// Count total users
///
/// # Errors
///
/// Returns error if database query fails.
pub async fn count(pool: &PgPool) -> Result<i64> {
  let (count,): (i64,) = sqlx::query_as("SELECT COUNT(*) FROM users")
    .fetch_one(pool)
    .await?;
  Ok(count)
}

/// Update a user with the provided data
///
/// # Errors
///
/// Returns error if validation fails or database update fails.
pub async fn update(
  pool: &PgPool,
  id: Uuid,
  data: &UpdateUser,
) -> Result<User> {
  // Apply all updates sequentially
  if let Some(ref email) = data.email {
    update_email(pool, id, email).await?;
  }

  if let Some(ref full_name) = data.full_name {
    update_full_name(pool, id, Some(full_name.as_str())).await?;
  }

  if let Some(ref password) = data.password {
    update_password(pool, id, password).await?;
  }

  if let Some(ref role) = data.role {
    update_role(pool, id, role).await?;
  }

  if let Some(enabled) = data.enabled {
    set_enabled(pool, id, enabled).await?;
  }

  if let Some(public) = data.public_dashboard {
    set_public_dashboard(pool, id, public).await?;
  }

  get(pool, id).await
}

/// Update user email with validation
///
/// # Errors
///
/// Returns error if validation fails or database update fails.
pub async fn update_email(
  pool: &PgPool,
  id: Uuid,
  email: &str,
) -> Result<User> {
  validate_email(email).map_err(|e| CiError::Validation(e.to_string()))?;

  sqlx::query_as::<_, User>(
    "UPDATE users SET email = $1 WHERE id = $2 RETURNING *",
  )
  .bind(email)
  .bind(id)
  .fetch_one(pool)
  .await
  .map_err(|e| {
    match &e {
      sqlx::Error::Database(db_err) if db_err.is_unique_violation() => {
        CiError::Conflict("Email already in use".to_string())
      },
      _ => CiError::Database(e),
    }
  })
}

/// Update user full name with validation
///
/// # Errors
///
/// Returns error if validation fails or database update fails.
pub async fn update_full_name(
  pool: &PgPool,
  id: Uuid,
  full_name: Option<&str>,
) -> Result<()> {
  if let Some(name) = full_name {
    validate_full_name(name).map_err(|e| CiError::Validation(e.to_string()))?;
  }

  sqlx::query("UPDATE users SET full_name = $1 WHERE id = $2")
    .bind(full_name)
    .bind(id)
    .execute(pool)
    .await?;
  Ok(())
}

/// Update user password with validation
///
/// # Errors
///
/// Returns error if validation fails or database update fails.
pub async fn update_password(
  pool: &PgPool,
  id: Uuid,
  password: &str,
) -> Result<()> {
  validate_password(password)
    .map_err(|e| CiError::Validation(e.to_string()))?;

  let hash = hash_password(password)?;
  sqlx::query("UPDATE users SET password_hash = $1 WHERE id = $2")
    .bind(&hash)
    .bind(id)
    .execute(pool)
    .await?;
  Ok(())
}

/// Update user role with validation
///
/// # Errors
///
/// Returns error if validation fails or database update fails.
pub async fn update_role(pool: &PgPool, id: Uuid, role: &str) -> Result<()> {
  validate_role(role, VALID_ROLES)
    .map_err(|e| CiError::Validation(e.to_string()))?;

  sqlx::query("UPDATE users SET role = $1 WHERE id = $2")
    .bind(role)
    .bind(id)
    .execute(pool)
    .await?;
  Ok(())
}

/// Enable/disable user
///
/// # Errors
///
/// Returns error if database update fails.
pub async fn set_enabled(pool: &PgPool, id: Uuid, enabled: bool) -> Result<()> {
  sqlx::query("UPDATE users SET enabled = $1 WHERE id = $2")
    .bind(enabled)
    .bind(id)
    .execute(pool)
    .await?;
  Ok(())
}

/// Set public dashboard preference
///
/// # Errors
///
/// Returns error if database update fails.
pub async fn set_public_dashboard(
  pool: &PgPool,
  id: Uuid,
  public: bool,
) -> Result<()> {
  sqlx::query("UPDATE users SET public_dashboard = $1 WHERE id = $2")
    .bind(public)
    .bind(id)
    .execute(pool)
    .await?;
  Ok(())
}

/// Delete a user
///
/// # Errors
///
/// Returns error if database delete fails or user not found.
pub async fn delete(pool: &PgPool, id: Uuid) -> Result<()> {
  let result = sqlx::query("DELETE FROM users WHERE id = $1")
    .bind(id)
    .execute(pool)
    .await?;
  if result.rows_affected() == 0 {
    return Err(CiError::NotFound(format!("User {id} not found")));
  }
  Ok(())
}

/// Create or update OAuth user
///
/// # Errors
///
/// Returns error if validation fails or database operation fails.
pub async fn upsert_oauth_user(
  pool: &PgPool,
  username: &str,
  email: Option<&str>,
  user_type: UserType,
  oauth_provider_id: &str,
) -> Result<User> {
  // Use provider ID in username to avoid collisions
  let unique_username = format!("{username}_{oauth_provider_id}");

  // Check if user exists by OAuth provider ID pattern
  let existing =
    sqlx::query_as::<_, User>("SELECT * FROM users WHERE username = $1")
      .bind(&unique_username)
      .fetch_optional(pool)
      .await?;

  if let Some(user) = existing {
    // Update existing user
    if let Some(e) = email {
      // Validate email before updating
      validate_email(e).map_err(|err| CiError::Validation(err.to_string()))?;
      sqlx::query(
        "UPDATE users SET email = $1, last_login_at = NOW(), updated_at = \
         NOW() WHERE id = $2",
      )
      .bind(e)
      .bind(user.id)
      .execute(pool)
      .await?;
    } else {
      sqlx::query(
        "UPDATE users SET last_login_at = NOW(), updated_at = NOW() WHERE id \
         = $1",
      )
      .bind(user.id)
      .execute(pool)
      .await?;
    }
    return get(pool, user.id).await;
  }

  // Create new user
  let user_type_str = match user_type {
    UserType::Local => "local",
    UserType::Github => "github",
    UserType::Google => "google",
    UserType::Ldap => "ldap",
  };

  sqlx::query_as::<_, User>(
    "INSERT INTO users (username, email, user_type, password_hash, role) \
     VALUES ($1, $2, $3, NULL, 'read-only') RETURNING *",
  )
  .bind(&unique_username)
  .bind(email.unwrap_or(&format!("{unique_username}@oauth.local")))
  .bind(user_type_str)
  .fetch_one(pool)
  .await
  .map_err(|e| {
    match &e {
      sqlx::Error::Database(db_err) if db_err.is_unique_violation() => {
        CiError::Conflict("Username or email already in use".to_string())
      },
      _ => CiError::Database(e),
    }
  })
}

/// Create a new session for a user. Returns (`session_token`, `session_id`).
///
/// # Errors
///
/// Returns error if database insert fails.
pub async fn create_session(
  pool: &PgPool,
  user_id: Uuid,
) -> Result<(String, Uuid)> {
  use sha2::{Digest, Sha256};

  // Generate random session token
  let token = Uuid::new_v4().to_string();
  let token_hash = hex::encode(Sha256::digest(token.as_bytes()));

  // Session expires in 7 days
  let expires_at = chrono::Utc::now() + chrono::Duration::days(7);

  let session_id: (Uuid,) = sqlx::query_as(
    "INSERT INTO user_sessions (user_id, session_token_hash, expires_at) \
     VALUES ($1, $2, $3) RETURNING id",
  )
  .bind(user_id)
  .bind(&token_hash)
  .bind(expires_at)
  .fetch_one(pool)
  .await
  .map_err(CiError::Database)?;

  Ok((token, session_id.0))
}

/// Validate a session token and return the associated user if valid.
///
/// # Errors
///
/// Returns error if database query fails.
pub async fn validate_session(
  pool: &PgPool,
  token: &str,
) -> Result<Option<User>> {
  use sha2::{Digest, Sha256};

  let token_hash = hex::encode(Sha256::digest(token.as_bytes()));

  let result = sqlx::query_as::<_, User>(
    "SELECT u.* FROM users u JOIN user_sessions s ON u.id = s.user_id WHERE \
     s.session_token_hash = $1 AND s.expires_at > NOW() AND u.enabled = true",
  )
  .bind(&token_hash)
  .fetch_optional(pool)
  .await?;

  // Update last_used_at
  if result.is_some()
    && let Err(e) = sqlx::query(
      "UPDATE user_sessions SET last_used_at = NOW() WHERE session_token_hash \
       = $1",
    )
    .bind(&token_hash)
    .execute(pool)
    .await
  {
    tracing::warn!(token_hash = %token_hash, "Failed to update session last_used_at: {e}");
  }

  Ok(result)
}
