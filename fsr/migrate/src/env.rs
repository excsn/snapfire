//! `process.env` reads. A `NEXT_PUBLIC_` variable a page or layout reads
//! becomes a `[public]` key its route's loader passes as a prop; every
//! other read is a note.

use std::collections::BTreeSet;

use swc_core::common::{Span, Spanned};
use swc_core::ecma::ast as js;
use swc_core::ecma::visit::{Visit, VisitWith};

use crate::edit::Edits;
use crate::items::{line_range, tidy};
use crate::lift::{ctx_type, default_function, props_name, route_id, route_kind};
use crate::relocate::dirname;
use crate::syntax::{is_script, parse, Parsed};
use crate::{Pass, Report, Tree};

pub(crate) struct PublicEnv;

impl Pass for PublicEnv {
  fn name(&self) -> &'static str {
    "public-env"
  }

  fn run(&self, tree: &mut Tree, report: &mut Report) {
    let scripts: Vec<String> = tree.paths().filter(|p| p.starts_with("app/") && is_script(p)).map(str::to_owned).collect();
    let mut keys = BTreeSet::new();
    for path in scripts {
      let Some(text) = tree.text(&path).map(str::to_owned) else { continue };
      if !text.contains("process") {
        continue;
      }
      let Ok(parsed) = parse(&path, &text) else { continue };
      let mut reads = EnvReads { found: Vec::new() };
      parsed.module.visit_with(&mut reads);
      if reads.found.is_empty() {
        continue;
      }
      let mut public = Vec::new();
      for read in reads.found {
        match read.name.strip_prefix("NEXT_PUBLIC_").map(str::to_lowercase) {
          Some(key) => public.push((read, key)),
          None => report.note(&path, parsed.line(read.span), format!("`{}` is a server-only variable; FSR keeps a deployment value a body reads under `[public]` and a secret under `[secrets]`, which only a client's credential sends, so it was left for you", read.name)),
        }
      }
      if public.is_empty() {
        continue;
      }
      let loader = path.ends_with(".loader.ts") || path.ends_with(".loader.js");
      let converted = match route_kind(&path) {
        Some(kind) => route_component(tree, &path, kind, &text, &parsed, &public, report),
        None if loader => loader_reads(tree, &path, &text, &parsed, &public, report),
        None => None,
      };
      match converted {
        Some(used) => keys.extend(used),
        None if route_kind(&path).is_none() && !loader => {
          for (read, _) in &public {
            report.note(&path, parsed.line(read.span), format!("`{}` is a public variable read below a route's page or layout; FSR passes a `[public]` value down from the route's loader, so it was left for you", read.name));
          }
        }
        None => {}
      }
    }
    if !keys.is_empty() {
      report.config.push(format!("[public]\n{}", keys.iter().map(|k| format!("{k} = \"\"\n")).collect::<String>()));
    }
  }
}

/// `process.env.NAME` or `process.env["NAME"]`, by `NAME`.
fn env_name(e: &js::MemberExpr) -> Option<String> {
  let js::Expr::Member(inner) = &*e.obj else { return None };
  let js::Expr::Ident(process) = &*inner.obj else { return None };
  let js::MemberProp::Ident(env) = &inner.prop else { return None };
  if &*process.sym != "process" || &*env.sym != "env" {
    return None;
  }
  match &e.prop {
    js::MemberProp::Ident(name) => Some(name.sym.to_string()),
    js::MemberProp::Computed(c) => match &*c.expr {
      js::Expr::Lit(js::Lit::Str(s)) => Some(s.value.to_string_lossy().into_owned()),
      _ => None,
    },
    _ => None,
  }
}

struct EnvRead {
  span: Span,
  name: String,
}

struct EnvReads {
  found: Vec<EnvRead>,
}

impl Visit for EnvReads {
  fn visit_member_expr(&mut self, n: &js::MemberExpr) {
    match env_name(n) {
      Some(name) => self.found.push(EnvRead { span: n.span, name }),
      None => n.visit_children_with(self),
    }
  }
}

