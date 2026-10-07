//! Askama template structs for every dashboard page. The structs are
//! field-private from outside this module, but `pub(super)` for sibling
//! handler modules. Each `#[derive(Template)]` macro looks for the
//! `path = "..."` HTML template under the configured templates root.
#![expect(
  dead_code,
  reason = "Askama templates read fields from generated render impls"
)]

pub(super) use super::shared::UiTemplateConfig;

pub(super) struct SortHeaderView {
  pub(super) key:         String,
  pub(super) label:       String,
  pub(super) href:        String,
  pub(super) default_dir: String,
  pub(super) active:      bool,
  pub(super) indicator:   String,
  pub(super) aria_sort:   String,
}
