//! `migrate.toml`, the migration map: what a codebase's own modules stand for, so `fsr migrate` rewrites a call to a shell package's wrapper the way it rewrites the framework call the wrapper makes.

use std::collections::{BTreeMap, BTreeSet};

use schemars::JsonSchema;
use serde::Deserialize;
use swc_core::common::{Span, Spanned};
use swc_core::ecma::ast as js;
use swc_core::ecma::visit::{Visit, VisitWith};

use crate::edit::Edits;
use crate::items::line_range;
use crate::syntax::{is_script, parse, Parsed};
use crate::workspace::WORKSPACE_DIR;
use crate::{Pass, Report, Tree};

/// The map's file name, at the application's root or a workspace package's.
pub const MAP_FILE: &str = "migrate.toml";

/// `migrate.toml`: what the modules a codebase writes for itself stand for, read by `fsr migrate` and by nothing at run time. The application's own file wins over one a workspace package ships.
#[derive(Debug, Default, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
#[schemars(title = "migrate.toml")]
pub struct MigrationMap {
  /// Each key names a module such as `@shell/navigation` or one export of it such as `@shell/navigation#Link`, with `#default` for its default export. An export's entry wins over its module's.
  #[serde(default)]
  pub map: BTreeMap<String, Entry>,
}

/// What one module or export stands for. Exactly one of `to`, `call` and `drop`; `note` goes with any of them or alone.
#[derive(Debug, Clone, Default, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Entry {
  /// The module or export this one is. A Next export such as `next/navigation#notFound` makes every rule `fsr migrate` has for Next apply to the wrapper. An FSR export such as `@snapfire/fsr/components#Link` is used as it is. A module's key takes a module, `next/navigation`.
  #[serde(default)]
  pub to: Option<String>,
  /// The expression a call to the export becomes in a route module, where the route's loader runs it: `params.locale` for a locale read off the URL. `$0`, `$1` and onward are the call's arguments as written. An `await` of the call is replaced with it.
  #[serde(default)]
  pub call: Option<String>,
  /// The statements calling the export are removed with its import, for setup FSR does itself: analytics, error reporting, a provider.
  #[serde(default)]
  pub drop: bool,
  /// Noted at each import of it, for what a person has to rewrite.
  #[serde(default)]
  pub note: Option<String>,
}

/// The JSON Schema of `migrate.toml`, for the `#:schema` line an editor reads.
pub fn schema() -> serde_json::Value {
  serde_json::to_value(schemars::schema_for!(MigrationMap)).expect("a schema serializes")
}

pub(crate) struct Map;

impl Pass for Map {
  fn name(&self) -> &'static str {
    "migration-map"
  }

  fn run(&self, tree: &mut Tree, report: &mut Report) {
    let map = read(tree, report);
    if map.is_empty() {
      return;
    }
    let paths: Vec<String> = tree.paths().filter(|p| is_script(p)).map(str::to_owned).collect();
    for path in paths {
      let Some(text) = tree.text(&path).map(str::to_owned) else { continue };
      if !map.keys().any(|k| text.contains(k.split('#').next().unwrap_or(k))) {
        continue;
      }
      let Ok(parsed) = parse(&path, &text) else { continue };
      if let Some(rewritten) = rewrite(&path, &text, &parsed, &map, report) {
        tree.insert(path, rewritten);
      }
    }
  }
}

/// Every map the tree holds, merged: the application's entries over its workspace packages'.
fn read(tree: &Tree, report: &mut Report) -> BTreeMap<String, Entry> {
  let mut files: Vec<String> = tree.paths().filter(|p| p.starts_with(WORKSPACE_DIR) && p.ends_with(&format!("/{MAP_FILE}"))).map(str::to_owned).collect();
  files.push(MAP_FILE.to_owned());
  let mut out = BTreeMap::new();
  for file in files {
    let Some(text) = tree.text(&file) else { continue };
    let shown = file.strip_prefix(WORKSPACE_DIR).unwrap_or(&file);
    let parsed: MigrationMap = match toml::from_str(text) {
      Ok(map) => map,
      Err(e) => {
        report.note(shown, 0, format!("does not parse, so its entries were not applied: {}", e.message()));
        continue;
      }
    };
    for (key, entry) in parsed.map {
      let kinds = [entry.to.is_some(), entry.call.is_some(), entry.drop].iter().filter(|k| **k).count();
      if kinds > 1 {
        report.note(shown, 0, format!("`{key}` gives more than one of `to`, `call` and `drop`, so it was not applied"));
        continue;
      }
      if let (Some(to), false) = (&entry.to, key.contains('#')) {
        if to.contains('#') {
          report.note(shown, 0, format!("`{key}` is a module and `to` names an export, `{to}`; a module stands for a module"));
          continue;
        }
      }
      out.insert(key, entry);
    }
  }
  out
}