fn lower_camel(key: &str) -> String {
  let mut out = String::new();
  for (i, part) in key.split('_').filter(|p| !p.is_empty()).enumerate() {
    let mut chars = part.chars();
    if let Some(first) = chars.next() {
      if i == 0 {
        out.extend(first.to_lowercase());
      } else {
        out.extend(first.to_uppercase());
      }
      out.push_str(chars.as_str());
    }
  }
  out
}

/// `e` without the `as T`, `!` and parentheses around it.
fn unwrapped(e: &js::Expr) -> &js::Expr {
  match e {
    js::Expr::TsAs(a) => unwrapped(&a.expr),
    js::Expr::TsNonNull(n) => unwrapped(&n.expr),
    js::Expr::Paren(p) => unwrapped(&p.expr),
    other => other,
  }
}

/// A page or layout whose `NEXT_PUBLIC_` reads become props its loader
/// passes from `config`, answering the keys it used; `None`, with a note,
/// when a read sits where a prop does not reach or the loader's returns are
/// not object literals.
fn route_component(tree: &mut Tree, path: &str, kind: &str, text: &str, parsed: &Parsed, public: &[(EnvRead, String)], report: &mut Report) -> Option<BTreeSet<String>> {
  let Some((function, _)) = default_function(&parsed.module) else {
    report.note(path, parsed.line(public[0].0.span), "reads a public variable outside a default-exported component, so it was left for you");
    return None;
  };
  let inside = parsed.range(function.span);
  let mut constants: Vec<(Span, String)> = Vec::new();
  for item in &parsed.module.body {
    let js::ModuleItem::Stmt(js::Stmt::Decl(js::Decl::Var(var))) = item else { continue };
    let [decl] = var.decls.as_slice() else { continue };
    let (js::Pat::Ident(name), Some(init)) = (&decl.name, &decl.init) else { continue };
    if let js::Expr::Member(m) = unwrapped(init) {
      if env_name(m).is_some_and(|n| n.starts_with("NEXT_PUBLIC_")) {
        constants.push((var.span, name.id.sym.to_string()));
      }
    }
  }
  let mut fields: Vec<(String, String)> = Vec::new();
  let mut reads: Vec<(Span, String)> = Vec::new();
  for (read, key) in public {
    let at = parsed.range(read.span);
    let prop = match constants.iter().find(|(span, _)| { let r = parsed.range(*span); r.start <= at.start && at.end <= r.end }) {
      Some((_, name)) => name.clone(),
      None if inside.start <= at.start && at.end <= inside.end => {
        let prop = lower_camel(key);
        reads.push((read.span, prop.clone()));
        prop
      }
      None => {
        report.note(path, parsed.line(read.span), format!("`{}` is read outside the component, where a prop does not reach, so it was left for you", read.name));
        return None;
      }
    };
    if !fields.iter().any(|(p, _)| *p == prop) {
      fields.push((prop, key.clone()));
    }
  }

  let loader_path = format!("{}/{kind}.loader.ts", dirname(path));
  let loader = with_fields(tree.text(&loader_path).unwrap_or_default(), &loader_path, &fields)?;

  let typed = path.ends_with(".tsx") || path.ends_with(".ts");
  let id = match (kind, route_id(path).as_str()) {
    ("layout", "$root") => "layout".to_owned(),
    ("layout", id) => format!("{id}.layout"),
    (_, id) => id.to_owned(),
  };
  let props_type = props_name(&id);
  let names: Vec<&str> = fields.iter().map(|(p, _)| p.as_str()).collect();
  let mut edits = Edits::default();
  let param = function.params.first().map(|p| &p.pat);
  let holder = match param {
    Some(js::Pat::Ident(i)) => Some(i.id.sym.to_string()),
    _ => None,
  };
  for (span, prop) in &reads {
    let with = match &holder {
      Some(holder) => format!("{holder}.{prop}"),
      None => prop.clone(),
    };
    edits.replace(parsed.range(*span), with);
  }
  for (span, _) in &constants {
    edits.replace(line_range(text, parsed.range(*span)), "");
  }
  let annotation = |type_ann: &Option<Box<js::TsTypeAnn>>, pattern_end: usize, edits: &mut Edits| {
    if !typed {
      return;
    }
    match type_ann {
      Some(t) if text[parsed.range(t.type_ann.span())].trim() == props_type => {}
      Some(t) => {
        let end = parsed.range(t.span).end;
        edits.replace(end..end, format!(" & {props_type}"));
      }
      None => edits.replace(pattern_end..pattern_end, format!(": {props_type}")),
    }
  };
  match param {
    None => {
      let open = text[inside.clone()].find('(').map(|i| inside.start + i + 1)?;
      let typed = if typed { format!(": {props_type}") } else { String::new() };
      edits.replace(open..open, format!("{{ {} }}{typed}", names.join(", ")));
    }
    Some(js::Pat::Object(obj)) => {
      let pattern = parsed.range(obj.span);
      let have: Vec<String> = obj.props.iter().filter_map(|p| match p {
        js::ObjectPatProp::Assign(a) => Some(a.key.sym.to_string()),
        js::ObjectPatProp::KeyValue(kv) => match &kv.key { js::PropName::Ident(i) => Some(i.sym.to_string()), _ => None },
        js::ObjectPatProp::Rest(_) => None,
      }).collect();
      let added: Vec<&str> = names.iter().copied().filter(|n| !have.iter().any(|h| h == n)).collect();
      match obj.props.last() {
        Some(last) if !added.is_empty() => {
          let end = parsed.range(last.span()).end;
          let line_start = text[..end].rfind('\n').map_or(0, |i| i + 1);
          let indent: String = text[line_start..].chars().take_while(|c| c.is_whitespace()).collect();
          let sep = if text[pattern.start..end].contains('\n') { format!(",\n{indent}") } else { ", ".to_owned() };
          edits.replace(end..end, added.iter().map(|n| format!("{sep}{n}")).collect::<String>());
        }
        None => {
          let open = pattern.start + 1;
          edits.replace(open..open, format!(" {} ", added.join(", ")));
        }
        _ => {}
      }
      let end = obj.type_ann.as_ref().map_or(pattern.end, |t| parsed.range(t.span).start);
      annotation(&obj.type_ann, end, &mut edits);
    }
    Some(js::Pat::Ident(i)) => annotation(&i.type_ann, parsed.range(i.span).end, &mut edits),
    Some(_) => {
      report.note(path, parsed.line(function.span), "the component's props are not a destructuring or a name, so its public variables were left for you");
      return None;
    }
  }
  let mut page = edits.apply(text);
  if typed && !page.lines().any(|l| l.starts_with("import") && l.contains(&props_type)) {
    page.insert_str(0, &format!("import type {{ {props_type} }} from \"@generated/client\";\n"));
  }
  tree.insert(path.to_owned(), tidy(&page));
  tree.insert(loader_path, tidy(&loader));
  Some(fields.into_iter().map(|(_, key)| key).collect())
}

