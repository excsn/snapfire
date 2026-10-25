//! A Prisma application's database calls as a Rust `#[service]`: the models
//! in `schema.prisma` as records, each distinct call a method whose body is
//! left for a person, the host crate that serves it and the loaders calling
//! `services.db`.

use std::collections::BTreeMap;

use swc_core::common::{Span, Spanned};
use swc_core::ecma::ast as js;
use swc_core::ecma::visit::{Visit, VisitWith};

use crate::edit::Edits;
use crate::items::{local, tidy};
use crate::syntax::{is_script, parse, Parsed};
use crate::{Pass, Report, Tree};

pub(crate) struct Prisma;

/// One `model` of `schema.prisma`.
struct Model {
  name: String,
  fields: Vec<ModelField>,
}

struct ModelField {
  name: String,
  ty: String,
  optional: bool,
  list: bool,
  /// `@relation(fields: [...])`: this side holds the foreign key.
  holds_key: bool,
}

/// One distinct call the loaders make, as the method that answers it.
struct Call {
  method: String,
  model: String,
  op: String,
  /// Each request-time value the call passes, as the field it is keyed by.
  params: Vec<String>,
  written: String,
}

impl Pass for Prisma {
  fn name(&self) -> &'static str {
    "prisma"
  }

  fn run(&self, tree: &mut Tree, report: &mut Report) {
    let Some(schema_path) = tree.paths().find(|p| p.starts_with("app/") && p.ends_with("schema.prisma")).map(str::to_owned) else { return };
    let models = models(tree.text(&schema_path).unwrap_or_default());
    if models.is_empty() {
      return;
    }
    let clients: Vec<String> = tree.paths().filter(|p| p.starts_with("app/") && is_script(p) && tree.text(p).is_some_and(|t| t.contains("new PrismaClient"))).map(str::to_owned).collect();
    let loaders: Vec<String> = tree.paths().filter(|p| p.starts_with("app/routes/") && (p.contains(".loader.") || p.ends_with("/actions.ts"))).map(str::to_owned).collect();
    let mut calls: BTreeMap<String, Call> = BTreeMap::new();
    let mut left = 0;
    for path in loaders {
      let Some(text) = tree.text(&path).map(str::to_owned) else { continue };
      let Ok(parsed) = parse(&path, &text) else { continue };
      let Some(client) = prisma_local(&path, &parsed, &clients) else { continue };
      let mut found = Calls { client: &client, found: Vec::new() };
      parsed.module.visit_with(&mut found);
      if found.found.is_empty() {
        continue;
      }
      let mut edits = Edits::default();
      for (span, model, op, args) in found.found {
        let line = parsed.line(span);
        let Some(model) = models.iter().find(|m| m.name.eq_ignore_ascii_case(&model)) else { continue };
        let Some(passed) = args.as_ref().map_or(Some(Vec::new()), |a| request_values(a, model, &models, &text, &parsed)) else {
          report.note(&path, line, format!("`{client}.{}.{op}` passes a value read at request time other than as a field of `where` or `data`; its method was not generated", lower_first(&model.name)));
          left += 1;
          continue;
        };
        let includes = args.as_ref().map(includes).unwrap_or_default();
        let mut method = format!("{}_{}", snake(&model.name), snake(&op));
        let by: Vec<String> = passed.iter().filter(|p| p.under_where).map(|p| snake(&p.field)).collect();
        if !by.is_empty() {
          method.push_str(&format!("_by_{}", by.join("_and_")));
        }
        if !includes.is_empty() {
          method.push_str(&format!("_with_{}", includes.iter().map(|i| snake(i)).collect::<Vec<_>>().join("_and_")));
        }
        let params: Vec<String> = passed.iter().map(|p| p.field.clone()).collect();
        if calls.get(&method).is_some_and(|c| c.params != params) {
          report.note(&path, line, format!("`{client}.{}.{op}` passes other fields than a call answered by `{method}`; its method was not generated", lower_first(&model.name)));
          left += 1;
          continue;
        }
        let written = text[parsed.range(span)].to_owned();
        let passing = passed.iter().map(|p| if p.value == p.field { p.field.clone() } else { format!("{}: {}", p.field, p.value) }).collect::<Vec<_>>().join(", ");
        let passing = if passing.is_empty() { String::new() } else { format!("{{ {passing} }}") };
        edits.replace(parsed.range(span), format!("services.db.{}({passing})", camel(&method)));
        calls.entry(method.clone()).or_insert(Call { method, model: model.name.clone(), op, params, written });
      }
      if edits.is_empty() {
        continue;
      }
      let mut rewritten = edits.apply(&text);
      rewritten = if path.ends_with("/actions.ts") { with_action_services(&path, &rewritten) } else { with_services(&path, &rewritten) };
      rewritten = crate::items::prune(&path, &rewritten, &[client.clone()].into_iter().collect());
      tree.insert(path, tidy(&rewritten));
    }
    if calls.is_empty() {
      return;
    }
    tree.insert("src/db.rs", service_source(&models, &calls));
    tree.insert("src/main.rs", MAIN.to_owned());
    tree.insert("Cargo.toml", CARGO.to_owned());
    report.note("src/db.rs", 0, format!("each of the {} methods holds the Prisma call it answers in its doc comment and a `todo!()` for its query; write it with sqlx or the database crate of your choice, then `cargo run` serves the application", calls.len()));
    if left > 0 {
      for path in &clients {
        report.note(path, 0, format!("still imported by the {left} Prisma calls the pass left; remove it with `schema.prisma` once each is a method in `src/db.rs`"));
      }
      retarget_types(tree);
      return;
    }
    for path in clients.iter().chain(std::iter::once(&schema_path)) {
      tree.remove(path);
    }
    let prisma_dir = schema_path.rsplit_once('/').map(|(d, _)| format!("{d}/")).unwrap_or_default();
    let tooling: Vec<String> = tree.paths().filter(|p| (p.starts_with(&prisma_dir) && !prisma_dir.is_empty()) || p.ends_with("prisma.config.ts")).map(str::to_owned).collect();
    for path in tooling {
      tree.remove(&path);
    }
    retarget_types(tree);
  }
}

