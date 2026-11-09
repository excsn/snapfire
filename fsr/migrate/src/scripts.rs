//! `next/script` and `@next/third-parties/google`. A `<Script src>` in a
//! page or layout becomes a row of its route's `meta` head; an embed becomes
//! the iframe it renders.

use std::collections::BTreeMap;

use oxc_allocator::Allocator;
use oxc_ast::ast::{self as js, ArrayExpressionElement, BindingPattern, Declaration, Expression, ImportDeclarationSpecifier, JSXAttributeItem, JSXAttributeName, JSXAttributeValue, JSXChild, ModuleExportName, Statement};
use oxc_ast_visit::{walk, Visit};
use oxc_span::{GetSpan, Span};

use crate::edit::Edits;
use crate::items::{drop_named_import, line_range, tidy};
use crate::lift::route_kind;
use crate::relocate::dirname;
use crate::syntax::{is_script, jsx_ident, key_value, lossy, parse, Parsed};
use crate::{Pass, Report, Tree};

pub(crate) struct Scripts;

impl Pass for Scripts {
  fn name(&self) -> &'static str {
    "next-script"
  }

  fn run(&self, tree: &mut Tree, report: &mut Report) {
    let paths: Vec<String> = tree.paths().filter(|p| p.starts_with("app/") && is_script(p)).map(str::to_owned).collect();
    for path in paths {
      let Some(text) = tree.text(&path).map(str::to_owned) else { continue };
      if !text.contains("next/script") && !text.contains("@next/third-parties/google") {
        continue;
      }
      let allocator = Allocator::default();
      let Ok(parsed) = parse(&allocator, &path, &text) else { continue };
      convert(tree, &path, &text, &parsed, report);
    }
  }
}

/// The names `parsed` imports `Script` and the Google embeds under, local to
/// imported.
fn imported(parsed: &Parsed) -> BTreeMap<String, String> {
  let mut out = BTreeMap::new();
  for item in parsed.body() {
    let Statement::ImportDeclaration(import) = item else { continue };
    match lossy(import.source.value).as_str() {
      "next/script" => {
        for s in import.specifiers.iter().flatten() {
          if let ImportDeclarationSpecifier::ImportDefaultSpecifier(d) = s {
            out.insert(d.local.name.to_string(), "Script".to_owned());
          }
        }
      }
      "@next/third-parties/google" => {
        for s in import.specifiers.iter().flatten() {
          if let ImportDeclarationSpecifier::ImportSpecifier(n) = s {
            let name = match &n.imported {
              ModuleExportName::IdentifierName(i) => i.name.to_string(),
              _ => n.local.name.to_string(),
            };
            if name == "GoogleMapsEmbed" || name == "YouTubeEmbed" {
              out.insert(n.local.name.to_string(), name);
            }
          }
        }
      }
      _ => {}
    }
  }
  out
}

/// An element that names an imported `Script` or embed, read as the conversion needs it.
struct Found {
  local: String,
  span: Span,
  attrs: Vec<(String, Value, String)>,
  /// Whether it holds anything but whitespace between its tags.
  holds_code: bool,
}

struct Elements<'a> {
  names: &'a BTreeMap<String, String>,
  text: &'a str,
  parsed: &'a Parsed<'a>,
  found: Vec<Found>,
  /// Spans of elements that are a function's whole result.
  returned: Vec<Span>,
}

impl<'a> Visit<'a> for Elements<'_> {
  fn visit_jsx_element(&mut self, n: &js::JSXElement<'a>) {
    if let Some((name, _)) = jsx_ident(&n.opening_element.name) {
      if self.names.contains_key(name) {
        let holds_code = n.children.iter().any(|c| match c {
          JSXChild::Text(t) => !t.value.trim().is_empty(),
          _ => true,
        });
        self.found.push(Found { local: name.to_owned(), span: n.span, attrs: attrs(self.text, self.parsed, &n.opening_element), holds_code });
        return;
      }
    }
    walk::walk_jsx_element(self, n);
  }

  fn visit_return_statement(&mut self, n: &js::ReturnStatement<'a>) {
    if let Some(Expression::JSXElement(e)) = n.argument.as_ref().map(unparen) {
      self.returned.push(e.span);
    }
    walk::walk_return_statement(self, n);
  }
}

fn unparen<'e, 'a>(e: &'e Expression<'a>) -> &'e Expression<'a> {
  match e {
    Expression::ParenthesizedExpression(p) => unparen(&p.expression),
    other => other,
  }
}