/// `loader` returning each field from `config` beside what it returned, with
/// a `load` written when it has none; `None` when a return of its `load` is
/// not an object literal.
fn with_fields(loader: &str, path: &str, fields: &[(String, String)]) -> Option<String> {
  let entries = |config: &str| fields.iter().map(|(prop, key)| format!("{prop}: {config}.{key}")).collect::<Vec<_>>().join(", ");
  let parsed = parse(path, loader).ok()?;
  let Some((params, body)) = load_function(&parsed.module) else {
    let import = ctx_import(loader);
    let sep = if loader.is_empty() { "" } else { "\n" };
    return Some(format!("{import}{loader}{sep}export async function load({{ config }}: {}) {{\n  return {{ {} }};\n}}\n", ctx_type(path), entries("config")));
  };
  let mut edits = Edits::default();
  let config = config_param(loader, &parsed, &params, body, path, &mut edits)?;
  let mut returns = Returns { found: Vec::new() };
  body.stmts.iter().for_each(|s| returns.stmt(s));
  if returns.found.is_empty() {
    return None;
  }
  for arg in returns.found {
    let js::Expr::Object(obj) = unwrapped(arg?) else { return None };
    match obj.props.last() {
      Some(last) => {
        let end = parsed.range(last.span()).end;
        edits.replace(end..end, format!(", {}", entries(&config)));
      }
      None => edits.replace(parsed.range(obj.span), format!("{{ {} }}", entries(&config))),
    }
  }
  let import = if params.is_empty() { ctx_import(loader) } else { String::new() };
  Some(format!("{import}{}", edits.apply(loader)))
}

