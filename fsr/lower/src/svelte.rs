//! Reads a Svelte component the plugin described and lowers it to a render
//! tree with the hydration markers Svelte's server compiler writes. The
//! plugin hands the template over cleaned and flagged as that compiler sees
//! it; the markers are placed here by the rules of its server visitors at
//! v5.57.2 (`IfBlock`, `EachBlock`, `RenderTag`, `HtmlTag`, `Fragment` and
//! `build_inline_component`). The scripts go through the body lowerer: runes
//! and Svelte 4's `export let` and `$:` bind as `let`s, `useStore` reads the
//! store. Anything outside that is residue and the component stays foreign.

use std::rc::Rc;

use serde::Deserialize;
use snapfire_compiler_wire::Described;
use snapfire_fsr_ir::ast::{ArithOp, Builtin, CompareOp, Component, Entry, Expr, Lit, LogicOp, Stmt, Tmpl};
use snapfire_fsr_ir::render::{RAW_ATTR, SVELTE_FLAGS};
use snapfire_fsr_ir::Owner;
use swc_core::common::Spanned;
use swc_core::ecma::ast as js;

use crate::assets::AssetResolver;
use crate::component::{bind_object, block_to_expr, HeadRow};
use crate::placements::{self, At, Placer};
use crate::vue::ChildRef;
use crate::{Lowered, Lowerer, Residue};

/// The client's Svelte adapter, where `useStore` and `useLocale` come from.
pub const SVELTE_CLIENT: &str = "@snapfire/fsr-client/svelte";

const BLOCK_OPEN: &str = "<!--[-->";
const BLOCK_OPEN_ELSE: &str = "<!--[!-->";
const BLOCK_CLOSE: &str = "<!--]-->";
const EMPTY: &str = "<!---->";

/// Calls at the top of a script that are the browser's alone.
const BROWSER_CALLS: &[&str] = &["$effect", "$effect.pre", "$effect.root", "$inspect", "onMount", "onDestroy", "beforeUpdate", "afterUpdate", "tick"];

#[derive(Debug, Deserialize)]
pub struct Template {
  pub runes: bool,
  pub instance: Option<(u32, u32)>,
  pub module: Option<(u32, u32)>,
  pub fragment: Fragment,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Fragment {
  #[serde(default)]
  pub hoisted: Vec<Node>,
  #[serde(default)]
  pub nodes: Vec<Node>,
  #[serde(default)]
  pub standalone: bool,
  #[serde(default)]
  pub text_first: bool,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Code {
  pub code: String,
  pub at: (usize, usize),
}

#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum Node {
  Text { data: String },
  Comment { data: String },
  Expr { code: String, at: (usize, usize) },
  Html { code: String, at: (usize, usize) },
  Const { id: Code, init: Code, at: (usize, usize) },
  Render { callee: Code, args: Vec<Code>, optional: bool, dynamic: bool, at: (usize, usize) },
  Element { name: String, svg: bool, #[serde(default)] mathml: bool, attrs: Vec<Attr>, fragment: Fragment, at: (usize, usize) },
  Component { name: String, dynamic: bool, attrs: Vec<Attr>, fragment: Fragment, at: (usize, usize) },
  Slot { attrs: Vec<Attr>, fragment: Fragment, at: (usize, usize) },
  If { elseif: bool, test: Code, consequent: Fragment, alternate: Option<Fragment>, at: (usize, usize) },
  Each { expression: Code, context: Option<Code>, index: Option<String>, key: Option<Code>, body: Fragment, fallback: Option<Fragment>, at: (usize, usize) },
  Snippet { name: String, params: Vec<Code>, body: Fragment, at: (usize, usize) },
  Other { kind: String, at: (usize, usize) },
}

#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum Attr {
  Attr { name: String, value: AttrValue, at: (usize, usize) },
  Spread { code: String, at: (usize, usize) },
  Attach { code: String, at: (usize, usize) },
  Directive { kind: String, name: String, #[serde(default)] modifiers: Vec<String>, #[serde(default)] expression: Option<Code>, #[serde(default)] value: Option<AttrValue>, at: (usize, usize) },
}

#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
pub enum AttrValue {
  Present(bool),
  Parts(Vec<Part>),
}

#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
pub enum Part {
  Text { text: String },
  Code(Code),
}

/// A snippet the template declared, in scope for a render tag below it.
#[derive(Clone)]
struct Snippet {
  name: String,
  params: Vec<Code>,
  body: Fragment,
}

pub(crate) struct SvelteLowerer<'a, 'p> {
  pub(crate) lowerer: Lowerer<'p>,
  described: &'a Described,
  template: Option<Template>,
  lets: Vec<Stmt>,
  state: Vec<String>,
  stores: Vec<(String, String)>,
  snippets: Vec<Snippet>,
  /// Snippets being rendered, so one that renders itself is refused.
  rendering: Vec<String>,
  pub(crate) child_refs: Vec<ChildRef>,
  assets: Rc<dyn AssetResolver>,
  pub(crate) heads: Vec<HeadRow>,
}

impl<'p> Placer<'p> for SvelteLowerer<'_, 'p> {
  fn lowerer(&mut self) -> &mut Lowerer<'p> {
    &mut self.lowerer
  }

  fn residue_at(&self, at: At, message: String) -> Residue {
    match at {
      At::Span(span) => self.lowerer.residue(span, message),
      At::Line(line, column) => self.at(line, column, message),
    }
  }

  fn assets(&self) -> Rc<dyn AssetResolver> {
    self.assets.clone()
  }

  fn head(&mut self, row: HeadRow) {
    if !self.heads.contains(&row) {
      self.heads.push(row);
    }
  }
}

impl<'a, 'p> SvelteLowerer<'a, 'p> {
  pub(crate) fn new(lowerer: Lowerer<'p>, described: &'a Described, assets: Rc<dyn AssetResolver>) -> Self {
    Self { lowerer, described, template: None, lets: Vec::new(), state: Vec::new(), stores: Vec::new(), snippets: Vec::new(), rendering: Vec::new(), child_refs: Vec::new(), assets, heads: Vec::new() }
  }

  pub(crate) fn component(&mut self) -> Lowered<Component> {
    let Some(value) = &self.described.template else { return Err(self.at(1, 1, "the plugin described no template")) };
    let template: Template = serde_json::from_value(value.clone()).map_err(|e| self.at(1, 1, format!("the template description did not read: {e}")))?;
    let runes = template.runes;
    let ranges = (template.module, template.instance);
    let fragment = template.fragment.clone();
    self.template = Some(template);
    self.script(runes, ranges)?;
    let mut nodes = self.fragment(&fragment)?;
    let render = match nodes.len() {
      1 => nodes.pop().expect("one node"),
      _ => Tmpl::Fragment(nodes),
    };
    Ok(Component { body: std::mem::take(&mut self.lets), render, state: std::mem::take(&mut self.state), stores: std::mem::take(&mut self.stores), handlers: Vec::new(), owner: Owner::Svelte, shadow: None, scope: None })
  }

  fn at(&self, line: usize, column: usize, message: impl Into<String>) -> Residue {
    Residue { file: self.lowerer.parsed.file.clone(), line, column, message: message.into(), hint: None, via: Vec::new() }
  }

  fn bind(&mut self, name: String, expr: Expr) {
    self.lowerer.scope.push((name, expr));
  }

  fn expr(&mut self, code: &str, (line, column): (usize, usize)) -> Lowered<Expr> {
    let parsed = self.lowerer.parsed.parse_expr_at(code, line, column).map_err(|message| self.at(line, column, format!("`{}`: {message}", code.trim())))?;
    if let Some(picture) = self.picture_call(&parsed, At::Line(line, column))? {
      return Ok(picture);
    }
    self.lowerer.expr(&parsed)
  }

  /// `picture({ … })`, the adapter's `picture` as the value it answers.
  fn picture_call(&mut self, expr: &js::Expr, at: At) -> Lowered<Option<Expr>> {
    let js::Expr::Call(call) = unwrap_types(expr) else { return Ok(None) };
    let js::Callee::Expr(callee) = &call.callee else { return Ok(None) };
    let js::Expr::Ident(id) = &**callee else { return Ok(None) };
    let meaning = self.lowerer.meanings.as_ref().and_then(|meanings| meanings.bindings.get(id.sym.as_ref())).and_then(|meant| meant.placement());
    if meaning != Some("picture") {
      return Ok(None);
    }
    let Some(js::Expr::Object(object)) = call.args.first().map(|a| unwrap_types(&a.expr)) else {
      return Err(self.residue_at(at, "`picture()` options that are not an object literal".to_owned()));
    };
    let mut placed: Vec<placements::Attr<'static>> = Vec::new();
    for prop in &object.props {
      let js::PropOrSpread::Prop(prop) = prop else { return Err(self.residue_at(at, "a spread in `picture()` options".to_owned())) };
      let (name, value) = match &**prop {
        js::Prop::KeyValue(kv) => (crate::prop_name(&kv.key).ok_or_else(|| self.residue_at(at, "a computed key in `picture()` options".to_owned()))?, kv.value.clone()),
        js::Prop::Shorthand(id) => (id.sym.to_string(), Box::new(js::Expr::Ident(id.clone()))),
        _ => return Err(self.residue_at(at, "a `picture()` option that is not `key: value`".to_owned())),
      };
      placed.push(placements::Attr { name, value: placements::Value::Script(std::borrow::Cow::Owned(*value)), at });
    }
    placements::picture_data(self, &placed, at).map(Some)
  }

