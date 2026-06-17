//! Read /proc/pressure on Linux. Returns zeros on other platforms or when
//! PSI is not available (kernel without `CONFIG_PSI=y`, container without
//! the cgroupv2 file).

use std::fs;

#[derive(Debug, Default, Clone, Copy)]
pub struct Snapshot {
  pub cpu_avg10: f32,
  pub mem_avg10: f32,
  pub io_avg10:  f32,
  pub cpu_avg60: f32,
  pub mem_avg60: f32,
  pub io_avg60:  f32,
}

/// Read all three PSI files; missing files contribute zero rather than an
/// error. The runner treats unknown PSI as "do not penalise".
#[must_use]
pub fn read() -> Snapshot {
  let mut out = Snapshot::default();
  let (cpu10, cpu60) = read_avg("/proc/pressure/cpu");
  out.cpu_avg10 = cpu10;
  out.cpu_avg60 = cpu60;
  let (mem10, mem60) = read_avg("/proc/pressure/memory");
  out.mem_avg10 = mem10;
  out.mem_avg60 = mem60;
  let (io10, io60) = read_avg("/proc/pressure/io");
  out.io_avg10 = io10;
  out.io_avg60 = io60;
  out
}

/// Parse the "some" line of a pressure file. Returns (avg10, avg60).
///
/// Format example:
/// ```text
/// some avg10=0.00 avg60=0.00 avg300=0.00 total=0
/// full avg10=0.00 avg60=0.00 avg300=0.00 total=0
/// ```
fn read_avg(path: &str) -> (f32, f32) {
  let Ok(contents) = fs::read_to_string(path) else {
    return (0.0, 0.0);
  };
  circus_common::psi::parse_pressure_some(&contents)
    .map_or((0.0, 0.0), |avg| (avg.avg10 as f32, avg.avg60 as f32))
}
