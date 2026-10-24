//! Next.js Pages Router: the file layout, `_app` as the root layout and the
//! data functions as the route's loader.

use std::collections::{BTreeMap, BTreeSet};
use std::ops::Range;

use swc_core::common::Spanned;
use swc_core::ecma::ast as js;
use swc_core::ecma::visit::{Visit, VisitWith};

use crate::edit::Edits;
use crate::items::{describe, drop_named_import, import_text, line_range, local, tidy, Item};
use crate::next::{read_packages, source_path};
use crate::relocate::{relocate, Aliases};
use crate::syntax::{is_script, parse, Parsed};
use crate::{Pass, Report, Tree};

pub(crate) fn passes() -> Vec<Box<dyn Pass>> {
  vec![Box::new(Layout), Box::new(AppShell), Box::new(DataFunctions)]
}

/// Moves `pages/` to `app/routes/`, each page into a directory of its own:
/// `pages/blog/[slug].tsx` is `routes/blog/[slug]/page.tsx` and
/// `pages/api/hello.ts` is `routes/api/hello/route.ts`. `_app` becomes the
/// root layout, `404` the not-found page and `_error` the root error
/// boundary; `_document` is dropped, since the host writes the document.
struct Layout;

impl Pass for Layout {
  fn name(&self) -> &'static str {
    "next-pages-layout"
  }

  fn run(&self, tree: &mut Tree, report: &mut Report) {
    let root = if tree.paths().any(|p| p.starts_with("src/pages/")) { "src/" } else { "" };
    let pages = format!("{root}pages/");
    read_packages(tree, report);
    let mut moves = BTreeMap::new();
    for path in tree.paths() {
      if let Some(rest) = path.strip_prefix(&pages) {
        if let Some(to) = route_path(path, rest, report) {
          moves.insert(path.to_owned(), to);
        }
      } else if let Some(rest) = path.strip_prefix("public/") {
        moves.insert(path.to_owned(), format!("app/public/{rest}"));
      } else if let Some(to) = source_path(path, root, report) {
        moves.insert(path.to_owned(), to);
      }
    }
    *tree = relocate(tree, &moves, &Aliases::read(tree), report);
  }
}

fn route_path(path: &str, rest: &str, report: &mut Report) -> Option<String> {
  let (dirs, file) = rest.rsplit_once('/').unwrap_or(("", rest));
  if file.contains(".test.") || file.contains(".spec.") {
    return Some(format!("app/tests/{rest}"));
  }
  if !is_script(file) {
    return Some(format!("app/routes/{rest}"));
  }
  let (stem, ext) = file.rsplit_once('.').unwrap_or((file, "tsx"));
  for dir in dirs.split('/') {
    if dir.starts_with("[[...") {
      report.note(path, 0, format!("`{dir}` is an optional catch-all; FSR's `[...rest]` needs at least one segment"));
    }
  }
  if dirs.is_empty() {
    match stem {
      "_app" => return Some(format!("app/routes/layout.{ext}")),
      "_document" => {
        report.note(path, 0, "`_document` was dropped: the host writes the document, `<html lang>` from the request's locale");
        return None;
      }
      "_error" => {
        report.note(path, 0, "`_error` became `routes/error`, which receives `{ error, kind }` where Next passed `statusCode`");
        return Some(format!("app/routes/error.{ext}"));
      }
      "404" => return Some(format!("app/routes/not-found.{ext}")),
      "500" => {
        report.note(path, 0, "a `500` page has no FSR counterpart of its own: `routes/error` answers every failure");
        return Some(format!("app/routes/error.500.{ext}"));
      }
      _ => {}
    }
  }
  let api = dirs == "api" || dirs.starts_with("api/");
  let mut parts: Vec<&str> = dirs.split('/').filter(|d| !d.is_empty()).collect();
  if stem.starts_with("[[...") {
    report.note(path, 0, format!("`{stem}` is an optional catch-all; FSR's `[...rest]` needs at least one segment"));
  }
  if stem != "index" {
    parts.push(stem);
  }
  let dir = if parts.is_empty() { String::new() } else { format!("{}/", parts.join("/")) };
  if api {
    report.note(path, 0, "a Next API route takes `(req, res)`; an FSR `route.ts` exports `GET`, `POST` and the rest, each taking the request context and returning the answer");
    let ext = if ext.starts_with('j') || ext == "js" { "ts" } else { ext.trim_end_matches('x') };
    return Some(format!("app/routes/{dir}route.{ext}"));
  }
  Some(format!("app/routes/{dir}page.{ext}"))
}

/// `_app`, now the root layout: `{ Component, pageProps }` becomes
/// `{ children }` and `<Component {...pageProps} />` becomes `{children}`.
struct AppShell;

