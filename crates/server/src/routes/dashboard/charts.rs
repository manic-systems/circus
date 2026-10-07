//! Server-rendered SVG charts for the metrics and cache pages.

use std::f64::consts::{PI, TAU};

use topcoat::{
  Result,
  view::{View, component, svg::ViewBox, view},
};

use super::shared::format_bytes;

const WIDTH: f64 = 640.0;
const HEIGHT: f64 = 260.0;
const PAD_SIDE: f64 = 80.0;
const PAD_END: f64 = 16.0;
const PAD_TOP: f64 = 12.0;
const PAD_BOTTOM: f64 = 28.0;
const Y_TICKS: u32 = 4;
const MAX_X_LABELS: usize = 7;
/// Past this many buckets, line points only show on hover.
const DENSE_POINTS: usize = 48;

/// A theme colour a series is drawn in.
#[derive(Clone, Copy)]
pub enum Tone {
  Success,
  Failed,
  Running,
  Accent,
  Muted,
  Subtle,
}

impl Tone {
  /// Slice colours for donut charts, in the order slices are drawn.
  const CYCLE: [Self; 6] = [
    Self::Success,
    Self::Running,
    Self::Accent,
    Self::Muted,
    Self::Failed,
    Self::Subtle,
  ];

  const fn class(self) -> &'static str {
    match self {
      Self::Success => "tone-success",
      Self::Failed => "tone-failed",
      Self::Running => "tone-running",
      Self::Accent => "tone-accent",
      Self::Muted => "tone-muted",
      Self::Subtle => "tone-subtle",
    }
  }
}

/// One named line or bar stack. `None` leaves a gap in a line.
pub struct Series {
  pub label:  String,
  pub tone:   Tone,
  pub values: Vec<Option<f64>>,
}

/// What a value axis measures, which decides its tick spacing and labels.
#[derive(Clone, Copy)]
pub enum Scale {
  Count,
  Percent,
  Seconds,
  Bytes,
}

impl Scale {
  /// The axis maximum for a series peaking at `peak`, chosen so every tick
  /// lands on a readable value.
  fn top(self, peak: f64) -> f64 {
    match self {
      Self::Percent => 100.0,
      Self::Count => {
        nice_ceil(peak / f64::from(Y_TICKS)).ceil() * f64::from(Y_TICKS)
      },
      Self::Seconds => nice_ceil(peak),
      Self::Bytes => {
        let unit = 1024f64.powf((peak.max(1.0).log2() / 10.0).floor());
        nice_ceil(peak / unit) * unit
      },
    }
  }

  fn format(self, value: f64) -> String {
    match self {
      Self::Count => format!("{value:.0}"),
      Self::Percent => format!("{value:.0}%"),
      Self::Bytes => format_bytes(value as i64),
      Self::Seconds if value < 60.0 => format!("{value:.0}s"),
      Self::Seconds if value < 3600.0 => {
        let minutes = (value / 60.0).floor();
        let seconds = value - minutes * 60.0;
        if seconds >= 1.0 {
          format!("{minutes:.0}m {seconds:.0}s")
        } else {
          format!("{minutes:.0}m")
        }
      },
      Self::Seconds => format!("{:.1}h", value / 3600.0),
    }
  }
}

/// A value axis starting at zero.
#[derive(Clone, Copy)]
pub struct Axis {
  pub title: Option<&'static str>,
  pub scale: Scale,
}

/// One wedge of a donut chart.
pub struct Slice {
  pub label: String,
  pub value: f64,
}

/// The x-axis buckets shared by every series in a chart.
pub struct Buckets {
  /// Short tick labels.
  pub ticks:  Vec<String>,
  /// Full labels used in tooltips.
  pub titles: Vec<String>,
}

struct LegendEntry {
  label: String,
  tone:  Tone,
}

struct Bar {
  x:      f64,
  y:      f64,
  height: f64,
  tone:   Tone,
  title:  String,
}

struct Point {
  x:     f64,
  y:     f64,
  title: String,
}

struct Line {
  tone:   Tone,
  path:   String,
  area:   Option<String>,
  points: Vec<Point>,
}

struct Wedge {
  tone:  Tone,
  paths: Vec<String>,
  title: String,
  label: String,
  share: f64,
}

struct Plot {
  frame: f64,
  right: f64,
  count: usize,
}

