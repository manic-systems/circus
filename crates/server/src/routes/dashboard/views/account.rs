//! Sign-in and account pages.

use std::fmt;

use circus_common::models::User;
use circus_config::Config;
use serde::Deserialize;
use topcoat::{
  Result,
  context::{Cx, app_context},
  router::{
    error::{internal_server_error, not_found, see_other},
    page,
    path_param,
    query_params,
  },
  view::{View, component, view},
};
use url::form_urlencoded;

use crate::{
  routes::{
    dashboard::{
      components::result_code,
      layout::{document, signed_in},
      shared::DashboardContext,
    },
    return_to,
  },
  state::AppState,
};

/// Why a sign-in attempt failed, carried to the login page as `?error=`.
#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum SignInError {
  InvalidPassword,
  PasswordDisabled,
  MissingKey,
  InvalidKey,
  MissingCredentials,
  ProviderUnavailable,
  AccountConflict,
  AlreadyLinked,
  ProviderFailed,
}

impl SignInError {
  const fn message(self) -> &'static str {
    match self {
      Self::InvalidPassword => "Invalid username or password",
      Self::PasswordDisabled => "Password sign-in is disabled",
      Self::MissingKey => "API key is required",
      Self::InvalidKey => "Invalid API key",
      Self::MissingCredentials => {
        "Please provide either username/password or API key"
      },
      Self::ProviderUnavailable => {
        "Sign-in is temporarily unavailable. Please try again."
      },
      Self::AccountConflict => {
        "An account already uses this username or email."
      },
      Self::AlreadyLinked => {
        "This sign-in is already linked to an account, or yours already has \
         one from this provider."
      },
      Self::ProviderFailed => {
        "Unable to sign in. Please try again or contact an administrator."
      },
    }
  }

  /// The login page showing this error, keeping a safe `next` target.
  #[must_use]
  pub fn login_href(self, next: Option<&str>) -> String {
    let mut query = form_urlencoded::Serializer::new(String::new());
    query.append_pair("error", &self.to_string());

    if let Some(next) = return_to(next) {
      query.append_pair("next", next);
    }

    format!("/login?{}", query.finish())
  }
}

impl fmt::Display for SignInError {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    f.write_str(match self {
      Self::InvalidPassword => "invalid-password",
      Self::PasswordDisabled => "password-disabled",
      Self::MissingKey => "missing-key",
      Self::InvalidKey => "invalid-key",
      Self::MissingCredentials => "missing-credentials",
      Self::ProviderUnavailable => "provider-unavailable",
      Self::AccountConflict => "account-conflict",
      Self::AlreadyLinked => "already-linked",
      Self::ProviderFailed => "provider-failed",
    })
  }
}

/// Why linking a provider failed, carried as `?error=`.
#[derive(Clone, Copy, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum LinkError {
  IncorrectPassword,
}

impl LinkError {
  const fn message(self) -> &'static str {
    match self {
      Self::IncorrectPassword => "Incorrect password",
    }
  }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Icon {
  Github,
  Oidc,
}

struct LoginLink {
  href:  String,
  label: String,
  icon:  Icon,
}

fn providers(config: &Config, next: Option<&str>) -> Vec<LoginLink> {
  let query = next.map(|next| {
    form_urlencoded::Serializer::new(String::new())
      .append_pair("next", next)
      .finish()
  });
  let href = |base: String| {
    match &query {
      Some(query) => format!("{base}?{query}"),
      None => base,
    }
  };
  let github = config.oauth.github.as_ref().map(|_| {
    LoginLink {
      href:  href("/api/v1/auth/github".to_owned()),
      label: "GitHub".to_owned(),
      icon:  Icon::Github,
    }
  });
  let oidc = config.oauth.oidc.iter().map(|(name, provider)| {
    LoginLink {
      href:  href(format!("/api/v1/auth/oidc/{name}")),
      label: provider.display_name.clone(),
      icon:  Icon::Oidc,
    }
  });

  github.into_iter().chain(oidc).collect()
}