  // ---- the scripts -------------------------------------------------------

  /// Every statement of `<script module>` and the instance script, module first as Svelte runs them.
  fn script(&mut self, runes: bool, (module, instance): (Option<(u32, u32)>, Option<(u32, u32)>)) -> Lowered<()> {
    if self.described.script.is_none() {
      return Ok(());
    }
    let parsed = self.lowerer.parsed;
    let start = parsed.cm.files().first().map(|f| f.start_pos.0).unwrap_or(0);
    let offset = |span: swc_core::common::Span| span.lo.0.saturating_sub(start);
    let within = |range: Option<(u32, u32)>, at: u32| range.is_some_and(|(from, to)| at >= from && at < to);
    for item in &parsed.module.body {
      let at = offset(item.span());
      let in_instance = within(instance, at);
      if !in_instance && !within(module, at) {
        continue;
      }
      match item {
        js::ModuleItem::ModuleDecl(js::ModuleDecl::Import(_)) => {}
        js::ModuleItem::ModuleDecl(js::ModuleDecl::ExportDecl(export)) => match &export.decl {
          js::Decl::Var(var) if !runes && in_instance => self.export_lets(var)?,
          js::Decl::Var(var) => self.declaration(var, runes)?,
          js::Decl::Fn(_) | js::Decl::TsInterface(_) | js::Decl::TsTypeAlias(_) => {}
          other => return Err(self.lowerer.residue(other.span(), "an export the build does not read")),
        },
        js::ModuleItem::ModuleDecl(other) => return Err(self.lowerer.residue(other.span(), "an export the build does not read")),
        js::ModuleItem::Stmt(stmt) => self.statement(stmt, runes)?,
      }
    }
    Ok(())
  }

  fn statement(&mut self, stmt: &'p js::Stmt, runes: bool) -> Lowered<()> {
    match stmt {
      js::Stmt::Decl(js::Decl::Var(var)) => self.declaration(var, runes),
      js::Stmt::Decl(js::Decl::Fn(_) | js::Decl::TsInterface(_) | js::Decl::TsTypeAlias(_)) => Ok(()),
      js::Stmt::Expr(e) => match &*e.expr {
        js::Expr::Call(call) if callee_name(call).is_some_and(|name| BROWSER_CALLS.contains(&name.as_str())) => Ok(()),
        _ => Err(self.statement_residue(e.span)),
      },
      js::Stmt::Labeled(labeled) if !runes && labeled.label.sym == "$" => self.reactive(labeled),
      js::Stmt::Empty(_) => Ok(()),
      other => Err(self.statement_residue(other.span())),
    }
  }

  fn statement_residue(&self, span: swc_core::common::Span) -> Residue {
    self.lowerer.residue_with(span, "a statement in a Svelte script the build cannot follow", "a script the build reads is declarations, runes, `useStore` and the calls the browser alone runs: `$effect`, `$inspect`, `onMount` and the other lifecycle functions")
  }

  /// `$: name = expr` is a value derived from what it reads; any other reactive statement runs in the browser.
  fn reactive(&mut self, labeled: &'p js::LabeledStmt) -> Lowered<()> {
    let js::Stmt::Expr(e) = &*labeled.body else { return Ok(()) };
    let js::Expr::Assign(assign) = &*e.expr else { return Ok(()) };
    let Some(js::SimpleAssignTarget::Ident(target)) = assign.left.as_simple() else { return Ok(()) };
    if assign.op != js::AssignOp::Assign {
      return Err(self.lowerer.residue(assign.span, "a `$:` that updates rather than assigns"));
    }
    let name = target.id.sym.to_string();
    let expr = self.lowerer.expr(&assign.right)?;
    self.lets.push(Stmt::Let { name: name.clone(), expr });
    self.bind(name.clone(), Expr::Var(name));
    Ok(())
  }

  /// `export let a = 1` in Svelte 4: the prop, else its default.
  fn export_lets(&mut self, var: &'p js::VarDecl) -> Lowered<()> {
    for decl in &var.decls {
      let js::Pat::Ident(name) = &decl.name else { return Err(self.lowerer.residue(decl.span, "an exported prop that is not a name")) };
      let name = name.id.sym.to_string();
      let read = Expr::Var("$props".to_owned()).field(name.clone());
      let expr = match &decl.init {
        Some(init) => Expr::Coalesce(Box::new(read), Box::new(self.lowerer.expr(init)?)),
        None => read,
      };
      self.lets.push(Stmt::Let { name: name.clone(), expr });
      self.bind(name.clone(), Expr::Var(name));
    }
    Ok(())
  }

  fn declaration(&mut self, var: &'p js::VarDecl, runes: bool) -> Lowered<()> {
    for decl in &var.decls {
      let Some(init) = decl.init.as_deref() else {
        if let js::Pat::Ident(name) = &decl.name {
          let name = name.id.sym.to_string();
          self.lets.push(Stmt::Let { name: name.clone(), expr: Expr::Lit(Lit::Null) });
          self.bind(name.clone(), Expr::Var(name));
          continue;
        }
        return Err(self.lowerer.residue(decl.span, "a declaration without a value"));
      };
      let init = unwrap_types(init);
      let called = match init {
        js::Expr::Call(call) => callee_name(call),
        _ => None,
      };
      match (called.as_deref(), &decl.name) {
        (Some("$props"), js::Pat::Object(obj)) if runes => {
          let obj = without_bindable(obj);
          bind_object(&mut self.lowerer, &obj, Expr::Var("$props".to_owned()))?;
        }
        (Some("$props"), js::Pat::Ident(name)) if runes => self.bind(name.id.sym.to_string(), Expr::Var("$props".to_owned())),
        (Some("$props"), other) if runes => return Err(self.lowerer.residue(other.span(), "`$props()` bound to a pattern the build does not read")),
        (Some(rune), js::Pat::Ident(name)) if runes && rune.starts_with('$') => {
          let name = name.id.sym.to_string();
          let js::Expr::Call(call) = init else { unreachable!("a rune is a call") };
          let arg = call.args.first().map(|a| &*a.expr);
          let expr = match rune {
            "$state" | "$state.raw" => match arg {
              Some(arg) => self.lowerer.expr(arg)?,
              None => Expr::Lit(Lit::Null),
            },
            "$derived" => match arg {
              Some(arg) => self.lowerer.expr(arg)?,
              None => return Err(self.lowerer.residue(call.span, "`$derived` without a value")),
            },
            "$derived.by" => match arg.map(unwrap_types) {
              Some(js::Expr::Arrow(arrow)) if arrow.params.is_empty() => match &*arrow.body {
                js::ArrowFunctionBody::Expr(e) => self.lowerer.expr(e)?,
                js::ArrowFunctionBody::FunctionBody(b) => block_to_expr(&mut self.lowerer, &b.stmts)?,
              },
              _ => return Err(self.lowerer.residue(call.span, "`$derived.by` of something other than an arrow without parameters")),
            },
            other => return Err(self.lowerer.residue(call.span, format!("`{other}`, a rune the build does not read"))),
          };
          self.lets.push(Stmt::Let { name: name.clone(), expr });
          if rune.starts_with("$state") {
            self.state.push(name.clone());
          }
          self.bind(name.clone(), Expr::Var(name));
        }
        (_, js::Pat::Ident(name)) => self.binding(name.id.sym.to_string(), init, decl.span, runes)?,
        (_, other) => return Err(self.lowerer.residue(other.span(), "a declaration pattern the build does not read")),
      }
    }
    Ok(())
  }

  fn binding(&mut self, name: String, init: &'p js::Expr, span: swc_core::common::Span, runes: bool) -> Lowered<()> {
    if let js::Expr::Call(call) = init {
      if let Some(hook) = self.adapter_hook(call) {
        match hook {
          "useStore" => {
            let Some(first) = call.args.first() else { return Err(self.lowerer.residue(call.span, "`useStore` without a key")) };
            let key = match self.lowerer.expr(&first.expr)? {
              Expr::Lit(Lit::Str(key)) => key,
              _ => return Err(self.lowerer.residue(first.expr.span(), "a `useStore` key that is not a string the build can read")),
            };
            let initial = match call.args.get(1) {
              Some(a) => self.lowerer.expr(&a.expr)?,
              None => Expr::Lit(Lit::Null),
            };
            self.lets.push(Stmt::Let { name: name.clone(), expr: Expr::Coalesce(Box::new(Expr::Store(key.clone())), Box::new(initial)) });
            self.state.push(name.clone());
            self.stores.push((name.clone(), key));
            self.bind(format!("${name}"), Expr::Var(name.clone()));
            self.bind(name.clone(), Expr::Var(name));
            return Ok(());
          }
          "useLocale" => {
            self.lets.push(Stmt::Let { name: name.clone(), expr: Expr::Locale });
            self.bind(format!("${name}"), Expr::Var(name.clone()));
            self.bind(name.clone(), Expr::Var(name));
            return Ok(());
          }
          _ => {}
        }
      }
    }
    if matches!(unwrap_types(init), js::Expr::Arrow(_) | js::Expr::Fn(_)) {
      return Ok(());
    }
    if let Some(picture) = self.picture_call(init, At::Span(init.span()))? {
      self.lets.push(Stmt::Let { name: name.clone(), expr: picture });
      self.bind(name.clone(), Expr::Var(name));
      return Ok(());
    }
    let expr = self.lowerer.expr(init).map_err(|residue| match residue.line {
      0 => self.lowerer.residue(span, residue.message),
      _ => residue,
    })?;
    self.lets.push(Stmt::Let { name: name.clone(), expr });
    if !runes {
      self.state.push(name.clone());
    }
    self.bind(name.clone(), Expr::Var(name.clone()));
    self.lowerer.note_kind(&name, init);
    Ok(())
  }

