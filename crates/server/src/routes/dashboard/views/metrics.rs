//! Build metrics and per-cache detail, charted on the server.

use circus_common::{
  CiError,
  repo::{
    build_metrics,
    cache_traffic::{self, Granularity},
  },
};
use jiff::Timestamp;
use topcoat::{
  Result,
  context::{Cx, app_context},
  router::{
    error::{bad_request, internal_server_error, not_found},
    page,
    path_param,
    query_params,
  },
  view::{View, component, view},
};
use uuid::Uuid;

use super::super::{
  charts::{
    Axis,
    Buckets,
    Scale,
    Series,
    Slice,
    Tone,
    donut,
    lines,
    stacked_bars,
  },
  components::{confirm_button, copy_button},
  layout::{document, viewer},
  shared::{DashboardPage, format_bytes},
};
use crate::{cache_overview, state::AppState};

path_param!(name);

const RANGES: [(i32, &str); 3] = [
  (24, "Last 24 hours"),
  (48, "Last 48 hours"),
  (168, "Last 7 days"),
];

const GRANULARITIES: [(Granularity, &str, &str); 4] = [
  (Granularity::Minutes, "minutes", "Minutes"),
  (Granularity::Hours, "hours", "Hours"),
  (Granularity::Days, "days", "Days"),
  (Granularity::Weeks, "weeks", "Weeks"),
];

const CACHE_POINTS: i64 = 48;
const CACHE_CHART_WIDTH: f64 = 1100.0;

fn db_error(error: CiError) -> topcoat::Error {
  internal_server_error(error).into()
}

#[query_params(error = bad_request)]
struct MetricsQuery {
  hours:   Option<i32>,
  project: Option<String>,
}

#[component]
async fn chart_panel(
  title: &str,
  child: topcoat::view::Child<'_>,
) -> Result<impl View> {
  Ok(view! {
    <section class="panel metric-panel">
      <div class="panel-header">
        <h2>(title)</h2>
      </div>
      <div class="metric-chart-body">(child)</div>
    </section>
  })
}

#[component]
async fn no_data(text: &str) -> Result<impl View> {
  Ok(view! { <p class="empty-hint chart-empty">(text)</p> })
}

