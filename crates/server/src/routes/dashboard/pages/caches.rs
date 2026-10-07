use axum::{
  extract::{Path, Query, State},
  response::{Html, IntoResponse, Response},
};

use super::{
  super::{
    shared::{
      DashboardContext,
      DashboardPage,
      PageError,
      RenderExt,
      enforce_page_access,
      format_bytes,
    },
    templates::CacheDetailTemplate,
  },
  ui_config,
};
use crate::state::AppState;

fn cache_db_err(error: circus_common::CiError) -> Response {
  crate::error::ApiError(error).into_response()
}

/// Result banner state for a completed `POST /caches/{name}/gc` redirect.
#[derive(Default, serde::Deserialize)]
pub(in crate::routes::dashboard) struct CacheGcNoticeParams {
  gc:         Option<String>,
  gc_deleted: Option<i64>,
  gc_freed:   Option<i64>,
  gc_failed:  Option<i64>,
}

impl CacheGcNoticeParams {
  fn notice(&self) -> (String, bool) {
    match self.gc.as_deref() {
      Some("error") => {
        (
          "Cache cleanup failed; see the server logs.".to_owned(),
          true,
        )
      },
      Some("done") => {
        let deleted = self.gc_deleted.unwrap_or(0);
        let freed = format_bytes(self.gc_freed.unwrap_or(0).max(0));
        let failed = self.gc_failed.unwrap_or(0);
        let mut notice = format!("Removed {deleted} cache entries ({freed}).");
        if failed > 0 {
          use std::fmt::Write as _;
          let _ = write!(
            notice,
            " {failed} backing objects could not be deleted; see the server \
             logs."
          );
        }
        (notice, failed > 0)
      },
      _ => (String::new(), false),
    }
  }
}

pub(in crate::routes::dashboard) async fn cache_detail_page(
  State(state): State<AppState>,
  ctx: DashboardContext,
  Path(name): Path<String>,
  Query(gc_params): Query<CacheGcNoticeParams>,
) -> Result<Html<String>, PageError> {
  enforce_page_access(&state.config, &ctx, DashboardPage::CacheDetail)?;
  let Some(cache) = crate::cache_overview::resolve_cache_ref(&state, &name)
    .await
    .map_err(IntoResponse::into_response)?
  else {
    return Err(super::super::shared::not_found("Cache"));
  };

  let storage = circus_common::repo::narinfo_cache::storage_summary(
    &state.pool,
    cache.scope,
  )
  .await
  .map_err(cache_db_err)?;
  let (requests_last_hour, bytes_served) =
    circus_common::repo::cache_traffic::traffic_last_hour(
      &state.pool,
      &cache.name,
    )
    .await
    .map_err(cache_db_err)?;

  let substituter =
    crate::cache_overview::substituter_url(&state.config, &cache);
  let public_key = crate::cache_overview::public_key(&state.config);
  let snippet = crate::cache_overview::nix_conf_snippet(
    substituter.as_deref(),
    public_key.as_deref(),
  );
  let (gc_notice, gc_error) = gc_params.notice();

  CacheDetailTemplate {
    ui: ui_config(&state),
    is_admin: ctx.is_admin,
    auth_name: ctx.auth_name,
    storage_timeseries_url: format!(
      "/api/v1/admin/caches/{}/storage-timeseries",
      cache.name
    ),
    traffic_timeseries_url: format!(
      "/api/v1/admin/caches/{}/traffic-timeseries",
      cache.name
    ),
    nars_href: format!("/caches/{}/nars", cache.name),
    scope_label: cache.scope_label().to_owned(),
    active: cache.active,
    packages_stored: storage.nar_count,
    uncompressed: format_bytes(storage.uncompressed_bytes),
    compressed: format_bytes(storage.compressed_bytes),
    requests_last_hour,
    traffic_last_hour: format_bytes(bytes_served),
    has_substituter: substituter.is_some(),
    substituter_url: substituter.unwrap_or_default(),
    has_public_key: public_key.is_some(),
    public_key: public_key.unwrap_or_default(),
    has_snippet: snippet.is_some(),
    nix_conf_snippet: snippet.unwrap_or_default(),
    csrf_token: ctx.csrf_token.clone(),
    gc_notice,
    gc_error,
    is_global: cache.scope.is_none(),
    name: cache.name,
  }
  .render_html_or_500()
}
