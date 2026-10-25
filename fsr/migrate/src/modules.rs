//! Next's own modules that FSR has a counterpart for, rewritten wherever an
//! application imports them: `next/link`, `next/image` and `next/font/google`.

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
  vec![Box::new(Links), Box::new(CatchErrors), Box::new(Images), Box::new(Fonts), Box::new(crate::head::Head), Box::new(Stylesheets), Box::new(crate::actions::ServerActions)]
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

/// `next/link`'s `Link` as FSR's from `@snapfire/fsr/components` and its
/// `useLinkStatus` as the React adapter's.
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
      let Some(import) = parsed.module.body.iter().find_map(|item| match item {
        js::ModuleItem::ModuleDecl(js::ModuleDecl::Import(import)) if import.src.value.to_string_lossy() == "next/link" => Some(import),
        _ => None,
      }) else {
        continue;
      };
      let mut local = None;
      let mut status = None;
      for specifier in &import.specifiers {
        match specifier {
          js::ImportSpecifier::Default(d) => local = Some(d.local.sym.to_string()),
          js::ImportSpecifier::Named(n) => {
            let imported = match &n.imported {
              Some(js::ModuleExportName::Ident(i)) => i.sym.to_string(),
              Some(js::ModuleExportName::Str(s)) => s.value.to_string_lossy().into_owned(),
              None => n.local.sym.to_string(),
            };
            if imported == "useLinkStatus" {
              status = Some(n.local.sym.to_string());
            }
          }
          js::ImportSpecifier::Namespace(_) => {}
        }
      }
      let mut lines = Vec::new();
      if let Some(local) = &local {
        let clause = if local == "Link" { "{ Link }".to_owned() } else { format!("{{ Link as {local} }}") };
        lines.push(format!("import {clause} from \"@snapfire/fsr/components\";"));
      }
      if let Some(status) = &status {
        let clause = if status == "useLinkStatus" { "{ useLinkStatus }".to_owned() } else { format!("{{ useLinkStatus as {status} }}") };
        lines.push(format!("import {clause} from \"@snapfire/fsr/client/react\";"));
      }
      if lines.is_empty() {
        continue;
      }
      let mut edits = Edits::default();
      edits.replace(parsed.range(import.span), lines.join("\n"));
      let Some(local) = local else {
        tree.insert(path, edits.apply(&text));
        continue;
      };
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
              report.note(&path, line, format!("`{}` was dropped: FSR's `Link` has no such prop; `navigate(href, true, {{ replace, scroll }})` from `@snapfire/fsr/client` takes `replace` and `scroll`", name.sym));
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

/// The boundary `catchError` stands for, written once into the application.
const CATCH_ERROR: &str = r#"import { Component, type ReactNode } from "react";
import { refresh } from "@snapfire/fsr/client";

/** What a fallback is given beside its props: what was thrown, `retry` to fetch the page again and render the children anew, `reset` to render them again as they are. */
export interface ErrorInfo {
  error: unknown;
  retry: () => void;
  reset: () => void;
}

/** A component that renders its children and, when one of them throws while rendering, `fallback` with its other props. */
export function catchError<P extends object>(fallback: (props: P, info: ErrorInfo) => ReactNode) {
  return class CaughtError extends Component<P & { children?: ReactNode }, { failed: boolean; error: unknown }> {
    state = { failed: false, error: undefined as unknown };

    static getDerivedStateFromError(error: unknown) {
      return { failed: true, error };
    }

    render(): ReactNode {
      if (!this.state.failed) return this.props.children;
      const { children: _children, ...props } = this.props;
      const reset = () => this.setState({ failed: false, error: undefined });
      return fallback(props as unknown as P, { error: this.state.error, reset, retry: () => void refresh().then(reset) });
    }
  };
}
"#;

/// `catchError` and `unstable_catchError` from `next/error` as a React error
/// boundary `app/src/catch-error.tsx` holds, with `retry` refreshing the
/// page. `unstable_rethrow` from `next/navigation` goes with a note: nothing
/// FSR's `redirect` or `notFound` does is thrown.
struct CatchErrors;

