//! Next's own modules that FSR has a counterpart for, rewritten wherever an
//! application imports them: `next/link` and `next/font/google`.

use std::collections::BTreeMap;
use std::ops::Range;

use swc_core::common::{Span, Spanned};
use swc_core::ecma::ast as js;
use swc_core::ecma::visit::{Visit, VisitWith};

use crate::edit::Edits;
use crate::items::line_range;
use crate::syntax::{is_script, parse, Parsed};
use crate::{Pass, Report, Tree};

pub(crate) fn passes() -> Vec<Box<dyn Pass>> {
  vec![Box::new(Links), Box::new(Fonts)]
}

fn scripts(tree: &Tree) -> Vec<String> {
  tree.paths().filter(|p| p.starts_with("app/") && is_script(p)).map(str::to_owned).collect()
}

/// The default import of `module` in `parsed`: the declaration and the
/// local name.
fn default_import<'a>(parsed: &'a Parsed, module: &str) -> Option<(&'a js::ImportDecl, String)> {
  parsed.module.body.iter().find_map(|item| match item {
    js::ModuleItem::ModuleDecl(js::ModuleDecl::Import(import)) if import.src.value.to_string_lossy() == module => import.specifiers.iter().find_map(|s| match s {
      js::ImportSpecifier::Default(d) => Some((import, d.local.sym.to_string())),
      _ => None,
    }),
    _ => None,
  })
}

/// `next/link`'s `Link` as FSR's, from `@snapfire/fsr/components`.
/// `prefetch={false}` is `prefetch="none"` and a `prefetch` that asks for
/// the default is dropped; `passHref` is dropped, since FSR's `Link` is the
/// anchor. `replace`, `scroll`, `shallow`, `locale` and `legacyBehavior`
/// are dropped with a note each.
struct Links;

impl Pass for Links {
  fn name(&self) -> &'static str {
    "next-link"
  }

  fn run(&self, tree: &mut Tree, report: &mut Report) {
    for path in scripts(tree) {
      let Some(text) = tree.text(&path).map(str::to_owned) else { continue };
      if !text.contains("next/link") {
        continue;
      }
      let Ok(parsed) = parse(&path, &text) else { continue };
      let Some((import, local)) = default_import(&parsed, "next/link") else { continue };
      let mut edits = Edits::default();
      let clause = if local == "Link" { "{ Link }".to_owned() } else { format!("{{ Link as {local} }}") };
      edits.replace(parsed.range(import.span), format!("import {clause} from \"@snapfire/fsr/components\";"));
      let mut found = Elements { name: local, found: Vec::new() };
      parsed.module.visit_with(&mut found);
      for opening in &found.found {
        for attr in &opening.attrs {
          let js::JSXAttrOrSpread::JSXAttr(attr) = attr else { continue };
          let js::JSXAttrName::Ident(name) = &attr.name else { continue };
          let line = parsed.line(attr.span);
          let range = with_space_before(&text, parsed.range(attr.span));
          match &*name.sym {
            "prefetch" => match attr.value.as_ref().map(|v| bool_value(v)) {
              Some(Some(false)) => edits.replace(parsed.range(attr.span), "prefetch=\"none\""),
              None | Some(Some(true)) => edits.replace(range, ""),
              Some(None) => report.note(&path, line, "`prefetch` is not a literal; FSR's `prefetch` takes \"hover\", \"viewport\" or \"none\""),
            },
            "passHref" => edits.replace(range, ""),
            "replace" | "scroll" | "shallow" | "locale" => {
              edits.replace(range, "");
              report.note(&path, line, format!("`{}` was dropped: FSR's `Link` has no such prop; `navigate(href, true, {{ replace, scroll }})` from `@snapfire/fsr-client` takes `replace` and `scroll`", name.sym));
            }
            "legacyBehavior" => {
              edits.replace(range, "");
              report.note(&path, line, "`legacyBehavior` was dropped: FSR's `Link` is the anchor, so an `<a>` inside it is one anchor too many");
            }
            "href" => {
              if let Some(js::JSXAttrValue::JSXExprContainer(js::JSXExprContainer { expr: js::JSXExpr::Expr(e), .. })) = &attr.value {
                if matches!(&**e, js::Expr::Object(_)) {
                  report.note(&path, line, "an `href` object is a string in FSR: the path and its query written out");
                }
              }
            }
            _ => {}
          }
        }
      }
      tree.insert(path, edits.apply(&text));
    }
  }
}

fn bool_value(value: &js::JSXAttrValue) -> Option<bool> {
  match value {
    js::JSXAttrValue::JSXExprContainer(js::JSXExprContainer { expr: js::JSXExpr::Expr(e), .. }) => match &**e {
      js::Expr::Lit(js::Lit::Bool(b)) => Some(b.value),
      js::Expr::Lit(js::Lit::Null(_)) => Some(true),
      _ => None,
    },
    _ => None,
  }
}

fn with_space_before(text: &str, range: Range<usize>) -> Range<usize> {
  let mut start = range.start;
  while start > 0 && matches!(text.as_bytes()[start - 1], b' ' | b'\t') {
    start -= 1;
  }
  start..range.end
}

