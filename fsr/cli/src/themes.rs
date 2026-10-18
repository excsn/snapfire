//! Named themes. Each `styles/themes/<name>.css` overrides tokens the root theme defines, written
//! as `@theme` blocks, a `:root` rule or bare custom properties. The build writes each one for the
//! bundle as plain CSS under `[data-sf-theme="<name>"]` in the `theme.named` layer, and the root's
//! values for every token a theme sets under `[data-sf-theme="default"]`. The root's tokens come
//! from `@theme` blocks with Tailwind's theme imported into `theme.root`, or from `:root` rules in
//! `@layer theme.root` without it.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use crate::css::{parse, Node};
use crate::{BuildError, ROOT_THEME, THEMES_DIR, THEME_ATTRIBUTE};

/// The cascade layers the root theme and the named themes sit in, in order. A rule an application
/// writes in `@layer theme` itself, such as a scheme's tokens, outranks both.
pub const LAYER_ORDER: &str = "@layer theme.root, theme.named;";

/// The tokens a root theme defines, each with its value: the last one an `@theme` block that is not
/// `reference` or a `:root` rule gives, else a `reference` block's.
#[derive(Debug, Clone, Default)]
pub struct RootTokens {
  values: BTreeMap<String, (String, bool)>,
  /// Whether the root theme sits in `theme.root`: Tailwind's theme imported there, or a `:root`
  /// rule in `@layer theme.root`.
  sublayer: bool,
}

impl RootTokens {
  pub fn names(&self) -> Vec<String> {
    self.values.keys().cloned().collect()
  }

  pub fn contains(&self, name: &str) -> bool {
    self.values.contains_key(name)
  }

  fn add(&mut self, name: String, value: String, reference: bool) {
    match self.values.get(&name) {
      Some((_, false)) if reference => {}
      _ => {
        self.values.insert(name, (value, reference));
      }
    }
  }

  fn value(&self, name: &str) -> Option<&str> {
    self.values.get(name).map(|(value, _)| value.as_str())
  }
}

/// One named theme: its name, its file relative to the app and the tokens it sets, in order.
#[derive(Debug, Clone)]
pub struct NamedTheme {
  pub name: String,
  pub file: String,
  pub tokens: Vec<(String, String)>,
}

/// What the build writes for an app's named themes.
#[derive(Debug, Default)]
pub struct Written {
  /// Each overlay file, relative to the app, with its content.
  pub files: Vec<(String, String)>,
  /// Overlay files no source shadows, which the bundle's config names itself.
  pub added: Vec<String>,
  pub warnings: Vec<String>,
}

/// The root tokens the top-level stylesheets under `styles` define, through the stylesheets they
/// import, Tailwind's own among them.
pub fn root_tokens(styles: &Path) -> Result<RootTokens, BuildError> {
  let mut tokens = RootTokens::default();
  let mut sheets: Vec<PathBuf> = std::fs::read_dir(styles).into_iter().flatten().flatten().map(|e| e.path()).filter(|p| p.is_file() && p.extension().is_some_and(|e| e == "css")).collect();
  sheets.sort();
  let mut seen = BTreeSet::new();
  for sheet in sheets {
    let css = std::fs::read_to_string(&sheet).map_err(|e| BuildError::Io(sheet.clone(), e))?;
    let base = Base::Dir(sheet.parent().unwrap_or(styles).to_path_buf());
    read_root(&css, &base, &mut tokens, &mut seen)?;
  }
  Ok(tokens)
}

enum Base {
  Tailwind(String),
  Dir(PathBuf),
}

fn read_root(css: &str, base: &Base, tokens: &mut RootTokens, seen: &mut BTreeSet<String>) -> Result<(), BuildError> {
  read_nodes(&parse(css), base, false, tokens, seen)
}

