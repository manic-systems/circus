//! Channels, news, starred jobs and the cache listings.

use axum::extract::Query;
use circus_common::{
  CiError,
  models::{BuildStatus, SortDirection},
  repo::narinfo_cache::{NarSort, NarSortColumn},
};
use jiff::Timestamp;
use topcoat::{
  Result,
  context::{Cx, app_context},
  router::{
    error::{bad_request, internal_server_error, not_found},
    page,
    path_param,
    request,
  },
  view::{View, component, view},
};
use uuid::Uuid;

use super::super::{
  components::{confirm_button, local_time},
  layout::{document, viewer},
  shared::{
    BuildView,
    CacheNarsParams,
    DashboardContext,
    DashboardPage,
    Pagination,
    SortHeaderView,
    StarredJobView,
    build_view,
    format_bytes,
    format_exact_bytes,
    status_badge,
    store_path_hash,
  },
};
use crate::{error::ApiError, state::AppState};

path_param!(id: Uuid, error = not_found);
path_param!(name);

fn db_error(error: CiError) -> topcoat::Error {
  internal_server_error(error).into()
}

fn api_error(error: ApiError) -> topcoat::Error {
  db_error(error.0)
}

#[component]
async fn breadcrumbs(
  trail: Vec<(String, String)>,
  current: String,
) -> Result<impl View> {
  Ok(view! {
    <nav class="breadcrumbs">
      <a href="/">"Home"</a>
      for (href, label) in trail {
        <span class="sep">"/"</span>
        <a href=(href)>(label)</a>
      }
      <span class="sep">"/"</span>
      <span class="current">(current)</span>
    </nav>
  })
}

#[component]
async fn empty(title: &str, hint: Option<&str>) -> Result<impl View> {
  Ok(view! {
    <div class="empty">
      <div class="empty-title">(title)</div>
      if let Some(hint) = hint {
        <div class="empty-hint">(hint)</div>
      }
    </div>
  })
}

#[component]
async fn optional_time(
  at: Option<Timestamp>,
  missing: &str,
) -> Result<impl View> {
  Ok(view! {
    match at {
      Some(at) => local_time(at: at),
      None => (missing.to_owned()),
    }
  })
}

#[page("/channels")]
async fn channels_page(cx: &Cx) -> Result<impl View> {
  let viewer = viewer(cx, DashboardPage::Channels).await?;
  let state = app_context::<AppState>(cx);
  let channels = circus_common::repo::channels::list_all(&state.pool)
    .await
    .unwrap_or_default();

  Ok(view! {
    document(title: "Channels", viewer: &viewer,
      breadcrumbs(trail: Vec::new(), current: "Channels".to_owned())
      <h1>"Channels"</h1>
      if channels.is_empty() {
        empty(
          title: "No channels configured",
          hint: Some("Channels track successful evaluations for stable release tracking."),
        )
      } else {
        <section class="panel">
          <div class="panel-header">
            <h2>"Channels"</h2>
          </div>
          <div class="table-wrap compact-table-wrap">
            <table>
              <thead>
                <tr>
                  <th>"Name"</th>
                  <th>"Status"</th>
                  <th>"Jobs"</th>
                  <th>"Current Evaluation"</th>
                  <th>"Updated"</th>
                  <th></th>
                </tr>
              </thead>
              <tbody>
                #[key(*channel.id.as_bytes())]
                for channel in channels {
                  <tr>
                    <td><a href=(format!("/channel/{}", channel.id))>(channel.name)</a></td>
                    <td>
                      if channel.current_evaluation_id.is_some() {
                        <span class="badge badge-completed">"Active"</span>
                      } else {
                        <span class="badge badge-pending">"Pending"</span>
                      }
                    </td>
                    <td class="numeric">"0"</td>
                    <td>
                      match channel.current_evaluation_id {
                        Some(eval_id) => <a href=(format!("/evaluation/{eval_id}"))>(eval_id.to_string())</a>,
                        None => "-",
                      }
                    </td>
                    <td>local_time(at: channel.updated_at)</td>
                    <td class="row-actions">
                      <a class="btn btn-small btn-secondary" href=(format!("/channel/{}", channel.id))>"Browse"</a>
                    </td>
                  </tr>
                }
              </tbody>
            </table>
          </div>
        </section>
      }
    )
  })
}

