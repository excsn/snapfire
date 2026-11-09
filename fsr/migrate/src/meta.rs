//! Next's `generateMetadata` as the loader's `meta`: each value it awaits that `load` already returns is read from `data`, its `params` and `searchParams` are the `params` and `query` `meta` receives and the `Metadata` object it returns is written through `fromNextMetadata`.

use std::collections::BTreeSet;

use oxc_allocator::Allocator;
use oxc_ast::ast::{self as js, ArrayExpressionElement, BindingPattern, Declaration, Expression, ObjectPropertyKind, Statement};
use oxc_span::GetSpan;

use crate::edit::Edits;
use crate::items::{describe, prune, tidy, Item};
use crate::lift::{closure, copied, holds, imports_for, reads, request_field, route_kind, ContextUses, RequestReads};
use crate::relocate::dirname;
use crate::syntax::{argument, key_value, params, parse, Node, Param, Parsed};
use crate::{Pass, Report, Tree};

pub(crate) struct GeneratedMeta;

impl Pass for GeneratedMeta {
  fn name(&self) -> &'static str {
    "generate-metadata"
  }

  fn run(&self, tree: &mut Tree, report: &mut Report) {
    let routes: Vec<(String, &'static str)> = tree.paths().filter_map(|p| route_kind(p).map(|k| (p.to_owned(), k))).collect();
    for (path, kind) in routes {
      let Some(text) = tree.text(&path).map(str::to_owned) else { continue };
      if !text.contains("generateMetadata") {
        continue;
      }
      let allocator = Allocator::default();
      let Ok(parsed) = parse(&allocator, &path, &text) else { continue };
      let Some((item, body)) = generate_metadata(&parsed) else { continue };
      let line = parsed.line(item.span());
      let loader_path = format!("{}/{kind}.loader.ts", dirname(&path));
      let loader = tree.text(&loader_path).unwrap_or_default().to_owned();
      match rewrite(&path, &text, &parsed, item, &body, &loader_path, &loader) {
        Ok((page, loader)) => {
          tree.insert(loader_path, tidy(&loader));
          tree.insert(path, tidy(&page));
        }
        Err(why) => report.note(&path, line, format!("`generateMetadata` was left in place: {why}")),
      }
    }
  }
}

/// What `generateMetadata` is made of: its statements before the `return` and the object it returns.
struct Body<'b, 'a> {
  stmts: Vec<&'b Statement<'a>>,
  returned: &'b Expression<'a>,
  ctx: Option<Param<'b, 'a>>,
}

/// The module item exporting `generateMetadata`, as a function or an arrow, with its body.
fn generate_metadata<'b, 'a>(parsed: &'b Parsed<'a>) -> Option<(&'b Statement<'a>, Body<'b, 'a>)> {
  for item in parsed.body() {
    let Statement::ExportDeclaration(export) = item else { continue };
    match &export.declaration {
      Declaration::FunctionDeclaration(f) if f.id.as_ref().is_some_and(|id| id.name == "generateMetadata") => return Some((item, of_block(f.body.as_ref()?, params(&f.params).into_iter().next())?)),
      Declaration::VariableDeclaration(var) => {
        for decl in &var.declarations {
          let (BindingPattern::BindingIdentifier(name), Some(Expression::ArrowFunctionExpression(arrow))) = (&decl.id, &decl.init) else { continue };
          if name.name != "generateMetadata" {
            continue;
          }
          let ctx = params(&arrow.params).into_iter().next();
          return Some((
            item,
            match &arrow.body {
              js::ArrowFunctionBody::FunctionBody(block) => of_block(block, ctx)?,
              e => Body { stmts: Vec::new(), returned: unparen(e.as_expression()?), ctx },
            },
          ));
        }
      }
      _ => {}
    }
  }
  None
}

fn of_block<'b, 'a>(block: &'b js::FunctionBody<'a>, ctx: Option<Param<'b, 'a>>) -> Option<Body<'b, 'a>> {
  let (last, before) = block.statements.split_last()?;
  let Statement::ReturnStatement(ret) = last else { return None };
  let returned = ret.argument.as_ref()?;
  Some(Body { stmts: before.iter().collect(), returned, ctx })
}

