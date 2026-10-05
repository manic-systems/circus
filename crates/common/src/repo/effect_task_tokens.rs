//! Per-attempt bearer tokens that let a running Effect call back into the API.

use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use circus_codegen::queries::effect_task_tokens as q;
use ring::rand::{SecureRandom as _, SystemRandom};
use sha2::{Digest as _, Sha256};
use uuid::Uuid;

use crate::{
  db::PgPool,
  error::{CiError, Result},
};

/// The Effect attempt a task token belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EffectTask {
  pub project_id: Uuid,
  pub build_id:   Uuid,
}

fn hash_token(token: &str) -> String {
  hex::encode(Sha256::digest(token.as_bytes()))
}

/// Issue a fresh token for `attempt` of `build_id`, replacing any token from
/// an earlier attempt. Only its hash is stored.
///
/// # Errors
///
/// Returns an error if randomness is unavailable or the write fails.
pub async fn issue(
  pool: &PgPool,
  build_id: Uuid,
  attempt: i32,
) -> Result<String> {
  let mut bytes = [0u8; 32];
  SystemRandom::new().fill(&mut bytes).map_err(|_| {
    CiError::Internal("Failed to generate an Effect task token".into())
  })?;
  let token = format!("circus_task_{}", URL_SAFE_NO_PAD.encode(bytes));
  let client = pool.get().await?;
  q::upsert()
    .bind(&client, &build_id, &attempt, &hash_token(&token))
    .await?;
  Ok(token)
}

/// Resolve a token to its Effect, but only while that exact attempt is still
/// running, so a token dies with its attempt on every terminal path.
///
/// # Errors
///
/// Returns an error if the query fails.
pub async fn active_task(
  pool: &PgPool,
  token: &str,
) -> Result<Option<EffectTask>> {
  let client = pool.get().await?;
  let row = q::active_project()
    .bind(&client, &hash_token(token))
    .opt()
    .await?;
  Ok(row.map(|row| {
    EffectTask {
      project_id: row.project_id,
      build_id:   row.build_id,
    }
  }))
}