enum Value {
  Str(String),
  Expr(String),
  True,
}

/// Each attribute by name: its value and the text of the whole attribute.
fn attrs(text: &str, parsed: &Parsed, opening: &js::JSXOpeningElement) -> Vec<(String, Value, String)> {
  opening
    .attributes
    .iter()
    .filter_map(|a| match a {
      JSXAttributeItem::Attribute(attr) => {
        let name = match &attr.name {
          JSXAttributeName::Identifier(i) => i.name.to_string(),
          JSXAttributeName::NamespacedName(n) => format!("{}:{}", n.namespace.name, n.name.name),
        };
        let value = match &attr.value {
          None => Value::True,
          Some(JSXAttributeValue::StringLiteral(s)) => Value::Str(lossy(s.value)),
          Some(JSXAttributeValue::ExpressionContainer(c)) => match c.expression.as_expression().map(unparen) {
            Some(Expression::StringLiteral(s)) => Value::Str(lossy(s.value)),
            Some(Expression::BooleanLiteral(b)) if b.value => Value::True,
            Some(other) => Value::Expr(text[parsed.range(other.span())].to_owned()),
            None => Value::Expr(text[parsed.range(c.span)].to_owned()),
          },
          Some(other) => Value::Expr(text[parsed.range(other.span())].to_owned()),
        };
        Some((name, value, text[parsed.range(attr.span)].to_owned()))
      }
      JSXAttributeItem::SpreadAttribute(_) => None,
    })
    .collect()
}

fn convert(tree: &mut Tree, path: &str, text: &str, parsed: &Parsed, report: &mut Report) {
  let names = imported(parsed);
  if names.is_empty() {
    return;
  }
  let mut found = Elements { names: &names, text, parsed, found: Vec::new(), returned: Vec::new() };
  found.visit_program(&parsed.program);
  let route = route_kind(path);
  let mut edits = Edits::default();
  let mut rows: Vec<String> = Vec::new();
  let mut left: Vec<String> = Vec::new();
  for element in &found.found {
    let local = element.local.clone();
    let line = parsed.line(element.span);
    let attrs = &element.attrs;
    let replaced = match names[&local].as_str() {
      "Script" => match script_row(element.holds_code, attrs) {
        Err(why) => {
          report.note(path, line, format!("`<{local}>` {why}, so it was left for you"));
          None
        }
        Ok(_) if route.is_none() => {
          report.note(path, line, format!("`<{local}>` sits in a component below a route; a script row belongs in its route's `meta`, so it was left for you"));
          None
        }
        Ok(row) => {
          rows.push(row);
          Some(String::new())
        }
      },
      "GoogleMapsEmbed" => Some(maps_iframe(attrs)),
      _ => Some(youtube_iframe(attrs)),
    };
    match replaced {
      Some(with) if with.is_empty() => {
        if found.returned.contains(&element.span) {
          edits.replace(parsed.range(element.span), "null");
        } else {
          edits.replace(line_range(text, parsed.range(element.span)), "");
        }
      }
      Some(with) => edits.replace(parsed.range(element.span), with),
      None => left.push(local),
    }
  }
  if edits.is_empty() {
    return;
  }
  if let (false, Some(kind)) = (rows.is_empty(), route) {
    let loader_path = format!("{}/{kind}.loader.ts", dirname(path));
    match with_head(tree.text(&loader_path).unwrap_or_default(), &loader_path, &rows) {
      Some(loader) => tree.insert(loader_path, tidy(&loader)),
      None => {
        report.note(path, 0, "the route's `meta` is not an object literal the scripts could join, so they were left for you");
        return;
      }
    }
  }
  let mut page = edits.apply(text);
  for (local, name) in &names {
    if left.contains(local) {
      continue;
    }
    let module = if name == "Script" { "next/script" } else { "@next/third-parties/google" };
    if let Some(stripped) = drop_named_import(&page, module, local) {
      page = stripped;
    }
  }
  tree.insert(path.to_owned(), tidy(&page));
}

