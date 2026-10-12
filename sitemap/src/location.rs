use crate::error::{Error, Result};
use crate::limit;

/// Validates a `loc` without parsing it.
///
/// A url crate would reject relative locations that the protocol itself only
/// discourages and would turn every rejection into a dropped entry rather than
/// a reported one.
pub(crate) fn location(loc: String) -> Result<String> {
  if loc.is_empty() {
    return Err(Error::LocationEmpty);
  }

  let length = loc.chars().count();

  if length >= limit::MAX_LOCATION_LEN {
    return Err(Error::LocationTooLong { length });
  }

  Ok(loc)
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn an_empty_location_is_rejected() {
    assert_eq!(location(String::new()).unwrap_err(), Error::LocationEmpty);
  }

  #[test]
  fn the_limit_is_exclusive() {
    let at_limit = "a".repeat(limit::MAX_LOCATION_LEN);
    let under = "a".repeat(limit::MAX_LOCATION_LEN - 1);

    assert_eq!(
      location(at_limit).unwrap_err(),
      Error::LocationTooLong {
        length: limit::MAX_LOCATION_LEN
      }
    );
    assert!(location(under).is_ok());
  }

  #[test]
  fn length_is_counted_in_characters_not_bytes() {
    let loc = "é".repeat(limit::MAX_LOCATION_LEN - 1);

    assert!(location(loc).is_ok());
  }
}
