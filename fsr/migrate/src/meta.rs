//! Next's `generateMetadata` as the loader's `meta`: each value it awaits that `load` already returns is read from `data`, its `params` and `searchParams` are the `params` and `query` `meta` receives and the `Metadata` object it returns is written through `fromNextMetadata`.

use std::collections::BTreeSet;

use swc_core::common::Spanned;
use swc_core::ecma::ast as js;
use swc_core::ecma::visit::VisitWith;

use crate::edit::Edits;
use crate::items::{describe, prune, tidy, Item};
use crate::lift::{closure, copied, holds, imports_for, reads, request_field, route_kind, ContextUses, RequestReads};
use crate::relocate::dirname;
use crate::syntax::{parse, Parsed};
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
      let Ok(parsed) = parse(&path, &text) else { continue };
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
struct Body<'a> {
  stmts: Vec<&'a js::Stmt>,
  returned: &'a js::Expr,
  ctx: Option<&'a js::Pat>,
}

/// The module item exporting `generateMetadata`, as a function or an arrow, with its body.
fn generate_metadata(parsed: &Parsed) -> Option<(&js::ModuleItem, Body<'_>)> {
  for item in &parsed.module.body {
    let js::ModuleItem::ModuleDecl(js::ModuleDecl::ExportDecl(export)) = item else { continue };
    match &export.decl {
      js::Decl::Fn(f) if &*f.ident.sym == "generateMetadata" => return Some((item, of_block(f.function.body.as_ref()?, f.function.params.first().map(|p| &p.pat))?)),
      js::Decl::Var(var) => {
        for decl in &var.decls {
          let (js::Pat::Ident(name), Some(js::Expr::Arrow(arrow))) = (&decl.name, decl.init.as_deref()) else { continue };
          if &*name.id.sym != "generateMetadata" {
            continue;
          }
          let ctx = arrow.params.first();
          return Some((
            item,
            match &*arrow.body {
              js::ArrowFunctionBody::FunctionBody(block) => of_block(block, ctx)?,
              js::ArrowFunctionBody::Expr(e) => Body { stmts: Vec::new(), returned: unparen(e), ctx },
            },
          ));
        }
      }
      _ => {}
    }
  }
  None
}

fn of_block<'a>(block: &'a js::FunctionBody, ctx: Option<&'a js::Pat>) -> Option<Body<'a>> {
  let (last, before) = block.stmts.split_last()?;
  let js::Stmt::Return(js::ReturnStmt { arg: Some(returned), .. }) = last else { return None };
  Some(Body { stmts: before.iter().collect(), returned, ctx })
}

/// The name `generateMetadata` binds its props to whole, `None` when it destructures `params` and `searchParams` by those names.
fn props_name(ctx: Option<&js::Pat>) -> Result<Option<String>, String> {
  let Some(ctx) = ctx else { return Ok(None) };
  match ctx {
    js::Pat::Ident(id) => Ok(Some(id.id.sym.to_string())),
    js::Pat::Object(obj) => {
      for prop in &obj.props {
        let js::ObjectPatProp::Assign(a) = prop else { return Err("it destructures its props other than as `params` and `searchParams`".to_owned()) };
        if a.value.is_some() || !["params", "searchParams"].contains(&&*a.key.id.sym) {
          return Err(format!("it reads `{}` of its props, which `meta` does not receive", a.key.id.sym));
        }
      }
      Ok(None)
    }
    _ => Err("its props are not one name or a destructuring".to_owned()),
  }
}

/// Whether `e` is the request's params or search params or a `.then` on them.
fn is_request(props: Option<&str>, e: &js::Expr) -> bool {
  match e {
    js::Expr::Ident(i) => ["params", "searchParams"].contains(&&*i.sym),
    js::Expr::Call(call) => match &call.callee {
      js::Callee::Expr(callee) => matches!(&**callee, js::Expr::Member(m) if matches!(&m.prop, js::MemberProp::Ident(p) if &*p.sym == "then") && is_request(props, &m.obj)),
      _ => false,
    },
    other => request_field(props, other).is_some(),
  }
}

/// The text of `node`, its request reads written as FSR's, with what of the request it reads.
struct AsFsr<'a> {
  text: &'a str,
  parsed: &'a Parsed,
  props: Option<String>,
  uses: BTreeSet<&'static str>,
}

