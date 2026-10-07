//! Small views shared by every Topcoat page.

use jiff::Timestamp;
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
/// run, so the local time goes through an `<output>`'s bound `value`.
#[component]
pub async fn local_time(cx: &Cx, at: Timestamp) -> Result<impl View> {
  let browser = signal(cx, || true);
  let millis = at.as_millisecond();
  let utc = at.strftime("%b %-d, %H:%M UTC").to_string();
  let local = expr!({
    let _bound = browser.get();
    raw!(
      "new Date(Number(${millis})).toLocaleString(undefined, {month: 'short', \
       day: 'numeric', hour: 'numeric', minute: '2-digit'})",
      utc.clone()
    )
  });

  Ok(view! {
    <time datetime=(at.to_string())><output :value=(local)>(utc)</output></time>
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