#[page("/metrics")]
async fn metrics_page(cx: &Cx) -> Result<impl View> {
  let viewer = viewer(cx, DashboardPage::Metrics).await?;
  let state = app_context::<AppState>(cx);
  let query = query_params::<MetricsQuery>(cx)?;
  let hours = query
    .hours
    .filter(|hours| RANGES.iter().any(|(range, _)| range == hours))
    .unwrap_or(24);
  let project_id = match query.project.as_deref() {
    None | Some("") => None,
    Some(raw) => {
      Some(
        raw
          .parse::<Uuid>()
          .map_err(|_| bad_request("invalid project id"))?,
      )
    },
  };

  let pool = &state.pool;
  let projects = circus_common::repo::projects::list(pool, 500, 0)
    .await
    .map_err(db_error)?;
  let builds = build_metrics::get_build_stats_timeseries(
    pool, project_id, None, hours, 60,
  )
  .await
  .map_err(db_error)?;
  let durations = build_metrics::get_duration_percentiles_timeseries(
    pool, project_id, None, hours, 60,
  )
  .await
  .map_err(db_error)?;
  let systems = build_metrics::get_system_distribution(pool, project_id, hours)
    .await
    .map_err(db_error)?;

  let tick_format = if hours <= 48 { "%H:%M" } else { "%b %-d %H:%M" };
  let buckets = |times: &mut dyn Iterator<Item = Timestamp>| {
    let times: Vec<Timestamp> = times.collect();
    Buckets {
      ticks:  times
        .iter()
        .map(|at| at.strftime(tick_format).to_string())
        .collect(),
      titles: times
        .iter()
        .map(|at| at.strftime("%b %-d %H:%M UTC").to_string())
        .collect(),
    }
  };
  let count_axis = Axis {
    title: None,
    scale: Scale::Count,
  };

  let succeeded: Vec<Option<f64>> = builds
    .iter()
    .map(|bucket| Some((bucket.total_builds - bucket.failed_builds) as f64))
    .collect();
  let failed = builds
    .iter()
    .map(|bucket| Some(bucket.failed_builds as f64))
    .collect();
  let success_rate = builds
    .iter()
    .map(|bucket| {
      (bucket.total_builds > 0).then(|| {
        (bucket.total_builds - bucket.failed_builds) as f64
          / bucket.total_builds as f64
          * 100.0
      })
    })
    .collect();
  let percentile = |label: &str, tone: Tone, values: Vec<Option<f64>>| {
    Series {
      label: label.to_owned(),
      tone,
      values,
    }
  };
  let percentiles = vec![
    percentile(
      "P50 (Median)",
      Tone::Accent,
      durations.iter().map(|bucket| bucket.p50).collect(),
    ),
    percentile(
      "P95",
      Tone::Running,
      durations.iter().map(|bucket| bucket.p95).collect(),
    ),
    percentile(
      "P99",
      Tone::Failed,
      durations.iter().map(|bucket| bucket.p99).collect(),
    ),
  ];

  Ok(view! {
    document(title: "Metrics", viewer: &viewer,
      <nav class="breadcrumbs">
        <a href="/">"Home"</a>
        <span class="sep">"/"</span>
        <span class="current">"Metrics"</span>
      </nav>
      <h1>"Build Metrics Dashboard"</h1>
      <p class="text-muted">"Chart times are UTC."</p>
      if viewer.is_admin {
        <p class="admin-notice">
          "Showing system-wide metrics. Filter by project below to view specific metrics."
        </p>
      }
      <form class="metrics-controls" method="get" action="/metrics">
        <label for="time-range">"Time Range:"</label>
        <select id="time-range" name="hours" onchange="this.form.requestSubmit()">
          for (range, label) in RANGES {
            <option value=(range.to_string()) selected=(range == hours)>(label)</option>
          }
        </select>
        <label for="project-filter">"Project:"</label>
        <select id="project-filter" name="project" onchange="this.form.requestSubmit()">
          <option value="" selected=(project_id.is_none())>"All Projects"</option>
          for project in projects {
            <option value=(project.id.to_string()) selected=(project_id == Some(project.id))>
              (project.name)
            </option>
          }
        </select>
        <button class="btn btn-small btn-secondary" type="submit">"Apply"</button>
      </form>
      <div class="metrics-grid">
        chart_panel(title: "Build Counts Over Time",
          if builds.is_empty() {
            no_data(text: "No data available")
          } else {
            stacked_bars(
              label: "Builds per hour",
              buckets: buckets(&mut builds.iter().map(|bucket| bucket.bucket_time)),
              series: vec![
                Series { label: "Successful".to_owned(), tone: Tone::Success, values: succeeded },
                Series { label: "Failed".to_owned(), tone: Tone::Failed, values: failed },
              ],
              axis: count_axis,
            )
          }
        )
        chart_panel(title: "Build Duration Percentiles",
          if durations.is_empty() {
            no_data(text: "No data available")
          } else {
            lines(
              label: "Build duration percentiles",
              buckets: buckets(&mut durations.iter().map(|bucket| bucket.bucket_time)),
              left: percentiles,
              left_axis: Axis { title: Some("Duration"), scale: Scale::Seconds },
            )
          }
        )
        chart_panel(title: "Builds by System",
          if systems.is_empty() {
            no_data(text: "No data available")
          } else {
            donut(
              label: "Builds by system",
              slices: systems
                .into_iter()
                .map(|(label, count)| Slice { label, value: count as f64 })
                .collect(),
            )
          }
        )
        chart_panel(title: "Success Rate Trend",
          if builds.is_empty() {
            no_data(text: "No data available")
          } else {
            lines(
              label: "Success rate",
              buckets: buckets(&mut builds.iter().map(|bucket| bucket.bucket_time)),
              left: vec![Series {
                label: "Success Rate (%)".to_owned(),
                tone: Tone::Success,
                values: success_rate,
              }],
              left_axis: Axis { title: None, scale: Scale::Percent },
            )
          }
        )
      </div>
    )
  })
}