  /// The adapter function a call names, followed through its import.
  fn adapter_hook(&self, call: &js::CallExpr) -> Option<&'static str> {
    let js::Callee::Expr(callee) = &call.callee else { return None };
    let js::Expr::Ident(id) = &**callee else { return None };
    self.lowerer.meanings.as_ref().and_then(|meanings| meanings.of_ident(id)).and_then(|meant| meant.hook())
  }

  // ---- the template ------------------------------------------------------

  /// A fragment as Svelte's `Fragment` visitor writes it: the hoisted
  /// snippets and `{@const}` tags first, an anchor when it starts with text,
  /// then its nodes.
  fn fragment(&mut self, fragment: &Fragment) -> Lowered<Vec<Tmpl>> {
    let snippets_before = self.snippets.len();
    let mut consts: Vec<(String, Expr)> = Vec::new();
    let scope_before = self.lowerer.scope.len();
    for node in &fragment.hoisted {
      match node {
        Node::Snippet { name, params, body, .. } => self.snippets.push(Snippet { name: name.clone(), params: params.clone(), body: body.clone() }),
        Node::Const { id, init, at } => {
          let name = id.code.trim().to_owned();
          if !is_identifier(&name) {
            return Err(self.at(at.0, at.1, "a `{@const}` that destructures"));
          }
          let expr = self.expr(&init.code, init.at)?;
          consts.push((name.clone(), expr));
          self.bind(name.clone(), Expr::Var(name));
        }
        Node::Other { kind, at } if kind == "DebugTag" => {
          let _ = at;
        }
        Node::Other { kind, at } => return Err(self.at(at.0, at.1, format!("`{}`, which the build does not render on the server", kind_name(kind)))),
        _ => {}
      }
    }
    let mut out = Vec::new();
    if fragment.text_first {
      out.push(Tmpl::Marker(EMPTY.to_owned()));
    }
    for node in &fragment.nodes {
      self.node(node, fragment, &mut out)?;
    }
    self.snippets.truncate(snippets_before);
    self.lowerer.scope.truncate(scope_before);
    for (name, expr) in consts.into_iter().rev() {
      out = vec![Tmpl::Let { name, expr, then: Box::new(Tmpl::Fragment(out)) }];
    }
    Ok(out)
  }

  fn node(&mut self, node: &Node, parent: &Fragment, out: &mut Vec<Tmpl>) -> Lowered<()> {
    match node {
      Node::Text { data } => out.push(Tmpl::Text(data.clone())),
      Node::Comment { data } => out.push(Tmpl::Marker(format!("<!--{data}-->"))),
      Node::Expr { code, at } => out.push(Tmpl::Expr(self.expr(code, *at)?)),
      Node::Html { at, .. } => return Err(self.at(at.0, at.1, "`{@html}` beside other content; the build writes it as an element's whole content")),
      Node::Element { .. } => out.push(self.element(node)?),
      Node::Component { .. } => self.component_node(node, parent.standalone, out)?,
      Node::Render { .. } => self.render_tag(node, parent.standalone, out)?,
      Node::If { .. } => self.if_block(node, out)?,
      Node::Each { .. } => self.each_block(node, out)?,
      Node::Slot { at, .. } => return Err(self.at(at.0, at.1, "`<slot>`, which the build does not lower yet")),
      Node::Const { at, .. } | Node::Snippet { at, .. } => return Err(self.at(at.0, at.1, "a declaration among the nodes")),
      Node::Other { kind, at } => return Err(self.at(at.0, at.1, format!("`{}`, which the build does not render on the server", kind_name(kind)))),
    }
    Ok(())
  }