/// The `meta` head row a `<Script src>` is; the reason it is none otherwise.
fn script_row(code: bool, attrs: &[(String, Value, String)]) -> Result<String, String> {
  if code || attrs.iter().any(|(n, _, _)| n == "dangerouslySetInnerHTML") {
    return Err("runs inline code, which FSR keeps in a module `src/main.ts` imports".to_owned());
  }
  if let Some((handler, _, _)) = attrs.iter().find(|(n, _, _)| ["onLoad", "onReady", "onError"].contains(&n.as_str())) {
    return Err(format!("runs code from `{handler}`, which belongs in a module that calls `loadScript` and awaits it"));
  }
  let src = match attrs.iter().find(|(n, _, _)| n == "src").map(|(_, v, _)| v) {
    Some(Value::Str(src)) => src.clone(),
    Some(_) => return Err("computes its `src`, which a head row cannot".to_owned()),
    None => return Err("has no `src`".to_owned()),
  };
  let when = match attrs.iter().find(|(n, _, _)| n == "strategy").map(|(_, v, _)| v) {
    Some(Value::Str(s)) if s == "lazyOnload" || s == "worker" => Some("idle"),
    _ => None,
  };
  let mut extra: Vec<String> = Vec::new();
  for (name, value, _) in attrs {
    let key = match name.as_str() {
      "crossOrigin" => "crossorigin".to_owned(),
      "referrerPolicy" => "referrerpolicy".to_owned(),
      "integrity" | "type" => name.clone(),
      other if other.starts_with("data-") => format!("{other:?}"),
      _ => continue,
    };
    let value = match value {
      Value::Str(s) => format!("{s:?}"),
      Value::Expr(e) => e.clone(),
      Value::True => "true".to_owned(),
    };
    extra.push(format!("{key}: {value}"));
  }
  Ok(match (extra.is_empty(), when) {
    (true, None) => format!("scriptTag({src:?})"),
    (true, Some(when)) => format!("scriptTag({src:?}, {when:?})"),
    (false, when) => {
      let when = when.map(|w| format!(", when: {w:?}")).unwrap_or_default();
      format!("{{ tag: \"script\", src: {src:?}{when}, {} }}", extra.join(", "))
    }
  })
}

/// `src` from literal text and expressions: a string attribute when every
/// part is literal, else a template literal.
fn src_attr(parts: &[(&str, &Value)]) -> String {
  if parts.iter().all(|(_, v)| matches!(v, Value::Str(_))) {
    let joined: String = parts.iter().map(|(lit, v)| match v { Value::Str(s) => format!("{lit}{s}"), _ => String::new() }).collect();
    return format!("src={joined:?}");
  }
  let joined: String = parts.iter().map(|(lit, v)| match v {
    Value::Str(s) => format!("{lit}{s}"),
    Value::Expr(e) => format!("{lit}${{{e}}}"),
    Value::True => lit.to_string(),
  }).collect();
  format!("src={{`{joined}`}}")
}

fn carried(attrs: &[(String, Value, String)], names: &[&str]) -> Vec<String> {
  attrs.iter().filter(|(n, _, _)| names.contains(&n.as_str())).map(|(_, _, t)| t.clone()).collect()
}

fn maps_iframe(attrs: &[(String, Value, String)]) -> String {
  let get = |n: &str| attrs.iter().find(|(name, _, _)| name == n).map(|(_, v, _)| v);
  let place = Value::Str("place".to_owned());
  let mut parts: Vec<(&str, &Value)> = vec![("https://www.google.com/maps/embed/v1/", get("mode").unwrap_or(&place))];
  let empty = Value::Str(String::new());
  parts.push(("?key=", get("apiKey").unwrap_or(&empty)));
  for (param, name) in [("&q=", "q"), ("&center=", "center"), ("&zoom=", "zoom"), ("&maptype=", "maptype"), ("&language=", "language"), ("&region=", "region")] {
    if let Some(value) = get(name) {
      parts.push((param, value));
    }
  }
  let mut out = vec![src_attr(&parts)];
  out.extend(carried(attrs, &["height", "width"]));
  out.push(carried(attrs, &["style"]).pop().unwrap_or_else(|| "style={{ border: 0 }}".to_owned()));
  out.push(carried(attrs, &["loading"]).pop().unwrap_or_else(|| "loading=\"lazy\"".to_owned()));
  out.push("allowFullScreen".to_owned());
  out.push("referrerPolicy=\"no-referrer-when-downgrade\"".to_owned());
  format!("<iframe {} />", out.join(" "))
}

