//! Small views shared by every Topcoat page.

use jiff::{Timestamp, tz::TimeZone};
use serde::de::{
  DeserializeOwned,
  IntoDeserializer,
  value::Error as ValueError,
};
use topcoat::{
  Result,
  context::Cx,
  runtime::{expr, signal},
  view::{View, component, view},
};

/// A result code from a redirect's query, ignoring codes a stale link carries.
pub fn result_code<T: DeserializeOwned>(code: Option<&str>) -> Option<T> {
  code.and_then(|code| {
    T::deserialize(IntoDeserializer::<ValueError>::into_deserializer(code)).ok()
  })
}

/// UTC until the runtime loads. Text expressions skip their first browser
/// run, so the local time goes through an `<output>`'s bound `value`. `zone`
/// names the viewer's timezone where the exact moment matters.
#[component]
pub async fn local_time(
  cx: &Cx,
  at: Timestamp,
  zone: bool,
) -> Result<impl View> {
  let browser = signal(cx, || true);
  let millis = at.as_millisecond();
  let this_year = at.to_zoned(TimeZone::UTC).year()
    == Timestamp::now().to_zoned(TimeZone::UTC).year();
  let utc = if this_year {
    at.strftime("%b %-d, %H:%M UTC")
  } else {
    at.strftime("%b %-d, %Y, %H:%M UTC")
  }
  .to_string();
  let full_utc = at.strftime("%A, %B %-d, %Y at %H:%M:%S UTC").to_string();
  let local = expr!({
    let _bound = browser.get();
    raw!(
      "(() => { const at = new Date(Number(${millis})); const opts = {month: \
       'short', day: 'numeric', hour: 'numeric', minute: '2-digit'}; if \
       (at.getFullYear() !== new Date().getFullYear()) opts.year = 'numeric'; \
       if (${zone}) opts.timeZoneName = 'short'; return \
       at.toLocaleString(undefined, opts); })()",
      utc.clone()
    )
  });
  let full = expr!({
    let _bound = browser.get();
    raw!(
      "new Date(Number(${millis})).toLocaleString(undefined, {dateStyle: \
       'full', timeStyle: 'long'})",
      full_utc.clone()
    )
  });

  Ok(view! {
    <time datetime=(at.to_string()) :title=(full)><output :value=(local)>(utc)</output></time>
  })
}

#[component]
pub async fn confirm_button(
  prompt: &str,
  class: &str,
  label: &str,
) -> Result<impl View> {
  Ok(view! {
    <button
      type="submit"
      class=(class)
      data-confirm=(prompt)
      onclick="return confirm(this.dataset.confirm)"
    >
      (label)
    </button>
  })
}

#[component]
pub async fn copy_button(text: &str) -> Result<impl View> {
  Ok(view! {
    <button
      type="button"
      class="btn btn-small btn-ghost"
      data-copy=(text)
      onclick="navigator.clipboard.writeText(this.dataset.copy)"
    >
      "Copy"
    </button>
  })
}