impl Pass for CatchErrors {
  fn name(&self) -> &'static str {
    "next-catch-error"
  }

  fn run(&self, tree: &mut Tree, report: &mut Report) {
    let mut wanted = false;
    for path in scripts(tree) {
      let Some(text) = tree.text(&path).map(str::to_owned) else { continue };
      if !text.contains("next/error") && !text.contains("unstable_rethrow") {
        continue;
      }
      let Ok(parsed) = parse(&path, &text) else { continue };
      let mut edits = Edits::default();
      let mut rethrow: Option<String> = None;
      for item in &parsed.module.body {
        let js::ModuleItem::ModuleDecl(js::ModuleDecl::Import(import)) = item else { continue };
        let module = import.src.value.to_string_lossy();
        if module != "next/error" && module != "next/navigation" {
          continue;
        }
        let mut moved = Vec::new();
        let mut kept = Vec::new();
        for specifier in &import.specifiers {
          let js::ImportSpecifier::Named(n) = specifier else {
            kept.push(text[parsed.range(specifier.span())].to_owned());
            continue;
          };
          let imported = match &n.imported {
            Some(js::ModuleExportName::Ident(i)) => i.sym.to_string(),
            Some(js::ModuleExportName::Str(s)) => s.value.to_string_lossy().into_owned(),
            None => n.local.sym.to_string(),
          };
          let local = n.local.sym.to_string();
          match (module.as_ref(), imported.as_str()) {
            ("next/error", "catchError" | "unstable_catchError" | "ErrorInfo") => {
              let name = if imported == "ErrorInfo" { "ErrorInfo" } else { "catchError" };
              let typed = if n.is_type_only || import.type_only { "type " } else { "" };
              moved.push(if local == name { format!("{typed}{name}") } else { format!("{typed}{name} as {local}") });
            }
            ("next/navigation", "unstable_rethrow") => rethrow = Some(local),
            _ => kept.push(text[parsed.range(n.span)].to_owned()),
          }
        }
        if moved.is_empty() && (module != "next/navigation" || rethrow.is_none()) {
          continue;
        }
        let mut lines = Vec::new();
        if !kept.is_empty() {
          let (defaults, named): (Vec<String>, Vec<String>) = kept.into_iter().partition(|k| !k.starts_with('*') && import.specifiers.iter().any(|s| matches!(s, js::ImportSpecifier::Default(d) if d.local.sym == **k)));
          let mut clause = defaults;
          if !named.is_empty() {
            clause.push(format!("{{ {} }}", named.join(", ")));
          }
          lines.push(format!("import {} from \"{module}\";", clause.join(", ")));
        }
        if !moved.is_empty() {
          wanted = true;
          lines.push(format!("import {{ {} }} from \"@src/catch-error\";", moved.join(", ")));
        }
        match lines.is_empty() {
          true => edits.replace(line_range(&text, parsed.range(import.span)), ""),
          false => edits.replace(parsed.range(import.span), lines.join("\n")),
        }
      }
      if let Some(name) = &rethrow {
        let mut calls = Rethrows { name: name.clone(), found: Vec::new() };
        parsed.module.visit_with(&mut calls);
        for span in &calls.found {
          edits.replace(line_range(&text, parsed.range(*span)), "");
          report.note(&path, parsed.line(*span), "`unstable_rethrow` was dropped: FSR's `redirect` returns a `Response` and a failure a `catch` takes is the body's own, so there is nothing to throw on");
        }
      }
      tree.insert(path, edits.apply(&text));
    }
    if wanted && tree.text("app/src/catch-error.tsx").is_none() {
      tree.insert("app/src/catch-error.tsx", CATCH_ERROR);
    }
  }
}

/// Every expression statement that calls `name`.
struct Rethrows {
  name: String,
  found: Vec<Span>,
}

