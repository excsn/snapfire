//! `process.env` reads. A `NEXT_PUBLIC_` variable a page or layout reads
//! becomes a `[public]` key its route's loader passes as a prop; every
//! other read is a note.

use std::collections::BTreeSet;

use oxc_allocator::Allocator;
use oxc_ast::ast::{self as js, BindingPattern, Declaration, Expression, MemberExpression, Statement};
use oxc_ast_visit::{walk, Visit};
use oxc_span::{GetSpan, Span};

use crate::edit::Edits;
use crate::items::{line_range, tidy};
use crate::lift::{ctx_type, default_function, props_name, route_id, route_kind};
use crate::relocate::dirname;
use crate::syntax::{is_script, last_property, lossy, params, parse, Param, Parsed};
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
      let allocator = Allocator::default();
      let Ok(parsed) = parse(&allocator, &path, &text) else { continue };
      let mut reads = EnvReads { found: Vec::new() };
      reads.visit_program(&parsed.program);
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
fn env_name(e: &MemberExpression) -> Option<String> {
  let Expression::StaticMemberExpression(inner) = e.object() else { return None };
  let Expression::Identifier(process) = &inner.object else { return None };
  if process.name != "process" || inner.property.name != "env" {
    return None;
  }
  match e {
    MemberExpression::StaticMemberExpression(m) => Some(m.property.name.to_string()),
    MemberExpression::ComputedMemberExpression(c) => match &c.expression {
      Expression::StringLiteral(s) => Some(lossy(s.value)),
      _ => None,
    },
    MemberExpression::PrivateFieldExpression(_) => None,
  }
}

struct EnvRead {
  span: Span,
  name: String,
}

struct EnvReads {
  found: Vec<EnvRead>,
}

