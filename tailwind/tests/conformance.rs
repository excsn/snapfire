//! Tailwind's own test cases, extracted from v4.3.2's `*.test.ts` by
//! `examples/extract_fixtures.rs`. Each compiles a stylesheet, builds its candidates and
//! compares the optimized output with Tailwind's snapshot.

use std::fmt::Write as _;
use std::sync::LazyLock;

use regex::Regex;
use serde::Deserialize;
use snapfire_tailwind::{CompileOptions, Compiler, OptimizeOptions, PolyfillFlags, optimize};

#[derive(Deserialize)]
struct Fixture {
  test: String,
  kind: String,
  candidates: Vec<String>,
  css: String,
  expected: String,
  loader: bool,
  polyfills: Option<String>,
}

static HIGH_PRECISION: LazyLock<Regex> = LazyLock::new(|| {
  Regex::new(r"(oklab\(|oklch\(|lab\(|lch\()[0-9]{1,2}\.?[0-9]{0,4}% -?[0-9]{0,2}\.([0-9]{4,8}) -?[0-9]{0,2}.([0-9]{4,8}) / \.[0-9]{1,2}\)").unwrap()
});

/// The snapshot serializer's rounding of `oklab()` arguments, applied to both sides.
fn reduce_precision(input: &str) -> String {
  HIGH_PRECISION
    .replace_all(input, |caps: &regex::Captures| {
      let whole = caps[0].to_owned();
      let first = &caps[2];
      let second = &caps[3];
      whole.replace(first, &first[..3]).replace(second, &second[..3])
    })
    .into_owned()
}

fn pretty(input: &str) -> String {
  let trimmed = input.trim();
  if trimmed.is_empty() {
    String::new()
  } else {
    format!("\n{trimmed}\n")
  }
}

fn polyfills(name: Option<&str>) -> PolyfillFlags {
  match name {
    Some("Polyfills.None") => PolyfillFlags::NONE,
    Some("Polyfills.AtProperty") => PolyfillFlags::AT_PROPERTY,
    Some("Polyfills.ColorMix") => PolyfillFlags::COLOR_MIX,
    _ => PolyfillFlags::ALL,
  }
}

fn run(fixture: &Fixture) -> Result<String, String> {
  let options = CompileOptions {
    polyfills: polyfills(fixture.polyfills.as_deref()),
    banner: false,
    ..CompileOptions::default()
  };
  let _ = fixture.loader;
  let mut compiler = Compiler::new(&fixture.css, options).map_err(|e| e.to_string())?;
  let built = compiler.build(fixture.candidates.iter().map(String::as_str));
  let optimized = optimize(&built, &OptimizeOptions::default()).map_err(|e| e.to_string())?;
  Ok(pretty(&optimized))
}

fn check(fixture: &Fixture) -> Option<String> {
  let result = std::panic::catch_unwind(|| run(fixture));
  let result = match result {
    Ok(r) => r,
    Err(_) => return Some("panicked".to_owned()),
  };
  match (fixture.kind.as_str(), result) {
    ("css", Ok(css)) => {
      let got = reduce_precision(&css);
      let want = reduce_precision(&fixture.expected);
      if got == want {
        None
      } else {
        Some(format!("--- expected\n{want}\n--- got\n{got}"))
      }
    }
    ("css", Err(e)) => Some(format!("error: {e}")),
    ("error", Err(e)) => {
      if e == fixture.expected {
        None
      } else {
        Some(format!("--- expected error\n{}\n--- got error\n{e}", fixture.expected))
      }
    }
    ("error", Ok(css)) => Some(format!("expected error {}, got css:\n{css}", fixture.expected)),
    _ => Some("unknown kind".to_owned()),
  }
}

/// Cases whose snapshot Lightning CSS 1.0.0-alpha.71 (the npm 1.32.0 Tailwind tests with)
/// produces and alpha.72 prints differently: `calc(infinity * 1px)` becomes `2147483647px`
/// rather than `3.40282e38px`. An unknown at-rule nested in a style rule stays nested.
/// Running the same two passes through alpha.71 on this crate's output gives the snapshot.
const LIGHTNINGCSS_VERSION: &[(&str, &str)] = &[
  ("rounded", "3.40282e38px"),
  ("@apply > should be usable with CSS mixins", "\n@apply --my-mixin-1;"),
];

fn lightningcss_version_difference(fixture: &Fixture) -> bool {
  LIGHTNINGCSS_VERSION
    .iter()
    .any(|(test, marker)| fixture.test == *test && fixture.expected.contains(marker))
}

#[test]
fn tailwind_test_suite() {
  let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures");
  let mut files: Vec<_> = std::fs::read_dir(&dir)
    .unwrap()
    .filter_map(|e| e.ok())
    .map(|e| e.path())
    .filter(|p| !p.to_string_lossy().ends_with(".skipped.json"))
    .collect();
  files.sort();

  let only = std::env::var("FIXTURE").ok();
  let mut report = String::new();
  let mut passed = 0;
  let mut failed = 0;
  let mut known = 0;

  for file in files {
    let fixtures: Vec<Fixture> = serde_json::from_str(&std::fs::read_to_string(&file).unwrap()).unwrap();
    let name = file.file_stem().unwrap().to_string_lossy().into_owned();
    let mut file_passed = 0;
    for fixture in &fixtures {
      if let Some(only) = &only
        && !fixture.test.contains(only.as_str())
      {
        continue;
      }
      match check(fixture) {
        None => file_passed += 1,
        Some(_) if lightningcss_version_difference(fixture) => {
          known += 1;
        }
        Some(diff) => {
          failed += 1;
          let _ = writeln!(
            report,
            "### {name}: {}\ncandidates: {:?}\n{diff}\n",
            fixture.test, fixture.candidates
          );
        }
      }
    }
    passed += file_passed;
    println!("{name}: {file_passed}/{}", fixtures.len());
  }

  let out = std::path::Path::new(env!("CARGO_TARGET_TMPDIR")).join("conformance-report.md");
  std::fs::write(&out, &report).unwrap();
  println!(
    "passed {passed}, failed {failed}, Lightning CSS version differences {known}; report at {}",
    out.display()
  );
  assert_eq!(failed, 0, "{failed} cases differ from Tailwind; see {}", out.display());
}
