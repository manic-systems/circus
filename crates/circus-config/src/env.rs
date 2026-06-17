pub fn apply_env_vars(
  table: &mut toml::Value,
  env_vars: impl IntoIterator<Item = (String, String)>,
) {
  const PREFIX: &str = "CIRCUS_";

  for (key, value) in env_vars {
    let Some(rest) = key.strip_prefix(PREFIX) else {
      continue;
    };
    if rest == "CONFIG_FILE" || value.is_empty() {
      continue;
    }

    let segments: Vec<String> =
      rest.split("__").map(str::to_ascii_lowercase).collect();
    set_nested(table, &segments, parse_env_value(&value));
  }
}

pub fn parse_env_value(s: &str) -> toml::Value {
  let trimmed = s.trim();
  if trimmed.starts_with('[')
    && trimmed.ends_with(']')
    && let Ok(mut value) =
      toml::from_str::<toml::Value>(&format!("value = {trimmed}"))
    && let Some(parsed) = value.as_table_mut().and_then(|t| t.remove("value"))
  {
    return parsed;
  }

  match s.to_ascii_lowercase().as_str() {
    "true" | "yes" | "on" => return toml::Value::Boolean(true),
    "false" | "no" | "off" => return toml::Value::Boolean(false),
    _ => {},
  }
  if let Ok(i) = s.parse::<i64>() {
    return toml::Value::Integer(i);
  }
  if s.contains('.')
    && let Ok(f) = s.parse::<f64>()
  {
    return toml::Value::Float(f);
  }
  toml::Value::String(s.to_string())
}

pub fn set_nested(
  table: &mut toml::Value,
  segments: &[String],
  value: toml::Value,
) {
  let [first, rest @ ..] = segments else { return };
  let toml::Value::Table(t) = table else { return };
  if rest.is_empty() {
    t.insert(first.clone(), value);
  } else {
    let child = t
      .entry(first.clone())
      .or_insert_with(|| toml::Value::Table(toml::map::Map::new()));
    set_nested(child, rest, value);
  }
}
