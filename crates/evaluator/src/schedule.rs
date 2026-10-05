//! `herculesCI.onSchedule.<name>.when`, matched in UTC like Hercules does.

use chrono::{DateTime, Datelike, Days, Utc, Weekday};
use serde::Deserialize;
use sha2::{Digest, Sha256};

/// Far enough ahead for `dayOfMonth = [29]` restricted to one weekday.
const SEARCH_DAYS: u64 = 366 * 30;

#[derive(Deserialize)]
#[serde(untagged)]
enum Hours {
  One(u32),
  Many(Vec<u32>),
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Raw {
  minute:       Option<u32>,
  hour:         Option<Hours>,
  day_of_week:  Option<Vec<Weekday>>,
  day_of_month: Option<Vec<u32>>,
}

/// One schedule's `when` set.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct When {
  minute:        u32,
  hours:         Vec<u32>,
  days_of_week:  Option<Vec<Weekday>>,
  days_of_month: Option<Vec<u32>>,
}

#[derive(Debug, thiserror::Error)]
pub enum WhenError {
  #[error("invalid `when`: {0}")]
  Shape(#[source] serde_json::Error),
  #[error("`when.{field}` = {value} is out of range")]
  Range { field: &'static str, value: u32 },
  #[error("`when` matches no time in the next {SEARCH_DAYS} days")]
  Never,
}

impl When {
  /// Parse a `when` value, seeding unset fields from `seed`.
  ///
  /// # Errors
  ///
  /// Returns an error when the shape or a field is invalid.
  pub fn parse(
    value: &serde_json::Value,
    seed: &str,
  ) -> Result<Self, WhenError> {
    let raw = Raw::deserialize(value).map_err(WhenError::Shape)?;
    let digest = Sha256::digest(seed.as_bytes());
    let arbitrary =
      u32::from_le_bytes([digest[0], digest[1], digest[2], digest[3]]);
    let check = |field, value, max| {
      if value > max {
        Err(WhenError::Range { field, value })
      } else {
        Ok(value)
      }
    };
    let minute = check("minute", raw.minute.unwrap_or(arbitrary % 60), 59)?;
    let mut hours = match raw.hour {
      None => vec![(arbitrary / 60) % 24],
      Some(Hours::One(hour)) => vec![hour],
      Some(Hours::Many(hours)) => hours,
    };
    for hour in &hours {
      check("hour", *hour, 23)?;
    }
    hours.sort_unstable();
    hours.dedup();
    if let Some(days) = &raw.day_of_month {
      for day in days {
        if *day == 0 {
          return Err(WhenError::Range {
            field: "dayOfMonth",
            value: 0,
          });
        }
        check("dayOfMonth", *day, 31)?;
      }
    }
    Ok(Self {
      minute,
      hours,
      days_of_week: raw.day_of_week,
      days_of_month: raw.day_of_month,
    })
  }

  /// The first matching minute strictly after `after`.
  ///
  /// # Errors
  ///
  /// Returns [`WhenError::Never`] when no day in range matches.
  pub fn next_after(
    &self,
    after: DateTime<Utc>,
  ) -> Result<DateTime<Utc>, WhenError> {
    let start = after.date_naive();
    for offset in 0..SEARCH_DAYS {
      let Some(day) = start.checked_add_days(Days::new(offset)) else {
        break;
      };
      if self
        .days_of_week
        .as_ref()
        .is_some_and(|days| !days.contains(&day.weekday()))
        || self
          .days_of_month
          .as_ref()
          .is_some_and(|days| !days.contains(&day.day()))
      {
        continue;
      }
      let next = self
        .hours
        .iter()
        .filter_map(|hour| day.and_hms_opt(*hour, self.minute, 0))
        .map(|time| time.and_utc())
        .find(|time| *time > after);
      if let Some(next) = next {
        return Ok(next);
      }
    }
    Err(WhenError::Never)
  }
}

#[cfg(test)]
mod tests {
  use chrono::TimeZone;

  use super::*;

  #[test]
  fn next_run_honours_every_field_in_utc() {
    let when = When::parse(
      &serde_json::json!({
        "minute": 30,
        "hour": [3, 15],
        "dayOfWeek": ["Mon"],
      }),
      "seed",
    )
    .expect("valid when");
    // 2026-10-01 is a Thursday.
    let after = Utc.with_ymd_and_hms(2026, 10, 1, 12, 0, 0).unwrap();
    let first = when.next_after(after).expect("next run");
    assert_eq!(first, Utc.with_ymd_and_hms(2026, 10, 5, 3, 30, 0).unwrap());
    assert_eq!(
      when.next_after(first).expect("same-day run"),
      Utc.with_ymd_and_hms(2026, 10, 5, 15, 30, 0).unwrap()
    );

    let leap = When::parse(&serde_json::json!({ "dayOfMonth": [29] }), "seed")
      .expect("valid when");
    let feb = Utc.with_ymd_and_hms(2027, 2, 1, 0, 0, 0).unwrap();
    assert_eq!(leap.next_after(feb).expect("next run").month(), 3);

    assert!(matches!(
      When::parse(&serde_json::json!({ "hour": 24 }), "seed"),
      Err(WhenError::Range {
        field: "hour",
        value: 24,
      })
    ));
  }
}
