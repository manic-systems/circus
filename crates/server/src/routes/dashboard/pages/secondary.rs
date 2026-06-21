use axum::{
  extract::{Path, State},
  response::{Html, IntoResponse, Redirect, Response},
};
use circus_common::models::BuildStatus;
use uuid::Uuid;

use super::{
  super::{
    shared::{
      DashboardContext,
      DashboardPage,
      RenderExt,
      StarredJobView,
      build_view,
      enforce_page_access,
      not_found,
      status_badge,
    },
    templates::{
      ChannelTemplate,
      ChannelView,
      ChannelsTemplate,
      MetricsTemplate,
      ProjectSetupTemplate,
      StarredTemplate,
    },
  },
  ui_config,
};
use crate::state::AppState;

pub(in crate::routes::dashboard) async fn channels_page(
  State(state): State<AppState>,
  ctx: DashboardContext,
) -> Result<Html<String>, Response> {
  enforce_page_access(&state.config, &ctx, DashboardPage::Channels)?;
  let channels = circus_common::repo::channels::list_all(&state.pool)
    .await
    .unwrap_or_default();

  let channel_views = channels
    .into_iter()
    .map(|channel| {
      let has_eval = channel.current_evaluation_id.is_some();
      ChannelView {
        id:                    channel.id,
        name:                  channel.name,
        current_evaluation_id: channel.current_evaluation_id,
        updated_at:            channel
          .updated_at
          .format("%Y-%m-%d %H:%M UTC")
          .to_string(),
        status_text:           if has_eval {
          "Active".into()
        } else {
          "Pending".into()
        },
        status_class:          if has_eval {
          "completed".into()
        } else {
          "pending".into()
        },
        job_count:             0,
      }
    })
    .collect();

  ChannelsTemplate {
    ui:        ui_config(&state),
    channels:  channel_views,
    is_admin:  ctx.is_admin,
    auth_name: ctx.auth_name.clone(),
  }
  .render_html_or_500()
}

pub(in crate::routes::dashboard) async fn channel_page(
  State(state): State<AppState>,
  Path(id): Path<Uuid>,
  ctx: DashboardContext,
) -> Result<Html<String>, Response> {
  enforce_page_access(&state.config, &ctx, DashboardPage::Channel)?;
  let Ok(channel) = circus_common::repo::channels::get(&state.pool, id).await
  else {
    return Err(not_found("Channel"));
  };

  let builds = if let Some(eval_id) = channel.current_evaluation_id {
    circus_common::repo::builds::list_for_evaluation(&state.pool, eval_id)
      .await
      .unwrap_or_default()
  } else {
    Vec::new()
  };

  let succeeded_count = builds
    .iter()
    .filter(|b| b.status == BuildStatus::Succeeded)
    .count() as i64;
  let failed_count = builds
    .iter()
    .filter(|b| {
      matches!(
        b.status,
        BuildStatus::Failed
          | BuildStatus::FailedWithOutput
          | BuildStatus::Timeout
          | BuildStatus::DependencyFailed
          | BuildStatus::Aborted
      )
    })
    .count() as i64;
  let pending_count = builds
    .iter()
    .filter(|b| matches!(b.status, BuildStatus::Pending | BuildStatus::Running))
    .count() as i64;

  ChannelTemplate {
    ui: ui_config(&state),
    channel,
    builds: builds.iter().map(build_view).collect(),
    succeeded_count,
    failed_count,
    pending_count,
    is_admin: ctx.is_admin,
    auth_name: ctx.auth_name.clone(),
  }
  .render_html_or_500()
}

pub(in crate::routes::dashboard) async fn starred_page(
  State(state): State<AppState>,
  ctx: DashboardContext,
) -> Result<Html<String>, Response> {
  enforce_page_access(&state.config, &ctx, DashboardPage::Starred)?;
  let viewer_user_id = ctx.viewer_user_id;
  let is_logged_in = viewer_user_id.is_some();

  let starred_jobs = if let Some(uid) = viewer_user_id {
    let starred = circus_common::repo::starred_jobs::list_for_user(
      &state.pool,
      uid,
      100,
      0,
    )
    .await
    .unwrap_or_default();

    let mut views = Vec::new();
    for s in starred {
      let project_name =
        circus_common::repo::projects::get(&state.pool, s.project_id)
          .await
          .map_or_else(|_| "-".to_string(), |p| p.name);

      let jobset_name = if let Some(js_id) = s.jobset_id {
        circus_common::repo::jobsets::get(&state.pool, js_id)
          .await
          .map_or_else(|_| "-".to_string(), |j| j.name)
      } else {
        "-".to_string()
      };

      let (status_text, status_class, latest_build_id) =
        if let Some(js_id) = s.jobset_id {
          let evals =
            circus_common::repo::evaluations::list_filtered_with_visibility(
              &state.pool,
              Some(js_id),
              None,
              1,
              0,
              ctx.is_admin,
            )
            .await
            .unwrap_or_default();

          let builds = if let Some(eval) = evals.first() {
            circus_common::repo::builds::list_filtered(
              &state.pool,
              Some(eval.id),
              None,
              None,
              Some(&s.job_name),
              1,
              0,
            )
            .await
            .unwrap_or_default()
          } else {
            Vec::new()
          };

          builds.first().map_or_else(
            || ("No builds".to_string(), "pending".to_string(), None),
            |build| {
              let (text, class) = status_badge(build.status);
              (text, class, Some(build.id))
            },
          )
        } else {
          ("No builds".to_string(), "pending".to_string(), None)
        };

      views.push(StarredJobView {
        id: s.id,
        project_id: s.project_id,
        project_name,
        jobset_id: s.jobset_id,
        jobset_name,
        job_name: s.job_name,
        status_text,
        status_class,
        latest_build_id,
      });
    }
    views
  } else {
    Vec::new()
  };

  StarredTemplate {
    ui: ui_config(&state),
    starred_jobs,
    is_logged_in,
    is_admin: ctx.is_admin,
    auth_name: ctx.auth_name.clone(),
    csrf_token: ctx.csrf_token.clone(),
  }
  .render_html_or_500()
}

pub(in crate::routes::dashboard) async fn metrics_page(
  State(state): State<AppState>,
  ctx: DashboardContext,
) -> Result<Html<String>, Response> {
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
) -> Result<Html<String>, Response> {
  if !ctx.is_admin {
    let target = if ctx.auth_name.is_empty() {
      "/login"
    } else {
      "/projects"
    };
    return Err(Redirect::to(target).into_response());
  }

  ProjectSetupTemplate {
    ui:         ui_config(&state),
    is_admin:   ctx.is_admin,
    auth_name:  ctx.auth_name,
    csrf_token: ctx.csrf_token,
  }
  .render_html_or_500()
}