/// What a module route file is: Next's route files and the loaders beside them, where the request is in reach.
fn is_route(path: &str) -> bool {
  if path.starts_with(WORKSPACE_DIR) {
    return false;
  }
  let file = path.rsplit('/').next().unwrap_or(path);
  let stem = file.split('.').next().unwrap_or(file);
  ["page", "layout", "route", "default", "template", "not-found", "error", "loading"].contains(&stem) && (path.starts_with("app/") || path.contains("/app/"))
}

/// One name an import binds that the map has an entry for.
struct Mapped<'a> {
  local: String,
  export: String,
  type_only: bool,
  entry: &'a Entry,
  /// The map's key the entry is under.
  key: String,
}

fn rewrite(path: &str, text: &str, parsed: &Parsed, map: &BTreeMap<String, Entry>, report: &mut Report) -> Option<String> {
  let shown = path.strip_prefix(WORKSPACE_DIR).unwrap_or(path);
  let mut edits = Edits::default();
  let mut mapped_imports: Vec<(&js::ImportDecl, Vec<Mapped>)> = Vec::new();
  for item in &parsed.module.body {
    let js::ModuleItem::ModuleDecl(js::ModuleDecl::Import(import)) = item else { continue };
    let source = import.src.value.to_string_lossy().into_owned();
    let mut found = Vec::new();
    for spec in &import.specifiers {
      let (local, export, type_only) = match spec {
        js::ImportSpecifier::Named(n) => {
          let export = match &n.imported {
            Some(js::ModuleExportName::Ident(i)) => i.sym.to_string(),
            Some(js::ModuleExportName::Str(s)) => s.value.to_string_lossy().into_owned(),
            None => n.local.sym.to_string(),
          };
          (n.local.sym.to_string(), export, n.is_type_only || import.type_only)
        }
        js::ImportSpecifier::Default(d) => (d.local.sym.to_string(), "default".to_owned(), import.type_only),
        js::ImportSpecifier::Namespace(ns) => {
          if map.keys().any(|k| k.starts_with(&format!("{source}#"))) && !map.contains_key(&source) {
            report.note(shown, parsed.line(import.span), format!("`* as {}` imports `{source}`, which the map names export by export, so it was left", ns.local.sym));
          }
          if let Some(entry) = map.get(&source) {
            found.push(Mapped { local: ns.local.sym.to_string(), export: "*".to_owned(), type_only: import.type_only, entry, key: source.clone() });
          }
          continue;
        }
      };
      let key = format!("{source}#{export}");
      if let Some((key, entry)) = map.get(&key).map(|e| (key.clone(), e)).or_else(|| map.get(&source).map(|e| (source.clone(), e))) {
        found.push(Mapped { local, export, type_only, entry, key });
      }
    }
    if !found.is_empty() {
      for m in &found {
        if let Some(note) = &m.entry.note {
          report.note(shown, parsed.line(import.span), format!("`{}`: {note}", m.key));
        }
      }
      mapped_imports.push((import, found));
    }
  }
  if mapped_imports.is_empty() {
    return None;
  }
  let replaced: BTreeMap<String, &Entry> = mapped_imports.iter().flat_map(|(_, f)| f.iter()).filter(|m| m.entry.call.is_some() || m.entry.drop).map(|m| (m.local.clone(), m.entry)).collect();
  let mut uses = Uses { locals: replaced.keys().cloned().collect(), calls: Vec::new(), other: BTreeSet::new() };
  parsed.module.visit_with(&mut uses);
  let route = is_route(path);
  let mut kept: BTreeSet<String> = uses.other.clone();
  for call in &uses.calls {
    let entry = replaced[&call.local];
    match (&entry.call, entry.drop, call.stmt, route) {
      (Some(template), _, _, true) => {
        let mut filled = template.clone();
        for (i, arg) in call.args.iter().enumerate().rev() {
          filled = filled.replace(&format!("${i}"), &text[parsed.range(*arg)]);
        }
        edits.replace(parsed.range(call.outer), filled);
      }
      (Some(_), _, _, false) => {
        report.note(shown, parsed.line(call.outer), format!("a call to `{}` outside a route module, where the map's `call` has no request to read, so it was left", call.local));
        kept.insert(call.local.clone());
      }
      (None, true, Some(stmt), _) => edits.replace(line_range(text, parsed.range(stmt)), ""),
      (None, true, None, _) => {
        report.note(shown, parsed.line(call.outer), format!("`{}` is called inside an expression, so the map's `drop` left it", call.local));
        kept.insert(call.local.clone());
      }
      _ => {}
    }
  }
  for local in &uses.other {
    report.note(shown, 0, format!("`{local}` is used other than called, so the map's entry for it was not applied there"));
  }
  for (import, found) in &mapped_imports {
    edits.replace(line_range(text, parsed.range(import.span)), imports_for(text, parsed, import, found, &kept));
  }
  Some(edits.apply(text))
}

