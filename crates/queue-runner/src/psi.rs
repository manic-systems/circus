//! Pressure Stall Information (PSI) checks for remote builders.
//!
//! Builders advertise themselves via SSH; before dispatching a build we
//! optionally read `/proc/pressure/{cpu,memory,io}` over SSH and skip
//! builders whose recent pressure exceeds a configured threshold.
//!
//! Hydra has no equivalent feature: its scheduler relies purely on
//! `maxJobs` and `speedFactor`. PSI augments that without replacing it,
//! and an SSH failure is never treated as a fault (the builder is
//! considered unloaded). Results are cached per builder for a short
//! window to avoid an SSH round trip on every dispatch decision.

use std::{sync::Arc, time::Duration};

use dashmap::DashMap;
use tokio::time::Instant;

#[derive(Clone, Copy, Debug, Default)]
pub struct PsiSnapshot {
  pub cpu_avg10:    f64,
  pub memory_avg10: f64,
  pub io_avg10:     f64,
}

impl PsiSnapshot {
  /// True when any of the three resources exceeds the threshold.
  #[must_use]
  pub fn exceeds(&self, threshold: f64) -> bool {
    self.cpu_avg10 > threshold
      || self.memory_avg10 > threshold
      || self.io_avg10 > threshold
  }
}

/// Cache of the most recent PSI reading per SSH URI. The TTL is whatever
/// `psi_check_timeout` is set to; in practice readings are useful for a
/// few seconds at most because `avg10` itself only smooths the past 10
/// seconds.
#[derive(Debug, Default)]
pub struct PsiCache {
  entries: DashMap<String, (Instant, Option<PsiSnapshot>)>,
}

impl PsiCache {
  #[must_use]
  pub fn new() -> Arc<Self> {
    Arc::new(Self::default())
  }

  /// Look up a cached reading; returns `None` if the entry is missing or
  /// stale. A cached `None` value indicates a recent SSH failure, which
  /// callers should treat as "unknown, do not penalize".
  pub fn get(
    &self,
    ssh_uri: &str,
    ttl: Duration,
  ) -> Option<Option<PsiSnapshot>> {
    let entry = self.entries.get(ssh_uri)?;
    if entry.0.elapsed() <= ttl {
      Some(entry.1)
    } else {
      None
    }
  }

  pub fn put(&self, ssh_uri: String, value: Option<PsiSnapshot>) {
    self.entries.insert(ssh_uri, (Instant::now(), value));
  }
}

/// Read PSI over SSH. `ssh_uri` is expected to come from trusted
/// admin-configured remote builder settings, not from end-user input.
///
/// Returns `None` if anything goes wrong - the caller then treats the builder
/// as unloaded rather than penalizing it for a transient connectivity blip.
pub async fn read(ssh_uri: &str, timeout: Duration) -> Option<PsiSnapshot> {
  let cmd = tokio::process::Command::new("ssh")
    .args([
      "-o",
      "BatchMode=yes",
      "-o",
      "ConnectTimeout=3",
      "-o",
      "StrictHostKeyChecking=accept-new",
      ssh_uri,
      "cat /proc/pressure/cpu /proc/pressure/memory /proc/pressure/io",
    ])
    .stdin(std::process::Stdio::null())
    .stderr(std::process::Stdio::null())
    .kill_on_drop(true)
    .output();

  let output = tokio::time::timeout(timeout, cmd).await.ok()?.ok()?;
  if !output.status.success() {
    return None;
  }

  let text = std::str::from_utf8(&output.stdout).ok()?;
  parse(text)
}

/// Convenience: check the cache first, fall back to a fresh SSH read.
pub async fn read_cached(
  cache: &PsiCache,
  ssh_uri: &str,
  timeout: Duration,
) -> Option<PsiSnapshot> {
  if let Some(cached) = cache.get(ssh_uri, timeout) {
    return cached;
  }
  let snapshot = read(ssh_uri, timeout).await;
  cache.put(ssh_uri.to_string(), snapshot);
  snapshot
}

/// Parse the concatenated output of three `/proc/pressure/*` files.
///
/// Each file emits a `some` line and (for memory/io) a `full` line. We
/// take the first `some avg10=` value from each of the three stanzas.
fn parse(text: &str) -> Option<PsiSnapshot> {
  let mut some_avg10s = text.lines().filter_map(|line| {
    let rest = line.strip_prefix("some ")?;
    rest
      .split_whitespace()
      .find_map(|kv| kv.strip_prefix("avg10="))
      .and_then(|v| v.parse::<f64>().ok())
  });

  Some(PsiSnapshot {
    cpu_avg10:    some_avg10s.next()?,
    memory_avg10: some_avg10s.next()?,
    io_avg10:     some_avg10s.next()?,
  })
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn parses_three_stanzas() {
    let text = "some avg10=5.00 avg60=3.00 avg300=2.00 total=12345\nsome \
                avg10=10.50 avg60=8.00 avg300=5.00 total=67890\nfull \
                avg10=2.00 avg60=1.00 avg300=0.50 total=11111\nsome \
                avg10=1.25 avg60=0.80 avg300=0.40 total=22222\nfull \
                avg10=0.50 avg60=0.30 avg300=0.20 total=33333\n";
    let snap = parse(text).expect("should parse");
    assert!((snap.cpu_avg10 - 5.0).abs() < f64::EPSILON);
    assert!((snap.memory_avg10 - 10.5).abs() < f64::EPSILON);
    assert!((snap.io_avg10 - 1.25).abs() < f64::EPSILON);
  }

  #[test]
  fn rejects_missing_stanzas() {
    assert!(parse("").is_none());
    assert!(parse("some avg10=5.00\n").is_none());
  }

  #[test]
  fn exceeds_threshold() {
    let snap = PsiSnapshot {
      cpu_avg10:    1.0,
      memory_avg10: 50.0,
      io_avg10:     2.0,
    };
    assert!(snap.exceeds(40.0));
    assert!(!snap.exceeds(60.0));
  }
}
