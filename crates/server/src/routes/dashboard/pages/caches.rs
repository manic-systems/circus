use axum::{
  extract::{Path, Query, State},
  response::{Html, IntoResponse, Response},
};
use circus_common::{
  models::SortDirection,
  repo::narinfo_cache::{NarSort, NarSortColumn},
};

use super::{
  super::{
    shared::{
      CacheNarsParams,
      DashboardContext,
      DashboardPage,
      PageError,
      Pagination,
      RenderExt,
      enforce_page_access,
      format_bytes,
      format_exact_bytes,
      store_path_hash,
    },
    templates::{
      CacheDetailTemplate,
      CacheNarsTemplate,
      CacheRowView,
      CachesTemplate,
      NarRowView,
      SortHeaderView,
    },
  },
  ui_config,
};
use crate::state::AppState;

fn cache_db_err(error: circus_common::CiError) -> Response {
  crate::error::ApiError(error).into_response()
}

fn fmt_opt_ts(ts: Option<jiff::Timestamp>) -> String {
  ts.map_or_else(
    || "-".to_owned(),
    |t| t.strftime("%Y-%m-%d %H:%M UTC").to_string(),
  )
}

pub(in crate::routes::dashboard) async fn caches_page(
  State(state): State<AppState>,
  ctx: DashboardContext,
) -> Result<Html<String>, PageError> {
  enforce_page_access(&state.config, &ctx, DashboardPage::Caches)?;
  let refs = crate::cache_overview::list_cache_refs(&state)
    .await
    .map_err(IntoResponse::into_response)?;

  // The unscoped (global) summary already covers every NAR; summing the
  // per-cache rows would double count project-scoped entries.
  let totals =
    circus_common::repo::narinfo_cache::storage_summary(&state.pool, None)
      .await
      .map_err(cache_db_err)?;

  let mut caches = Vec::with_capacity(refs.len());
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
    total_nars: totals.nar_count,
    total_compressed: format_bytes(totals.compressed_bytes),
    total_uncompressed: format_bytes(totals.uncompressed_bytes),
    caches,
  }
  .render_html_or_500()
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

/// Each header click steps its column through ascending, descending, and back
/// to the default newest-first order.
fn nar_sort_headers(
  detail_href: &str,
  params: &CacheNarsParams,
  active: Option<NarSort>,
) -> Vec<SortHeaderView> {
  NarSortColumn::ALL
    .into_iter()
    .map(|column| {
      let active_dir = active
        .filter(|sort| sort.column == column)
        .map(|sort| sort.direction);
      let next_dir = match active_dir {
        None => Some(SortDirection::Asc),
        Some(SortDirection::Asc) => Some(SortDirection::Desc),
        Some(SortDirection::Desc) => None,
      };

      let next_sort = next_dir.map(|direction| NarSort { column, direction });

      SortHeaderView {
        key:         column.as_str().to_owned(),
        label:       nar_sort_label(column).to_owned(),
        href:        nars_href(detail_href, params, next_sort, None),
        default_dir: SortDirection::Asc.as_str().to_owned(),
        active:      active_dir.is_some(),
        indicator:   active_dir.map_or("", SortDirection::as_str).to_owned(),
        aria_sort:   match active_dir {
          None => "none",
          Some(SortDirection::Asc) => "ascending",
          Some(SortDirection::Desc) => "descending",
        }
        .to_owned(),
      }
    })
    .collect()
}

/// `page` is an `(offset, limit)` pair, and `None` starts from the first page.
fn nars_href(
  detail_href: &str,
  params: &CacheNarsParams,
  sort: Option<NarSort>,
  page: Option<(i64, i64)>,
) -> String {
  let mut query = url::form_urlencoded::Serializer::new(String::new());
  if let Some(hash) = &params.hash {
    query.append_pair("hash", hash);
  }
  if let Some(package) = &params.package {
    query.append_pair("package", package);
  }
  if let Some(active) = sort {
    query.append_pair("sort", active.column.as_str());
    query.append_pair("dir", active.direction.as_str());
  }
  if let Some((offset, limit)) = page {
    query.append_pair("offset", &offset.to_string());
    query.append_pair("limit", &limit.to_string());
  }
  format!("{detail_href}/nars?{}", query.finish())
}

