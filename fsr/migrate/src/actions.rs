//! Next's server actions, the async functions a `"use server"` module
//! exports, as FSR actions: each in the `actions.ts` of the route it belongs
//! to, its input type under `schemas/`, every caller reaching it through the
//! generated `actions`.

use std::collections::BTreeSet;

use swc_core::common::{Span, Spanned};
use swc_core::ecma::ast as js;
use swc_core::ecma::visit::{Visit, VisitWith};

use crate::edit::Edits;
use crate::items::{line_range, local, prune, tidy};
use crate::lift::{imports_for, route_id};
use crate::relocate::{dirname, normalize};
use crate::syntax::{is_script, parse, specifiers, Parsed, SCRIPT_EXTENSIONS};
use crate::{Pass, Report, Tree};

pub(crate) struct ServerActions;

/// One exported function, rewritten as an action.
struct Converted {
  name: String,
  /// The type its one parameter is annotated with; `None` when it takes none.
  input: Option<String>,
  text: String,
}

impl Pass for ServerActions {
  fn name(&self) -> &'static str {
    "next-server-actions"
  }

  fn run(&self, tree: &mut Tree, report: &mut Report) {
    let modules: Vec<String> = tree.paths().filter(|p| p.starts_with("app/") && is_script(p)).map(str::to_owned).collect();
    for path in modules {
      let Some(text) = tree.text(&path).map(str::to_owned) else { continue };
      if !text.contains("use server") {
        continue;
      }
      let Ok(parsed) = parse(&path, &text) else { continue };
      if uses_server(&parsed.module) {
        convert_module(tree, &path, &text, &parsed, report);
      }
    }
  }
}

fn uses_server(module: &js::Module) -> bool {
  module.body.iter().map_while(|item| match item {
    js::ModuleItem::Stmt(js::Stmt::Expr(e)) => match &*e.expr {
      js::Expr::Lit(js::Lit::Str(s)) => Some(s.value.to_string_lossy() == "use server"),
      _ => None,
    },
    _ => None,
  })
  .any(|server| server)
}

/// The route directory a module's actions belong to: the nearest one at or
/// above it holding a page; `app/routes` for a module outside the routes.
fn route_dir(tree: &Tree, path: &str) -> String {
  let mut dir = if path.starts_with("app/routes/") { dirname(path).to_owned() } else { "app/routes".to_owned() };
  while dir != "app/routes" && !SCRIPT_EXTENSIONS.iter().any(|ext| tree.contains(&format!("{dir}/page{ext}"))) {
    dir = dirname(&dir).to_owned();
  }
  dir
}

/// `actions.<id>`, a segment that is no identifier written as a key.
fn accessor(id: &str) -> String {
  let mut out = "actions".to_owned();
  for segment in id.split('.') {
    let identifier = segment.starts_with(|c: char| c.is_ascii_alphabetic() || c == '_' || c == '$') && segment.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '$');
    if identifier {
      out.push('.');
      out.push_str(segment);
    } else {
      out.push_str(&format!("[{segment:?}]"));
    }
  }
  out
}

