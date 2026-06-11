use serde::{Deserialize, Deserializer};

/// Deserialize an optional query-string field, mapping a blank value to
/// [`None`].
pub fn empty_string_as_none<'de, D>(de: D) -> Result<Option<String>, D::Error>
where
  D: Deserializer<'de>,
{
  Ok(Option::<String>::deserialize(de)?.filter(|s| !s.is_empty()))
}