fn read_nodes(nodes: &[Node], base: &Base, in_root: bool, tokens: &mut RootTokens, seen: &mut BTreeSet<String>) -> Result<(), BuildError> {
  for node in nodes {
    match node {
      Node::At { name, params, nodes } if name == "@import" && nodes.is_empty() => {
        let Some(id) = specifier(params) else { continue };
        let into_root = params.contains("layer(theme.root)");
        if into_root && matches!(id.as_str(), "tailwindcss/theme" | "tailwindcss/theme.css") {
          tokens.sublayer = true;
        }
        let tailwind_base = match base {
          Base::Tailwind(b) => b.as_str(),
          Base::Dir(_) => "",
        };
        if let Some(loaded) = snapfire_tailwind::tailwind_stylesheet(&id, tailwind_base) {
          if seen.insert(loaded.path.clone()) {
            read_nodes(&parse(&loaded.content), &Base::Tailwind(loaded.base.clone()), in_root || into_root, tokens, seen)?;
          }
          continue;
        }
        let Base::Dir(dir) = base else { continue };
        if !(id.starts_with("./") || id.starts_with("../")) {
          continue;
        }
        let path = dir.join(&id);
        if !path.is_file() || !seen.insert(path.display().to_string()) {
          continue;
        }
        let text = std::fs::read_to_string(&path).map_err(|e| BuildError::Io(path.clone(), e))?;
        read_nodes(&parse(&text), &Base::Dir(path.parent().unwrap_or(dir).to_path_buf()), in_root || into_root, tokens, seen)?;
      }
      Node::At { name, params, nodes } if name == "@theme" => {
        let reference = params.split_whitespace().any(|word| word == "reference");
        for (property, value) in custom_properties(nodes) {
          tokens.add(property, value, reference);
        }
      }
      Node::At { name, params, nodes } if name == "@layer" => read_nodes(nodes, base, in_root || params.trim() == "theme.root", tokens, seen)?,
      Node::Rule { selector, nodes } if in_root && is_root(selector) => {
        tokens.sublayer = true;
        for (property, value) in custom_properties(nodes) {
          tokens.add(property, value, false);
        }
      }
      _ => {}
    }
  }
  Ok(())
}

/// Whether a selector is `:root`, alone or with `:host`.
fn is_root(selector: &str) -> bool {
  selector.split(',').all(|s| matches!(s.trim(), ":root" | ":host")) && selector.contains(":root")
}

/// The custom properties among `nodes`, in order.
fn custom_properties(nodes: &[Node]) -> Vec<(String, String)> {
  nodes
    .iter()
    .filter_map(|node| match node {
      Node::Declaration { property, value } if property.starts_with("--") => Some((property.clone(), value.clone())),
      _ => None,
    })
    .collect()
}

/// The specifier an `@import`'s params name first, quoted or in `url()`.
fn specifier(params: &str) -> Option<String> {
  let rest = params.trim();
  let rest = rest.strip_prefix("url(").map(str::trim_start).unwrap_or(rest);
  let quote = rest.chars().next().filter(|c| *c == '"' || *c == '\'')?;
  let body = &rest[1..];
  body.find(quote).map(|end| body[..end].to_owned())
}

/// The tokens a theme file sets: its bare custom properties and those of its `@theme` blocks and
/// `:root` rules, in order.
fn theme_tokens(css: &str) -> Vec<(String, String)> {
  let mut out = Vec::new();
  for node in parse(css) {
    match node {
      Node::Declaration { property, value } if property.starts_with("--") => out.push((property, value)),
      Node::At { name, nodes, .. } if name == "@theme" => out.extend(custom_properties(&nodes)),
      Node::Rule { selector, nodes } if is_root(&selector) => out.extend(custom_properties(&nodes)),
      _ => {}
    }
  }
  out
}

/// The named themes under `styles/themes/`, by name.
pub fn named_themes(app: &Path, styles: &str) -> Result<Vec<NamedTheme>, BuildError> {
  let dir = app.join(styles).join(THEMES_DIR);
  let mut themes = Vec::new();
  for name in crate::theme_files(&dir)? {
    if name == ROOT_THEME {
      continue;
    }
    let file = format!("{styles}/{THEMES_DIR}/{name}.css");
    let path = app.join(&file);
    let css = std::fs::read_to_string(&path).map_err(|e| BuildError::Io(path.clone(), e))?;
    let tokens = theme_tokens(&css);
    themes.push(NamedTheme { name, file, tokens });
  }
  Ok(themes)
}

