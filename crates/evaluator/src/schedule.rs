//! `herculesCI.onSchedule.<name>.when`, matched in UTC like Hercules does.

use jiff::{Timestamp, ToSpan, civil::Weekday, tz::TimeZone};
use serde::Deserialize;
use sha2::{Digest, Sha256};

/// Far enough ahead for `dayOfMonth = [29]` restricted to one weekday.
const SEARCH_DAYS: i32 = 366 * 30;

#[derive(Deserialize)]
#[serde(untagged)]
enum Hours {
  One(u32),
  Many(Vec<u32>),
}

#[derive(Clone, Copy, Deserialize)]
enum Day {
  #[serde(alias = "Monday")]
  Mon,
  #[serde(alias = "Tuesday")]
  Tue,
  #[serde(alias = "Wednesday")]
  Wed,
  #[serde(alias = "Thursday")]
  Thu,
  #[serde(alias = "Friday")]
  Fri,
  #[serde(alias = "Saturday")]
  Sat,
  #[serde(alias = "Sunday")]
  Sun,
}

impl From<Day> for Weekday {
  fn from(day: Day) -> Self {
    match day {
      Day::Mon => Self::Monday,
      Day::Tue => Self::Tuesday,
      Day::Wed => Self::Wednesday,
      Day::Thu => Self::Thursday,
      Day::Fri => Self::Friday,
      Day::Sat => Self::Saturday,
      Day::Sun => Self::Sunday,
    }
  }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Raw {
  minute:       Option<u32>,
  hour:         Option<Hours>,
  day_of_week:  Option<Vec<Day>>,
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
      days_of_week: raw
        .day_of_week
        .map(|days| days.into_iter().map(Weekday::from).collect()),
      days_of_month: raw.day_of_month,
    })
  }

  /// The first matching minute strictly after `after`.
  ///
  /// # Errors
  ///
  /// Returns [`WhenError::Never`] when no day in range matches.
  pub fn next_after(&self, after: Timestamp) -> Result<Timestamp, WhenError> {
    let start = after.to_zoned(TimeZone::UTC).date();
    for offset in 0..SEARCH_DAYS {
      let Ok(day) = start.checked_add(offset.days()) else {
        break;
      };
      if self
        .days_of_week
        .as_ref()
        .is_some_and(|days| !days.contains(&day.weekday()))
        || self.days_of_month.as_ref().is_some_and(|days| {
          u32::try_from(day.day()).is_ok_and(|today| !days.contains(&today))
        })
      {
        continue;
      }
      let next = self
        .hours
        .iter()
        .filter_map(|hour| {
          let hour = i8::try_from(*hour).ok()?;
          let minute = i8::try_from(self.minute).ok()?;
          TimeZone::UTC.to_timestamp(day.at(hour, minute, 0, 0)).ok()
        })
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
  use jiff::civil::date;

  use super::*;

  fn utc(year: i16, month: i8, day: i8, hour: i8, minute: i8) -> Timestamp {
    TimeZone::UTC
      .to_timestamp(date(year, month, day).at(hour, minute, 0, 0))
      .expect("valid UTC time")
  }

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
    let after = utc(2026, 10, 1, 12, 0);
    let first = when.next_after(after).expect("next run");
    assert_eq!(first, utc(2026, 10, 5, 3, 30));
    assert_eq!(
      when.next_after(first).expect("same-day run"),
      utc(2026, 10, 5, 15, 30)
    );

    let leap = When::parse(&serde_json::json!({ "dayOfMonth": [29] }), "seed")
      .expect("valid when");
    let feb = utc(2027, 2, 1, 0, 0);
    assert_eq!(
      leap
        .next_after(feb)
        .expect("next run")
        .to_zoned(TimeZone::UTC)
        .month(),
      3
    );

    assert!(matches!(
      When::parse(&serde_json::json!({ "hour": 24 }), "seed"),
      Err(WhenError::Range {
        field: "hour",
        value: 24,
      })
    ));
  }
}
