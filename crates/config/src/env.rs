use serde::{
  Deserializer,
  de::{
    self,
    DeserializeOwned,
    IntoDeserializer,
    Unexpected,
    Visitor,
    value::{MapDeserializer, SeqDeserializer},
  },
  forward_to_deserialize_any,
};

/// Overlays every `<prefix>A__B=value` variable onto `table.a.b`.
pub fn apply_env_vars(
  table: &mut toml::Value,
  prefix: &str,
  env_vars: impl IntoIterator<Item = (String, String)>,
) {
  for (key, value) in env_vars {
    let Some(rest) = key.strip_prefix(prefix) else {
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

/// Deserializes `value` into `T`, converting each scalar to the type its
/// field asks for, so `"8"` fills an integer and `off` fills a string.
///
/// # Errors
///
/// Returns an error when a value cannot become the type its field asks for.
pub fn from_value<T: DeserializeOwned>(
  value: toml::Value,
) -> Result<T, toml::de::Error> {
  T::deserialize(Coerce(value))
}

struct Coerce(toml::Value);

impl Coerce {
  fn number<'de, V: Visitor<'de>>(
    self,
    visitor: V,
  ) -> Result<V::Value, toml::de::Error> {
    let toml::Value::String(text) = &self.0 else {
      return self.deserialize_any(visitor);
    };

    let text = text.trim();
    if let Ok(int) = text.parse::<i64>() {
      visitor.visit_i64(int)
    } else if let Ok(int) = text.parse::<u64>() {
      visitor.visit_u64(int)
    } else if let Ok(float) = text.parse::<f64>() {
      visitor.visit_f64(float)
    } else {
      Err(de::Error::invalid_type(Unexpected::Str(text), &visitor))
    }
  }
}

impl IntoDeserializer<'_, toml::de::Error> for Coerce {
  type Deserializer = Self;

  fn into_deserializer(self) -> Self {
    self
  }
}

macro_rules! coerce_numbers {
  ($($method:ident),*) => {
    $(
      fn $method<V: Visitor<'de>>(
        self,
        visitor: V,
      ) -> Result<V::Value, Self::Error> {
        self.number(visitor)
      }
    )*
  };
}

impl<'de> Deserializer<'de> for Coerce {
  type Error = toml::de::Error;

  coerce_numbers!(
    deserialize_i8,
    deserialize_i16,
    deserialize_i32,
    deserialize_i64,
    deserialize_i128,
    deserialize_u8,
    deserialize_u16,
    deserialize_u32,
    deserialize_u64,
    deserialize_u128,
    deserialize_f32,
    deserialize_f64
  );

  forward_to_deserialize_any! {
    bytes byte_buf unit unit_struct seq tuple tuple_struct map struct
    identifier ignored_any
  }

  fn deserialize_any<V: Visitor<'de>>(
    self,
    visitor: V,
  ) -> Result<V::Value, Self::Error> {
    match self.0 {
      toml::Value::String(text) => visitor.visit_string(text),
      toml::Value::Integer(int) => visitor.visit_i64(int),
      toml::Value::Float(float) => visitor.visit_f64(float),
      toml::Value::Boolean(flag) => visitor.visit_bool(flag),
      toml::Value::Array(items) => {
        SeqDeserializer::new(items.into_iter().map(Self))
          .deserialize_any(visitor)
      },
      toml::Value::Table(table) => {
        MapDeserializer::new(
          table.into_iter().map(|(key, value)| (key, Self(value))),
        )
        .deserialize_any(visitor)
      },
      datetime @ toml::Value::Datetime(_) => datetime.deserialize_any(visitor),
    }
  }

  fn deserialize_bool<V: Visitor<'de>>(
    self,
    visitor: V,
  ) -> Result<V::Value, Self::Error> {
    let flag = match &self.0 {
      toml::Value::Integer(0) => Some(false),
      toml::Value::Integer(1) => Some(true),
      toml::Value::String(text) => {
        match text.trim().to_ascii_lowercase().as_str() {
          "true" | "yes" | "on" | "1" => Some(true),
          "false" | "no" | "off" | "0" => Some(false),
          _ => None,
        }
      },
      _ => None,
    };

    match flag {
      Some(flag) => visitor.visit_bool(flag),
      None => self.deserialize_any(visitor),
    }
  }

  fn deserialize_str<V: Visitor<'de>>(
    self,
    visitor: V,
  ) -> Result<V::Value, Self::Error> {
    match self.0 {
      toml::Value::Integer(int) => visitor.visit_string(int.to_string()),
      toml::Value::Float(float) => visitor.visit_string(float.to_string()),
      toml::Value::Boolean(flag) => visitor.visit_string(flag.to_string()),
      other => Self(other).deserialize_any(visitor),
    }
  }

  fn deserialize_string<V: Visitor<'de>>(
    self,
    visitor: V,
  ) -> Result<V::Value, Self::Error> {
    self.deserialize_str(visitor)
  }

  fn deserialize_char<V: Visitor<'de>>(
    self,
    visitor: V,
  ) -> Result<V::Value, Self::Error> {
    self.deserialize_str(visitor)
  }

  fn deserialize_option<V: Visitor<'de>>(
    self,
    visitor: V,
  ) -> Result<V::Value, Self::Error> {
    visitor.visit_some(self)
  }

  fn deserialize_newtype_struct<V: Visitor<'de>>(
    self,
    _name: &'static str,
    visitor: V,
  ) -> Result<V::Value, Self::Error> {
    visitor.visit_newtype_struct(self)
  }

  fn deserialize_enum<V: Visitor<'de>>(
    self,
    name: &'static str,
    variants: &'static [&'static str],
    visitor: V,
  ) -> Result<V::Value, Self::Error> {
    match self.0 {
      toml::Value::String(text) => visitor.visit_enum(text.into_deserializer()),
      other => other.deserialize_enum(name, variants, visitor),
    }
  }
}