impl Visit for Rethrows {
  fn visit_expr_stmt(&mut self, stmt: &js::ExprStmt) {
    if let js::Expr::Call(call) = &*stmt.expr {
      if let js::Callee::Expr(callee) = &call.callee {
        if matches!(&**callee, js::Expr::Ident(id) if *id.sym == *self.name) {
          self.found.push(stmt.span);
        }
      }
    }
    stmt.visit_children_with(self);
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

/// `next/image`'s `Image` as FSR's `Picture`, from `@snapfire/fsr/components`,
/// under the name the module gave it. `src`, `alt`, `width`, `height`,
/// `priority`, `sizes` and `quality` mean the same. `placeholder`,
/// `blurDataURL`, `unoptimized`, `fill`, `loader`, `layout`, `objectFit`
/// and `objectPosition` are dropped with a note each.
struct Images;

impl Pass for Images {
  fn name(&self) -> &'static str {
    "next-image"
  }

  fn run(&self, tree: &mut Tree, report: &mut Report) {
    for path in scripts(tree) {
      let Some(text) = tree.text(&path).map(str::to_owned) else { continue };
      if !text.contains("next/image") {
        continue;
      }
      let Ok(parsed) = parse(&path, &text) else { continue };
      let Some((import, local)) = default_import(&parsed, "next/image").or_else(|| default_import(&parsed, "next/legacy/image")) else { continue };
      let mut edits = Edits::default();
      let clause = if local == "Picture" { "{ Picture }".to_owned() } else { format!("{{ Picture as {local} }}") };
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
            "placeholder" | "blurDataURL" | "unoptimized" => {
              edits.replace(range, "");
              report.note(&path, line, format!("`{}` was dropped: FSR builds each image's variants itself", name.sym));
            }
            "fill" | "loader" | "layout" | "objectFit" | "objectPosition" => {
              edits.replace(range, "");
              report.note(&path, line, format!("`{}` was dropped: `Picture` has no such prop; size the image with CSS, an `[images.sources]` entry serves a remote one", name.sym));
            }
            _ => {}
          }
        }
      }
      tree.insert(path, edits.apply(&text));
    }
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

/// A stylesheet imported for its effect, `import "./globals.css"`. FSR links
/// it on the documents whose route reaches the importer, as Next loads it with
/// the route, so it stays. One that uses Tailwind moves under `app/styles/`,
/// where FSR compiles Tailwind, with the import taken out; a sheet whose
/// relative `url()`s that move would break is a note.
struct Stylesheets;

impl Pass for Stylesheets {
  fn name(&self) -> &'static str {
    "next-stylesheets"
  }

  fn run(&self, tree: &mut Tree, report: &mut Report) {
    let mut moved: BTreeMap<String, String> = BTreeMap::new();
    for path in scripts(tree) {
      let Some(text) = tree.text(&path).map(str::to_owned) else { continue };
      if !text.contains(".css") {
        continue;
      }
      let Ok(parsed) = parse(&path, &text) else { continue };
      let mut edits = Edits::default();
      for item in &parsed.module.body {
        let js::ModuleItem::ModuleDecl(js::ModuleDecl::Import(import)) = item else { continue };
        let spec = import.src.value.to_string_lossy().into_owned();
        if !import.specifiers.is_empty() || !spec.ends_with(".css") || spec.ends_with(".module.css") {
          continue;
        }
        let target = if let Some(rest) = spec.strip_prefix("@src/") {
          Some(format!("app/src/{rest}"))
        } else if spec.starts_with("./") || spec.starts_with("../") {
          crate::relocate::normalize(crate::relocate::dirname(&path), &spec)
        } else {
          None
        };
        let Some(target) = target.filter(|t| tree.contains(t)) else { continue };
        if !tree.text(&target).is_some_and(uses_tailwind) {
          continue;
        }
        if !moved.contains_key(&target) {
          let file = target.rsplit('/').next().unwrap_or(&target);
          let mut to = format!("app/styles/{file}");
          let mut n = 2;
          while tree.contains(&to) || moved.values().any(|v| *v == to) {
            to = format!("app/styles/{}-{n}.css", file.trim_end_matches(".css"));
            n += 1;
          }
          moved.insert(target.clone(), to);
        }
        edits.replace(line_range(&text, parsed.range(import.span)), "");
      }
      if !edits.is_empty() {
        tree.insert(path, crate::items::tidy(&edits.apply(&text)));
      }
    }
    let sheets: Vec<String> = tree.paths().filter(|p| p.starts_with("app/") && p.ends_with(".css")).map(str::to_owned).collect();
    for path in sheets {
      if let Some(rewritten) = tree.text(&path).and_then(tailwind_four) {
        tree.insert(path, rewritten);
      }
    }
    for (from, to) in moved {
      let Some(bytes) = tree.remove(&from) else { continue };
      let Ok(css) = String::from_utf8(bytes.clone()) else {
        tree.insert(to, bytes);
        continue;
      };
      let mut out = css.clone();
      let mut unmoved = false;
      for (range, url) in relative_urls(&css).into_iter().rev() {
        match crate::relocate::normalize(crate::relocate::dirname(&from), &url).filter(|asset| tree.contains(asset)) {
          Some(asset) => out.replace_range(range, &crate::relocate::relative(crate::relocate::dirname(&to), &asset)),
          None => unmoved = true,
        }
      }
      if unmoved {
        report.note(&to, 0, format!("moved from `{from}`; a relative `url()` in it names a file that did not come, so it resolves from `app/styles/`"));
      }
      tree.insert(to, out);
    }
  }
}

