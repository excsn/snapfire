//! FSR's own lint rules and the levels FSR sets under a project's, for `snapfire_lint`.

use std::path::Path;

use snapfire_lint::oxc_ast::{
  AstKind,
  ast::{Expression, ImportDeclarationSpecifier, Statement},
};
use snapfire_lint::oxc_semantic::{AstNode, Semantic, SymbolId};
use snapfire_lint::oxc_span::GetSpan;
use snapfire_lint::{Check, Config, File, Level, RuleInfo};

/// The checks only FSR can make.
pub fn checks() -> Vec<Box<dyn Check>> {
  vec![Box::new(BrowserGlobals), Box::new(FileRead), Box::new(OutputClash)]
}

/// The attributes of htmx 2 and htmx 4, which a JSX element may carry when htmx drives it.
const HTMX_ATTRIBUTES: &[&str] = &[
  "hx-action",
  "hx-boost",
  "hx-config",
  "hx-confirm",
  "hx-delete",
  "hx-disable",
  "hx-disabled-elt",
  "hx-disinherit",
  "hx-encoding",
  "hx-ext",
  "hx-get",
  "hx-headers",
  "hx-history",
  "hx-history-elt",
  "hx-ignore",
  "hx-include",
  "hx-indicator",
  "hx-inherit",
  "hx-method",
  "hx-morph-skip",
  "hx-morph-skip-children",
  "hx-on",
  "hx-params",
  "hx-patch",
  "hx-post",
  "hx-preserve",
  "hx-prompt",
  "hx-push-url",
  "hx-put",
  "hx-query",
  "hx-replace-url",
  "hx-request",
  "hx-select",
  "hx-select-oob",
  "hx-status",
  "hx-swap",
  "hx-swap-oob",
  "hx-sync",
  "hx-target",
  "hx-trigger",
  "hx-validate",
  "hx-vals",
  "hx-vars",
];

/// What FSR sets under a project's own `fsr.lint.toml`, as a framework's shared ESLint config does.
pub fn base() -> Config {
  let ignore = HTMX_ATTRIBUTES.iter().map(|a| format!("\"{a}\"")).collect::<Vec<_>>().join(", ");
  Config::parse(&format!("[rules]\n\"react/no-unknown-property\" = [\"error\", {{ ignore = [{ignore}] }}]\n"), Path::new("fsr")).expect("FSR's base configuration parses")
}

/// Globals that exist only in a browser.
const BROWSER_GLOBALS: &[&str] = &[
  "alert",
  "cancelAnimationFrame",
  "confirm",
  "document",
  "getComputedStyle",
  "history",
  "innerHeight",
  "innerWidth",
  "IntersectionObserver",
  "localStorage",
  "location",
  "matchMedia",
  "MutationObserver",
  "navigator",
  "prompt",
  "requestAnimationFrame",
  "ResizeObserver",
  "screen",
  "sessionStorage",
  "window",
];

/// Hooks whose callback runs while the component renders.
const RENDER_HOOKS: &[&str] = &["useMemo", "useState", "useReducer"];

/// Whether `path` is a module only the server runs, by FSR's file names.
fn server_module(path: &Path) -> bool {
  let name = path.file_name().and_then(|n| n.to_str()).unwrap_or_default();
  name.contains(".loader.") || name.starts_with("actions.") || name.starts_with("route.") || name.starts_with("middleware.")
}

fn component_file(path: &Path) -> bool {
  matches!(path.extension().and_then(|e| e.to_str()), Some("tsx" | "jsx"))
}

/// Where code sits relative to a component's render.
enum Place {
  /// Runs when the component renders, or when its module loads.
  Render,
  /// Inside a function the render does not call: a handler, an effect, a helper.
  Later,
}

fn is_component_name(name: &str) -> bool {
  name.starts_with(|c: char| c.is_ascii_uppercase())
}

