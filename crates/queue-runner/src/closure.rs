use std::path::Path;

use circus_common::{PgPool, repo};
use uuid::Uuid;

use crate::worker::{ClosurePathInfo, get_recursive_path_infos_with_nix};

pub async fn record(pool: &PgPool, build_id: Uuid, output_paths: &[String]) {
  let Some(closure) = path_infos(output_paths).await else {
    return;
  };

  let size = closure.iter().map(|info| info.nar_size).sum();
  if let Err(error) = repo::builds::set_closure_size(pool, build_id, size).await
  {
    tracing::warn!(%build_id, "Failed to record closure size: {error}");
  }
}

async fn path_infos(output_paths: &[String]) -> Option<Vec<ClosurePathInfo>> {
  if output_paths.is_empty() {
    return None;
  }
  get_recursive_path_infos_with_nix(Path::new("nix"), output_paths).await
}
