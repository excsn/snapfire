//! Named themes: the names a build knows, one per file under `styles/themes/` and the root's own, and the plain stylesheets the build writes for them.

use snapfire_fsr_cli::{theme_files, BuildError};

fn dir(files: &[&str]) -> std::path::PathBuf {
  let root = std::env::temp_dir().join(format!("fsr-cli-themes-{}-{}", std::process::id(), files.join("-")));
  let _ = std::fs::remove_dir_all(&root);
  std::fs::create_dir_all(&root).unwrap();
  for file in files {
    std::fs::write(root.join(file), "@theme {\n  --color-brand: #0e7490;\n}\n").unwrap();
  }
  root
}

#[test]
fn the_root_theme_is_named_default_once_there_are_themes() {
  assert_eq!(theme_files(&dir(&["ocean.css", "forest.css"])).unwrap(), ["default", "forest", "ocean"]);
  assert!(theme_files(&dir(&[])).unwrap().is_empty());
}

#[test]
fn a_theme_file_named_default_is_refused() {
  assert!(matches!(theme_files(&dir(&["default.css"])), Err(BuildError::ReservedTheme { .. })));
}

static NEXT: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);

/// An app with React in its import map and `files` under it.
fn app(files: &[(&str, &str)]) -> std::path::PathBuf {
  let n = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
  let dir = std::env::temp_dir().join(format!("fsr-cli-named-themes-{}-{n}", std::process::id()));
  let _ = std::fs::remove_dir_all(&dir);
  std::fs::create_dir_all(dir.join("vendor")).unwrap();
  std::fs::write(dir.join("importmap.json"), r#"{"imports":{"@snapfire/fsr-client/react":"/r","react":"/r","react-dom/client":"/d"}}"#).unwrap();
  std::fs::write(dir.join("vendor/.fsr-vendor.json"), r#"{"packages":{"react":{"version":"18.3.1"},"react-dom":{"version":"18.3.1"}}}"#).unwrap();
  for (name, source) in [("routes/page.tsx", "export default function Page() {\n  return <p className=\"text-accent\">page</p>;\n}\n")].iter().chain(files) {
    let path = dir.join(name);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, source).unwrap();
  }
  dir
}

fn written<'a>(built: &'a snapfire_fsr_cli::Built, rel: &str) -> Option<&'a str> {
  built.files.iter().find(|(f, _)| f == rel).map(|(_, text)| text.as_str())
}

const ROOT: &str = "@import \"tailwindcss/theme\" layer(theme.root);\n@import \"tailwindcss/utilities\" layer(utilities);\n\n@theme {\n  --color-brand: #1f6feb;\n  --color-accent: var(--color-brand);\n  --color-ink: #111111;\n}\n";

#[test]
fn a_named_theme_is_written_as_plain_css_in_the_named_sublayer() {
  let dir = app(&[("styles/app.css", ROOT), ("styles/themes/ocean.css", "@theme {\n  --color-accent: #0e7490;\n  --color-blue-500: #0000ff;\n}\n")]);
  let built = snapfire_fsr_cli::build(&dir, &snapfire_fsr_cli::Options::default()).expect("builds");
  assert_eq!(
    written(&built, ".fsr-bundle/styles/themes/ocean.css"),
    Some("@layer theme.root, theme.named;\n\n@layer theme.named {\n  [data-sf-theme=\"ocean\"] {\n    --color-accent: #0e7490;\n    --color-blue-500: #0000ff;\n  }\n}\n")
  );
  std::fs::remove_dir_all(&dir).unwrap();
}

