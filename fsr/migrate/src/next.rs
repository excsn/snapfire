//! Next.js App Router: the file layout, then the root layout's document.

use std::collections::BTreeMap;

use swc_core::common::Spanned;
use swc_core::ecma::ast as js;
use swc_core::ecma::visit::{Visit, VisitWith};

use crate::edit::Edits;
use crate::items::{drop_named_import, line_range};
use crate::relocate::{relocate, Aliases};
use crate::syntax::parse;
use crate::{Pass, Report, Tree};

pub(crate) fn app_passes() -> Vec<Box<dyn Pass>> {
  vec![Box::new(Layout), Box::new(Document), Box::new(Metadata)]
}

/// Files that configure Next or the tooling around it and have no place in an
/// FSR application.
const TOOLING: &[&str] = &[
  "package.json",
  "package-lock.json",
  "yarn.lock",
  "pnpm-lock.yaml",
  "pnpm-workspace.yaml",
  "bun.lockb",
  "bun.lock",
  "tsconfig.json",
  "jsconfig.json",
  "next-env.d.ts",
  "vercel.json",
  "netlify.toml",
  "Dockerfile",
  "README.md",
  "LICENSE",
  "LICENSE.md",
];

const TOOLING_STEMS: &[&str] = &["next.config", "postcss.config", "tailwind.config", "eslint.config", "prettier.config", "jest.config", "vitest.config", "playwright.config", "cypress.config", "babel.config", "docker-compose", "instrumentation", "sentry.client.config", "sentry.server.config", "sentry.edge.config"];

/// A file at the source's root is carried over only when it is code or a
/// stylesheet; the rest there configures something other than the application.
const STYLESHEETS: &[&str] = &[".css", ".scss", ".sass"];

const TEST_DIRS: &[&str] = &["__tests__", "tests", "test", "e2e", "cypress", "playwright"];

/// Moves the App Router tree to `app/routes/`, `public/` to `app/public/`,
/// tests to `app/tests/` and every other source to `app/src/`, rewriting
/// imports to follow.
struct Layout;