/// The name `generateMetadata` binds its props to whole, `None` when it destructures `params` and `searchParams` by those names.
fn props_name(ctx: Option<&Param>) -> Result<Option<String>, String> {
  let Some(ctx) = ctx else { return Ok(None) };
  match ctx.plain() {
    Some(BindingPattern::BindingIdentifier(id)) => Ok(Some(id.name.to_string())),
    Some(BindingPattern::ObjectPattern(obj)) => {
      if obj.rest.is_some() {
        return Err("it destructures its props other than as `params` and `searchParams`".to_owned());
      }
      for prop in &obj.properties {
        let js::PropertyKey::StaticIdentifier(key) = &prop.key else { return Err("it destructures its props other than as `params` and `searchParams`".to_owned()) };
        if !prop.shorthand {
          return Err("it destructures its props other than as `params` and `searchParams`".to_owned());
        }
        if !matches!(prop.value, BindingPattern::BindingIdentifier(_)) || !["params", "searchParams"].contains(&key.name.as_str()) {
          return Err(format!("it reads `{}` of its props, which `meta` does not receive", key.name));
        }
      }
      Ok(None)
    }
    _ => Err("its props are not one name or a destructuring".to_owned()),
  }
}

/// Whether `e` is the request's params or search params or a `.then` on them.
fn is_request(props: Option<&str>, e: &Expression) -> bool {
  match e {
    Expression::Identifier(i) => ["params", "searchParams"].contains(&i.name.as_str()),
    Expression::CallExpression(call) => matches!(&call.callee, Expression::StaticMemberExpression(m) if m.property.name == "then" && is_request(props, &m.object)),
    other => request_field(props, other).is_some(),
  }
}

/// The text of `node`, its request reads written as FSR's, with what of the request it reads.
struct AsFsr<'a> {
  text: &'a str,
  parsed: &'a Parsed<'a>,
  props: Option<String>,
  uses: BTreeSet<&'static str>,
}

impl AsFsr<'_> {
  fn write<'n>(&mut self, node: &impl Node<'n>) -> String {
    let mut reads = RequestReads { props: self.props.clone(), names: BTreeSet::new() };
    node.accept(&mut reads);
    if reads.names.contains("params") {
      self.uses.insert("params");
    }
    if reads.names.contains("searchParams") {
      self.uses.insert("query");
    }
    let mut found = ContextUses { props: self.props.clone(), found: Vec::new(), parsed: self.parsed, text: self.text };
    node.accept(&mut found);
    let range = self.parsed.range(node.span());
    let mut edits = Edits::default();
    for (span, with) in found.found {
      let r = self.parsed.range(span);
      edits.replace(r.start - range.start..r.end - range.start, with);
    }
    edits.apply(&self.text[range])
  }
}