impl Plot {
  fn new(frame: f64, count: usize, dual: bool) -> Self {
    let right = frame - if dual { PAD_SIDE } else { PAD_END };
    Self {
      frame,
      right,
      count,
    }
  }

  const fn view_box(&self) -> ViewBox {
    ViewBox::new(0.0, 0.0, self.frame as f32, HEIGHT as f32)
  }

  fn width(&self) -> f64 {
    self.right - PAD_SIDE
  }

  fn y(value: f64, top: f64) -> f64 {
    let bottom = HEIGHT - PAD_BOTTOM;
    let ratio = if top > 0.0 { value / top } else { 0.0 };
    round(ratio.clamp(0.0, 1.0).mul_add(PAD_TOP - bottom, bottom))
  }

  fn point_x(&self, index: usize) -> f64 {
    if self.count <= 1 {
      return round(PAD_SIDE + self.width() / 2.0);
    }
    round(PAD_SIDE + self.width() * index as f64 / (self.count - 1) as f64)
  }

  fn band(&self) -> f64 {
    self.width() / self.count.max(1) as f64
  }

  fn band_x(&self, index: usize) -> f64 {
    round(self.band().mul_add(index as f64 + 0.5, PAD_SIDE))
  }
}

fn round(value: f64) -> f64 {
  (value * 10.0).round() / 10.0
}

/// The smallest 1, 2, 2.5 or 5 times a power of ten at or above `value`.
fn nice_ceil(value: f64) -> f64 {
  if value <= 0.0 {
    return 1.0;
  }
  let magnitude = 10f64.powf(value.log10().floor());
  [1.0, 2.0, 2.5, 5.0, 10.0]
    .into_iter()
    .map(|step| step * magnitude)
    .find(|candidate| *candidate >= value)
    .unwrap_or(10.0 * magnitude)
}

fn peak<'series>(values: impl Iterator<Item = &'series Option<f64>>) -> f64 {
  values.flatten().copied().fold(0.0, f64::max)
}

fn line_path(plot: &Plot, values: &[Option<f64>], top: f64) -> String {
  let mut pen_down = false;

  values
    .iter()
    .enumerate()
    .filter_map(|(index, value)| {
      let Some(value) = value else {
        pen_down = false;
        return None;
      };
      let verb = if pen_down { 'L' } else { 'M' };
      pen_down = true;
      Some(format!(
        "{verb}{} {} ",
        plot.point_x(index),
        Plot::y(*value, top)
      ))
    })
    .collect()
}

fn area_path(plot: &Plot, values: &[Option<f64>], top: f64) -> Option<String> {
  if values.iter().any(Option::is_none) {
    return None;
  }
  let last = values.len().checked_sub(1)?;
  let bottom = HEIGHT - PAD_BOTTOM;
  Some(format!(
    "{}L{} {bottom} L{} {bottom} Z",
    line_path(plot, values, top),
    plot.point_x(last),
    plot.point_x(0),
  ))
}

fn legend_entries<'series>(
  series: impl Iterator<Item = &'series Series>,
) -> Vec<LegendEntry> {
  series
    .map(|series| {
      LegendEntry {
        label: series.label.clone(),
        tone:  series.tone,
      }
    })
    .collect()
}

#[component]
async fn legend(entries: Vec<LegendEntry>) -> Result<impl View> {
  Ok(view! {
    <ul class="chart-legend">
      for entry in entries {
        <li>
          <span class=(format!("chart-swatch {}", entry.tone.class()))></span>
          (entry.label)
        </li>
      }
    </ul>
  })
}

/// Tick labels for one axis. The left axis also draws the gridlines.
#[component]
async fn value_axis(
  plot: &Plot,
  axis: Axis,
  top: f64,
  left: bool,
) -> Result<impl View> {
  let (x, anchor, title_x) = if left {
    (PAD_SIDE - 8.0, "end", 10.0)
  } else {
    (plot.right + 8.0, "start", plot.frame - 10.0)
  };
  let ticks: Vec<(f64, String)> = (0..=Y_TICKS)
    .map(|tick| {
      let value = top * f64::from(tick) / f64::from(Y_TICKS);
      (Plot::y(value, top), axis.scale.format(value))
    })
    .collect();

  Ok(view! {
    for (y, text) in ticks {
      if left {
        <line class="chart-grid" x1=(PAD_SIDE) x2=(plot.frame - PAD_END) y1=(y) y2=(y)></line>
      }
      <text class="chart-tick" x=(x) y=(y + 4.0) text-anchor=(anchor)>(text)</text>
    }
    if let Some(title) = axis.title {
      <text
        class="chart-axis-title"
        transform=(format!("translate({title_x} {}) rotate(-90)", HEIGHT / 2.0))
        text-anchor="middle"
      >
        (title)
      </text>
    }
  })
}

