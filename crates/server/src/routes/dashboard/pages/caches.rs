use axum::{
  extract::{Path, Query, State},
  response::{Html, IntoResponse, Response},
};

use super::{
  super::{
    shared::{
      CacheNarsParams,
      DashboardContext,
      DashboardPage,
      Pagination,
      RenderExt,
      enforce_page_access,
      format_bytes,
      store_path_hash,
    },
    templates::{
      CacheDetailTemplate,
      CacheNarsTemplate,
      CacheRowView,
      CachesTemplate,
      NarRowView,
    },
  },
  ui_config,
};
use crate::state::AppState;

fn cache_db_err(error: circus_common::CiError) -> Response {
  crate::error::ApiError(error).into_response()
}

fn fmt_opt_ts(ts: Option<chrono::DateTime<chrono::Utc>>) -> String {
  ts.map_or_else(
    || "-".to_owned(),
    |t| t.format("%Y-%m-%d %H:%M").to_string(),
  )
}

pub(in crate::routes::dashboard) async fn caches_page(
  State(state): State<AppState>,
  ctx: DashboardContext,
) -> Result<Html<String>, Response> {
  enforce_page_access(&state.config, &ctx, DashboardPage::Caches)?;
  let refs = crate::cache_overview::list_cache_refs(&state)
    .await
    .map_err(IntoResponse::into_response)?;

  let mut caches = Vec::with_capacity(refs.len());
  let mut total_nars = 0i64;
  let mut total_compressed = 0i64;
  let mut total_uncompressed = 0i64;
  for cache in refs {
    let storage = circus_common::repo::narinfo_cache::storage_summary(
      &state.pool,
      cache.scope,
    )
    .await
    .map_err(cache_db_err)?;
    let (requests_per_hour, _bytes) =
      circus_common::repo::cache_traffic::traffic_last_hour(
        &state.pool,
        &cache.name,
      )
      .await
      .map_err(cache_db_err)?;

    total_nars += storage.nar_count;
    total_compressed += storage.compressed_bytes;
    total_uncompressed += storage.uncompressed_bytes;
    caches.push(CacheRowView {
      detail_href: format!("/caches/{}", cache.name),
      scope_label: cache.scope_label().to_owned(),
      name: cache.name,
      active: cache.active,
      nar_count: storage.nar_count,
      compressed: format_bytes(storage.compressed_bytes),
      requests_per_hour,
    });
  }

  CachesTemplate {
    ui: ui_config(&state),
    is_admin: ctx.is_admin,
    auth_name: ctx.auth_name,
    total_nars,
    total_compressed: format_bytes(total_compressed),
    total_uncompressed: format_bytes(total_uncompressed),
    caches,
  }
  .render_html_or_500()
}

pub(in crate::routes::dashboard) async fn cache_detail_page(
  State(state): State<AppState>,
  ctx: DashboardContext,
  Path(name): Path<String>,
) -> Result<Html<String>, Response> {
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
    name: cache.name,
  }
  .render_html_or_500()
}

pub(in crate::routes::dashboard) async fn cache_nars_page(
  State(state): State<AppState>,
  ctx: DashboardContext,
  Path(name): Path<String>,
  Query(params): Query<CacheNarsParams>,
) -> Result<Html<String>, Response> {
  enforce_page_access(&state.config, &ctx, DashboardPage::CacheNars)?;
  let Some(cache) = crate::cache_overview::resolve_cache_ref(&state, &name)
    .await
    .map_err(IntoResponse::into_response)?
  else {
    return Err(super::super::shared::not_found("Cache"));
  };

  let limit = params.limit.unwrap_or(50).clamp(1, 200);
  let offset = params.offset.unwrap_or(0).max(0);
  let hash = params.hash.clone();
  let package = params.package.clone();

  let items = circus_common::repo::narinfo_cache::list_filtered(
    &state.pool,
    cache.scope,
    hash.as_deref(),
    package.as_deref(),
    limit,
    offset,
  )
  .await
  .map_err(cache_db_err)?;
  let total = circus_common::repo::narinfo_cache::count_filtered(
    &state.pool,
    cache.scope,
    hash.as_deref(),
    package.as_deref(),
  )
  .await
  .map_err(cache_db_err)?;
  let summary = circus_common::repo::narinfo_cache::storage_summary(
    &state.pool,
    cache.scope,
  )
  .await
  .map_err(cache_db_err)?;
  let (last_uploaded, oldest_fetched) =
    circus_common::repo::narinfo_cache::storage_extremes(
      &state.pool,
      cache.scope,
    )
    .await
    .map_err(cache_db_err)?;

  let nars = items
    .into_iter()
    .map(|it| {
      NarRowView {
        hash:         store_path_hash(&it.store_path),
        package:      it.package_name,
        nar_size:     format_bytes(it.nar_size),
        compressed:   it.file_size.map_or_else(|| "-".to_owned(), format_bytes),
        created_at:   it.created_at.format("%Y-%m-%d %H:%M").to_string(),
        last_fetched: it.last_fetched_at.map_or_else(
          || "Never".to_owned(),
          |t| t.format("%Y-%m-%d %H:%M").to_string(),
        ),
        store_path:   it.store_path,
      }
    })
    .collect();

  let pagination = Pagination::new(total, offset, limit);
  CacheNarsTemplate {
    ui: ui_config(&state),
    is_admin: ctx.is_admin,
    auth_name: ctx.auth_name,
    detail_href: format!("/caches/{}", cache.name),
    scope_label: cache.scope_label().to_owned(),
    name: cache.name,
    filter_hash: params.hash.unwrap_or_default(),
    filter_package: params.package.unwrap_or_default(),
    total_nars: summary.nar_count,
    nar_size: format_bytes(summary.uncompressed_bytes),
    file_size: format_bytes(summary.compressed_bytes),
    last_uploaded: fmt_opt_ts(last_uploaded),
    oldest_fetched: fmt_opt_ts(oldest_fetched),
    nars,
    page: pagination.page,
    total_pages: pagination.total_pages,
    has_prev: pagination.has_prev,
    has_next: pagination.has_next,
    prev_offset: pagination.prev_offset,
    next_offset: pagination.next_offset,
    limit,
  }
  .render_html_or_500()
}
