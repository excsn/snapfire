//! A `Date` written in this machine's zone and locale, each value as node v26.8.1 prints it with `TZ=America/New_York` and the locale variables each case sets. One test, since the zone is read once per process.

use snapfire_fsr_core::{Params, Value};
use snapfire_fsr_ir::Interpreter;
use snapfire_fsr_lower::lower_loader;
use snapfire_fsr_runtime::RequestCtx;

fn answer(expr: &str) -> String {
  let source = format!("export async function load() {{\n  const v = {expr};\n  return {{ v: JSON.stringify(v) }};\n}}\n");
  let body = lower_loader("l.ts", &source).unwrap_or_else(|e| panic!("{expr}: {e}"));
  let ctx = RequestCtx::anonymous(Params::new());
  match futures::executor::block_on(Interpreter::default().run(&body, &ctx, None)).map(|outcome| outcome.value) {
    Ok(Value::Map(fields)) => match fields.get("v") {
      Some(Value::Str(text)) => text.to_string(),
      other => panic!("{expr}: {other:?}"),
    },
    other => panic!("{expr}: {other:?}"),
  }
}

const EN: &[(&str, &str)] = &[
  ("new Date(1726011440123).toTimeString()", "\"19:37:20 GMT-0400 (Eastern Daylight Time)\""),
  ("new Date(1726011440123).toString()", "\"Tue Sep 10 2024 19:37:20 GMT-0400 (Eastern Daylight Time)\""),
  ("String(new Date(1726011440123))", "\"Tue Sep 10 2024 19:37:20 GMT-0400 (Eastern Daylight Time)\""),
  ("`${new Date(1726011440123)}`", "\"Tue Sep 10 2024 19:37:20 GMT-0400 (Eastern Daylight Time)\""),
  ("\"\" + new Date(1726011440123)", "\"Tue Sep 10 2024 19:37:20 GMT-0400 (Eastern Daylight Time)\""),
  ("new Date(0).toTimeString()", "\"19:00:00 GMT-0500 (Eastern Standard Time)\""),
  ("new Date(NaN).toTimeString()", "\"Invalid Date\""),
  ("new Date(NaN).toString()", "\"Invalid Date\""),
  ("new Date(-62198755200000).toString()", "\"Thu Dec 31 -0002 19:03:58 GMT-0456 (Eastern Standard Time)\""),
  ("new Date(-2208988800000).toString()", "\"Sun Dec 31 1899 19:00:00 GMT-0500 (Eastern Standard Time)\""),
  ("new Date(8.64e15).toString()", "\"Fri Sep 12 275760 20:00:00 GMT-0400 (Eastern Daylight Time)\""),
  ("new Date(1726011440123).toLocaleDateString()", "\"9/10/2024\""),
  ("new Date(1726011440123).toLocaleString()", "\"9/10/2024, 7:37:20 PM\""),
  ("new Date(1726011440123).toLocaleString(\"de-DE\")", "\"10.9.2024, 19:37:20\""),
  ("new Date(Date.UTC(2024, 0, 1, 2)).toLocaleDateString(\"en-US\", { dateStyle: \"medium\" })", "\"Dec 31, 2023\""),
  ("new Date(Date.UTC(2024, 0, 1, 2)).toLocaleTimeString(\"en-US\", { timeStyle: \"short\" })", "\"9:00 PM\""),
  ("new Intl.DateTimeFormat(\"en-US\").format(new Date(Date.UTC(2024, 0, 1, 2)))", "\"12/31/2023\""),
  ("new Date(NaN).toLocaleString()", "\"Invalid Date\""),
  ("new Date(1726011440123).toLocaleString(\"pt-BR\")", "\"10/09/2024, 19:37:20\""),
  ("new Date(1726011440123).toLocaleString(\"fa\")", "\"۱۴۰۳/۶/۲۰, ۱۹:۳۷:۲۰\""),
  ("new Date(1726011440123).toLocaleDateString(\"ps\")", "\"AP ۱۴۰۳-۰۶-۲۰\""),
  ("new Date(1726011440123).toLocaleString(\"th\")", "\"10/9/2567 19:37:20\""),
  ("new Date(1726011440123).toLocaleTimeString(\"ckb\")", "\"٧:٣٧:٢٠ د.ن\""),
  ("new Date(1726011440123).toLocaleString(\"en-u-ca-persian\")", "\"6/20/1403 AP, 7:37:20 PM\""),
  ("new Date(1726011440123).toLocaleDateString(\"ja-JP-u-ca-japanese\")", "\"R6/9/10\""),
  ("new Date(1726011440123).toLocaleTimeString(\"zh-Hant\")", "\"下午7:37:20\""),
];

#[test]
fn a_date_is_written_in_this_machines_zone_and_locale() {
  // SAFETY: this target holds one test, so no other thread reads the environment.
  unsafe {
    std::env::set_var("TZ", "America/New_York");
    std::env::remove_var("LC_ALL");
    std::env::remove_var("LC_MESSAGES");
    std::env::remove_var("LC_TIME");
    std::env::set_var("LANG", "en_US.UTF-8");
  }
  let mut failed: Vec<String> = EN.iter().filter_map(|(expr, want)| (answer(expr) != *want).then(|| format!("{expr}\n    got {}\n    want {want}", answer(expr)))).collect();

  unsafe { std::env::set_var("LANG", "de_DE.UTF-8") };
  let mut check = |expr: &str, want: &str| {
    let got = answer(expr);
    if got != want {
      failed.push(format!("{expr} under {:?}\n    got {got}\n    want {want}", std::env::vars().filter(|(k, _)| k.starts_with("LC_") || k == "LANG").collect::<Vec<_>>()));
    }
  };
  check("new Date(1726011440123).toString()", "\"Tue Sep 10 2024 19:37:20 GMT-0400 (Nordamerikanische Ostküsten-Sommerzeit)\"");

  unsafe {
    std::env::set_var("LANG", "en_US.UTF-8");
    std::env::set_var("LC_MESSAGES", "fr_FR.UTF-8");
    std::env::set_var("LC_TIME", "de_DE.UTF-8");
  }
  check("[new Date(1726011440123).toTimeString(), new Date(1726011440123).toLocaleTimeString()]", "[\"19:37:20 GMT-0400 (heure d’été de l’Est nord-américain)\",\"19:37:20\"]");

  unsafe {
    std::env::remove_var("LC_MESSAGES");
    std::env::set_var("LANG", "de_DE.UTF-8");
    std::env::set_var("LC_TIME", "en_US.UTF-8");
  }
  check("new Date(1726011440123).toLocaleTimeString()", "\"19:37:20\"");

  unsafe { std::env::set_var("LC_ALL", "ja_JP.UTF-8") };
  check("new Date(1726011440123).toTimeString()", "\"19:37:20 GMT-0400 (米国東部夏時間)\"");
  assert!(failed.is_empty(), "{} failed:\n{}", failed.len(), failed.join("\n"));
}
