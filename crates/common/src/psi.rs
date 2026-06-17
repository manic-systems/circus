#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct PressureAverages {
  pub avg10: f64,
  pub avg60: f64,
}

#[must_use]
pub fn parse_pressure_some(text: &str) -> Option<PressureAverages> {
  text.lines().find_map(|line| {
    let rest = line.strip_prefix("some ")?;
    parse_pressure_fields(rest)
  })
}

#[must_use]
pub fn parse_pressure_triplet_avg10(text: &str) -> Option<(f64, f64, f64)> {
  let mut avg10s = text.lines().filter_map(|line| {
    let rest = line.strip_prefix("some ")?;
    parse_pressure_fields(rest).map(|avg| avg.avg10)
  });

  Some((avg10s.next()?, avg10s.next()?, avg10s.next()?))
}

fn parse_pressure_fields(fields: &str) -> Option<PressureAverages> {
  let mut avg10 = None;
  let mut avg60 = None;
  for kv in fields.split_whitespace() {
    if let Some(v) = kv.strip_prefix("avg10=") {
      avg10 = v.parse().ok();
    } else if let Some(v) = kv.strip_prefix("avg60=") {
      avg60 = v.parse().ok();
    }
  }
  Some(PressureAverages {
    avg10: avg10?,
    avg60: avg60?,
  })
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn parses_some_pressure_averages() {
    let avg = parse_pressure_some(
      "some avg10=5.00 avg60=3.00 avg300=2.00 total=12345\nfull avg10=1.00 \
       avg60=1.00 avg300=1.00 total=1",
    )
    .expect("some line should parse");

    assert_eq!(avg, PressureAverages {
      avg10: 5.0,
      avg60: 3.0,
    });
  }

  #[test]
  fn parses_three_pressure_stanzas() {
    let avg = parse_pressure_triplet_avg10(
      "some avg10=5.00 avg60=3.00 avg300=2.00 total=12345\nsome avg10=10.50 \
       avg60=8.00 avg300=5.00 total=67890\nfull avg10=2.00 avg60=1.00 \
       avg300=0.50 total=11111\nsome avg10=1.25 avg60=0.80 avg300=0.40 \
       total=22222\n",
    )
    .expect("three some lines should parse");

    assert_eq!(avg, (5.0, 10.5, 1.25));
  }

  #[test]
  fn rejects_missing_pressure_stanzas() {
    assert!(parse_pressure_triplet_avg10("").is_none());
    assert!(parse_pressure_triplet_avg10("some avg10=5.00\n").is_none());
  }
}