fn ctx_import(text: &str) -> String {
  if text.lines().any(|l| l.starts_with("import") && l.contains("Ctx")) { String::new() } else { "import type { Ctx } from \"@snapfire/fsr/host\";\n".to_owned() }
}

/// How `load` reaches `config`, `config` from its destructuring or
/// `ctx.config`, with the edits that give it the field when it had none.
fn config_param(loader: &str, parsed: &Parsed, params: &[&js::Pat], body: &js::FunctionBody, path: &str, edits: &mut Edits) -> Option<String> {
  match params.first() {
    None => {
      let open = loader[..parsed.range(body.span).start].rfind('(')? + 1;
      edits.replace(open..open, format!("{{ config }}: {}", ctx_type(path)));
      Some("config".to_owned())
    }
    Some(js::Pat::Object(obj)) => {
      let has = obj.props.iter().any(|p| matches!(p, js::ObjectPatProp::Assign(a) if &*a.key.sym == "config"));
      if !has {
        match obj.props.last() {
          Some(last) => {
            let end = parsed.range(last.span()).end;
            edits.replace(end..end, ", config");
          }
          None => {
            let open = parsed.range(obj.span).start + 1;
            edits.replace(open..open, " config ");
          }
        }
      }
      Some("config".to_owned())
    }
    Some(js::Pat::Ident(i)) => Some(format!("{}.config", i.id.sym)),
    Some(_) => None,
  }
}

/// A loader whose `load` reads `NEXT_PUBLIC_` variables, each read becoming
/// one of `config`, answering the keys it used.
fn loader_reads(tree: &mut Tree, path: &str, text: &str, parsed: &Parsed, public: &[(EnvRead, String)], report: &mut Report) -> Option<BTreeSet<String>> {
  let Some((params, body)) = load_function(&parsed.module) else {
    report.note(path, parsed.line(public[0].0.span), format!("`{}` is read outside `load`, where `config` does not reach, so it was left for you", public[0].0.name));
    return None;
  };
  let within = parsed.range(body.span);
  if let Some((read, _)) = public.iter().find(|(read, _)| { let at = parsed.range(read.span); at.start < within.start || at.end > within.end }) {
    report.note(path, parsed.line(read.span), format!("`{}` is read outside `load`, where `config` does not reach, so it was left for you", read.name));
    return None;
  }
  let mut edits = Edits::default();
  let config = config_param(text, parsed, &params, body, path, &mut edits)?;
  for (read, key) in public {
    edits.replace(parsed.range(read.span), format!("{config}.{key}"));
  }
  let import = if params.is_empty() { ctx_import(text) } else { String::new() };
  tree.insert(path.to_owned(), tidy(&format!("{import}{}", edits.apply(text))));
  Some(public.iter().map(|(_, key)| key.clone()).collect())
}