fn models(schema: &str) -> Vec<Model> {
  let mut out = Vec::new();
  let mut current: Option<Model> = None;
  for line in schema.lines() {
    let line = line.split("//").next().unwrap_or("").trim();
    if let Some(rest) = line.strip_prefix("model ") {
      current = Some(Model { name: rest.trim_end_matches('{').trim().to_owned(), fields: Vec::new() });
      continue;
    }
    if line == "}" {
      if let Some(model) = current.take() {
        out.push(model);
      }
      continue;
    }
    let Some(model) = current.as_mut() else { continue };
    if line.is_empty() || line.starts_with("@@") {
      continue;
    }
    let mut parts = line.split_whitespace();
    let (Some(name), Some(ty)) = (parts.next(), parts.next()) else { continue };
    let list = ty.ends_with("[]");
    let optional = ty.ends_with('?');
    let ty = ty.trim_end_matches("[]").trim_end_matches('?').to_owned();
    let holds_key = line.contains("@relation(") && line.contains("fields:");
    model.fields.push(ModelField { name: name.to_owned(), ty, optional, list, holds_key });
  }
  out
}

/// The name the loader binds the Prisma client to: an import of a module
/// that makes one.
fn prisma_local(path: &str, parsed: &Parsed, clients: &[String]) -> Option<String> {
  for item in &parsed.module.body {
    let js::ModuleItem::ModuleDecl(js::ModuleDecl::Import(import)) = item else { continue };
    let spec = import.src.value.to_string_lossy().into_owned();
    let base = match spec.strip_prefix("@src/") {
      Some(rest) => format!("app/src/{rest}"),
      None => crate::relocate::normalize(crate::relocate::dirname(path), &spec).unwrap_or_default(),
    };
    let made_here = clients.iter().any(|c| c == &base || c.strip_prefix(&base).is_some_and(|ext| ext.starts_with('.')));
    if made_here {
      if let Some(found) = import.specifiers.first() {
        return Some(local(found));
      }
    }
  }
  None
}

/// `{ include: { category: true } }` as the relations it includes.
fn includes(args: &js::Expr) -> Vec<String> {
  let js::Expr::Object(object) = args else { return Vec::new() };
  let mut out = Vec::new();
  for prop in &object.props {
    let js::PropOrSpread::Prop(prop) = prop else { continue };
    let js::Prop::KeyValue(kv) = &**prop else { continue };
    if !matches!(&kv.key, js::PropName::Ident(i) if &*i.sym == "include") {
      continue;
    }
    if let js::Expr::Object(included) = &*kv.value {
      for p in &included.props {
        if let js::PropOrSpread::Prop(p) = p {
          if let js::Prop::KeyValue(js::KeyValueProp { key: js::PropName::Ident(i), .. }) = &**p {
            out.push(i.sym.to_string());
          }
        }
      }
    }
  }
  out
}

