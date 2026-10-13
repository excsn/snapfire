//! Stylesheets that use Tailwind. Such a sheet is run through `snapfire_tailwind` with the
//! classes the driver listed before LightningCSS sees it, so the utilities it asks for are in the
//! output like any rule written by hand.

use std::cell::RefCell;
use std::path::{Path, PathBuf};

use snapfire_tailwind::{CompileOptions, Compiler, Error, Loaded, NamedTheme, tailwind_stylesheet};

/// What expanding one stylesheet produced. `reads` holds every file it read besides itself,
/// whether or not it succeeded, so a change to any of them compiles it again.
pub struct Expanded {
  pub css: Result<String, String>,
  pub reads: Vec<PathBuf>,
  pub warnings: Vec<String>,
  /// Each listed class that is not a utility and that the stylesheet does not define, with the
  /// modules using it.
  pub unknown: Vec<(String, String)>,
}

const AT_RULES: &[&str] = &["@tailwind", "@theme", "@apply", "@utility", "@custom-variant", "@variant", "@reference", "@source", "@plugin", "@config"];
const FUNCTIONS: &[&str] = &["--theme(", "--spacing(", "--alpha(", "theme("];

/// Whether `css` uses Tailwind: it imports `tailwindcss`, uses one of its at-rules or calls one
/// of its functions.
pub fn uses_tailwind(css: &str) -> bool {
  css.lines().any(|line| {
    let line = line.trim_start();
    (line.starts_with("@import") && line.contains("tailwindcss")) || AT_RULES.iter().any(|at| line.starts_with(at) && line[at.len()..].starts_with([' ', ';', '{', '('])) || FUNCTIONS.iter().any(|f| line.contains(f))
  })
}

/// The classes file a driver writes: one class per line, optionally followed by a tab and the
/// modules that use it, separated by spaces.
pub fn read_classes(path: &Path) -> Vec<(String, String)> {
  std::fs::read_to_string(path)
    .unwrap_or_default()
    .lines()
    .filter(|line| !line.trim().is_empty())
    .map(|line| match line.split_once('\t') {
      Some((class, modules)) => (class.to_owned(), modules.trim().to_owned()),
      None => (line.trim().to_owned(), String::new()),
    })
    .collect()
}

/// The named themes in `dir`, one per `<name>.css`, selected by `[<attribute>="<name>"]`, in
/// name order.
pub fn named_themes(dir: &Path, attribute: &str) -> Vec<(PathBuf, NamedTheme)> {
  let mut files: Vec<PathBuf> = std::fs::read_dir(dir).into_iter().flatten().flatten().map(|e| e.path()).filter(|p| p.extension().is_some_and(|e| e == "css")).collect();
  files.sort();
  files
    .into_iter()
    .filter_map(|file| {
      let name = file.file_stem()?.to_string_lossy().into_owned();
      let css = std::fs::read_to_string(&file).ok()?;
      let from = Some(file.to_string_lossy().into_owned());
      Some((file, NamedTheme { selector: format!("[{attribute}=\"{name}\"]"), css, from }))
    })
    .collect()
}

/// Runs `content`, the stylesheet at `path`, through Tailwind with the classes in `classes` and
/// the named themes in `themes`, a directory and the attribute that selects one.
/// An import resolves to one of Tailwind's own stylesheets first, then to a file relative to
/// the stylesheet that names it.
pub fn expand(path: &Path, content: &str, classes: Option<&Path>, themes: Option<(PathBuf, &str)>, source_name: &str) -> Expanded {
  let reads = RefCell::new(Vec::new());
  let listed = match classes {
    Some(file) => {
      reads.borrow_mut().push(file.to_path_buf());
      read_classes(file)
    }
    None => Vec::new(),
  };
  let dir = path.parent().unwrap_or_else(|| Path::new(".")).to_string_lossy().into_owned();
  let mut named = Vec::new();
  if let Some((themes_dir, attribute)) = themes {
    reads.borrow_mut().push(themes_dir.clone());
    for (file, theme) in named_themes(&themes_dir, attribute) {
      reads.borrow_mut().push(file);
      named.push(theme);
    }
  }

  let load = |id: &str, base: &str| -> Result<Loaded, Error> {
    if let Some(loaded) = tailwind_stylesheet(id, base) {
      return Ok(loaded);
    }
    let file = crate::graph::normalise(&Path::new(base).join(id));
    reads.borrow_mut().push(file.clone());
    let content = std::fs::read_to_string(&file).map_err(|e| Error::Message(format!("Could not read `{id}` from {base}: {e}")))?;
    Ok(Loaded { path: file.to_string_lossy().into_owned(), base: file.parent().unwrap_or(Path::new(".")).to_string_lossy().into_owned(), content })
  };

  let options = CompileOptions { base: dir, load_stylesheet: Some(Box::new(load)), from: Some(source_name.to_owned()), themes: named, ..CompileOptions::default() };
  let compiled = Compiler::new(content, options).map(|mut compiler| {
    let css = compiler.build(listed.iter().map(|(class, _)| class.as_str()));
    let invalid = compiler.invalid_candidates();
    let defined = defined_classes(&css);
    let unknown: Vec<(String, String)> = listed.iter().filter(|(class, _)| invalid.binary_search(class).is_ok() && !defined.contains(class.as_str())).cloned().collect();
    let warnings = compiler.warnings().into_iter().map(|w| format!("{source_name}: {w}")).collect();
    (css, warnings, unknown)
  });

  let reads = reads.into_inner();
  match compiled {
    Ok((css, warnings, unknown)) => Expanded { css: Ok(css), reads, warnings, unknown },
    Err(e) => Expanded { css: Err(e.to_string()), reads, warnings: Vec::new(), unknown: Vec::new() },
  }
}