  /// `IfBlock`: each branch opens with its index in the flattened else-if
  /// chain and the final branch with `-1` whether or not it renders. The
  /// block closes after the chain.
  fn if_block(&mut self, node: &Node, out: &mut Vec<Tmpl>) -> Lowered<()> {
    let mut branches: Vec<(Expr, Vec<Tmpl>)> = Vec::new();
    let mut current = node;
    let last = loop {
      let Node::If { test, consequent, alternate, .. } = current else { unreachable!("an if block") };
      let cond = self.expr(&test.code, test.at)?;
      let mut body = vec![Tmpl::Marker(format!("<!--[{}-->", branches.len()))];
      body.extend(self.fragment(consequent)?);
      branches.push((cond, body));
      match alternate {
        Some(alt) if alt.hoisted.is_empty() && matches!(alt.nodes.as_slice(), [Node::If { elseif: true, .. }]) => current = &alt.nodes[0],
        Some(alt) => {
          let mut body = vec![Tmpl::Marker("<!--[-1-->".to_owned())];
          body.extend(self.fragment(alt)?);
          break body;
        }
        None => break vec![Tmpl::Marker("<!--[-1-->".to_owned())],
      }
    };
    let mut chain = Tmpl::Fragment(last);
    for (cond, body) in branches.into_iter().rev() {
      chain = Tmpl::If { cond, then: Box::new(Tmpl::Fragment(body)), r#else: Some(Box::new(chain)) };
    }
    out.push(chain);
    out.push(Tmpl::Marker(BLOCK_CLOSE.to_owned()));
    Ok(())
  }

  /// `EachBlock`: the items between a block's open and close. An `{:else}`
  /// that renders writes its fallback after `<!--[!-->` instead.
  fn each_block(&mut self, node: &Node, out: &mut Vec<Tmpl>) -> Lowered<()> {
    let Node::Each { expression, context, index, body, fallback, at, .. } = node else { unreachable!("an each block") };
    let over = Expr::Coalesce(Box::new(self.expr(&expression.code, expression.at)?), Box::new(Expr::Array(Vec::new())));
    let length = self.expr(&format!("(({}) ?? []).length", expression.code), expression.at)?;
    let scope_before = self.lowerer.scope.len();
    let item = match context {
      Some(context) => {
        let name = context.code.trim().to_owned();
        if !is_identifier(&name) {
          return Err(self.at(context.at.0, context.at.1, "an `{#each}` item that destructures"));
        }
        name
      }
      None => self.lowerer.temp(),
    };
    let index_name = index.clone().unwrap_or_else(|| self.lowerer.temp());
    self.bind(item.clone(), Expr::Var(item.clone()));
    self.bind(index_name.clone(), Expr::Var(index_name.clone()));
    let lowered = self.fragment(body);
    self.lowerer.scope.truncate(scope_before);
    let items = Tmpl::For { over: over.clone(), params: vec![item, index_name], body: Box::new(Tmpl::Fragment(lowered?)) };
    match fallback {
      Some(fallback) => {
        let mut empty = vec![Tmpl::Marker(BLOCK_OPEN_ELSE.to_owned())];
        empty.extend(self.fragment(fallback)?);
        out.push(Tmpl::If {
          cond: Expr::Compare(CompareOp::Ne, Box::new(length), Box::new(Expr::Lit(Lit::Int(0)))),
          then: Box::new(Tmpl::Fragment(vec![Tmpl::Marker(BLOCK_OPEN.to_owned()), items])),
          r#else: Some(Box::new(Tmpl::Fragment(empty))),
        });
      }
      None => {
        out.push(Tmpl::Marker(BLOCK_OPEN.to_owned()));
        out.push(items);
      }
    }
    let _ = at;
    out.push(Tmpl::Marker(BLOCK_CLOSE.to_owned()));
    Ok(())
  }

  /// `RenderTag`: a snippet the template declared renders where it is
  /// called, `children` where the caller's children go; an anchor follows
  /// unless the tag is its fragment's one node.
  fn render_tag(&mut self, node: &Node, standalone: bool, out: &mut Vec<Tmpl>) -> Lowered<()> {
    let Node::Render { callee, args, dynamic, at, .. } = node else { unreachable!("a render tag") };
    let name = callee.code.trim();
    if *dynamic {
      if name != "children" || !args.is_empty() {
        return Err(self.at(at.0, at.1, format!("`{{@render {name}()}}`, a snippet passed in by the caller")));
      }
      out.push(Tmpl::Slot("content".to_owned()));
      out.push(Tmpl::Marker(EMPTY.to_owned()));
      return Ok(());
    }
    let Some(snippet) = self.snippets.iter().rev().find(|s| s.name == name).cloned() else {
      return Err(self.at(at.0, at.1, format!("`{{@render {name}()}}`, a snippet the template does not declare")));
    };
    if self.rendering.contains(&snippet.name) {
      return Err(self.at(at.0, at.1, format!("`{name}`, a snippet that renders itself")));
    }
    let mut bound = Vec::new();
    for (i, param) in snippet.params.iter().enumerate() {
      let param_name = param.code.trim().to_owned();
      if !is_identifier(&param_name) {
        return Err(self.at(param.at.0, param.at.1, "a snippet parameter that destructures or has a default"));
      }
      let value = match args.get(i) {
        Some(arg) => self.expr(&arg.code, arg.at)?,
        None => Expr::Lit(Lit::Null),
      };
      bound.push((param_name, value));
    }
    let scope_before = self.lowerer.scope.len();
    for (param, _) in &bound {
      self.bind(param.clone(), Expr::Var(param.clone()));
    }
    self.rendering.push(snippet.name.clone());
    let body = self.fragment(&snippet.body);
    self.rendering.pop();
    self.lowerer.scope.truncate(scope_before);
    let mut tree = Tmpl::Fragment(body?);
    for (param, value) in bound.into_iter().rev() {
      tree = Tmpl::Let { name: param, expr: value, then: Box::new(tree) };
    }
    out.push(tree);
    if !standalone {
      out.push(Tmpl::Marker(EMPTY.to_owned()));
    }
    Ok(())
  }

  /// `build_inline_component` for a component the script imports: its props
  /// and its children as the caller's children, an anchor after it unless it
  /// is its fragment's one node.
  fn component_node(&mut self, node: &Node, standalone: bool, out: &mut Vec<Tmpl>) -> Lowered<()> {
    let Node::Component { name, dynamic, attrs, fragment, at } = node else { unreachable!("a component") };
    if *dynamic || name.contains('.') {
      return Err(self.at(at.0, at.1, format!("`<{name}>`, a component chosen at run time")));
    }
    let mut props = Vec::new();
    for attr in attrs {
      match attr {
        Attr::Attr { name: prop, value, .. } if !prop.starts_with("--") => {
          let value = self.attr_value(value, false)?;
          props.push(Entry::Field(prop.clone(), value));
        }
        Attr::Attr { at, .. } => return Err(self.at(at.0, at.1, "a CSS custom property passed to a component")),
        Attr::Spread { code, at } => props.push(Entry::Spread(self.expr(code, *at)?)),
        Attr::Directive { kind, at, .. } if kind == "bind" => return Err(self.at(at.0, at.1, "`bind:` on a component")),
        Attr::Directive { .. } | Attr::Attach { .. } => {}
      }
    }
    if fragment.hoisted.iter().any(|n| matches!(n, Node::Snippet { .. })) {
      return Err(self.at(at.0, at.1, format!("a snippet passed to `<{name}>`")));
    }
    let children = self.fragment(fragment)?;
    self.child_refs.push(ChildRef { local: name.clone(), attrs: Vec::new(), line: at.0, column: at.1 });
    out.push(Tmpl::Component { module: name.clone(), props, children, id: 0, keyed: false });
    if !standalone {
      out.push(Tmpl::Marker(EMPTY.to_owned()));
    }
    Ok(())
  }

  /// An attribute's value as Svelte writes it: present as `""`, one part as
  /// itself, several as one string with `null` written as nothing.
  fn attr_value(&mut self, value: &AttrValue, stringify: bool) -> Lowered<Expr> {
    match value {
      AttrValue::Present(_) => Ok(Expr::lit_str(String::new())),
      AttrValue::Parts(parts) => match parts.as_slice() {
        [Part::Text { text }] => Ok(Expr::lit_str(text.clone())),
        [Part::Code(code)] if !stringify => self.expr(&code.code, code.at),
        parts => {
          let mut pieces = Vec::new();
          for part in parts {
            pieces.push(match part {
              Part::Text { text } => Expr::lit_str(text.clone()),
              Part::Code(code) => Expr::Coalesce(Box::new(self.expr(&code.code, code.at)?), Box::new(Expr::lit_str(String::new()))),
            });
          }
          Ok(Expr::Template(pieces))
        }
      },
    }
  }

  /// The adapter placement a spread calls, `link({ href })`, with its options object.
  fn placement_spread(&mut self, code: &str, at: (usize, usize)) -> Lowered<Option<(&'static str, Box<js::Expr>)>> {
    let parsed = self.lowerer.parsed.parse_expr_at(code, at.0, at.1).map_err(|message| self.at(at.0, at.1, format!("`{}`: {message}", code.trim())))?;
    let js::Expr::Call(call) = &*parsed else { return Ok(None) };
    let js::Callee::Expr(callee) = &call.callee else { return Ok(None) };
    let js::Expr::Ident(id) = &**callee else { return Ok(None) };
    let meaning = self.lowerer.meanings.as_ref().and_then(|meanings| meanings.bindings.get(id.sym.as_ref())).and_then(|meant| meant.placement());
    let Some(placement) = meaning else { return Ok(None) };
    match call.args.first() {
      Some(arg) => Ok(Some((placement, arg.expr.clone()))),
      None => Err(self.at(at.0, at.1, format!("`{placement}()` without its options"))),
    }
  }

  /// `<a {...link({ href })} class="x">`: the options and the element's own
  /// attributes as one `Link` placement, which writes the marks the
  /// navigator reads the way the adapter's `link` does in the browser.
  fn link_element(&mut self, options: &js::Expr, attrs: &[Attr], fragment: &Fragment, at: (usize, usize)) -> Lowered<Tmpl> {
    let js::Expr::Object(object) = options else { return Err(self.at(at.0, at.1, "`link()` options that are not an object literal")) };
    let mut placed: Vec<placements::Attr<'static>> = Vec::new();
    for prop in &object.props {
      let js::PropOrSpread::Prop(prop) = prop else { return Err(self.at(at.0, at.1, "a spread in `link()` options")) };
      let (name, value) = match &**prop {
        js::Prop::KeyValue(kv) => (crate::prop_name(&kv.key).ok_or_else(|| self.at(at.0, at.1, "a computed key in `link()` options"))?, kv.value.clone()),
        js::Prop::Shorthand(id) => (id.sym.to_string(), Box::new(js::Expr::Ident(id.clone()))),
        _ => return Err(self.at(at.0, at.1, "a `link()` option that is not `key: value`")),
      };
      placed.push(placements::Attr { name, value: placements::Value::Script(std::borrow::Cow::Owned(*value)), at: At::Line(at.0, at.1) });
    }
    let mut extra: Vec<Entry> = Vec::new();
    for attr in attrs {
      match attr {
        Attr::Spread { .. } | Attr::Attach { .. } => {}
        Attr::Attr { name, value, at } => {
          if name.starts_with("on") && matches!(value, AttrValue::Parts(parts) if matches!(parts.as_slice(), [Part::Code(_)])) {
            continue;
          }
          let _ = at;
          let expr = self.attr_value(value, true)?;
          extra.push(Entry::Field(name.clone(), expr));
        }
        Attr::Directive { kind, at, .. } if !matches!(kind.as_str(), "on" | "use" | "transition" | "in" | "out" | "animate") => return Err(self.at(at.0, at.1, format!("`{kind}:` beside a `link()` spread"))),
        Attr::Directive { .. } => {}
      }
    }
    let children = self.fragment(fragment)?;
    let Tmpl::Element { tag, mut attrs, children } = placements::link(self, &placed, children)? else { unreachable!("a link is an element") };
    attrs.extend(extra);
    Ok(Tmpl::Element { tag, attrs, children })
  }

  /// `build_element_attributes`: each attribute, binding and spread in order,
  /// through `$.attributes` when a spread or an input's `defaultValue` is
  /// among them and one by one otherwise.
  fn element(&mut self, node: &Node) -> Lowered<Tmpl> {
    let Node::Element { name, svg, mathml, attrs, fragment, at } = node else { unreachable!("an element") };
    for attr in attrs {
      if let Attr::Spread { code, at: spread_at } = attr {
        match self.placement_spread(code, *spread_at)? {
          Some(("link", options)) if name == "a" => return self.link_element(&options, attrs, fragment, *at),
          Some((placement, _)) => return Err(self.at(spread_at.0, spread_at.1, format!("a `{placement}()` spread on `<{name}>`"))),
          None => {}
        }
      }
    }
    if name == "select" && attrs.iter().any(|a| matches!(a, Attr::Attr { name, .. } if name == "value") || matches!(a, Attr::Directive { kind, name, .. } if kind == "bind" && name == "value")) {
      return Err(self.at(at.0, at.1, "a `<select>` with a `value`, whose options Svelte marks selected"));
    }
    let namespaced = *svg || *mathml;
    let attr_name = |n: &str| if namespaced { n.to_owned() } else { n.to_lowercase() };
    let load_error = LOAD_ERROR.contains(&name.as_str());
    let whole = attrs.iter().any(|a| matches!(a, Attr::Spread { .. }) || (name == "input" && matches!(a, Attr::Attr { name, .. } if name == "defaultValue" || name == "defaultChecked")));
    let mut items: Vec<Item> = Vec::new();
    let mut body: Option<Body> = None;
    let mut captures: Vec<&'static str> = Vec::new();
    let capture = |captures: &mut Vec<&'static str>, names: &[&'static str]| {
      for n in names {
        if !captures.contains(n) {
          captures.push(n);
        }
      }
    };
    let mut class_directives: Vec<(String, Expr)> = Vec::new();
    let mut style_directives: Vec<StyleDirective> = Vec::new();
    for attr in attrs {
      match attr {
        Attr::Attr { name: a, value, .. } => {
          if a == "value" && name == "textarea" {
            body = Some(Body::Text(self.textarea_value(value)?));
          } else if a == "value" && name == "select" {
          } else if is_event(a, value) {
            if load_error && (a == "onload" || a == "onerror") {
              capture(&mut captures, &[if a == "onload" { "onload" } else { "onerror" }]);
            }
          } else if a == "defaultValue" || a == "defaultChecked" {
            if name == "input" {
              items.push(Item::Attr { name: a.clone(), value: value.clone() });
            }
          } else {
            items.push(Item::Attr { name: a.clone(), value: value.clone() });
          }
        }
        Attr::Spread { code, at } => {
          items.push(Item::Spread(self.expr(code, *at)?));
          if load_error {
            capture(&mut captures, &["onload", "onerror"]);
          }
        }
        Attr::Attach { .. } => {}
        Attr::Directive { kind, name: directive, expression, value, modifiers, at } => match kind.as_str() {
          "bind" => {
            let file = items.iter().any(|item| matches!(item, Item::Attr { name, value } if name == "type" && static_text(value) == Some("file")));
            if (directive == "value" && (name == "select" || file)) || directive == "this" || OMIT_IN_SSR.contains(&directive.as_str()) {
              continue;
            }
            let (value, sequence) = self.bound(expression, directive, *at)?;
            if directive == "innerHTML" {
              body = Some(Body::Raw(value));
            } else if matches!(directive.as_str(), "textContent" | "innerText") || (directive == "value" && name == "textarea") {
              body = Some(Body::Text(value));
            } else if directive == "group" && !sequence {
              let Some(Attr::Attr { value: given, .. }) = attrs.iter().find(|a| matches!(a, Attr::Attr { name, .. } if name == "value")) else { continue };
              let checkbox = attrs.iter().any(|a| matches!(a, Attr::Attr { name, value, .. } if name == "type" && static_text(value) == Some("checkbox")));
              let given = self.ss(given, false)?;
              let checked = match checkbox {
                true => call(Builtin::Includes, vec![value, given]),
                false => cmp(CompareOp::Eq, value, given),
              };
              items.push(Item::Bound { name: "checked".to_owned(), value: checked });
            } else {
              items.push(Item::Bound { name: attr_name(directive), value });
            }
          }
          "use" => {
            if load_error {
              capture(&mut captures, &["onload", "onerror"]);
            }
          }
          "class" => {
            let cond = match expression {
              Some(code) => self.expr(&code.code, code.at)?,
              None => self.expr(directive, *at)?,
            };
            class_directives.push((directive.clone(), cond));
          }
          "style" => {
            let value = match value {
              Some(value) => self.ss(value, true)?,
              None => self.expr(directive, *at)?,
            };
            style_directives.push(match whole {
              true => StyleDirective { name: directive.clone(), value, important: false },
              false => StyleDirective { name: css_name(directive), value, important: modifiers.iter().any(|m| m == "important") },
            });
          }
          "on" | "transition" | "in" | "out" | "animate" | "let" => {}
          other => return Err(self.at(at.0, at.1, format!("`{other}:`, a directive the build does not read"))),
        },
      }
    }
    let mut entries: Vec<Entry> = Vec::new();
    if whole {
      let mut fields: Vec<Entry> = Vec::new();
      for item in items {
        match item {
          Item::Attr { name: a, value } => {
            let n = attr_name(&a);
            let value = match n.as_str() {
              "class" => self.class_value(&value)?,
              "style" => self.ss(&value, true)?,
              _ => self.ss(&value, false)?,
            };
            fields.push(Entry::Field(n, value));
          }
          Item::Bound { name, value } => fields.push(Entry::Field(name, value)),
          Item::Spread(e) => fields.push(Entry::Spread(e)),
        }
      }
      entries.push(Entry::Spread(merged_attributes(fields, class_directives, style_directives)));
      let flags = if namespaced { 1 | 2 } else if name.contains('-') || attrs.iter().any(|a| matches!(a, Attr::Attr { name, .. } if name == "is")) { 2 } else if name == "input" { 4 } else { 0 };
      if flags != 0 {
        entries.push(Entry::Field(SVELTE_FLAGS.to_owned(), int(flags)));
      }
    } else {
      for item in items {
        match item {
          Item::Bound { name, value } => entries.push(Entry::Field(name, value)),
          Item::Spread(_) => unreachable!("a spread takes the other path"),
          Item::Attr { name: a, value } => {
            let n = attr_name(&a);
            let directed = (n == "class" && !class_directives.is_empty()) || (n == "style" && !style_directives.is_empty());
            let trim = n == "class" || n == "style";
            if !directed && (matches!(value, AttrValue::Present(true)) || static_text(&value).is_some()) {
              let written = match &value {
                AttrValue::Present(_) => String::new(),
                _ => match self.ss(&value, trim)? {
                  Expr::Lit(Lit::Str(text)) => text.to_string(),
                  _ => unreachable!("static text is a literal"),
                },
              };
              if n != "class" || !written.is_empty() || matches!(value, AttrValue::Present(_)) {
                entries.push(Entry::Baked(format!(" {n}=\"{written}\"")));
              }
              continue;
            }
            match n.as_str() {
              "class" => {
                let class = self.class_value(&value)?;
                let directives = std::mem::take(&mut class_directives);
                entries.push(Entry::Field(n, self.class_attr(Some(class), directives)?.expect("a class")));
              }
              "style" => {
                let style = self.ss(&value, true)?;
                let directives = std::mem::take(&mut style_directives);
                entries.push(Entry::Field(n, self.style_attr(Some(style), directives)?.expect("a style")));
              }
              _ => {
                let value = self.ss(&value, false)?;
                entries.push(Entry::Field(n, value));
              }
            }
          }
        }
      }
      if !class_directives.is_empty() {
        entries.push(Entry::Field("class".to_owned(), self.class_attr(None, class_directives)?.expect("a class")));
      }
      if !style_directives.is_empty() {
        entries.push(Entry::Field("style".to_owned(), self.style_attr(None, style_directives)?.expect("a style")));
      }
    }
    for name in captures {
      entries.push(Entry::Baked(format!(" {name}=\"this.__e=event\"")));
    }
    let children = match (body, fragment.nodes.as_slice()) {
      (Some(Body::Text(value)), _) => vec![Tmpl::Expr(value)],
      (Some(Body::Raw(value)), _) => {
        entries.push(Entry::Field(RAW_ATTR.to_owned(), Expr::Coalesce(Box::new(value), Box::new(text("")))));
        Vec::new()
      }
      (None, [Node::Html { code, at }]) if fragment.hoisted.is_empty() => {
        let value = Expr::Coalesce(Box::new(self.expr(code, *at)?), Box::new(Expr::lit_str(String::new())));
        entries.push(Entry::Field(RAW_ATTR.to_owned(), Expr::Template(vec![Expr::lit_str(EMPTY.to_owned()), value, Expr::lit_str(EMPTY.to_owned())])));
        Vec::new()
      }
      _ => self.fragment(fragment)?,
    };
    Ok(Tmpl::Element { tag: name.clone(), attrs: entries, children })
  }