fn youtube_iframe(attrs: &[(String, Value, String)]) -> String {
  let get = |n: &str| attrs.iter().find(|(name, _, _)| name == n).map(|(_, v, _)| v);
  let empty = Value::Str(String::new());
  let mut parts: Vec<(&str, &Value)> = vec![("https://www.youtube-nocookie.com/embed/", get("videoid").unwrap_or(&empty))];
  if let Some(params) = get("params") {
    parts.push(("?", params));
  }
  let mut out = vec![src_attr(&parts)];
  out.extend(carried(attrs, &["height", "width", "style"]));
  out.push(match get("playlabel") {
    Some(Value::Str(label)) => format!("title={label:?}"),
    Some(Value::Expr(e)) => format!("title={{{e}}}"),
    _ => "title=\"YouTube video\"".to_owned(),
  });
  out.push("allow=\"accelerometer; autoplay; clipboard-write; encrypted-media; gyroscope; picture-in-picture\"".to_owned());
  out.push("allowFullScreen".to_owned());
  format!("<iframe {} />", out.join(" "))
}

/// `loader` with `rows` joined to its `meta` head, a `meta` written when it
/// has none; `None` when its `meta` is not an object literal.
pub(crate) fn with_head(loader: &str, path: &str, rows: &[String]) -> Option<String> {
  let allocator = Allocator::default();
  let parsed = parse(&allocator, path, loader).ok()?;
  let joined = rows.join(", ");
  let mut edits = Edits::default();
  let mut has_meta = false;
  for item in parsed.body() {
    let Statement::ExportDeclaration(export) = item else { continue };
    let Declaration::VariableDeclaration(var) = &export.declaration else { continue };
    for decl in &var.declarations {
      let (BindingPattern::BindingIdentifier(name), Some(init)) = (&decl.id, &decl.init) else { continue };
      if name.name != "meta" {
        continue;
      }
      has_meta = true;
      let Expression::ArrowFunctionExpression(arrow) = unparen(init) else { return None };
      let Some(body) = arrow.body.as_expression() else { return None };
      let Expression::ObjectExpression(object) = unparen(body) else { return None };
      let head = object.properties.iter().find_map(|p| key_value(p).filter(|(key, _)| *key == "head").map(|(_, value)| value));
      match head.map(|h| unparen(h)) {
        Some(Expression::ArrayExpression(array)) => match array.elements.iter().filter(|e| !matches!(e, ArrayExpressionElement::Elision(_))).last() {
          Some(last) => {
            let end = parsed.range(last.span()).end;
            edits.replace(end..end, format!(", {joined}"));
          }
          None => edits.replace(parsed.range(array.span), format!("[{joined}]")),
        },
        Some(_) => return None,
        None => match object.properties.last() {
          Some(last) => {
            let end = parsed.range(last.span()).end;
            edits.replace(end..end, format!(", head: [{joined}]"));
          }
          None => edits.replace(parsed.range(object.span), format!("{{ head: [{joined}] }}")),
        },
      }
    }
  }
  let mut out = edits.apply(loader);
  if !has_meta {
    let sep = if out.is_empty() || out.ends_with("\n\n") { "" } else if out.ends_with('\n') { "\n" } else { "\n\n" };
    out.push_str(&format!("{sep}export const meta = () => ({{ head: [{joined}] }});\n"));
  }
  for helper in ["scriptTag", "linkTag"] {
    if !rows.iter().any(|r| r.starts_with(&format!("{helper}("))) || out.lines().any(|l| l.starts_with("import") && l.contains(helper)) {
      continue;
    }
    match out.lines().position(|l| l.starts_with("import {") && l.ends_with("from \"@snapfire/fsr/head\";")) {
      Some(at) => {
        let lines: Vec<String> = out.lines().enumerate().map(|(i, l)| if i == at { l.replacen("import { ", &format!("import {{ {helper}, "), 1) } else { l.to_owned() }).collect();
        out = lines.join("\n") + "\n";
      }
      None => out.insert_str(0, &format!("import {{ {helper} }} from \"@snapfire/fsr/head\";\n")),
    }
  }
  Some(out)
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

  const NEXT: &str = r#"{ "dependencies": { "next": "latest" } }"#;

  #[test]
  fn a_script_tag_in_a_layout_becomes_a_head_row_beside_the_meta_it_has() {
    let layout = "import Script from \"next/script\";\n\nexport const metadata = { title: \"Clerk\" };\n\nexport default function RootLayout({ children }: { children: React.ReactNode }) {\n  return (\n    <html lang=\"en\">\n      <body>{children}</body>\n      <Script src=\"https://cdn.example/prism-core.min.js\" />\n      <Script src=\"https://cdn.example/stats.js\" strategy=\"lazyOnload\" crossOrigin=\"anonymous\" />\n    </html>\n  );\n}\n";
    let input = tree(&[("package.json", NEXT), ("app/layout.tsx", layout), ("app/page.tsx", "export default function Page() {\n  return <main />;\n}\n")]);
    let (out, report) = migrate(&input);
    let layout = out.text("app/routes/layout.tsx").unwrap();
    assert!(!layout.contains("Script") && !layout.contains("next/script"), "{layout}");
    let loader = out.text("app/routes/layout.loader.ts").unwrap();
    assert!(loader.contains("import { scriptTag } from \"@snapfire/fsr/head\";"), "{loader}");
    assert!(loader.contains("head: [scriptTag(\"https://cdn.example/prism-core.min.js\"), { tag: \"script\", src: \"https://cdn.example/stats.js\", when: \"idle\", crossorigin: \"anonymous\" }]"), "{loader}");
    assert!(loader.contains("title: \"Clerk\""), "{loader}");
    assert!(report.notes.iter().all(|n| !n.message.contains("Script")), "{:?}", report.notes);
  }

  #[test]
  fn a_page_whose_only_markup_is_a_script_renders_nothing_and_gets_a_meta() {
    let page = "import Script from \"next/script\";\n\nexport default function Page() {\n  return <Script src=\"https://cdn.example/a.js\" strategy=\"afterInteractive\" nonce=\"x\" />;\n}\n";
    let input = tree(&[("package.json", NEXT), ("app/page.tsx", page)]);
    let (out, _) = migrate(&input);
    let page = out.text("app/routes/page.tsx").unwrap();
    assert!(page.contains("return null;"), "{page}");
    let loader = out.text("app/routes/page.loader.ts").unwrap();
    assert!(loader.contains("export const meta = () => ({ head: [scriptTag(\"https://cdn.example/a.js\")] });"), "{loader}");
  }

  #[test]
  fn inline_script_code_and_a_load_handler_are_notes() {
    let page = "import Script from \"next/script\";\n\nexport default function Page() {\n  return (\n    <main>\n      <Script id=\"cb\">{`window.go = () => 1`}</Script>\n      <Script src=\"https://cdn.example/w.js\" onLoad={() => go()} />\n    </main>\n  );\n}\n";
    let input = tree(&[("package.json", NEXT), ("app/page.tsx", page)]);
    let (out, report) = migrate(&input);
    assert!(out.text("app/routes/page.tsx").unwrap().contains("<Script id=\"cb\">"));
    let notes: Vec<String> = report.notes.iter().map(|n| n.to_string()).collect();
    assert!(notes.iter().any(|n| n.contains("page.tsx:6") && n.contains("inline code")), "{notes:?}");
    assert!(notes.iter().any(|n| n.contains("page.tsx:7") && n.contains("`onLoad`")), "{notes:?}");
  }

  #[test]
  fn a_google_embed_becomes_the_iframe_it_renders() {
    let page = "import { GoogleMapsEmbed, YouTubeEmbed } from \"@next/third-parties/google\";\n\nexport default function Page({ apiKey }: { apiKey: string }) {\n  return (\n    <div>\n      <GoogleMapsEmbed apiKey={apiKey} height={400} width={700} mode=\"place\" q=\"Brooklyn+Bridge,New+York,NY\" />\n      <YouTubeEmbed videoid=\"ogfYd705cRs\" height={400} params=\"controls=0\" />\n    </div>\n  );\n}\n";
    let input = tree(&[("package.json", NEXT), ("app/page.tsx", page)]);
    let (out, _) = migrate(&input);
    let page = out.text("app/routes/page.tsx").unwrap();
    assert!(page.contains("<iframe src={`https://www.google.com/maps/embed/v1/place?key=${apiKey}&q=Brooklyn+Bridge,New+York,NY`} height={400} width={700} style={{ border: 0 }} loading=\"lazy\" allowFullScreen referrerPolicy=\"no-referrer-when-downgrade\" />"), "{page}");
    assert!(page.contains("<iframe src=\"https://www.youtube-nocookie.com/embed/ogfYd705cRs?controls=0\" height={400} title=\"YouTube video\" allow=\"accelerometer; autoplay; clipboard-write; encrypted-media; gyroscope; picture-in-picture\" allowFullScreen />"), "{page}");
    assert!(!page.contains("@next/third-parties"), "{page}");
  }
}