/// The parameters and body of the `load` a loader exports.
fn load_function(module: &js::Module) -> Option<(Vec<&js::Pat>, &js::FunctionBody)> {
  for item in &module.body {
    let js::ModuleItem::ModuleDecl(js::ModuleDecl::ExportDecl(export)) = item else { continue };
    match &export.decl {
      js::Decl::Fn(f) if &*f.ident.sym == "load" => return Some((f.function.params.iter().map(|p| &p.pat).collect(), f.function.body.as_ref()?)),
      js::Decl::Var(var) => {
        for decl in &var.decls {
          let (js::Pat::Ident(name), Some(init)) = (&decl.name, &decl.init) else { continue };
          if &*name.id.sym != "load" {
            continue;
          }
          match &**init {
            js::Expr::Arrow(a) => match &*a.body {
              js::ArrowFunctionBody::FunctionBody(b) => return Some((a.params.iter().collect(), b)),
              js::ArrowFunctionBody::Expr(_) => return None,
            },
            js::Expr::Fn(f) => return Some((f.function.params.iter().map(|p| &p.pat).collect(), f.function.body.as_ref()?)),
            _ => return None,
          }
        }
      }
      _ => {}
    }
  }
  None
}

/// The values of the `return`s a body makes itself, not those of the
/// functions inside it; `None` for a bare `return`.
struct Returns<'a> {
  found: Vec<Option<&'a js::Expr>>,
}