/// The page without `generateMetadata` and the loader with the `meta` it becomes; the reason it cannot move otherwise.
fn rewrite(path: &str, text: &str, parsed: &Parsed, item: &Statement, body: &Body, loader_path: &str, loader: &str) -> Result<(String, String), String> {
  let returned = loaded_fields(loader_path, loader);
  let mut fsr = AsFsr { text, parsed, props: props_name(body.ctx.as_ref())?, uses: BTreeSet::new() };
  let mut statements = String::new();
  let mut locals: BTreeSet<String> = BTreeSet::new();
  let mut needed: BTreeSet<String> = BTreeSet::new();
  let mut dropped: BTreeSet<String> = BTreeSet::new();
  for stmt in &body.stmts {
    if let Some(pairs) = awaited(stmt) {
      for (pat, read) in pairs {
        let mut bound = Vec::new();
        crate::items::pat_names(pat, &mut bound);
        locals.extend(bound);
        let pattern = &text[parsed.range(pat.span())];
        if is_request(fsr.props.as_deref(), read) {
          needed.extend(reads(read));
          let value = fsr.write(read);
          statements.push_str(&format!("  const {pattern} = {value};\n"));
          continue;
        }
        dropped.extend(reads(read));
        let BindingPattern::BindingIdentifier(name) = pat else { return Err(format!("it awaits into `{pattern}`, which is not one name `load` returns")) };
        let name = name.name.to_string();
        if !returned.contains(&name) {
          return Err(format!("it awaits `{name}`, which `load` does not return"));
        }
        statements.push_str(&format!("  const {name} = data.{name};\n"));
      }
      continue;
    }
    if holds(*stmt).awaits {
      return Err("it awaits in a statement other than `const x = await ...`".to_owned());
    }
    needed.extend(reads(*stmt));
    locals.extend(crate::lift::binds(stmt));
    let written = fsr.write(*stmt);
    statements.push_str(&format!("  {written}\n"));
  }
  if holds(body.returned).awaits {
    return Err("its returned object awaits".to_owned());
  }
  needed.extend(reads(body.returned));
  let returned_text = fsr.write(body.returned);
  let props = fsr.props.clone();
  needed.retain(|n| !locals.contains(n) && n != "data" && n != "params" && n != "searchParams" && Some(n) != props.as_ref());
  let ctx: Vec<&str> = std::iter::once("data").chain(fsr.uses.iter().copied()).collect();
  let ctx = ctx.join(", ");
  let (head_rows, old_meta) = match existing_meta(loader_path, loader) {
    Ok(found) => found,
    Err(why) => return Err(why),
  };
  let items: Vec<Item> = parsed.body().iter().map(|i| describe(i, parsed)).collect();
  let declared = declared_in(loader_path, loader);
  let mut wanted = needed.clone();
  let mut taken = closure(&items, &mut wanted);
  let own = parsed.body().iter().position(|i| std::ptr::eq(i, item));
  taken.retain(|n| Some(*n) != own && !items[*n].binds.iter().all(|b| declared.contains(b)));
  let decls = copied(text, parsed, &items, &taken, path, &mut Report::default());
  let imports: String = imports_for(text, parsed, &wanted, path, loader_path).lines().filter(|l| !loader.lines().any(|e| e == *l)).map(|l| format!("{l}\n")).collect();
  let expr = returned_text;
  let meta = match head_rows {
    Some(rows) => format!("export const meta = ({{ {ctx} }}) => {{\n{statements}  const next = fromNextMetadata({expr});\n  return {{ ...next, head: [...next.head, {rows}] }};\n}};\n"),
    None => format!("export const meta = ({{ {ctx} }}) => {{\n{statements}  return fromNextMetadata({expr});\n}};\n"),
  };
  let mut kept = Edits::default();
  if let Some(range) = old_meta {
    kept.replace(range, "");
  }
  let mut loader_text = with_head_import(&kept.apply(loader));
  loader_text.insert_str(0, &imports);
  loader_text.push_str(&format!("\n{decls}{meta}"));
  let mut page = Edits::default();
  page.replace(parsed.range(item.span()), "");
  let mut removable = wanted;
  removable.extend(dropped);
  removable.insert("Metadata".to_owned());
  Ok((prune(path, &page.apply(text), &removable), loader_text))
}

/// `const p = await e;` as `p` and `e`; `const [p, q] = await Promise.all([e, f]);` as each pattern and its value.
fn awaited<'b, 'a>(stmt: &'b Statement<'a>) -> Option<Vec<(&'b BindingPattern<'a>, &'b Expression<'a>)>> {
  let Statement::VariableDeclaration(var) = stmt else { return None };
  let [decl] = var.declarations.as_slice() else { return None };
  let Some(Expression::AwaitExpression(a)) = &decl.init else { return None };
  if let (BindingPattern::ArrayPattern(pats), Some(values)) = (&decl.id, promise_all(&a.argument)) {
    if pats.rest.is_none() && pats.elements.len() == values.len() && pats.elements.iter().all(Option::is_some) {
      return Some(pats.elements.iter().flatten().zip(values).collect());
    }
  }
  Some(vec![(&decl.id, &a.argument)])
}