impl Pass for AppShell {
  fn name(&self) -> &'static str {
    "next-pages-app"
  }

  fn run(&self, tree: &mut Tree, report: &mut Report) {
    let Some(path) = ["tsx", "jsx", "js", "ts"].iter().map(|e| format!("app/routes/layout.{e}")).find(|p| tree.contains(p)) else { return };
    let Some(text) = tree.text(&path).map(str::to_owned) else { return };
    let Ok(parsed) = parse(&path, &text) else { return };
    let Some(app) = default_component(&parsed.module) else {
      report.note(&path, 0, "the default export of `_app` is not a function this pass can read, so it still takes `{ Component, pageProps }`");
      return;
    };
    let mut edits = Edits::default();
    if let Some((span, wrapper, inner)) = &app.wrapped {
      edits.replace(parsed.range(*span), inner.clone());
      report.note(&path, parsed.line(*span), format!("`{wrapper}` wrapped the app for Next and was dropped; a provider it set up goes in an island of the layout"));
    }
    let Some(param) = app.params.first() else {
      tree.insert(path, edits.apply(&text));
      return;
    };
    let js::Pat::Object(object) = param else {
      report.note(&path, parsed.line(param.span()), "`_app`'s props are not destructured, so they were not rewritten to `{ children }`");
      return;
    };
    let names: Vec<String> = object.props.iter().filter_map(|p| match p {
      js::ObjectPatProp::Assign(a) => Some(a.key.sym.to_string()),
      js::ObjectPatProp::KeyValue(js::KeyValuePatProp { value, .. }) => match &**value {
        js::Pat::Ident(i) => Some(i.id.sym.to_string()),
        _ => None,
      },
      _ => None,
    }).collect();
    let component = names.iter().find(|n| *n == "Component").cloned().unwrap_or_else(|| "Component".to_owned());
    edits.replace(parsed.range(param.span()), "{ children }");
    let mut finder = Placements { name: component.clone(), found: Vec::new(), other: Vec::new() };
    app.body.visit_with(&mut finder);
    for span in &finder.found {
      edits.replace(parsed.range(*span), "<>{children}</>");
    }
    for span in &finder.other {
      report.note(&path, parsed.line(*span), format!("`{component}` is read other than placed; FSR's layout receives the page as `children`"));
    }
    let page_props = object.props.iter().find_map(|p| match p {
      js::ObjectPatProp::Assign(a) if &*a.key.sym == "pageProps" => Some(a.key.sym.to_string()),
      js::ObjectPatProp::KeyValue(js::KeyValuePatProp { key: js::PropName::Ident(key), value }) if &*key.sym == "pageProps" => match &**value {
        js::Pat::Ident(i) => Some(i.id.sym.to_string()),
        _ => None,
      },
      _ => None,
    });
    if let Some(name) = page_props {
      let outside = |span: swc_core::common::Span| {
        let at = parsed.range(span);
        let param = parsed.range(param.span());
        !(param.start <= at.start && at.end <= param.end) && finder.found.iter().all(|f| { let f = parsed.range(*f); !(f.start <= at.start && at.end <= f.end) })
      };
      let mut uses = PageProps { name, spreads: Vec::new(), reads: Vec::new() };
      app.body.visit_page_props(&mut uses);
      for dots in uses.spreads.into_iter().filter(|s| outside(*s)) {
        let at = parsed.range(dots).start;
        let (Some(open), Some(close)) = (text[..at].rfind('{'), text[at..].find('}').map(|i| at + i + 1)) else { continue };
        let open = if text[..open].ends_with(' ') { open - 1 } else { open };
        edits.replace(open..close, "");
        report.note(&path, parsed.line(dots), format!("`{{...{}}}` was taken off this element: an FSR layout receives its own loader's data, not the page's props", uses.name));
      }
      for read in uses.reads.into_iter().filter(|s| outside(*s)) {
        edits.replace(parsed.range(read), "({})");
        report.note(&path, parsed.line(read), format!("`{}` reads the page's props, which an FSR layout does not receive, so it reads an empty object", uses.name));
      }
    }
    let mut out = edits.apply(&text);
    if let Some(stripped) = drop_named_import(&out, "next/app", "AppProps") {
      out = stripped;
    }
    if let Some((_, wrapper, _)) = &app.wrapped {
      out = tidy(&crate::items::prune(&path, &out, &[wrapper.clone()].into_iter().collect()));
    }
    tree.insert(path, out);
  }
}

/// `_app`'s component: its parameters and its body. `wrapped` is set when
/// the default export wraps it in a call: that call's span, the callee and
/// the component's name.
struct AppComponent<'a> {
  params: Vec<&'a js::Pat>,
  body: &'a dyn AppBody,
  wrapped: Option<(swc_core::common::Span, String, String)>,
}

trait AppBody {
  fn visit_with(&self, v: &mut Placements);
  fn visit_page_props(&self, v: &mut PageProps);
}

impl AppBody for js::Function {
  fn visit_with(&self, v: &mut Placements) {
    VisitWith::visit_with(self, v);
  }

  fn visit_page_props(&self, v: &mut PageProps) {
    VisitWith::visit_with(self, v);
  }
}

impl AppBody for js::ArrowExpr {
  fn visit_with(&self, v: &mut Placements) {
    VisitWith::visit_with(self, v);
  }

  fn visit_page_props(&self, v: &mut PageProps) {
    VisitWith::visit_with(self, v);
  }
}

