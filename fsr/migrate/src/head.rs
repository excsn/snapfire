//! `next/head`: a page's or a layout's `<Head>` as the loader's `meta`.

use std::collections::{BTreeMap, BTreeSet};

use swc_core::common::{Span, Spanned};
use swc_core::ecma::ast as js;
use swc_core::ecma::visit::{Visit, VisitWith};

use crate::edit::Edits;
use crate::items::{drop_named_import, import_text, line_range, local, tidy};
use crate::syntax::{is_script, parse, Parsed};
use crate::{Pass, Report, Tree};

/// Each `<Head>` taken out of the module that places it. In a page or a
/// layout its title, description and the head elements `@snapfire/fsr/head`
/// has a helper for become the `meta` of the loader beside it, an
/// expression reading the page's props reading them from `data`; anywhere
/// else (or for an element no helper writes) each element is a note.
pub(crate) struct Head;

impl Pass for Head {
  fn name(&self) -> &'static str {
    "next-head"
  }

  fn run(&self, tree: &mut Tree, report: &mut Report) {
    let paths: Vec<String> = tree.paths().filter(|p| p.starts_with("app/") && is_script(p)).map(str::to_owned).collect();
    for path in paths {
      let Some(text) = tree.text(&path).map(str::to_owned) else { continue };
      if !text.contains("next/head") {
        continue;
      }
      let Ok(parsed) = parse(&path, &text) else { continue };
      let Some(local_name) = parsed.module.body.iter().find_map(|item| match item {
        js::ModuleItem::ModuleDecl(js::ModuleDecl::Import(import)) if import.src.value.to_string_lossy() == "next/head" => import.specifiers.iter().find_map(|s| match s {
          js::ImportSpecifier::Default(d) => Some(d.local.sym.to_string()),
          _ => None,
        }),
        _ => None,
      }) else {
        continue;
      };
      let route = route_kind(&path);
      let scope = Scope::of(&text, &parsed, route == Some("page"));
      let mut found = Heads { name: local_name.clone(), found: Vec::new() };
      parsed.module.visit_with(&mut found);
      let mut edits = Edits::default();
      let mut fields: Vec<(String, String)> = Vec::new();
      let mut helpers: BTreeSet<&'static str> = BTreeSet::new();
      let mut needed: BTreeSet<String> = BTreeSet::new();
      let mut head: Vec<String> = Vec::new();
      for element in &found.found {
        for child in &element.children {
          let js::JSXElementChild::JSXElement(tag) = child else { continue };
          let line = parsed.line(tag.span);
          match convert(&text, &parsed, tag) {
            Converted::Dropped => {}
            Converted::Field(name, _) | Converted::Head(name, _) if route.is_none() => {
              report.note(&path, line, format!("`<{name}>` in `<Head>` was taken out: a head element comes from a route's loader `meta`, not from a component"));
            }
            Converted::Unknown(what) => report.note(&path, line, format!("{what} in `<Head>` was taken out; `meta` writes head elements through `head`")),
            Converted::Field(name, value) => match scope.rewrite(&value) {
              Some((rewritten, uses)) => {
                needed.extend(uses);
                fields.retain(|(n, _)| *n != name);
                fields.push((name, rewritten));
              }
              None => report.note(&path, line, format!("the {name} reads a value the loader does not return, so it was taken out")),
            },
            Converted::Head(helper, args) => match scope.rewrite(&args) {
              Some((rewritten, uses)) => {
                needed.extend(uses);
                helpers.insert(helper_name(&helper));
                head.push(format!("{helper}({rewritten})"));
              }
              None => report.note(&path, line, format!("a `{helper}` element reads a value the loader does not return, so it was taken out")),
            },
          }
        }
        edits.replace(line_range(&text, parsed.range(element.span)), "");
      }
      let mut page = edits.apply(&text);
      if let Some(stripped) = drop_named_import(&page, "next/head", &local_name) {
        page = stripped;
      }
      tree.insert(path.clone(), tidy(&page));
      let Some(kind) = route else { continue };
      if fields.is_empty() && head.is_empty() {
        continue;
      }
      if !head.is_empty() {
        fields.push(("head".to_owned(), format!("[{}]", head.join(", "))));
      }
      let reads_data = fields.iter().any(|(_, v)| v.contains("data."));
      let object = fields.iter().map(|(k, v)| format!("{k}: {v}")).collect::<Vec<_>>().join(", ");
      let arg = if reads_data { "({ data })" } else { "()" };
      let meta = format!("export const meta = {arg} => ({{ {object} }});\n");
      let dir = path.rsplit_once('/').map(|(d, _)| d).unwrap_or("");
      let loader_path = format!("{dir}/{kind}.loader.ts");
      let existing = tree.text(&loader_path).unwrap_or_default().to_owned();
      if existing.contains("export const meta") {
        report.note(&path, 0, "the loader already exports `meta`, so `<Head>` was not carried over");
        continue;
      }
      let mut prelude = String::new();
      if !helpers.is_empty() {
        prelude.push_str(&format!("import {{ {} }} from \"@snapfire/fsr/head\";\n", helpers.iter().copied().collect::<Vec<_>>().join(", ")));
      }
      prelude.push_str(&scope.imports_for(&text, &parsed, &needed, &existing));
      let mut decls = String::new();
      for name in &needed {
        if let Some(decl) = scope.declarations.get(name) {
          if !existing.contains(decl.as_str()) {
            decls.push_str(decl);
            decls.push('\n');
          }
        }
      }
      let loader = format!("{prelude}{existing}{}{decls}{}{meta}", if existing.is_empty() || existing.ends_with('\n') { "" } else { "\n" }, if existing.is_empty() && decls.is_empty() { "" } else { "\n" });
      tree.insert(loader_path, tidy(&loader));
    }
  }
}