fn convert_module(tree: &mut Tree, path: &str, text: &str, parsed: &Parsed, report: &mut Report) {
  let mut converted = Vec::new();
  let mut kept = Edits::default();
  for item in &parsed.module.body {
    let js::ModuleItem::ModuleDecl(js::ModuleDecl::ExportDecl(js::ExportDecl { decl: js::Decl::Fn(f), .. })) = item else { continue };
    let name = f.ident.sym.to_string();
    let line = parsed.line(item.span());
    let (Some(body), true) = (&f.function.body, f.function.is_async) else { continue };
    let params: Vec<&js::Pat> = f.function.params.iter().map(|p| &p.pat).collect();
    let input = match params.as_slice() {
      [] => None,
      [param] => match annotated_type(param) {
        Some(t) if t == "FormData" => {
          report.note(path, line, format!("`{name}` takes a `FormData`; an FSR action takes the form's fields as its input, typed under `schemas/`, so it was left in place"));
          continue;
        }
        Some(t) => Some(t),
        None => {
          report.note(path, line, format!("`{name}`'s parameter has no type; an FSR action's input is a type under `schemas/`, so it was left in place"));
          continue;
        }
      },
      _ => {
        report.note(path, line, format!("`{name}` takes more than one argument; an FSR action takes one input, so it was left in place"));
        continue;
      }
    };
    let body_range = parsed.range(body.span);
    let mut edits = Edits::default();
    if let (Some(js::Pat::Ident(param)), Some(_)) = (params.first(), &input) {
      edits.replace(1..1, format!("\n  const {} = input;", param.id.sym));
    }
    let mut revalidates = Revalidates { found: Vec::new() };
    body.visit_with(&mut revalidates);
    for span in revalidates.found {
      let range = line_range(text, parsed.range(span));
      edits.replace(range.start - body_range.start..range.end - body_range.start, "");
    }
    let ctx = match &input {
      Some(t) => format!("{{ input }}: ActionCtx<{t}>"),
      None => "_ctx: ActionCtx".to_owned(),
    };
    let body_text = edits.apply(&text[body_range.clone()]);
    converted.push(Converted { name: name.clone(), input, text: format!("export const {name} = action(async ({ctx}) => {body_text});\n") });
    kept.replace(line_range(text, parsed.range(item.span())), "");
  }
  if converted.is_empty() {
    return;
  }

  let dir = route_dir(tree, path);
  let id = route_id(&format!("{dir}/page.tsx"));
  let actions_path = format!("{dir}/actions.ts");
  let inputs: BTreeSet<String> = converted.iter().filter_map(|c| c.input.clone()).collect();

  let mut schemas = String::new();
  for input in &inputs {
    match declaration(tree, path, parsed, input) {
      Some(declared) => {
        let file = format!("app/schemas/{}.ts", input.to_lowercase());
        tree.insert(file, declared);
        schemas.push_str(&format!("import type {{ {input} }} from \"@schemas/{}\";\n", input.to_lowercase()));
      }
      None => report.note(path, 0, format!("`{input}` is not declared in a module of the application, so `schemas/{}.ts` was not written", input.to_lowercase())),
    }
  }

  let mut needed: BTreeSet<String> = BTreeSet::new();
  for item in &parsed.module.body {
    let js::ModuleItem::ModuleDecl(js::ModuleDecl::Import(import)) = item else { continue };
    if import.src.value.to_string_lossy() == "next/cache" {
      continue;
    }
    needed.extend(import.specifiers.iter().map(local).filter(|l| !inputs.contains(l)));
  }
  let imports = imports_for(text, parsed, &needed, path, &actions_path);
  let existing = if actions_path == path { String::new() } else { tree.text(&actions_path).unwrap_or_default().to_owned() };
  let mut actions = format!("import {{ action }} from \"@snapfire/fsr\";\nimport type {{ ActionCtx }} from \"@snapfire/fsr\";\n{schemas}{imports}\n{existing}");
  for c in &converted {
    actions.push('\n');
    actions.push_str(&c.text);
  }
  let actions = prune(&actions_path, &actions, &needed);
  tree.insert(actions_path.clone(), tidy(&actions));

  let rest = prune(path, &kept.apply(text), &needed);
  let exports_left = parse(path, &rest).is_ok_and(|p| p.module.body.iter().any(|item| matches!(item, js::ModuleItem::ModuleDecl(js::ModuleDecl::ExportDecl(_)))));
  // FSR reads every export of `actions.ts` as an action, so what could not be
  // converted moves out of the way when the module was that file.
  let leftover = match (exports_left, path == actions_path) {
    (false, _) => None,
    (true, false) => Some(path.to_owned()),
    (true, true) => Some(format!("{dir}/server-actions.ts")),
  };
  let names: BTreeSet<String> = converted.iter().map(|c| c.name.clone()).collect();
  rewrite_callers(tree, path, &names, &accessor(&id), leftover.as_deref(), report);
  match &leftover {
    Some(target) => {
      if target != path {
        report.note(target, 0, format!("the server actions this pass could not convert moved here from `{path}`, since every export of `actions.ts` is an FSR action"));
      }
      tree.insert(target.clone(), tidy(&rest));
    }
    None if path != actions_path => {
      tree.remove(path);
    }
    None => {}
  }
}

/// `T` as an interface for `schemas/`: a `type T = { ... }` or an
/// `interface T` declared in the module or in the local module it imports
/// `T` from.
fn declaration(tree: &Tree, path: &str, parsed: &Parsed, name: &str) -> Option<String> {
  let in_module = |p: &Parsed, text: &str| -> Option<String> {
    p.module.body.iter().find_map(|item| {
      let decl = match item {
        js::ModuleItem::ModuleDecl(js::ModuleDecl::ExportDecl(e)) => &e.decl,
        js::ModuleItem::Stmt(js::Stmt::Decl(d)) => d,
        _ => return None,
      };
      match decl {
        js::Decl::TsInterface(i) if *i.id.sym == *name => Some(format!("export interface {name} {}\n", &text[p.range(i.body.span)])),
        js::Decl::TsTypeAlias(t) if *t.id.sym == *name => match &*t.type_ann {
          js::TsType::TsTypeLit(lit) => Some(format!("export interface {name} {}\n", &text[p.range(lit.span)])),
          _ => None,
        },
        _ => None,
      }
    })
  };
  if let Some(found) = in_module(parsed, tree.text(path)?) {
    return Some(found);
  }
  for item in &parsed.module.body {
    let js::ModuleItem::ModuleDecl(js::ModuleDecl::Import(import)) = item else { continue };
    if !import.specifiers.iter().any(|s| local(s) == name) {
      continue;
    }
    let target = resolve(tree, path, &import.src.value.to_string_lossy())?;
    let text = tree.text(&target)?;
    let other = parse(&target, text).ok()?;
    return in_module(&other, text);
  }
  None
}