/// The `...` of each `{...pageProps}` spread on an element and every other
/// read of the name.
struct PageProps {
  name: String,
  spreads: Vec<swc_core::common::Span>,
  reads: Vec<swc_core::common::Span>,
}

impl Visit for PageProps {
  fn visit_jsx_attr_or_spread(&mut self, n: &js::JSXAttrOrSpread) {
    if let js::JSXAttrOrSpread::SpreadElement(spread) = n {
      if matches!(&*spread.expr, js::Expr::Ident(i) if *i.sym == *self.name) {
        self.spreads.push(spread.dot3_token);
        return;
      }
    }
    n.visit_children_with(self);
  }

  fn visit_ident(&mut self, n: &js::Ident) {
    if *n.sym == *self.name {
      self.reads.push(n.span);
    }
  }
}

fn default_component(module: &js::Module) -> Option<AppComponent<'_>> {
  let mut named = None;
  let mut wrapped = None;
  for item in &module.body {
    match item {
      js::ModuleItem::ModuleDecl(js::ModuleDecl::ExportDefaultDecl(js::ExportDefaultDecl { decl: js::DefaultDecl::Fn(f), .. })) => {
        return Some(AppComponent { params: f.function.params.iter().map(|p| &p.pat).collect(), body: &*f.function, wrapped: None });
      }
      js::ModuleItem::ModuleDecl(js::ModuleDecl::ExportDefaultExpr(e)) => match &*e.expr {
        js::Expr::Ident(i) => named = Some(i.sym.to_string()),
        js::Expr::Call(call) => {
          if let Some((callee, inner)) = wrapped_component(call, module) {
            named = Some(inner.clone());
            wrapped = Some((call.span, callee, inner));
          }
        }
        _ => {}
      },
      _ => {}
    }
  }
  let named = named?;
  module.body.iter().find_map(|item| match item {
    js::ModuleItem::Stmt(js::Stmt::Decl(js::Decl::Fn(f))) if f.ident.sym == *named => {
      Some(AppComponent { params: f.function.params.iter().map(|p| &p.pat).collect(), body: &*f.function, wrapped: wrapped.clone() })
    }
    js::ModuleItem::Stmt(js::Stmt::Decl(js::Decl::Var(v))) => v.decls.iter().find_map(|d| match (&d.name, d.init.as_deref()) {
      (js::Pat::Ident(i), Some(js::Expr::Arrow(arrow))) if i.id.sym == *named => Some(AppComponent { params: arrow.params.iter().collect(), body: arrow, wrapped: wrapped.clone() }),
      (js::Pat::Ident(i), Some(js::Expr::Fn(f))) if i.id.sym == *named => Some(AppComponent { params: f.function.params.iter().map(|p| &p.pat).collect(), body: &*f.function, wrapped: wrapped.clone() }),
      _ => None,
    }),
    _ => None,
  })
}

/// `hoc(App)`, `hoc(options)(App)` or `hoc(App, options)` with `App` a
/// function the module declares: the outermost callee and `App`.
fn wrapped_component(call: &js::CallExpr, module: &js::Module) -> Option<(String, String)> {
  let declared = |name: &str| {
    module.body.iter().any(|item| match item {
      js::ModuleItem::Stmt(js::Stmt::Decl(js::Decl::Fn(f))) => *f.ident.sym == *name,
      js::ModuleItem::Stmt(js::Stmt::Decl(js::Decl::Var(v))) => v.decls.iter().any(|d| matches!(&d.name, js::Pat::Ident(i) if *i.id.sym == *name)),
      _ => false,
    })
  };
  let callee = match &call.callee {
    js::Callee::Expr(e) => match &**e {
      js::Expr::Ident(i) => i.sym.to_string(),
      js::Expr::Call(inner) => match &inner.callee {
        js::Callee::Expr(e) => match &**e {
          js::Expr::Ident(i) => i.sym.to_string(),
          _ => return None,
        },
        _ => return None,
      },
      _ => return None,
    },
    _ => return None,
  };
  let inner = call.args.iter().find_map(|a| match &*a.expr {
    js::Expr::Ident(i) if declared(&i.sym) => Some(i.sym.to_string()),
    _ => None,
  })?;
  Some((callee, inner))
}

struct Placements {
  name: String,
  found: Vec<swc_core::common::Span>,
  other: Vec<swc_core::common::Span>,
}

impl Visit for Placements {
  fn visit_jsx_element(&mut self, n: &js::JSXElement) {
    if let js::JSXElementName::Ident(i) = &n.opening.name {
      if *i.sym == *self.name {
        self.found.push(n.span);
        return;
      }
    }
    n.visit_children_with(self);
  }

  fn visit_ident(&mut self, n: &js::Ident) {
    if *n.sym == *self.name {
      self.other.push(n.span);
    }
  }
}

/// `getStaticProps` and `getServerSideProps` moved to `page.loader.ts` as
/// `load`, `return { props }` returning the props themselves and
/// `return { notFound: true }` failing `not_found`; `getStaticPaths` moved
/// beside it as `paths`, each `{ params }` its params. The imports and
/// module-level declarations they read go with them and the page keeps only
/// what it still reads.
struct DataFunctions;

