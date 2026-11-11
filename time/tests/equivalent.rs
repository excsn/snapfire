//! The instant V8 reads a zone's daylight saving at for a `Date` before 1970 or after 2038: the same month, day and time in a year from 2008 to 2035 that starts on the same weekday and has the same length.

use snapfire_time::equivalent_time;

const DAY: f64 = 86_400_000.0;

#[test]
fn an_instant_outside_1970_to_2038_moves_to_an_equivalent_year() {
  let noon = |days: f64| days * DAY + 43_200_000.0;
  for (from, to) in [
    (noon(-25_568.0), noon(19_722.0)),
    (noon(-25_567.0), noon(23_741.0)),
    (noon(25_932.0), noon(15_705.0)),
    (noon(-1.0), noon(22_644.0)),
  ] {
    assert_eq!(equivalent_time(from), to, "{from}");
  }
}

#[test]
fn an_instant_from_1970_to_2038_stays() {
  for ms in [0.0, 1_720_000_000_000.0, 2_147_483_647_000.0] {
    assert_eq!(equivalent_time(ms), ms);
  }
}
