//! The dashboard shell and access checks shared by every Topcoat page.

use axum::http::Extensions;
use circus_common::models::User;
use circus_config::PageAccessLevel;
use topcoat::{
  Result,
  context::{Cx, app_context},
  router::{
    Slot,
    StatusCode,
    error::{UnauthorizedError, see_other, unauthorized},
    layout,
    request,
  },
  view::{Child, View, component, error_boundary, view},
};

use super::shared::{DashboardContext, DashboardPage, UiTemplateConfig};
use crate::{auth_middleware::session_extensions, state::AppState};

const STYLESHEET: &str = "/static/style.css?v=graphite-dashboard-v19";

struct NavLink {
  href:  &'static str,
  label: &'static str,
  admin: bool,
}

const fn link(href: &'static str, label: &'static str) -> NavLink {
  NavLink {
    href,
    label,
    admin: false,
  }
}

const fn admin_link(href: &'static str, label: &'static str) -> NavLink {
  NavLink {
    href,
    label,
    admin: true,
  }
}

const NAV: &[(&str, &[NavLink])] = &[
  ("Operate", &[
    link("/builds", "Builds"),
    link("/queue", "Queue"),
    link("/evaluations", "Evaluations"),
    link("/projects", "Projects"),
    link("/channels", "Channels"),
    link("/starred", "Starred"),
    link("/metrics", "Metrics"),
  ]),
  ("System", &[
    link("/news", "News"),
    admin_link("/caches", "Caches"),
    admin_link("/users", "Users"),
    admin_link("/admin", "Admin"),
  ]),
];

/// Nav entries highlighted for detail pages that have no link of their own.
const NAV_PARENTS: &[(&str, &str)] = &[
  ("/build/", "/builds"),
  ("/project/", "/projects"),
  ("/jobset/", "/projects"),
  ("/evaluation/", "/evaluations"),
  ("/channel/", "/channels"),
];

/// Shard and WebSocket requests skip axum's middleware, so the session comes
/// from the cookie here.
pub async fn viewer(cx: &Cx, page: DashboardPage) -> Result<DashboardContext> {
  let viewer = session(cx).await;

  if allowed(cx, &viewer, page) {
    Ok(viewer)
  } else if viewer.is_authenticated {
    Err(see_other("/").into())
  } else {
    Err(unauthorized().into())
  }
}

/// [`viewer`] for a shard. Its endpoint has no layout to render the private
/// page, and a run that already rendered can only redirect.
pub async fn shard_viewer(
  cx: &Cx,
  page: DashboardPage,
) -> Result<DashboardContext> {
  let viewer = session(cx).await;

  if allowed(cx, &viewer, page) {
    Ok(viewer)
  } else if viewer.is_authenticated {
    Err(see_other("/").into())
  } else {
    Err(see_other("/login").into())
  }
}

fn allowed(cx: &Cx, viewer: &DashboardContext, page: DashboardPage) -> bool {
  match page.access(&app_context::<AppState>(cx).config.server) {
    PageAccessLevel::Public => true,
    PageAccessLevel::Authenticated => viewer.is_authenticated,
    PageAccessLevel::Admin => viewer.is_admin,
  }
}

async fn session(cx: &Cx) -> DashboardContext {
  signed_in(cx).await.0
}

/// The viewer and their user row, for pages outside the page access table.
pub async fn signed_in(cx: &Cx) -> (DashboardContext, Option<User>) {
  let state = app_context::<AppState>(cx);
  let session = session_extensions(state, request::headers(cx)).await;
  (
    DashboardContext::from_extensions(&session),
    session.get::<User>().cloned(),
  )
}

fn login_href(cx: &Cx) -> String {
  let uri = request::uri(cx);
  let here = uri.path_and_query().map_or("/", |path| path.as_str());

  if uri.path() == "/login" {
    return "/login".to_owned();
  }

  let next: String =
    url::form_urlencoded::byte_serialize(here.as_bytes()).collect();
  format!("/login?next={next}")
}

fn nav_active(path: &str, href: &str) -> bool {
  let section = NAV_PARENTS
    .iter()
    .find(|(prefix, _)| path.starts_with(prefix))
    .map_or(path, |(_, parent)| parent);

  if href == "/" {
    path == "/"
  } else {
    section == href || path == href || path.starts_with(&format!("{href}/"))
  }
}