#[query_params(error = bad_request)]
struct LoginQuery {
  next:  Option<String>,
  /// Show the page even when it would hand straight off to a provider, so an
  /// API key can still be entered.
  local: Option<String>,
  error: Option<String>,
}

#[page("/login")]
async fn login(cx: &Cx) -> Result<impl View> {
  let state = app_context::<AppState>(cx);
  let query = query_params::<LoginQuery>(cx)?;
  let sign_in_error = result_code::<SignInError>(query.error.as_deref());
  let next = return_to(query.next.as_deref()).map(str::to_owned);
  let providers = providers(&state.config, next.as_deref());
  let password_login = state.config.server.password_login;

  if let [provider] = providers.as_slice()
    && !password_login
    && query.local.is_none()
    && sign_in_error.is_none()
  {
    return Err(see_other(provider.href.clone()).into());
  }

  let (viewer, _) = signed_in(cx).await;
  let brand = state.config.ui.brand_name.clone();
  let error = sign_in_error.map(SignInError::message);

  Ok(view! {
    document(title: "Sign in", viewer: &viewer, hide_auth: true,
      <div class="login-container">
        <div class="login-card form-card">
          <h1>"Sign in to " (brand)</h1>
          if let Some(message) = error {
            <div class="flash-message flash-error">(message)</div>
          }
          if password_login {
            <form method="POST" action="/login">
              next_field(next: next.clone())
              <div class="form-group">
                <label for="username">"Username"</label>
                <input type="text" id="username" name="username" autocomplete="username" autofocus=(true)>
              </div>
              <div class="form-group">
                <label for="password">"Password"</label>
                <input type="password" id="password" name="password" autocomplete="current-password">
              </div>
              <button type="submit" class="btn btn-full">"Sign in"</button>
            </form>
            <div class="login-divider">
              <span>"or"</span>
            </div>
          }
          <div class="login-alternatives">
            for provider in providers {
              <a href=(provider.href) class="btn btn-secondary btn-full">
                match provider.icon {
                  Icon::Github => github_icon(),
                  Icon::Oidc => shield_icon(),
                }
                "Continue with " (provider.label)
              </a>
            }
            <details class="login-api-key" ontoggle="this.open && this.querySelector('input').focus()">
              <summary class="btn btn-secondary btn-full">
                key_icon()
                "Continue with an API key"
              </summary>
              <form method="POST" action="/login">
                next_field(next: next.clone())
                <input
                  type="password"
                  name="api_key"
                  placeholder="circus_..."
                  aria-label="API key"
                  autocomplete="current-password"
                >
                <button type="submit" class="btn">"Sign in"</button>
              </form>
            </details>
          </div>
        </div>
      </div>
    )
  })
}