/// Whether the function `node` is a component: named with a capital, or a module's default export.
fn is_component<'a>(node: &AstNode<'a>, semantic: &Semantic<'a>) -> bool {
  let nodes = semantic.nodes();
  match node.kind() {
    AstKind::Function(function) => {
      if let Some(id) = &function.id {
        return is_component_name(&id.name);
      }
      matches!(nodes.parent_kind(node.id()), AstKind::ExportDefaultDeclaration(_))
    }
    AstKind::ArrowFunctionExpression(_) => match nodes.parent_kind(node.id()) {
      AstKind::VariableDeclarator(declarator) => declarator.id.get_identifier_name().is_some_and(|name| is_component_name(&name)),
      AstKind::ExportDefaultDeclaration(_) => true,
      _ => false,
    },
    _ => false,
  }
}

/// Whether the function `node` is the callback of a hook that runs it while rendering.
fn render_hook_callback<'a>(node: &AstNode<'a>, semantic: &Semantic<'a>) -> bool {
  let AstKind::CallExpression(call) = semantic.nodes().parent_kind(node.id()) else { return false };
  matches!(&call.callee, Expression::Identifier(ident) if RENDER_HOOKS.contains(&ident.name.as_str()))
}

fn place<'a>(node: &AstNode<'a>, semantic: &Semantic<'a>) -> Place {
  for ancestor in semantic.nodes().ancestors(node.id()) {
    if matches!(ancestor.kind(), AstKind::Function(_) | AstKind::ArrowFunctionExpression(_)) {
      if render_hook_callback(ancestor, semantic) {
        continue;
      }
      return match is_component(ancestor, semantic) {
        true => Place::Render,
        false => Place::Later,
      };
    }
  }
  Place::Render
}

/// Whether `node` sits where `typeof <global>` has been tested: a guard that runs it only in a browser.
fn guarded<'a>(node: &AstNode<'a>, semantic: &Semantic<'a>, source: &str) -> bool {
  semantic.nodes().ancestors(node.id()).any(|ancestor| {
    let test = match ancestor.kind() {
      AstKind::IfStatement(s) => Some(s.test.span()),
      AstKind::ConditionalExpression(e) => Some(e.test.span()),
      AstKind::LogicalExpression(e) => Some(e.left.span()),
      _ => None,
    };
    test.is_some_and(|span| source.get(span.start as usize..span.end as usize).is_some_and(|t| t.contains("typeof ")))
  })
}


struct BrowserGlobals;

impl Check for BrowserGlobals {
  fn info(&self) -> RuleInfo {
    RuleInfo::new(
      "fsr/no-browser-globals-in-render",
      Level::Error,
      "correctness",
      "### What it does\n\nDisallows a browser-only global, such as `window`, `document` or `localStorage`, where the server runs it: in a component's render or at the top of a component file, and anywhere in a loader, `actions.ts`, `route.ts` or middleware.\n\n### Why is this bad?\n\nEvery page renders on the server first, where there is no `window`, so the read throws and the page fails. Read it in an effect or a handler, or behind `typeof window !== \"undefined\"`.\n",
    )
  }

  fn check(&self, file: &File<'_>, found: &mut dyn FnMut(usize, String)) {
    let server = server_module(file.path);
    if !server && !component_file(file.path) {
      return;
    }
    let semantic = file.semantic;
    for node in semantic.nodes().iter() {
      let AstKind::IdentifierReference(ident) = node.kind() else { continue };
      if !BROWSER_GLOBALS.contains(&ident.name.as_str()) || semantic.scoping().get_reference(ident.reference_id()).symbol_id().is_some() {
        continue;
      }
      if matches!(semantic.nodes().parent_kind(node.id()), AstKind::UnaryExpression(u) if u.operator.is_typeof()) {
        continue;
      }
      let runs_on_server = server || (matches!(place(node, semantic), Place::Render) && !guarded(node, semantic, file.source));
      if runs_on_server {
        let what = if server { "this module runs only on the server" } else { "the component renders on the server first" };
        found(ident.span.start as usize, format!("`{}` is read where there is no browser: {what}", ident.name));
      }
    }
  }
}