#[component]
async fn x_labels(
  plot: &Plot,
  buckets: &Buckets,
  bars: bool,
) -> Result<impl View> {
  let step = buckets.ticks.len().div_ceil(MAX_X_LABELS).max(1);

  Ok(view! {
    for (index, tick) in buckets.ticks.iter().enumerate().step_by(step) {
      <text
        class="chart-tick"
        x=(if bars { plot.band_x(index) } else { plot.point_x(index) })
        y=(HEIGHT - 8.0)
        text-anchor="middle"
      >
        (tick.as_str())
      </text>
    }
  })
}

/// Bars stacked bottom to top in `series` order.
#[component]
pub async fn stacked_bars(
  label: &str,
  buckets: Buckets,
  series: Vec<Series>,
  axis: Axis,
) -> Result<impl View> {
  let plot = Plot::new(WIDTH, buckets.ticks.len(), false);
  let value = |series: &Series, index: usize| {
    series.values.get(index).copied().flatten().unwrap_or(0.0)
  };
  let tallest = (0..buckets.ticks.len())
    .map(|index| series.iter().map(|series| value(series, index)).sum())
    .fold(0.0, f64::max);
  let top = axis.scale.top(tallest);
  let width = round((plot.band() * 0.7).max(1.0));
  let mut bars = Vec::new();

  for (index, title) in buckets.titles.iter().enumerate() {
    let mut base = 0.0;
    for series in &series {
      let amount = value(series, index);
      if amount <= 0.0 {
        continue;
      }
      let y = Plot::y(base + amount, top);
      bars.push(Bar {
        x: round(plot.band_x(index) - width / 2.0),
        y,
        height: round(Plot::y(base, top) - y),
        tone: series.tone,
        title: format!(
          "{title}\n{}: {}",
          series.label,
          axis.scale.format(amount)
        ),
      });
      base += amount;
    }
  }

  Ok(view! {
    <figure class="chart">
      legend(entries: legend_entries(series.iter()))
      <svg
        class="chart-svg"
        viewBox=(plot.view_box())
        role="img"
        aria-label=(label)
      >
        value_axis(plot: &plot, axis: axis, top: top, left: true)
        for bar in bars {
          <rect
            class=(format!("chart-bar {}", bar.tone.class()))
            x=(bar.x)
            y=(bar.y)
            width=(width)
            height=(bar.height)
            rx="2"
          >
            <title>(bar.title)</title>
          </rect>
        }
        x_labels(plot: &plot, buckets: &buckets, bars: true)
      </svg>
    </figure>
  })
}

/// Lines on a left axis, plus optional lines on a second right axis. Only the
/// left series are filled, so a right series never hides behind an area.
/// `width` sets the aspect ratio, since the chart is drawn at 100% width.
#[component]
pub async fn lines(
  label: &str,
  buckets: Buckets,
  left: Vec<Series>,
  left_axis: Axis,
  #[default] right: Vec<Series>,
  #[default] right_axis: Option<Axis>,
  #[default(WIDTH)] width: f64,
) -> Result<impl View> {
  let plot = Plot::new(width, buckets.ticks.len(), right_axis.is_some());
  let left_top = left_axis
    .scale
    .top(peak(left.iter().flat_map(|series| &series.values)));
  let right_scale = right_axis.map(|axis| {
    (
      axis,
      axis
        .scale
        .top(peak(right.iter().flat_map(|series| &series.values))),
    )
  });
  let sides = left
    .iter()
    .map(|series| (series, left_axis.scale, left_top, true))
    .chain(right_scale.into_iter().flat_map(|(axis, top)| {
      right
        .iter()
        .map(move |series| (series, axis.scale, top, false))
    }));
  let point_class = if buckets.ticks.len() > DENSE_POINTS {
    "chart-point-quiet"
  } else {
    ""
  };
  let mut drawn = Vec::new();

  for (series, scale, top, filled) in sides {
    let points = series
      .values
      .iter()
      .zip(&buckets.titles)
      .enumerate()
      .filter_map(|(index, (value, title))| {
        let value = (*value)?;
        Some(Point {
          x:     plot.point_x(index),
          y:     Plot::y(value, top),
          title: format!("{title}\n{}: {}", series.label, scale.format(value)),
        })
      })
      .collect();
    drawn.push(Line {
      tone: series.tone,
      path: line_path(&plot, &series.values, top),
      area: filled
        .then(|| area_path(&plot, &series.values, top))
        .flatten(),
      points,
    });
  }

  Ok(view! {
    <figure class="chart">
      legend(entries: legend_entries(left.iter().chain(&right)))
      <svg
        class="chart-svg"
        viewBox=(plot.view_box())
        role="img"
        aria-label=(label)
      >
        value_axis(plot: &plot, axis: left_axis, top: left_top, left: true)
        if let Some((axis, top)) = right_scale {
          value_axis(plot: &plot, axis: axis, top: top, left: false)
        }
        for line in drawn {
          if let Some(area) = line.area {
            <path class=(format!("chart-area {}", line.tone.class())) d=(area)></path>
          }
          <path class=(format!("chart-line {}", line.tone.class())) d=(line.path)></path>
          for point in line.points {
            <circle
              class=(format!("chart-point {point_class} {}", line.tone.class()))
              cx=(point.x)
              cy=(point.y)
              r="3"
            >
              <title>(point.title)</title>
            </circle>
          }
        }
        x_labels(plot: &plot, buckets: &buckets, bars: false)
      </svg>
    </figure>
  })
}

