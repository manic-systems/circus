use color_eyre::eyre::Result;
use comfy_table::Table;
use serde_json::Value;

pub fn print_users(value: &Value) {
  print_table(
    &["ID", "Username", "Email", "Role", "Enabled", "Type"],
    &items(value)
      .iter()
      .map(|user| {
        vec![
          field(user, "id"),
          field(user, "username"),
          field(user, "email"),
          field(user, "role"),
          field(user, "enabled"),
          field(user, "user_type"),
        ]
      })
      .collect::<Vec<_>>(),
  );
}

pub fn print_builders(value: &Value) {
  print_table(
    &[
      "ID", "Name", "SSH URI", "Systems", "Jobs", "Speed", "Enabled",
    ],
    &items(value)
      .iter()
      .map(|builder| {
        vec![
          field(builder, "id"),
          field(builder, "name"),
          field(builder, "ssh_uri"),
          field(builder, "systems"),
          field(builder, "max_jobs"),
          field(builder, "speed_factor"),
          field(builder, "enabled"),
        ]
      })
      .collect::<Vec<_>>(),
  );
}

pub fn print_builder_sessions(value: &Value) {
  print_table(
    &[
      "Machine",
      "Name",
      "Host",
      "Systems",
      "Jobs",
      "Load",
      "Connected",
      "Last Seen",
    ],
    &items(value)
      .iter()
      .map(|session| {
        vec![
          field(session, "machine_id"),
          field(session, "name"),
          field(session, "hostname"),
          field(session, "systems"),
          format!(
            "{}/{}",
            field(session, "current_jobs"),
            field(session, "max_jobs")
          ),
          field(session, "load1"),
          field(session, "connected"),
          field(session, "last_seen"),
        ]
      })
      .collect::<Vec<_>>(),
  );
}

pub fn print_pinned_outputs(value: &Value) {
  print_table(
    &["Build", "Job", "System", "Status", "Product", "Root"],
    &items(value)
      .iter()
      .map(|product| {
        vec![
          field(product, "build_id"),
          field(product, "job_name"),
          field(product, "system"),
          field(product, "status"),
          field(product, "product_name"),
          short(&field(product, "gc_root_path"), 80),
        ]
      })
      .collect::<Vec<_>>(),
  );
}

pub fn print_builds(value: &Value) {
  print_table(
    &[
      "ID", "Job", "Status", "System", "Priority", "Keep", "Created",
    ],
    &items(value)
      .iter()
      .map(|build| {
        vec![
          field(build, "id"),
          field(build, "job_name"),
          field(build, "status"),
          field(build, "system"),
          field(build, "priority"),
          field(build, "keep"),
          field(build, "created_at"),
        ]
      })
      .collect::<Vec<_>>(),
  );
}

pub fn print_json(value: &Value) -> Result<()> {
  println!("{}", serde_json::to_string_pretty(value)?);
  Ok(())
}

pub fn print_page(value: &Value) {
  if value.get("total").is_some() {
    println!(
      "total={} limit={} offset={}",
      field(value, "total"),
      field(value, "limit"),
      field(value, "offset")
    );
  }
}

pub fn print_table(headers: &[&str], rows: &[Vec<String>]) {
  if rows.is_empty() {
    println!("No rows.");
    return;
  }

  let mut table = Table::new();
  table.set_header(headers.to_vec());
  for row in rows {
    table.add_row(row.clone());
  }
  println!("{table}");
}

pub fn items(value: &Value) -> Vec<&Value> {
  if let Some(items) = value.get("items").and_then(Value::as_array) {
    return items.iter().collect();
  }
  if let Some(items) = value.as_array() {
    return items.iter().collect();
  }
  vec![value]
}

pub fn field(value: &Value, key: &str) -> String {
  value
    .get(key)
    .map_or_else(|| "-".to_string(), value_to_string)
}

fn value_to_string(value: &Value) -> String {
  match value {
    Value::Null => "-".to_string(),
    Value::Bool(v) => v.to_string(),
    Value::Number(v) => v.to_string(),
    Value::String(v) if v.is_empty() => "-".to_string(),
    Value::String(v) => v.clone(),
    Value::Array(values) => {
      values
        .iter()
        .map(value_to_string)
        .collect::<Vec<_>>()
        .join(",")
    },
    Value::Object(_) => {
      serde_json::to_string(value).unwrap_or_else(|_| "-".to_string())
    },
  }
}

pub fn short(value: &str, max: usize) -> String {
  if value.chars().count() <= max {
    return value.to_string();
  }
  let mut out = value
    .chars()
    .take(max.saturating_sub(3))
    .collect::<String>();
  out.push_str("...");
  out
}
