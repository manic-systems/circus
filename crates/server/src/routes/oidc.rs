use std::{collections::BTreeMap, error::Error, sync::Arc, time::Duration};

use askama::Template;
use axum::{
  Router,
  extract::{Path, Query, State, rejection::QueryRejection},
  http::{StatusCode, header::SET_COOKIE},
  response::{AppendHeaders, Html, IntoResponse, Redirect, Response},
  routing::get,
};
use axum_extra::extract::cookie::CookieJar;
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use circus_common::{CiError, audit::Actor, repo};
use circus_config::OidcProviderConfig;
use jsonwebtoken::{
  DecodingKey,
  Validation,
  decode,
  decode_header,
  jwk::{JwkSet, PublicKeyUse},
};
use moka::sync::Cache;
use oauth2::{
  AuthType,
  AuthUrl,
  AuthorizationCode,
  ClientId,
  ClientSecret,
  CsrfToken,
  EndpointNotSet,
  EndpointSet,
  ExtraTokenFields,
  PkceCodeChallenge,
  PkceCodeVerifier,
  RedirectUrl,
  RequestTokenError,
  Scope,
  StandardRevocableToken,
  StandardTokenResponse,
  TokenResponse,
  TokenUrl,
  basic::{
    BasicErrorResponse,
    BasicRevocationErrorResponse,
    BasicTokenIntrospectionResponse,
    BasicTokenType,
  },
  reqwest::{Client as HttpClient, header::ACCEPT, redirect::Policy},
};
use serde::{
  Deserialize,
  Serialize,
  de::{DeserializeOwned, IgnoredAny},
};
use serde_json::Value;
use sha2::{Digest, Sha256};
use subtle::ConstantTimeEq;

use crate::{
  audit::record_with_actor,
  routes::dashboard::templates::LoginTemplate,
  session_cookie::{
    OIDC_FLOW_COOKIE,
    clear_oidc_flow_cookie,
    oauth_user_session_cookie,
    oidc_flow_cookie,
  },
  state::AppState,
};

type BoxError = Box<dyn Error + Send + Sync>;

const MAX_USERNAME_LEN: usize = 32;

const DISCOVERY_TTL: Duration = Duration::from_mins(10);

#[derive(Debug, Clone, Deserialize, Serialize)]
struct IdTokenField {
  id_token: String,
}

impl ExtraTokenFields for IdTokenField {}

type OidcClient = oauth2::Client<
  BasicErrorResponse,
  StandardTokenResponse<IdTokenField, BasicTokenType>,
  BasicTokenIntrospectionResponse,
  StandardRevocableToken,
  BasicRevocationErrorResponse,
  EndpointSet,
  EndpointNotSet,
  EndpointNotSet,
  EndpointNotSet,
  EndpointSet,
>;

#[derive(Deserialize)]
struct ProviderMetadata {
  issuer:                                String,
  authorization_endpoint:                AuthUrl,
  token_endpoint:                        TokenUrl,
  jwks_uri:                              String,
  userinfo_endpoint:                     Option<String>,
  #[serde(default)]
  token_endpoint_auth_methods_supported: Vec<String>,
}

struct Provider {
  http:              HttpClient,
  client:            OidcClient,
  issuer:            String,
  jwks_uri:          String,
  userinfo_endpoint: Option<String>,
}

/// Discovered providers keyed by config name, so unauthenticated login starts
/// do not each trigger a discovery fetch.
#[derive(Clone)]
pub struct OidcProviders(Cache<String, Arc<Provider>>);

impl Default for OidcProviders {
  fn default() -> Self {
    Self(Cache::builder().time_to_live(DISCOVERY_TTL).build())
  }
}

#[derive(Deserialize)]
#[serde(untagged)]
enum Audience {
  Many(Vec<IgnoredAny>),
  One(IgnoredAny),
}

#[derive(Deserialize)]
struct IdTokenClaims {
  sub:                String,
  aud:                Audience,
  azp:                Option<String>,
  nonce:              Option<String>,
  preferred_username: Option<String>,
  email:              Option<String>,
  #[serde(default)]
  email_verified:     bool,
  #[serde(flatten)]
  extra:              BTreeMap<String, Value>,
}

#[derive(Deserialize)]
struct UserInfo {
  sub:   String,
  #[serde(flatten)]
  extra: BTreeMap<String, Value>,
}