fn route_kind(path: &str) -> Option<&'static str> {
  if !path.starts_with("app/routes/") {
    return None;
  }
  let file = path.rsplit('/').next().unwrap_or(path);
  if file.matches('.').count() != 1 {
    return None;
  }
  match file.split('.').next() {
    Some("page") => Some("page"),
    Some("layout") => Some("layout"),
    _ => None,
  }
}

fn helper_name(helper: &str) -> &'static str {
  match helper {
    "og" => "og",
    "twitter" => "twitter",
    "robots" => "robots",
    "canonical" => "canonical",
    "themeColor" => "themeColor",
    "metaTag" => "metaTag",
    _ => "linkTag",
  }
}

enum Converted {
  /// `title` or `description` and the expression it takes.
  Field(String, String),
  /// A helper from `@snapfire/fsr/head` and its arguments.
  Head(String, String),
  /// What the host writes itself: the charset and the viewport.
  Dropped,
  Unknown(String),
}

fn attr<'a>(text: &'a str, parsed: &Parsed, opening: &js::JSXOpeningElement, name: &str) -> Option<String> {
  opening.attrs.iter().find_map(|a| match a {
    js::JSXAttrOrSpread::JSXAttr(js::JSXAttr { name: js::JSXAttrName::Ident(n), value: Some(value), .. }) if &*n.sym == name => Some(match value {
      js::JSXAttrValue::Str(s) => format!("{:?}", s.value.to_string_lossy()),
      js::JSXAttrValue::JSXExprContainer(js::JSXExprContainer { expr: js::JSXExpr::Expr(e), .. }) => text[parsed.range(e.span())].to_owned(),
      other => text[parsed.range(other.span())].to_owned(),
    }),
    _ => None,
  })
}