#[page("/channel/{id}")]
async fn channel_page(cx: &Cx) -> Result<impl View> {
  let viewer = viewer(cx, DashboardPage::Channel).await?;
  let state = app_context::<AppState>(cx);
  let id = *path_param::<Id>(cx)?;
  let channel = circus_common::repo::channels::get(&state.pool, id)
    .await
    .map_err(|_| not_found())?;

  let mut builds = match channel.current_evaluation_id {
    Some(eval_id) => {
      circus_common::repo::builds::list_for_evaluation(&state.pool, eval_id)
        .await
        .unwrap_or_default()
    },
    None => Vec::new(),
  };
  builds.retain(|build| !build.kind.is_effect());
  let count = |pred: fn(BuildStatus) -> bool| {
    builds.iter().filter(|build| pred(build.status)).count()
  };
  let succeeded = count(|status| status == BuildStatus::Succeeded);
  let failed = count(|status| {
    matches!(
      status,
      BuildStatus::Failed
        | BuildStatus::FailedWithOutput
        | BuildStatus::Timeout
        | BuildStatus::DependencyFailed
        | BuildStatus::Aborted
    )
  });
  let pending = count(|status| {
    matches!(status, BuildStatus::Pending | BuildStatus::Running)
  });
  let builds: Vec<BuildView> = builds.iter().map(build_view).collect();
  let title = format!("Channel {}", channel.name);
  let nix_channel = format!(
    "nix-channel --add http://<server>/api/v1/channels/{}/nixexprs.tar.xz {}",
    channel.id, channel.name
  );

  Ok(view! {
    document(title: &title, viewer: &viewer,
      breadcrumbs(
        trail: vec![("/channels".to_owned(), "Channels".to_owned())],
        current: channel.name.clone(),
      )
      <h1>"Channel: " (channel.name.clone())</h1>
      <dl class="detail-grid">
        <dt>"Current Evaluation"</dt>
        <dd>
          match channel.current_evaluation_id {
            Some(eval_id) => <a href=(format!("/evaluation/{eval_id}"))>(eval_id.to_string())</a>,
            None => <em>"None"</em>,
          }
        </dd>
        <dt>"Updated"</dt>
        <dd>local_time(at: channel.updated_at)</dd>
        <dt>"Nix Channel"</dt>
        <dd><code>(nix_channel)</code></dd>
      </dl>
      if builds.is_empty() {
        if channel.current_evaluation_id.is_none() {
          empty(
            title: "No evaluation promoted to this channel yet",
            hint: Some("Use the API or admin panel to promote a completed evaluation."),
          )
        } else {
          empty(title: "No succeeded builds in current evaluation", hint: None)
        }
      } else {
        <section class="panel">
          <div class="panel-header">
            <h2>"Contents (" (builds.len()) " jobs)"</h2>
            <span class="panel-actions">
              if succeeded > 0 {
                <span class="badge badge-succeeded">(succeeded) " succeeded"</span>
              }
              if failed > 0 {
                <span class="badge badge-failed">(failed) " failed"</span>
              }
              if pending > 0 {
                <span class="badge badge-pending">(pending) " pending"</span>
              }
            </span>
          </div>
          <div class="table-wrap compact-table-wrap">
            <table>
              <thead>
                <tr>
                  <th>"Job"</th>
                  <th>"System"</th>
                  <th>"Status"</th>
                  <th>"Output"</th>
                </tr>
              </thead>
              <tbody>
                for build in builds {
                  <tr>
                    <td><a href=(format!("/build/{}", build.id))>(build.job_name)</a></td>
                    <td>(build.system)</td>
                    <td>
                      <span class=(format!("badge badge-{}", build.status_class))>(build.status_text)</span>
                    </td>
                    <td>
                      if build.output_path.is_empty() {
                        "-"
                      } else {
                        <code class="path-short">(build.output_path)</code>
                      }
                    </td>
                  </tr>
                }
              </tbody>
            </table>
          </div>
        </section>
      }
    )
  })
}