#[query_params(error = bad_request)]
struct CacheQuery {
  storage:    Option<String>,
  traffic:    Option<String>,
  gc:         Option<String>,
  gc_deleted: Option<i64>,
  gc_freed:   Option<i64>,
  gc_failed:  Option<i64>,
}

/// The result banner after a `POST /caches/{name}/gc` redirect.
struct GcNotice {
  text:  String,
  error: bool,
}

impl CacheQuery {
  fn notice(&self) -> Option<GcNotice> {
    match self.gc.as_deref()? {
      "error" => {
        Some(GcNotice {
          text:  "Cache cleanup failed; see the server logs.".to_owned(),
          error: true,
        })
      },
      "done" => {
        let deleted = self.gc_deleted.unwrap_or(0);
        let freed = format_bytes(self.gc_freed.unwrap_or(0).max(0));
        let failed = self.gc_failed.unwrap_or(0);
        let removed = format!("Removed {deleted} cache entries ({freed}).");
        let text = if failed > 0 {
          format!(
            "{removed} {failed} backing objects could not be deleted; see the \
             server logs."
          )
        } else {
          removed
        };
        Some(GcNotice {
          text,
          error: failed > 0,
        })
      },
      _ => None,
    }
  }
}

fn granularity_param(raw: Option<&str>) -> &'static str {
  let chosen = Granularity::from_param(raw.unwrap_or_default());
  GRANULARITIES
    .iter()
    .find(|(granularity, ..)| *granularity == chosen)
    .map_or("hours", |(_, param, _)| param)
}

fn cache_buckets(
  times: impl Iterator<Item = Timestamp>,
  granularity: Granularity,
) -> Buckets {
  let tick_format = match granularity {
    Granularity::Days | Granularity::Weeks => "%b %-d",
    Granularity::Minutes | Granularity::Hours => "%H:%M",
  };
  let times: Vec<Timestamp> = times.collect();
  Buckets {
    ticks:  times
      .iter()
      .map(|at| at.strftime(tick_format).to_string())
      .collect(),
    titles: times
      .iter()
      .map(|at| at.strftime("%b %-d %H:%M UTC").to_string())
      .collect(),
  }
}

/// Minutes/Hours/Days/Weeks links. `href_prefix` carries the other chart's
/// choice and ends where this chart's parameter value goes.
#[component]
async fn granularity_toggle(
  name: &str,
  current: &str,
  href_prefix: &str,
) -> Result<impl View> {
  Ok(view! {
    <div class="granularity-toggle" aria-label=(format!("{name} granularity"))>
      for (_, param, label) in GRANULARITIES {
        <a href=(format!("{href_prefix}{param}")) class=(if param == current { "active" } else { "" })>(label)</a>
      }
    </div>
  })
}

#[component]
async fn stat(label: &str, value: String) -> Result<impl View> {
  Ok(view! {
    <div class="metric-card">
      <div class="stat-label">(label)</div>
      <div class="stat-value">(value)</div>
    </div>
  })
}

#[component]
async fn copy_field(id: &str, label: &str, value: &str) -> Result<impl View> {
  Ok(view! {
    <label class="copy-label" for=(id)>(label)</label>
    <div class="copy-field">
      <input id=(id) type="text" readonly=(true) value=(value)>
      copy_button(text: value)
    </div>
  })
}