fn convert(text: &str, parsed: &Parsed, tag: &js::JSXElement) -> Converted {
  let js::JSXElementName::Ident(name) = &tag.opening.name else { return Converted::Unknown("an element".to_owned()) };
  let attr = |n: &str| attr(text, parsed, &tag.opening, n);
  match &*name.sym {
    "title" => match title(text, parsed, tag) {
      Some(t) => Converted::Field("title".to_owned(), t),
      None => Converted::Unknown("a `<title>` this pass cannot read".to_owned()),
    },
    "meta" => {
      if attr("charSet").is_some() || attr("charset").is_some() || attr("name").as_deref() == Some("\"viewport\"") || attr("httpEquiv").is_some() {
        return Converted::Dropped;
      }
      let Some(content) = attr("content") else { return Converted::Unknown("a `<meta>` with no `content`".to_owned()) };
      if let Some(property) = attr("property") {
        if let Some(rest) = property.strip_prefix("\"og:") {
          return Converted::Head("og".to_owned(), format!("\"{rest}, {content}"));
        }
        return Converted::Unknown(format!("`<meta property={property}>`"));
      }
      let Some(meta_name) = attr("name") else { return Converted::Unknown("a `<meta>` with no `name`".to_owned()) };
      match meta_name.as_str() {
        "\"description\"" => Converted::Field("description".to_owned(), content),
        "\"robots\"" => Converted::Head("robots".to_owned(), content),
        "\"theme-color\"" => Converted::Head("themeColor".to_owned(), content),
        n if n.starts_with("\"twitter:") => Converted::Head("twitter".to_owned(), format!("\"{}, {content}", &n["\"twitter:".len()..])),
        n => Converted::Head("metaTag".to_owned(), format!("{n}, {content}")),
      }
    }
    "link" => {
      let (Some(rel), Some(href)) = (attr("rel"), attr("href")) else { return Converted::Unknown("a `<link>` with no `rel` or `href`".to_owned()) };
      if rel == "\"canonical\"" {
        return Converted::Head("canonical".to_owned(), href);
      }
      let others: Vec<String> = tag.opening.attrs.iter().filter_map(|a| match a {
        js::JSXAttrOrSpread::JSXAttr(js::JSXAttr { name: js::JSXAttrName::Ident(n), .. }) if !["rel", "href", "key"].contains(&&*n.sym) => Some(n.sym.to_string()),
        _ => None,
      }).collect();
      if !others.is_empty() {
        return Converted::Unknown(format!("a `<link rel={rel}>` with {}", others.join(", ")));
      }
      Converted::Head("linkTag".to_owned(), format!("{rel}, {href}"))
    }
    other => Converted::Unknown(format!("`<{other}>`")),
  }
}

/// A `<title>`'s children as one expression: a string literal when it is
/// all text, a template literal when it mixes text and expressions.
fn title(text: &str, parsed: &Parsed, tag: &js::JSXElement) -> Option<String> {
  let mut parts: Vec<Result<String, String>> = Vec::new();
  for child in &tag.children {
    match child {
      js::JSXElementChild::JSXText(t) => {
        let value = t.value.to_string_lossy();
        let collapsed = value.split_whitespace().collect::<Vec<_>>().join(" ");
        if !collapsed.is_empty() {
          let lead = if value.starts_with(char::is_whitespace) && !parts.is_empty() { " " } else { "" };
          let trail = if value.ends_with(char::is_whitespace) { " " } else { "" };
          parts.push(Ok(format!("{lead}{collapsed}{trail}")));
        } else if !parts.is_empty() && value.contains(' ') && !value.contains('\n') {
          parts.push(Ok(" ".to_owned()));
        }
      }
      js::JSXElementChild::JSXExprContainer(js::JSXExprContainer { expr: js::JSXExpr::Expr(e), .. }) => parts.push(Err(text[parsed.range(e.span())].to_owned())),
      js::JSXElementChild::JSXExprContainer(_) => {}
      _ => return None,
    }
  }
  match parts.as_slice() {
    [] => None,
    [Ok(s)] => Some(format!("{:?}", s.trim())),
    [Err(e)] => Some(e.clone()),
    _ => {
      let mut out = String::from("`");
      for (i, part) in parts.iter().enumerate() {
        match part {
          Ok(s) => {
            let s = if i == 0 { s.trim_start() } else if i == parts.len() - 1 { s.trim_end() } else { s.as_str() };
            out.push_str(&s.replace('`', "\\`").replace("${", "\\${"));
          }
          Err(e) => out.push_str(&format!("${{{e}}}")),
        }
      }
      out.push('`');
      Some(out)
    }
  }
}

