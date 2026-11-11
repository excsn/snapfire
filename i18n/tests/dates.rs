//! Each row is a locale, then what node v26.8.1 (ICU 78.3, CLDR 48) writes under `timeZone: "UTC"`: `toLocaleDateString`, `toLocaleString` and `toLocaleTimeString` of 2024-09-10 12:00:00, then `toLocaleDateString` of 0987-01-05.

use snapfire_i18n::{date_with_time, numeric_date};

/// A join node writes that CLDR's data does not hold: ICU joins a Portuguese date and time with a comma, while CLDR 48.0 and 48.2.1 give `pt` a space, so the crate writes the space.
const NOT_IN_CLDR: &[(&str, &str)] = &[("pt-BR", "10/09/2024 12:00:00")];

const NODE: &[(&str, &str, &str, &str, &str)] = &[
  ("en-US", "9/10/2024", "9/10/2024, 12:00:00 PM", "12:00:00 PM", "1/5/987"),
  ("de-DE", "10.9.2024", "10.9.2024, 12:00:00", "12:00:00", "5.1.987"),
  ("ja-JP", "2024/9/10", "2024/9/10 12:00:00", "12:00:00", "987/1/5"),
  ("fr-FR", "10/09/2024", "10/09/2024 12:00:00", "12:00:00", "05/01/987"),
  ("en-GB", "10/09/2024", "10/09/2024, 12:00:00", "12:00:00", "05/01/987"),
  ("ar-EG", "١٠‏/٩‏/٢٠٢٤", "١٠‏/٩‏/٢٠٢٤، ١٢:٠٠:٠٠ م", "١٢:٠٠:٠٠ م", "٥‏/١‏/٩٨٧"),
  ("hi-IN", "10/9/2024", "10/9/2024, 12:00:00 pm", "12:00:00 pm", "5/1/987"),
  ("zh-CN", "2024/9/10", "2024/9/10 12:00:00", "12:00:00", "987/1/5"),
  ("es-ES", "10/9/2024", "10/9/2024, 12:00:00", "12:00:00", "5/1/987"),
  ("ru-RU", "10.09.2024", "10.09.2024, 12:00:00", "12:00:00", "05.01.987"),
  ("pt-BR", "10/09/2024", "10/09/2024, 12:00:00", "12:00:00", "05/01/987"),
  ("ko-KR", "2024. 9. 10.", "2024. 9. 10. 오후 12:00:00", "오후 12:00:00", "987. 1. 5."),
];

#[test]
fn a_date_with_no_options_is_written_as_node_writes_it() {
  let mut failed = Vec::new();
  for &(tag, date, both, time, early) in NODE {
    let written = numeric_date(tag, 2024, 9, 10);
    if written != date {
      failed.push(format!("{tag} date: got {written}, want {date}"));
    }
    let joined = date_with_time(tag, date, time);
    let both = NOT_IN_CLDR.iter().find(|(t, _)| *t == tag).map_or(both, |(_, cldr)| *cldr);
    if joined != both {
      failed.push(format!("{tag} date and time: got {joined}, want {both}"));
    }
    let written = numeric_date(tag, 987, 1, 5);
    if written != early {
      failed.push(format!("{tag} early date: got {written}, want {early}"));
    }
  }
  assert!(failed.is_empty(), "{}", failed.join("\n"));
}