/// The values of `Promise.all([...])`, each without a spread.
fn promise_all<'b, 'a>(e: &'b Expression<'a>) -> Option<Vec<&'b Expression<'a>>> {
  let Expression::CallExpression(call) = e else { return None };
  let Expression::StaticMemberExpression(m) = &call.callee else { return None };
  let Expression::Identifier(obj) = &m.object else { return None };
  if obj.name != "Promise" || m.property.name != "all" {
    return None;
  }
  let [arg] = call.arguments.as_slice() else { return None };
  let Expression::ArrayExpression(array) = argument(arg) else { return None };
  if matches!(arg, js::Argument::SpreadElement(_)) {
    return None;
  }
  array.elements.iter().map(|e| match e {
    ArrayExpressionElement::SpreadElement(_) | ArrayExpressionElement::Elision(_) => None,
    other => Some(other.to_expression()),
  }).collect()
}

/// The names `load` returns in its final `return { ... }`.
fn loaded_fields(path: &str, loader: &str) -> BTreeSet<String> {
  let allocator = Allocator::default();
  let Ok(parsed) = parse(&allocator, path, loader) else { return BTreeSet::new() };
  let mut out = BTreeSet::new();
  for item in parsed.body() {
    let Statement::ExportDeclaration(export) = item else { continue };
    let Declaration::FunctionDeclaration(f) = &export.declaration else { continue };
    if !f.id.as_ref().is_some_and(|id| id.name == "load") {
      continue;
    }
    let Some(Statement::ReturnStatement(ret)) = f.body.as_ref().and_then(|b| b.statements.last()) else { continue };
    let Some(Expression::ObjectExpression(object)) = &ret.argument else { continue };
    for prop in &object.properties {
      let ObjectPropertyKind::ObjectProperty(p) = prop else { continue };
      let js::PropertyKey::StaticIdentifier(id) = &p.key else { continue };
      if p.shorthand || key_value(prop).is_some() {
        out.insert(id.name.to_string());
      }
    }
  }
  out
}

/// The loader's `meta` when it is `() => ({ head: [...] })`: its rows and its range, so the new `meta` keeps them; nothing when there is none; an error for any other `meta`.
fn existing_meta(path: &str, loader: &str) -> Result<(Option<String>, Option<std::ops::Range<usize>>), String> {
  let allocator = Allocator::default();
  let Ok(parsed) = parse(&allocator, path, loader) else { return Ok((None, None)) };
  for item in parsed.body() {
    let Statement::ExportDeclaration(export) = item else { continue };
    let Declaration::VariableDeclaration(var) = &export.declaration else { continue };
    for decl in &var.declarations {
      let (BindingPattern::BindingIdentifier(name), Some(init)) = (&decl.id, &decl.init) else { continue };
      if name.name != "meta" {
        continue;
      }
      let rows = match init {
        Expression::ArrowFunctionExpression(arrow) if params(&arrow.params).is_empty() => match arrow.body.as_expression().map(unparen) {
          Some(Expression::ObjectExpression(object)) if object.properties.len() == 1 => object.properties.first().and_then(|p| match key_value(p) {
            Some(("head", Expression::ArrayExpression(array))) => Some(array.elements.iter().filter(|e| !matches!(e, ArrayExpressionElement::Elision(_))).map(|e| loader[parsed.range(e.span())].to_owned()).collect::<Vec<_>>().join(", ")),
            _ => None,
          }),
          _ => None,
        },
        _ => None,
      };
      return match rows {
        Some(rows) => Ok((Some(rows), Some(crate::items::line_range(loader, parsed.range(item.span()))))),
        None => Err("the loader already exports a `meta` other than head rows".to_owned()),
      };
    }
  }
  Ok((None, None))
}

fn unparen<'e, 'a>(e: &'e Expression<'a>) -> &'e Expression<'a> {
  match e {
    Expression::ParenthesizedExpression(p) => unparen(&p.expression),
    other => other,
  }
}

/// Every name the loader declares at its top, imports included.
fn declared_in(path: &str, loader: &str) -> BTreeSet<String> {
  let allocator = Allocator::default();
  let Ok(parsed) = parse(&allocator, path, loader) else { return BTreeSet::new() };
  parsed.body().iter().flat_map(|i| describe(i, &parsed).binds).collect()
}