  /// A binding's value, the getter's result for `bind:x={get, set}`, and
  /// whether it was that pair.
  fn bound(&mut self, expression: &Option<Code>, name: &str, at: (usize, usize)) -> Lowered<(Expr, bool)> {
    let (code, at) = match expression {
      Some(code) => (code.code.as_str(), code.at),
      None => (name, at),
    };
    let parsed = self.lowerer.parsed.parse_expr_at(code, at.0, at.1).map_err(|message| self.at(at.0, at.1, format!("`{}`: {message}", code.trim())))?;
    match unwrap_types(&parsed) {
      js::Expr::Seq(seq) if !seq.exprs.is_empty() => {
        let call = js::Expr::Call(js::CallExpr { callee: js::Callee::Expr(seq.exprs[0].clone()), ..Default::default() });
        Ok((self.lowerer.expr(&call)?, true))
      }
      _ => Ok((self.lowerer.expr(&parsed)?, false)),
    }
  }

  /// `build_attribute_value`: `true` as it is; a value that is text alone a
  /// literal escaped for an attribute; an expression alone as it is; text and
  /// expressions a template of the text and each expression stringified.
  /// With `trim` each run of `[ \t\n\r\f]` in the text is one space and text
  /// alone is trimmed.
  fn ss(&mut self, value: &AttrValue, trim: bool) -> Lowered<Expr> {
    let value = if trim { collapsed(value) } else { value.clone() };
    match &value {
      AttrValue::Present(_) => Ok(Expr::Lit(Lit::Bool(true))),
      AttrValue::Parts(parts) => match parts.as_slice() {
        [Part::Text { text }] => Ok(Expr::lit_str(escape_attr(text))),
        [Part::Code(code)] => self.expr(&code.code, code.at),
        parts => {
          let mut pieces = Vec::new();
          for part in parts {
            pieces.push(match part {
              Part::Text { text } => Expr::lit_str(text.clone()),
              Part::Code(code) => Expr::Coalesce(Box::new(self.expr(&code.code, code.at)?), Box::new(Expr::lit_str(String::new()))),
            });
          }
          Ok(Expr::Template(pieces))
        }
      },
    }
  }