const fn nar_sort_label(column: NarSortColumn) -> &'static str {
  match column {
    NarSortColumn::Hash => "Hash",
    NarSortColumn::Package => "Package",
    NarSortColumn::NarSize => "NAR size",
    NarSortColumn::Compressed => "Compressed",
    NarSortColumn::Created => "Created",
    NarSortColumn::LastFetched => "Last fetched",
  }
}

pub(in crate::routes::dashboard) async fn cache_nars_page(
  State(state): State<AppState>,
  ctx: DashboardContext,
  Path(name): Path<String>,
  Query(params): Query<CacheNarsParams>,
) -> Result<Html<String>, PageError> {
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
  let sort = params.sort.map(|column| {
    NarSort {
      column,
      direction: params.dir.unwrap_or(SortDirection::Asc),
    }
  });

  let items = circus_common::repo::narinfo_cache::list_filtered(
    &state.pool,
    cache.scope,
    hash.as_deref(),
    package.as_deref(),
    sort,
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
        hash:             store_path_hash(&it.store_path),
        package:          it.package_name,
        nar_size:         format_bytes(it.nar_size),
        nar_bytes:        format_exact_bytes(it.nar_size),
        compressed:       it
          .file_size
          .map_or_else(|| "-".to_owned(), format_bytes),
        compressed_bytes: it
          .file_size
          .map(format_exact_bytes)
          .unwrap_or_default(),
        created_at:       it
          .created_at
          .strftime("%Y-%m-%d %H:%M UTC")
          .to_string(),
        created_iso:      it.created_at.to_string(),
        last_fetched:     it.last_fetched_at.map_or_else(
          || "Never".to_owned(),
          |t| t.strftime("%Y-%m-%d %H:%M UTC").to_string(),
        ),
        last_fetched_iso: it
          .last_fetched_at
          .map(|t| t.to_string())
          .unwrap_or_default(),
        store_path:       it.store_path,
      }
    })
    .collect();

  let pagination = Pagination::new(total, offset, limit);
  let detail_href = format!("/caches/{}", cache.name);
  let page_href = |page_offset| {
    nars_href(&detail_href, &params, sort, Some((page_offset, limit)))
  };
  let prev_href = page_href(pagination.prev_offset);
  let next_href = page_href(pagination.next_offset);
  let sort_headers = nar_sort_headers(&detail_href, &params, sort);

  CacheNarsTemplate {
    ui: ui_config(&state),
    is_admin: ctx.is_admin,
    auth_name: ctx.auth_name,
    sort_headers,
    sort_key: sort.map_or("", |active| active.column.as_str()).to_owned(),
    sort_dir: sort
      .map_or("", |active| active.direction.as_str())
      .to_owned(),
    detail_href,
    scope_label: cache.scope_label().to_owned(),
    name: cache.name,
    filter_hash: params.hash.unwrap_or_default(),
    filter_package: params.package.unwrap_or_default(),
    total_nars: summary.nar_count,
    nar_size: format_bytes(summary.uncompressed_bytes),
    file_size: format_bytes(summary.compressed_bytes),
    last_uploaded: fmt_opt_ts(last_uploaded),
    last_uploaded_iso: last_uploaded.map(|t| t.to_string()).unwrap_or_default(),
    oldest_fetched: fmt_opt_ts(oldest_fetched),
    oldest_fetched_iso: oldest_fetched
      .map(|t| t.to_string())
      .unwrap_or_default(),
    nars,
    page: pagination.page,
    total_pages: pagination.total_pages,
    has_prev: pagination.has_prev,
    has_next: pagination.has_next,
    prev_href,
    next_href,
  }
  .render_html_or_500()
}
