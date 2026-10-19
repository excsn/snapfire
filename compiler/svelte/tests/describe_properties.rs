//! The describe answer over components that know where their scripts are,
//! every cut of the repository's own components and arbitrary text: the
//! script comes back byte for byte where it stands and nothing else does.

use proptest::prelude::*;
use snapfire_compiler_wire::{Options, Outcome, Severity};
use snapfire_svelte::Compiler;

thread_local! {
  static COMPILER: Compiler = Compiler::new().expect("the compiler boots");
}

fn describe(source: &str) -> Outcome {
  COMPILER.with(|compiler| compiler.describe("src/C.svelte", source, &Options::default()).expect("the driver answers"))
}

/// The blanking: every byte of a script body as written, every other byte a space or the newline the source has there.
fn assert_blanked(source: &str, content: &str, bodies: &[std::ops::Range<usize>]) {
  assert_eq!(content.len(), source.len(), "the content is the source's length in bytes:\n{source}");
  for body in bodies {
    assert_eq!(&content[body.clone()], &source[body.clone()], "a script body stands where it is written:\n{source}");
  }
  for (at, byte) in content.bytes().enumerate() {
    if bodies.iter().any(|body| body.contains(&at)) {
      continue;
    }
    let expected = if source.as_bytes()[at] == b'\n' { b'\n' } else { b' ' };
    assert_eq!(byte, expected, "byte {at} outside a script:\n{source}");
  }
}

fn text() -> impl Strategy<Value = String> {
  prop::sample::select(vec!["<p>plain</p>", "<p>é and ü</p>", "<p>🙂 two units</p>", "<h1>{title}</h1>", "{#if open}<b>x</b>{/if}", "<!-- <script>not one</script> -->", "<a href=\"/x\">link</a>", "\n\n", "<p>tab\there</p>", "<p>{'{'}</p>"]).prop_map(str::to_owned)
}

fn body(lines: Vec<&'static str>) -> impl Strategy<Value = String> {
  prop::sample::subsequence(lines.clone(), 0..=lines.len()).prop_map(|lines| format!("\n{}\n", lines.join("\n")))
}

fn module_body() -> impl Strategy<Value = String> {
  body(vec!["import { a } from \"./a.js\";", "const s = \"é <b> 🙂\";", "export const k = 2;", "/* block é */", "const t = `x ${1} y`;"])
}

fn instance_body() -> impl Strategy<Value = String> {
  prop_oneof![
    body(vec!["import B from '@src/b';", "let n = $state(1);", "// a comment with é", "let { title } = $props();", "const u = \"<p>\";"]),
    body(vec!["import B from '@src/b';", "let n = 1;", "// a comment with é", "export let title = 'é';", "const u = \"<p>\";", "$: double = n * 2;"]),
  ]
}

fn style() -> impl Strategy<Value = String> {
  prop::sample::select(vec!["", "<style>p { color: red; }</style>", "<style>\n  .é { margin: 0; }\n</style>"]).prop_map(str::to_owned)
}

/// A component and the byte ranges its script bodies occupy.
fn component() -> impl Strategy<Value = (String, Vec<std::ops::Range<usize>>)> {
  (proptest::option::of(module_body()), proptest::option::of((prop::bool::ANY, instance_body())), prop::collection::vec(text(), 0..4), style(), prop::collection::vec(text(), 0..3)).prop_map(|(module, instance, before, style, after)| {
    let mut source = String::new();
    let mut bodies = Vec::new();
    source.push_str(&before.join(""));
    if let Some(body) = module {
      source.push_str("<script module>");
      bodies.push(source.len()..source.len() + body.len());
      source.push_str(&body);
      source.push_str("</script>\n");
    }
    source.push_str(&after.join(""));
    if let Some((typed, body)) = instance {
      source.push_str(if typed { "<script lang=\"ts\">" } else { "<script>" });
      bodies.push(source.len()..source.len() + body.len());
      source.push_str(&body);
      source.push_str("</script>\n");
    }
    source.push_str(&style);
    source.push_str(&before.join(""));
    (source, bodies)
  })
}

proptest! {
  #![proptest_config(ProptestConfig::with_cases(256))]

  #[test]
  fn a_description_holds_each_script_where_it_stands_and_blanks_the_rest((source, bodies) in component()) {
    match describe(&source) {
      Outcome::Described(described) => {
        prop_assert_eq!(described.diagnostics.iter().filter(|d| d.severity == Severity::Error).count(), 0);
        match described.script {
          Some(script) => assert_blanked(&source, &script.content, &bodies),
          None => prop_assert!(bodies.is_empty(), "{}", source),
        }
      }
      Outcome::Failed { diagnostics } => prop_assert!(false, "refused {:?}:\n{}", diagnostics, source),
      other => prop_assert!(false, "{:?}", other),
    }
  }
}

fn components() -> Vec<(std::path::PathBuf, String)> {
  fn walk(dir: &std::path::Path, out: &mut Vec<(std::path::PathBuf, String)>) {
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    for path in entries.flatten().map(|e| e.path()) {
      let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
      if path.is_dir() {
        if !["target", "vendor", "node_modules", ".git", ".fsr", "dist"].contains(&name.as_str()) {
          walk(&path, out);
        }
      } else if name.ends_with(".svelte") {
        out.push((path.clone(), std::fs::read_to_string(&path).unwrap()));
      }
    }
  }
  let mut out = Vec::new();
  walk(&std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fsr/examples"), &mut out);
  out.sort();
  out
}

#[test]
fn every_cut_of_the_repositorys_components_is_described_or_refused_and_a_script_keeps_its_length() {
  let components = components();
  assert!(components.len() >= 5, "{components:?}");
  for (path, source) in &components {
    let Outcome::Described(whole) = describe(source) else { panic!("{} is not described", path.display()) };
    match whole.script {
      Some(script) => assert_eq!(script.content.len(), source.len(), "{}", path.display()),
      None => assert!(!source.contains("<script"), "{} has a script it does not describe", path.display()),
    }
    let step = (source.len() / 48).max(1);
    for (cut, _) in source.char_indices().step_by(step) {
      for piece in [&source[..cut], &source[cut..]] {
        match describe(piece) {
          Outcome::Described(described) => {
            if let Some(script) = described.script {
              assert_eq!(script.content.len(), piece.len(), "{} cut at {cut}", path.display());
            }
          }
          Outcome::Failed { diagnostics } => assert!(!diagnostics.is_empty()),
          other => panic!("{other:?}"),
        }
      }
    }
  }
}

#[test]
fn describing_arbitrary_text_answers_and_keeps_the_length() {
  bolero::check!().with_iterations(300).for_each(|bytes: &[u8]| {
    let source = String::from_utf8_lossy(bytes);
    match describe(&source) {
      Outcome::Described(described) => {
        if let Some(script) = described.script {
          assert_eq!(script.content.len(), source.len());
        }
      }
      Outcome::Failed { diagnostics } => assert!(!diagnostics.is_empty()),
      other => panic!("{other:?}"),
    }
  });
}