/// What a `meta` body may read: the page's props as `data`, the module's
/// imports and its module-level declarations.
struct Scope {
  props: BTreeSet<String>,
  /// The name a page binds its whole props object to, when it does.
  props_object: Option<String>,
  imports: BTreeSet<String>,
  declarations: BTreeMap<String, String>,
}

impl Scope {
  fn of(text: &str, parsed: &Parsed, page: bool) -> Self {
    let mut scope = Scope { props: BTreeSet::new(), props_object: None, imports: BTreeSet::new(), declarations: BTreeMap::new() };
    for item in &parsed.module.body {
      match item {
        js::ModuleItem::ModuleDecl(js::ModuleDecl::Import(i)) => scope.imports.extend(i.specifiers.iter().map(local)),
        js::ModuleItem::Stmt(js::Stmt::Decl(js::Decl::Var(v))) if v.kind == js::VarDeclKind::Const => {
          for d in &v.decls {
            if let js::Pat::Ident(i) = &d.name {
              scope.declarations.insert(i.id.sym.to_string(), text[parsed.range(item.span())].to_owned());
            }
          }
        }
        _ => {}
      }
    }
    if page {
      if let Some(function) = default_component(&parsed.module) {
        match function.params.first().map(|p| &p.pat) {
          Some(js::Pat::Object(o)) => {
            for p in &o.props {
              match p {
                js::ObjectPatProp::Assign(a) => {
                  scope.props.insert(a.key.sym.to_string());
                }
                js::ObjectPatProp::KeyValue(kv) => {
                  if let (js::PropName::Ident(k), js::Pat::Ident(v)) = (&kv.key, &*kv.value) {
                    if k.sym == v.id.sym {
                      scope.props.insert(k.sym.to_string());
                    }
                  }
                }
                js::ObjectPatProp::Rest(_) => {}
              }
            }
          }
          Some(js::Pat::Ident(i)) => scope.props_object = Some(i.id.sym.to_string()),
          _ => {}
        }
      }
    }
    scope
  }

  /// `expression` with every read of a prop written `data.<prop>`, plus the
  /// imports and declarations it reads; `None` when it reads anything else
  /// the module does not bind at its top.
  fn rewrite(&self, expression: &str) -> Option<(String, BTreeSet<String>)> {
    let wrapped = format!("({expression})");
    let inner = parse("meta.tsx", &wrapped).ok()?;
    let mut reads = Free { found: Vec::new() };
    inner.module.visit_with(&mut reads);
    let mut edits = Edits::default();
    let mut uses = BTreeSet::new();
    for (span, name) in reads.found {
      let at = inner.range(span);
      if self.props.contains(&name) {
        edits.replace(at, format!("data.{name}"));
      } else if self.props_object.as_deref() == Some(name.as_str()) {
        edits.replace(at, "data");
      } else if self.imports.contains(&name) || self.declarations.contains_key(&name) {
        uses.insert(name);
      } else if !GLOBALS.contains(&name.as_str()) {
        return None;
      }
    }
    let out = edits.apply(&wrapped);
    Some((out[1..out.len() - 1].to_owned(), uses))
  }

  /// The import declarations binding `needed`, those `existing` already
  /// has left out.
  fn imports_for(&self, text: &str, parsed: &Parsed, needed: &BTreeSet<String>, existing: &str) -> String {
    let mut out = String::new();
    for item in &parsed.module.body {
      let js::ModuleItem::ModuleDecl(js::ModuleDecl::Import(import)) = item else { continue };
      let kept: Vec<&js::ImportSpecifier> = import.specifiers.iter().filter(|s| needed.contains(&local(s))).collect();
      if kept.is_empty() {
        continue;
      }
      let line = import_text(text, parsed, import, &kept);
      if !existing.contains(line.trim_end()) {
        out.push_str(&line);
      }
    }
    out
  }
}

