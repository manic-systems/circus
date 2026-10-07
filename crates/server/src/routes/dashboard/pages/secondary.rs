use axum::{
  extract::State,
  response::{Html, Redirect},
};

use super::{
  super::{
    shared::{
      DashboardContext,
      DashboardPage,
      PageError,
      RenderExt,
      enforce_page_access,
    },
    templates::{MetricsTemplate, ProjectSetupTemplate},
  },
  ui_config,
};
use crate::state::AppState;

pub(in crate::routes::dashboard) async fn metrics_page(
  State(state): State<AppState>,
  ctx: DashboardContext,
) -> Result<Html<String>, PageError> {
  enforce_page_access(&state.config, &ctx, DashboardPage::Metrics)?;
  MetricsTemplate {
    ui:        ui_config(&state),
    is_admin:  ctx.is_admin,
    auth_name: ctx.auth_name,
  }
  .render_html_or_500()
}

pub(in crate::routes::dashboard) async fn project_setup_page(
  State(state): State<AppState>,
  ctx: DashboardContext,
) -> Result<Html<String>, PageError> {
  if !ctx.is_admin {
    let target = if ctx.auth_name.is_empty() {
      "/login"
    } else {
      "/projects"
    };
    return Err(PageError::new(Redirect::to(target)));
  }

  ProjectSetupTemplate {
    ui:         ui_config(&state),
    is_admin:   ctx.is_admin,
    auth_name:  ctx.auth_name,
    csrf_token: ctx.csrf_token,
  }
  .render_html_or_500()
}
