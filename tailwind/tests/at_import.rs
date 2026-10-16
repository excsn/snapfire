//! Tailwind v4.3.2's `at-import.test.ts`, ported by hand because each case brings its own
//! `loadStylesheet`. The two cases that load JavaScript configs and plugins through
//! `loadModule` are left out: this crate runs no JavaScript.

use snapfire_tailwind::{CompileOptions, Compiler, Error, Loaded, OptimizeOptions, Source, optimize};

fn pretty(input: &str) -> String {
  let trimmed = input.trim();
  if trimmed.is_empty() {
    String::new()
  } else {
    format!("\n{trimmed}\n")
  }
}

fn run<'a>(
  candidates: &[&str],
  css: &str,
  base: &str,
  load: impl Fn(&str, &str) -> Result<Loaded, Error> + 'a,
) -> Result<String, Error> {
  let options = CompileOptions::default().base(base.to_owned()).load_stylesheet(Some(Box::new(load))).banner(false);
  let mut compiler = Compiler::new(css, options)?;
  let built = compiler.build(candidates.iter().copied());
  Ok(pretty(&optimize(&built, &OptimizeOptions::default())?))
}

fn compile_css<'a>(
  css: &str,
  base: &str,
  load: impl Fn(&str, &str) -> Result<Loaded, Error> + 'a,
) -> Result<String, Error> {
  run(&[], css, base, load)
}

fn loaded(content: &str, base: &str) -> Result<Loaded, Error> {
  Ok(Loaded::new(String::new(), base.to_owned(), content.to_owned()))
}

fn example(id: &str, _: &str) -> Result<Loaded, Error> {
  if !id.ends_with("example.css") {
    return Err(Error::Message(format!("Unexpected import: {id}")));
  }
  loaded("a {\n  color: red;\n}", "/root")
}

fn unexpected(_: &str, _: &str) -> Result<Loaded, Error> {
  Err(Error::Message("Unexpected stylesheet".to_owned()))
}

const RED_A: &str = "\na {\n  color: red;\n}\n";

#[test]
fn can_resolve_relative_imports() {
  let out = compile_css("@import './foo/bar.css';", "/root", |id, base| {
    assert_eq!(base, "/root");
    assert_eq!(id, "./foo/bar.css");
    loaded(".foo {\n  color: red;\n}", "/root/foo")
  });
  assert_eq!(out.unwrap(), "\n.foo {\n  color: red;\n}\n");
}

#[test]
fn can_recursively_resolve_relative_imports() {
  let out = compile_css("@import './foo/bar.css';", "/root", |id, base| match (base, id) {
    ("/root", "./foo/bar.css") => loaded("@import './bar/baz.css';", "/root/foo"),
    ("/root/foo", "./bar/baz.css") => loaded(".baz {\n  color: blue;\n}", "/root/foo/bar"),
    _ => Err(Error::Message(format!("Unexpected import: {id}"))),
  });
  assert_eq!(out.unwrap(), "\n.baz {\n  color: #00f;\n}\n");
}

#[test]
fn extracts_path_from_import_nodes() {
  for import in [
    "@import 'example.css';",
    "@import './example.css';",
    "@import '/example.css';",
  ] {
    assert_eq!(compile_css(import, "/root", example).unwrap(), RED_A);
  }
}

#[test]
fn url_imports_are_passed_through() {
  for (import, expected) in [
    ("@import url('example.css');", "\n@import \"example.css\";\n"),
    ("@import url('./example.css');", "\n@import \"./example.css\";\n"),
    ("@import url('/example.css');", "\n@import \"/example.css\";\n"),
    ("@import url(example.css);", "\n@import \"example.css\";\n"),
    ("@import url(./example.css);", "\n@import \"./example.css\";\n"),
    ("@import url(/example.css);", "\n@import \"/example.css\";\n"),
  ] {
    assert_eq!(compile_css(import, "/root", unexpected).unwrap(), expected);
  }
}

#[test]
fn handles_case_insensitive_import_directive() {
  assert_eq!(compile_css("@import 'example.css';", "/root", example).unwrap(), RED_A);
}

#[test]
fn media() {
  for (import, expected) in [
    (
      "@import 'example.css' print;",
      "\n@media print {\n  a {\n    color: red;\n  }\n}\n",
    ),
    (
      "@import 'example.css' print, screen;",
      "\n@media print, screen {\n  a {\n    color: red;\n  }\n}\n",
    ),
    (
      "@import 'example.css' screen and (orientation: landscape);",
      "\n@media screen and (orientation: landscape) {\n  a {\n    color: red;\n  }\n}\n",
    ),
    (
      "@import 'example.css' foo(bar);",
      "\n@media foo(bar) {\n  a {\n    color: red;\n  }\n}\n",
    ),
  ] {
    assert_eq!(compile_css(import, "/root", example).unwrap(), expected, "{import}");
  }
}