/// `--color-accent` reads `--color-brand`, so the reset declares both; `--color-blue-500` is
/// Tailwind's own.
#[test]
fn the_default_theme_resets_every_token_a_theme_sets_to_the_roots_value() {
  let dir = app(&[("styles/app.css", ROOT), ("styles/themes/ocean.css", "@theme {\n  --color-accent: #0e7490;\n  --color-blue-500: #0000ff;\n}\n"), ("styles/themes/forest.css", "@theme {\n  --color-ink: #052e16;\n}\n")]);
  let built = snapfire_fsr_cli::build(&dir, &snapfire_fsr_cli::Options::default()).expect("builds");
  assert_eq!(
    written(&built, ".fsr-bundle/styles/themes/default.css"),
    Some("@layer theme.root, theme.named;\n\n@layer theme.named {\n  [data-sf-theme=\"default\"] {\n    --color-accent: var(--color-brand);\n    --color-blue-500: oklch(62.3% 0.214 259.815);\n    --color-brand: #1f6feb;\n    --color-ink: #111111;\n  }\n}\n")
  );
  let config = written(&built, "tsconfig.build.json").expect("the bundle's config");
  assert!(config.contains("\"styles/themes/default.css\""), "the reset exists in the overlay alone, so the config names it: {config}");
  std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn a_theme_setting_a_token_the_root_does_not_define_is_refused() {
  let dir = app(&[("styles/app.css", ROOT), ("styles/themes/ocean.css", "@theme {\n  --color-wave: #0e7490;\n}\n")]);
  let err = snapfire_fsr_cli::build(&dir, &snapfire_fsr_cli::Options::default()).err().expect("refused").to_string();
  assert!(err.contains("styles/themes/ocean.css") && err.contains("--color-wave"), "{err}");
  std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn a_root_token_in_a_stylesheet_the_root_imports_counts() {
  let dir = app(&[
    ("styles/app.css", "@import \"tailwindcss/theme\" layer(theme.root);\n@import \"./tokens.css\";\n"),
    ("styles/tokens.css", "@theme {\n  --color-wave: #1f6feb;\n}\n"),
    ("styles/themes/ocean.css", "@theme {\n  --color-wave: #0e7490;\n}\n"),
  ]);
  let built = snapfire_fsr_cli::build(&dir, &snapfire_fsr_cli::Options::default()).expect("builds");
  assert!(written(&built, ".fsr-bundle/styles/themes/default.css").unwrap().contains("--color-wave: #1f6feb;"));
  std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn a_sites_theme_may_set_what_its_shells_root_defines_and_writes_no_reset() {
  let shell = app(&[("styles/app.css", ROOT), ("styles/themes/ocean.css", "@theme {\n  --color-accent: #0e7490;\n}\n")]);
  let built = snapfire_fsr_cli::build(&shell, &snapfire_fsr_cli::Options::default()).expect("the shell builds");
  std::fs::create_dir_all(shell.join("generated")).unwrap();
  std::fs::write(shell.join("generated/shell.json"), written(&built, "generated/shell.json").unwrap()).unwrap();
  let site_with = |theme: &str| app(&[("styles/site.css", "@import \"tailwindcss/theme\" layer(theme.root);\n\n@theme reference {\n  --color-ink: #111111;\n}\n"), ("styles/themes/editorial.css", theme)]);
  let mut options = snapfire_fsr_cli::Options::default();
  options.site = Some(snapfire_fsr_cli::SiteOptions::new("blog".to_owned(), "/blog".to_owned()).shell(Some(shell.join("generated/shell.json"))));
  let site = site_with("@theme {\n  --color-brand: #6d28d9;\n  --color-ink: #222222;\n}\n");
  let built = snapfire_fsr_cli::build(&site, &options).expect("the site builds");
  assert!(written(&built, ".fsr-bundle/styles/themes/editorial.css").unwrap().contains("[data-sf-theme=\"editorial\"] {\n    --color-brand: #6d28d9;\n    --color-ink: #222222;\n  }"));
  assert_eq!(written(&built, ".fsr-bundle/styles/themes/default.css"), None, "the shell owns the root theme and its reset");
  let refused = site_with("@theme {\n  --color-wave: #6d28d9;\n}\n");
  let err = snapfire_fsr_cli::build(&refused, &options).err().expect("refused").to_string();
  assert!(err.contains("--color-wave"), "{err}");
  for dir in [shell, site, refused] {
    std::fs::remove_dir_all(&dir).unwrap();
  }
}

/// With the root theme in `theme` itself it outranks every named theme, so none would apply.
#[test]
fn named_themes_with_the_root_theme_outside_its_sublayer_are_refused() {
  for root in ["@import \"tailwindcss/theme\" layer(theme);\n@theme {\n  --color-accent: #1f6feb;\n}\n", "@import \"tailwindcss\";\n@theme {\n  --color-accent: #1f6feb;\n}\n"] {
    let dir = app(&[("styles/app.css", root), ("styles/themes/ocean.css", "@theme {\n  --color-accent: #0e7490;\n}\n")]);
    let err = snapfire_fsr_cli::build(&dir, &snapfire_fsr_cli::Options::default()).err().expect("refused").to_string();
    assert!(err.contains("@import \"tailwindcss/theme\" layer(theme.root);"), "{err}");
    std::fs::remove_dir_all(&dir).unwrap();
  }
}

const PLAIN_ROOT: &str = "@layer theme.root {\n  :root {\n    --brand: #1f6feb;\n    --ink: #111111;\n  }\n}\n\nbody {\n  color: var(--ink);\n  background: var(--brand);\n}\n";

/// Theming is not Tailwind's: a stylesheet without it declares the root's tokens in `:root` in
/// `theme.root`, and a theme sets them in `:root`, in `@theme` or as bare declarations.
#[test]
fn themes_work_without_tailwind() {
  let dir = app(&[
    ("styles/app.css", PLAIN_ROOT),
    ("styles/themes/ocean.css", ":root {\n  --brand: #0e7490;\n}\n"),
    ("styles/themes/forest.css", "--ink: #052e16;\n"),
  ]);
  let built = snapfire_fsr_cli::build(&dir, &snapfire_fsr_cli::Options::default()).expect("builds");
  assert_eq!(written(&built, ".fsr-bundle/styles/themes/ocean.css"), Some("@layer theme.root, theme.named;\n\n@layer theme.named {\n  [data-sf-theme=\"ocean\"] {\n    --brand: #0e7490;\n  }\n}\n"));
  assert!(written(&built, ".fsr-bundle/styles/themes/forest.css").unwrap().contains("--ink: #052e16;"));
  assert!(written(&built, ".fsr-bundle/styles/themes/default.css").unwrap().contains("    --brand: #1f6feb;\n    --ink: #111111;\n"));
  std::fs::remove_dir_all(&dir).unwrap();
}

/// A `:root` outside `theme.root` is unlayered and outranks every theme.
#[test]
fn root_tokens_outside_the_root_sublayer_are_refused() {
  let dir = app(&[("styles/app.css", ":root {\n  --brand: #1f6feb;\n}\n"), ("styles/themes/ocean.css", ":root {\n  --brand: #0e7490;\n}\n")]);
  let err = snapfire_fsr_cli::build(&dir, &snapfire_fsr_cli::Options::default()).err().expect("refused").to_string();
  assert!(err.contains("@layer theme.root"), "{err}");
  std::fs::remove_dir_all(&dir).unwrap();
}