const GLOBALS: &[&str] = &["undefined", "String", "Number", "Math", "JSON", "Date", "Boolean", "Array", "Object"];

fn default_component(module: &js::Module) -> Option<&js::Function> {
  let mut named = None;
  for item in &module.body {
    match item {
      js::ModuleItem::ModuleDecl(js::ModuleDecl::ExportDefaultDecl(js::ExportDefaultDecl { decl: js::DefaultDecl::Fn(f), .. })) => return Some(&f.function),
      js::ModuleItem::ModuleDecl(js::ModuleDecl::ExportDefaultExpr(e)) => {
        if let js::Expr::Ident(i) = &*e.expr {
          named = Some(i.sym.to_string());
        }
      }
      _ => {}
    }
  }
  let named = named?;
  module.body.iter().find_map(|item| match item {
    js::ModuleItem::Stmt(js::Stmt::Decl(js::Decl::Fn(f))) if f.ident.sym == *named => Some(&*f.function),
    _ => None,
  })
}

/// The identifiers an expression reads, not the property names it names.
struct Free {
  found: Vec<(Span, String)>,
}

impl Visit for Free {
  fn visit_ident(&mut self, n: &js::Ident) {
    self.found.push((n.span, n.sym.to_string()));
  }

  fn visit_member_expr(&mut self, n: &js::MemberExpr) {
    n.obj.visit_with(self);
    if let js::MemberProp::Computed(c) = &n.prop {
      c.visit_with(self);
    }
  }
}

struct Heads {
  name: String,
  found: Vec<js::JSXElement>,
}

impl Visit for Heads {
  fn visit_jsx_element(&mut self, n: &js::JSXElement) {
    if let js::JSXElementName::Ident(i) = &n.opening.name {
      if *i.sym == *self.name {
        self.found.push(n.clone());
        return;
      }
    }
    n.visit_children_with(self);
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
  fn a_pages_head_becomes_its_loaders_meta_reading_the_props_from_data() {
    let page = "import Head from \"next/head\";\nimport { CMS_NAME } from \"../../lib/constants\";\n\nexport default function Post({ post }) {\n  return (\n    <main>\n      <Head>\n        <title>\n          {post.title} | Blog with {CMS_NAME}\n        </title>\n        <meta name=\"viewport\" content=\"width=device-width\" />\n        <meta property=\"og:image\" content={post.image} />\n        <script src=\"/x.js\" />\n      </Head>\n      <h1>{post.title}</h1>\n    </main>\n  );\n}\n\nexport async function getStaticProps() {\n  return { props: { post: { title: \"t\", image: \"i\" } } };\n}\n";
    let input = tree(&[("package.json", PACKAGE), ("pages/posts/[slug].js", page), ("lib/constants.js", "export const CMS_NAME = \"Acme\";\n")]);
    let (out, report) = migrate(&input);
    assert_eq!(
      out.text("app/routes/posts/[slug]/page.js").unwrap(),
      "import { CMS_NAME } from \"../../../src/lib/constants\";\n\nexport default function Post({ post }) {\n  return (\n    <main>\n      <h1>{post.title}</h1>\n    </main>\n  );\n}\n"
    );
    assert_eq!(
      out.text("app/routes/posts/[slug]/page.loader.ts").unwrap(),
      "import { og } from \"@snapfire/fsr/head\";\nimport { CMS_NAME } from \"../../../src/lib/constants\";\nexport async function load() {\n  return { post: { title: \"t\", image: \"i\" } };\n}\n\nexport const meta = ({ data }) => ({ title: `${data.post.title} | Blog with ${CMS_NAME}`, head: [og(\"image\", data.post.image)] });\n"
    );
    assert_eq!(report.notes.iter().filter(|n| n.message.contains("`<script>`")).count(), 1, "{:?}", report.notes);
  }
}