/// A request-time value a call passes as a field of `where` or `data`.
struct Passed {
  field: String,
  /// The value's source text.
  value: String,
  under_where: bool,
}

/// The request-time values `args` passes, each directly under `where` or
/// `data` and keyed by a scalar field of `model`; None when one sits
/// anywhere else, such as `{ title: { contains: q } }` or `take: n`.
fn request_values(args: &js::Expr, model: &Model, models: &[Model], text: &str, parsed: &Parsed) -> Option<Vec<Passed>> {
  let js::Expr::Object(object) = args else { return literal(args).then(Vec::new) };
  let scalar = |name: &str| model.fields.iter().any(|f| f.name == name && !models.iter().any(|m| m.name == f.ty));
  let mut out: Vec<Passed> = Vec::new();
  for prop in &object.props {
    let js::PropOrSpread::Prop(prop) = prop else { return None };
    let js::Prop::KeyValue(kv) = &**prop else { return None };
    let section = match &kv.key {
      js::PropName::Ident(i) => i.sym.to_string(),
      _ => return None,
    };
    if literal(&kv.value) {
      continue;
    }
    if section != "where" && section != "data" {
      return None;
    }
    let js::Expr::Object(fields) = &*kv.value else { return None };
    for field in &fields.props {
      let js::PropOrSpread::Prop(field) = field else { return None };
      let (name, value) = match &**field {
        js::Prop::Shorthand(i) => (i.sym.to_string(), i.sym.to_string()),
        js::Prop::KeyValue(kv) => {
          let js::PropName::Ident(key) = &kv.key else { return None };
          if literal(&kv.value) {
            continue;
          }
          if matches!(&*kv.value, js::Expr::Object(_) | js::Expr::Array(_)) {
            return None;
          }
          (key.sym.to_string(), text[parsed.range(kv.value.span())].to_owned())
        }
        _ => return None,
      };
      if !scalar(&name) || out.iter().any(|p| p.field == name) {
        return None;
      }
      out.push(Passed { field: name, value, under_where: section == "where" });
    }
  }
  Some(out)
}

fn literal(e: &js::Expr) -> bool {
  match e {
    js::Expr::Lit(_) => true,
    js::Expr::Object(o) => o.props.iter().all(|p| match p {
      js::PropOrSpread::Prop(p) => matches!(&**p, js::Prop::KeyValue(kv) if literal(&kv.value)),
      js::PropOrSpread::Spread(_) => false,
    }),
    js::Expr::Array(a) => a.elems.iter().flatten().all(|e| e.spread.is_none() && literal(&e.expr)),
    js::Expr::Paren(p) => literal(&p.expr),
    _ => false,
  }
}

/// Each `action(async (ctx) => ...)` whose body calls a service with
/// `services` added to what it destructures from its context.
fn with_action_services(path: &str, text: &str) -> String {
  struct Actions<'a> {
    text: &'a str,
    parsed: &'a Parsed,
    edits: Edits,
  }
  impl Visit for Actions<'_> {
    fn visit_call_expr(&mut self, n: &js::CallExpr) {
      let is_action = matches!(&n.callee, js::Callee::Expr(e) if matches!(&**e, js::Expr::Ident(i) if &*i.sym == "action"));
      let arrow = n.args.first().and_then(|a| match &*a.expr {
        js::Expr::Arrow(arrow) => Some(arrow),
        _ => None,
      });
      if let (true, Some(arrow)) = (is_action, arrow) {
        if self.text[self.parsed.range(arrow.body.span())].contains("services.") {
          match arrow.params.first() {
            Some(js::Pat::Object(obj)) if !obj.props.iter().any(|p| matches!(p, js::ObjectPatProp::Assign(a) if &*a.key.sym == "services")) => {
              if let Some(last) = obj.props.last() {
                let at = self.parsed.range(last.span()).end;
                self.edits.replace(at..at, ", services");
              }
            }
            Some(js::Pat::Ident(ctx)) => self.edits.replace(self.parsed.range(ctx.id.span), "{ services }"),
            _ => {}
          }
        }
      }
      n.visit_children_with(self);
    }
  }
  let Ok(parsed) = parse(path, text) else { return text.to_owned() };
  let mut actions = Actions { text, parsed: &parsed, edits: Edits::default() };
  parsed.module.visit_with(&mut actions);
  actions.edits.apply(text)
}

