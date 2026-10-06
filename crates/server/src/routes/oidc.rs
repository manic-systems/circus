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
use circus_common::{CiError, audit::Actor, models::User, repo};
use circus_config::OidcProviderConfig;
use data_encoding::{BASE64URL_NOPAD, HEXLOWER};
use jsonwebtoken::{
  DecodingKey,
  Validation,
  decode,
  decode_header,
  jwk::{JwkSet, PublicKeyUse},
};
use moka::sync::Cache;
use reqwest::{Client as HttpClient, header::ACCEPT, redirect::Policy};
use serde::{
  Deserialize,
  Serialize,
  de::{DeserializeOwned, IgnoredAny},
};
use serde_json::Value;
use sha2::{Digest, Sha256};
use subtle::ConstantTimeEq;
use url::Url;
use uuid::Uuid;

use crate::{
  audit::record_with_actor,
  oauth_client::{AuthCodeClient, ClientAuth, Secret, TokenError},
  routes::dashboard::templates::LoginTemplate,
  session_cookie::{
    OIDC_FLOW_COOKIE,
    USER_SESSION_MAX_AGE_SECS,
    clear_oidc_flow_cookie,
    oauth_user_session_cookie,
    oidc_flow_cookie,
    oidc_provider_cookie,
  },
  state::AppState,
};

type BoxError = Box<dyn Error + Send + Sync>;

const MAX_USERNAME_LEN: usize = 32;

const DISCOVERY_TTL: Duration = Duration::from_mins(10);

#[derive(Deserialize)]
struct TokenResponse {
  access_token: String,
  id_token:     String,
}

#[derive(Deserialize)]
struct ProviderMetadata {
  issuer:                                String,
  authorization_endpoint:                String,
  token_endpoint:                        String,
  jwks_uri:                              String,
  userinfo_endpoint:                     Option<String>,
  end_session_endpoint:                  Option<String>,
  #[serde(default)]
  token_endpoint_auth_methods_supported: Vec<String>,
}