impl Pass for DataFunctions {
  fn name(&self) -> &'static str {
    "next-pages-data"
  }

  fn run(&self, tree: &mut Tree, report: &mut Report) {
    let pages: Vec<String> = tree.paths().filter(|p| p.starts_with("app/routes/") && is_page(p)).map(str::to_owned).collect();
    for path in pages {
      let Some(text) = tree.text(&path).map(str::to_owned) else { continue };
      let Ok(parsed) = parse(&path, &text) else { continue };
      let Some(moved) = data_functions(&path, &text, &parsed, report) else { continue };
      let loader_path = format!("{}/page.loader.ts", path.rsplit_once('/').map(|(d, _)| d).unwrap_or(""));
      if tree.contains(&loader_path) {
        report.note(&path, 0, "the route already has a `page.loader.ts`, so its data functions were left in place");
        continue;
      }
      tree.insert(loader_path, moved.loader);
      tree.insert(path, moved.page);
    }
  }
}

fn is_page(path: &str) -> bool {
  let file = path.rsplit('/').next().unwrap_or(path);
  file.starts_with("page.") && !file.starts_with("page.loader.") && file.matches('.').count() == 1
}

struct Moved {
  loader: String,
  page: String,
}

/// Whether a node holds a JSX element or fragment.
struct Jsx(bool);

impl Visit for Jsx {
  fn visit_jsx_element(&mut self, _: &js::JSXElement) {
    self.0 = true;
  }

  fn visit_jsx_fragment(&mut self, _: &js::JSXFragment) {
    self.0 = true;
  }
}

pub(crate) const DATA_FUNCTIONS: &[&str] = &["getStaticProps", "getServerSideProps", "getStaticPaths"];

fn data_functions(path: &str, text: &str, parsed: &Parsed, report: &mut Report) -> Option<Moved> {
  let items: Vec<Item> = parsed.module.body.iter().map(|item| describe(item, parsed)).collect();
  if !items.iter().any(|i| i.data.is_some()) {
    return None;
  }
  let mut needed: BTreeSet<String> = items.iter().filter(|i| i.data.is_some()).flat_map(|i| i.reads.iter().cloned()).collect();
  let mut moved_items: BTreeSet<usize> = items.iter().enumerate().filter(|(_, i)| i.data.is_some()).map(|(n, _)| n).collect();
  loop {
    let before = moved_items.len();
    for (n, item) in items.iter().enumerate() {
      if item.data.is_none() && !item.import && item.binds.iter().any(|b| needed.contains(b)) && moved_items.insert(n) {
        needed.extend(item.reads.iter().cloned());
      }
    }
    if moved_items.len() == before {
      break;
    }
  }
  if let Some(n) = moved_items.iter().copied().find(|n| {
    let mut found = Jsx(false);
    parsed.module.body[*n].visit_with(&mut found);
    found.0
  }) {
    report.note(path, parsed.line(parsed.module.body[n].span()), "the data functions render JSX, which a loader cannot, since it returns data; they were left in place");
    return None;
  }

  let mut loader = String::new();
  let mut fails = false;
  for (n, item) in parsed.module.body.iter().enumerate() {
    if let js::ModuleItem::ModuleDecl(js::ModuleDecl::Import(import)) = item {
      let kept: Vec<&js::ImportSpecifier> = import.specifiers.iter().filter(|s| needed.contains(&local(s))).collect();
      if !kept.is_empty() {
        loader.push_str(&import_text(text, parsed, import, &kept));
      }
      continue;
    }
    if !moved_items.contains(&n) {
      continue;
    }
    let source = &text[items[n].range.clone()];
    match items[n].data {
      Some(name) => {
        let (rewritten, failed) = rewrite_data_function(path, text, parsed, item, name, report);
        fails |= failed;
        loader.push('\n');
        loader.push_str(&rewritten);
        loader.push('\n');
      }
      None => {
        loader.push('\n');
        loader.push_str(source);
        loader.push('\n');
      }
    }
  }
  if fails {
    loader.insert_str(0, "import { fail } from \"@snapfire/fsr\";\n");
  }

  let mut kept: BTreeSet<usize> = items.iter().enumerate().filter(|(_, i)| !i.import && i.data.is_none()).map(|(n, _)| n).collect();
  loop {
    let unread = kept.iter().copied().find(|&n| {
      moved_items.contains(&n) && !items[n].binds.iter().any(|b| kept.iter().any(|&m| m != n && items[m].reads.contains(b)))
    });
    match unread {
      Some(n) => {
        kept.remove(&n);
      }
      None => break,
    }
  }
  let still_read: BTreeSet<String> = kept.iter().flat_map(|&n| items[n].reads.iter().cloned()).collect();
  let mut page_edits = Edits::default();
  for (n, item) in items.iter().enumerate() {
    if !item.import && !kept.contains(&n) {
      page_edits.replace(line_range(text, item.range.clone()), "");
    }
  }
  for item in &parsed.module.body {
    let js::ModuleItem::ModuleDecl(js::ModuleDecl::Import(import)) = item else { continue };
    if import.specifiers.is_empty() {
      continue;
    }
    let kept: Vec<&js::ImportSpecifier> = import.specifiers.iter().filter(|s| still_read.contains(&local(s))).collect();
    if kept.len() < import.specifiers.len() {
      page_edits.replace(line_range(text, parsed.range(import.span)), if kept.is_empty() { String::new() } else { import_text(text, parsed, import, &kept) });
    }
  }
  Some(Moved { loader: loader.trim_start().to_owned(), page: tidy(&page_edits.apply(text)) })
}