/// `load()` or `load({ params }: Ctx)` with `services` added to what it
/// reads and the context typed.
fn with_services(path: &str, text: &str) -> String {
  if text.contains("export async function load({ services") {
    return text.to_owned();
  }
  let ctx = crate::lift::ctx_type(path);
  let import = if text.contains("import type { Ctx }") { "" } else { "import type { Ctx } from \"@snapfire/fsr/host\";\n" };
  if let Some(at) = text.find("export async function load() {") {
    return format!("{import}{}export async function load({{ services }}: {ctx}) {{{}", &text[..at], &text[at + "export async function load() {".len()..]);
  }
  if let Some(at) = text.find("export async function load({ ") {
    return format!("{import}{}export async function load({{ services, {}", &text[..at], &text[at + "export async function load({ ".len()..]);
  }
  text.to_owned()
}

/// Imports of the Prisma client's generated types, now the records `fsr
/// build` generates from the service under the same names.
fn retarget_types(tree: &mut Tree) {
  let scripts: Vec<String> = tree.paths().filter(|p| p.starts_with("app/") && is_script(p)).map(str::to_owned).collect();
  for path in scripts {
    let Some(text) = tree.text(&path).map(str::to_owned) else { continue };
    if !text.contains("prisma") {
      continue;
    }
    let Ok(parsed) = parse(&path, &text) else { continue };
    let mut edits = Edits::default();
    for item in &parsed.module.body {
      let js::ModuleItem::ModuleDecl(js::ModuleDecl::Import(import)) = item else { continue };
      let spec = import.src.value.to_string_lossy();
      if import.type_only && (spec == "@prisma/client" || spec.contains("generated/prisma")) {
        edits.replace(parsed.range(import.src.span), "\"@generated/client\"");
      }
    }
    if !edits.is_empty() {
      tree.insert(path, edits.apply(&text));
    }
  }
}

fn rust_type(field: &ModelField, models: &[Model]) -> String {
  let scalar = match field.ty.as_str() {
    "Int" => "i32".to_owned(),
    "BigInt" => "i64".to_owned(),
    "Float" => "f64".to_owned(),
    "Boolean" => "bool".to_owned(),
    "String" | "Decimal" | "DateTime" => "String".to_owned(),
    other if models.iter().any(|m| m.name == other) => other.to_owned(),
    _ => "String".to_owned(),
  };
  match (field.list, field.optional) {
    (true, _) => format!("Vec<{scalar}>"),
    (false, true) => format!("Option<{scalar}>"),
    (false, false) => scalar,
  }
}

/// One record per model, so a page imports the model's name from
/// `@generated/client` as it imported it from `@prisma/client`. A relation
/// is a `Vec` or an `Option` that is empty unless the call includes it. Of a
/// one-to-one pair only the side holding the key keeps the field, since the
/// record would otherwise contain itself.
fn service_source(models: &[Model], calls: &BTreeMap<String, Call>) -> String {
  let mut out = String::from("use snapfire_fsr_macros::{service, Record};\nuse snapfire_fsr_runtime::ServiceError;\n\n");
  let single_back = |from: &str, to: &str| models.iter().find(|m| m.name == to).is_some_and(|m| m.fields.iter().any(|f| f.ty == from && !f.list));
  for model in models {
    out.push_str(&format!("/// `model {}` from `schema.prisma`.\n#[derive(Record, Clone)]\npub struct {} {{\n", model.name, model.name));
    for field in &model.fields {
      let relation = models.iter().any(|m| m.name == field.ty);
      if relation && !field.list && !field.holds_key && single_back(&model.name, &field.ty) {
        continue;
      }
      let ty = if relation && !field.list { format!("Option<{}>", field.ty) } else { rust_type(field, models) };
      let decimal = if field.ty == "Decimal" { "  /// A `Decimal`: the value model has no decimal, so it travels as its text.\n" } else { "" };
      out.push_str(&format!("{decimal}  pub {}: {ty},\n", snake(&field.name)));
    }
    out.push_str("}\n\n");
  }
  out.push_str("#[derive(Clone)]\npub struct Db;\n\n#[service]\nimpl Db {\n");
  for call in calls.values() {
    let record = &call.model;
    let returns = match call.op.as_str() {
      "findMany" => format!("Vec<{record}>"),
      "findUnique" | "findFirst" => format!("Option<{record}>"),
      "count" => "i64".to_owned(),
      _ => record.clone(),
    };
    let written = call.written.split_whitespace().collect::<Vec<_>>().join(" ").replace(", }", " }");
    let model = models.iter().find(|m| m.name == call.model);
    let params: String = call.params.iter().filter_map(|p| model?.fields.iter().find(|f| &f.name == p)).map(|f| format!(", {}: {}", snake(&f.name), rust_type(f, models))).collect();
    out.push_str(&format!("  /// `{written}`\n  pub fn {}(&self{params}) -> Result<{returns}, ServiceError> {{\n    todo!(\"the query the doc comment names\")\n  }}\n\n", call.method));
  }
  out.truncate(out.trim_end().len());
  out.push_str("\n}\n");
  out
}