impl Pass for Layout {
  fn name(&self) -> &'static str {
    "next-layout"
  }

  fn run(&self, tree: &mut Tree, report: &mut Report) {
    let root = if tree.paths().any(|p| p.starts_with("src/app/")) { "src/" } else { "" };
    let app = format!("{root}app/");
    read_packages(tree, report);
    let mut moves = BTreeMap::new();
    for path in tree.paths() {
      if let Some(rest) = path.strip_prefix(&app) {
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

pub(crate) fn read_packages(tree: &Tree, report: &mut Report) {
  let Some(package) = tree.text("package.json").and_then(|t| serde_json::from_str::<serde_json::Value>(t).ok()) else { return };
  let Some(deps) = package.get("dependencies").and_then(|d| d.as_object()) else { return };
  for (name, range) in deps {
    if ["next", "react", "react-dom"].contains(&name.as_str()) || name.starts_with("@types/") {
      continue;
    }
    report.packages.insert(name.clone(), range.as_str().unwrap_or_default().to_owned());
  }
}

/// Where `path`, at `rest` under the App Router directory, goes.
fn route_path(path: &str, rest: &str, report: &mut Report) -> Option<String> {
  let (dirs, file) = rest.rsplit_once('/').map(|(d, f)| (d, f)).unwrap_or(("", rest));
  let stem = file.split('.').next().unwrap_or(file);
  if file.contains(".test.") || file.contains(".spec.") {
    return Some(format!("app/tests/{rest}"));
  }
  if dirs.is_empty() && file == "favicon.ico" {
    return Some("app/icons/favicon.ico".to_owned());
  }
  match stem {
    "opengraph-image" | "twitter-image" if crate::syntax::is_script(file) => {}
    "icon" | "apple-icon" | "opengraph-image" | "twitter-image" | "sitemap" | "robots" | "manifest" => {
      report.note(path, 0, format!("Next's `{stem}` metadata file has no FSR counterpart yet; kept beside the route"));
    }
    "template" => report.note(path, 0, "Next's `template` has no FSR counterpart: a layout already keeps its DOM across navigations"),
    "global-error" => report.note(path, 0, "Next's `global-error` has no FSR counterpart: `routes/error.tsx` answers every route"),
    "default" if dirs.split('/').any(|d| d.starts_with('@')) => {
      report.note(path, 0, "a slot's `default` is the children of `<Slot name>` in FSR, the region's fallback");
    }
    _ => {}
  }
  let mut mapped = Vec::new();
  for dir in dirs.split('/').filter(|d| !d.is_empty()) {
    if let Some(slot) = dir.strip_prefix('@') {
      mapped.push("slots".to_owned());
      mapped.push(slot.to_owned());
    } else {
      if dir.starts_with("(.)") || dir.starts_with("(..)") || dir.starts_with("(...)") {
        report.note(path, 0, format!("`{dir}` intercepts a route; FSR intercepts with `page.<slot>.tsx` beside the target's page"));
      } else if dir.starts_with("[[...") {
        report.note(path, 0, format!("`{dir}` is an optional catch-all; FSR's `[...rest]` needs at least one segment"));
      }
      mapped.push(dir.to_owned());
    }
  }
  mapped.push(file.to_owned());
  Some(format!("app/routes/{}", mapped.join("/")))
}

/// Where `path`, outside the App Router directory and `public/`, goes:
/// `None` for Next's own configuration and tooling.
pub(crate) fn source_path(path: &str, root: &str, report: &mut Report) -> Option<String> {
  let file = path.rsplit('/').next().unwrap_or(path);
  let top_level = !path.contains('/');
  if top_level && (TOOLING.contains(&file) || TOOLING_STEMS.iter().any(|s| file.starts_with(&format!("{s}.")))) {
    if file.starts_with("tailwind.config.") {
      report.note(path, 0, "Tailwind's configuration is not carried over; FSR compiles Tailwind from the CSS it finds");
    }
    return None;
  }
  if path.split('/').any(|p| p.starts_with('.') || p == "node_modules") {
    if file.starts_with(".env") {
      report.note(path, 0, "environment files are not carried over; a value a page reads goes under `[public]` in `config/app.toml`");
    }
    return None;
  }
  let inner = path.strip_prefix(root).unwrap_or(path);
  if !inner.contains('/') && !crate::syntax::is_script(inner) && !inner.ends_with(".d.ts") && !STYLESHEETS.iter().any(|e| inner.ends_with(e)) {
    return None;
  }
  if inner.split('/').next().is_some_and(|d| TEST_DIRS.contains(&d)) && inner.contains('/') {
    return Some(format!("app/tests/{}", inner.split_once('/').map(|(_, r)| r).unwrap_or(inner)));
  }
  if inner == "middleware.ts" || inner == "middleware.js" {
    report.note(path, 0, "Next's middleware takes a `NextRequest`; FSR's `middleware.ts` takes the request context and returns what to do");
    return Some(format!("app/{inner}"));
  }
  Some(format!("app/src/{inner}"))
}

/// The root layout's `<html>` and `<body>`, which FSR's host writes itself:
/// `<html>` becomes a fragment and `<body>` a `<div>` keeping its attributes.
/// `<head>` is taken out with a note, since a route's head comes from its
/// loader's `meta`.
pub(crate) struct Document;

impl Pass for Document {
  fn name(&self) -> &'static str {
    "next-document"
  }

  fn run(&self, tree: &mut Tree, report: &mut Report) {
    let Some(path) = ["tsx", "jsx", "js", "ts"].iter().map(|e| format!("app/routes/layout.{e}")).find(|p| tree.contains(p)) else { return };
    let Some(text) = tree.text(&path).map(str::to_owned) else { return };
    let Ok(parsed) = parse(&path, &text) else { return };
    let mut find = Elements { found: Vec::new() };
    parsed.module.visit_with(&mut find);
    let mut edits = Edits::default();
    for element in &find.found {
      let js::JSXElementName::Ident(name) = &element.opening.name else { continue };
      let open = parsed.range(element.opening.span);
      let close = element.closing.as_ref().map(|c| parsed.range(c.span));
      match &*name.sym {
        "html" => {
          if let Some(lang) = attribute(&element.opening, "lang").filter(|l| l != "en") {
            report.note(&path, parsed.line(element.span), format!("`<html lang=\"{lang}\">`: FSR writes `lang` from the request's locale, `[locales]` in `config/app.toml`"));
          }
          edits.replace(open, "<>");
          if let Some(close) = close {
            edits.replace(close, "</>");
          }
        }
        "body" => {
          let tag = &text[parsed.range(element.opening.span)];
          edits.replace(open.start..open.start + "<body".len(), "<div");
          if tag.ends_with("/>") {
            continue;
          }
          if let Some(close) = close {
            edits.replace(close, "</div>");
          }
        }
        "head" => {
          report.note(&path, parsed.line(element.span), "the root layout's `<head>` was taken out; a route's head comes from its loader's `meta`");
          edits.replace(parsed.range(element.span), "");
        }
        _ => {}
      }
    }
    if !edits.is_empty() {
      tree.insert(path, edits.apply(&text));
    }
  }
}

/// A page's or a layout's literal `metadata` export, moved to the loader
/// beside it as `meta`: `title` and `description` carried over, a title's
/// `default` taken as the title. Every other field is a note.
struct Metadata;

impl Pass for Metadata {
  fn name(&self) -> &'static str {
    "next-metadata"
  }

  fn run(&self, tree: &mut Tree, report: &mut Report) {
    let modules: Vec<String> = tree
      .paths()
      .filter(|p| p.starts_with("app/routes/"))
      .filter(|p| {
        let file = p.rsplit('/').next().unwrap_or(p);
        ["page.", "layout."].iter().any(|s| file.starts_with(s)) && file.matches('.').count() == 1
      })
      .map(str::to_owned)
      .collect();
    for path in modules {
      let Some(text) = tree.text(&path).map(str::to_owned) else { continue };
      let Ok(parsed) = parse(&path, &text) else { continue };
      for item in &parsed.module.body {
        let js::ModuleItem::ModuleDecl(js::ModuleDecl::ExportDecl(js::ExportDecl { decl: js::Decl::Var(var), .. })) = item else { continue };
        let Some(decl) = var.decls.first() else { continue };
        let js::Pat::Ident(name) = &decl.name else { continue };
        if &*name.id.sym == "generateMetadata" {
          report.note(&path, parsed.line(item.span()), "`generateMetadata` was left in place: the loader's `meta` is a function of what `load` returned");
          continue;
        }
        if &*name.id.sym != "metadata" {
          continue;
        }
        let line = parsed.line(item.span());
        let Some(js::Expr::Object(object)) = decl.init.as_deref().map(strip) else {
          report.note(&path, line, "`metadata` is not an object literal, so it was left in place");
          continue;
        };
        let mut fields = Vec::new();
        for prop in &object.props {
          let js::PropOrSpread::Prop(prop) = prop else {
            report.note(&path, line, "a spread in `metadata` was not carried over");
            continue;
          };
          let js::Prop::KeyValue(kv) = &**prop else { continue };
          let key = match &kv.key {
            js::PropName::Ident(i) => i.sym.to_string(),
            js::PropName::Str(s) => s.value.to_string_lossy().into_owned(),
            _ => continue,
          };
          match (key.as_str(), strip(&kv.value)) {
            ("title" | "description", value) if literal(value) => fields.push(format!("{key}: {}", &text[parsed.range(value.span())])),
            ("title", js::Expr::Object(title)) => {
              for part in &title.props {
                let js::PropOrSpread::Prop(part) = part else { continue };
                let js::Prop::KeyValue(kv) = &**part else { continue };
                let js::PropName::Ident(k) = &kv.key else { continue };
                match &*k.sym {
                  "default" if literal(&kv.value) => fields.push(format!("title: {}", &text[parsed.range(kv.value.span())])),
                  "template" => report.note(&path, line, format!("the title template {} goes in `config/app.toml` as `[document] title_template`, `%s` written `{{page}}`", &text[parsed.range(kv.value.span())])),
                  "absolute" => report.note(&path, line, "an absolute title is `absolute: true` beside the title in `meta`"),
                  _ => {}
                }
              }
            }
            _ => report.note(&path, line, format!("`metadata.{key}` was not carried over; `meta` writes other head elements through `head` and the helpers in `@snapfire/fsr/head`")),
          }
        }
        let (dir, file) = path.rsplit_once('/').unwrap_or(("", &path));
        let kind = if file.starts_with("page.") { "page" } else { "layout" };
        let loader = format!("{dir}/{kind}.loader.ts");
        let meta = format!("export const meta = () => ({{ {} }});\n", fields.join(", "));
        let loader_text = match tree.text(&loader) {
          Some(existing) if existing.contains("export const meta") => {
            report.note(&path, line, "the loader already exports `meta`, so `metadata` was left in place");
            continue;
          }
          Some(existing) => format!("{}\n{meta}", existing.trim_end_matches('\n').to_owned() + "\n"),
          None => meta,
        };
        let mut edits = Edits::default();
        edits.replace(line_range(&text, parsed.range(item.span())), "");
        let mut page = edits.apply(&text);
        if let Some(stripped) = drop_named_import(&page, "next", "Metadata") {
          page = stripped;
        }
        tree.insert(loader, loader_text);
        tree.insert(path.clone(), crate::items::tidy(&page));
        break;
      }
    }
  }
}

fn strip(e: &js::Expr) -> &js::Expr {
  match e {
    js::Expr::Paren(p) => strip(&p.expr),
    js::Expr::TsAs(a) => strip(&a.expr),
    js::Expr::TsSatisfies(s) => strip(&s.expr),
    js::Expr::TsConstAssertion(c) => strip(&c.expr),
    other => other,
  }
}

/// A string written out: a literal or a template with no holes.
fn literal(e: &js::Expr) -> bool {
  match strip(e) {
    js::Expr::Lit(js::Lit::Str(_)) => true,
    js::Expr::Tpl(t) => t.exprs.is_empty(),
    _ => false,
  }
}

fn attribute(opening: &js::JSXOpeningElement, name: &str) -> Option<String> {
  opening.attrs.iter().find_map(|a| match a {
    js::JSXAttrOrSpread::JSXAttr(js::JSXAttr { name: js::JSXAttrName::Ident(n), value: Some(js::JSXAttrValue::Str(s)), .. }) if &*n.sym == name => {
      Some(s.value.to_string_lossy().into_owned())
    }
    _ => None,
  })
}

struct Elements {
  found: Vec<js::JSXElement>,
}

impl Visit for Elements {
  fn visit_jsx_element(&mut self, n: &js::JSXElement) {
    if let js::JSXElementName::Ident(name) = &n.opening.name {
      if ["html", "body", "head"].contains(&&*name.sym) {
        self.found.push(n.clone());
      }
    }
    n.visit_children_with(self);
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

  #[test]
  fn an_app_router_tree_moves_under_routes_and_its_imports_follow() {
    let input = tree(&[
      ("package.json", r#"{ "dependencies": { "next": "latest", "react": "19", "clsx": "^2" } }"#),
      ("next.config.js", "module.exports = {};\n"),
      ("app/page.tsx", "import Nav from \"../components/nav\";\nexport default function Page() { return <Nav />; }\n"),
      ("app/@modal/page.tsx", "export default function M() { return null; }\n"),
      ("app/blog/[slug]/page.tsx", "export default function P() { return null; }\n"),
      ("components/nav.tsx", "export default function Nav() { return <nav />; }\n"),
      ("public/logo.svg", "<svg/>"),
      ("app/blog/[slug]/page.test.tsx", "test(\"renders\", () => {});\n"),
    ]);
    let (out, report) = migrate(&input);
    assert_eq!(report.source, Some(Source::NextApp));
    let paths: Vec<&str> = out.paths().collect();
    assert_eq!(paths, ["app/public/logo.svg", "app/routes/blog/[slug]/page.tsx", "app/routes/page.tsx", "app/routes/slots/modal/page.tsx", "app/src/components/nav.tsx", "app/tests/blog/[slug]/page.test.tsx"]);
    assert!(out.text("app/routes/page.tsx").unwrap().starts_with("import Nav from \"../src/components/nav\";"));
    assert_eq!(report.packages.keys().collect::<Vec<_>>(), ["clsx"]);
  }

  #[test]
  fn a_src_app_tree_moves_the_same_way() {
    let input = tree(&[
      ("package.json", r#"{ "dependencies": { "next": "latest" } }"#),
      ("src/app/page.tsx", "import { x } from \"@/lib/x\";\n"),
      ("src/lib/x.ts", "export const x = 1;\n"),
      ("tsconfig.json", "{ \"compilerOptions\": { \"paths\": { \"@/*\": [\"./src/*\"] } } }"),
    ]);
    let (out, _) = migrate(&input);
    assert_eq!(out.text("app/routes/page.tsx").unwrap(), "import { x } from \"@src/lib/x\";\n");
    assert!(out.contains("app/src/lib/x.ts"));
  }

  #[test]
  fn a_literal_metadata_export_becomes_the_loaders_meta() {
    let input = tree(&[
      ("package.json", r#"{ "dependencies": { "next": "latest" } }"#),
      (
        "app/about/page.tsx",
        "import type { Metadata } from \"next\";\n\nexport const metadata: Metadata = {\n  title: { default: \"About\", template: \"%s | Acme\" },\n  description: \"Who we are\",\n  openGraph: { title: \"About\" },\n};\n\nexport default function About() {\n  return <p>about</p>;\n}\n",
      ),
    ]);
    let (out, report) = migrate(&input);
    assert_eq!(out.text("app/routes/about/page.loader.ts").unwrap(), "export const meta = () => ({ title: \"About\", description: \"Who we are\" });\n");
    assert_eq!(out.text("app/routes/about/page.tsx").unwrap(), "export default function About() {\n  return <p>about</p>;\n}\n");
    let notes: Vec<String> = report.notes.iter().map(|n| n.message.clone()).collect();
    assert_eq!(notes.len(), 2, "{notes:?}");
  }

  #[test]
  fn the_root_layout_loses_html_and_head_and_its_body_becomes_a_div() {
    let input = tree(&[
      ("package.json", r#"{ "dependencies": { "next": "latest" } }"#),
      (
        "app/layout.tsx",
        "export default function RootLayout({ children }) {\n  return (\n    <html lang=\"fr\">\n      <head><link rel=\"icon\" href=\"/x.png\" /></head>\n      <body className={inter.className}>{children}</body>\n    </html>\n  );\n}\n",
      ),
    ]);
    let (out, report) = migrate(&input);
    assert_eq!(
      out.text("app/routes/layout.tsx").unwrap(),
      "export default function RootLayout({ children }) {\n  return (\n    <>\n      \n      <div className={inter.className}>{children}</div>\n    </>\n  );\n}\n"
    );
    let notes: Vec<String> = report.notes.iter().map(|n| n.to_string()).collect();
    assert_eq!(notes.len(), 2, "{notes:?}");
  }
}