/// The data function `item` rewritten under FSR's name for it, plus whether
/// the rewrite calls `fail`.
fn rewrite_data_function(path: &str, text: &str, parsed: &Parsed, item: &js::ModuleItem, name: &str, report: &mut Report) -> (String, bool) {
  let range = parsed.range(item.span());
  let mut edits = Edits::default();
  let mut returns = Returns { depth: 0, found: Vec::new() };
  let fsr_name = if name == "getStaticPaths" { "paths" } else { "load" };
  if let js::ModuleItem::ModuleDecl(js::ModuleDecl::ExportDecl(e)) = item {
    match &e.decl {
      js::Decl::Fn(f) => {
        edits.replace(shift(parsed.range(f.ident.span), range.start), fsr_name);
        f.function.body.visit_with(&mut returns);
        let params: Vec<&js::Pat> = f.function.params.iter().map(|p| &p.pat).collect();
        let body = f.function.body.as_ref().map(|b| parsed.range(b.span).start + 1);
        context(path, text, parsed, &params, body, range.clone(), &mut edits, report);
      }
      js::Decl::Var(v) => {
        if let Some(d) = v.decls.first() {
          if let js::Pat::Ident(i) = &d.name {
            let ident = parsed.range(i.id.span);
            edits.replace(shift(ident.start..ident.end, range.start), fsr_name);
            if let Some(type_ann) = &i.type_ann {
              edits.replace(shift(parsed.range(type_ann.span), range.start), "");
            }
          }
          match d.init.as_deref() {
            Some(js::Expr::Arrow(a)) => {
              a.body.visit_with(&mut returns);
              let params: Vec<&js::Pat> = a.params.iter().collect();
              let body = match &*a.body {
                js::ArrowFunctionBody::FunctionBody(b) => Some(parsed.range(b.span).start + 1),
                js::ArrowFunctionBody::Expr(_) => None,
              };
              context(path, text, parsed, &params, body, range.clone(), &mut edits, report);
            }
            Some(js::Expr::Fn(f)) => {
              f.function.body.visit_with(&mut returns);
              let params: Vec<&js::Pat> = f.function.params.iter().map(|p| &p.pat).collect();
              let body = f.function.body.as_ref().map(|b| parsed.range(b.span).start + 1);
              context(path, text, parsed, &params, body, range.clone(), &mut edits, report);
            }
            _ => {}
          }
        }
      }
      _ => {}
    }
  }
  let mut fails = false;
  for ret in returns.found {
    let Some(arg) = &ret.arg else { continue };
    let line = parsed.line(ret.span);
    let js::Expr::Object(object) = strip_parens(arg) else {
      report.note(path, line, format!("`{name}` returns something other than an object literal, which was left as it is"));
      continue;
    };
    let field = |key: &str| {
      object.props.iter().find_map(|p| match p {
        js::PropOrSpread::Prop(prop) => match &**prop {
          js::Prop::KeyValue(kv) if prop_name(&kv.key).as_deref() == Some(key) => Some(&*kv.value),
          js::Prop::Shorthand(i) if &*i.sym == key => None,
          _ => None,
        },
        _ => None,
      })
    };
    let arg_range = shift(parsed.range(arg.span()), range.start);
    if name == "getStaticPaths" {
      match field("paths") {
        Some(paths) => edits.replace(arg_range, paths_text(text, parsed, paths)),
        None => report.note(path, line, "`getStaticPaths` returns no `paths`, which was left as it is"),
      }
      continue;
    }
    if let Some(props) = field("props") {
      let props_range = shift(parsed.range(props.span()), range.start);
      edits.replace(arg_range.start..props_range.start, "");
      edits.replace(props_range.end..arg_range.end, "");
      if field("revalidate").is_some() {
        report.note(path, line, "`revalidate` has no counterpart on a loader; FSR's data cache sets its time to live under `[cache]`");
      }
    } else if field("notFound").is_some() {
      let stmt = shift(parsed.range(ret.span), range.start);
      edits.replace(stmt, "fail(\"not_found\", \"not found\");");
      fails = true;
    } else if field("redirect").is_some() {
      report.note(path, line, "a redirect from a data function belongs in `middleware.ts`, which answers `Response.redirect(to, 307)` before the route runs");
    } else {
      report.note(path, line, format!("`{name}` returns an object with no `props`, which was left as it is"));
    }
  }
  let source = &text[range.clone()];
  if name == "getServerSideProps" && (source.contains("req.") || source.contains("res.") || source.contains("{ req") || source.contains("{ res")) {
    report.note(path, parsed.line(item.span()), "`getServerSideProps` reads `req` or `res`; FSR's loader reads `session`, `query` and `params` from its context");
  }
  (edits.apply(source), fails)
}