/// `loader` importing `fromNextMetadata` beside any head helper it already imports.
pub(crate) fn with_head_import(loader: &str) -> String {
  if loader.lines().any(|l| l.starts_with("import") && l.contains("fromNextMetadata")) {
    return loader.to_owned();
  }
  match loader.lines().position(|l| l.starts_with("import {") && l.ends_with("from \"@snapfire/fsr/head\";")) {
    Some(at) => loader.lines().enumerate().map(|(i, l)| if i == at { l.replacen("import { ", "import { fromNextMetadata, ", 1) } else { l.to_owned() }).collect::<Vec<_>>().join("\n") + "\n",
    None => format!("import {{ fromNextMetadata }} from \"@snapfire/fsr/head\";\n{loader}"),
  }
}

#[cfg(test)]
mod tests {
  use crate::{lift, Tree};

  fn tree(files: &[(&str, &str)]) -> Tree {
    let mut t = Tree::new();
    for (p, s) in files {
      t.insert(*p, *s);
    }
    t
  }

  #[test]
  fn generate_metadata_reading_what_load_returns_is_the_loaders_meta() {
    let page = "import type { Metadata } from \"next\";\nimport { getPage } from \"../src/cms\";\nimport { toAlternates } from \"../src/seo\";\n\nconst forLocale = (l: string) => l;\n\nexport async function generateMetadata(): Promise<Metadata> {\n  const page = await getPage();\n  const image = page.image;\n  return { title: { absolute: page.title }, alternates: toAlternates(forLocale(\"en\")), openGraph: { images: [image] } };\n}\n\nexport default async function Home() {\n  const page = await getPage();\n  return <p>{page.title}</p>;\n}\n";
    let input = tree(&[("app/routes/page.tsx", page), ("app/src/cms.ts", "export const getPage = async () => ({ title: \"t\", image: \"i\" });\n"), ("app/src/seo.ts", "export const toAlternates = (l) => ({ canonical: l });\n")]);
    let (out, report) = lift(&input);
    let loader = out.text("app/routes/page.loader.ts").unwrap();
    assert!(loader.contains("import { fromNextMetadata } from \"@snapfire/fsr/head\";"), "{loader}");
    assert!(loader.contains("import { toAlternates } from \"../src/seo\";"), "{loader}");
    assert!(loader.contains("const forLocale = (l: string) => l;"), "{loader}");
    assert!(loader.contains("export const meta = ({ data }) => {\n  const page = data.page;\n  const image = page.image;\n  return fromNextMetadata({ title: { absolute: page.title }, alternates: toAlternates(forLocale(\"en\")), openGraph: { images: [image] } });\n};"), "{loader}");
    let page = out.text("app/routes/page.tsx").unwrap();
    assert!(!page.contains("generateMetadata") && !page.contains("Metadata") && !page.contains("toAlternates") && !page.contains("forLocale"), "{page}");
    assert!(!page.contains("getPage"), "the page no longer reads what only `generateMetadata` awaited: {page}");
    assert!(report.notes.is_empty(), "{:?}", report.notes);
  }

  #[test]
  fn generate_metadata_awaiting_what_load_does_not_return_is_left_with_a_note() {
    let page = "import { getPage, getTitle } from \"../src/cms\";\n\nexport async function generateMetadata() {\n  const title = await getTitle();\n  return { title };\n}\n\nexport default async function Home() {\n  const page = await getPage();\n  return <p>{page.title}</p>;\n}\n";
    let input = tree(&[("app/routes/page.tsx", page), ("app/src/cms.ts", "export const getPage = async () => ({ title: \"t\" });\nexport const getTitle = async () => \"t\";\n")]);
    let (out, report) = lift(&input);
    assert!(out.text("app/routes/page.tsx").unwrap().contains("generateMetadata"));
    assert!(report.notes.iter().any(|n| n.message.contains("it awaits `title`, which `load` does not return")), "{:?}", report.notes);
  }