  /// A `<textarea>`'s `value` attribute as its content, a leading line break
  /// doubled as the parser would otherwise drop it.
  fn textarea_value(&mut self, value: &AttrValue) -> Lowered<Expr> {
    match value {
      AttrValue::Parts(parts) if matches!(parts.first(), Some(Part::Text { text }) if text.starts_with('\n') || text.starts_with("\r\n")) => {
        let mut parts = parts.clone();
        if let Some(Part::Text { text }) = parts.first_mut() {
          text.insert(0, '\n');
        }
        self.ss(&AttrValue::Parts(parts), false)
      }
      other => self.ss(other, false),
    }
  }

  /// `class={value}`: an expression alone through `clsx`, else as `build_attribute_value` gives it.
  fn class_value(&mut self, value: &AttrValue) -> Lowered<Expr> {
    match value {
      AttrValue::Parts(parts) if matches!(parts.as_slice(), [Part::Code(_)]) => {
        let expr = self.ss(value, true)?;
        Ok(svelte_clsx(expr))
      }
      other => self.ss(other, true),
    }
  }

  /// `to_class(value, hash, directives)`: a directive that holds appends its
  /// name; one that does not removes each whitespace-delimited occurrence of
  /// the name together with the character after it.
  fn class_attr(&mut self, class: Option<Expr>, directives: Vec<(String, Expr)>) -> Lowered<Option<Expr>> {
    if directives.is_empty() {
      return Ok(class);
    }
    let literal = match &class {
      None => Some(String::new()),
      Some(Expr::Lit(Lit::Str(text))) => Some(text.to_string()),
      Some(_) => None,
    };
    if let Some(text) = literal.filter(|text| !directives.iter().any(|(name, _)| text.split_whitespace().any(|word| word == name))) {
      let mut parts: Vec<Entry> = class.filter(|_| !text.is_empty()).map(|c| vec![Entry::Item(c)]).unwrap_or_default();
      for (name, cond) in directives {
        parts.push(Entry::Item(Expr::Ternary(Box::new(cond), Box::new(Expr::lit_str(name)), Box::new(Expr::lit_str(String::new())))));
      }
      return Ok(Some(Expr::Builtin { name: Builtin::ClassNames, args: vec![Expr::Array(parts)] }));
    }
    Ok(Some(to_class(class, directives)))
  }

  /// `to_style(value, directives)`: the attribute's declarations less those
  /// a directive names, then each directive whose value is neither `null`
  /// nor empty, trimmed. A `style` the render computes beside a directive is
  /// not read yet.
  fn style_attr(&mut self, style: Option<Expr>, directives: Vec<StyleDirective>) -> Lowered<Option<Expr>> {
    if directives.is_empty() {
      return Ok(style);
    }
    let reserved: Vec<String> = directives.iter().map(|d| d.name.clone()).collect();
    let kept = match &style {
      None => String::new(),
      Some(Expr::Lit(Lit::Str(text))) if !text.contains("/*") => kept_declarations(text, &reserved),
      Some(_) => return Ok(Some(to_style(style.unwrap(), &reserved, directives))),
    };
    let mut parts = vec![Expr::lit_str(kept)];
    parts.extend(appended_styles(directives));
    Ok(Some(Expr::Builtin { name: Builtin::Trim, args: vec![Expr::Template(parts)] }))
  }
}

/// One entry of the list `build_element_attributes` builds.
enum Item {
  Attr { name: String, value: AttrValue },
  Bound { name: String, value: Expr },
  Spread(Expr),
}

/// What a binding or a `<textarea>`'s value writes in place of the element's content.
enum Body {
  Text(Expr),
  Raw(Expr),
}

/// A `style:` directive. Through `$.attributes` its name is the one written
/// and `important` is dropped, as `build_spread_object` builds one object.
struct StyleDirective {
  name: String,
  value: Expr,
  important: bool,
}

/// The elements whose `onload` and `onerror` Svelte captures before hydration, `is_load_error_element`.
const LOAD_ERROR: &[&str] = &["body", "embed", "iframe", "img", "link", "object", "script", "style", "track"];

/// The bindings Svelte's binding table marks `omit_in_ssr`.
const OMIT_IN_SSR: &[&str] = &["currentTime", "duration", "focused", "paused", "buffered", "seekable", "played", "volume", "muted", "playbackRate", "seeking", "ended", "readyState", "videoHeight", "videoWidth", "naturalWidth", "naturalHeight", "activeElement", "fullscreenElement", "pointerLockElement", "visibilityState", "innerWidth", "innerHeight", "outerWidth", "outerHeight", "scrollX", "scrollY", "online", "devicePixelRatio", "clientWidth", "clientHeight", "offsetWidth", "offsetHeight", "contentRect", "contentBoxSize", "borderBoxSize", "devicePixelContentBoxSize", "indeterminate", "this", "files"];

/// `is_event_attribute`: an `on` name whose value is one expression.
fn is_event(name: &str, value: &AttrValue) -> bool {
  name.starts_with("on") && matches!(value, AttrValue::Parts(parts) if matches!(parts.as_slice(), [Part::Code(_)]))
}

fn static_text(value: &AttrValue) -> Option<&str> {
  match value {
    AttrValue::Parts(parts) => match parts.as_slice() {
      [Part::Text { text }] => Some(text),
      _ => None,
    },
    AttrValue::Present(_) => None,
  }
}

/// `escape_html(value, true)`.
fn escape_attr(text: &str) -> String {
  text.replace('&', "&amp;").replace('"', "&quot;").replace('<', "&lt;")
}

/// `append_styles` for each directive whose value is neither `null` nor `''`:
/// ` name: value;`, the `important` ones after the rest with ` !important;`.
fn appended_styles(directives: Vec<StyleDirective>) -> Vec<Expr> {
  let (important, normal): (Vec<_>, Vec<_>) = directives.into_iter().partition(|d| d.important);
  normal
    .into_iter()
    .chain(important)
    .map(|StyleDirective { name, value, important }| {
      let written = logic(LogicOp::And, cmp(CompareOp::Ne, value.clone(), Expr::Lit(Lit::Null)), cmp(CompareOp::Ne, value.clone(), Expr::lit_str(String::new())));
      let close = if important { " !important;" } else { ";" };
      let declaration = Expr::Template(vec![Expr::lit_str(format!(" {name}: ")), value, Expr::lit_str(close)]);
      Expr::Ternary(Box::new(written), Box::new(declaration), Box::new(Expr::lit_str(String::new())))
    })
    .collect()
}

const ATTRS: &str = "attrs@";
const CLSX: &str = "clsx@";

/// Svelte's `clsx`: an object through `clsx`, anything else as it is with `null` as `''`.
fn svelte_clsx(value: Expr) -> Expr {
  let object = cmp(CompareOp::Eq, call(Builtin::TypeOf, vec![var(CLSX)]), text("object"));
  Expr::Block(vec![
    bind(CLSX, value),
    Stmt::Return(Expr::Ternary(Box::new(object), Box::new(call(Builtin::ClassNames, vec![var(CLSX)])), Box::new(Expr::Coalesce(Box::new(var(CLSX)), Box::new(text("")))))),
  ])
}

/// The object `$.attributes` receives, its `style` through `to_style` when a
/// `style:` directive is present, its `class` through `clsx` when it holds
/// anything and then through `to_class` when a `class:` directive is present.
fn merged_attributes(fields: Vec<Entry>, class_directives: Vec<(String, Expr)>, style_directives: Vec<StyleDirective>) -> Expr {
  let with = |name: &str, value: Expr| Expr::Object(vec![Entry::Spread(var(ATTRS)), Entry::Field(name.to_owned(), value)]);
  let field = |name: &str| var(ATTRS).field(name);
  let mut body = vec![bind(ATTRS, Expr::Object(fields))];
  if !style_directives.is_empty() {
    let reserved: Vec<String> = style_directives.iter().map(|d| css_name(&d.name)).collect();
    body.push(set(ATTRS, with("style", to_style(field("style"), &reserved, style_directives))));
  }
  body.push(when(field("class"), vec![set(ATTRS, with("class", svelte_clsx(field("class"))))], Vec::new()));
  if !class_directives.is_empty() {
    body.push(set(ATTRS, with("class", to_class(Some(field("class")), class_directives))));
  }
  body.push(Stmt::Return(var(ATTRS)));
  Expr::Block(body)
}

