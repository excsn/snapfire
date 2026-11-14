//! A host formats from the locale data its application's build wrote and refuses data cut for other locales. This target holds one test, since what a program installed is the whole program's.

use std::path::PathBuf;

use snapfire_fsr_host::locale::DATA_FILE;
use snapfire_fsr_host::Host;
use snapfire_fsr_ir::intl_data::{Cut, IntlData};
use snapfire_fsr_ir::locales::Unlisted;

const PLAN: &str = r#"(plan 2)
(route /
  (node 0 shell#document (slot content (node 1 routes/index/page.tsx#default))))
"#;

const CONFIG: &str = r#"
[app]
dir = "."

[server]
listen = "127.0.0.1:0"

[document]
title = "Locales"

[session]
key = "test-key"

[locales]
supported = ["fr_FR", "en"]
unlisted = "error"
"#;

fn app_dir() -> PathBuf {
  let nanos = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
  let dir = snapfire_fsr_test_internal::scratch_root().join(format!("fsr-locale-data-{}-{nanos}", std::process::id()));
  std::fs::create_dir_all(dir.join("generated")).unwrap();
  let plan = snapfire_fsr_plan::Manifest::from_text(PLAN).unwrap().to_sexpr();
  std::fs::write(dir.join("generated/plan.sexp"), plan).unwrap();
  std::fs::write(dir.join("app.toml"), CONFIG).unwrap();
  dir
}

fn refusal(dir: &std::path::Path) -> String {
  Host::from(dir.join("app.toml")).and_then(|builder| builder.build()).map(|_| ()).unwrap_err().to_string()
}

fn cut(locales: &[&str], default: &str, unlisted: Unlisted) -> Vec<u8> {
  IntlData::packed_from_compiled(&Cut::new(locales.iter().map(|tag| (*tag).to_owned()).collect(), default.to_owned(), unlisted))
}

#[test]
fn a_host_formats_from_the_data_cut_for_its_locales_and_refuses_any_other() {
  let dir = app_dir();
  let file = dir.join(DATA_FILE);

  std::fs::write(&file, b"not locale data").unwrap();
  let garbage = refusal(&dir);
  assert!(garbage.contains("locale.data") && garbage.contains("`fsr build` writes it again"), "{garbage}");

  std::fs::write(&file, cut(&["en"], "en", Unlisted::Fallback)).unwrap();
  let other = refusal(&dir);
  assert!(other.contains("was cut for the locales en (default en,") && other.contains("now calls for fr-FR, en (default fr-FR, unlisted = \"error\")"), "{other}");

  std::fs::write(&file, cut(&["fr-FR", "en"], "fr-FR", Unlisted::Fallback)).unwrap();
  let setting = refusal(&dir);
  assert!(setting.contains("was cut for the locales fr-FR, en (default fr-FR, unlisted = \"fallback\")"), "{setting}");
  assert!(IntlData::current().unwrap().cut().is_none(), "a refused file is not installed");

  std::fs::write(&file, cut(&["fr-FR", "en"], "fr-FR", Unlisted::Error)).unwrap();
  let host = Host::from(dir.join("app.toml")).unwrap().build().unwrap();
  let held = IntlData::current().unwrap().cut().cloned().unwrap();
  assert_eq!((held.locales, held.default, held.unlisted), (vec!["fr-FR".to_owned(), "en".to_owned()], "fr-FR".to_owned(), Unlisted::Error));
  drop(host);
  std::fs::remove_dir_all(&dir).unwrap();
}