struct Elements {
  name: String,
  found: Vec<js::JSXOpeningElement>,
}

impl Visit for Elements {
  fn visit_jsx_opening_element(&mut self, n: &js::JSXOpeningElement) {
    if let js::JSXElementName::Ident(i) = &n.name {
      if *i.sym == *self.name {
        self.found.push(n.clone());
      }
    }
    n.visit_children_with(self);
  }
}

/// A `next/font/google` loader called at module level, `const inter =
/// Inter({ ... })`, as a font FSR links from Google: a `[fonts.<key>]`
/// table naming the family with `remote`, `inter.className` the class
/// `font-<key>` that `app/styles/fonts.css` points at `--font-<key>`,
/// `inter.variable` nothing, since the variable is on `:root`, and
/// `inter.style` the same family as an inline style.
struct Fonts;

impl Pass for Fonts {
  fn name(&self) -> &'static str {
    "next-font"
  }

  fn run(&self, tree: &mut Tree, report: &mut Report) {
    let mut families: BTreeMap<String, (String, Vec<String>)> = BTreeMap::new();
    for path in scripts(tree) {
      let Some(text) = tree.text(&path).map(str::to_owned) else { continue };
      if !text.contains("next/font") {
        continue;
      }
      let Ok(parsed) = parse(&path, &text) else { continue };
      let mut edits = Edits::default();
      let mut loaders: BTreeMap<String, String> = BTreeMap::new();
      for item in &parsed.module.body {
        let js::ModuleItem::ModuleDecl(js::ModuleDecl::Import(import)) = item else { continue };
        let module = import.src.value.to_string_lossy();
        if module == "next/font/local" {
          report.note(&path, parsed.line(import.span), "a local font is `[fonts.<key>]` with `files` in `config/app.toml`, the files under `app/fonts/`");
          continue;
        }
        if module != "next/font/google" {
          continue;
        }
        for s in &import.specifiers {
          if let js::ImportSpecifier::Named(n) = s {
            let imported = match &n.imported {
              Some(js::ModuleExportName::Ident(i)) => i.sym.to_string(),
              Some(js::ModuleExportName::Str(s)) => s.value.to_string_lossy().into_owned(),
              None => n.local.sym.to_string(),
            };
            loaders.insert(n.local.sym.to_string(), imported);
          }
        }
        edits.replace(line_range(&text, parsed.range(import.span)), "");
      }
      if loaders.is_empty() {
        continue;
      }
      let mut fonts: BTreeMap<String, String> = BTreeMap::new();
      for item in &parsed.module.body {
        let decl = match item {
          js::ModuleItem::Stmt(js::Stmt::Decl(js::Decl::Var(v))) => v,
          js::ModuleItem::ModuleDecl(js::ModuleDecl::ExportDecl(js::ExportDecl { decl: js::Decl::Var(v), .. })) => v,
          _ => continue,
        };
        let [d] = decl.decls.as_slice() else { continue };
        let (js::Pat::Ident(name), Some(js::Expr::Call(call))) = (&d.name, d.init.as_deref()) else { continue };
        let js::Callee::Expr(callee) = &call.callee else { continue };
        let js::Expr::Ident(callee) = &**callee else { continue };
        let Some(family) = loaders.get(&*callee.sym) else { continue };
        let family = family.replace('_', " ");
        let key = family.to_lowercase().replace(' ', "-");
        let weights = call.args.first().map(|a| weights(&a.expr)).unwrap_or_default();
        let entry = families.entry(key.clone()).or_insert_with(|| (family.clone(), Vec::new()));
        for w in weights {
          if !entry.1.contains(&w) {
            entry.1.push(w);
          }
        }
        fonts.insert(name.id.sym.to_string(), key);
        edits.replace(line_range(&text, parsed.range(item.span())), "");
      }
      let mut uses = Uses { fonts: &fonts, found: Vec::new(), other: Vec::new() };
      parsed.module.visit_with(&mut uses);
      for (span, key, field) in uses.found {
        let with = match field.as_str() {
          "className" => format!("\"font-{key}\""),
          "variable" => "\"\"".to_owned(),
          "style" => format!("{{ fontFamily: \"var(--font-{key})\" }}"),
          _ => {
            report.note(&path, parsed.line(span), format!("`.{field}` of a Next font has no counterpart; the family is `var(--font-{key})`"));
            continue;
          }
        };
        edits.replace(parsed.range(span), with);
      }
      for span in uses.other {
        report.note(&path, parsed.line(span), "a Next font object is read whole; the family is `var(--font-<key>)` in CSS");
      }
      tree.insert(path, crate::items::tidy(&edits.apply(&text)));
    }
    if families.is_empty() {
      return;
    }
    let mut css = String::new();
    for (key, (family, weights)) in &families {
      let weights = if weights.is_empty() { vec!["400".to_owned(), "700".to_owned()] } else { weights.clone() };
      let query = format!("{}:wght@{}", family.replace(' ', "+"), weights.join(";"));
      report.config.push(format!("[fonts.{key}]\nfamily = \"{family}\"\nremote = \"https://fonts.googleapis.com/css2?family={query}&display=swap\"\n"));
      report.note("app/styles/fonts.css", 0, format!("{family} is linked from Google; `fsr fonts add app google:{}@{}` serves its faces from the application instead, with `remote` taken out of `[fonts.{key}]`", family.replace(' ', "+"), weights.join(",")));
      css.push_str(&format!(".font-{key} {{\n  font-family: var(--font-{key});\n}}\n"));
    }
    tree.insert("app/styles/fonts.css", css);
  }
}