/// The class names a stylesheet's selectors name, so a class an author wrote a rule for is not
/// mistaken for a misspelt utility. Declarations and strings are skipped.
pub fn defined_classes(css: &str) -> std::collections::BTreeSet<&str> {
  let mut defined = std::collections::BTreeSet::new();
  let bytes = css.as_bytes();
  let mut in_declaration = false;
  let mut i = 0;
  while i < bytes.len() {
    match bytes[i] {
      b'{' | b';' | b'}' => in_declaration = declaration_follows(&bytes[i + 1..]),
      b'"' | b'\'' => {
        let quote = bytes[i];
        i += 1;
        while i < bytes.len() && bytes[i] != quote {
          i += if bytes[i] == b'\\' { 2 } else { 1 };
        }
      }
      b'/' if bytes.get(i + 1) == Some(&b'*') => {
        i = css[i + 2..].find("*/").map_or(bytes.len(), |end| i + 2 + end + 1);
      }
      b'.' if !in_declaration && bytes.get(i + 1).is_some_and(|b| b.is_ascii_alphabetic() || *b == b'_' || *b == b'-') => {
        let start = i + 1;
        let mut end = start;
        while end < bytes.len() && (bytes[end].is_ascii_alphanumeric() || bytes[end] == b'-' || bytes[end] == b'_') {
          end += 1;
        }
        defined.insert(&css[start..end]);
        i = end;
        continue;
      }
      _ => {}
    }
    i += 1;
  }
  defined
}

/// Whether the text after a `{` starts with a declaration rather than a nested rule, so a
/// number like `.5` in a value is not read as a class.
fn declaration_follows(rest: &[u8]) -> bool {
  let text = String::from_utf8_lossy(&rest[..rest.len().min(200)]);
  let head = text.trim_start();
  let until = head.find([';', '{', '}']).unwrap_or(head.len());
  let first = &head[..until];
  first.contains(':') && !head[until..].starts_with('{')
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn recognises_a_tailwind_stylesheet() {
    assert!(uses_tailwind("@import \"tailwindcss\";\n"));
    assert!(uses_tailwind("@import 'tailwindcss/theme.css' layer(theme);"));
    assert!(uses_tailwind(".a {\n  @apply flex;\n}"));
    assert!(uses_tailwind("@theme {\n  --color-brand: red;\n}"));
    assert!(uses_tailwind(".a { margin: --spacing(4); }"));
    assert!(!uses_tailwind(":root {\n  --bg: #fff;\n}\n@media (width > 40rem) {\n  .a { color: red; }\n}"));
    assert!(!uses_tailwind("@import \"./base.css\";\n.themed { color: var(--theme-ink); }"));
  }

  #[test]
  fn reads_the_classes_selectors_name() {
    let css = ".a, .b-c:hover > .d_e { margin: .5rem; background: url(x.png); }\n@media (width > 1px) {\n  .f { color: red; }\n}\n/* .g */\n.h { content: \".i\"; &.j { color: red; } }";
    let defined: Vec<&str> = defined_classes(css).into_iter().collect();
    assert_eq!(defined, ["a", "b-c", "d_e", "f", "h", "j"]);
  }

  #[test]
  fn expands_the_listed_classes_and_names_the_unknown_ones() {
    let dir = std::env::temp_dir().join(format!("snapfirec-tailwind-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let classes = dir.join("classes");
    std::fs::write(&classes, "flex\troutes/page.tsx#default\nbg-brand\troutes/page.tsx#default\nflx\troutes/layout.tsx#default\nhero\troutes/page.tsx#default\n").unwrap();
    std::fs::write(dir.join("brand.css"), "@theme {\n  --color-brand: #123456;\n}\n").unwrap();
    let sheet = dir.join("app.css");
    let content = "@import \"tailwindcss/utilities\";\n@import \"./brand.css\";\n.hero {\n  margin: 0 .5rem;\n}\n";

    let expanded = expand(&sheet, content, Some(&classes), None, "styles/app.css");
    let css = expanded.css.unwrap();
    assert!(css.contains(".flex {\n  display: flex;\n}"), "{css}");
    assert!(css.contains("background-color: var(--color-brand)"), "{css}");
    assert!(expanded.warnings.is_empty(), "{:?}", expanded.warnings);
    assert_eq!(expanded.unknown, [("flx".to_owned(), "routes/layout.tsx#default".to_owned())]);
    assert_eq!(expanded.reads, [classes, dir.join("brand.css")]);
    std::fs::remove_dir_all(&dir).unwrap();
  }
}