/// `disambiguate` appends a hash of `sub` for a second user whose claims
/// sanitize to a taken name.
fn username(
  claims: &IdTokenClaims,
  provider: &str,
  disambiguate: bool,
) -> String {
  let base = [
    claims.preferred_username.as_deref(),
    claims
      .email
      .as_deref()
      .and_then(|email| email.split('@').next()),
  ]
  .into_iter()
  .flatten()
  .find(|name| !name.is_empty())
  .unwrap_or(&claims.sub);
  let mut name: String = base
    .chars()
    .map(|character| {
      if character.is_ascii_alphanumeric() || matches!(character, '-' | '_') {
        character
      } else {
        '-'
      }
    })
    .collect();

  let suffix = if disambiguate {
    let digest = Sha256::digest(claims.sub.as_bytes());
    format!("-{}", hex::encode(&digest[..2]))
  } else {
    String::new()
  };

  name.truncate(
    MAX_USERNAME_LEN.saturating_sub(suffix.len() + provider.len() + 1),
  );
  format!("{name}{suffix}_{provider}")
}

fn claim_groups(
  claims: &BTreeMap<String, Value>,
  name: &str,
) -> Result<Option<Vec<String>>, LoginError> {
  match claims.get(name) {
    None | Some(Value::Null) => Ok(None),
    Some(Value::String(group)) => Ok(Some(vec![group.clone()])),
    Some(value) => {
      Vec::deserialize(value)
        .map(Some)
        .map_err(LoginError::invalid)
    },
  }
}

