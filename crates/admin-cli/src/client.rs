use color_eyre::eyre::{Context, Result, bail};
use reqwest::Method;
use serde_json::{Map, Value, json};

pub struct ApiClient {
  client:   reqwest::Client,
  base_url: reqwest::Url,
  api_key:  Option<String>,
}

impl ApiClient {
  pub fn new(base_url: &str, api_key: Option<String>) -> Result<Self> {
    let mut normalized = base_url.trim().to_string();
    if !normalized.ends_with('/') {
      normalized.push('/');
    }
    let base_url = reqwest::Url::parse(&normalized)
      .with_context(|| format!("invalid Circus URL: {base_url}"))?;
    Ok(Self {
      client: reqwest::Client::new(),
      base_url,
      api_key,
    })
  }

  pub async fn get(&self, path: &str, auth_required: bool) -> Result<Value> {
    self.send(Method::GET, path, None, auth_required).await
  }

  pub async fn post(
    &self,
    path: &str,
    body: Value,
    auth_required: bool,
  ) -> Result<Value> {
    self
      .send(Method::POST, path, Some(body), auth_required)
      .await
  }

  pub async fn put(
    &self,
    path: &str,
    body: Value,
    auth_required: bool,
  ) -> Result<Value> {
    self
      .send(Method::PUT, path, Some(body), auth_required)
      .await
  }

  pub async fn delete(&self, path: &str, auth_required: bool) -> Result<Value> {
    self.send(Method::DELETE, path, None, auth_required).await
  }

  async fn send(
    &self,
    method: Method,
    path: &str,
    body: Option<Value>,
    auth_required: bool,
  ) -> Result<Value> {
    if auth_required && self.api_key.is_none() {
      bail!(
        "this command requires an API key; pass --api-key or set \
         CIRCUS_API_KEY"
      );
    }

    let url = self.endpoint(path)?;
    let mut request = self.client.request(method.clone(), url.clone());
    if let Some(api_key) = &self.api_key {
      request = request.bearer_auth(api_key);
    }
    if let Some(body) = body {
      request = request.json(&body);
    }

    let response = request
      .send()
      .await
      .with_context(|| format!("request failed: {method} {url}"))?;
    let status = response.status();
    let text = response
      .text()
      .await
      .with_context(|| format!("reading response body for {method} {url}"))?;

    if !status.is_success() {
      bail!(
        "{} {} failed with {}: {}",
        method,
        url,
        status,
        response_error(&text)
      );
    }

    if text.trim().is_empty() {
      return Ok(json!({}));
    }
    serde_json::from_str(&text).map_or_else(|_| Ok(json!({ "body": text })), Ok)
  }

  fn endpoint(&self, path: &str) -> Result<reqwest::Url> {
    self
      .base_url
      .join(path.trim_start_matches('/'))
      .with_context(|| format!("invalid endpoint path: {path}"))
  }
}
pub fn with_query(path: &str, params: &[(&str, String)]) -> String {
  let query = params
    .iter()
    .filter(|(_, value)| !value.is_empty())
    .map(|(key, value)| {
      format!("{}={}", key, urlencoding::encode(value).into_owned())
    })
    .collect::<Vec<_>>()
    .join("&");
  if query.is_empty() {
    path.to_string()
  } else {
    format!("{path}?{query}")
  }
}

pub fn push_optional(
  params: &mut Vec<(&'static str, String)>,
  key: &'static str,
  value: Option<String>,
) {
  if let Some(value) = value {
    params.push((key, value));
  }
}

pub fn insert_optional(
  map: &mut Map<String, Value>,
  key: &str,
  value: Option<String>,
) {
  if let Some(value) = value {
    map.insert(key.to_string(), Value::String(value));
  }
}

pub fn insert_optional_i32(
  map: &mut Map<String, Value>,
  key: &str,
  value: Option<i32>,
) {
  if let Some(value) = value {
    map.insert(key.to_string(), json!(value));
  }
}

pub fn insert_optional_vec(
  map: &mut Map<String, Value>,
  key: &str,
  value: &[String],
) {
  if !value.is_empty() {
    map.insert(key.to_string(), json!(value));
  }
}

fn response_error(text: &str) -> String {
  if text.trim().is_empty() {
    return "empty response body".to_string();
  }
  if let Ok(value) = serde_json::from_str::<Value>(text) {
    if let Some(error) = value.get("error").and_then(Value::as_str) {
      return error.to_string();
    }
    if let Some(message) = value.get("message").and_then(Value::as_str) {
      return message.to_string();
    }
  }
  text.to_string()
}