impl<'a> Returns<'a> {
  fn stmt(&mut self, stmt: &'a js::Stmt) {
    match stmt {
      js::Stmt::Return(r) => self.found.push(r.arg.as_deref()),
      js::Stmt::Block(b) => b.stmts.iter().for_each(|s| self.stmt(s)),
      js::Stmt::If(i) => {
        self.stmt(&i.cons);
        if let Some(alt) = &i.alt {
          self.stmt(alt);
        }
      }
      js::Stmt::Try(t) => {
        t.block.stmts.iter().for_each(|s| self.stmt(s));
        if let Some(handler) = &t.handler {
          handler.body.stmts.iter().for_each(|s| self.stmt(s));
        }
      }
      js::Stmt::For(f) => self.stmt(&f.body),
      js::Stmt::ForOf(f) => self.stmt(&f.body),
      js::Stmt::ForIn(f) => self.stmt(&f.body),
      js::Stmt::While(w) => self.stmt(&w.body),
      js::Stmt::Switch(s) => s.cases.iter().flat_map(|c| &c.cons).for_each(|s| self.stmt(s)),
      _ => {}
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
  fn a_layout_reading_a_public_variable_gets_it_from_its_loader_as_a_prop() {
    let layout = "import { GoogleAnalytics } from \"@next/third-parties/google\";\n\nexport default function RootLayout({\n  children,\n}: {\n  children: React.ReactNode;\n}) {\n  return (\n    <html lang=\"en\">\n      <GoogleAnalytics gaId={process.env.NEXT_PUBLIC_GA_ID as string} />\n      <body>{children}</body>\n    </html>\n  );\n}\n";
    let input = tree(&[("package.json", r#"{ "dependencies": { "next": "latest" } }"#), ("app/layout.tsx", layout), ("app/page.tsx", "export default function Page() {\n  return <main />;\n}\n")]);
    let (out, report) = migrate(&input);
    let layout = out.text("app/routes/layout.tsx").unwrap();
    assert!(layout.contains("<GoogleAnalytics gaId={gaId as string} />"), "{layout}");
    assert!(layout.contains("children,\n  gaId,\n}: {\n  children: React.ReactNode;\n} & LayoutProps)"), "{layout}");
    assert!(layout.contains("import type { LayoutProps } from \"@generated/client\";"), "{layout}");
    assert!(!layout.contains("process.env"), "{layout}");
    let loader = out.text("app/routes/layout.loader.ts").unwrap();
    assert!(loader.contains("export async function load({ config }: Ctx) {\n  return { gaId: config.ga_id };\n}"), "{loader}");
    assert!(loader.contains("import type { Ctx } from \"@snapfire/fsr/host\";"), "{loader}");
    assert_eq!(report.config.iter().filter(|t| t.starts_with("[public]")).collect::<Vec<_>>(), ["[public]\nga_id = \"\"\n"]);
  }

  #[test]
  fn a_module_constant_holding_a_public_variable_becomes_the_prop_of_that_name() {
    let page = "const API_KEY = process.env.NEXT_PUBLIC_GOOGLE_API_KEY;\n\nexport default function Page() {\n  return <iframe src={`https://maps.example/?key=${API_KEY}`} />;\n}\n";
    let input = tree(&[("package.json", r#"{ "dependencies": { "next": "latest" } }"#), ("app/page.tsx", page)]);
    let (out, report) = migrate(&input);
    let page = out.text("app/routes/page.tsx").unwrap();
    assert!(page.contains("export default function Page({ API_KEY }: RootProps) {"), "{page}");
    assert!(!page.contains("const API_KEY") && !page.contains("process.env"), "{page}");
    let loader = out.text("app/routes/page.loader.ts").unwrap();
    assert!(loader.contains("export async function load({ config }: Ctx<\"/\">) {\n  return { API_KEY: config.google_api_key };\n}"), "{loader}");
    assert!(report.config.iter().any(|t| t == "[public]\ngoogle_api_key = \"\"\n"), "{:?}", report.config);
  }

  #[test]
  fn an_existing_loader_returns_the_value_beside_its_own_and_takes_config() {
    let page = "export default async function Page() {\n  const posts = await fetchPosts();\n  return <Feed posts={posts} site={process.env.NEXT_PUBLIC_SITE_NAME} />;\n}\n\nasync function fetchPosts() {\n  return [];\n}\n";
    let input = tree(&[("package.json", r#"{ "dependencies": { "next": "latest" } }"#), ("app/page.tsx", page)]);
    let (out, _) = migrate(&input);
    let loader = out.text("app/routes/page.loader.ts").unwrap();
    assert!(loader.contains("export async function load({ config }: Ctx<\"/\">) {"), "{loader}");
    assert!(loader.contains("return { posts, siteName: config.site_name };"), "{loader}");
    let page = out.text("app/routes/page.tsx").unwrap();
    assert!(page.contains("site={siteName}"), "{page}");
    assert!(page.contains("{ posts, siteName }"), "{page}");
  }

  #[test]
  fn a_loader_reads_a_public_variable_from_config() {
    let lift = crate::lift(&tree(&[("app/routes/blog/page.loader.ts", "export async function load({ params }) {\n  const token = process.env.NEXT_PUBLIC_BUTTER_CMS_API_KEY;\n  return { token, slug: params.slug };\n}\n")])).0;
    let mut out = lift.clone();
    let mut report = crate::Report::default();
    crate::Pass::run(&super::PublicEnv, &mut out, &mut report);
    let loader = out.text("app/routes/blog/page.loader.ts").unwrap();
    assert!(loader.contains("export async function load({ params, config }) {\n  const token = config.butter_cms_api_key;"), "{loader}");
    assert_eq!(report.config, ["[public]\nbutter_cms_api_key = \"\"\n"]);
  }

  #[test]
  fn a_server_only_variable_and_a_read_outside_a_route_component_are_notes() {
    let page = "export default function Page() {\n  return <p>{process.env.SECRET_TOKEN}</p>;\n}\n";
    let helper = "export const key = () => process.env.NEXT_PUBLIC_KEY;\n";
    let input = tree(&[("package.json", r#"{ "dependencies": { "next": "latest" } }"#), ("app/page.tsx", page), ("lib/key.ts", helper)]);
    let (out, report) = migrate(&input);
    assert!(out.text("app/routes/page.tsx").unwrap().contains("process.env.SECRET_TOKEN"));
    let notes: Vec<String> = report.notes.iter().map(|n| n.to_string()).collect();
    assert!(notes.iter().any(|n| n.contains("app/routes/page.tsx:2") && n.contains("`SECRET_TOKEN`") && n.contains("server-only")), "{notes:?}");
    assert!(notes.iter().any(|n| n.contains("app/src/lib/key.ts:1") && n.contains("`NEXT_PUBLIC_KEY`") && n.contains("below a route")), "{notes:?}");
    assert!(!report.config.iter().any(|t| t.starts_with("[public]")), "{:?}", report.config);
  }
}