impl<'a> Visit<'a> for EnvReads {
  fn visit_member_expression(&mut self, n: &MemberExpression<'a>) {
    match env_name(n) {
      Some(name) => self.found.push(EnvRead { span: n.span(), name }),
      None => walk::walk_member_expression(self, n),
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
fn unwrapped<'e, 'a>(e: &'e Expression<'a>) -> &'e Expression<'a> {
  match e {
    Expression::TSAsExpression(a) => unwrapped(&a.expression),
    Expression::TSNonNullExpression(n) => unwrapped(&n.expression),
    Expression::ParenthesizedExpression(p) => unwrapped(&p.expression),
    other => other,
  }
}

/// A page or layout whose `NEXT_PUBLIC_` reads become props its loader
/// passes from `config`, answering the keys it used; `None`, with a note,
/// when a read sits where a prop does not reach or the loader's returns are
/// not object literals.
fn route_component(tree: &mut Tree, path: &str, kind: &str, text: &str, parsed: &Parsed, public: &[(EnvRead, String)], report: &mut Report) -> Option<BTreeSet<String>> {
  let Some((function, _)) = default_function(parsed) else {
    report.note(path, parsed.line(public[0].0.span), "reads a public variable outside a default-exported component, so it was left for you");
    return None;
  };
  let inside = parsed.range(function.span);
  let mut constants: Vec<(Span, String)> = Vec::new();
  for item in parsed.body() {
    let Statement::VariableDeclaration(var) = item else { continue };
    let [decl] = var.declarations.as_slice() else { continue };
    let (BindingPattern::BindingIdentifier(name), Some(init)) = (&decl.id, &decl.init) else { continue };
    if let Some(m) = unwrapped(init).as_member_expression() {
      if env_name(m).is_some_and(|n| n.starts_with("NEXT_PUBLIC_")) {
        constants.push((var.span, name.name.to_string()));
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
  let param = params(&function.params).into_iter().next();
  let holder = match &param {
    Some(Param::Plain(p)) => match &p.pattern {
      BindingPattern::BindingIdentifier(i) => Some(i.name.to_string()),
      _ => None,
    },
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
  let annotation = |type_ann: &Option<oxc_allocator::Box<js::TSTypeAnnotation>>, pattern_end: usize, edits: &mut Edits| {
    if !typed {
      return;
    }
    match type_ann {
      Some(t) if text[parsed.range(t.type_annotation.span())].trim() == props_type => {}
      Some(t) => {
        let end = parsed.range(t.span).end;
        edits.replace(end..end, format!(" & {props_type}"));
      }
      None => edits.replace(pattern_end..pattern_end, format!(": {props_type}")),
    }
  };
  let plain = match param {
    None => None,
    Some(Param::Plain(p)) => Some(p),
    Some(Param::Other(_)) => {
      report.note(path, parsed.line(function.span), "the component's props are not a destructuring or a name, so its public variables were left for you");
      return None;
    }
  };
  match plain.map(|p| (p, &p.pattern)) {
    None => {
      let open = text[inside.clone()].find('(').map(|i| inside.start + i + 1)?;
      let typed = if typed { format!(": {props_type}") } else { String::new() };
      edits.replace(open..open, format!("{{ {} }}{typed}", names.join(", ")));
    }
    Some((param, BindingPattern::ObjectPattern(obj))) => {
      let pattern = parsed.range(obj.span);
      let have: Vec<String> = obj.properties.iter().filter_map(|p| match &p.key {
        js::PropertyKey::StaticIdentifier(i) if !p.computed => Some(i.name.to_string()),
        _ => None,
      }).collect();
      let added: Vec<&str> = names.iter().copied().filter(|n| !have.iter().any(|h| h == n)).collect();
      match last_property(obj) {
        Some(last) if !added.is_empty() => {
          let end = parsed.range(last).end;
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
      let end = param.type_annotation.as_ref().map_or(pattern.end, |t| parsed.range(t.span).start);
      annotation(&param.type_annotation, end, &mut edits);
    }
    Some((param, BindingPattern::BindingIdentifier(i))) => annotation(&param.type_annotation, parsed.range(i.span).end, &mut edits),
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
  let allocator = Allocator::default();
  let parsed = parse(&allocator, path, loader).ok()?;
  let Some((formals, body)) = load_function(&parsed) else {
    let import = ctx_import(loader);
    let sep = if loader.is_empty() { "" } else { "\n" };
    return Some(format!("{import}{loader}{sep}export async function load({{ config }}: {}) {{\n  return {{ {} }};\n}}\n", ctx_type(path), entries("config")));
  };
  let mut edits = Edits::default();
  let config = config_param(loader, &parsed, formals, body, path, &mut edits)?;
  let mut returns = Returns { found: Vec::new() };
  body.statements.iter().for_each(|s| returns.stmt(s));
  if returns.found.is_empty() {
    return None;
  }
  for arg in returns.found {
    let Expression::ObjectExpression(obj) = unwrapped(arg?) else { return None };
    match obj.properties.last() {
      Some(last) => {
        let end = parsed.range(last.span()).end;
        edits.replace(end..end, format!(", {}", entries(&config)));
      }
      None => edits.replace(parsed.range(obj.span), format!("{{ {} }}", entries(&config))),
    }
  }
  let import = if params(formals).is_empty() { ctx_import(loader) } else { String::new() };
  Some(format!("{import}{}", edits.apply(loader)))
}

fn ctx_import(text: &str) -> String {
  if text.lines().any(|l| l.starts_with("import") && l.contains("Ctx")) { String::new() } else { "import type { Ctx } from \"@snapfire/fsr/host\";\n".to_owned() }
}

/// How `load` reaches `config`, `config` from its destructuring or
/// `ctx.config`, with the edits that give it the field when it had none.
fn config_param(loader: &str, parsed: &Parsed, formals: &js::FormalParameters, body: &js::FunctionBody, path: &str, edits: &mut Edits) -> Option<String> {
  let first = params(formals).into_iter().next().map(|p| match p {
    Param::Plain(p) => Some(&p.pattern),
    Param::Other(_) => None,
  });
  match first {
    None => {
      let open = loader[..parsed.range(body.span).start].rfind('(')? + 1;
      edits.replace(open..open, format!("{{ config }}: {}", ctx_type(path)));
      Some("config".to_owned())
    }
    Some(Some(BindingPattern::ObjectPattern(obj))) => {
      let has = obj.properties.iter().any(|p| p.shorthand && matches!(&p.key, js::PropertyKey::StaticIdentifier(k) if k.name == "config"));
      if !has {
        match last_property(obj) {
          Some(last) => {
            let end = parsed.range(last).end;
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
    Some(Some(BindingPattern::BindingIdentifier(i))) => Some(format!("{}.config", i.name)),
    Some(_) => None,
  }
}

/// A loader whose `load` reads `NEXT_PUBLIC_` variables, each read becoming
/// one of `config`, answering the keys it used.
fn loader_reads(tree: &mut Tree, path: &str, text: &str, parsed: &Parsed, public: &[(EnvRead, String)], report: &mut Report) -> Option<BTreeSet<String>> {
  let Some((formals, body)) = load_function(parsed) else {
    report.note(path, parsed.line(public[0].0.span), format!("`{}` is read outside `load`, where `config` does not reach, so it was left for you", public[0].0.name));
    return None;
  };
  let within = parsed.range(body.span);
  if let Some((read, _)) = public.iter().find(|(read, _)| { let at = parsed.range(read.span); at.start < within.start || at.end > within.end }) {
    report.note(path, parsed.line(read.span), format!("`{}` is read outside `load`, where `config` does not reach, so it was left for you", read.name));
    return None;
  }
  let mut edits = Edits::default();
  let config = config_param(text, parsed, formals, body, path, &mut edits)?;
  for (read, key) in public {
    edits.replace(parsed.range(read.span), format!("{config}.{key}"));
  }
  let import = if params(formals).is_empty() { ctx_import(text) } else { String::new() };
  tree.insert(path.to_owned(), tidy(&format!("{import}{}", edits.apply(text))));
  Some(public.iter().map(|(_, key)| key.clone()).collect())
}

/// The parameters and body of the `load` a loader exports.
fn load_function<'p, 'a>(parsed: &'p Parsed<'a>) -> Option<(&'p js::FormalParameters<'a>, &'p js::FunctionBody<'a>)> {
  for item in parsed.body() {
    let Statement::ExportDeclaration(export) = item else { continue };
    match &export.declaration {
      Declaration::FunctionDeclaration(f) if f.id.as_ref().is_some_and(|id| id.name == "load") => return Some((&f.params, f.body.as_ref()?)),
      Declaration::VariableDeclaration(var) => {
        for decl in &var.declarations {
          let (BindingPattern::BindingIdentifier(name), Some(init)) = (&decl.id, &decl.init) else { continue };
          if name.name != "load" {
            continue;
          }
          match init {
            Expression::ArrowFunctionExpression(a) => match &a.body {
              js::ArrowFunctionBody::FunctionBody(b) => return Some((&a.params, b)),
              _ => return None,
            },
            Expression::FunctionExpression(f) => return Some((&f.params, f.body.as_ref()?)),
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
struct Returns<'b, 'a> {
  found: Vec<Option<&'b Expression<'a>>>,
}

impl<'b, 'a> Returns<'b, 'a> {
  fn stmt(&mut self, stmt: &'b Statement<'a>) {
    match stmt {
      Statement::ReturnStatement(r) => self.found.push(r.argument.as_ref()),
      Statement::BlockStatement(b) => b.body.iter().for_each(|s| self.stmt(s)),
      Statement::IfStatement(i) => {
        self.stmt(&i.consequent);
        if let Some(alt) = &i.alternate {
          self.stmt(alt);
        }
      }
      Statement::TryStatement(t) => {
        t.block.body.iter().for_each(|s| self.stmt(s));
        if let Some(handler) = &t.handler {
          handler.body.body.iter().for_each(|s| self.stmt(s));
        }
      }
      Statement::ForStatement(f) => self.stmt(&f.body),
      Statement::ForOfStatement(f) => self.stmt(&f.body),
      Statement::ForInStatement(f) => self.stmt(&f.body),
      Statement::WhileStatement(w) => self.stmt(&w.body),
      Statement::SwitchStatement(s) => s.cases.iter().flat_map(|c| &c.consequent).for_each(|s| self.stmt(s)),
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