#[layout("/")]
pub async fn root(cx: &Cx, slot: Slot<'_>) -> Result<impl View> {
  let login = login_href(cx);
  let anonymous = DashboardContext::from_extensions(&Extensions::default());

  Ok(view! {
    error_boundary(
      fallback: move |error| {
        if error.downcast_ref::<UnauthorizedError>().is_none() {
          return Err(error);
        }

        Ok(view! {
          (StatusCode::UNAUTHORIZED)
          document(title: "Sign in required", viewer: &anonymous,
            <div class="login-container">
              <div class="private-notice">
                <p class="private-title">"This page is private"</p>
                <p class="private-hint">"Sign in to access this content."</p>
                <a class="btn" href=(login.as_str())>"Sign in"</a>
              </div>
            </div>
          )
        })
      },
      (slot)
    )
  })
}

#[component]
pub async fn document(
  cx: &Cx,
  title: &str,
  viewer: &DashboardContext,
  /// Leaves out the sign-in controls, for the login page itself.
  #[default]
  hide_auth: bool,
  #[default(true)] topbar: bool,
  child: Child<'_>,
) -> Result<impl View> {
  let state = app_context::<AppState>(cx);
  let ui = UiTemplateConfig::from_config(&state.config.ui);

  Ok(view! {
    <!DOCTYPE html>
    <html lang="en">
      head(title: title, ui: &ui)
      <body>
        <div class="app-shell">
          sidebar(ui: &ui, viewer: viewer)
          <div class="shell-main">
            if topbar {
              header(viewer: viewer, hide_auth: hide_auth)
            }
            <main class="page-main">
              <div class="container">(child)</div>
            </main>
            <footer class="footer">
              <p>(ui.brand_name.as_str()) " — " (ui.brand_subtitle.as_str())</p>
              <p class="footer-version mono">"circus " (ui.version)</p>
            </footer>
          </div>
        </div>
      </body>
    </html>
  })
}

#[component]
async fn head(title: &str, ui: &UiTemplateConfig) -> Result<impl View> {
  let description = format!(
    "{} dashboard for Nix CI builds, evaluations, queues, and binary caches.",
    ui.brand_name
  );

  Ok(view! {
    <head>
      <meta charset="UTF-8">
      <meta name="viewport" content="width=device-width, initial-scale=1.0">
      <meta name="color-scheme" content="light dark">
      <meta name="description" content=(description)>
      <title>(title) " - " (ui.brand_name.as_str())</title>
      if ui.has_favicon {
        <link rel="icon" href=(ui.favicon_url.as_str())>
      }
      <link rel="stylesheet" href=(STYLESHEET)>
      <link rel="stylesheet" href="/static/theme.css">
      if ui.has_custom_css {
        <link rel="stylesheet" href="/static/custom.css">
      }
      topcoat::runtime::script()
    </head>
  })
}

#[component]
async fn sidebar(
  cx: &Cx,
  ui: &UiTemplateConfig,
  viewer: &DashboardContext,
) -> Result<impl View> {
  let path = request::uri(cx).path();

  Ok(view! {
    <aside class="sidebar" aria-label="Primary navigation">
      <div class="sidebar-brand">
        <a href="/">
          if ui.has_logo {
            <img class="brand-logo" src=(ui.logo_url.as_str()) alt="" aria-hidden="true">
          }
          <span class="brand-copy">
            <span class="brand-name">(ui.brand_name.as_str())</span>
            <span class="brand-subtitle">(ui.brand_subtitle.as_str())</span>
          </span>
        </a>
      </div>
      for (label, links) in NAV {
        <div class="nav-group">
          <div class="nav-label">(*label)</div>
          <div class="nav-links">
            for link in links.iter().filter(|link| viewer.is_admin || !link.admin) {
              <a
                href=(link.href)
                class=(if nav_active(path, link.href) { "active" } else { "" })
              >
                (link.label)
              </a>
            }
          </div>
        </div>
      }
    </aside>
  })
}

#[component]
async fn header(
  cx: &Cx,
  viewer: &DashboardContext,
  hide_auth: bool,
) -> Result<impl View> {
  let login = login_href(cx);

  Ok(view! {
    <header class="topbar">
      <div class="topbar-title">
        <strong>"Operations"</strong>
        <span>"Nix CI"</span>
      </div>
      <form class="topbar-search" method="get" action="/builds">
        <input
          class="command-input"
          type="search"
          name="job_name"
          placeholder="Filter builds by job name"
          aria-label="Filter builds by job name"
        >
        <button class="btn btn-small btn-secondary" type="submit">"Search"</button>
      </form>
      <div class="topbar-actions nav-auth">
        if !hide_auth {
          if viewer.auth_name.is_empty() {
            <a class="btn btn-secondary" href=(login)>"Login"</a>
          } else {
            <a class="auth-user" href="/account">(viewer.auth_name.as_str())</a>
            <form method="POST" action="/logout">
              <button class="btn-ghost" type="submit">"Logout"</button>
            </form>
          }
        }
      </div>
    </header>
  })
}
