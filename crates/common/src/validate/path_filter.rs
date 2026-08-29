pub(super) fn validate_path_filters(filters: &[String]) -> Result<(), String> {
  if filters.len() > 128 {
    return Err("path_filters must contain at most 128 entries".to_string());
  }
  for filter in filters {
    if filter.is_empty() || filter.len() > 512 {
      return Err(
        "path_filters entries must contain 1 to 512 characters".to_string(),
      );
    }
    if filter.starts_with('/')
      || filter.contains(['\0', '\\'])
      || filter.split('/').any(|component| component == "..")
    {
      return Err(
        "path_filters entries must be relative Git pathspecs without '..' or \
         backslashes"
          .to_string(),
      );
    }
  }
  Ok(())
}

pub fn validate_path_filter_policy(
  source_change: bool,
  filters: &[String],
) -> Result<(), String> {
  if !source_change && !filters.is_empty() {
    return Err(
      "path_filters requires trigger_mode 'source_change'".to_string(),
    );
  }
  Ok(())
}

#[cfg(test)]
mod tests {
  use super::{validate_path_filter_policy, validate_path_filters};

  #[test]
  fn rejects_parent_traversal() {
    assert!(validate_path_filters(&["../outside".to_string()]).is_err());
  }

  #[test]
  fn rejects_interval_policy() {
    assert!(
      validate_path_filter_policy(false, &["packages/**".to_string()]).is_err()
    );
  }
}