/// The local symbols bound to FSR's `file` by an import from `@snapfire/fsr/host`.
fn fsr_file_symbols(semantic: &Semantic<'_>) -> Vec<SymbolId> {
  let program = semantic.nodes().program();
  let mut out = Vec::new();
  for statement in &program.body {
    let Statement::ImportDeclaration(import) = statement else { continue };
    if import.source.value != "@snapfire/fsr/host" {
      continue;
    }
    for specifier in import.specifiers.iter().flatten() {
      if let ImportDeclarationSpecifier::ImportSpecifier(named) = specifier
        && named.imported.name() == "file"
      {
        out.push(named.local.symbol_id());
      }
    }
  }
  out
}

struct FileRead;

impl Check for FileRead {
  fn info(&self) -> RuleInfo {
    RuleInfo::new(
      "fsr/no-file-read-in-render",
      Level::Error,
      "perf",
      "### What it does\n\nDisallows a call to `file` from `@snapfire/fsr/host` while a component renders.\n\n### Why is this bad?\n\nA component renders again on every change, and a terminal app many times a second, so the read repeats each time. Read the file in the page's loader, or once in an effect.\n",
    )
  }

  fn check(&self, file: &File<'_>, found: &mut dyn FnMut(usize, String)) {
    if server_module(file.path) || !component_file(file.path) {
      return;
    }
    let semantic = file.semantic;
    let symbols = fsr_file_symbols(semantic);
    if symbols.is_empty() {
      return;
    }
    for node in semantic.nodes().iter() {
      let AstKind::CallExpression(call) = node.kind() else { continue };
      let Some(member) = call.callee.as_member_expression() else { continue };
      let Expression::Identifier(object) = member.object() else { continue };
      let Some(symbol) = semantic.scoping().get_reference(object.reference_id()).symbol_id() else { continue };
      if symbols.contains(&symbol) && matches!(place(node, semantic), Place::Render) {
        let method = member.static_property_name().map_or_else(|| "call".to_owned(), |name| name.as_str().unwrap_or("call").to_owned());
        found(call.span.start as usize, format!("`file.{method}` runs on every render"));
      }
    }
  }
}

/// The output name snapfirec writes for a source, lowercased, or `None` for a file it does not compile alone.
fn output_name(name: &str) -> Option<String> {
  let lower = name.to_lowercase();
  if lower.ends_with(".d.ts") || lower.ends_with(".d.mts") || lower.ends_with(".d.cts") || lower.ends_with(".vue") || lower.ends_with(".svelte") {
    return None;
  }
  for ext in [".tsx", ".jsx", ".ts", ".js", ".mts", ".mjs", ".cts", ".cjs"] {
    if let Some(stem) = lower.strip_suffix(ext) {
      return Some(format!("{stem}.js"));
    }
  }
  None
}

struct OutputClash;

impl Check for OutputClash {
  fn info(&self) -> RuleInfo {
    RuleInfo::new(
      "fsr/output-name-clash",
      Level::Error,
      "correctness",
      "### What it does\n\nDisallows two sources in one directory that compile to the same output name, compared without case: `format.ts` beside `Format.js`, or `Card.svelte.ts` beside `Card.svelte.js`.\n\n### Why is this bad?\n\nBoth become one file in `dist/`, so one overwrites the other, and on a case-insensitive disk such as macOS's the names collide even when the case differs.\n",
    )
  }

  fn check(&self, file: &File<'_>, found: &mut dyn FnMut(usize, String)) {
    let Some(name) = file.path.file_name().and_then(|n| n.to_str()) else { return };
    let Some(output) = output_name(name) else { return };
    let Some(dir) = file.path.parent() else { return };
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    let mut clashing: Vec<String> = entries.filter_map(Result::ok).filter_map(|e| e.file_name().into_string().ok()).filter(|other| other != name && output_name(other).as_deref() == Some(output.as_str())).collect();
    clashing.sort();
    if let Some(first) = clashing.first()
      && name.to_lowercase() > first.to_lowercase()
    {
      found(0, format!("`{name}` and `{first}` both compile to `{output}`"));
    }
  }
}