const MAIN: &str = "mod db;\n\nuse std::sync::Arc;\n\nuse snapfire_fsr_host::Host;\n\n#[tokio::main]\nasync fn main() -> std::io::Result<()> {\n  let host = Host::from(env!(\"CARGO_MANIFEST_DIR\")).map(|b| b.service(Arc::new(db::Db))).and_then(|b| b.build()).map_err(std::io::Error::other)?;\n  let host = Arc::new(host);\n  print!(\"{}\", host.report());\n  let listen = host.listen().to_owned();\n  host.serve(&listen).await\n}\n";

const CARGO: &str = "[package]\nname = \"app\"\nversion = \"0.1.0\"\nedition = \"2024\"\npublish = false\n\n[dependencies]\nsnapfire_fsr_core = \"0\"\nsnapfire_fsr_host = \"0\"\nsnapfire_fsr_macros = \"0\"\nsnapfire_fsr_runtime = \"0\"\nsnapfire_fsr_service = \"0\"\ntokio = { version = \"1\", features = [\"rt-multi-thread\", \"macros\"] }\n";

struct Calls<'a> {
  client: &'a str,
  /// Each `client.<model>.<op>(args)`: its span, the model, the operation and its argument.
  found: Vec<(Span, String, String, Option<js::Expr>)>,
}

impl Visit for Calls<'_> {
  fn visit_call_expr(&mut self, n: &js::CallExpr) {
    if let js::Callee::Expr(callee) = &n.callee {
      if let js::Expr::Member(op) = &**callee {
        if let (js::Expr::Member(model), js::MemberProp::Ident(op_name)) = (&*op.obj, &op.prop) {
          if let (js::Expr::Ident(client), js::MemberProp::Ident(model_name)) = (&*model.obj, &model.prop) {
            if *client.sym == *self.client {
              self.found.push((n.span, model_name.sym.to_string(), op_name.sym.to_string(), n.args.first().map(|a| (*a.expr).clone())));
              return;
            }
          }
        }
      }
    }
    n.visit_children_with(self);
  }
}

fn snake(name: &str) -> String {
  let mut out = String::new();
  for (i, c) in name.chars().enumerate() {
    if c.is_uppercase() && i > 0 {
      out.push('_');
    }
    out.extend(c.to_lowercase());
  }
  out
}

fn camel(name: &str) -> String {
  let mut out = String::new();
  let mut upper = false;
  for c in name.chars() {
    if c == '_' {
      upper = true;
    } else if upper {
      out.extend(c.to_uppercase());
      upper = false;
    } else {
      out.push(c);
    }
  }
  out
}