fn resolve(tree: &Tree, from: &str, spec: &str) -> Option<String> {
  let base = match spec.strip_prefix("@src/") {
    Some(rest) => format!("app/src/{rest}"),
    None if spec.starts_with("./") || spec.starts_with("../") => normalize(dirname(from), spec)?,
    None => return None,
  };
  std::iter::once(base.clone()).chain(SCRIPT_EXTENSIONS.iter().map(|e| format!("{base}{e}"))).find(|c| tree.contains(c))
}

/// Every module importing one of `names` from `module`: the import dropped
/// for `actions` from `@generated/client`, each call through `accessor`.
fn rewrite_callers(tree: &mut Tree, module: &str, names: &BTreeSet<String>, accessor: &str, leftover: Option<&str>, report: &mut Report) {
  let scripts: Vec<String> = tree.paths().filter(|p| p.starts_with("app/") && is_script(p) && *p != module).map(str::to_owned).collect();
  for path in scripts {
    let Some(text) = tree.text(&path).map(str::to_owned) else { continue };
    let Ok(parsed) = parse(&path, &text) else { continue };
    if !specifiers(&parsed).iter().any(|s| resolve(tree, &path, &s.value).as_deref() == Some(module)) {
      continue;
    }
    let mut edits = Edits::default();
    let mut locals: Vec<(String, String)> = Vec::new();
    for item in &parsed.module.body {
      let js::ModuleItem::ModuleDecl(js::ModuleDecl::Import(import)) = item else { continue };
      if resolve(tree, &path, &import.src.value.to_string_lossy()).as_deref() != Some(module) {
        continue;
      }
      let mut kept = Vec::new();
      for s in &import.specifiers {
        let js::ImportSpecifier::Named(n) = s else {
          kept.push(s);
          continue;
        };
        let imported = match &n.imported {
          Some(js::ModuleExportName::Ident(i)) => i.sym.to_string(),
          _ => n.local.sym.to_string(),
        };
        if names.contains(&imported) {
          locals.push((n.local.sym.to_string(), imported));
        } else {
          kept.push(s);
        }
      }
      let line = line_range(&text, parsed.range(import.span));
      let mut replacement = if kept.is_empty() { String::new() } else { crate::items::import_text(&text, &parsed, import, &kept) };
      if let Some(target) = leftover.filter(|t| *t != module && !replacement.is_empty()) {
        let spec = import.src.value.to_string_lossy().into_owned();
        let moved = crate::relocate::relative(dirname(&path), target.trim_end_matches(".ts"));
        replacement = replacement.replace(&format!("\"{spec}\""), &format!("\"{moved}\"")).replace(&format!("'{spec}'"), &format!("'{moved}'"));
      }
      edits.replace(line, replacement);
    }
    if locals.is_empty() {
      continue;
    }
    let mut uses = Uses { locals: &locals, calls: Vec::new(), other: Vec::new() };
    parsed.module.visit_with(&mut uses);
    for (span, imported) in uses.calls {
      edits.replace(parsed.range(span), format!("{accessor}.{imported}"));
    }
    for (span, local) in uses.other {
      report.note(&path, parsed.line(span), format!("`{local}` is used other than called; an FSR action is called as `{accessor}.<name>(input)`, a form posts to `/_sf/action/<id>`"));
    }
    let rewritten = format!("import {{ actions }} from \"@generated/client\";\n{}", edits.apply(&text));
    tree.insert(path, tidy(&rewritten));
  }
}

fn annotated_type(pat: &js::Pat) -> Option<String> {
  let ann = match pat {
    js::Pat::Ident(i) => i.type_ann.as_ref()?,
    js::Pat::Object(o) => o.type_ann.as_ref()?,
    _ => return None,
  };
  match &*ann.type_ann {
    js::TsType::TsTypeRef(js::TsTypeRef { type_name: js::TsEntityName::Ident(i), .. }) => Some(i.sym.to_string()),
    _ => None,
  }
}

