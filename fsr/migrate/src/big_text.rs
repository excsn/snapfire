//! `ink-big-text`'s `font` named as the cfonts font file the application now ships under `app/fonts/`, copied from the cfonts package the input installed. FSR's big text draws the file it is given and ships no font.

use std::collections::BTreeSet;

use swc_core::ecma::ast as js;
use swc_core::ecma::visit::{Visit, VisitWith};

use crate::edit::Edits;
use crate::syntax::{is_script, parse, Parsed};
use crate::{Pass, Report, Tree};

/// Where the input tree holds the font files of the cfonts package the application installed, `<name>.json` each; `fsr migrate` reads them from `node_modules/cfonts/fonts/`.
pub const CFONTS_DIR: &str = ".cfonts/";

const FONTS_DIR: &str = "app/fonts/";

/// The font `ink-big-text` draws when none is named.
const DEFAULT_FONT: &str = "block";

/// The module that maps every copied font's name to its file, for a `font` chosen as the page runs.
const FONT_TABLE: &str = "cfonts";

pub(crate) struct BigText;

impl Pass for BigText {
  fn name(&self) -> &'static str {
    "ink-big-text fonts"
  }

  fn run(&self, tree: &mut Tree, report: &mut Report) {
    let installed: Vec<(String, Vec<u8>)> = tree.iter().filter_map(|(path, bytes)| Some((path.strip_prefix(CFONTS_DIR)?.strip_suffix(".json")?.to_owned(), bytes.to_vec()))).collect();
    let mut used: BTreeSet<String> = BTreeSet::new();
    let mut table = false;
    let paths: Vec<String> = tree.paths().filter(|p| p.starts_with("app/") && is_script(p)).map(str::to_owned).collect();
    for path in paths {
      let Some(text) = tree.text(&path).map(str::to_owned) else { continue };
      if !text.contains("ink-big-text") {
        continue;
      }
      let Ok(parsed) = parse(&path, &text) else { continue };
      let Some((local, last_import)) = imported(&parsed) else { continue };
      let mut uses = Uses { parsed: &parsed, text: &text, local: &local, path: &path, edits: Edits::default(), fonts: BTreeSet::new(), table: false, report };
      parsed.module.visit_with(&mut uses);
      let Uses { mut edits, fonts, table: chosen, .. } = uses;
      if fonts.is_empty() && !chosen {
        continue;
      }
      let up = up_to_app(&path);
      let mut imports: Vec<String> = fonts.iter().map(|name| format!("\nimport {} from \"{up}fonts/{name}.json\";", ident(name))).collect();
      if chosen {
        imports.push(format!("\nimport {FONT_TABLE}Fonts from \"{up}fonts/{FONT_TABLE}\";"));
      }
      edits.replace(last_import..last_import, imports.concat());
      tree.insert(path, edits.apply(&text));
      used.extend(fonts);
      table |= chosen;
    }
    if table {
      used.extend(installed.iter().map(|(name, _)| name.clone()));
      let imports: Vec<String> = installed.iter().map(|(name, _)| format!("import {} from \"./{name}.json\";\n", ident(name))).collect();
      let entries: Vec<String> = installed.iter().map(|(name, _)| format!("\"{name}\": {}", ident(name))).collect();
      tree.insert(format!("{FONTS_DIR}{FONT_TABLE}.ts"), format!("{}\nconst fonts: Record<string, unknown> = {{ {} }};\n\nexport default fonts;\n", imports.concat(), entries.join(", ")));
    }
    for name in &used {
      let file = format!("{FONTS_DIR}{name}.json");
      match installed.iter().find(|(held, _)| held == name) {
        Some((_, bytes)) => {
          tree.insert(file.clone(), bytes.clone());
          report.note(&file, 0, format!("cfonts' `{name}` font, copied from node_modules/cfonts; the application ships it under cfonts' licence, GPL-3.0-or-later"));
        }
        None if !tree.contains(&file) => report.note(&file, 0, format!("cfonts' `{name}` font is not installed; copy node_modules/cfonts/fonts/{name}.json here")),
        None => {}
      }
    }
    let carried: Vec<String> = tree.paths().filter(|p| p.starts_with(CFONTS_DIR)).map(str::to_owned).collect();
    for path in carried {
      tree.remove(&path);
    }
  }
}

