use std::time::Duration;

/// `<n>`, `<n>s`, `<n>m`, `<n>h` or `<n>d`; the spelling every lifetime in a
/// configuration file or a contract uses.
pub fn parse_duration(raw: &str) -> Option<Duration> {
  let raw = raw.trim();
  let (digits, unit) = raw.split_at(raw.find(|c: char| !c.is_ascii_digit()).unwrap_or(raw.len()));
  let n: u64 = digits.parse().ok()?;
  // Checked: `9999999999999999d` parses as a `u64` and then leaves the range,
  // which panics a debug build and silently wraps a release one.
  let seconds = match unit.trim() {
    "" | "s" => Some(n),
    "m" => n.checked_mul(60),
    "h" => n.checked_mul(3600),
    "d" => n.checked_mul(86_400),
    _ => None,
  };
  seconds.map(Duration::from_secs)
}