/// `import` written again: the names the map sends elsewhere imported from where it sends them, the rest from where they were.
fn imports_for(text: &str, parsed: &Parsed, import: &js::ImportDecl, found: &[Mapped], kept: &BTreeSet<String>) -> String {
  let source = import.src.value.to_string_lossy().into_owned();
  let mut by_source: BTreeMap<String, Vec<String>> = BTreeMap::new();
  let mut defaults: BTreeMap<String, String> = BTreeMap::new();
  let mut namespaces: Vec<(String, String)> = Vec::new();
  for spec in &import.specifiers {
    let local = match spec {
      js::ImportSpecifier::Named(n) => n.local.sym.to_string(),
      js::ImportSpecifier::Default(d) => d.local.sym.to_string(),
      js::ImportSpecifier::Namespace(ns) => ns.local.sym.to_string(),
    };
    let mapped = found.iter().find(|m| m.local == local);
    let (target, export, type_only) = match mapped {
      Some(m) if (m.entry.call.is_some() || m.entry.drop) && !kept.contains(&local) => continue,
      Some(Mapped { entry: Entry { to: Some(to), .. }, export, type_only, .. }) => match to.split_once('#') {
        Some((module, name)) => (module.to_owned(), name.to_owned(), *type_only),
        None => (to.clone(), export.clone(), *type_only),
      },
      _ => {
        let written = text[parsed.range(spec.span())].to_owned();
        match spec {
          js::ImportSpecifier::Default(_) => {
            defaults.insert(source.clone(), local);
          }
          js::ImportSpecifier::Namespace(_) => namespaces.push((source.clone(), local)),
          js::ImportSpecifier::Named(_) => by_source.entry(source.clone()).or_default().push(written.trim_start_matches("type ").to_owned()),
        }
        continue;
      }
    };
    if export == "*" {
      namespaces.push((target, local));
    } else if export == "default" {
      defaults.insert(target, local);
    } else {
      let named = if export == local { local } else { format!("{export} as {local}") };
      by_source.entry(target).or_default().push(if type_only && !import.type_only { format!("type {named}") } else { named });
    }
  }
  let keyword = if import.type_only { "import type" } else { "import" };
  let mut out = String::new();
  let sources: BTreeSet<&String> = by_source.keys().chain(defaults.keys()).collect();
  for target in sources {
    let named = by_source.get(target).map(|n| format!("{{ {} }}", n.join(", ")));
    let clause = match (defaults.get(target), named) {
      (Some(d), Some(n)) => format!("{d}, {n}"),
      (Some(d), None) => d.clone(),
      (None, Some(n)) => n,
      (None, None) => continue,
    };
    out.push_str(&format!("{keyword} {clause} from \"{target}\";\n"));
  }
  for (target, local) in namespaces {
    out.push_str(&format!("{keyword} * as {local} from \"{target}\";\n"));
  }
  out
}

/// One call to a mapped name: the expression it replaces, an `await` of it included, its arguments and the statement it is when it stands alone.
struct Call {
  local: String,
  outer: Span,
  args: Vec<Span>,
  stmt: Option<Span>,
}

struct Uses {
  locals: BTreeSet<String>,
  calls: Vec<Call>,
  other: BTreeSet<String>,
}

impl Uses {
  /// The call `e` is when it calls a mapped name directly or through `await`.
  fn call_of<'e>(&self, e: &'e js::Expr) -> Option<(&'e js::CallExpr, Span)> {
    let (call, outer) = match e {
      js::Expr::Await(a) => match &*a.arg {
        js::Expr::Call(c) => (c, a.span),
        _ => return None,
      },
      js::Expr::Call(c) => (c, c.span),
      _ => return None,
    };
    let js::Callee::Expr(callee) = &call.callee else { return None };
    let js::Expr::Ident(id) = &**callee else { return None };
    self.locals.contains(id.sym.as_ref()).then_some((call, outer))
  }

  fn record(&mut self, call: &js::CallExpr, outer: Span, stmt: Option<Span>) {
    let js::Callee::Expr(callee) = &call.callee else { return };
    let js::Expr::Ident(id) = &**callee else { return };
    self.calls.push(Call { local: id.sym.to_string(), outer, args: call.args.iter().map(|a| a.expr.span()).collect(), stmt });
    for arg in &call.args {
      arg.visit_with(self);
    }
  }
}

impl Visit for Uses {
  fn visit_import_decl(&mut self, _: &js::ImportDecl) {}

  fn visit_expr_stmt(&mut self, n: &js::ExprStmt) {
    match self.call_of(&n.expr) {
      Some((call, outer)) => self.record(call, outer, Some(n.span)),
      None => n.visit_children_with(self),
    }
  }