/// Whether a stylesheet is a Tailwind entry: Tailwind 4's import or Tailwind 3's directives.
fn uses_tailwind(css: &str) -> bool {
  css.contains("@tailwind ") || css.contains("@import \"tailwindcss") || css.contains("@import 'tailwindcss")
}

/// Each relative `url()` in a stylesheet, the range of its path and the path: not a URL with a scheme, not root-relative, not `data:` and not a `var()`. A query or a fragment after the path is left out of both.
pub(crate) fn relative_urls(css: &str) -> Vec<(Range<usize>, String)> {
  let mut out = Vec::new();
  let mut from = 0;
  while let Some(at) = css[from..].find("url(") {
    let open = from + at + "url(".len();
    let Some(close) = css[open..].find(')').map(|c| open + c) else { break };
    from = close;
    let inner = &css[open..close];
    let lead = inner.len() - inner.trim_start().len();
    let quoted = inner.trim().starts_with(['"', '\'']);
    let start = open + lead + usize::from(quoted);
    let body = css[start..close].trim_end();
    let body = if quoted { body.trim_end_matches(['"', '\'']) } else { body };
    let path = body.split(['?', '#']).next().unwrap_or(body);
    if path.is_empty() || path.contains(':') || path.starts_with('/') || path.starts_with("var(") {
      continue;
    }
    out.push((start..start + path.len(), path.to_owned()));
  }
  out
}