#[test]
fn supports() {
  for (import, expected) in [
    (
      "@import 'example.css' supports(display: grid);",
      "\n@supports (display: grid) {\n  a {\n    color: red;\n  }\n}\n",
    ),
    (
      "@import 'example.css' supports(display: grid) screen and (max-width: 400px);",
      "\n@supports (display: grid) {\n  @media screen and (max-width: 400px) {\n    a {\n      color: red;\n    }\n  }\n}\n",
    ),
    (
      "@import 'example.css' supports((not (display: grid)) and (display: flex)) screen and\n  (max-width: 400px);",
      "\n@supports (not (display: grid)) and (display: flex) {\n  @media screen and (max-width: 400px) {\n    a {\n      color: red;\n    }\n  }\n}\n",
    ),
    (
      "@import 'example.css'\nsupports((selector(h2 > p)) and (font-tech(color-COLRv1)));",
      "\n@supports selector(h2 > p) and font-tech(color-COLRv1) {\n  a {\n    color: red;\n  }\n}\n",
    ),
  ] {
    assert_eq!(compile_css(import, "/root", example).unwrap(), expected, "{import}");
  }
}

#[test]
fn layer() {
  assert_eq!(
    compile_css("@import 'example.css' layer(utilities);", "/root", example).unwrap(),
    "\n@layer utilities {\n  a {\n    color: red;\n  }\n}\n"
  );
  assert_eq!(
    compile_css("@import 'example.css' layer();", "/root", example).unwrap(),
    "\n@layer {\n  a {\n    color: red;\n  }\n}\n"
  );
}

#[test]
fn supports_theme_reference_imports() {
  let out = run(
    &["text-red-500"],
    "@tailwind utilities;\n@import 'example.css' theme(reference);",
    "/root",
    |_, _| loaded("@theme {\n  --color-red-500: red;\n}", ""),
  );
  assert_eq!(
    out.unwrap(),
    "\n.text-red-500 {\n  color: var(--color-red-500, red);\n}\n"
  );
}

#[test]
fn emits_the_right_base_for_source_directives_inside_nested_files() {
  let options = CompileOptions::default().base("/root".to_owned()).load_stylesheet(Some(Box::new(|_, _| loaded("@source './nested/**/*.css';", "/root/foo")))).banner(false);
  let compiler = Compiler::new("@import './foo/bar.css';\n@source './root/**/*.css';", options).unwrap();
  assert_eq!(
    compiler.sources(),
    &[
      Source::new("/root/foo".to_owned(), "./nested/**/*.css".to_owned(), false),
      Source::new("/root".to_owned(), "./root/**/*.css".to_owned(), false),
    ]
  );
}

#[test]
fn it_crashes_when_inside_a_cycle() {
  let input = "@import 'foo.css';";
  let err = compile_css(input, "/root", |_, _| loaded(input, "/root")).unwrap_err();
  assert_eq!(
    err.to_string(),
    "Exceeded maximum recursion depth while resolving `foo.css` in `/root`)"
  );
}

#[test]
fn resolves_reference_as_import_reference() {
  let out = run(
    &["text-red-500"],
    "@reference './foo/bar.css';\n@tailwind utilities;",
    "/root",
    |id, base| {
      assert_eq!(base, "/root");
      assert_eq!(id, "./foo/bar.css");
      loaded(
        "@theme {\n  --color-red-500: red;\n}\n.foo {\n  color: red;\n}",
        "/root/foo",
      )
    },
  );
  assert_eq!(
    out.unwrap(),
    "\n.text-red-500 {\n  color: var(--color-red-500, red);\n}\n"
  );
}

#[test]
fn resolves_variant_used_as_custom_variant_inside_reference() {
  let out = run(
    &["dark:flex"],
    "@reference './foo/bar.css';\n@tailwind utilities;",
    "/root",
    |id, base| {
      assert_eq!(base, "/root");
      assert_eq!(id, "./foo/bar.css");
      loaded(
        "@variant dark {\n  &:where([data-theme='dark'] *) {\n    @slot;\n  }\n}",
        "/root/foo",
      )
    },
  );
  assert_eq!(
    out.unwrap(),
    "\n.dark\\:flex:where([data-theme=\"dark\"] *) {\n  display: flex;\n}\n"
  );
}