/// A `class`, `style` or `style:` value as `build_attribute_value` hands it
/// on with `trim_whitespace`: each run of `[ \t\n\r\f]` in its text is one
/// space and a value that is text alone is trimmed.
fn collapsed(value: &AttrValue) -> AttrValue {
  let collapse = |text: &str| {
    let mut out = String::with_capacity(text.len());
    let mut run = false;
    for c in text.chars() {
      if matches!(c, ' ' | '\t' | '\n' | '\r' | '\u{c}') {
        if !run {
          out.push(' ');
        }
        run = true;
      } else {
        out.push(c);
        run = false;
      }
    }
    out
  };
  match value {
    AttrValue::Parts(parts) => match parts.as_slice() {
      [Part::Text { text }] => AttrValue::Parts(vec![Part::Text { text: collapse(text).trim_matches(js_whitespace).to_owned() }]),
      parts => AttrValue::Parts(parts.iter().map(|part| match part {
        Part::Text { text } => Part::Text { text: collapse(text) },
        code => code.clone(),
      }).collect()),
    },
    other => other.clone(),
  }
}

/// `to_css_name`: a property lowercased unless it is a custom property.
fn css_name(name: &str) -> String {
  match name.starts_with("--") {
    true => name.to_owned(),
    false => name.to_lowercase(),
  }
}

/// The declarations of a literal `style` holding no comment that `to_style`
/// keeps: each `name: value` whose name no directive reserves, written with a
/// leading space and a closing `;`.
fn kept_declarations(text: &str, reserved: &[String]) -> String {
  let chars: Vec<char> = text.trim_matches(js_whitespace).chars().collect();
  let mut out = String::new();
  let (mut in_str, mut depth, mut start, mut name_at) = (None::<char>, 0i32, 0usize, None::<usize>);
  for i in 0..chars.len() {
    let c = chars[i];
    match in_str {
      Some(q) if q == c => in_str = None,
      Some(_) => {}
      None if c == '"' || c == '\'' => in_str = Some(c),
      None if c == '(' => depth += 1,
      None if c == ')' => depth -= 1,
      None => {}
    }
    if in_str.is_none() && depth == 0 {
      if c == ':' && name_at.is_none() {
        name_at = Some(i);
      } else if c == ';' || i == chars.len() - 1 {
        if let Some(n) = name_at {
          let name = css_name(chars[start..n].iter().collect::<String>().trim_matches(js_whitespace));
          if !reserved.contains(&name) {
            let end = if c == ';' { i } else { i + 1 };
            out.push(' ');
            out.push_str(chars[start..end].iter().collect::<String>().trim_matches(js_whitespace));
            out.push(';');
          }
        }
        start = i + 1;
        name_at = None;
      }
    }
  }
  out
}

fn callee_name(call: &js::CallExpr) -> Option<String> {
  let js::Callee::Expr(callee) = &call.callee else { return None };
  match &**callee {
    js::Expr::Ident(id) => Some(id.sym.to_string()),
    js::Expr::Member(m) => match (&*m.obj, &m.prop) {
      (js::Expr::Ident(obj), js::MemberProp::Ident(prop)) => Some(format!("{}.{}", obj.sym, prop.sym)),
      _ => None,
    },
    _ => None,
  }
}

/// `{ a = $bindable(1) }` read as `{ a = 1 }`: on the server a bindable prop is a prop.
fn without_bindable(obj: &js::ObjectPat) -> js::ObjectPat {
  let mut obj = obj.clone();
  for prop in &mut obj.props {
    match prop {
      js::ObjectPatProp::Assign(assign) => {
        if let Some(value) = &mut assign.value {
          unwrap_bindable(value);
        }
      }
      js::ObjectPatProp::KeyValue(kv) => {
        if let js::Pat::Assign(assign) = &mut *kv.value {
          unwrap_bindable(&mut assign.right);
        }
      }
      js::ObjectPatProp::Rest(_) => {}
    }
  }
  obj
}

fn unwrap_bindable(value: &mut Box<js::Expr>) {
  let js::Expr::Call(call) = &**value else { return };
  if callee_name(call).as_deref() != Some("$bindable") {
    return;
  }
  *value = match call.args.first() {
    Some(arg) => arg.expr.clone(),
    None => Box::new(js::Expr::Ident(js::Ident::new_no_ctxt("undefined".into(), call.span))),
  };
}

fn unwrap_types(expr: &js::Expr) -> &js::Expr {
  match expr {
    js::Expr::TsAs(e) => unwrap_types(&e.expr),
    js::Expr::TsSatisfies(e) => unwrap_types(&e.expr),
    js::Expr::TsNonNull(e) => unwrap_types(&e.expr),
    js::Expr::TsTypeAssertion(e) => unwrap_types(&e.expr),
    js::Expr::Paren(e) => unwrap_types(&e.expr),
    other => other,
  }
}

fn is_identifier(text: &str) -> bool {
  let mut chars = text.chars();
  chars.next().is_some_and(|c| c.is_alphabetic() || c == '_' || c == '$') && chars.all(|c| c.is_alphanumeric() || c == '_' || c == '$')
}

/// A Svelte node type as an author writes it.
fn kind_name(kind: &str) -> String {
  match kind {
    "AwaitBlock" => "{#await}".to_owned(),
    "KeyBlock" => "{#key}".to_owned(),
    "SvelteElement" => "<svelte:element>".to_owned(),
    "SvelteComponent" => "<svelte:component>".to_owned(),
    "SvelteSelf" => "<svelte:self>".to_owned(),
    "SvelteFragment" => "<svelte:fragment>".to_owned(),
    "SvelteBoundary" => "<svelte:boundary>".to_owned(),
    "SvelteHead" => "<svelte:head>".to_owned(),
    "SvelteBody" => "<svelte:body>".to_owned(),
    "SvelteWindow" => "<svelte:window>".to_owned(),
    "SvelteDocument" => "<svelte:document>".to_owned(),
    "TitleElement" => "<title>".to_owned(),
    "DeclarationTag" => "a template declaration".to_owned(),
    other => other.to_owned(),
  }
}

fn var(name: &str) -> Expr {
  Expr::var(name)
}

fn int(n: i128) -> Expr {
  Expr::Lit(Lit::Int(n))
}

fn text(s: &str) -> Expr {
  Expr::lit_str(s)
}

fn call(name: Builtin, args: Vec<Expr>) -> Expr {
  Expr::Builtin { name, args }
}

fn cmp(op: CompareOp, a: Expr, b: Expr) -> Expr {
  Expr::Compare(op, Box::new(a), Box::new(b))
}

fn arith(op: ArithOp, a: Expr, b: Expr) -> Expr {
  Expr::Arith(op, Box::new(a), Box::new(b))
}

fn logic(op: LogicOp, a: Expr, b: Expr) -> Expr {
  Expr::Logic(op, Box::new(a), Box::new(b))
}

fn plus(a: Expr, n: i128) -> Expr {
  arith(ArithOp::Add, a, int(n))
}

fn bind(name: &str, expr: Expr) -> Stmt {
  Stmt::Let { name: name.to_owned(), expr }
}

fn set(name: &str, expr: Expr) -> Stmt {
  Stmt::Set { name: name.to_owned(), expr }
}

