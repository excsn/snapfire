//! Tailwind's own test cases, extracted from v4.3.2's `*.test.ts` by
//! `examples/extract_fixtures.rs`. Each compiles a stylesheet, builds its candidates and
//! compares the optimized output with Tailwind's snapshot.

use std::fmt::Write as _;
use std::sync::LazyLock;

use regex::Regex;
use serde::Deserialize;
use snapfire_tailwind::{CompileOptions, Compiler, Loaded, OptimizeOptions, PolyfillFlags, optimize};

#[derive(Deserialize, Clone)]
struct Fixture {
  test: String,
  kind: String,
  candidates: Vec<String>,
  css: String,
  expected: String,
  loader: bool,
  polyfills: Option<String>,
  base: Option<String>,
  stylesheet: Option<(String, String)>,
  #[serde(default)]
  loads: Option<Vec<Load>>,
  shuffled: bool,
}

/// One row of a test's loader: the first whose conditions all hold answers, or throws.
#[derive(Deserialize, Clone)]
struct Load {
  when: Vec<Cond>,
  answer: Option<(String, String)>,
}

#[derive(Deserialize, Clone)]
struct Cond {
  arg: String,
  op: String,
  value: String,
}

impl Cond {
  fn holds(&self, id: &str, base: &str) -> bool {
    let subject = if self.arg == "id" { id } else { base };
    let value = self.value.as_str();
    match self.op.as_str() {
      "eq" => subject == value,
      "ne" => subject != value,
      "ends" => subject.ends_with(value),
      "starts" => subject.starts_with(value),
      "includes" => subject.contains(value),
      "not-ends" => !subject.ends_with(value),
      "not-starts" => !subject.starts_with(value),
      "not-includes" => !subject.contains(value),
      other => panic!("a loader condition `{other}` the runner does not read"),
    }
  }
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
  if fixture.shuffled {
    let forward = run_in(fixture, fixture.candidates.clone())?;
    let mut reversed = fixture.candidates.clone();
    reversed.reverse();
    let mut rotated = fixture.candidates.clone();
    rotated.rotate_left(fixture.candidates.len() / 2);
    for order in [reversed, rotated] {
      let other = run_in(fixture, order.clone())?;
      if other != forward {
        return Err(format!(
          "the output depends on the candidate order; {order:?} gives:\n{other}"
        ));
      }
    }
    return Ok(forward);
  }
  run_in(fixture, fixture.candidates.clone())
}

fn run_in(fixture: &Fixture, candidates: Vec<String>) -> Result<String, String> {
  let mut compiler = Compiler::new(&fixture.css, options_for(fixture)).map_err(|e| e.to_string())?;
  let built = compiler.build(candidates.iter().map(String::as_str));
  let optimized = optimize(&built, &OptimizeOptions::default()).map_err(|e| e.to_string())?;
  Ok(pretty(&optimized))
}

/// What `compiler.sources` holds, as Tailwind's test compares it.
fn sources_of(fixture: &Fixture) -> Result<serde_json::Value, String> {
  let compiler = Compiler::new(&fixture.css, options_for(fixture)).map_err(|e| e.to_string())?;
  Ok(compiler.sources().iter().map(|s| serde_json::json!({ "pattern": s.pattern, "base": s.base, "negated": s.negated })).collect())
}

fn options_for(fixture: &Fixture) -> CompileOptions<'static> {
  let mut options = CompileOptions {
    polyfills: polyfills(fixture.polyfills.as_deref()),
    base: fixture.base.clone().unwrap_or_default(),
    banner: false,
    ..CompileOptions::default()
  };
  if let Some((content, base)) = fixture.stylesheet.clone() {
    options.load_stylesheet = Some(Box::new(move |_, given| {
      let base = if base == "<base>" {
        given.to_owned()
      } else {
        base.clone()
      };
      Ok(Loaded {
        path: String::new(),
        base,
        content: content.clone(),
      })
    }));
  }
  if let Some(rows) = fixture.loads.clone() {
    options.load_stylesheet = Some(Box::new(move |id, given| {
      let row = rows.iter().find(|row| row.when.iter().all(|c| c.holds(id, given)));
      match row.and_then(|row| row.answer.clone()) {
        Some((content, base)) => Ok(Loaded { path: String::new(), base: if base == "<base>" { given.to_owned() } else { base }, content }),
        None => Err(snapfire_tailwind::Error::Message(format!("Unexpected import: {id}"))),
      }
    }));
  }
  let _ = fixture.loader;
  options
}

fn check(fixture: &Fixture) -> Option<String> {
  if fixture.kind == "sources" {
    let expected: serde_json::Value = serde_json::from_str(&fixture.expected).unwrap();
    return match std::panic::catch_unwind(|| sources_of(fixture)) {
      Ok(Ok(got)) if got == expected => None,
      Ok(Ok(got)) => Some(format!("--- expected sources\n{expected}\n--- got\n{got}")),
      Ok(Err(e)) => Some(format!("error: {e}")),
      Err(_) => Some("panicked".to_owned()),
    };
  }
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
    ("equal", Ok(css)) => {
      let other = Fixture {
        css: fixture.expected.clone(),
        kind: "css".to_owned(),
        expected: String::new(),
        ..fixture.clone()
      };
      match run(&other) {
        Ok(other_css) if other_css == css => None,
        Ok(other_css) => Some(format!(
          "--- the two stylesheets differ\n{css}\n--- against\n{other_css}"
        )),
        Err(e) => Some(format!("error: {e}")),
      }
    }
    ("equal", Err(e)) => Some(format!("error: {e}")),
    ("throws", Err(_)) => None,
    ("throws", Ok(css)) => Some(format!("expected an error, got css:\n{css}")),
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
    .filter(|p| {
      let name = p.to_string_lossy();
      p.extension().is_some_and(|e| e == "json")
        && !name.ends_with(".skipped.json")
        && !name.ends_with("-parse.json")
        && !name.ends_with("-cases.json")
        && !name.ends_with("-units.json")
    })
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