/// Fields of Next's context FSR's loader context has no counterpart for.
/// Draft mode is off, so `preview` and `draftMode` are `false`; the others
/// are notes.
const UNREAD_CONTEXT: &[&str] = &["preview", "previewData", "draftMode", "defaultLocale", "req", "res", "resolvedUrl"];

/// The data function's context parameter without the fields in
/// `UNREAD_CONTEXT`: taken out of a destructuring, `preview` and `draftMode`
/// declared `false` at the top of the body; `ctx.preview` is read as
/// `false`.
#[allow(clippy::too_many_arguments)]
fn context(path: &str, text: &str, parsed: &Parsed, params: &[&js::Pat], body: Option<usize>, item: Range<usize>, edits: &mut Edits, report: &mut Report) {
  let Some(param) = params.first() else { return };
  let mut declared = Vec::new();
  match param {
    js::Pat::Object(object) => {
      let mut kept = Vec::new();
      let mut dropped = false;
      for prop in &object.props {
        let key = match prop {
          js::ObjectPatProp::Assign(a) => Some(a.key.sym.to_string()),
          js::ObjectPatProp::KeyValue(kv) => prop_name(&kv.key),
          js::ObjectPatProp::Rest(_) => None,
        };
        match key {
          Some(key) if UNREAD_CONTEXT.contains(&key.as_str()) => {
            dropped = true;
            let local = match prop {
              js::ObjectPatProp::KeyValue(js::KeyValuePatProp { value, .. }) => match &**value {
                js::Pat::Ident(i) => i.id.sym.to_string(),
                _ => key.clone(),
              },
              _ => key.clone(),
            };
            match key.as_str() {
              "preview" | "draftMode" => declared.push(format!("const {local} = false;")),
              _ => report.note(path, parsed.line(prop.span()), format!("`{key}` has no counterpart in FSR's loader context, so it was taken out of the parameter")),
            }
          }
          _ => kept.push(&text[parsed.range(prop.span())]),
        }
      }
      if dropped {
        let pattern = parsed.range(object.span);
        let typed = object.type_ann.as_ref().filter(|t| parsed.range(t.span).end <= pattern.end).map(|t| format!(": {}", text[parsed.range(t.type_ann.span())].trim()));
        let braces = if kept.is_empty() { "{}".to_owned() } else { format!("{{ {} }}", kept.join(", ")) };
        edits.replace(shift(pattern, item.start), format!("{braces}{}", typed.unwrap_or_default()));
      }
    }
    js::Pat::Ident(ident) => {
      let mut reads = ContextReads { name: ident.id.sym.to_string(), found: Vec::new() };
      parsed.module.visit_with(&mut reads);
      for (span, field) in reads.found {
        let at = parsed.range(span);
        if at.start < item.start || at.end > item.end {
          continue;
        }
        match field.as_str() {
          "preview" | "draftMode" => edits.replace(shift(at, item.start), "false"),
          f if UNREAD_CONTEXT.contains(&f) => report.note(path, parsed.line(span), format!("`{f}` has no counterpart in FSR's loader context")),
          _ => {}
        }
      }
    }
    _ => {}
  }
  if let (Some(body), false) = (body, declared.is_empty()) {
    edits.replace(body - item.start..body - item.start, format!("\n  {}", declared.join("\n  ")));
  }
}

/// Reads of `name.<field>`.
struct ContextReads {
  name: String,
  found: Vec<(swc_core::common::Span, String)>,
}

impl Visit for ContextReads {
  fn visit_member_expr(&mut self, n: &js::MemberExpr) {
    if let (js::Expr::Ident(obj), js::MemberProp::Ident(prop)) = (&*n.obj, &n.prop) {
      if *obj.sym == *self.name {
        self.found.push((n.span, prop.sym.to_string()));
        return;
      }
    }
    n.visit_children_with(self);
  }
}

/// `paths` from `getStaticPaths` as FSR's `paths` returns them: an array
/// literal of `{ params: x }` as the `x`s, anything else mapped to its params.
fn paths_text(text: &str, parsed: &Parsed, paths: &js::Expr) -> String {
  if let js::Expr::Array(array) = strip_parens(paths) {
    let mut out = Vec::new();
    for elem in array.elems.iter().flatten() {
      if elem.spread.is_some() {
        out.clear();
        break;
      }
      let js::Expr::Object(o) = strip_parens(&elem.expr) else {
        out.clear();
        break;
      };
      let params = o.props.iter().find_map(|p| match p {
        js::PropOrSpread::Prop(prop) => match &**prop {
          js::Prop::KeyValue(kv) if prop_name(&kv.key).as_deref() == Some("params") => Some(&text[parsed.range(kv.value.span())]),
          _ => None,
        },
        _ => None,
      });
      match params {
        Some(p) if o.props.len() == 1 => out.push(p.to_owned()),
        _ => {
          out.clear();
          break;
        }
      }
    }
    if out.len() == array.elems.len() {
      return format!("[{}]", out.join(", "));
    }
  }
  format!("({}).map((p) => p.params)", &text[parsed.range(paths.span())])
}

