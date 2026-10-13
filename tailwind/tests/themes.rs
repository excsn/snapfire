//! Named themes: stylesheets of `@theme` blocks written under their own selector after the root
//! theme.

use snapfire_tailwind::{CompileOptions, Compiler, NamedTheme, OptimizeOptions, optimize};

fn build(css: &str, themes: Vec<NamedTheme>, candidates: &[&str]) -> (String, Vec<String>) {
  let options = CompileOptions { banner: false, themes, ..CompileOptions::default() };
  let mut compiler = Compiler::new(css, options).unwrap();
  let built = compiler.build(candidates.iter().copied());
  (optimize(&built, &OptimizeOptions::default()).unwrap(), compiler.warnings())
}

fn named(name: &str, css: &str) -> NamedTheme {
  NamedTheme { selector: format!("[data-theme=\"{name}\"]"), css: css.to_owned(), from: Some(format!("themes/{name}.css")) }
}

#[test]
fn a_named_theme_follows_the_root_theme_in_its_layer() {
  let (css, warnings) = build(
    "@import \"tailwindcss/theme.css\" layer(theme);\n@import \"tailwindcss/utilities.css\" layer(utilities);\n@theme {\n  --color-accent: #1f6feb;\n}",
    vec![named("ocean", "@theme {\n  --color-accent: #0e7490;\n  --color-canvas: #ecfeff;\n}")],
    &["text-accent"],
  );
  assert!(warnings.is_empty(), "{warnings:?}");
  let root = css.find("--color-accent: #1f6feb").expect(&css);
  let ocean = css.find("[data-theme=\"ocean\"]").expect(&css);
  assert!(root < ocean, "{css}");
  assert!(css.contains("[data-theme=\"ocean\"] {\n    --color-accent: #0e7490;\n    --color-canvas: #ecfeff;\n  }"), "every variable is written, used or not: {css}");
  assert!(css.find("@layer utilities").unwrap() > ocean, "{css}");
}

#[test]
fn a_token_only_a_named_theme_defines_gets_its_utility() {
  let (css, _) = build("@import \"tailwindcss/utilities.css\";\n@theme {\n  --color-ink: #111;\n}", vec![named("brand", "@theme {\n  --color-brand: #e11d48;\n}")], &["bg-brand", "text-ink"]);
  assert!(css.contains(".bg-brand {\n  background-color: var(--color-brand, #e11d48);\n}"), "{css}");
  assert!(!css.contains(":root, :host {\n  --color-ink: #111;\n  --color-brand"), "the root does not take the named theme's value: {css}");
}

#[test]
fn a_breakpoint_in_a_named_theme_is_a_warning() {
  let (_, warnings) = build("@import \"tailwindcss/utilities.css\";\n@theme {\n  --color-ink: #111;\n}", vec![named("wide", "@theme {\n  --breakpoint-md: 60rem;\n}")], &[]);
  assert_eq!(warnings, ["themes/wide.css: `--breakpoint-md` is read when the stylesheet is built, so a theme that sets it changes no media or container query"]);
}