/// The name `ink-big-text` is imported under by default and the end of the file's last import.
fn imported(parsed: &Parsed) -> Option<(String, usize)> {
  let mut local = None;
  let mut last = None;
  for item in &parsed.module.body {
    let js::ModuleItem::ModuleDecl(js::ModuleDecl::Import(import)) = item else { continue };
    last = Some(parsed.range(import.span).end);
    if import.src.value.to_string_lossy() == "ink-big-text" {
      if let Some(js::ImportSpecifier::Default(d)) = import.specifiers.first() {
        local = Some(d.local.sym.to_string());
      }
    }
  }
  Some((local?, last?))
}

/// `../` once for each directory between `path` and `app/`, or `./` for a file directly in it.
fn up_to_app(path: &str) -> String {
  let depth = path.strip_prefix("app/").unwrap_or(path).matches('/').count();
  if depth == 0 { "./".to_owned() } else { "../".repeat(depth) }
}

/// The name a font file is imported under: `fontBlock`, `fontSimple3d`, `font3d`.
fn ident(name: &str) -> String {
  let mut out = String::from("font");
  let mut upper = true;
  for c in name.chars() {
    if !c.is_ascii_alphanumeric() {
      upper = true;
      continue;
    }
    out.push(if upper { c.to_ascii_uppercase() } else { c });
    upper = false;
  }
  out
}

struct Uses<'a> {
  parsed: &'a Parsed,
  text: &'a str,
  local: &'a str,
  path: &'a str,
  edits: Edits,
  /// The fonts named by string.
  fonts: BTreeSet<String>,
  /// Whether a `font` is chosen as the page runs.
  table: bool,
  report: &'a mut Report,
}

impl Visit for Uses<'_> {
  fn visit_jsx_opening_element(&mut self, n: &js::JSXOpeningElement) {
    n.visit_children_with(self);
    let js::JSXElementName::Ident(name) = &n.name else { return };
    if *name.sym != *self.local {
      return;
    }
    let font = n.attrs.iter().find_map(|attr| match attr {
      js::JSXAttrOrSpread::JSXAttr(a) if matches!(&a.name, js::JSXAttrName::Ident(i) if &*i.sym == "font") => Some(a),
      _ => None,
    });
    let Some(font) = font else {
      if n.attrs.iter().any(|attr| matches!(attr, js::JSXAttrOrSpread::SpreadElement(_))) {
        self.report.note(self.path, self.parsed.line(n.span), format!("`<{}>` spreads its props, so a `font` among them is still a name; pass the font file's JSON", self.local));
      }
      let at = self.parsed.range(name.span).end;
      self.edits.replace(at..at, format!(" font={{{}}}", ident(DEFAULT_FONT)));
      self.fonts.insert(DEFAULT_FONT.to_owned());
      return;
    };
    let named = match &font.value {
      Some(js::JSXAttrValue::Str(s)) => Some(s.value.to_string_lossy().into_owned()),
      Some(js::JSXAttrValue::JSXExprContainer(js::JSXExprContainer { expr: js::JSXExpr::Expr(e), .. })) => match &**e {
        js::Expr::Lit(js::Lit::Str(s)) => Some(s.value.to_string_lossy().into_owned()),
        _ => None,
      },
      _ => return,
    };
    let Some(value) = &font.value else { return };
    let value = match value {
      js::JSXAttrValue::Str(s) => self.parsed.range(s.span),
      js::JSXAttrValue::JSXExprContainer(c) => self.parsed.range(c.span),
      _ => return,
    };
    match named {
      Some(named) => {
        self.edits.replace(value, format!("{{{}}}", ident(&named)));
        self.fonts.insert(named);
      }
      None => {
        let js::JSXAttrValue::JSXExprContainer(js::JSXExprContainer { expr: js::JSXExpr::Expr(e), .. }) = font.value.as_ref().expect("a value") else { return };
        let written = &self.text[self.parsed.range(swc_core::common::Spanned::span(&**e))];
        self.edits.replace(value, format!("{{{FONT_TABLE}Fonts[{written}]}}"));
        self.table = true;
      }
    }
  }
}
