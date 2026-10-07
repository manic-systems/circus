use axum::response::Response;

use super::{super::shared::PrivateTemplate, fixtures::ui, render};

pub(super) async fn private() -> Response {
  render(PrivateTemplate {
    ui:        ui(),
    is_admin:  false,
    auth_name: String::new(),
  })
}