/// `revalidatePath(...)` and `revalidateTag(...)` statements: an FSR action
/// revalidates the route by default.
struct Revalidates {
  found: Vec<Span>,
}

impl Visit for Revalidates {
  fn visit_expr_stmt(&mut self, n: &js::ExprStmt) {
    let expr = match &*n.expr {
      js::Expr::Await(a) => &*a.arg,
      other => other,
    };
    if let js::Expr::Call(js::CallExpr { callee: js::Callee::Expr(callee), .. }) = expr {
      if matches!(&**callee, js::Expr::Ident(i) if ["revalidatePath", "revalidateTag"].contains(&&*i.sym)) {
        self.found.push(n.span);
        return;
      }
    }
    n.visit_children_with(self);
  }
}

/// Each call of an imported action by the callee's span; each other use apart.
struct Uses<'a> {
  locals: &'a [(String, String)],
  calls: Vec<(Span, String)>,
  other: Vec<(Span, String)>,
}

impl Visit for Uses<'_> {
  fn visit_call_expr(&mut self, n: &js::CallExpr) {
    if let js::Callee::Expr(callee) = &n.callee {
      if let js::Expr::Ident(i) = &**callee {
        if let Some((_, imported)) = self.locals.iter().find(|(local, _)| *local == *i.sym) {
          self.calls.push((i.span, imported.clone()));
          for arg in &n.args {
            arg.visit_with(self);
          }
          return;
        }
      }
    }
    n.visit_children_with(self);
  }

  fn visit_import_decl(&mut self, _: &js::ImportDecl) {}

  fn visit_ident(&mut self, n: &js::Ident) {
    if self.locals.iter().any(|(local, _)| *local == *n.sym) {
      self.other.push((n.span, n.sym.to_string()));
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

  #[test]
  fn a_server_action_is_an_fsr_action_its_input_a_schema_and_its_callers_reach_it_through_actions() {
    let actions = "\"use server\";\n\nimport { kv } from \"@vercel/kv\";\nimport { revalidatePath } from \"next/cache\";\nimport { Feature } from \"./types\";\n\nexport async function upvote(feature: Feature) {\n  await kv.hset(`item:${feature.id}`, feature);\n  revalidatePath(\"/\");\n}\n\nexport async function saveEmail(formData: FormData) {\n  await kv.sadd(\"emails\", formData.get(\"email\"));\n}\n";
    let types = "export type Feature = {\n  id: string;\n  title: string;\n};\n";
    let form = "\"use client\";\nimport { upvote } from \"./actions\";\nimport { Feature } from \"./types\";\n\nexport default function Item({ feature }: { feature: Feature }) {\n  return <button onClick={() => upvote(feature)}>up</button>;\n}\n";
    let page = "import Item from \"./item\";\n\nexport default function Page() {\n  return <Item feature={{ id: \"1\", title: \"t\" }} />;\n}\n";
    let input = tree(&[
      ("package.json", r#"{ "dependencies": { "next": "latest" } }"#),
      ("app/actions.ts", actions),
      ("app/types.ts", types),
      ("app/item.tsx", form),
      ("app/page.tsx", page),
    ]);
    let (out, report) = migrate(&input);
    assert_eq!(out.text("app/schemas/feature.ts").unwrap(), "export interface Feature {\n  id: string;\n  title: string;\n}\n");
    assert_eq!(
      out.text("app/routes/actions.ts").unwrap(),
      "import { action } from \"@snapfire/fsr\";\nimport type { ActionCtx } from \"@snapfire/fsr\";\nimport type { Feature } from \"@schemas/feature\";\nimport { kv } from \"@vercel/kv\";\n\nexport const upvote = action(async ({ input }: ActionCtx<Feature>) => {\n  const feature = input;\n  await kv.hset(`item:${feature.id}`, feature);\n});\n"
    );
    assert!(out.text("app/routes/item.tsx").unwrap().contains("onClick={() => actions.$root.upvote(feature)}"), "{}", out.text("app/routes/item.tsx").unwrap());
    assert!(out.text("app/routes/item.tsx").unwrap().starts_with("import { actions } from \"@generated/client\";\n"));
    assert!(!out.text("app/routes/actions.ts").unwrap().contains("saveEmail"));
    let left = out.text("app/routes/server-actions.ts").expect("the action left in place moves out of actions.ts");
    assert!(left.starts_with("\"use server\";") && left.contains("export async function saveEmail"), "{left}");
    assert!(report.notes.iter().any(|n| n.message.starts_with("`saveEmail` takes a `FormData`")), "{:?}", report.notes);
  }
}