  fn visit_expr(&mut self, n: &js::Expr) {
    match self.call_of(n) {
      Some((call, outer)) => self.record(call, outer, None),
      None => n.visit_children_with(self),
    }
  }

  fn visit_ident(&mut self, n: &js::Ident) {
    if self.locals.contains(n.sym.as_ref()) {
      self.other.insert(n.sym.to_string());
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

  const MAP: &str = r#"#:schema ./migrate.schema.json
[map."@shell/nav/server#notFound"]
to = "next/navigation#notFound"

[map."@shell/nav#Link"]
to = "@snapfire/fsr/components#Link"

[map."@shell/i18n/server#getLocale"]
call = "params.locale"

[map."@shell/i18n/server#getMessage"]
call = "($1 ?? $0)"

[map."@shell/telemetry#init"]
drop = true

[map."@shell/legacy"]
note = "rewrite its banner by hand"
"#;

  #[test]
  fn each_kind_of_entry_rewrites_what_it_names() {
    let page = "import { notFound } from \"@shell/nav/server\";\nimport { Link } from \"@shell/nav\";\nimport { getLocale, getMessage } from \"@shell/i18n/server\";\nimport { init } from \"@shell/telemetry\";\nimport { Banner } from \"@shell/legacy\";\nimport { getPage } from \"../lib/cms\";\n\nexport default async function Home() {\n  init();\n  const locale = await getLocale();\n  const title = getMessage(\"title\", \"Home\");\n  const page = await getPage(locale);\n  if (!page) notFound();\n  return <Banner><Link href=\"/\">{title}{page.body}</Link></Banner>;\n}\n";
    let input = tree(&[("package.json", r#"{ "dependencies": { "next": "15" } }"#), ("migrate.toml", MAP), ("app/page.tsx", page), ("lib/cms.ts", "export const getPage = async (l) => ({ body: l });\n")]);
    let (out, report) = migrate(&input);
    let loader = out.text("app/routes/page.loader.ts").unwrap();
    assert!(loader.contains("load({ params }"), "{loader}");
    assert!(loader.contains("const locale = params.locale;"), "{loader}");
    assert!(loader.contains("const title = (\"Home\" ?? \"title\");"), "{loader}");
    assert!(loader.contains("fail(\"not_found\", \"not found\")") && !loader.contains("notFound"), "{loader}");
    assert!(!loader.contains("init") && !loader.contains("@shell/i18n") && !loader.contains("@shell/telemetry"), "{loader}");
    let page = out.text("app/routes/page.tsx").unwrap();
    assert!(page.contains("import { Link } from \"@snapfire/fsr/components\";"), "{page}");
    assert!(page.contains("import { Banner } from \"@shell/legacy\";"), "{page}");
    assert!(report.notes.iter().any(|n| n.message == "`@shell/legacy`: rewrite its banner by hand"), "{:?}", report.notes);
  }

  #[test]
  fn a_call_outside_a_route_module_is_left_with_a_note() {
    let card = "import { getLocale } from \"@shell/i18n/server\";\nexport function Card() {\n  return <p>{getLocale()}</p>;\n}\n";
    let input = tree(&[("package.json", r#"{ "dependencies": { "next": "15" } }"#), ("migrate.toml", MAP), ("app/page.tsx", "export default function Page() { return null; }\n"), ("components/Card.tsx", card)]);
    let (out, report) = migrate(&input);
    assert!(out.text("app/src/components/Card.tsx").unwrap().contains("import { getLocale } from \"@shell/i18n/server\";"));
    assert!(report.notes.iter().any(|n| n.message.starts_with("a call to `getLocale` outside a route module")), "{:?}", report.notes);
  }

  #[test]
  fn an_entry_giving_two_kinds_is_noted_and_not_applied() {
    let map = "[map.\"@shell/nav#Link\"]\nto = \"next/link#default\"\ndrop = true\n\n[map.\"@shell/nav\"]\nto = \"next/navigation#redirect\"\n";
    let input = tree(&[("package.json", r#"{ "dependencies": { "next": "15" } }"#), ("migrate.toml", map), ("app/page.tsx", "import { Link } from \"@shell/nav\";\nexport default function Page() { return <Link href=\"/\" />; }\n")]);
    let (_, report) = migrate(&input);
    assert!(report.notes.iter().any(|n| n.message.contains("`@shell/nav#Link` gives more than one of `to`, `call` and `drop`")), "{:?}", report.notes);
    assert!(report.notes.iter().any(|n| n.message.contains("a module stands for a module")), "{:?}", report.notes);
  }

  #[test]
  fn the_schema_describes_every_field_of_an_entry() {
    let schema = super::schema().to_string();
    for field in ["\"to\"", "\"call\"", "\"drop\"", "\"note\"", "migrate.toml"] {
      assert!(schema.contains(field), "{field}: {schema}");
    }
  }
}
