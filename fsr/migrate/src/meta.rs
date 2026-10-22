//! Next's `generateMetadata` as the loader's `meta`: each value it awaits that `load` already returns is read from `data` and the `Metadata` object it returns is written through `fromNextMetadata`.

use std::collections::BTreeSet;

use swc_core::common::Spanned;
use swc_core::ecma::ast as js;

use crate::edit::Edits;
use crate::items::{describe, prune, tidy, Item};
use crate::lift::{closure, copied, holds, imports_for, reads, route_kind};
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
  takes_params: bool,
}

/// The module item exporting `generateMetadata`, as a function or an arrow, with its body.
fn generate_metadata(parsed: &Parsed) -> Option<(&js::ModuleItem, Body<'_>)> {
  for item in &parsed.module.body {
    let js::ModuleItem::ModuleDecl(js::ModuleDecl::ExportDecl(export)) = item else { continue };
    match &export.decl {
      js::Decl::Fn(f) if &*f.ident.sym == "generateMetadata" => return Some((item, of_block(f.function.body.as_ref()?, !f.function.params.is_empty())?)),
      js::Decl::Var(var) => {
        for decl in &var.decls {
          let (js::Pat::Ident(name), Some(js::Expr::Arrow(arrow))) = (&decl.name, decl.init.as_deref()) else { continue };
          if &*name.id.sym != "generateMetadata" {
            continue;
          }
          let takes_params = !arrow.params.is_empty();
          return Some((
            item,
            match &*arrow.body {
              js::ArrowFunctionBody::FunctionBody(block) => of_block(block, takes_params)?,
              js::ArrowFunctionBody::Expr(e) => Body { stmts: Vec::new(), returned: unparen(e), takes_params },
            },
          ));
        }
      }
      _ => {}
    }
  }
  None
}

fn of_block(block: &js::FunctionBody, takes_params: bool) -> Option<Body<'_>> {
  let (last, before) = block.stmts.split_last()?;
  let js::Stmt::Return(js::ReturnStmt { arg: Some(returned), .. }) = last else { return None };
  Some(Body { stmts: before.iter().collect(), returned, takes_params })
}

/// The page without `generateMetadata` and the loader with the `meta` it becomes; the reason it cannot move otherwise.
fn rewrite(path: &str, text: &str, parsed: &Parsed, item: &js::ModuleItem, body: &Body, loader_path: &str, loader: &str) -> Result<(String, String), String> {
  if body.takes_params {
    return Err("it reads the route's params, which `meta` does not receive".to_owned());
  }
  let returned = loaded_fields(loader_path, loader);
  let mut statements = String::new();
  let mut locals: BTreeSet<String> = BTreeSet::new();
  let mut needed: BTreeSet<String> = BTreeSet::new();
  let mut dropped: BTreeSet<String> = BTreeSet::new();
  for stmt in &body.stmts {
    if let Some((name, read)) = awaited(stmt) {
      dropped.extend(reads(read));
      if !returned.contains(&name) {
        return Err(format!("it awaits `{name}`, which `load` does not return"));
      }
      statements.push_str(&format!("  const {name} = data.{name};\n"));
      locals.insert(name);
      continue;
    }
    if holds(*stmt).awaits {
      return Err("it awaits in a statement other than `const x = await ...`".to_owned());
    }
    needed.extend(reads(*stmt));
    locals.extend(crate::lift::binds(stmt));
    statements.push_str(&format!("  {}\n", &text[parsed.range(stmt.span())]));
  }
  needed.extend(reads(body.returned));
  needed.retain(|n| !locals.contains(n) && n != "data");
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
  let expr = &text[parsed.range(body.returned.span())];
  let meta = match head_rows {
    Some(rows) => format!("export const meta = ({{ data }}) => {{\n{statements}  const next = fromNextMetadata({expr});\n  return {{ ...next, head: [...next.head, {rows}] }};\n}};\n"),
    None => format!("export const meta = ({{ data }}) => {{\n{statements}  return fromNextMetadata({expr});\n}};\n"),
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

/// `const x = await e;` as `x` and `e`.
fn awaited(stmt: &js::Stmt) -> Option<(String, &js::Expr)> {
  let js::Stmt::Decl(js::Decl::Var(var)) = stmt else { return None };
  let [decl] = var.decls.as_slice() else { return None };
  let (js::Pat::Ident(name), Some(js::Expr::Await(a))) = (&decl.name, decl.init.as_deref()) else { return None };
  Some((name.id.sym.to_string(), &a.arg))
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