fn lower_first(s: &str) -> String {
  let mut c = s.chars();
  c.next().map(|f| f.to_lowercase().collect::<String>() + c.as_str()).unwrap_or_default()
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

  const SCHEMA: &str = "datasource db {\n  provider = \"mysql\"\n}\n\nmodel Product {\n  id          Int       @id @default(autoincrement())\n  name        String\n  price       Decimal\n  category    Category @relation(fields: [categoryId], references: [id])\n  categoryId  Int\n\n  @@index([categoryId])\n}\n\nmodel Category {\n  id       Int       @id\n  name     String\n  products Product[]\n}\n";

  #[test]
  fn a_prisma_call_a_page_awaits_is_a_service_method_and_the_models_are_records() {
    let input = tree(&[
      ("package.json", r#"{ "dependencies": { "next": "latest", "@prisma/client": "6" } }"#),
      ("prisma/schema.prisma", SCHEMA),
      ("prisma.config.ts", "export default {};\n"),
      ("lib/prisma.ts", "import { PrismaClient } from \"@prisma/client\";\nconst prisma = new PrismaClient();\nexport default prisma;\n"),
      ("app/page.tsx", "import prisma from \"@/lib/prisma\";\n\nexport default async function Home() {\n  const products = await prisma.product.findMany({ include: { category: true } });\n  return <ul>{products.map((p) => <li key={p.id}>{p.name}</li>)}</ul>;\n}\n"),
      ("components/Product.tsx", "import type { Product as P } from \"@prisma/client\";\n\nexport function Product({ product }: { product: P }) {\n  return <p>{product.name}</p>;\n}\n"),
      ("tsconfig.json", "{ \"compilerOptions\": { \"paths\": { \"@/*\": [\"./*\"] } } }"),
    ]);
    let (out, _) = migrate(&input);
    assert_eq!(
      out.text("app/routes/page.loader.ts").unwrap(),
      "import type { Ctx } from \"@snapfire/fsr/host\";\n\nexport async function load({ services }: Ctx<\"/\">) {\n  const products = await services.db.productFindManyWithCategory();\n  return { products };\n}\n"
    );
    let db = out.text("src/db.rs").unwrap();
    for expected in [
      "pub struct Product {\n  pub id: i32,\n  pub name: String,\n  /// A `Decimal`: the value model has no decimal, so it travels as its text.\n  pub price: String,\n  pub category: Option<Category>,\n  pub category_id: i32,\n}",
      "pub struct Category {\n  pub id: i32,\n  pub name: String,\n  pub products: Vec<Product>,\n}",
      "  /// `prisma.product.findMany({ include: { category: true } })`\n  pub fn product_find_many_with_category(&self) -> Result<Vec<Product>, ServiceError> {",
    ] {
      assert!(db.contains(expected), "missing {expected} in {db}");
    }
    assert!(out.contains("src/main.rs") && out.contains("Cargo.toml"));
    assert!(!out.paths().any(|p| p.contains("prisma")), "{:?}", out.paths().collect::<Vec<_>>());
    assert!(out.text("app/src/components/Product.tsx").unwrap().contains("from \"@generated/client\""));
  }

  #[test]
  fn a_request_time_value_under_where_or_data_is_a_parameter_of_the_method() {
    let input = tree(&[
      ("package.json", r#"{ "dependencies": { "next": "latest", "@prisma/client": "6" } }"#),
      ("prisma/schema.prisma", "model Post {\n  id       Int     @id\n  title    String\n  author   User    @relation(fields: [authorId], references: [id])\n  authorId Int\n}\n\nmodel User {\n  id    Int    @id\n  email String @unique\n  posts Post[]\n}\n"),
      ("lib/prisma.ts", "import { PrismaClient } from \"@prisma/client\";\nconst prisma = new PrismaClient();\nexport default prisma;\n"),
      ("app/posts/[id]/page.tsx", "import prisma from \"@/lib/prisma\";\n\nexport default async function Post({ params }) {\n  const { id } = await params;\n  const postId = parseInt(id);\n  const post = await prisma.post.findUnique({ where: { id: postId }, include: { author: true } });\n  return <h1>{post.title}</h1>;\n}\n"),
      ("app/users/[email]/page.tsx", "import prisma from \"@/lib/prisma\";\n\nexport default async function User({ params }) {\n  const { email } = await params;\n  const user = await prisma.user.findUnique({ where: { email } });\n  const found = await prisma.post.findMany({ where: { title: { contains: email } } });\n  return <h1>{user.email}{found.length}</h1>;\n}\n"),
      ("tsconfig.json", "{ \"compilerOptions\": { \"paths\": { \"@/*\": [\"./*\"] } } }"),
    ]);
    let (out, report) = migrate(&input);
    let post = out.text("app/routes/posts/[id]/page.loader.ts").unwrap();
    assert!(post.contains("const post = await services.db.postFindUniqueByIdWithAuthor({ id: postId });"), "{post}");
    let user = out.text("app/routes/users/[email]/page.loader.ts").unwrap();
    assert!(user.contains("const user = await services.db.userFindUniqueByEmail({ email });"), "{user}");
    let db = out.text("src/db.rs").unwrap();
    for expected in [
      "  /// `prisma.post.findUnique({ where: { id: postId }, include: { author: true } })`\n  pub fn post_find_unique_by_id_with_author(&self, id: i32) -> Result<Option<Post>, ServiceError> {",
      "  pub fn user_find_unique_by_email(&self, email: String) -> Result<Option<User>, ServiceError> {",
    ] {
      assert!(db.contains(expected), "missing {expected} in {db}");
    }
    assert!(!db.contains("post_find_many"), "{db}");
    assert!(report.notes.iter().any(|n| n.file == "app/routes/users/[email]/page.loader.ts" && n.message.contains("`prisma.post.findMany`")), "{:?}", report.notes);
  }

  #[test]
  fn a_prisma_call_in_an_action_is_a_service_method_the_action_reaches_through_services() {
    let input = tree(&[
      ("package.json", r#"{ "dependencies": { "next": "latest", "@prisma/client": "6" } }"#),
      ("prisma/schema.prisma", "model Post {\n  id    Int    @id\n  title String\n}\n"),
      ("lib/prisma.ts", "import { PrismaClient } from \"@prisma/client\";\nconst prisma = new PrismaClient();\nexport default prisma;\n"),
      ("app/posts/[id]/page.tsx", "import prisma from \"@/lib/prisma\";\n\nexport default async function Post({ params }) {\n  const { id } = await params;\n  const postId = parseInt(id);\n  const post = await prisma.post.findUnique({ where: { id: postId } });\n\n  async function deletePost() {\n    \"use server\";\n    await prisma.post.delete({ where: { id: postId } });\n  }\n\n  return (\n    <form action={deletePost}>\n      <button type=\"submit\">Delete {post.title}</button>\n    </form>\n  );\n}\n"),
      ("tsconfig.json", "{ \"compilerOptions\": { \"paths\": { \"@/*\": [\"./*\"] } } }"),
    ]);
    let (out, _) = migrate(&input);
    assert_eq!(
      out.text("app/routes/posts/[id]/actions.ts").unwrap(),
      "import { action } from \"@snapfire/fsr/host\";\nimport type { ActionCtx } from \"@snapfire/fsr/host\";\nimport type { DeletePostInput } from \"@schemas/deletepostinput\";\n\nexport const deletePost = action(async ({ input, services }: ActionCtx<DeletePostInput>) => {\n  const { postId } = input;\n  await services.db.postDeleteById({ id: postId });\n});\n"
    );
    let db = out.text("src/db.rs").unwrap();
    assert!(db.contains("  pub fn post_delete_by_id(&self, id: i32) -> Result<Post, ServiceError> {"), "{db}");
    assert!(db.contains("  pub fn post_find_unique_by_id(&self, id: i32) -> Result<Option<Post>, ServiceError> {"), "{db}");
  }

  #[test]
  fn the_client_module_stays_while_a_call_the_pass_left_still_imports_it() {
    let input = tree(&[
      ("package.json", r#"{ "dependencies": { "next": "latest", "@prisma/client": "6" } }"#),
      ("prisma/schema.prisma", "model Post {\n  id    Int    @id\n  title String\n}\n"),
      ("lib/prisma.ts", "import { PrismaClient } from \"@prisma/client\";\nconst prisma = new PrismaClient();\nexport default prisma;\n"),
      ("app/page.tsx", "import prisma from \"@/lib/prisma\";\n\nexport default async function Home({ searchParams }) {\n  const { q } = await searchParams;\n  const posts = await prisma.post.findMany();\n  const found = await prisma.post.findMany({ where: { title: { contains: q } } });\n  return <p>{posts.length}{found.length}</p>;\n}\n"),
      ("tsconfig.json", "{ \"compilerOptions\": { \"paths\": { \"@/*\": [\"./*\"] } } }"),
    ]);
    let (out, report) = migrate(&input);
    let loader = out.text("app/routes/page.loader.ts").unwrap();
    assert!(loader.contains("import prisma from \"@src/lib/prisma\";") && loader.contains("services.db.postFindMany()"), "{loader}");
    assert!(out.contains("app/src/lib/prisma.ts") && out.contains("app/src/prisma/schema.prisma"), "{:?}", out.paths().collect::<Vec<_>>());
    assert!(report.notes.iter().any(|n| n.file == "app/src/lib/prisma.ts" && n.message.contains("still")), "{:?}", report.notes);
  }
}