fn prop_name(key: &js::PropName) -> Option<String> {
  match key {
    js::PropName::Ident(i) => Some(i.sym.to_string()),
    js::PropName::Str(s) => Some(s.value.to_string_lossy().into_owned()),
    _ => None,
  }
}

fn strip_parens(e: &js::Expr) -> &js::Expr {
  match e {
    js::Expr::Paren(p) => strip_parens(&p.expr),
    js::Expr::TsAs(a) => strip_parens(&a.expr),
    js::Expr::TsSatisfies(s) => strip_parens(&s.expr),
    other => other,
  }
}

fn shift(range: Range<usize>, by: usize) -> Range<usize> {
  range.start - by..range.end - by
}

/// The `return`s of one function body, not of the functions inside it.
struct Returns {
  depth: usize,
  found: Vec<js::ReturnStmt>,
}

impl Visit for Returns {
  fn visit_return_stmt(&mut self, n: &js::ReturnStmt) {
    if self.depth == 0 {
      self.found.push(n.clone());
    }
  }

  fn visit_function(&mut self, n: &js::Function) {
    self.depth += 1;
    n.visit_children_with(self);
    self.depth -= 1;
  }

  fn visit_arrow_expr(&mut self, n: &js::ArrowExpr) {
    self.depth += 1;
    n.visit_children_with(self);
    self.depth -= 1;
  }
}

#[cfg(test)]
mod tests {
  use crate::{migrate, Source, Tree};

  fn tree(files: &[(&str, &str)]) -> Tree {
    let mut t = Tree::new();
    for (p, s) in files {
      t.insert(*p, *s);
    }
    t
  }

  const PACKAGE: &str = r#"{ "dependencies": { "next": "latest" } }"#;

  #[test]
  fn each_page_moves_into_a_directory_of_its_own() {
    let input = tree(&[
      ("package.json", PACKAGE),
      ("pages/index.js", "export default function Home() { return <p>home</p>; }\n"),
      ("pages/about.tsx", "export default function About() { return null; }\n"),
      ("pages/blog/[slug].tsx", "export default function Post() { return null; }\n"),
      ("pages/api/hello.js", "export default function handler(req, res) {}\n"),
      ("pages/404.tsx", "export default function Missing() { return null; }\n"),
      ("pages/_document.tsx", "export default function D() { return null; }\n"),
    ]);
    let (out, report) = migrate(&input);
    assert_eq!(report.source, Some(Source::NextPages));
    let paths: Vec<&str> = out.paths().collect();
    assert_eq!(paths, ["app/routes/about/page.tsx", "app/routes/api/hello/route.ts", "app/routes/blog/[slug]/page.tsx", "app/routes/not-found.tsx", "app/routes/page.js"]);
  }

  #[test]
  fn app_becomes_the_root_layout_placing_children() {
    let input = tree(&[
      ("package.json", PACKAGE),
      (
        "pages/_app.tsx",
        "import type { AppProps } from \"next/app\";\nimport \"../styles/globals.css\";\n\nexport default function MyApp({ Component, pageProps }: AppProps) {\n  return <Layout><Component {...pageProps} /></Layout>;\n}\n",
      ),
      ("styles/globals.css", "body {}\n"),
    ]);
    let (out, _) = migrate(&input);
    assert_eq!(
      out.text("app/routes/layout.tsx").unwrap(),
      "import \"../src/styles/globals.css\";\n\nexport default function MyApp({ children }) {\n  return <Layout><>{children}</></Layout>;\n}\n"
    );
    assert_eq!(out.text("app/src/styles/globals.css").unwrap(), "body {}\n", "the root layout links it on every document");
  }

  #[test]
  fn draft_mode_fields_leave_the_context_and_preview_is_false() {
    let page = "import { getPost } from \"../lib/api\";\n\nexport default function P({ post }) {\n  return <p>{post}</p>;\n}\n\nexport async function getStaticProps({ params, preview = false, locales }) {\n  return { props: { post: await getPost(params.slug, preview) } };\n}\n";
    let other = "export default function Q({ q }) {\n  return <p>{q}</p>;\n}\n\nexport const getServerSideProps = async (context) => {\n  return { props: { q: context.preview ? 1 : 0 } };\n};\n";
    let input = tree(&[("package.json", PACKAGE), ("pages/[slug].js", page), ("pages/q.js", other), ("lib/api.js", "export function getPost() {}\n")]);
    let (out, report) = migrate(&input);
    assert_eq!(
      out.text("app/routes/[slug]/page.loader.ts").unwrap(),
      "import { getPost } from \"../../src/lib/api\";\n\nexport async function load({ params, locales }) {\n  const preview = false;\n  return { post: await getPost(params.slug, preview) };\n}\n"
    );
    assert_eq!(out.text("app/routes/q/page.loader.ts").unwrap(), "export const load = async (context) => {\n  return { q: false ? 1 : 0 };\n};\n");
    assert_eq!(report.notes.iter().filter(|n| n.message.contains("`locales`")).count(), 0, "`ctx.locales` reads them: {:?}", report.notes);
  }