#[page("/news")]
async fn news_page(cx: &Cx) -> Result<impl View> {
  let viewer = viewer(cx, DashboardPage::News).await?;
  let state = app_context::<AppState>(cx);
  let items = circus_common::repo::news::list(&state.pool, 50, 0)
    .await
    .unwrap_or_default();
  let csrf_token = viewer.csrf_token.clone();
  let is_admin = viewer.is_admin;

  Ok(view! {
    document(title: "News", viewer: &viewer,
      <h1>"News"</h1>
      if is_admin {
        <section class="panel">
          <div class="panel-header">
            <h2>"Post news item"</h2>
          </div>
          <div class="panel-body">
            <form method="POST" action="/news" id="news-form">
              <input type="hidden" name="csrf_token" value=(csrf_token.clone())>
              <div class="form-group">
                <label for="title">"Title"</label>
                <input type="text" id="title" name="title" required=(true) maxlength="200">
              </div>
              <div class="form-group">
                <label for="content">"Content"</label>
                <textarea id="content" name="content" rows="4" required=(true)></textarea>
              </div>
              <button type="submit" class="btn">"Post"</button>
            </form>
          </div>
        </section>
      }
      if items.is_empty() {
        empty(title: "No news", hint: Some("News items will appear here."))
      } else {
        <div class="news-list">
          #[key(*item.id.as_bytes())]
          for item in items {
            <section class="panel news-item">
              <div class="panel-header">
                <h2 class="news-title">(item.title)</h2>
                <span class="panel-actions">
                  <span class="news-date">local_time(at: item.created_at)</span>
                  if is_admin {
                    <form method="POST" action=(format!("/news/{}/delete", item.id)) class="inline-form">
                      <input type="hidden" name="csrf_token" value=(csrf_token.clone())>
                      confirm_button(
                        prompt: "Delete this news item?",
                        class: "btn btn-small btn-danger",
                        label: "Delete",
                      )
                    </form>
                  }
                </span>
              </div>
              <div class="panel-body">
                <div class="news-content">(item.content)</div>
              </div>
            </section>
          }
        </div>
      }
    )
  })
}

/// The latest build of each starred job, from the newest visible evaluation
/// of its jobset.
async fn starred_jobs(
  state: &AppState,
  viewer: &DashboardContext,
  user: Uuid,
) -> Vec<StarredJobView> {
  let pool = &state.pool;
  let starred =
    circus_common::repo::starred_jobs::list_for_user(pool, user, 100, 0)
      .await
      .unwrap_or_default();
  let mut views = Vec::with_capacity(starred.len());

  for star in starred {
    let project_name =
      circus_common::repo::projects::get(pool, star.project_id)
        .await
        .map_or_else(|_| "-".to_owned(), |project| project.name);
    let mut jobset_name = "-".to_owned();
    let mut latest = None;

    if let Some(jobset_id) = star.jobset_id {
      if let Ok(jobset) =
        circus_common::repo::jobsets::get(pool, jobset_id).await
      {
        jobset_name = jobset.name;
      }

      let evals =
        circus_common::repo::evaluations::list_filtered_with_visibility(
          pool,
          Some(jobset_id),
          None,
          1,
          0,
          viewer.is_admin,
        )
        .await
        .unwrap_or_default();

      if let Some(eval) = evals.first() {
        latest = circus_common::repo::builds::list_filtered(
          pool,
          Some(eval.id),
          None,
          None,
          Some(&star.job_name),
          Some("build"),
          1,
          0,
        )
        .await
        .unwrap_or_default()
        .into_iter()
        .next();
      }
    }

    let (status_text, status_class) = latest.as_ref().map_or_else(
      || ("No builds".to_owned(), "pending".to_owned()),
      |build| status_badge(build.status),
    );

    views.push(StarredJobView {
      id: star.id,
      project_id: star.project_id,
      project_name,
      jobset_id: star.jobset_id,
      jobset_name,
      job_name: star.job_name,
      status_text,
      status_class,
      latest_build_id: latest.map(|build| build.id),
    });
  }

  views
}