#[page("/caches/{name}")]
async fn cache_detail_page(cx: &Cx) -> Result<impl View> {
  let viewer = viewer(cx, DashboardPage::CacheDetail).await?;
  let state = app_context::<AppState>(cx);
  let name = path_param::<Name>(cx);
  let query = query_params::<CacheQuery>(cx)?;
  let Some(cache) = cache_overview::resolve_cache_ref(state, name)
    .await
    .map_err(|error| db_error(error.0))?
  else {
    return Err(not_found().into());
  };

  let pool = &state.pool;
  let storage =
    circus_common::repo::narinfo_cache::storage_summary(pool, cache.scope)
      .await
      .map_err(db_error)?;
  let (requests_last_hour, bytes_served) =
    cache_traffic::traffic_last_hour(pool, &cache.name)
      .await
      .map_err(db_error)?;

  let storage_param = granularity_param(query.storage.as_deref());
  let traffic_param = granularity_param(query.traffic.as_deref());
  let storage_granularity = Granularity::from_param(storage_param);
  let traffic_granularity = Granularity::from_param(traffic_param);

  // The storage and traffic series stay admin-only even where the page is
  // public.
  let (storage_series, traffic_series) = if viewer.is_admin {
    let storage_series = cache_traffic::storage_timeseries(
      pool,
      cache.scope,
      storage_granularity,
      CACHE_POINTS,
    )
    .await
    .map_err(db_error)?;
    let traffic_series = cache_traffic::traffic_timeseries(
      pool,
      &cache.name,
      traffic_granularity,
      CACHE_POINTS,
    )
    .await
    .map_err(db_error)?;
    (Some(storage_series), Some(traffic_series))
  } else {
    (None, None)
  };

  let substituter = cache_overview::substituter_url(&state.config, &cache);
  let public_key = cache_overview::public_key(&state.config);
  let snippet = cache_overview::nix_conf_snippet(
    substituter.as_deref(),
    public_key.as_deref(),
  );
  let notice = query.notice();
  let title = format!("{} cache", cache.name);
  let base = format!("/caches/{}", cache.name);
  let storage_href = format!("{base}?traffic={traffic_param}&storage=");
  let traffic_href = format!("{base}?storage={storage_param}&traffic=");
  let bytes_axis = |title| {
    Axis {
      title: Some(title),
      scale: Scale::Bytes,
    }
  };
  let count_axis = |title| {
    Axis {
      title: Some(title),
      scale: Scale::Count,
    }
  };
  let gc_prompt = if cache.is_global() {
    "Delete the matching NARs from the global cache and every project cache? \
     This removes their shared entries and objects and cannot be undone."
  } else {
    "Delete the matching NARs from this cache? This cannot be undone."
  };

  Ok(view! {
    document(title: &title, viewer: &viewer,
      <nav class="breadcrumbs">
        <a href="/">"Home"</a>
        <span class="sep">"/"</span>
        <a href="/caches">"Caches"</a>
        <span class="sep">"/"</span>
        <span class="current">(cache.name.as_str())</span>
      </nav>
      <div class="cache-header">
        <div class="cache-header-title">
          <h1>(cache.name.as_str())</h1>
          <span class="badge badge-neutral">(cache.scope_label())</span>
          if cache.active {
            <span class="badge badge-success">"Active"</span>
          } else {
            <span class="badge badge-pending">"Inactive"</span>
          }
        </div>
        <a class="btn btn-secondary" href=(format!("{base}/nars"))>"Browse NARs »"</a>
      </div>
      <section class="panel cache-section">
        <div class="panel-header">
          <h2>"Storage" <span class="text-muted">"(UTC)"</span></h2>
          if viewer.is_admin {
            <div class="panel-actions">
              granularity_toggle(name: "Storage", current: storage_param, href_prefix: &storage_href)
            </div>
          }
        </div>
        <div class="panel-body">
          <div class="cache-stat-row">
            stat(label: "Packages Stored", value: storage.nar_count.to_string())
            stat(label: "Uncompressed", value: format_bytes(storage.uncompressed_bytes))
            stat(label: "Compressed", value: format_bytes(storage.compressed_bytes))
          </div>
          if let Some(series) = storage_series {
            <div class="cache-chart-body">
              if series.is_empty() {
                no_data(text: "No storage data yet")
              } else {
                lines(
                  label: "Storage added",
                  buckets: cache_buckets(series.iter().map(|point| point.bucket_time), storage_granularity),
                  left: vec![Series {
                    label: "Bytes added".to_owned(),
                    tone: Tone::Accent,
                    values: series.iter().map(|point| Some(point.bytes_added as f64)).collect(),
                  }],
                  left_axis: bytes_axis("Bytes"),
                  right: vec![Series {
                    label: "Packages added".to_owned(),
                    tone: Tone::Running,
                    values: series.iter().map(|point| Some(point.packages_added as f64)).collect(),
                  }],
                  right_axis: Some(count_axis("Packages")),
                  width: CACHE_CHART_WIDTH,
                )
              }
            </div>
          }
        </div>
      </section>
      <section class="panel cache-section">
        <div class="panel-header">
          <h2>"Traffic" <span class="text-muted">"(UTC)"</span></h2>
          if viewer.is_admin {
            <div class="panel-actions">
              granularity_toggle(name: "Traffic", current: traffic_param, href_prefix: &traffic_href)
            </div>
          }
        </div>
        <div class="panel-body">
          <div class="cache-stat-row">
            stat(label: "Requests (last hour)", value: requests_last_hour.to_string())
            stat(label: "Traffic (last hour)", value: format_bytes(bytes_served))
          </div>
          if let Some(series) = traffic_series {
            <div class="cache-chart-body">
              if series.is_empty() {
                no_data(text: "No traffic data yet")
              } else {
                lines(
                  label: "Traffic served",
                  buckets: cache_buckets(series.iter().map(|point| point.bucket_time), traffic_granularity),
                  left: vec![Series {
                    label: "Bytes served".to_owned(),
                    tone: Tone::Accent,
                    values: series.iter().map(|point| Some(point.bytes as f64)).collect(),
                  }],
                  left_axis: bytes_axis("Bytes"),
                  right: vec![Series {
                    label: "Requests".to_owned(),
                    tone: Tone::Running,
                    values: series.iter().map(|point| Some(point.requests as f64)).collect(),
                  }],
                  right_axis: Some(count_axis("Requests")),
                  width: CACHE_CHART_WIDTH,
                )
              }
            </div>
          }
        </div>
      </section>
      <section class="panel cache-section">
        <div class="panel-header">
          <h2>"How to use this cache"</h2>
        </div>
        <div class="panel-body">
          if let Some(url) = substituter.as_deref() {
            copy_field(id: "cache-substituter-url", label: "Substituter URL", value: url)
          } else {
            <p class="empty-hint">
              "No " <code>"cache.cache_url"</code>
              " is configured, so a substituter URL cannot be shown."
            </p>
          }
          if let Some(key) = public_key.as_deref() {
            copy_field(id: "cache-public-key", label: "Trusted public key", value: key)
          } else {
            <p class="empty-hint">"Signing is disabled, so there is no public key to publish."</p>
          }
          if let Some(snippet) = snippet.as_deref() {
            <label class="copy-label" for="cache-nix-conf-snippet">"nix.conf snippet"</label>
            <div class="copy-field copy-field-block">
              <textarea id="cache-nix-conf-snippet" readonly=(true) rows="2">(snippet)</textarea>
              copy_button(text: snippet)
            </div>
          }
        </div>
      </section>
      if viewer.is_admin {
        <section class="panel cache-section">
          <div class="panel-header">
            <h2>"Maintenance"</h2>
          </div>
          <div class="panel-body">
            if let Some(notice) = notice {
              <div class=(if notice.error { "flash-message flash-error" } else { "flash-message flash-success" })>
                (notice.text)
              </div>
            }
            <form method="POST" action=(format!("{base}/gc")) class="filter-form">
              <input type="hidden" name="csrf_token" value=(viewer.csrf_token.as_str())>
              <label>
                "Delete: "
                <select name="mode">
                  <option value="stale">"NARs not fetched recently"</option>
                  <option value="all">"All NARs"</option>
                </select>
              </label>
              <label>
                "Not fetched in (days): "
                <input type="number" name="days" value="30" min="1" max="3650">
              </label>
              confirm_button(prompt: gc_prompt, class: "btn btn-danger", label: "Clean up")
            </form>
            <p class="empty-hint">
              "Removes cache entries and their uploaded NAR objects. Store paths served from the \
               local Nix store stay on disk until garbage collection runs (Admin › Run GC now)."
            </p>
          </div>
        </section>
      }
    )
  })
}
