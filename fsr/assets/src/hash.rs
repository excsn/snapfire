/// Eight hex digits of xxh3 over the bytes, the same digest snapfirec names
/// an emitted asset with, so a file hashed here and one it hashed agree.
pub fn of(bytes: &[u8]) -> String {
  format!("{:08x}", xxhash_rust::xxh3::xxh3_64(bytes) as u32)
}

/// `<stem>.<hash>.<ext>`, the name an asset is served under.
pub fn emitted_name(stem: &str, hash: &str, ext: &str) -> String {
  format!("{stem}.{hash}.{ext}")
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn the_digest_is_eight_hex_digits_and_stable() {
    let a = of(b"hello");
    assert_eq!(a.len(), 8);
    assert!(a.chars().all(|c| c.is_ascii_hexdigit()));
    assert_eq!(a, of(b"hello"));
    assert_ne!(a, of(b"hello!"));
    assert_eq!(emitted_name("hero", &a, "png"), format!("hero.{a}.png"));
  }
}