impl AsFsr<'_> {
  fn write<N: for<'b> VisitWith<ContextUses<'b>> + VisitWith<RequestReads> + Spanned>(&mut self, node: &N) -> String {
    let mut reads = RequestReads { props: self.props.clone(), names: BTreeSet::new() };
    node.visit_with(&mut reads);
    if reads.names.contains("params") {
      self.uses.insert("params");
    }
    if reads.names.contains("searchParams") {
      self.uses.insert("query");
    }
    let mut found = ContextUses { props: self.props.clone(), found: Vec::new(), parsed: self.parsed, text: self.text };
    node.visit_with(&mut found);
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
fn rewrite(path: &str, text: &str, parsed: &Parsed, item: &js::ModuleItem, body: &Body, loader_path: &str, loader: &str) -> Result<(String, String), String> {
  let returned = loaded_fields(loader_path, loader);
  let mut fsr = AsFsr { text, parsed, props: props_name(body.ctx)?, uses: BTreeSet::new() };
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
        let js::Pat::Ident(name) = pat else { return Err(format!("it awaits into `{pattern}`, which is not one name `load` returns")) };
        let name = name.id.sym.to_string();
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
  let items: Vec<Item> = parsed.module.body.iter().map(|i| describe(i, parsed)).collect();
  let declared = declared_in(loader_path, loader);
  let mut wanted = needed.clone();
  let mut taken = closure(&items, &mut wanted);
  let own = parsed.module.body.iter().position(|i| std::ptr::eq(i, item));
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
fn awaited(stmt: &js::Stmt) -> Option<Vec<(&js::Pat, &js::Expr)>> {
  let js::Stmt::Decl(js::Decl::Var(var)) = stmt else { return None };
  let [decl] = var.decls.as_slice() else { return None };
  let Some(js::Expr::Await(a)) = decl.init.as_deref() else { return None };
  if let (js::Pat::Array(pats), Some(values)) = (&decl.name, promise_all(&a.arg)) {
    if pats.elems.len() == values.len() && pats.elems.iter().all(Option::is_some) {
      return Some(pats.elems.iter().flatten().zip(values).collect());
    }
  }
  Some(vec![(&decl.name, &*a.arg)])
}

/// The values of `Promise.all([...])`, each without a spread.
fn promise_all(e: &js::Expr) -> Option<Vec<&js::Expr>> {
  let js::Expr::Call(call) = e else { return None };
  let js::Callee::Expr(callee) = &call.callee else { return None };
  let js::Expr::Member(m) = &**callee else { return None };
  let (js::Expr::Ident(obj), js::MemberProp::Ident(prop)) = (&*m.obj, &m.prop) else { return None };
  if &*obj.sym != "Promise" || &*prop.sym != "all" {
    return None;
  }
  let [arg] = call.args.as_slice() else { return None };
  let js::Expr::Array(array) = &*arg.expr else { return None };
  array.elems.iter().map(|e| e.as_ref().filter(|e| e.spread.is_none()).map(|e| &*e.expr)).collect()
}

/// The names `load` returns in its final `return { ... }`.
fn loaded_fields(path: &str, loader: &str) -> BTreeSet<String> {
  let Ok(parsed) = parse(path, loader) else { return BTreeSet::new() };
  let mut out = BTreeSet::new();
  for item in &parsed.module.body {
    let js::ModuleItem::ModuleDecl(js::ModuleDecl::ExportDecl(js::ExportDecl { decl: js::Decl::Fn(f), .. })) = item else { continue };
    if &*f.ident.sym != "load" {
      continue;
    }
    let Some(js::Stmt::Return(js::ReturnStmt { arg: Some(arg), .. })) = f.function.body.as_ref().and_then(|b| b.stmts.last()) else { continue };
    let js::Expr::Object(object) = &**arg else { continue };
    for prop in &object.props {
      let js::PropOrSpread::Prop(prop) = prop else { continue };
      match &**prop {
        js::Prop::Shorthand(id) => {
          out.insert(id.sym.to_string());
        }
        js::Prop::KeyValue(kv) => {
          if let js::PropName::Ident(id) = &kv.key {
            out.insert(id.sym.to_string());
          }
        }
        _ => {}
      }
    }
  }
  out
}

/// The loader's `meta` when it is `() => ({ head: [...] })`: its rows and its range, so the new `meta` keeps them; nothing when there is none; an error for any other `meta`.
fn existing_meta(path: &str, loader: &str) -> Result<(Option<String>, Option<std::ops::Range<usize>>), String> {
  let Ok(parsed) = parse(path, loader) else { return Ok((None, None)) };
  for item in &parsed.module.body {
    let js::ModuleItem::ModuleDecl(js::ModuleDecl::ExportDecl(js::ExportDecl { decl: js::Decl::Var(var), .. })) = item else { continue };
    for decl in &var.decls {
      let (js::Pat::Ident(name), Some(init)) = (&decl.name, decl.init.as_deref()) else { continue };
      if &*name.id.sym != "meta" {
        continue;
      }
      let rows = match init {
        js::Expr::Arrow(arrow) if arrow.params.is_empty() => match &*arrow.body {
          js::ArrowFunctionBody::Expr(e) => match unparen(e) {
            js::Expr::Object(object) if object.props.len() == 1 => object.props.first().and_then(|p| match p {
              js::PropOrSpread::Prop(p) => match &**p {
                js::Prop::KeyValue(kv) if matches!(&kv.key, js::PropName::Ident(i) if &*i.sym == "head") => match &*kv.value {
                  js::Expr::Array(array) => Some(array.elems.iter().flatten().map(|e| loader[parsed.range(e.span())].to_owned()).collect::<Vec<_>>().join(", ")),
                  _ => None,
                },
                _ => None,
              },
              _ => None,
            }),
            _ => None,
          },
          js::ArrowFunctionBody::FunctionBody(_) => None,
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

fn unparen(e: &js::Expr) -> &js::Expr {
  match e {
    js::Expr::Paren(p) => unparen(&p.expr),
    other => other,
  }
}

/// Every name the loader declares at its top, imports included.
fn declared_in(path: &str, loader: &str) -> BTreeSet<String> {
  let Ok(parsed) = parse(path, loader) else { return BTreeSet::new() };
  parsed.module.body.iter().flat_map(|i| describe(i, &parsed).binds).collect()
}

/// `loader` importing `fromNextMetadata` beside any head helper it already imports.
fn with_head_import(loader: &str) -> String {
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