#[page("/starred")]
async fn starred_page(cx: &Cx) -> Result<impl View> {
  let viewer = viewer(cx, DashboardPage::Starred).await?;
  let state = app_context::<AppState>(cx);
  let jobs = match viewer.viewer_user_id {
    Some(user) => Some(starred_jobs(state, &viewer, user).await),
    None => None,
  };
  let csrf_token = viewer.csrf_token.clone();

  Ok(view! {
    document(title: "Starred Jobs", viewer: &viewer,
      <h1>"Starred Jobs"</h1>
      match jobs {
        None => {
          <div class="empty">
            <div class="empty-title">"Login required"</div>
            <div class="empty-hint">
              "Please " <a href="/login?next=%2Fstarred">"login"</a> " to view your starred jobs."
            </div>
          </div>
        },
        Some(jobs) => {
          if jobs.is_empty() {
            empty(
              title: "No starred jobs",
              hint: Some("Star jobs from project or build pages to track them here."),
            )
          } else {
            <div class="table-wrap">
              <table>
                <thead>
                  <tr>
                    <th>"Project"</th>
                    <th>"Jobset"</th>
                    <th>"Job Name"</th>
                    <th>"Latest Status"</th>
                    <th class="row-actions">"Actions"</th>
                  </tr>
                </thead>
                <tbody>
                  #[key(*job.id.as_bytes())]
                  for job in jobs {
                    <tr>
                      <td><a href=(format!("/project/{}", job.project_id))>(job.project_name)</a></td>
                      <td>
                        match job.jobset_id {
                          Some(id) => <a href=(format!("/jobset/{id}"))>(job.jobset_name)</a>,
                          None => "-",
                        }
                      </td>
                      <td>(job.job_name)</td>
                      <td>
                        <span class=(format!("badge badge-{}", job.status_class))>(job.status_text)</span>
                      </td>
                      <td class="row-actions">
                        <form method="POST" action=(format!("/starred/{}/delete", job.id)) class="inline-form">
                          <input type="hidden" name="csrf_token" value=(csrf_token.clone())>
                          confirm_button(
                            prompt: "Remove this job from your starred list?",
                            class: "btn btn-small btn-secondary",
                            label: "Unstar",
                          )
                        </form>
                        if let Some(id) = job.latest_build_id {
                          <a href=(format!("/build/{id}")) class="btn btn-small btn-secondary">"View Build"</a>
                        }
                      </td>
                    </tr>
                  }
                </tbody>
              </table>
            </div>
          }
        },
      }
    )
  })
}

struct CacheRow {
  name:              String,
  scope_label:       String,
  active:            bool,
  nar_count:         i64,
  compressed:        String,
  requests_per_hour: i64,
}

#[page("/caches")]
async fn caches_page(cx: &Cx) -> Result<impl View> {
  let viewer = viewer(cx, DashboardPage::Caches).await?;
  let state = app_context::<AppState>(cx);
  let refs = crate::cache_overview::list_cache_refs(state)
    .await
    .map_err(api_error)?;

  // The unscoped (global) summary already covers every NAR. Summing the
  // per-cache rows would double count project-scoped entries.
  let totals =
    circus_common::repo::narinfo_cache::storage_summary(&state.pool, None)
      .await
      .map_err(db_error)?;

  let mut rows = Vec::with_capacity(refs.len());
  for cache in refs {
    let storage = circus_common::repo::narinfo_cache::storage_summary(
      &state.pool,
      cache.scope,
    )
    .await
    .map_err(db_error)?;
    let (requests_per_hour, _bytes) =
      circus_common::repo::cache_traffic::traffic_last_hour(
        &state.pool,
        &cache.name,
      )
      .await
      .map_err(db_error)?;

    rows.push(CacheRow {
      scope_label: cache.scope_label().to_owned(),
      name: cache.name,
      active: cache.active,
      nar_count: storage.nar_count,
      compressed: format_bytes(storage.compressed_bytes),
      requests_per_hour,
    });
  }

  Ok(view! {
    document(title: "Caches", viewer: &viewer,
      <h1>"Binary Caches"</h1>
      <div class="stat-strip">
        stat_card(label: "Total NARs", value: totals.nar_count.to_string())
        stat_card(label: "Compressed", value: format_bytes(totals.compressed_bytes))
        stat_card(label: "Uncompressed", value: format_bytes(totals.uncompressed_bytes))
      </div>
      if rows.is_empty() {
        empty(
          title: "No caches configured",
          hint: Some("Enable the global cache in config or enable a project's cache to see it here."),
        )
      } else {
        <div class="table-wrap">
          <table>
            <colgroup>
              <col style="width:30%">
              <col style="width:12%">
              <col style="width:12%">
              <col style="width:14%">
              <col style="width:16%">
              <col style="width:16%">
            </colgroup>
            <thead>
              <tr>
                <th>"Name"</th>
                <th>"Scope"</th>
                <th>"Status"</th>
                <th>"NARs"</th>
                <th>"Compressed"</th>
                <th>"Requests/hr"</th>
              </tr>
            </thead>
            <tbody>
              for row in rows {
                <tr>
                  <td><a href=(format!("/caches/{}", row.name))>(row.name)</a></td>
                  <td><span class="badge badge-neutral">(row.scope_label)</span></td>
                  <td>
                    if row.active {
                      <span class="badge badge-success">"Active"</span>
                    } else {
                      <span class="badge badge-pending">"Inactive"</span>
                    }
                  </td>
                  <td>(row.nar_count)</td>
                  <td>(row.compressed)</td>
                  <td>(row.requests_per_hour)</td>
                </tr>
              }
            </tbody>
          </table>
        </div>
      }
    )
  })
}