  #[test]
  fn an_arrow_app_wrapped_in_a_library_hoc_becomes_the_layout_without_the_wrapper() {
    let input = tree(&[
      ("package.json", PACKAGE),
      ("pages/_app.js", "import { withGraphQLApp } from \"next-graphql-react\";\n\nconst App = ({ Component, pageProps }) => (\n  <main>\n    <Component {...pageProps} />\n  </main>\n);\n\nexport default withGraphQLApp(App);\n"),
    ]);
    let (out, report) = migrate(&input);
    assert_eq!(
      out.text("app/routes/layout.js").unwrap(),
      "const App = ({ children }) => (\n  <main>\n    <>{children}</>\n  </main>\n);\n\nexport default App;\n"
    );
    assert!(report.notes.iter().any(|n| n.message.starts_with("`withGraphQLApp` wrapped the app")), "{:?}", report.notes);
  }

  #[test]
  fn get_static_props_becomes_the_loader_with_what_it_reads() {
    let page = "import fs from \"fs\";\nimport Link from \"next/link\";\nimport { getPost, formatDate } from \"../../lib/posts\";\n\nconst DIR = \"posts\";\n\nexport default function Post({ post }) {\n  return <Link href=\"/\">{formatDate(post.date)}</Link>;\n}\n\nexport async function getStaticProps({ params }) {\n  const post = getPost(DIR, params.slug, fs);\n  if (!post) {\n    return { notFound: true };\n  }\n  return { props: { post }, revalidate: 60 };\n}\n\nexport async function getStaticPaths() {\n  return { paths: [{ params: { slug: \"a\" } }, { params: { slug: \"b\" } }], fallback: false };\n}\n";
    let input = tree(&[("package.json", PACKAGE), ("pages/blog/[slug].js", page), ("lib/posts.js", "export function getPost() {}\nexport function formatDate() {}\n")]);
    let (out, report) = migrate(&input);
    assert_eq!(
      out.text("app/routes/blog/[slug]/page.loader.ts").unwrap(),
      "import { fail } from \"@snapfire/fsr\";\nimport fs from \"fs\";\nimport { getPost } from \"../../../src/lib/posts\";\n\nconst DIR = \"posts\";\n\nexport async function load({ params }) {\n  const post = getPost(DIR, params.slug, fs);\n  if (!post) {\n    fail(\"not_found\", \"not found\");\n  }\n  return { post };\n}\n\nexport async function paths() {\n  return [{ slug: \"a\" }, { slug: \"b\" }];\n}\n"
    );
    assert_eq!(
      out.text("app/routes/blog/[slug]/page.js").unwrap(),
      "import { Link } from \"@snapfire/fsr/components\";\nimport { formatDate } from \"../../../src/lib/posts\";\n\nexport default function Post({ post }) {\n  return <Link href=\"/\">{formatDate(post.date)}</Link>;\n}\n"
    );
    assert!(report.notes.iter().any(|n| n.message.contains("revalidate")), "{:?}", report.notes);
  }

  #[test]
  fn what_app_hands_on_from_page_props_is_dropped_with_a_note() {
    let app = "import { StoreProvider } from \"../components/StoreProvider\";\n\nexport default function App({ Component, pageProps }) {\n  const initial = pageProps.initialState;\n  return (\n    <StoreProvider {...pageProps} initial={initial}>\n      <Component {...pageProps} />\n    </StoreProvider>\n  );\n}\n";
    let (out, report) = migrate(&tree(&[("package.json", r#"{ "dependencies": { "next": "latest" } }"#), ("pages/_app.js", app), ("pages/index.js", "export default function P() {\n  return <p />;\n}\n")]));
    let layout = out.text("app/routes/layout.js").unwrap();
    assert!(layout.contains("<StoreProvider initial={initial}>"), "{layout}");
    assert!(layout.contains("const initial = ({}).initialState;"), "{layout}");
    assert!(layout.contains("<>{children}</>"), "{layout}");
    assert!(!layout.contains("pageProps"), "{layout}");
    let notes: Vec<String> = report.notes.iter().filter(|n| n.message.contains("pageProps")).map(|n| n.to_string()).collect();
    assert_eq!(notes.len(), 2, "{notes:?}");
  }

  #[test]
  fn a_data_function_that_renders_jsx_stays_in_the_page_with_a_note() {
    let page = "import { render } from \"cms\";\n\nexport default function P({ data }) {\n  return <p>{data}</p>;\n}\n\nexport async function getStaticProps() {\n  const data = await render(<Root />);\n  return { props: { data } };\n}\n";
    let (out, report) = migrate(&tree(&[("package.json", r#"{ "dependencies": { "next": "latest" } }"#), ("pages/index.js", page)]));
    assert!(!out.contains("app/routes/page.loader.ts"), "{:?}", out.paths().collect::<Vec<_>>());
    assert!(out.text("app/routes/page.js").unwrap().contains("getStaticProps"));
    assert!(report.notes.iter().any(|n| n.file == "app/routes/page.js" && n.message.contains("render JSX")), "{:?}", report.notes);
  }
}