struct Provider {
  http:                 HttpClient,
  client:               AuthCodeClient,
  issuer:               String,
  jwks_uri:             String,
  userinfo_endpoint:    Option<String>,
  end_session_endpoint: Option<String>,
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

/// Usernames to try in order, the bare name first, then with `_<provider>`
/// for a name already taken locally, then with a hash of `sub` for a second
/// user of the same provider whose claims sanitize alike.
fn usernames(claims: &IdTokenClaims, provider: &str) -> [String; 3] {
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
  let name: String = base
    .chars()
    .map(|character| {
      if character.is_ascii_alphanumeric() || matches!(character, '-' | '_') {
        character
      } else {
        '-'
      }
    })
    .collect();

  let digest = Sha256::digest(claims.sub.as_bytes());
  let hashed = format!("-{}_{provider}", HEXLOWER.encode(&digest[..2]));
  let suffixed = format!("_{provider}");

  [String::new(), suffixed, hashed].map(|suffix| {
    let keep = MAX_USERNAME_LEN.saturating_sub(suffix.len());
    let head: String = name.chars().take(keep).collect();
    format!("{head}{suffix}")
  })
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
  #[error("identity is already linked to an account")]
  AlreadyLinked(#[source] CiError),
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
      Self::AlreadyLinked(_) => {
        "This sign-in is already linked to an account, or yours already has \
         one from this provider."
      },
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
  state:         Secret,
  nonce:         Secret,
  pkce_verifier: Secret,
  link:          Option<LinkGrant>,
  next:          Option<String>,
}

/// The account a flow links to. The flow cookie is unsigned, so `mac` is what
/// stops a browser from naming someone else's account or provider.
#[derive(Deserialize, Serialize)]
struct LinkGrant {
  user: Uuid,
  mac:  String,
}

impl LinkGrant {
  fn new(state: &AppState, provider: &str, user: Uuid, csrf: &Secret) -> Self {
    Self {
      user,
      mac: Self::mac(state, provider, user, csrf),
    }
  }

  fn mac(
    state: &AppState,
    provider: &str,
    user: Uuid,
    csrf: &Secret,
  ) -> String {
    state
      .csrf_token_for(&format!("oidc-link:{provider}:{user}:{}", csrf.as_str()))
  }

  fn verify(
    &self,
    state: &AppState,
    provider: &str,
    csrf: &Secret,
  ) -> Result<Uuid, LoginError> {
    let expected = Self::mac(state, provider, self.user, csrf);

    if bool::from(expected.as_bytes().ct_eq(self.mac.as_bytes())) {
      Ok(self.user)
    } else {
      Err(LoginError::InvalidState)
    }
  }
}

impl LoginFlow {
  fn from_cookie(jar: &CookieJar) -> Result<Self, LoginError> {
    let cookie = jar.get(OIDC_FLOW_COOKIE).ok_or(LoginError::InvalidState)?;
    let json = BASE64URL_NOPAD
      .decode(cookie.value().as_bytes())
      .map_err(|_| LoginError::InvalidState)?;
    serde_json::from_slice(&json).map_err(|_| LoginError::InvalidState)
  }

  fn to_cookie_value(&self) -> serde_json::Result<String> {
    Ok(BASE64URL_NOPAD.encode(&serde_json::to_vec(self)?))
  }
}

#[derive(Deserialize)]
struct CallbackParams {
  state:   Secret,
  #[serde(flatten)]
  outcome: CallbackOutcome,
}

#[derive(Deserialize)]
#[serde(untagged)]
enum CallbackOutcome {
  Granted {
    code: String,
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
    .and_then(reqwest::Response::error_for_status)
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

  let methods = &metadata.token_endpoint_auth_methods_supported;
  let post_only = !methods.is_empty()
    && !methods.iter().any(|method| method == "client_secret_basic")
    && methods.iter().any(|method| method == "client_secret_post");
  let client = AuthCodeClient {
    client_id:     config.client_id.clone(),
    client_secret: config.client_secret.clone(),
    auth_url:      Url::parse(&metadata.authorization_endpoint)
      .map_err(LoginError::unavailable)?,
    token_url:     Url::parse(&metadata.token_endpoint)
      .map_err(LoginError::unavailable)?,
    redirect_uri:  config.redirect_uri.clone(),
    auth:          if post_only {
      ClientAuth::RequestBody
    } else {
      ClientAuth::Basic
    },
  };

  Ok(Provider {
    http,
    client,
    issuer: metadata.issuer,
    jwks_uri: metadata.jwks_uri,
    userinfo_endpoint: metadata.userinfo_endpoint,
    end_session_endpoint: metadata.end_session_endpoint,
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
  link: Option<Uuid>,
  next: Option<&str>,
) -> Result<Response, LoginError> {
  let config = state
    .config
    .oauth
    .oidc
    .get(provider)
    .ok_or(LoginError::UnknownProvider)?;
  let idp = cached_provider(state, provider, config).await?;
  let csrf = Secret::random().map_err(LoginError::unavailable)?;
  let nonce = Secret::random().map_err(LoginError::unavailable)?;
  let pkce_verifier = Secret::random().map_err(LoginError::unavailable)?;
  let url = idp.client.authorize_url(
    &csrf,
    std::iter::once("openid").chain(
      config
        .scopes
        .iter()
        .map(String::as_str)
        .filter(|scope| *scope != "openid"),
    ),
    &[
      ("nonce", nonce.as_str()),
      ("code_challenge", &pkce_verifier.pkce_challenge()),
      ("code_challenge_method", "S256"),
    ],
  );
  let flow = LoginFlow {
    provider: provider.to_owned(),
    link: link.map(|user| LinkGrant::new(state, provider, user, &csrf)),
    next: crate::routes::return_to(next).map(str::to_owned),
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
  let expected = flow.state.as_str().as_bytes();
  let received = params.state.as_str().as_bytes();

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
  let token: TokenResponse = idp
    .client
    .exchange(&idp.http, &code, Some(&flow.pkce_verifier))
    .await
    .map_err(|error| {
      match error {
        TokenError::Rejected { .. } => LoginError::invalid(error),
        _ => LoginError::unavailable(error),
      }
    })?;
  let claims = verify_id_token(
    &idp,
    &config.client_id,
    &token.id_token,
    flow.nonce.as_str(),
  )
  .await?;

  let groups = match claim_groups(&claims.extra, &config.groups_claim)? {
    Some(groups) => groups,
    None => {
      match &idp.userinfo_endpoint {
        Some(endpoint) => {
          let userinfo: UserInfo =
            get_json(&idp.http, endpoint, Some(&token.access_token)).await?;

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

  let external_id = format!("{}#{}", idp.issuer, claims.sub);
  let clear_flow =
    clear_oidc_flow_cookie(&state.config.server, &config.redirect_uri);

  if let Some(grant) = &flow.link {
    let user = repo::users::get(
      &state.pool,
      grant.verify(state, provider, &flow.state)?,
    )
    .await?;

    if repo::users::native_external_id(&state.pool, user.id)
      .await?
      .as_ref()
      == Some(&external_id)
    {
      return Err(LoginError::AlreadyLinked(CiError::Conflict(
        "account was created with this identity".into(),
      )));
    }

    repo::users::link_identity(&state.pool, user.id, provider, &external_id)
      .await
      .map_err(|error| {
        match error {
          CiError::Conflict(_) => LoginError::AlreadyLinked(error),
          _ => error.into(),
        }
      })?;
    audit_login(
      state,
      provider,
      &Actor::user(user.id, user.username),
      "OIDC_LINK",
      None,
    )
    .await;

    return Ok(
      (
        StatusCode::FOUND,
        [(SET_COOKIE, clear_flow)],
        Redirect::to("/account"),
      )
        .into_response(),
    );
  }

  if let Some(user) =
    repo::users::login_linked_identity(&state.pool, &external_id).await?
  {
    return start_session(
      state,
      provider,
      user,
      clear_flow,
      config,
      flow.next.as_deref(),
    )
    .await;
  }

  let role = config
    .role_mappings
    .iter()
    .find(|mapping| groups.contains(&mapping.group))
    .map_or(config.default_role, |mapping| mapping.role);
  let managed_role = (!config.role_mappings.is_empty()).then_some(role);
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
  let [bare, suffixed, hashed] = usernames(&claims, provider);
  let user = match upsert(&bare).await {
    Err(CiError::Conflict(_)) => {
      match upsert(&suffixed).await {
        Err(CiError::Conflict(_)) => upsert(&hashed).await,
        result => result,
      }
    },
    result => result,
  }?;

  start_session(
    state,
    provider,
    user,
    clear_flow,
    config,
    flow.next.as_deref(),
  )
  .await
}

async fn start_session(
  state: &AppState,
  provider: &str,
  user: User,
  clear_flow: String,
  config: &OidcProviderConfig,
  next: Option<&str>,
) -> Result<Response, LoginError> {
  if !user.enabled {
    return Err(LoginError::Disabled);
  }

  let max_age = config
    .session_max_age
    .and_then(|secs| i64::try_from(secs).ok())
    .unwrap_or(USER_SESSION_MAX_AGE_SECS);
  let (session_token, _) = repo::users::create_session_for(
    &state.pool,
    user.id,
    jiff::SignedDuration::from_secs(max_age),
  )
  .await?;
  audit_login(
    state,
    provider,
    &Actor::user(user.id, user.username),
    "LOGIN_SUCCESS",
    None,
  )
  .await;
  let cookies = [
    clear_flow,
    oauth_user_session_cookie(
      &session_token,
      &state.config.server,
      &config.redirect_uri,
      max_age,
    ),
    oidc_provider_cookie(
      provider,
      &state.config.server,
      &config.redirect_uri,
      max_age,
    ),
  ];

  Ok(
    (
      StatusCode::FOUND,
      AppendHeaders(cookies.map(|cookie| (SET_COOKIE, cookie))),
      Redirect::to(crate::routes::return_to(next).unwrap_or("/")),
    )
      .into_response(),
  )
}

#[derive(Deserialize)]
struct LoginParams {
  next: Option<String>,
}

async fn oidc_login(
  State(state): State<AppState>,
  Path(provider): Path<String>,
  Query(params): Query<LoginParams>,
) -> Response {
  match start_login(&state, &provider, None, params.next.as_deref()).await {
    Ok(response) => response,
    Err(error) => login_failure(&state, &provider, error).await,
  }
}

/// The provider's logout page for a session from `provider`, when that
/// provider has a `post_logout_redirect_uri` to come back to.
pub async fn end_session_url(
  state: &AppState,
  provider: &str,
) -> Option<String> {
  let config = state.config.oauth.oidc.get(provider)?;
  let back = config.post_logout_redirect_uri.as_deref()?;
  let idp = match cached_provider(state, provider, config).await {
    Ok(idp) => idp,
    Err(error) => {
      tracing::warn!(%provider, ?error, "OIDC provider unavailable at logout");
      return None;
    },
  };
  let mut url = Url::parse(idp.end_session_endpoint.as_deref()?).ok()?;

  url
    .query_pairs_mut()
    .append_pair("client_id", &config.client_id)
    .append_pair("post_logout_redirect_uri", back);
  Some(url.into())
}

/// Starts a flow that links the identity the provider returns to `user`,
/// rather than signing in with it.
pub async fn start_link(
  state: &AppState,
  provider: &str,
  user: Uuid,
) -> Response {
  match start_login(state, provider, Some(user), None).await {
    Ok(response) => response,
    Err(error) => login_failure(state, provider, error).await,
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

  use super::{IdTokenClaims, MAX_USERNAME_LEN, claim_groups, usernames};

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
  fn usernames_are_valid_and_fall_back_to_suffixes() {
    let long = claims(json!({
      "sub": "0f8fad5b-d9cb-469f-a165-70867728950e",
      "aud": "circus",
      "preferred_username": "jane.doe+ci@corp.example.com-with-a-long-tail",
    }));

    let [bare, suffixed, hashed] = usernames(&long, "sixteen-chars-ab");
    assert!(suffixed.ends_with("_sixteen-chars-ab"), "{suffixed}");
    assert!(hashed.ends_with("_sixteen-chars-ab"), "{hashed}");
    assert_ne!(suffixed, hashed);

    for name in [&bare, &suffixed, &hashed] {
      assert!(name.len() <= MAX_USERNAME_LEN, "{name}");
      assert!(
        name.chars().all(|character| {
          character.is_ascii_alphanumeric() || matches!(character, '-' | '_')
        }),
        "{name}"
      );
    }

    let email_only = claims(json!({
      "sub": "opaque-subject",
      "aud": "circus",
      "preferred_username": "",
      "email": "kilgore@kilgore.trout",
    }));
    let [bare, suffixed, _] = usernames(&email_only, "dex");
    assert_eq!(bare, "kilgore");
    assert_eq!(suffixed, "kilgore_dex");
  }
}