#[component]
async fn stat_card(label: &str, value: String) -> Result<impl View> {
  Ok(view! {
    <div class="stat-card">
      <div class="stat-label">(label)</div>
      <div class="stat-value">(value)</div>
    </div>
  })
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
        label:     nar_sort_label(column).to_owned(),
        href:      nars_href(detail_href, params, next_sort, None),
        active:    active_dir.is_some(),
        indicator: active_dir.map_or("", SortDirection::as_str).to_owned(),
        aria_sort: match active_dir {
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

#[page("/caches/{name}/nars")]
async fn cache_nars_page(cx: &Cx) -> Result<impl View> {
  let viewer = viewer(cx, DashboardPage::CacheNars).await?;
  let state = app_context::<AppState>(cx);
  let Query(params) = Query::<CacheNarsParams>::try_from_uri(request::uri(cx))
    .map_err(|_| bad_request("invalid NAR filter"))?;
  let name = path_param::<Name>(cx);
  let cache = crate::cache_overview::resolve_cache_ref(state, name)
    .await
    .map_err(api_error)?
    .ok_or_else(not_found)?;

  let limit = params.limit.unwrap_or(50).clamp(1, 200);
  let offset = params.offset.unwrap_or(0).max(0);
  let sort = params.sort.map(|column| {
    NarSort {
      column,
      direction: params.dir.unwrap_or(SortDirection::Asc),
    }
  });
  let pool = &state.pool;
  let hash = params.hash.clone();
  let package = params.package.clone();

  let items = circus_common::repo::narinfo_cache::list_filtered(
    pool,
    cache.scope,
    hash.as_deref(),
    package.as_deref(),
    sort,
    limit,
    offset,
  )
  .await
  .map_err(db_error)?;
  let total = circus_common::repo::narinfo_cache::count_filtered(
    pool,
    cache.scope,
    hash.as_deref(),
    package.as_deref(),
  )
  .await
  .map_err(db_error)?;
  let summary =
    circus_common::repo::narinfo_cache::storage_summary(pool, cache.scope)
      .await
      .map_err(db_error)?;
  let (last_uploaded, oldest_fetched) =
    circus_common::repo::narinfo_cache::storage_extremes(pool, cache.scope)
      .await
      .map_err(db_error)?;

  let pagination = Pagination::new(total, offset, limit);
  let detail_href = format!("/caches/{}", cache.name);
  let prev_href = nars_href(
    &detail_href,
    &params,
    sort,
    Some((pagination.prev_offset, limit)),
  );
  let next_href = nars_href(
    &detail_href,
    &params,
    sort,
    Some((pagination.next_offset, limit)),
  );
  let sort_headers = nar_sort_headers(&detail_href, &params, sort);
  let title = format!("{} NARs", cache.name);
  let scope_label = cache.scope_label().to_owned();

  Ok(view! {
    document(title: &title, viewer: &viewer,
      breadcrumbs(
        trail: vec![
          ("/caches".to_owned(), "Caches".to_owned()),
          (detail_href.clone(), cache.name.clone()),
        ],
        current: "NARs".to_owned(),
      )
      <div class="cache-header">
        <div class="cache-header-title">
          <h1>(cache.name.clone()) " · NARs"</h1>
          <span class="badge badge-neutral">(scope_label)</span>
        </div>
      </div>
      <div class="stat-strip">
        stat_card(label: "Total NARs", value: summary.nar_count.to_string())
        stat_card(label: "NAR size", value: format_bytes(summary.uncompressed_bytes))
        stat_card(label: "File size", value: format_bytes(summary.compressed_bytes))
        <div class="stat-card">
          <div class="stat-label">"Last uploaded"</div>
          <div class="stat-value">optional_time(at: last_uploaded, missing: "-")</div>
        </div>
        <div class="stat-card">
          <div class="stat-label">"Oldest fetched"</div>
          <div class="stat-value">optional_time(at: oldest_fetched, missing: "-")</div>
        </div>
      </div>
      <form method="get" action=(format!("{detail_href}/nars")) class="filter-form">
        <label>
          "Hash: "
          <input type="text" name="hash" value=(hash.unwrap_or_default()) placeholder="store hash prefix">
        </label>
        <label>
          "Package: "
          <input type="text" name="package" value=(package.unwrap_or_default()) placeholder="package name">
        </label>
        if let Some(active) = sort {
          <input type="hidden" name="sort" value=(active.column.as_str())>
          <input type="hidden" name="dir" value=(active.direction.as_str())>
        }
        <button type="submit" class="btn btn-secondary">"Filter"</button>
      </form>
      if items.is_empty() {
        empty(
          title: "No NARs match filters",
          hint: Some("Adjust the filters above, or wait for builds to upload paths."),
        )
      } else {
        <div class="table-wrap">
          <table>
            <colgroup>
              <col style="width:18%">
              <col style="width:30%">
              <col style="width:12%">
              <col style="width:12%">
              <col style="width:14%">
              <col style="width:14%">
            </colgroup>
            <thead>
              <tr>
                for header in sort_headers {
                  <th aria-sort=(header.aria_sort)>
                    <a
                      class=(if header.active { "sort-link is-active" } else { "sort-link" })
                      href=(header.href)
                    >
                      <span>(header.label)</span>
                      <span class=(format!("sort-indicator sort-{}", header.indicator)) aria-hidden="true"></span>
                    </a>
                  </th>
                }
              </tr>
            </thead>
            <tbody>
              #[key(item.store_path.clone())]
              for item in items {
                nar_row(
                  hash: store_path_hash(&item.store_path),
                  package: item.package_name,
                  store_path: item.store_path,
                  nar_size: item.nar_size,
                  file_size: item.file_size,
                  created_at: item.created_at,
                  last_fetched_at: item.last_fetched_at,
                )
              }
            </tbody>
          </table>
        </div>
        if pagination.total_pages > 1 {
          <nav class="pagination">
            if pagination.has_prev {
              <a href=(prev_href) class="btn btn-small btn-secondary">"« Previous"</a>
            }
            <span class="text-muted">"Page " (pagination.page) " of " (pagination.total_pages)</span>
            if pagination.has_next {
              <a href=(next_href) class="btn btn-small btn-secondary">"Next »"</a>
            }
          </nav>
        }
      }
    )
  })
}

#[component]
async fn nar_row(
  hash: String,
  package: String,
  store_path: String,
  nar_size: i64,
  file_size: Option<i64>,
  created_at: Timestamp,
  last_fetched_at: Option<Timestamp>,
) -> Result<impl View> {
  Ok(view! {
    <tr>
      <td><code class="mono-trunc">(hash.clone())</code></td>
      <td>
        <details class="nar-detail">
          <summary>(package.clone())</summary>
          <div class="store-path">
            <code title=(store_path.clone())>
              <span class="store-path-prefix">"/nix/store/"</span>
              <span class="store-path-hash">(hash)</span>
              <span class="store-path-name">"-" (package)</span>
            </code>
            <button
              type="button"
              class="store-path-copy"
              data-copy-text=(store_path)
              aria-label="Copy store path"
              title="Copy store path"
              onclick="navigator.clipboard.writeText(this.dataset.copyText).then(() => { this.classList.add('is-copied'); setTimeout(() => this.classList.remove('is-copied'), 1200) })"
            ></button>
          </div>
        </details>
      </td>
      <td title=(format_exact_bytes(nar_size))>(format_bytes(nar_size))</td>
      <td title=(file_size.map(format_exact_bytes).unwrap_or_default())>
        (file_size.map_or_else(|| "-".to_owned(), format_bytes))
      </td>
      <td>local_time(at: created_at)</td>
      <td>optional_time(at: last_fetched_at, missing: "Never")</td>
    </tr>
  })
}