#[derive(Debug, thiserror::Error)]
enum LoginError {
  #[error("unknown OIDC provider")]
  UnknownProvider,
  #[error("invalid or expired login state")]
  InvalidState,
  #[error("provider denied login with {0}")]
  Denied(String),
  #[error("provider sent invalid claims")]
  InvalidClaims(#[source] BoxError),
  #[error("user is in no allowed group")]
  NotAllowed,
  #[error("username or email belongs to another account")]
  Conflict(#[source] CiError),
  #[error("user is disabled")]
  Disabled,
  #[error("identity provider unavailable")]
  Unavailable(#[source] BoxError),
}

impl LoginError {
  fn invalid(error: impl Into<BoxError>) -> Self {
    Self::InvalidClaims(error.into())
  }

  fn unavailable(error: impl Into<BoxError>) -> Self {
    Self::Unavailable(error.into())
  }

  const fn status(&self) -> StatusCode {
    match self {
      Self::UnknownProvider => StatusCode::NOT_FOUND,
      Self::Unavailable(_) => StatusCode::BAD_GATEWAY,
      _ => StatusCode::UNAUTHORIZED,
    }
  }

  const fn message(&self) -> &'static str {
    match self {
      Self::Unavailable(_) => {
        "Sign-in is temporarily unavailable. Please try again."
      },
      Self::Conflict(_) => "An account already uses this username or email.",
      _ => "Unable to sign in. Please try again or contact an administrator.",
    }
  }
}

impl From<CiError> for LoginError {
  fn from(error: CiError) -> Self {
    match error {
      CiError::Conflict(_) => Self::Conflict(error),
      CiError::Validation(_) => Self::invalid(error),
      _ => Self::unavailable(error),
    }
  }
}

#[derive(Deserialize, Serialize)]
struct LoginFlow {
  provider:      String,
  state:         CsrfToken,
  nonce:         String,
  pkce_verifier: PkceCodeVerifier,
}

impl LoginFlow {
  fn from_cookie(jar: &CookieJar) -> Result<Self, LoginError> {
    let cookie = jar.get(OIDC_FLOW_COOKIE).ok_or(LoginError::InvalidState)?;
    let json = URL_SAFE_NO_PAD
      .decode(cookie.value())
      .map_err(|_| LoginError::InvalidState)?;
    serde_json::from_slice(&json).map_err(|_| LoginError::InvalidState)
  }

  fn to_cookie_value(&self) -> serde_json::Result<String> {
    Ok(URL_SAFE_NO_PAD.encode(serde_json::to_vec(self)?))
  }
}

#[derive(Deserialize)]
struct CallbackParams {
  state:   CsrfToken,
  #[serde(flatten)]
  outcome: CallbackOutcome,
}

#[derive(Deserialize)]
#[serde(untagged)]
enum CallbackOutcome {
  Granted {
    code: AuthorizationCode,
  },
  Denied {
    error:             String,
    error_description: Option<String>,
  },
}

async fn get_json<T: DeserializeOwned>(
  http: &HttpClient,
  url: &str,
  bearer: Option<&str>,
) -> Result<T, LoginError> {
  let mut request = http.get(url).header(ACCEPT, "application/json");
  if let Some(token) = bearer {
    request = request.bearer_auth(token);
  }

  let body = request
    .send()
    .await
    .and_then(oauth2::reqwest::Response::error_for_status)
    .map_err(LoginError::unavailable)?
    .bytes()
    .await
    .map_err(LoginError::unavailable)?;
  serde_json::from_slice(&body).map_err(LoginError::unavailable)
}

async fn discover(config: &OidcProviderConfig) -> Result<Provider, LoginError> {
  let http = HttpClient::builder()
    .redirect(Policy::none())
    .timeout(Duration::from_secs(30))
    .build()
    .map_err(LoginError::unavailable)?;
  let url = format!(
    "{}/.well-known/openid-configuration",
    config.issuer_url.trim_end_matches('/')
  );
  let metadata: ProviderMetadata = get_json(&http, &url, None).await?;

  if metadata.issuer != config.issuer_url {
    return Err(LoginError::unavailable(format!(
      "discovery issuer {} does not match {}",
      metadata.issuer, config.issuer_url
    )));
  }

  let redirect = RedirectUrl::new(config.redirect_uri.clone())
    .map_err(LoginError::unavailable)?;
  let methods = &metadata.token_endpoint_auth_methods_supported;
  let post_only = !methods.is_empty()
    && !methods.iter().any(|method| method == "client_secret_basic")
    && methods.iter().any(|method| method == "client_secret_post");
  let mut client: OidcClient =
    oauth2::Client::new(ClientId::new(config.client_id.clone()))
      .set_auth_uri(metadata.authorization_endpoint)
      .set_token_uri(metadata.token_endpoint)
      .set_redirect_uri(redirect);

  if let Some(secret) = &config.client_secret {
    client = client.set_client_secret(ClientSecret::new(secret.clone()));
  }

  if post_only {
    client = client.set_auth_type(AuthType::RequestBody);
  }

  Ok(Provider {
    http,
    client,
    issuer: metadata.issuer,
    jwks_uri: metadata.jwks_uri,
    userinfo_endpoint: metadata.userinfo_endpoint,
  })
}

async fn cached_provider(
  state: &AppState,
  name: &str,
  config: &OidcProviderConfig,
) -> Result<Arc<Provider>, LoginError> {
  if let Some(provider) = state.oidc_providers.0.get(name) {
    return Ok(provider);
  }

  let provider = Arc::new(discover(config).await?);
  state
    .oidc_providers
    .0
    .insert(name.to_owned(), Arc::clone(&provider));
  Ok(provider)
}

async fn verify_id_token(
  provider: &Provider,
  client_id: &str,
  id_token: &str,
  nonce: &str,
) -> Result<IdTokenClaims, LoginError> {
  let header = decode_header(id_token).map_err(LoginError::invalid)?;
  let jwks: JwkSet = get_json(&provider.http, &provider.jwks_uri, None).await?;
  let jwk = match &header.kid {
    Some(kid) => jwks.find(kid),
    None => {
      match jwks.keys.as_slice() {
        [only] => Some(only),
        _ => None,
      }
    },
  }
  .filter(|jwk| jwk.common.public_key_use != Some(PublicKeyUse::Encryption))
  .ok_or_else(|| LoginError::invalid("no signing key matches the ID token"))?;
  let key = DecodingKey::from_jwk(jwk).map_err(LoginError::invalid)?;

  let mut validation = Validation::new(header.alg);
  validation.set_issuer(&[&provider.issuer]);
  validation.set_audience(&[client_id]);
  validation.set_required_spec_claims(&["exp", "iat", "iss", "aud", "sub"]);
  let claims = decode::<IdTokenClaims>(id_token, &key, &validation)
    .map_err(LoginError::invalid)?
    .claims;

  if claims.nonce.as_deref() != Some(nonce) {
    return Err(LoginError::invalid("ID token nonce does not match"));
  }

  let azp_required =
    matches!(&claims.aud, Audience::Many(audiences) if audiences.len() > 1);
  if claims
    .azp
    .as_deref()
    .map_or(azp_required, |azp| azp != client_id)
  {
    return Err(LoginError::invalid("ID token azp does not match"));
  }

  Ok(claims)
}

async fn audit_login(
  state: &AppState,
  provider: &str,
  actor: &Actor,
  action: &str,
  reason: Option<&LoginError>,
) {
  record_with_actor(
    &state.pool,
    actor,
    None,
    action,
    Some("oidc"),
    Some(provider),
    serde_json::json!({
      "method": "oidc",
      "provider": provider,
      "reason": reason.map(ToString::to_string),
    }),
  )
  .await;
}

async fn login_failure(
  state: &AppState,
  provider: &str,
  error: LoginError,
) -> Response {
  tracing::debug!(%provider, ?error, "OIDC login failed");

  // An unconfigured name is not a login attempt, and auditing it would let
  // anyone write rows with arbitrary provider strings.
  if !matches!(error, LoginError::UnknownProvider) {
    audit_login(
      state,
      provider,
      &Actor::anonymous(),
      "LOGIN_FAILURE",
      Some(&error),
    )
    .await;
  }

  let html = LoginTemplate::new(&state.config, Some(error.message().into()))
    .render()
    .unwrap_or_else(|render_error| {
      tracing::error!(%render_error, "Unable to render OIDC login failure");
      error.message().to_owned()
    });
  let redirect_uri = state
    .config
    .oauth
    .oidc
    .get(provider)
    .map_or("", |config| config.redirect_uri.as_str());

  (
    error.status(),
    [(
      SET_COOKIE,
      clear_oidc_flow_cookie(&state.config.server, redirect_uri),
    )],
    Html(html),
  )
    .into_response()
}

async fn start_login(
  state: &AppState,
  provider: &str,
) -> Result<Response, LoginError> {
  let config = state
    .config
    .oauth
    .oidc
    .get(provider)
    .ok_or(LoginError::UnknownProvider)?;
  let idp = cached_provider(state, provider, config).await?;
  let (challenge, pkce_verifier) = PkceCodeChallenge::new_random_sha256();
  let nonce = CsrfToken::new_random().into_secret();
  let (url, csrf) = idp
    .client
    .authorize_url(CsrfToken::new_random)
    .add_scope(Scope::new("openid".into()))
    .add_scopes(
      config
        .scopes
        .iter()
        .filter(|scope| *scope != "openid")
        .cloned()
        .map(Scope::new),
    )
    .add_extra_param("nonce", &nonce)
    .set_pkce_challenge(challenge)
    .url();
  let flow = LoginFlow {
    provider: provider.to_owned(),
    state: csrf,
    nonce,
    pkce_verifier,
  };
  let cookie = oidc_flow_cookie(
    &flow.to_cookie_value().map_err(LoginError::unavailable)?,
    &state.config.server,
    &config.redirect_uri,
  );

  Ok(
    (
      StatusCode::FOUND,
      [(SET_COOKIE, cookie)],
      Redirect::to(url.as_str()),
    )
      .into_response(),
  )
}

async fn complete_login(
  state: &AppState,
  provider: &str,
  jar: &CookieJar,
  params: Result<Query<CallbackParams>, QueryRejection>,
) -> Result<Response, LoginError> {
  let config = state
    .config
    .oauth
    .oidc
    .get(provider)
    .ok_or(LoginError::UnknownProvider)?;
  let Query(params) = params.map_err(|_| LoginError::InvalidState)?;
  let flow = LoginFlow::from_cookie(jar)?;
  let expected = flow.state.secret().as_bytes();
  let received = params.state.secret().as_bytes();

  if flow.provider != provider
    || expected.len() != received.len()
    || !bool::from(expected.ct_eq(received))
  {
    return Err(LoginError::InvalidState);
  }

  let code = match params.outcome {
    CallbackOutcome::Granted { code } => code,
    CallbackOutcome::Denied {
      error,
      error_description,
    } => {
      tracing::debug!(%error, ?error_description, "OIDC provider denied login");
      return Err(LoginError::Denied(error));
    },
  };

  let idp = cached_provider(state, provider, config).await?;
  let token = idp
    .client
    .exchange_code(code)
    .set_pkce_verifier(flow.pkce_verifier)
    .request_async(&idp.http)
    .await
    .map_err(|error| {
      match error {
        RequestTokenError::ServerResponse(_) => LoginError::invalid(error),
        _ => LoginError::unavailable(error),
      }
    })?;
  let claims = verify_id_token(
    &idp,
    &config.client_id,
    &token.extra_fields().id_token,
    &flow.nonce,
  )
  .await?;

  let groups = match claim_groups(&claims.extra, &config.groups_claim)? {
    Some(groups) => groups,
    None => {
      match &idp.userinfo_endpoint {
        Some(endpoint) => {
          let userinfo: UserInfo =
            get_json(&idp.http, endpoint, Some(token.access_token().secret()))
              .await?;

          if userinfo.sub != claims.sub {
            return Err(LoginError::invalid("userinfo subject does not match"));
          }

          claim_groups(&userinfo.extra, &config.groups_claim)?
            .unwrap_or_default()
        },
        None => Vec::new(),
      }
    },
  };

  if !config.allowed_groups.is_empty()
    && !config
      .allowed_groups
      .iter()
      .any(|group| groups.contains(group))
  {
    return Err(LoginError::NotAllowed);
  }

  let role = config
    .role_mappings
    .iter()
    .find(|mapping| groups.contains(&mapping.group))
    .map_or(config.default_role, |mapping| mapping.role);
  let managed_role = (!config.role_mappings.is_empty()).then_some(role);
  let external_id = format!("{}#{}", idp.issuer, claims.sub);
  let email = claims.email.as_deref().filter(|_| claims.email_verified);
  let upsert = async |username: &str| {
    repo::users::upsert_oidc_user(
      &state.pool,
      username,
      email,
      &external_id,
      role,
      managed_role,
      state.email_regex.as_deref(),
    )
    .await
  };
  let user = match upsert(&username(&claims, provider, false)).await {
    Err(CiError::Conflict(_)) => {
      upsert(&username(&claims, provider, true)).await
    },
    result => result,
  }?;

  if !user.enabled {
    return Err(LoginError::Disabled);
  }

  let (session_token, _) =
    repo::users::create_session(&state.pool, user.id).await?;
  audit_login(
    state,
    provider,
    &Actor::user(user.id, user.username),
    "LOGIN_SUCCESS",
    None,
  )
  .await;
  let cookies = [
    clear_oidc_flow_cookie(&state.config.server, &config.redirect_uri),
    oauth_user_session_cookie(
      &session_token,
      &state.config.server,
      &config.redirect_uri,
    ),
  ];

  Ok(
    (
      StatusCode::FOUND,
      AppendHeaders(cookies.map(|cookie| (SET_COOKIE, cookie))),
      Redirect::to("/"),
    )
      .into_response(),
  )
}

async fn oidc_login(
  State(state): State<AppState>,
  Path(provider): Path<String>,
) -> Response {
  match start_login(&state, &provider).await {
    Ok(response) => response,
    Err(error) => login_failure(&state, &provider, error).await,
  }
}

async fn oidc_callback(
  State(state): State<AppState>,
  Path(provider): Path<String>,
  jar: CookieJar,
  params: Result<Query<CallbackParams>, QueryRejection>,
) -> Response {
  match complete_login(&state, &provider, &jar, params).await {
    Ok(response) => response,
    Err(error) => login_failure(&state, &provider, error).await,
  }
}

pub fn router() -> Router<AppState> {
  Router::new()
    .route("/api/v1/auth/oidc/{provider}", get(oidc_login))
    .route("/api/v1/auth/oidc/{provider}/callback", get(oidc_callback))
}

#[cfg(test)]
#[expect(clippy::unwrap_used, reason = "fine in tests")]
mod tests {
  use serde_json::json;

  use super::{IdTokenClaims, MAX_USERNAME_LEN, claim_groups, username};

  fn claims(value: serde_json::Value) -> IdTokenClaims {
    serde_json::from_value(value).unwrap()
  }

  #[test]
  fn malformed_groups_claim_fails_closed() {
    for value in [json!(42), json!([1, 2]), json!({ "admins": true })] {
      let extra = [("groups".to_owned(), value)].into();
      assert!(claim_groups(&extra, "groups").is_err());
    }

    let single = [("groups".to_owned(), json!("admins"))].into();
    assert_eq!(
      claim_groups(&single, "groups").unwrap(),
      Some(vec!["admins".to_owned()])
    );
    let null = [("groups".to_owned(), json!(null))].into();
    assert_eq!(claim_groups(&null, "groups").unwrap(), None);
  }

  #[test]
  fn usernames_are_valid_and_keep_the_provider_suffix() {
    let long = claims(json!({
      "sub": "0f8fad5b-d9cb-469f-a165-70867728950e",
      "aud": "circus",
      "preferred_username": "jane.doe+ci@corp.example.com-with-a-long-tail",
    }));
    let provider = "sixteen-chars-ab";

    for disambiguate in [false, true] {
      let name = username(&long, provider, disambiguate);
      assert!(name.len() <= MAX_USERNAME_LEN, "{name}");
      assert!(name.ends_with("_sixteen-chars-ab"), "{name}");
      assert!(
        name.chars().all(|character| {
          character.is_ascii_alphanumeric() || matches!(character, '-' | '_')
        }),
        "{name}"
      );
    }

    assert_ne!(
      username(&long, provider, false),
      username(&long, provider, true)
    );

    let email_only = claims(json!({
      "sub": "opaque-subject",
      "aud": "circus",
      "preferred_username": "",
      "email": "kilgore@kilgore.trout",
    }));
    assert_eq!(username(&email_only, "dex", false), "kilgore_dex");
  }
}
