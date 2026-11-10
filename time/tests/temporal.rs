//! The Temporal surface over strings, each answer as node v26.8.1 gives it.

use snapfire_time::temporal::{call, call_static, construct, get, now, Answer, Arg, Kind};

#[test]
fn a_date_moves_by_a_month_and_reads_its_weekday() {
  let due = call_static(Kind::PlainDate, "from", &[Arg::Str("2024-01-31".into())]).unwrap();
  let Answer::Temporal(kind, text) = due else { panic!("{due:?}") };
  let next = call(kind, &text, "add", &[Arg::Obj(vec![("months".into(), Arg::Num(1.0))])]).unwrap();
  assert_eq!(next, Answer::Temporal(Kind::PlainDate, "2024-02-29".into()));
  assert_eq!(get(Kind::PlainDate, "2024-02-29", "dayOfWeek").unwrap(), Answer::Num(4.0));
}

#[test]
fn now_reads_the_instant_it_is_handed() {
  if !snapfire_time::Source::Compiled.built() && !snapfire_time::Source::System.built() {
    return;
  }
  assert_eq!(now("plainDateISO", 1_726_011_440_123.0, &[Arg::Str("Asia/Tokyo".into())]).unwrap(), Answer::Temporal(Kind::PlainDate, "2024-09-11".into()));
  assert_eq!(now("instant", 0.0, &[]).unwrap(), Answer::Temporal(Kind::Instant, "1970-01-01T00:00:00Z".into()));
}

#[test]
fn a_bad_value_is_a_range_error_and_a_wrong_kind_a_type_error() {
  assert!(matches!(call_static(Kind::PlainDate, "from", &[Arg::Str("2024-13-01".into())]), Err(snapfire_time::Error::Range(_))));
  assert!(matches!(call_static(Kind::PlainDate, "from", &[Arg::Bool(true)]), Err(snapfire_time::Error::Type(_))));
  assert!(matches!(call(Kind::Duration, "PT1H", "valueOf", &[]), Err(snapfire_time::Error::Type(_))));
  assert_eq!(construct(Kind::Duration, &[Arg::Num(1.0), Arg::Num(2.0)]).unwrap(), Answer::Temporal(Kind::Duration, "P1Y2M".into()));
}
