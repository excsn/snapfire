//! `fsr census`: every residue an application holds, counted by cause, where
//! `fsr build` stops at the first.

use std::path::PathBuf;

use snapfire_fsr_cli::{build, census, Options};

fn app(files: &[(&str, &str)]) -> PathBuf {
  let nanos = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
  let dir = std::env::temp_dir().join(format!("fsr-census-{}-{nanos}", std::process::id()));
  std::fs::create_dir_all(dir.join("routes")).unwrap();
  std::fs::create_dir_all(dir.join("vendor")).unwrap();
  std::fs::write(dir.join("importmap.json"), r#"{"imports":{"@snapfire/fsr-client/react":"/r","react":"/r","react-dom/client":"/d"}}"#).unwrap();
  std::fs::write(dir.join("vendor/.fsr-vendor.json"), r#"{"packages":{"react":{"version":"18.3.1"},"react-dom":{"version":"18.3.1"}}}"#).unwrap();
  for (name, source) in files {
    let path = dir.join(name);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, source).unwrap();
  }
  dir
}

#[test]
fn a_census_counts_every_residue_by_cause_where_a_build_stops_at_the_first() {
  let dir = app(&[
    ("routes/page.tsx", "export default function Page() {\n  return <p>home</p>;\n}\n"),
    ("routes/a/page.tsx", "export default function Page() {\n  return <p>a</p>;\n}\n"),
    ("routes/a/page.loader.ts", "export async function load({ params }) {\n  let total = 0;\n  for (let i = 0; i < 3; i++) total += i;\n  return { total };\n}\n"),
    ("routes/b/page.tsx", "export default function Page() {\n  return <p>b</p>;\n}\n"),
    ("routes/b/page.loader.ts", "export async function load({ params }) {\n  let sum = 0;\n  for (let j = 0; j < 9; j++) sum += j;\n  return { sum };\n}\n"),
  ]);
  assert!(build(&dir, &Options::default()).is_err(), "a build stops at the residue");

  let census = census::run(&[dir.clone()]);
  assert!(census.failed.is_empty(), "{census}");
  let counted: Vec<(&str, usize)> = census.refused.iter().map(|(cause, seen)| (cause.as_str(), seen.len())).collect();
  assert_eq!(counted.len(), 1, "one construct under two names is one cause: {census}");
  assert_eq!(counted[0].1, 2, "{census}");
  let mut at: Vec<&str> = census.refused.values().next().unwrap().iter().map(|s| s.at.as_str()).collect();
  at.sort();
  assert!(at[0].starts_with("routes/a/page.loader.ts:3:") && at[1].starts_with("routes/b/page.loader.ts:3:"), "{at:?}");
  std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn a_census_of_applications_that_lower_says_so() {
  let dir = app(&[("routes/page.tsx", "export default function Page() {\n  return <p>home</p>;\n}\n")]);
  let census = census::run(&[dir.clone()]);
  assert!(census.refused.is_empty() && census.client.is_empty() && census.foreign.is_empty() && census.failed.is_empty(), "{census}");
  assert!(census.to_string().contains("every module lowered"), "{census}");
  std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn a_cause_counts_one_construct_under_any_name() {
  assert_eq!(census::cause_of("`total` is not bound here"), "`…` is not bound here");
  assert_eq!(census::cause_of("`a` as a whole; read `b`"), "`…` as a whole; read `…`");
  assert_eq!(census::cause_of("no names"), "no names");
  assert_eq!(census::cause_of("an open `tick"), "an open `…`");
}