fn arc_point(radius: f64, angle: f64) -> (f64, f64) {
  (round(radius * angle.sin()), round(-radius * angle.cos()))
}

fn wedge_path(start: f64, end: f64, outer: f64, inner: f64) -> String {
  let large = u8::from(end - start > PI);
  let (outer_x1, outer_y1) = arc_point(outer, start);
  let (outer_x2, outer_y2) = arc_point(outer, end);
  let (inner_x2, inner_y2) = arc_point(inner, end);
  let (inner_x1, inner_y1) = arc_point(inner, start);
  format!(
    "M{outer_x1} {outer_y1} A{outer} {outer} 0 {large} 1 {outer_x2} \
     {outer_y2} L{inner_x2} {inner_y2} A{inner} {inner} 0 {large} 0 \
     {inner_x1} {inner_y1} Z"
  )
}

/// A ring of slices with a legend listing each share.
#[component]
pub async fn donut(label: &str, slices: Vec<Slice>) -> Result<impl View> {
  const OUTER: f64 = 90.0;
  const INNER: f64 = 56.0;

  let total: f64 = slices.iter().map(|slice| slice.value).sum();
  let mut start = 0.0;
  let mut wedges = Vec::new();

  for (index, slice) in slices.into_iter().enumerate() {
    let share = if total > 0.0 {
      slice.value / total
    } else {
      0.0
    };
    let end = start + share * TAU;
    // An arc whose end meets its start draws nothing, so a full ring is split.
    let paths = if share > 0.999 {
      vec![
        wedge_path(0.0, PI, OUTER, INNER),
        wedge_path(PI, TAU - 1e-4, OUTER, INNER),
      ]
    } else {
      vec![wedge_path(start, end, OUTER, INNER)]
    };
    wedges.push(Wedge {
      tone: Tone::CYCLE[index % Tone::CYCLE.len()],
      paths,
      title: format!(
        "{}: {} ({:.1}%)",
        slice.label,
        slice.value,
        share * 100.0
      ),
      label: slice.label,
      share,
    });
    start = end;
  }

  Ok(view! {
    <figure class="chart chart-donut">
      <svg
        class="chart-donut-svg"
        viewBox=(ViewBox::new(-100.0, -100.0, 200.0, 200.0))
        role="img"
        aria-label=(label)
      >
        for wedge in &wedges {
          for path in &wedge.paths {
            <path class=(format!("chart-wedge {}", wedge.tone.class())) d=(path.as_str())>
              <title>(wedge.title.as_str())</title>
            </path>
          }
        }
      </svg>
      <ul class="chart-legend chart-legend-list">
        for wedge in &wedges {
          <li>
            <span class=(format!("chart-swatch {}", wedge.tone.class()))></span>
            <span class="chart-legend-label">(wedge.label.as_str())</span>
            <span class="chart-legend-share">(format!("{:.0}%", wedge.share * 100.0))</span>
          </li>
        }
      </ul>
    </figure>
  })
}