  #[test]
  fn generate_metadata_reading_params_and_search_params_reads_what_meta_receives() {
    let page = "import { getPage } from \"../../src/cms\";\nimport { clean } from \"../../src/clean\";\n\nexport async function generateMetadata({ params, searchParams }: Props) {\n  const [{ locale }, page] = await Promise.all([params, getPage()]);\n  const q = await searchParams.then(clean);\n  return { title: { absolute: `${page.title} ${locale}` }, robots: q.code ? undefined : { index: false } };\n}\n\nexport default async function Home() {\n  const page = await getPage();\n  return <p>{page.title}</p>;\n}\n";
    let input = tree(&[("app/routes/[locale]/page.tsx", page), ("app/src/cms.ts", "export const getPage = async () => ({ title: \"t\" });\n"), ("app/src/clean.ts", "export const clean = (q) => q;\n")]);
    let (out, report) = lift(&input);
    let loader = out.text("app/routes/[locale]/page.loader.ts").unwrap();
    assert!(loader.contains("import { clean } from \"../../src/clean\";"), "{loader}");
    assert!(loader.contains("export const meta = ({ data, params, query }) => {\n  const { locale } = params;\n  const page = data.page;\n  const q = clean(query);\n  return fromNextMetadata({ title: { absolute: `${page.title} ${locale}` }, robots: q.code ? undefined : { index: false } });\n};"), "{loader}");
    assert!(!out.text("app/routes/[locale]/page.tsx").unwrap().contains("generateMetadata"));
    assert!(report.notes.is_empty(), "{:?}", report.notes);
  }

  #[test]
  fn generate_metadata_reading_its_props_whole_reads_them_off_meta() {
    let page = "export async function generateMetadata(props) {\n  const { slug } = await props.params;\n  return { title: slug };\n}\n\nexport default function Post() {\n  return <p>post</p>;\n}\n";
    let (out, report) = lift(&tree(&[("app/routes/[slug]/page.tsx", page)]));
    let loader = out.text("app/routes/[slug]/page.loader.ts").unwrap();
    assert!(loader.contains("export const meta = ({ data, params }) => {\n  const { slug } = params;\n  return fromNextMetadata({ title: slug });\n};"), "{loader}");
    assert!(report.notes.is_empty(), "{:?}", report.notes);
  }

  #[test]
  fn generate_metadata_awaiting_in_its_returned_object_is_left_with_a_note() {
    let page = "import { alternates } from \"../src/seo\";\n\nexport async function generateMetadata() {\n  return { alternates: await alternates() };\n}\n\nexport default function Home() {\n  return <p>home</p>;\n}\n";
    let (out, report) = lift(&tree(&[("app/routes/page.tsx", page), ("app/src/seo.ts", "export const alternates = async () => ({});\n")]));
    assert!(out.text("app/routes/page.tsx").unwrap().contains("generateMetadata"));
    assert!(report.notes.iter().any(|n| n.message.contains("its returned object awaits")), "{:?}", report.notes);
  }

  #[test]
  fn the_loaders_head_rows_are_kept_beside_what_generate_metadata_writes() {
    let page = "import { preconnect } from \"react-dom\";\nimport { getPage } from \"../src/cms\";\n\nexport async function generateMetadata() {\n  const page = await getPage();\n  return { title: page.title };\n}\n\nexport default async function Home() {\n  preconnect(\"https://media.example\");\n  const page = await getPage();\n  return <p>{page.title}</p>;\n}\n";
    let input = tree(&[("app/routes/page.tsx", page), ("app/src/cms.ts", "export const getPage = async () => ({ title: \"t\" });\n")]);
    let (out, _) = lift(&input);
    let loader = out.text("app/routes/page.loader.ts").unwrap();
    assert!(loader.contains("import { fromNextMetadata, linkTag } from \"@snapfire/fsr/head\";"), "{loader}");
    assert!(loader.contains("  const next = fromNextMetadata({ title: page.title });\n  return { ...next, head: [...next.head, linkTag(\"preconnect\", \"https://media.example\")] };"), "{loader}");
    assert_eq!(loader.matches("export const meta").count(), 1, "{loader}");
  }
}
