use std::time::Duration;

/// `<n>`, `<n>s`, `<n>m`, `<n>h` or `<n>d`; the spelling every lifetime in a
/// configuration file or a contract uses.
pub fn parse_duration(raw: &str) -> Option<Duration> {
  let raw = raw.trim();
  let (digits, unit) = raw.split_at(raw.find(|c: char| !c.is_ascii_digit()).unwrap_or(raw.len()));
  let n: u64 = digits.parse().ok()?;
  let seconds = match unit.trim() {
    "" | "s" => n,
    "m" => n * 60,
    "h" => n * 3600,
    "d" => n * 86_400,
    _ => return None,
  };
  Some(Duration::from_secs(seconds))
}