/// The weights a font loader's options name, `weight: "400"` or
/// `weight: ["400", "700"]`.
fn weights(options: &js::Expr) -> Vec<String> {
  let js::Expr::Object(object) = options else { return Vec::new() };
  let mut out = Vec::new();
  for prop in &object.props {
    let js::PropOrSpread::Prop(prop) = prop else { continue };
    let js::Prop::KeyValue(kv) = &**prop else { continue };
    let js::PropName::Ident(k) = &kv.key else { continue };
    if &*k.sym != "weight" {
      continue;
    }
    match &*kv.value {
      js::Expr::Lit(js::Lit::Str(s)) => out.push(s.value.to_string_lossy().into_owned()),
      js::Expr::Array(a) => {
        for e in a.elems.iter().flatten() {
          if let js::Expr::Lit(js::Lit::Str(s)) = &*e.expr {
            out.push(s.value.to_string_lossy().into_owned());
          }
        }
      }
      _ => {}
    }
  }
  out.retain(|w| w.chars().all(|c| c.is_ascii_digit()));
  out
}

struct Uses<'a> {
  fonts: &'a BTreeMap<String, String>,
  found: Vec<(Span, String, String)>,
  other: Vec<Span>,
}

impl Visit for Uses<'_> {
  fn visit_member_expr(&mut self, n: &js::MemberExpr) {
    if let (js::Expr::Ident(obj), js::MemberProp::Ident(prop)) = (&*n.obj, &n.prop) {
      if let Some(key) = self.fonts.get(&*obj.sym) {
        self.found.push((n.span, key.clone(), prop.sym.to_string()));
        return;
      }
    }
    n.visit_children_with(self);
  }

  fn visit_var_declarator(&mut self, n: &js::VarDeclarator) {
    if let Some(init) = &n.init {
      init.visit_with(self);
    }
  }

  fn visit_ident(&mut self, n: &js::Ident) {
    if self.fonts.contains_key(&*n.sym) {
      self.other.push(n.span);
    }
  }
}

#[cfg(test)]
mod tests {
  use crate::{migrate, Tree};

  fn tree(files: &[(&str, &str)]) -> Tree {
    let mut t = Tree::new();
    for (p, s) in files {
      t.insert(*p, *s);
    }
    t
  }

  const PACKAGE: &str = r#"{ "dependencies": { "next": "latest" } }"#;

  #[test]
  fn next_link_is_fsrs_link_and_its_props_follow() {
    let input = tree(&[
      ("package.json", PACKAGE),
      ("app/page.tsx", "import NextLink from \"next/link\";\n\nexport default function Page() {\n  return <NextLink href=\"/a\" prefetch={false} passHref replace>a</NextLink>;\n}\n"),
    ]);
    let (out, report) = migrate(&input);
    assert_eq!(
      out.text("app/routes/page.tsx").unwrap(),
      "import { Link as NextLink } from \"@snapfire/fsr/components\";\n\nexport default function Page() {\n  return <NextLink href=\"/a\" prefetch=\"none\">a</NextLink>;\n}\n"
    );
    assert_eq!(report.notes.len(), 1, "{:?}", report.notes);
  }

  #[test]
  fn a_google_font_is_linked_and_its_class_names_the_family() {
    let input = tree(&[
      ("package.json", PACKAGE),
      (
        "app/layout.tsx",
        "import { Inter, Roboto_Mono } from \"next/font/google\";\n\nconst inter = Inter({ subsets: [\"latin\"] });\nconst mono = Roboto_Mono({ weight: [\"400\", \"700\"], subsets: [\"latin\"] });\n\nexport default function Layout({ children }) {\n  return <div className={`${inter.className} ${mono.variable}`}>{children}</div>;\n}\n",
      ),
    ]);
    let (out, report) = migrate(&input);
    assert_eq!(out.text("app/routes/layout.tsx").unwrap(), "export default function Layout({ children }) {\n  return <div className={`${\"font-inter\"} ${\"\"}`}>{children}</div>;\n}\n");
    assert_eq!(out.text("app/styles/fonts.css").unwrap(), ".font-inter {\n  font-family: var(--font-inter);\n}\n.font-roboto-mono {\n  font-family: var(--font-roboto-mono);\n}\n");
    assert_eq!(
      report.config,
      [
        "[fonts.inter]\nfamily = \"Inter\"\nremote = \"https://fonts.googleapis.com/css2?family=Inter:wght@400;700&display=swap\"\n",
        "[fonts.roboto-mono]\nfamily = \"Roboto Mono\"\nremote = \"https://fonts.googleapis.com/css2?family=Roboto+Mono:wght@400;700&display=swap\"\n"
      ]
    );
  }
}
