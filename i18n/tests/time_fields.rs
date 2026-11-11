//! A locale's numbering system, hour cycle and the widths of a time's fields, each as node v26.8.1's `Intl` resolves them: `new Intl.NumberFormat(tag).resolvedOptions().numberingSystem` and `new Intl.DateTimeFormat(tag, { hour, minute, second }).resolvedOptions()`.

/// A tag, its numbering system, its hour cycle, whether the hour and minutes of `{ hour }` then `{ hour, minute }` are two digits, then the same for `{ hour, minute, second }`.
const NODE: &[(&str, &str, &str, [bool; 3], [bool; 3])] = &[
  ("en", "latn", "h12", [false, false, true], [false, true, true]),
  ("de", "latn", "h23", [true, true, true], [true, true, true]),
  ("ru", "latn", "h23", [false, true, true], [true, true, true]),
  ("it", "latn", "h23", [false, true, true], [true, true, true]),
  ("zh", "latn", "h23", [false, true, true], [true, true, true]),
  ("ko-CN", "latn", "h23", [false, true, true], [false, false, false]),
  ("fa-AF", "arabext", "h23", [false, false, true], [false, true, true]),
  ("ps", "arabext", "h23", [false, false, true], [false, true, true]),
  ("th", "latn", "h23", [true, true, true], [true, true, true]),
  ("ar-EG", "arab", "h12", [false, false, true], [false, true, true]),
  ("bn", "beng", "h12", [false, false, true], [false, true, true]),
  ("ja", "latn", "h23", [false, false, true], [false, true, true]),
  ("en-u-hc-h23", "latn", "h23", [true, true, true], [true, true, true]),
  ("de-u-hc-h12", "latn", "h12", [false, false, true], [false, true, true]),
  ("hi-IN", "latn", "h12", [false, false, true], [false, true, true]),
  ("my", "mymr", "h23", [true, true, true], [true, true, true]),
];

#[test]
fn a_locales_numbering_system_hour_cycle_and_time_widths_are_what_node_resolves() {
  let mut wrong = Vec::new();
  for (tag, numbering, cycle, [hour, hm_hour, hm_minute], hms) in NODE {
    let (alone, with_minute) = (snapfire_i18n::time_widths(tag, false, false), snapfire_i18n::time_widths(tag, true, false));
    let got = (snapfire_i18n::numbering_system(tag), snapfire_i18n::hour_cycle(tag), [alone[0], with_minute[0], with_minute[1]], snapfire_i18n::time_widths(tag, true, true));
    let want = (*numbering, *cycle, [*hour, *hm_hour, *hm_minute], *hms);
    if got != want {
      wrong.push(format!("{tag}: got {got:?} want {want:?}"));
    }
  }
  assert!(wrong.is_empty(), "{}", wrong.join("\n"));
}
