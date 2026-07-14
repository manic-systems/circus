//! User repository - CRUD operations and authentication

use std::str::FromStr;

use circus_codegen::queries::users as q;
use regex::Regex;
use uuid::Uuid;

use crate::{
  db::{PgPool, is_unique_violation},
  error::{CiError, Result},
  models::{CreateUser, LoginCredentials, UpdateUser, User, UserType},
  roles::GlobalRole,
  validation::{
    validate_email,
    validate_full_name,
    validate_password,
    validate_username,
  },
};

impl TryFrom<q::UserRow> for User {
  type Error = CiError;

  fn try_from(r: q::UserRow) -> Result<Self> {
    let user_type = UserType::from_str(&r.user_type).map_err(|_| {
      CiError::Internal(format!(
        "user {} has unknown user_type '{}' in the database",
        r.id, r.user_type
      ))
    })?;
    let role = GlobalRole::from_str(&r.role).map_err(|_| {
      CiError::Internal(format!(
        "user {} has unknown role '{}' in the database",
        r.id, r.role
      ))
    })?;
    Ok(Self {
      id: r.id,
      username: r.username,
      email: r.email,
      full_name: r.full_name,
      password_hash: r.password_hash,
      user_type,
      role,
      enabled: r.enabled,
      email_verified: r.email_verified,
      public_dashboard: r.public_dashboard,
      created_at: r.created_at,
      updated_at: r.updated_at,
      last_login_at: r.last_login_at,
    })
  }
}

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
/// `email_regex` controls email format validation - see [`validate_email`].
///
/// # Errors
///
/// Returns error if validation fails or database insert fails.
pub async fn create(
  pool: &PgPool,
  data: &CreateUser,
  email_regex: Option<&Regex>,
) -> Result<User> {
  // Validate username
  validate_username(&data.username)?;

  // Validate email
  validate_email(&data.email, email_regex)?;

  // Validate password
  validate_password(&data.password)?;

  // Validate full name if provided
  if let Some(ref name) = data.full_name {
    validate_full_name(name)?;
  }

  let role = data.role.unwrap_or(GlobalRole::ReadOnly);

  let password_hash = hash_password(&data.password)?;

  let client = pool.get().await?;
  let row = q::create()
    .bind(
      &client,
      &data.username,
      &data.email,
      &data.full_name,
      &password_hash,
      &role.as_str(),
    )
    .one()
    .await
    .map_err(|e| {
      if is_unique_violation(&e) {
        CiError::Conflict("Username or email already exists".to_string())
      } else {
        CiError::Database(e)
      }
    })?;
  User::try_from(row)
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
  let client = pool.get().await?;
  let user = q::authenticate_fetch()
    .bind(&client, &creds.username)
    .opt()
    .await
    .map_err(|_| CiError::Unauthorized("Invalid credentials".to_string()))?
    .map(User::try_from)
    .transpose()?
    .ok_or_else(|| CiError::Unauthorized("Invalid credentials".to_string()))?;

  if let Some(ref hash) = user.password_hash {
    if verify_password(&creds.password, hash)? {
      // Update last login time
      if let Err(e) = q::authenticate_touch().bind(&client, &user.id).await {
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
  let client = pool.get().await?;
  q::get()
    .bind(&client, &id)
    .opt()
    .await?
    .map(User::try_from)
    .transpose()?
    .ok_or_else(|| CiError::NotFound(format!("User {id} not found")))
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
  let client = pool.get().await?;
  q::get_by_username()
    .bind(&client, &username)
    .opt()
    .await?
    .map(User::try_from)
    .transpose()
}

/// Get a user by email
///
/// # Errors
///
/// Returns error if database query fails.
pub async fn get_by_email(pool: &PgPool, email: &str) -> Result<Option<User>> {
  let client = pool.get().await?;
  q::get_by_email()
    .bind(&client, &email)
    .opt()
    .await?
    .map(User::try_from)
    .transpose()
}

/// List all users with pagination
///
/// # Errors
///
/// Returns error if database query fails.
pub async fn list(pool: &PgPool, limit: i64, offset: i64) -> Result<Vec<User>> {
  let client = pool.get().await?;
  let rows = q::list().bind(&client, &limit, &offset).all().await?;
  rows.into_iter().map(User::try_from).collect()
}

/// Count total users
///
/// # Errors
///
/// Returns error if database query fails.
pub async fn count(pool: &PgPool) -> Result<i64> {
  let client = pool.get().await?;
  Ok(q::count().bind(&client).one().await?)
}

/// Update a user with the provided data
///
/// `email_regex` controls email format validation - see [`validate_email`].
///
/// # Errors
///
/// Returns error if validation fails or database update fails.
pub async fn update(
  pool: &PgPool,
  id: Uuid,
  data: &UpdateUser,
  email_regex: Option<&Regex>,
) -> Result<User> {
  // Apply all updates sequentially
  if let Some(ref email) = data.email {
    update_email(pool, id, email, email_regex).await?;
  }

  if let Some(ref full_name) = data.full_name {
    update_full_name(pool, id, Some(full_name.as_str())).await?;
  }

  if let Some(ref password) = data.password {
    update_password(pool, id, password).await?;
  }

  if let Some(role) = data.role {
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
/// `email_regex` controls email format validation - see [`validate_email`].
///
/// # Errors
///
/// Returns error if validation fails or database update fails.
pub async fn update_email(
  pool: &PgPool,
  id: Uuid,
  email: &str,
  email_regex: Option<&Regex>,
) -> Result<User> {
  validate_email(email, email_regex)?;

  let client = pool.get().await?;
  let row = q::update_email()
    .bind(&client, &email, &id)
    .one()
    .await
    .map_err(|e| {
      if is_unique_violation(&e) {
        CiError::Conflict("Email already in use".to_string())
      } else {
        CiError::Database(e)
      }
    })?;
  User::try_from(row)
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
    validate_full_name(name)?;
  }

  let client = pool.get().await?;
  q::update_full_name().bind(&client, &full_name, &id).await?;
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
  validate_password(password)?;

  let hash = hash_password(password)?;
  let client = pool.get().await?;
  q::update_password().bind(&client, &hash, &id).await?;
  Ok(())
}

/// Update user role with validation
///
/// # Errors
///
/// Returns error if validation fails or database update fails.
pub async fn update_role(
  pool: &PgPool,
  id: Uuid,
  role: GlobalRole,
) -> Result<()> {
  let client = pool.get().await?;
  q::update_role().bind(&client, &role.as_str(), &id).await?;
  Ok(())
}

/// Enable/disable user
///
/// # Errors
///
/// Returns error if database update fails.
pub async fn set_enabled(pool: &PgPool, id: Uuid, enabled: bool) -> Result<()> {
  let client = pool.get().await?;
  q::set_enabled().bind(&client, &enabled, &id).await?;
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
  let client = pool.get().await?;
  q::set_public_dashboard()
    .bind(&client, &public, &id)
    .await?;
  Ok(())
}

/// Delete a user
///
/// # Errors
///
/// Returns error if database delete fails or user not found.
pub async fn delete(pool: &PgPool, id: Uuid) -> Result<()> {
  let client = pool.get().await?;
  let affected = q::delete().bind(&client, &id).await?;
  if affected == 0 {
    return Err(CiError::NotFound(format!("User {id} not found")));
  }
  Ok(())
}

/// Create or update OAuth user
///
/// `email_regex` controls email format validation - see [`validate_email`].
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
  email_regex: Option<&Regex>,
) -> Result<User> {
  // Use provider ID in username to avoid collisions
  let unique_username = format!("{username}_{oauth_provider_id}");

  // Check if user exists by OAuth provider ID pattern
  let existing = {
    let client = pool.get().await?;
    q::upsert_oauth_user_fetch()
      .bind(&client, &unique_username)
      .opt()
      .await?
      .map(User::try_from)
      .transpose()?
  };

  if let Some(user) = existing {
    // Update existing user
    {
      let client = pool.get().await?;
      if let Some(e) = email {
        // Validate email before updating
        validate_email(e, email_regex)?;
        q::upsert_oauth_user_update_email()
          .bind(&client, &e, &user.id)
          .await?;
      } else {
        q::upsert_oauth_user_touch().bind(&client, &user.id).await?;
      }
    }
    return get(pool, user.id).await;
  }

  // Create new user
  let fallback_email = format!("{unique_username}@oauth.local");
  let email = email.unwrap_or(&fallback_email);

  let client = pool.get().await?;
  let row = q::upsert_oauth_user_insert()
    .bind(&client, &unique_username, &email, &user_type.as_db_str())
    .one()
    .await
    .map_err(|e| {
      if is_unique_violation(&e) {
        CiError::Conflict("Username or email already in use".to_string())
      } else {
        CiError::Database(e)
      }
    })?;
  User::try_from(row)
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

  let client = pool.get().await?;
  let session_id = q::create_session()
    .bind(&client, &user_id, &token_hash, &expires_at)
    .one()
    .await?;

  Ok((token, session_id))
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

  let client = pool.get().await?;
  let result = q::validate_session_fetch()
    .bind(&client, &token_hash)
    .opt()
    .await?
    .map(User::try_from)
    .transpose()?;

  // Update last_used_at
  if result.is_some()
    && let Err(e) = q::validate_session_touch().bind(&client, &token_hash).await
  {
    tracing::warn!("Failed to update session last_used_at: {e}");
  }

  Ok(result)
}

/// Delete a user session by its raw session token.
///
/// # Errors
///
/// Returns error if the database delete fails.
pub async fn delete_session(pool: &PgPool, token: &str) -> Result<bool> {
  use sha2::{Digest, Sha256};

  let token_hash = hex::encode(Sha256::digest(token.as_bytes()));
  let client = pool.get().await?;
  let deleted = q::delete_session().bind(&client, &token_hash).await?;

  Ok(deleted > 0)
}