fn when(cond: Expr, then: Vec<Stmt>, otherwise: Vec<Stmt>) -> Stmt {
  Stmt::If { cond, then, r#else: otherwise }
}

/// `for (const _ of range(n))`.
fn repeat(name: &str, n: Expr, body: Vec<Stmt>) -> Stmt {
  Stmt::ForOf { name: name.to_owned(), over: call(Builtin::Range, vec![n]), body }
}

/// Whether `haystack` holds `needle` at `at`, a UTF-16 index.
fn holds_at(haystack: &str, needle: Expr, at: Expr) -> Expr {
  call(Builtin::StartsWith, vec![var(haystack), needle, at])
}

/// JavaScript's `WhiteSpace` and `LineTerminator`, what `trim` strips.
fn js_whitespace(c: char) -> bool {
  matches!(c, '\t' | '\n' | '\u{b}' | '\u{c}' | '\r' | ' ' | '\u{a0}' | '\u{1680}' | '\u{2000}'..='\u{200a}' | '\u{2028}' | '\u{2029}' | '\u{202f}' | '\u{205f}' | '\u{3000}' | '\u{feff}')
}

const CLASS: &str = "class@";
const AT: &str = "at@";
const END: &str = "end@";

/// The whitespace `to_class` splits on, from `svelte/src/internal/shared/attributes.js`.
const CLASS_WHITESPACE: &[&str] = &[" ", "\t", "\n", "\r", "\u{c}", "\u{a0}", "\u{b}", "\u{feff}"];

/// `to_class` written out in the IR, its `while` a loop over enough
/// iterations for each to remove or step past one occurrence.
fn to_class(class: Option<Expr>, directives: Vec<(String, Expr)>) -> Expr {
  let eq = |a: Expr, b: Expr| cmp(CompareOp::Eq, a, b);
  let space_at = |at: Expr| {
    let whitespace = Expr::Array(CLASS_WHITESPACE.iter().map(|w| Entry::Item(text(w))).collect());
    Expr::Some(Box::new(whitespace), Box::new(Expr::lambda(&["w@"], holds_at(CLASS, var("w@"), at))))
  };
  let start = match class {
    Some(value) => Expr::Ternary(Box::new(eq(value.clone(), Expr::Lit(Lit::Null))), Box::new(text("")), Box::new(Expr::Str(Box::new(value)))),
    None => text(""),
  };
  let mut body = vec![bind(CLASS, start)];
  for (name, cond) in directives {
    let len = name.encode_utf16().count() as i128;
    let append = set(CLASS, Expr::Ternary(Box::new(eq(var(CLASS), text(""))), Box::new(text(&name)), Box::new(Expr::Template(vec![var(CLASS), text(&format!(" {name}"))]))));
    let bounded = logic(
      LogicOp::And,
      logic(LogicOp::Or, eq(var(AT), int(0)), space_at(plus(var(AT), -1))),
      logic(LogicOp::Or, eq(var(END), Expr::Length(Box::new(var(CLASS)))), space_at(var(END))),
    );
    let remove = set(CLASS, Expr::Template(vec![call(Builtin::Substring, vec![var(CLASS), int(0), var(AT)]), call(Builtin::Substring, vec![var(CLASS), plus(var(END), 1)])]));
    let found = cmp(CompareOp::Ge, var(AT), int(0));
    let step = when(
      found.clone(),
      vec![set(AT, call(Builtin::IndexOf, vec![var(CLASS), text(&name), var(AT)])), when(found, vec![bind(END, plus(var(AT), len)), when(bounded, vec![remove], vec![set(AT, var(END))])], Vec::new())],
      Vec::new(),
    );
    let passes = plus(arith(ArithOp::Mul, Expr::Length(Box::new(var(CLASS))), int(2)), 1);
    body.push(when(cond, vec![append], vec![bind(AT, int(0)), repeat("_", passes, vec![step])]));
  }
  body.push(Stmt::Return(Expr::Ternary(Box::new(eq(var(CLASS), text(""))), Box::new(Expr::Lit(Lit::Null)), Box::new(var(CLASS)))));
  Expr::Block(body)
}

const STYLE: &str = "style@";
const OUT: &str = "out@";
const SRC: &str = "src@";
const KEPT: &str = "kept@";
const FROM: &str = "from@";
const OPEN: &str = "open@";
const CLOSE: &str = "close@";
const I: &str = "i@";
const LEN: &str = "len@";
const QUOTE: &str = "quote@";
const DEPTH: &str = "depth@";
const COMMENT: &str = "comment@";
const NAME_AT: &str = "name_at@";
const NAME: &str = "name@";
const START: &str = "start@";

/// `to_style` written out in the IR for a `style` the render computes: the
/// value with `/\/\*.*?\*\//g` removed and trimmed, then each declaration
/// whose name no directive reserves, then the directives.
fn to_style(style: Expr, reserved: &[String], directives: Vec<StyleDirective>) -> Expr {
  let eq = |a: Expr, b: Expr| cmp(CompareOp::Eq, a, b);
  let at = |c: &str| holds_at(STYLE, text(c), var(I));
  let reserved = Expr::Array(reserved.iter().map(|name| Entry::Item(text(name))).collect());
  let line_break = Expr::Some(
    Box::new(Expr::Array(["\n", "\r", "\u{2028}", "\u{2029}"].iter().map(|w| Entry::Item(text(w))).collect())),
    Box::new(Expr::lambda(&["w@"], call(Builtin::Includes, vec![call(Builtin::Substring, vec![var(SRC), plus(var(OPEN), 2), var(CLOSE)]), var("w@")]))),
  );
  let uncomment = vec![
    bind(SRC, Expr::Str(Box::new(var(STYLE)))),
    bind(KEPT, int(0)),
    bind(FROM, int(0)),
    set(STYLE, text("")),
    repeat(
      "_",
      plus(Expr::Length(Box::new(var(SRC))), 2),
      vec![when(
        cmp(CompareOp::Ge, var(KEPT), int(0)),
        vec![
          bind(OPEN, call(Builtin::IndexOf, vec![var(SRC), text("/*"), var(FROM)])),
          when(
            cmp(CompareOp::Lt, var(OPEN), int(0)),
            vec![set(STYLE, Expr::Template(vec![var(STYLE), call(Builtin::Substring, vec![var(SRC), var(KEPT)])])), set(KEPT, int(-1))],
            vec![
              bind(CLOSE, call(Builtin::IndexOf, vec![var(SRC), text("*/"), plus(var(OPEN), 2)])),
              when(
                logic(LogicOp::And, cmp(CompareOp::Ge, var(CLOSE), int(0)), Expr::Not(Box::new(line_break))),
                vec![set(STYLE, Expr::Template(vec![var(STYLE), call(Builtin::Substring, vec![var(SRC), var(KEPT), var(OPEN)])])), set(KEPT, plus(var(CLOSE), 2)), set(FROM, plus(var(CLOSE), 2))],
                vec![set(FROM, plus(var(OPEN), 1))],
              ),
            ],
          ),
        ],
        Vec::new(),
      )],
    ),
    set(STYLE, call(Builtin::Trim, vec![var(STYLE)])),
  ];
  let track = when(
    var(COMMENT),
    vec![when(logic(LogicOp::And, at("/"), logic(LogicOp::And, cmp(CompareOp::Gt, var(I), int(0)), holds_at(STYLE, text("*"), plus(var(I), -1)))), vec![set(COMMENT, Expr::Lit(Lit::Bool(false)))], Vec::new())],
    vec![when(
      cmp(CompareOp::Ne, var(QUOTE), text("")),
      vec![when(holds_at(STYLE, var(QUOTE), var(I)), vec![set(QUOTE, text(""))], Vec::new())],
      vec![when(
        logic(LogicOp::And, at("/"), holds_at(STYLE, text("*"), plus(var(I), 1))),
        vec![set(COMMENT, Expr::Lit(Lit::Bool(true)))],
        vec![when(
          at("\""),
          vec![set(QUOTE, text("\""))],
          vec![when(at("'"), vec![set(QUOTE, text("'"))], vec![when(at("("), vec![set(DEPTH, plus(var(DEPTH), 1))], vec![when(at(")"), vec![set(DEPTH, plus(var(DEPTH), -1))], Vec::new())])])],
        )],
      )],
    )],
  );
  let declaration = vec![
    bind(NAME, call(Builtin::Trim, vec![call(Builtin::Substring, vec![var(STYLE), var(START), var(NAME_AT)])])),
    set(NAME, Expr::Ternary(Box::new(call(Builtin::StartsWith, vec![var(NAME), text("--")])), Box::new(var(NAME)), Box::new(call(Builtin::Lower, vec![var(NAME)])))),
    when(
      Expr::Not(Box::new(call(Builtin::Includes, vec![reserved, var(NAME)]))),
      vec![set(OUT, Expr::Template(vec![var(OUT), text(" "), call(Builtin::Trim, vec![call(Builtin::Substring, vec![var(STYLE), var(START), Expr::Ternary(Box::new(at(";")), Box::new(var(I)), Box::new(plus(var(I), 1)))])]), text(";")]))],
      Vec::new(),
    ),
  ];
  let split = when(
    logic(LogicOp::And, Expr::Not(Box::new(var(COMMENT))), logic(LogicOp::And, eq(var(QUOTE), text("")), eq(var(DEPTH), int(0)))),
    vec![when(
      logic(LogicOp::And, at(":"), eq(var(NAME_AT), int(-1))),
      vec![set(NAME_AT, var(I))],
      vec![when(
        logic(LogicOp::Or, at(";"), eq(var(I), plus(var(LEN), -1))),
        vec![when(cmp(CompareOp::Ne, var(NAME_AT), int(-1)), declaration, Vec::new()), set(START, plus(var(I), 1)), set(NAME_AT, int(-1))],
        Vec::new(),
      )],
    )],
    Vec::new(),
  );
  let mut scan = uncomment;
  scan.extend([
    bind(QUOTE, text("")),
    bind(DEPTH, int(0)),
    bind(COMMENT, Expr::Lit(Lit::Bool(false))),
    bind(START, int(0)),
    bind(NAME_AT, int(-1)),
    bind(LEN, Expr::Length(Box::new(var(STYLE)))),
    repeat(I, var(LEN), vec![track, split]),
  ]);
  let mut tail = vec![var(OUT)];
  tail.extend(appended_styles(directives));
  let body = vec![
    bind(STYLE, style),
    bind(OUT, text("")),
    when(var(STYLE), scan, Vec::new()),
    set(OUT, call(Builtin::Trim, vec![Expr::Template(tail)])),
    Stmt::Return(Expr::Ternary(Box::new(eq(var(OUT), text(""))), Box::new(Expr::Lit(Lit::Null)), Box::new(var(OUT)))),
  ];
  Expr::Block(body)
}
