//! Address ranges for telling internal hosts from public ones.

use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};

/// Special-purpose ranges from the IANA registries.
const INTERNAL: [IpRange; 19] = [
  IpRange::v4([0, 0, 0, 0], 8),
  IpRange::v4([10, 0, 0, 0], 8),
  IpRange::v4([100, 64, 0, 0], 10),
  IpRange::v4([127, 0, 0, 0], 8),
  IpRange::v4([169, 254, 0, 0], 16),
  IpRange::v4([172, 16, 0, 0], 12),
  IpRange::v4([192, 0, 0, 0], 24),
  IpRange::v4([192, 168, 0, 0], 16),
  IpRange::v4([198, 18, 0, 0], 15),
  IpRange::v4([224, 0, 0, 0], 4),
  IpRange::v4([240, 0, 0, 0], 4),
  IpRange::v6(Ipv6Addr::UNSPECIFIED, 128),
  IpRange::v6(Ipv6Addr::LOCALHOST, 128),
  IpRange::v6(Ipv6Addr::new(0x64, 0xFF9B, 1, 0, 0, 0, 0, 0), 48),
  IpRange::v6(Ipv6Addr::new(0x2001, 0, 0, 0, 0, 0, 0, 0), 32),
  IpRange::v6(Ipv6Addr::new(0xFC00, 0, 0, 0, 0, 0, 0, 0), 7),
  IpRange::v6(Ipv6Addr::new(0xFEC0, 0, 0, 0, 0, 0, 0, 0), 10),
  IpRange::v6(Ipv6Addr::new(0xFE80, 0, 0, 0, 0, 0, 0, 0), 10),
  IpRange::v6(Ipv6Addr::new(0xFF00, 0, 0, 0, 0, 0, 0, 0), 8),
];

/// A CIDR range, or a single address when the prefix covers all of it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct IpRange {
  addr: IpAddr,
  len:  u32,
}

impl IpRange {
  const fn v4(octets: [u8; 4], len: u32) -> Self {
    Self {
      addr: IpAddr::V4(Ipv4Addr::new(
        octets[0], octets[1], octets[2], octets[3],
      )),
      len,
    }
  }

  const fn v6(addr: Ipv6Addr, len: u32) -> Self {
    Self {
      addr: IpAddr::V6(addr),
      len,
    }
  }

  #[must_use]
  pub fn contains(&self, ip: IpAddr) -> bool {
    let (addr, net, width) = match (ip, self.addr) {
      (IpAddr::V4(ip), IpAddr::V4(net)) => {
        (u128::from(u32::from(ip)), u128::from(u32::from(net)), 32)
      },
      (IpAddr::V6(ip), IpAddr::V6(net)) => {
        (u128::from(ip), u128::from(net), 128)
      },
      _ => return false,
    };

    (addr ^ net)
      .checked_shr(width - self.len)
      .is_none_or(|diff| diff == 0)
  }
}

/// Whether `ip` is loopback, private, link-local or otherwise not public,
/// including IPv4 addresses inside compatible, mapped, NAT64 and 6to4 IPv6
/// addresses.
#[must_use]
pub fn is_internal_ip(ip: IpAddr) -> bool {
  INTERNAL.iter().any(|range| range.contains(ip))
    || matches!(ip, IpAddr::V6(v6) if embedded_ipv4(v6).is_some_and(|v4| is_internal_ip(v4.into())))
}

fn embedded_ipv4(ip: Ipv6Addr) -> Option<Ipv4Addr> {
  let ([0, 0, 0, 0, 0, 0 | 0xFFFF, hi, lo]
  | [0x64, 0xFF9B, 0, 0, 0, 0, hi, lo]
  | [0x2002, hi, lo, ..]) = ip.segments()
  else {
    return None;
  };

  Some(Ipv4Addr::from((u32::from(hi) << 16) | u32::from(lo)))
}

/// A URL host with brackets, a trailing root dot and case removed, since DNS
/// ignores the last two.
#[must_use]
pub fn normalize_host(raw_host: &str) -> String {
  raw_host
    .trim_matches(['[', ']'])
    .trim_end_matches('.')
    .to_ascii_lowercase()
}

#[cfg(test)]
#[expect(clippy::unwrap_used, reason = "fine in tests")]
mod tests {
  use super::*;

  #[test]
  fn internal_ranges() {
    for (ip, internal) in [
      ("10.0.0.1", true),
      ("11.0.0.1", false),
      ("100.64.0.1", true),
      ("100.128.0.1", false),
      ("172.31.255.255", true),
      ("172.32.0.0", false),
      ("198.19.255.255", true),
      ("198.20.0.0", false),
      ("255.255.255.255", true),
      ("8.8.8.8", false),
      ("::ffff:10.0.0.1", true),
      ("::10.0.0.1", true),
      ("::8.8.8.8", false),
      ("64:ff9b:1::1", true),
      ("2001::1", true),
      ("fec0::1", true),
      ("64:ff9b::7f00:1", true),
      ("2002:7f00:1::", true),
      ("2002:808:808::", false),
      ("fd7a:115c:a1e0::1", true),
      ("fe80::1", true),
      ("2606:4700::1111", false),
    ] {
      assert_eq!(is_internal_ip(ip.parse().unwrap()), internal, "{ip}");
    }
  }
}