/// Tailwind 3's `@tailwind base;`, `@tailwind components;` and
/// `@tailwind utilities;` as Tailwind 4's one `@import "tailwindcss";`, which
/// FSR compiles, written where the first of them was.
fn tailwind_four(css: &str) -> Option<String> {
  let directive = |line: &str| matches!(line.trim(), "@tailwind base;" | "@tailwind components;" | "@tailwind utilities;");
  if !css.lines().any(directive) {
    return None;
  }
  let mut out = String::with_capacity(css.len());
  let mut written = false;
  for line in css.lines() {
    if directive(line) {
      if !written {
        out.push_str("@import \"tailwindcss\";\n");
        written = true;
      }
      continue;
    }
    out.push_str(line);
    out.push('\n');
  }
  Some(out)
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
  fn use_link_status_comes_from_the_react_adapter_beside_the_link() {
    let input = tree(&[
      ("package.json", PACKAGE),
      ("app/page.tsx", "import Link, { useLinkStatus } from \"next/link\";\n\nfunction Hint() {\n  const { pending } = useLinkStatus();\n  return pending ? <i>…</i> : null;\n}\n\nexport default function Page() {\n  return <Link href=\"/a\">a<Hint /></Link>;\n}\n"),
      ("app/b/page.tsx", "import { useLinkStatus as linkStatus } from \"next/link\";\n\nexport default function Page() {\n  return linkStatus().pending ? <i>…</i> : null;\n}\n"),
    ]);
    let (out, _) = migrate(&input);
    let page = out.text("app/routes/page.tsx").unwrap();
    assert!(page.starts_with("import { Link } from \"@snapfire/fsr/components\";\nimport { useLinkStatus } from \"@snapfire/fsr/client/react\";\n\nfunction Hint() {"), "{page}");
    let b = out.text("app/routes/b/page.tsx").unwrap();
    assert!(b.starts_with("import { useLinkStatus as linkStatus } from \"@snapfire/fsr/client/react\";\n"), "{b}");
  }

  #[test]
  fn catch_error_comes_from_a_boundary_written_into_the_app() {
    let input = tree(&[
      ("package.json", PACKAGE),
      ("app/ui/boundary.tsx", "'use client';\nimport { unstable_catchError as catchError, type ErrorInfo } from \"next/error\";\n\nfunction Fallback(props: { title: string }, { error, retry }: ErrorInfo) {\n  return <button onClick={() => retry()}>{props.title}</button>;\n}\n\nexport default catchError(Fallback);\n"),
      ("app/page.tsx", "import { unstable_rethrow } from \"next/navigation\";\nimport Boundary from \"./ui/boundary\";\n\nexport default async function Page() {\n  try {\n    return <Boundary title=\"x\">a</Boundary>;\n  } catch (e) {\n    unstable_rethrow(e);\n    return null;\n  }\n}\n"),
    ]);
    let (out, report) = migrate(&input);
    let boundary = out.paths().find(|p| p.ends_with("boundary.tsx")).map(|p| out.text(p).unwrap().to_owned()).expect("the boundary moved");
    assert!(boundary.contains("import { catchError, type ErrorInfo } from \"@src/catch-error\";"), "{boundary}");
    assert!(!boundary.contains("next/error"), "{boundary}");
    let module = out.text("app/src/catch-error.tsx").expect("the boundary module is written");
    assert!(module.contains("export function catchError") && module.contains("getDerivedStateFromError") && module.contains("refresh()"), "{module}");
    let page = out.text("app/routes/page.tsx").unwrap();
    assert!(!page.contains("unstable_rethrow"), "{page}");
    assert!(page.contains("  } catch (e) {\n    return null;\n  }"), "{page}");
    assert!(report.notes.iter().any(|n| n.message.contains("unstable_rethrow") && n.line == 8), "{:?}", report.notes);
  }

  #[test]
  fn next_image_is_fsrs_picture_and_its_next_only_props_go() {
    let input = tree(&[
      ("package.json", PACKAGE),
      ("app/page.tsx", "import Image from \"next/image\";\nimport hero from \"./hero.png\";\n\nexport default function Page() {\n  return <Image src={hero} alt=\"Hero\" priority placeholder=\"blur\" fill />;\n}\n"),
    ]);
    let (out, report) = migrate(&input);
    assert_eq!(
      out.text("app/routes/page.tsx").unwrap(),
      "import { Picture as Image } from \"@snapfire/fsr/components\";\nimport hero from \"./hero.png\";\n\nexport default function Page() {\n  return <Image src={hero} alt=\"Hero\" priority />;\n}\n"
    );
    assert_eq!(report.notes.len(), 2, "{:?}", report.notes);
  }

  #[test]
  fn a_stylesheet_imported_for_its_effect_stays_beside_its_importer_and_a_tailwind_one_moves_under_styles() {
    let input = tree(&[
      ("package.json", PACKAGE),
      ("app/layout.tsx", "import \"./globals.css\";\nimport styles from \"./page.module.css\";\n\nexport default function Layout({ children }) {\n  return <div className={styles.x}>{children}</div>;\n}\n"),
      ("app/globals.css", "@tailwind base;\n@tailwind components;\n@tailwind utilities;\n\nbody {\n  @apply bg-gray-50;\n}\n"),
      ("app/page.module.css", ".x {}\n"),
      ("app/card.tsx", "import \"./card.css\";\n\nexport function Card() {\n  return <div className=\"card\" />;\n}\n"),
      ("app/card.css", ".card {\n  background: url(\"./back.png\");\n}\n"),
      ("app/back.png", "png"),
      ("Dockerfile.bun", "FROM oven/bun\n"),
      ("compose.yml", "services: {}\n"),
    ]);
    let (out, _) = migrate(&input);
    let paths: Vec<&str> = out.paths().collect();
    assert_eq!(paths, ["app/routes/back.png", "app/routes/card.css", "app/routes/card.tsx", "app/routes/layout.tsx", "app/routes/page.module.css", "app/styles/globals.css"]);
    assert!(out.text("app/routes/layout.tsx").unwrap().starts_with("import styles from \"./page.module.css\";\n"));
    assert!(out.text("app/routes/card.tsx").unwrap().starts_with("import \"./card.css\";\n"), "linked on the documents that place the component");
    assert_eq!(out.text("app/routes/card.css").unwrap(), ".card {\n  background: url(\"./back.png\");\n}\n");
    assert_eq!(out.text("app/styles/globals.css").unwrap(), "@import \"tailwindcss\";\n\nbody {\n  @apply bg-gray-50;\n}\n", "Tailwind 3's directives are Tailwind 4's import");
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