/// The overlay files for `themes`: one per theme and, for an app that owns its root theme, the
/// reset under `default`. A token neither `root` nor `shell` defines is refused.
pub fn write(styles: &str, themes: &[NamedTheme], root: &RootTokens, shell: Option<&[String]>) -> Result<Written, BuildError> {
  let mut written = Written::default();
  if themes.is_empty() {
    return Ok(written);
  }
  if !root.sublayer {
    return Err(BuildError::ThemeLayer { file: themes[0].file.clone() });
  }
  for theme in themes {
    for (token, _) in &theme.tokens {
      if !root.contains(token) && !shell.is_some_and(|names| names.iter().any(|n| n == token)) {
        let defined = match shell {
          Some([]) => "the site's root theme, and the shell's contract lists no tokens: build the shell first",
          Some(_) => "the site's root theme or its shell's",
          None => "the root theme",
        };
        return Err(BuildError::ThemeToken { file: theme.file.clone(), token: token.clone(), defined: defined.to_owned() });
      }
      if token.starts_with("--breakpoint-") || token.starts_with("--container-") {
        written.warnings.push(format!("{}: `{token}` is read when the stylesheet is built, so a theme that sets it changes no media or container query", theme.file));
      }
    }
    written.files.push((format!("{}/{}", crate::dev::BUNDLE_OVERLAY, theme.file), sheet(&theme.name, &theme.tokens)));
  }
  if shell.is_none() {
    let mut reset: BTreeMap<String, String> = BTreeMap::new();
    let mut pending: Vec<String> = themes.iter().flat_map(|t| t.tokens.iter().map(|(name, _)| name.clone())).collect();
    while let Some(name) = pending.pop() {
      if reset.contains_key(&name) {
        continue;
      }
      let Some(value) = root.value(&name) else { continue };
      pending.extend(variables(value).into_iter().filter(|v| root.contains(v)));
      reset.insert(name, value.to_owned());
    }
    let file = format!("{styles}/{THEMES_DIR}/{ROOT_THEME}.css");
    let tokens: Vec<(String, String)> = reset.into_iter().collect();
    written.files.push((format!("{}/{file}", crate::dev::BUNDLE_OVERLAY), sheet(ROOT_THEME, &tokens)));
    written.added.push(file);
  }
  Ok(written)
}

fn sheet(name: &str, tokens: &[(String, String)]) -> String {
  let mut out = format!("{LAYER_ORDER}\n\n@layer theme.named {{\n  [{THEME_ATTRIBUTE}=\"{name}\"] {{\n");
  for (token, value) in tokens {
    out.push_str(&format!("    {token}: {value};\n"));
  }
  out.push_str("  }\n}\n");
  out
}

/// The custom properties a value reads through `var()`.
fn variables(value: &str) -> Vec<String> {
  let mut out = Vec::new();
  let mut rest = value;
  while let Some(at) = rest.find("var(") {
    rest = &rest[at + 4..];
    let name: String = rest.trim_start().chars().take_while(|c| !matches!(c, ',' | ')') && !c.is_whitespace()).collect();
    if name.starts_with("--") {
      out.push(name);
    }
  }
  out
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn a_theme_file_sets_the_properties_of_each_form_and_skips_a_nested_block() {
    let css = "/* x; { */ --a: 1px;\n@theme default inline {\n  --font-sans: \"A; B\", serif;\n  @keyframes spin {\n    to { transform: rotate(1turn); }\n  }\n}\n:root, :host {\n  --shadow:\n    0 1px rgb(0 0 0 / 0.1);\n}\n.a { --not: 1; }\n";
    assert_eq!(theme_tokens(css), [("--a".to_owned(), "1px".to_owned()), ("--font-sans".to_owned(), "\"A; B\", serif".to_owned()), ("--shadow".to_owned(), "0 1px rgb(0 0 0 / 0.1)".to_owned())]);
    assert_eq!(specifier("\"tailwindcss/theme\" layer(theme.root)").as_deref(), Some("tailwindcss/theme"));
  }

  #[test]
  fn a_value_names_the_variables_it_reads() {
    assert_eq!(variables("color-mix(in oklab, var(--color-brand) 70%, var( --color-ink , #000))"), ["--color-brand", "--color-ink"]);
  }
}