#[component]
async fn next_field(next: Option<String>) -> Result<impl View> {
  Ok(view! {
    if let Some(next) = next {
      <input type="hidden" name="next" value=(next)>
    }
  })
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum LinkStatus {
  /// The account was created by signing in with this provider.
  Native,
  Linked,
  Unlinked,
}

struct AccountProvider {
  name:   String,
  label:  String,
  status: LinkStatus,
}

/// The signed-in user, or a redirect to sign in that comes back to `next`.
async fn account_user(
  cx: &Cx,
  next: Option<&str>,
) -> Result<(DashboardContext, User)> {
  let (viewer, user) = signed_in(cx).await;
  let Some(user) = user else {
    let href = next.map_or_else(
      || "/login".to_owned(),
      |next| format!("/login?next={next}"),
    );
    return Err(see_other(href).into());
  };

  Ok((viewer, user))
}

#[page("/account")]
async fn account(cx: &Cx) -> Result<impl View> {
  let state = app_context::<AppState>(cx);
  let (viewer, user) = account_user(cx, Some("/account")).await?;
  let linked =
    circus_common::repo::users::linked_providers(&state.pool, user.id)
      .await
      .map_err(|error| {
        tracing::error!(user_id = %user.id, "failed to list linked identities: {error}");
        internal_server_error(error)
      })?;
  let native_issuer =
    circus_common::repo::users::native_external_id(&state.pool, user.id)
      .await
      .map_err(|error| {
        tracing::error!(user_id = %user.id, "failed to read account identity: {error}");
        internal_server_error(error)
      })?
      .and_then(|id| id.rsplit_once('#').map(|(issuer, _)| issuer.to_owned()));
  let providers: Vec<AccountProvider> = state
    .config
    .oauth
    .oidc
    .iter()
    .map(|(name, provider)| {
      let native = native_issuer.as_deref().is_some_and(|issuer| {
        issuer.trim_end_matches('/')
          == provider.issuer_url.trim_end_matches('/')
      });

      AccountProvider {
        name:   name.clone(),
        label:  provider.display_name.clone(),
        status: if native {
          LinkStatus::Native
        } else if linked.contains(name) {
          LinkStatus::Linked
        } else {
          LinkStatus::Unlinked
        },
      }
    })
    .collect();
  let has_password =
    state.config.server.password_login && user.password_hash.is_some();
  let role = user.role.to_string();
  let csrf_token = viewer.csrf_token.clone();

  Ok(view! {
    document(title: "Account", viewer: &viewer,
      <nav class="breadcrumbs">
        <a href="/">"Home"</a>
        <span class="sep">"/"</span>
        <span class="current">"Account"</span>
      </nav>
      <div class="account-header">
        <h1>(user.username)</h1>
        <span class="account-role">(role)</span>
      </div>
      <section class="panel account-panel">
        <div class="panel-header">
          <h2>"Sign-in methods"</h2>
        </div>
        <ul class="account-methods">
          if has_password {
            <li class="account-method">
              lock_icon()
              <div class="account-method-name">
                "Password"
                <span class="account-method-status linked">"Set"</span>
              </div>
            </li>
          }
          for provider in providers {
            account_method(provider: provider, csrf_token: csrf_token.clone())
          }
        </ul>
      </section>
    )
  })
}

#[component]
async fn account_method(
  provider: AccountProvider,
  csrf_token: String,
) -> Result<impl View> {
  let unlink = format!("/account/unlink/{}", provider.name);
  let link = format!("/account/link/{}", provider.name);

  Ok(view! {
    <li class="account-method">
      shield_icon()
      <div class="account-method-name">
        (provider.label)
        match provider.status {
          LinkStatus::Native => {
            <span class="account-method-status linked">"Account created with this provider"</span>
          },
          LinkStatus::Linked => <span class="account-method-status linked">"Linked"</span>,
          LinkStatus::Unlinked => <span class="account-method-status">"Not linked"</span>,
        }
      </div>
      match provider.status {
        LinkStatus::Native => {},
        LinkStatus::Linked => {
          <form method="POST" action=(unlink)>
            <input type="hidden" name="csrf_token" value=(csrf_token)>
            <button class="btn btn-small btn-secondary" type="submit">"Unlink"</button>
          </form>
        },
        LinkStatus::Unlinked => <a class="btn btn-small" href=(link)>"Link"</a>,
      }
    </li>
  })
}

path_param!(provider);

#[query_params(error = bad_request)]
struct LinkQuery {
  error: Option<String>,
}

#[page("/account/link/{provider}")]
async fn account_link(cx: &Cx) -> Result<impl View> {
  let state = app_context::<AppState>(cx);
  let (viewer, user) = account_user(cx, None).await?;
  let name = path_param::<Provider>(cx).to_owned();
  let config = state.config.oauth.oidc.get(&name).ok_or_else(not_found)?;
  let label = config.display_name.clone();
  let error =
    result_code::<LinkError>(query_params::<LinkQuery>(cx)?.error.as_deref())
      .map(LinkError::message);
  let has_password = user.password_hash.is_some();
  let action = format!("/account/link/{name}");
  let link_label = format!("Link {label}");

  Ok(view! {
    document(title: &link_label, viewer: &viewer,
      <nav class="breadcrumbs">
        <a href="/">"Home"</a>
        <span class="sep">"/"</span>
        <a href="/account">"Account"</a>
        <span class="sep">"/"</span>
        <span class="current">(link_label.clone())</span>
      </nav>
      <div class="form-card account-card">
        <h1>(link_label.clone())</h1>
        <p class="account-hint">
          "You will sign in with " (label.clone())
          ", and that identity will then sign you in as "
          <strong>(user.username)</strong> "."
        </p>
        if let Some(message) = error {
          <div class="flash-message flash-error">(message)</div>
        }
        <form method="POST" action=(action)>
          <input type="hidden" name="csrf_token" value=(viewer.csrf_token.clone())>
          if has_password {
            <div class="form-group">
              <label for="password">"Confirm your password"</label>
              <input
                type="password"
                id="password"
                name="password"
                autocomplete="current-password"
                required=(true)
                autofocus=(true)
              >
            </div>
          }
          <div class="account-actions">
            <button class="btn" type="submit">"Continue to " (label)</button>
            <a class="btn btn-secondary" href="/account">"Cancel"</a>
          </div>
        </form>
      </div>
    )
  })
}

#[component]
async fn github_icon() -> Result<impl View> {
  Ok(view! {
    <svg class="login-icon" viewBox="0 0 24 24" fill="currentColor" aria-hidden="true">
      <path d="M12 .297c-6.63 0-12 5.373-12 12 0 5.303 3.438 9.8 8.205 11.385.6.113.82-.258.82-.577 0-.285-.01-1.04-.015-2.04-3.338.724-4.042-1.61-4.042-1.61C4.422 18.07 3.633 17.7 3.633 17.7c-1.087-.744.084-.729.084-.729 1.205.084 1.838 1.236 1.838 1.236 1.07 1.835 2.809 1.305 3.495.998.108-.776.417-1.305.76-1.605-2.665-.3-5.466-1.332-5.466-5.93 0-1.31.465-2.38 1.235-3.22-.135-.303-.54-1.523.105-3.176 0 0 1.005-.322 3.3 1.23.96-.267 1.98-.399 3-.405 1.02.006 2.04.138 3 .405 2.28-1.552 3.285-1.23 3.285-1.23.645 1.653.24 2.873.12 3.176.765.84 1.23 1.91 1.23 3.22 0 4.61-2.805 5.625-5.475 5.92.42.36.81 1.096.81 2.22 0 1.606-.015 2.896-.015 3.286 0 .315.21.69.825.57C20.565 22.092 24 17.592 24 12.297c0-6.627-5.373-12-12-12"></path>
    </svg>
  })
}

#[component]
async fn shield_icon() -> Result<impl View> {
  Ok(view! {
    <svg class="login-icon" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
      <path d="M20 13c0 5-3.5 7.5-7.66 8.95a1 1 0 0 1-.67-.01C7.5 20.5 4 18 4 13V6a1 1 0 0 1 1-1c2 0 4.5-1.2 6.24-2.72a1.17 1.17 0 0 1 1.52 0C14.51 3.81 17 5 19 5a1 1 0 0 1 1 1z"></path>
      <path d="m9 12 2 2 4-4"></path>
    </svg>
  })
}

#[component]
async fn key_icon() -> Result<impl View> {
  Ok(view! {
    <svg class="login-icon" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
      <path d="M2.586 17.414A2 2 0 0 0 2 18.828V21a1 1 0 0 0 1 1h3a1 1 0 0 0 1-1v-1a1 1 0 0 1 1-1h1a1 1 0 0 0 1-1v-1a1 1 0 0 1 1-1h.172a2 2 0 0 0 1.414-.586l.814-.814a6.5 6.5 0 1 0-4-4z"></path>
      <circle cx="16.5" cy="7.5" r=".5" fill="currentColor"></circle>
    </svg>
  })
}

#[component]
async fn lock_icon() -> Result<impl View> {
  Ok(view! {
    <svg class="login-icon" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
      <rect width="18" height="11" x="3" y="11" rx="2" ry="2"></rect>
      <path d="M7 11V7a5 5 0 0 1 10 0v4"></path>
    </svg>
  })
}
