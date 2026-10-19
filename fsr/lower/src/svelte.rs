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
use snapfire_fsr_ir::render::{CHOSEN_TAG, CONTEXT_PREFIX, RAW_ATTR, SLOTS_GIVEN, SLOT_CONTENT_PREFIX, SLOT_OUT_PREFIX, SLOT_PROPS, SVELTE_FLAGS, SVELTE_SELECTED};
use snapfire_fsr_ir::Owner;
use swc_core::common::Spanned;
use swc_core::ecma::ast as js;
use swc_core::ecma::visit::{VisitMut, VisitMutWith, VisitWith};

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
  /// `read` when the plugin read which elements Svelte's CSS scopes, `unread` when it could not.
  #[serde(default)]
  pub scoping: Option<String>,
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
pub struct Declarator {
  pub id: Code,
  pub init: Option<Code>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct SlotGroup {
  pub name: String,
  pub fragment: Fragment,
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
  /// `scoped` is the `svelte-` class Svelte's CSS gives the element, when it gives one.
  Element { name: String, svg: bool, #[serde(default)] mathml: bool, #[serde(default)] scoped: Option<String>, attrs: Vec<Attr>, fragment: Fragment, at: (usize, usize) },
  /// `slots` groups the children as `build_inline_component` does: by their
  /// static `slot` attribute, snippets aside, each group cleaned on its own.
  Component { name: String, dynamic: bool, #[serde(default)] svg: bool, attrs: Vec<Attr>, fragment: Fragment, #[serde(default)] slots: Vec<SlotGroup>, at: (usize, usize) },
  Slot { attrs: Vec<Attr>, fragment: Fragment, at: (usize, usize) },
  If { elseif: bool, test: Code, consequent: Fragment, alternate: Option<Fragment>, at: (usize, usize) },
  Each { expression: Code, context: Option<Code>, index: Option<String>, key: Option<Code>, body: Fragment, fallback: Option<Fragment>, at: (usize, usize) },
  Snippet { name: String, params: Vec<Code>, body: Fragment, at: (usize, usize) },
  Key { expression: Code, fragment: Fragment, at: (usize, usize) },
  /// `{let x = …}` or `{const x = …}` in the template, each declarator.
  Declare { items: Vec<Declarator>, at: (usize, usize) },
  Await { expression: Code, value: Option<Code>, error: Option<Code>, pending: Option<Fragment>, then: Option<Fragment>, catch: Option<Fragment>, at: (usize, usize) },
  /// A `svelte:` element or `<title>`: `kind` is Svelte's node type and `this`
  /// the expression `svelte:element` and `svelte:component` take.
  Special { kind: String, name: String, #[serde(default)] scoped: Option<String>, attrs: Vec<Attr>, this: Option<Code>, fragment: Fragment, #[serde(default)] slots: Vec<SlotGroup>, at: (usize, usize) },
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
  /// Each name a prop is bound to and the prop's key, so a prop holding a
  /// snippet renders and forwards the caller's content for that slot.
  prop_keys: Vec<(String, String)>,
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
    Self { lowerer, described, template: None, lets: Vec::new(), state: Vec::new(), stores: Vec::new(), snippets: Vec::new(), rendering: Vec::new(), child_refs: Vec::new(), assets, heads: Vec::new(), prop_keys: Vec::new() }
  }

  pub(crate) fn component(&mut self) -> Lowered<Component> {
    let Some(value) = &self.described.template else { return Err(self.at(1, 1, "the plugin described no template")) };
    let template: Template = serde_json::from_value(value.clone()).map_err(|e| self.at(1, 1, format!("the template description did not read: {e}")))?;
    let runes = template.runes;
    let ranges = (template.module, template.instance);
    if template.scoping.as_deref() == Some("unread") {
      return Err(self.at(1, 1, "a `<style>` whose scoping the build could not read from Svelte's compiler"));
    }
    let fragment = template.fragment.clone();
    self.template = Some(template);
    self.lets.push(Stmt::Let { name: PROPS.to_owned(), expr: props_with_slots() });
    let reads_slots = value.to_string().contains("$$slots") || self.described.script.as_ref().is_some_and(|s| s.content.contains("$$slots"));
    if reads_slots {
      let filled = Expr::lambda(&["e@"], cmp(CompareOp::Eq, Expr::Index(Box::new(var("e@")), Box::new(int(1))), Expr::Lit(Lit::Bool(true))));
      self.lets.push(Stmt::Let { name: SLOTS.to_owned(), expr: call(Builtin::FromEntries, vec![Expr::Filter(Box::new(Expr::Entries(Box::new(var(SLOTS_GIVEN)))), Box::new(filled))]) });
      self.bind("$$slots".to_owned(), var(SLOTS));
    }
    self.script(runes, ranges)?;
    if !runes && reads_props_whole(value, self.described.script.as_ref().map(|s| s.content.as_str())) {
      let mut internal = vec![Expr::Var("$props".to_owned())];
      internal.extend(["children", "$$slots", "$$events", "$$legacy"].iter().map(|k| text(k)));
      let mut rest = internal.clone();
      rest.extend(self.prop_keys.iter().map(|(_, key)| text(key)));
      self.bind("$$props".to_owned(), call(Builtin::Omit, internal));
      self.bind("$$restProps".to_owned(), call(Builtin::Omit, rest));
    }
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
    let mut parsed = self.lowerer.parsed.parse_expr_at(code, line, column).map_err(|message| self.at(line, column, format!("`{}`: {message}", code.trim())))?;
    if let Some(top) = self.lowerer.meanings.as_ref().map(|meanings| meanings.top) {
      let local: Vec<String> = self.lowerer.scope.iter().map(|(name, _)| name.clone()).collect();
      parsed.visit_mut_with(&mut TopLevel { top, local });
    }
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
        js::ModuleItem::ModuleDecl(js::ModuleDecl::Import(import)) => {
          if import.src.value.to_atom_lossy().ends_with(".svelte") {
            for specifier in &import.specifiers {
              if let js::ImportSpecifier::Default(default) = specifier {
                let local = default.local.sym.to_string();
                self.bind(local.clone(), call(Builtin::ComponentRef, vec![text(&local)]));
              }
            }
          }
        }
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
      js::Stmt::Decl(js::Decl::Fn(f)) => {
        self.function(f);
        Ok(())
      }
      js::Stmt::Decl(js::Decl::TsInterface(_) | js::Decl::TsTypeAlias(_)) => Ok(()),
      js::Stmt::Expr(e) => match &*e.expr {
        js::Expr::Call(call) if callee_name(call).is_some_and(|name| BROWSER_CALLS.contains(&name.as_str())) || inspects(call) => Ok(()),
        js::Expr::Call(call) if subscribed(call).is_some() => self.subscription(call),
        _ => Err(self.statement_residue(e.span)),
      },
      js::Stmt::Labeled(labeled) if !runes && labeled.label.sym == "$" => self.reactive(labeled),
      js::Stmt::Empty(_) => Ok(()),
      other => Err(self.statement_residue(other.span())),
    }
  }

  /// A function the script declares, as a function value the template and
  /// the script call in the component's scope. One whose body the build
  /// cannot lower (or that is `async` and so answers a promise) is the
  /// browser's: read as a value it is a function that renders nothing, which
  /// is all a handler passed to a child is on the server.
  fn function(&mut self, f: &'p js::FnDecl) {
    let name = f.ident.sym.to_string();
    if f.function.is_async || f.function.is_generator {
      self.lowerer.browser_functions.push(name);
      return;
    }
    let value = js::Expr::Fn(js::FnExpr { ident: None, function: f.function.clone() });
    let before = self.lowerer.scope.len();
    let lowered = self.lowerer.expr(&value);
    self.lowerer.scope.truncate(before);
    match lowered {
      Ok(lambda) => {
        self.lets.push(Stmt::Let { name: name.clone(), expr: lambda });
        self.bind(name.clone(), Expr::Var(name));
      }
      Err(_) => {
        self.lowerer.unbound = None;
        self.lowerer.browser_functions.push(name);
      }
    }
  }

  /// Whether a prop's expression is a function written out, which on the
  /// server is a handler the child runs in the browser.
  fn is_function(&self, code: &Code) -> bool {
    self.lowerer.parsed.parse_expr_at(&code.code, code.at.0, code.at.1).is_ok_and(|e| matches!(unwrap_types(&e), js::Expr::Arrow(_) | js::Expr::Fn(_)))
  }

  /// `store.subscribe(fn)` at the top of a script: Svelte's server calls `fn`
  /// with the store's value as it subscribes, so its writes hold for the render.
  fn subscription(&mut self, call: &'p js::CallExpr) -> Lowered<()> {
    let (store, callback) = subscribed(call).expect("checked by the caller");
    let value = self.lowerer.expr(store)?;
    let (params, body): (Vec<&js::Pat>, Vec<js::Stmt>) = match unwrap_types(callback) {
      js::Expr::Arrow(arrow) => (arrow.params.iter().collect(), match &*arrow.body {
        js::ArrowFunctionBody::FunctionBody(block) => block.stmts.clone(),
        js::ArrowFunctionBody::Expr(e) => vec![js::Stmt::Expr(js::ExprStmt { span: e.span(), expr: e.clone() })],
      }),
      js::Expr::Fn(f) => (f.function.params.iter().map(|p| &p.pat).collect(), f.function.body.as_ref().map(|b| b.stmts.clone()).unwrap_or_default()),
      _ => return Err(self.lowerer.residue(call.span, "a `subscribe` callback that is not a function written out")),
    };
    let scope_before = self.lowerer.scope.len();
    if let Some(js::Pat::Ident(param)) = params.first() {
      let name = param.id.sym.to_string();
      self.lets.push(Stmt::Let { name: name.clone(), expr: value });
      self.bind(name.clone(), Expr::Var(name));
    }
    let lowered = self.lowerer.block(&body);
    self.lowerer.scope.truncate(scope_before);
    let ran = self.lowerer.temp();
    self.lets.push(Stmt::Let { name: ran, expr: Expr::Block(lowered?) });
    Ok(())
  }

  fn statement_residue(&self, span: swc_core::common::Span) -> Residue {
    self.lowerer.residue_with(span, "a statement in a Svelte script the build cannot follow", "a script the build reads is declarations, runes, `useStore` and the calls the browser alone runs: `$effect`, `$inspect`, `onMount` and the other lifecycle functions")
  }

  /// `$:` runs where it is written on the server: `$: name = expr` binds the
  /// name and a destructuring assignment each name it writes; any other
  /// statement runs as a block whose writes reach the names declared before
  /// it. A name a `$:` assigns that nothing declares is declared first, as
  /// Svelte does.
  fn reactive(&mut self, labeled: &'p js::LabeledStmt) -> Lowered<()> {
    if let js::Stmt::Expr(e) = &*labeled.body {
      if let js::Expr::Assign(assign) = &*e.expr {
        if let (Some(js::SimpleAssignTarget::Ident(target)), js::AssignOp::Assign) = (assign.left.as_simple(), assign.op) {
          let name = target.id.sym.to_string();
          let expr = self.lowerer.expr(&assign.right)?;
          self.lets.push(Stmt::Let { name: name.clone(), expr });
          self.bind(format!("${name}"), Expr::Var(name.clone()));
          self.bind(name.clone(), Expr::Var(name));
          return Ok(());
        }
      }
    }
    if let js::Stmt::Expr(e) = &*labeled.body {
      if let js::Expr::Assign(assign) = unwrap_types(&e.expr) {
        if let (js::AssignTarget::Pat(target), js::AssignOp::Assign) = (&assign.left, assign.op) {
          let holder = self.lowerer.temp();
          let value = self.lowerer.expr(&assign.right)?;
          self.lets.push(Stmt::Let { name: holder.clone(), expr: value });
          let pat: js::Pat = target.clone().into();
          let before = self.lowerer.scope.len();
          crate::component::bind_pattern(&mut self.lowerer, &pat, Expr::Var(holder))?;
          let bound: Vec<(String, Expr)> = self.lowerer.scope.drain(before..).collect();
          for (name, expr) in bound {
            self.lets.push(Stmt::Let { name: name.clone(), expr });
            self.bind(format!("${name}"), Expr::Var(name.clone()));
            self.bind(name.clone(), Expr::Var(name));
          }
          return Ok(());
        }
      }
    }
    let mut assigned = Vec::new();
    assigned_names(&labeled.body, &mut assigned);
    for name in assigned {
      if !self.lowerer.scope.iter().any(|(bound, _)| *bound == name) {
        self.lets.push(Stmt::Let { name: name.clone(), expr: Expr::Lit(Lit::Null) });
        self.bind(format!("${name}"), Expr::Var(name.clone()));
        self.bind(name.clone(), Expr::Var(name));
      }
    }
    let body = match &*labeled.body {
      js::Stmt::Block(block) => self.lowerer.block(&block.stmts)?,
      stmt => self.lowerer.block(std::slice::from_ref(stmt))?,
    };
    let ran = self.lowerer.temp();
    self.lets.push(Stmt::Let { name: ran, expr: Expr::Block(body) });
    Ok(())
  }

  /// `export let a = 1` in Svelte 4: the prop, else its default.
  fn export_lets(&mut self, var: &'p js::VarDecl) -> Lowered<()> {
    for decl in &var.decls {
      let js::Pat::Ident(name) = &decl.name else { return Err(self.lowerer.residue(decl.span, "an exported prop that is not a name")) };
      let name = name.id.sym.to_string();
      let read = Expr::var(PROPS).field(name.clone());
      self.prop_keys.push((name.clone(), name.clone()));
      let expr = match &decl.init {
        Some(init) => Expr::Coalesce(Box::new(read), Box::new(self.lowerer.expr(init)?)),
        None => read,
      };
      self.lets.push(Stmt::Let { name: name.clone(), expr });
      self.bind(format!("${name}"), Expr::Var(name.clone()));
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
          let rest = obj.props.iter().any(|p| matches!(p, js::ObjectPatProp::Rest(_)));
          for prop in &obj.props {
            match prop {
              js::ObjectPatProp::Assign(a) => self.prop_keys.push((a.key.id.sym.to_string(), a.key.id.sym.to_string())),
              js::ObjectPatProp::KeyValue(kv) => {
                if let (Some(key), js::Pat::Ident(local)) = (crate::prop_name(&kv.key), unwrap_assign(&kv.value)) {
                  self.prop_keys.push((local.id.sym.to_string(), key));
                }
              }
              js::ObjectPatProp::Rest(_) => {}
            }
          }
          bind_object(&mut self.lowerer, &obj, if rest { Expr::Var("$props".to_owned()) } else { Expr::var(PROPS) })?;
          let bound: Vec<String> = self.prop_keys.iter().map(|(local, _)| local.clone()).collect();
          for local in bound {
            if let Some(read) = self.lowerer.scope.iter().rev().find(|(n, _)| *n == local).map(|(_, e)| e.clone()) {
              self.bind(format!("${local}"), read);
            }
          }
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
              Some(f) => Expr::Apply { f: Box::new(self.lowerer.expr(f)?), args: Vec::new() },
              None => return Err(self.lowerer.residue(call.span, "`$derived.by` without a function")),
            },
            "$state.snapshot" => match arg {
              Some(arg) => self.lowerer.expr(arg)?,
              None => Expr::Lit(Lit::Null),
            },
            "$effect.tracking" => Expr::Lit(Lit::Bool(false)),
            "$props.id" => return Err(self.lowerer.residue(call.span, "`$props.id()`, whose ids Svelte counts in render order, which the build does not count yet")),
            other => return Err(self.lowerer.residue(call.span, format!("`{other}`, a rune the build does not read"))),
          };
          self.lets.push(Stmt::Let { name: name.clone(), expr });
          if rune.starts_with("$state") {
            self.state.push(name.clone());
          }
          self.bind(name.clone(), Expr::Var(name));
        }
        (_, js::Pat::Ident(name)) => self.binding(name.id.sym.to_string(), init, decl.span, runes)?,
        (_, pattern) => {
          if matches!(unwrap_types(init), js::Expr::Arrow(_) | js::Expr::Fn(_)) {
            continue;
          }
          let holder = self.lowerer.temp();
          let expr = self.lowerer.expr(init)?;
          self.lets.push(Stmt::Let { name: holder.clone(), expr });
          let before = self.lowerer.scope.len();
          crate::component::bind_pattern(&mut self.lowerer, pattern, Expr::Var(holder))?;
          let bound: Vec<(String, Expr)> = self.lowerer.scope.drain(before..).collect();
          for (name, expr) in bound {
            self.lets.push(Stmt::Let { name: name.clone(), expr });
            if !runes {
              self.state.push(name.clone());
            }
            self.bind(format!("${name}"), Expr::Var(name.clone()));
            self.bind(name.clone(), Expr::Var(name));
          }
        }
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
    self.bind(format!("${name}"), Expr::Var(name.clone()));
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
    let legacy = self.template.as_ref().is_some_and(|t| !t.runes);
    let hoisted = match legacy {
      true => self.sorted_consts(&fragment.hoisted),
      false => fragment.hoisted.clone(),
    };
    for node in &hoisted {
      match node {
        Node::Snippet { name, params, body, .. } => self.snippets.push(Snippet { name: name.clone(), params: params.clone(), body: body.clone() }),
        Node::Const { id, init, .. } => {
          let holder = self.holder(id);
          let expr = self.expr(&init.code, init.at)?;
          consts.push((holder.clone(), expr));
          self.pattern(id, &holder)?;
        }
        Node::Declare { items, .. } => {
          for item in items {
            let holder = self.holder(&item.id);
            let expr = match &item.init {
              Some(init) => self.expr(&init.code, init.at)?,
              None => Expr::Lit(Lit::Null),
            };
            consts.push((holder.clone(), expr));
            self.pattern(&item.id, &holder)?;
          }
        }
        Node::Other { kind, at } if kind == "DebugTag" => {
          let _ = at;
        }
        Node::Special { kind, .. } if matches!(kind.as_str(), "SvelteWindow" | "SvelteBody" | "SvelteDocument") => {}
        Node::Special { kind, at, .. } if kind == "SvelteHead" || kind == "TitleElement" => {
          return Err(self.at(at.0, at.1, format!("`{}` in markup the browser hydrates; what goes in the document head belongs in a loader's `meta` export", kind_name(kind))));
        }
        Node::Other { kind, at } | Node::Special { kind, at, .. } => return Err(self.at(at.0, at.1, format!("`{}`, which the build does not render on the server", kind_name(kind)))),
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
      Node::Html { code, at } => {
        out.push(Tmpl::Marker(EMPTY.to_owned()));
        out.push(Tmpl::Html(self.expr(code, *at)?));
        out.push(Tmpl::Marker(EMPTY.to_owned()));
      }
      Node::Element { .. } => out.push(self.element(node)?),
      Node::Component { .. } => self.component_node(node, parent.standalone, out)?,
      Node::Render { .. } => self.render_tag(node, parent.standalone, out)?,
      Node::If { .. } => self.if_block(node, out)?,
      Node::Each { .. } => self.each_block(node, out)?,
      Node::Slot { attrs, fragment, .. } => self.slot_element(attrs, fragment, out)?,
      Node::Special { kind, fragment, .. } if kind == "SvelteFragment" => out.extend(self.fragment(fragment)?),
      Node::Special { kind, fragment, .. } if kind == "SvelteBoundary" => self.boundary(fragment, out)?,
      Node::Special { kind, attrs, scoped, this: Some(this), fragment, at, .. } if kind == "SvelteElement" => self.dynamic_element(attrs, scoped, this, fragment, *at, out)?,
      Node::Special { kind, attrs, this: Some(this), fragment, slots, .. } if kind == "SvelteComponent" => {
        let choice = self.expr(&this.code, this.at)?;
        let (props, children, css) = self.component_parts(attrs, fragment, slots)?;
        let placed = chosen_component(choice, props, children);
        match css.is_empty() {
          true => out.push(placed),
          false => out.extend(css_props(false, css, placed, true)),
        }
      }
      Node::Special { kind, attrs, fragment, slots, at, .. } if kind == "SvelteSelf" => {
        let (props, children, _) = self.component_parts(attrs, fragment, slots)?;
        self.child_refs.push(ChildRef { local: SELF.to_owned(), attrs: Vec::new(), line: at.0, column: at.1 });
        out.push(Tmpl::Component { module: SELF.to_owned(), props, children, id: 0, keyed: false });
        if !parent.standalone {
          out.push(Tmpl::Marker(EMPTY.to_owned()));
        }
      }
      Node::Const { at, .. } | Node::Snippet { at, .. } | Node::Declare { at, .. } => return Err(self.at(at.0, at.1, "a declaration among the nodes")),
      Node::Other { kind, at } | Node::Special { kind, at, .. } => return Err(self.at(at.0, at.1, format!("`{}`, which the build does not render on the server", kind_name(kind)))),
      Node::Key { fragment, .. } => {
        out.push(Tmpl::Marker(EMPTY.to_owned()));
        out.extend(self.fragment(fragment)?);
        out.push(Tmpl::Marker(EMPTY.to_owned()));
      }
      Node::Await { expression, value, pending, then, .. } => self.await_block(expression, value.as_ref(), pending.as_ref(), then.as_ref(), out)?,
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
      Some(context) => self.holder(context),
      None => self.lowerer.temp(),
    };
    let index_name = index.clone().unwrap_or_else(|| self.lowerer.temp());
    self.bind(item.clone(), Expr::Var(item.clone()));
    self.bind(index_name.clone(), Expr::Var(index_name.clone()));
    if let Some(context) = context {
      self.pattern(context, &item)?;
    }
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
      let Some(key) = self.prop_keys.iter().rev().find(|(local, _)| local == name).map(|(_, key)| key.clone()) else {
        return Err(self.at(at.0, at.1, format!("`{{@render {name}()}}` of something other than a prop or a snippet the template declares")));
      };
      let mut given = Vec::new();
      for arg in args {
        given.push(Entry::Item(self.expr(&arg.code, arg.at)?));
      }
      out.push(Tmpl::Let { name: format!("{SLOT_OUT_PREFIX}{}", slot_name(&key)), expr: Expr::Array(given), then: Box::new(Tmpl::Fragment(Vec::new())) });
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
    for (i, _) in snippet.params.iter().enumerate() {
      let value = match args.get(i) {
        Some(arg) => self.expr(&arg.code, arg.at)?,
        None => Expr::Lit(Lit::Null),
      };
      bound.push(value);
    }
    let scope_before = self.lowerer.scope.len();
    let mut holders = Vec::new();
    for param in &snippet.params {
      let holder = self.holder(param);
      self.pattern(param, &holder)?;
      holders.push(holder);
    }
    let bound: Vec<(String, Expr)> = holders.into_iter().zip(bound).collect();
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

  /// `{#await}`, `$.await`: a value the render holds is never a promise, so
  /// the `then` branch with the value bound, after the else marker. An
  /// expression the build cannot lower is taken for the promise the block
  /// is written for, which Svelte's server writes as its pending branch.
  fn await_block(&mut self, expression: &Code, value: Option<&Code>, pending: Option<&Fragment>, then: Option<&Fragment>, out: &mut Vec<Tmpl>) -> Lowered<()> {
    let awaited = match self.expr(&expression.code, expression.at) {
      Ok(awaited) => awaited,
      Err(_) => {
        self.lowerer.unbound = None;
        out.push(Tmpl::Marker(BLOCK_OPEN.to_owned()));
        if let Some(pending) = pending {
          out.extend(self.fragment(pending)?);
        }
        out.push(Tmpl::Marker(BLOCK_CLOSE.to_owned()));
        return Ok(());
      }
    };
    out.push(Tmpl::Marker(BLOCK_OPEN_ELSE.to_owned()));
    if let Some(then) = then {
      let scope_before = self.lowerer.scope.len();
      let name = match value {
        Some(code) => {
          let holder = self.holder(code);
          self.pattern(code, &holder)?;
          Some(holder)
        }
        None => None,
      };
      let content = self.fragment(then);
      self.lowerer.scope.truncate(scope_before);
      let content = Tmpl::Fragment(content?);
      out.push(match name {
        Some(name) => Tmpl::Let { name, expr: awaited, then: Box::new(content) },
        None => content,
      });
    }
    out.push(Tmpl::Marker(BLOCK_CLOSE.to_owned()));
    Ok(())
  }

  /// `<svelte:boundary>`: its `pending` snippet after the else marker when it
  /// has one, else its content, between block markers. A `failed` snippet
  /// renders only when the content throws, which a lowered render does not
  /// recover from.
  fn boundary(&mut self, fragment: &Fragment, out: &mut Vec<Tmpl>) -> Lowered<()> {
    let pending = fragment.hoisted.iter().find_map(|node| match node {
      Node::Snippet { name, params, body, .. } if name == "pending" => Some(Snippet { name: name.clone(), params: params.clone(), body: body.clone() }),
      _ => None,
    });
    match pending {
      Some(pending) => {
        out.push(Tmpl::Marker(BLOCK_OPEN_ELSE.to_owned()));
        out.push(self.snippet_content(&pending)?);
      }
      None => {
        out.push(Tmpl::Marker(BLOCK_OPEN.to_owned()));
        out.extend(self.fragment(fragment)?);
      }
    }
    out.push(Tmpl::Marker(BLOCK_CLOSE.to_owned()));
    Ok(())
  }

  /// `sort_const_tags` in Svelte 4 mode: each `{@const}` after the ones its
  /// value reads, the other hoisted nodes where they were.
  fn sorted_consts(&self, hoisted: &[Node]) -> Vec<Node> {
    let consts: Vec<(usize, Vec<String>, Vec<String>)> = hoisted
      .iter()
      .enumerate()
      .filter_map(|(i, node)| match node {
        Node::Const { id, init, .. } => Some((i, self.names_in(&format!("({}) => 0", id.code), true), self.names_in(&init.code, false))),
        _ => None,
      })
      .collect();
    let mut placed: Vec<usize> = Vec::new();
    let mut pending: Vec<&(usize, Vec<String>, Vec<String>)> = consts.iter().collect();
    while !pending.is_empty() {
      let ready = pending.iter().position(|(_, _, reads)| !reads.iter().any(|read| pending.iter().any(|(_, declares, _)| declares.contains(read)))).unwrap_or(0);
      placed.push(pending.remove(ready).0);
    }
    let mut order = placed.into_iter();
    hoisted.iter().map(|node| match node {
      Node::Const { .. } => hoisted[order.next().expect("a const for each const")].clone(),
      other => other.clone(),
    }).collect()
  }

  /// The names an expression reads (with `declared` the names an arrow's parameters bind).
  fn names_in(&self, code: &str, declared: bool) -> Vec<String> {
    struct Names(Vec<String>, bool);
    impl swc_core::ecma::visit::Visit for Names {
      fn visit_ident(&mut self, id: &js::Ident) {
        if !self.1 {
          self.0.push(id.sym.to_string());
        }
      }
      fn visit_binding_ident(&mut self, id: &js::BindingIdent) {
        if self.1 {
          self.0.push(id.id.sym.to_string());
        }
      }
    }
    let Ok(parsed) = self.lowerer.parsed.parse_expr_at(code, 1, 1) else { return Vec::new() };
    let mut names = Names(Vec::new(), declared);
    match (&*parsed, declared) {
      (js::Expr::Arrow(arrow), true) => arrow.params.visit_with(&mut names),
      (expr, _) => expr.visit_with(&mut names),
    }
    names.0
  }

  /// A pattern the template binds, `{ a, b: [c] = d }` or a name, bound to
  /// the parts of `value`, which is read once: a name is bound to it and a
  /// pattern to the parts of the variable `holder` it is held in.
  fn pattern(&mut self, code: &Code, holder: &str) -> Lowered<()> {
    let text = code.code.trim();
    if is_identifier(text) {
      self.bind(text.to_owned(), Expr::Var(holder.to_owned()));
      return Ok(());
    }
    let (line, column) = code.at;
    let wrapped = format!("({text}) => 0");
    let parsed = self.lowerer.parsed.parse_expr_at(&wrapped, line, column.saturating_sub(1).max(1)).map_err(|message| self.at(line, column, format!("`{text}`: {message}")))?;
    let js::Expr::Arrow(arrow) = &*parsed else { return Err(self.at(line, column, format!("`{text}`, a pattern the build does not read"))) };
    let Some(pat) = arrow.params.first() else { return Err(self.at(line, column, format!("`{text}`, a pattern the build does not read"))) };
    crate::component::bind_pattern(&mut self.lowerer, pat, Expr::Var(holder.to_owned()))
  }

  /// The variable a template binding is held in: its own name (a fresh one for a pattern).
  fn holder(&mut self, code: &Code) -> String {
    let text = code.code.trim();
    if is_identifier(text) { text.to_owned() } else { self.lowerer.temp() }
  }

  /// `<slot name="…" {props}>fallback</slot>`, `$.slot` between block
  /// markers: the caller's content for the slot with the props as its one
  /// argument, else the fallback. A snippet passed as an attribute is a prop
  /// and fills no `<slot>`.
  fn slot_element(&mut self, attrs: &[Attr], fragment: &Fragment, out: &mut Vec<Tmpl>) -> Lowered<()> {
    let mut slot = "default".to_owned();
    let mut given = Vec::new();
    for attr in attrs {
      match attr {
        Attr::Attr { name, value, .. } if name == "name" => {
          if let Some(text) = static_text(value) {
            slot = text.to_owned();
          }
        }
        Attr::Attr { name, value, .. } => given.push(Entry::Field(name.clone(), self.attr_value(value, false)?)),
        Attr::Spread { code, at } => given.push(Entry::Spread(self.expr(code, *at)?)),
        Attr::Attach { .. } | Attr::Directive { .. } => {}
      }
    }
    let fallback = Tmpl::Fragment(self.fragment(fragment)?);
    let filled = Tmpl::Let { name: format!("{SLOT_OUT_PREFIX}{slot}"), expr: Expr::Array(vec![Entry::Item(Expr::Object(given))]), then: Box::new(fallback.clone()) };
    out.push(Tmpl::Marker(BLOCK_OPEN.to_owned()));
    out.push(Tmpl::If { cond: cmp(CompareOp::Eq, var(SLOTS_GIVEN).field(slot), text("prop")), then: Box::new(fallback), r#else: Some(Box::new(filled)) });
    out.push(Tmpl::Marker(BLOCK_CLOSE.to_owned()));
    Ok(())
  }

  /// A snippet as the caller's content for a slot: its parameters read from
  /// the arguments the callee renders it with.
  fn snippet_content(&mut self, snippet: &Snippet) -> Lowered<Tmpl> {
    if self.rendering.contains(&snippet.name) {
      return Err(self.at(1, 1, format!("`{}`, a snippet that renders itself", snippet.name)));
    }
    let scope_before = self.lowerer.scope.len();
    let mut params = Vec::new();
    for (i, param) in snippet.params.iter().enumerate() {
      let holder = self.holder(param);
      self.pattern(param, &holder)?;
      params.push((holder, Expr::Index(Box::new(var(SLOT_PROPS)), Box::new(int(i as i128)))));
    }
    self.rendering.push(snippet.name.clone());
    let body = self.fragment(&snippet.body);
    self.rendering.pop();
    self.lowerer.scope.truncate(scope_before);
    let mut tree = Tmpl::Fragment(body?);
    for (name, expr) in params.into_iter().rev() {
      tree = Tmpl::Let { name, expr, then: Box::new(tree) };
    }
    Ok(tree)
  }

  /// `let:name` and `let:name={local}`: the name bound to that field of the slot's props.
  fn slot_lets(&mut self, attrs: &[Attr], lets: &mut Vec<(String, Expr)>) -> Lowered<()> {
    for attr in attrs {
      if let Attr::Directive { kind, name, expression, at, .. } = attr {
        if kind != "let" {
          continue;
        }
        let value = Expr::Index(Box::new(var(SLOT_PROPS)), Box::new(int(0))).field(name.clone());
        match expression {
          Some(code) if !is_identifier(code.code.trim()) => {
            let holder = self.lowerer.temp();
            lets.push((holder.clone(), value));
            let before = self.lowerer.scope.len();
            self.pattern(code, &holder)?;
            let bound: Vec<(String, Expr)> = self.lowerer.scope.drain(before..).collect();
            lets.extend(bound);
          }
          Some(code) => lets.push((code.code.trim().to_owned(), value)),
          None => lets.push((name.clone(), value)),
        }
        let _ = at;
      }
    }
    Ok(())
  }

  /// One group of a component's children with the `let:` names bound around it.
  fn slot_group(&mut self, fragment: &Fragment, lets: Vec<(String, Expr)>) -> Lowered<(Vec<Tmpl>, bool)> {
    let scope_before = self.lowerer.scope.len();
    for (name, _) in &lets {
      self.bind(name.clone(), Expr::Var(name.clone()));
    }
    let content = self.fragment(fragment);
    self.lowerer.scope.truncate(scope_before);
    let content = content?;
    let bound = !lets.is_empty();
    let mut tree = content;
    for (name, expr) in lets.into_iter().rev() {
      tree = vec![Tmpl::Let { name, expr, then: Box::new(Tmpl::Fragment(tree)) }];
    }
    Ok((tree, bound))
  }

  /// `build_inline_component` for a component the script imports: its props;
  /// its default children as the caller's children; every other slot group,
  /// every snippet among its children and every snippet passed as a prop as
  /// the caller's content for that slot; an anchor after it unless it is its
  /// fragment's one node.
  fn component_node(&mut self, node: &Node, standalone: bool, out: &mut Vec<Tmpl>) -> Lowered<()> {
    let Node::Component { name, dynamic, svg, attrs, fragment, slots, at } = node else { unreachable!("a component") };
    let (props, children, css) = self.component_parts(attrs, fragment, slots)?;
    let dynamic = *dynamic || name.contains('.');
    let placed = match dynamic {
      true => {
        let choice = self.expr(name, *at)?;
        chosen_component(choice, props, children)
      }
      false => {
        self.child_refs.push(ChildRef { local: name.clone(), attrs: Vec::new(), line: at.0, column: at.1 });
        Tmpl::Component { module: name.clone(), props, children, id: 0, keyed: false }
      }
    };
    if !css.is_empty() {
      out.extend(css_props(*svg, css, placed, dynamic));
      return Ok(());
    }
    out.push(placed);
    if !dynamic && !standalone {
      out.push(Tmpl::Marker(EMPTY.to_owned()));
    }
    Ok(())
  }

  /// A component's props and its children as `build_inline_component` passes them.
  fn component_parts(&mut self, attrs: &[Attr], fragment: &Fragment, slots: &[SlotGroup]) -> Lowered<(Vec<Entry>, Vec<Tmpl>, Vec<(String, Expr)>)> {
    let mut props = Vec::new();
    let mut css: Vec<(String, Expr)> = Vec::new();
    let mut wrappers = Vec::new();
    let mut own_lets = Vec::new();
    for attr in attrs {
      match attr {
        Attr::Attr { name: prop, value, .. } if !prop.starts_with("--") => {
          if let AttrValue::Parts(parts) = value {
            if let [Part::Code(code)] = parts.as_slice() {
              let ident = code.code.trim();
              if let Some(snippet) = self.snippets.iter().rev().find(|s| s.name == ident).cloned() {
                let content = self.snippet_content(&snippet)?;
                wrappers.push(Tmpl::Let { name: format!("{SLOT_CONTENT_PREFIX}{}", slot_name(prop)), expr: text("prop"), then: Box::new(content) });
                continue;
              }
              if let Some(key) = self.prop_keys.iter().rev().find(|(local, _)| local == ident).map(|(_, key)| key.clone()) {
                let forwarded = Tmpl::Let { name: format!("{SLOT_OUT_PREFIX}{}", slot_name(&key)), expr: var(SLOT_PROPS), then: Box::new(Tmpl::Fragment(Vec::new())) };
                wrappers.push(Tmpl::Let { name: format!("{SLOT_CONTENT_PREFIX}{}", slot_name(prop)), expr: var(SLOTS_GIVEN).field(slot_name(&key)), then: Box::new(forwarded) });
              }
            }
          }
          let value = match self.attr_value(value, false) {
            Ok(lowered) => lowered,
            Err(residue) => match value {
              AttrValue::Parts(parts) if matches!(parts.as_slice(), [Part::Code(code)] if self.is_function(code)) => {
                self.lowerer.unbound = None;
                Expr::Handler(snapfire_fsr_ir::render::BROWSER_HANDLER)
              }
              _ => return Err(residue),
            },
          };
          props.push(Entry::Field(prop.clone(), value));
        }
        Attr::Attr { name: prop, value, .. } => css.push((prop.clone(), self.attr_value(value, false)?)),
        Attr::Spread { code, at } => props.push(Entry::Spread(self.expr(code, *at)?)),
        Attr::Directive { kind, name: bound, expression, at, .. } if kind == "bind" => {
          if bound != "this" {
            let (value, _) = self.bound(expression, bound, *at)?;
            props.push(Entry::Field(bound.clone(), value));
          }
        }
        Attr::Directive { .. } | Attr::Attach { .. } => {}
      }
    }
    self.slot_lets(attrs, &mut own_lets)?;
    let snippets_before = self.snippets.len();
    let declared: Vec<Snippet> = fragment.hoisted.iter().filter_map(|n| match n {
      Node::Snippet { name, params, body, .. } => Some(Snippet { name: name.clone(), params: params.clone(), body: body.clone() }),
      _ => None,
    }).collect();
    self.snippets.extend(declared.iter().cloned());
    let groups: Vec<SlotGroup> = match slots.is_empty() {
      true => vec![SlotGroup { name: "default".to_owned(), fragment: Fragment { hoisted: fragment.hoisted.iter().filter(|n| !matches!(n, Node::Snippet { .. })).cloned().collect(), nodes: fragment.nodes.clone(), standalone: fragment.standalone, text_first: fragment.text_first } }],
      false => slots.to_vec(),
    };
    let mut children = Vec::new();
    let lowered: Lowered<()> = (|| {
      for snippet in &declared {
        let content = self.snippet_content(snippet)?;
        wrappers.push(Tmpl::Let { name: format!("{SLOT_CONTENT_PREFIX}{}", slot_name(&snippet.name)), expr: Expr::Lit(Lit::Bool(true)), then: Box::new(content) });
      }
      for group in &groups {
        let mut lets = if group.name == "default" { own_lets.clone() } else { Vec::new() };
        for node in &group.fragment.nodes {
          match node {
            Node::Element { attrs, .. } | Node::Component { attrs, .. } | Node::Special { attrs, .. } => self.slot_lets(attrs, &mut lets)?,
            _ => {}
          }
        }
        let (content, bound) = self.slot_group(&group.fragment, lets)?;
        if content.is_empty() {
          continue;
        }
        if group.name == "default" && !bound {
          children.extend(content);
        } else {
          wrappers.push(Tmpl::Let { name: format!("{SLOT_CONTENT_PREFIX}{}", group.name), expr: Expr::Lit(Lit::Bool(true)), then: Box::new(Tmpl::Fragment(content)) });
        }
      }
      Ok(())
    })();
    self.snippets.truncate(snippets_before);
    lowered?;
    children.extend(wrappers);
    Ok((props, children, css))
  }

  /// `<svelte:element this={tag}>`, `$.element`: its attributes as an
  /// element's, built without the rules an element name picks, its content
  /// then an anchor unless the tag holds raw text, `<!---->` around it all
  /// and nothing between for a tag that is `null` or `''`.
  fn dynamic_element(&mut self, attrs: &[Attr], scoped: &Option<String>, this: &Code, fragment: &Fragment, at: (usize, usize), out: &mut Vec<Tmpl>) -> Lowered<()> {
    let tag = self.expr(&this.code, this.at)?;
    let shell = Node::Element { name: "svelte:element".to_owned(), svg: false, mathml: false, scoped: scoped.clone(), attrs: attrs.to_vec(), fragment: Fragment { hoisted: Vec::new(), nodes: Vec::new(), standalone: false, text_first: false }, at };
    let Tmpl::Element { attrs: mut entries, .. } = self.element(&shell)? else { unreachable!("an element") };
    let mut flags = DYNAMIC_ELEMENT;
    entries.retain(|entry| match entry {
      Entry::Field(name, Expr::Lit(Lit::Int(n))) if name == SVELTE_FLAGS => {
        flags |= *n;
        false
      }
      _ => true,
    });
    entries.insert(0, Entry::Field(CHOSEN_TAG.to_owned(), tag.clone()));
    entries.push(Entry::Field(SVELTE_FLAGS.to_owned(), int(flags)));
    let mut children = self.fragment(fragment)?;
    let raw = Expr::Array(["textarea", "script", "style", "title"].iter().map(|t| Entry::Item(text(t))).collect());
    children.push(Tmpl::If { cond: Expr::Not(Box::new(call(Builtin::Includes, vec![raw, tag.clone()]))), then: Box::new(Tmpl::Marker(EMPTY.to_owned())), r#else: None });
    out.push(Tmpl::Marker(EMPTY.to_owned()));
    out.push(Tmpl::If { cond: tag, then: Box::new(Tmpl::Element { tag: CHOSEN_TAG.to_owned(), attrs: entries, children }), r#else: None });
    out.push(Tmpl::Marker(EMPTY.to_owned()));
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
    let Node::Element { name, svg, mathml, scoped, attrs, fragment, at } = node else { unreachable!("an element") };
    let hash = scoped.clone();
    for attr in attrs {
      if let Attr::Spread { code, at: spread_at } = attr {
        match self.placement_spread(code, *spread_at)? {
          Some(("link", options)) if name == "a" => return self.link_element(&options, attrs, fragment, *at),
          Some((placement, _)) => return Err(self.at(spread_at.0, spread_at.1, format!("a `{placement}()` spread on `<{name}>`"))),
          None => {}
        }
      }
    }
    let valued = |n: &str| n == "value" || n.to_lowercase() == "defaultvalue";
    let valued_select = name == "select" && attrs.iter().any(|a| matches!(a, Attr::Attr { name, .. } if valued(name)) || matches!(a, Attr::Directive { kind, name, .. } if kind == "bind" && valued(name)) || matches!(a, Attr::Spread { .. }));
    if valued_select || name == "option" {
      return self.listed(node);
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
      entries.push(Entry::Spread(merged_attributes(fields, hash.as_deref(), class_directives, style_directives)));
      let flags = if namespaced { 1 | 2 } else if name.contains('-') || attrs.iter().any(|a| matches!(a, Attr::Attr { name, .. } if name == "is")) { 2 } else if name == "input" { 4 } else { 0 };
      if flags != 0 {
        entries.push(Entry::Field(SVELTE_FLAGS.to_owned(), int(flags)));
      }
    } else {
      let items_had_class = items.iter().any(|item| matches!(item, Item::Attr { name, value } if (if namespaced { name.clone() } else { name.to_lowercase() }) == "class" && (matches!(value, AttrValue::Present(true)) || static_text(value).is_some())));
      let mut classed = false;
      for item in items {
        match item {
          Item::Bound { name, value } => entries.push(Entry::Field(name, value)),
          Item::Spread(_) => unreachable!("a spread takes the other path"),
          Item::Attr { name: a, value } => {
            let n = attr_name(&a);
            let directed = (n == "class" && !class_directives.is_empty()) || (n == "style" && !style_directives.is_empty());
            let trim = n == "class" || n == "style";
            if !directed && (matches!(value, AttrValue::Present(true)) || static_text(&value).is_some()) {
              let mut written = match &value {
                AttrValue::Present(_) => String::new(),
                _ => match self.ss(&value, trim)? {
                  Expr::Lit(Lit::Str(text)) => text.to_string(),
                  _ => unreachable!("static text is a literal"),
                },
              };
              if let (true, Some(hash)) = (n == "class", &hash) {
                written = format!("{written} {hash}").trim().to_owned();
              }
              if n != "class" || !written.is_empty() || matches!(value, AttrValue::Present(_)) {
                entries.push(Entry::Baked(format!(" {n}=\"{written}\"")));
              }
              continue;
            }
            match n.as_str() {
              "class" => {
                let class = self.class_value(&value)?;
                let directives = std::mem::take(&mut class_directives);
                classed = true;
                entries.push(Entry::Field(n, self.class_attr(Some(class), hash.as_deref(), directives)?.expect("a class")));
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
      let static_class = items_had_class;
      match (&hash, class_directives.is_empty()) {
        _ if classed || static_class => {}
        (Some(hash), true) => entries.push(Entry::Baked(format!(" class=\"{hash}\""))),
        (Some(hash), false) => entries.push(Entry::Field("class".to_owned(), self.class_attr(Some(text(hash)), None, class_directives)?.expect("a class"))),
        (None, false) => entries.push(Entry::Field("class".to_owned(), self.class_attr(None, None, class_directives)?.expect("a class"))),
        (None, true) => {}
      }
      if !style_directives.is_empty() {
        entries.push(Entry::Field("style".to_owned(), self.style_attr(None, style_directives)?.expect("a style")));
      }
    }
    for name in captures {
      entries.push(Entry::Baked(format!(" {name}=\"this.__e=event\"")));
    }
    let children = match (body, fragment.nodes.as_slice()) {
      (None, [Node::Text { data }]) if name == "script" || name == "style" => vec![Tmpl::Marker(data.clone())],
      (None, nodes) if name == "textarea" && (nodes.len() > 1 || !matches!(nodes.first(), Some(Node::Text { .. }) | None)) => {
        let mut content = fragment.clone();
        if let Some(Node::Text { data }) = content.nodes.first_mut() {
          *data = data.strip_prefix("\r\n").or_else(|| data.strip_prefix('\n')).unwrap_or(data).to_owned();
        }
        self.fragment(&content)?
      }
      (Some(Body::Text(value)), _) => vec![Tmpl::Expr(value)],
      (Some(Body::Raw(value)), _) => {
        entries.push(Entry::Field(RAW_ATTR.to_owned(), Expr::Coalesce(Box::new(value), Box::new(text("")))));
        Vec::new()
      }
      _ => self.fragment(fragment)?,
    };
    Ok(Tmpl::Element { tag: name.clone(), attrs: entries, children })
  }

  /// `<option>` and a `<select>` with a value as `$$renderer.option` and
  /// `$$renderer.select` write them: the attributes through `$.attributes`; the
  /// select's value (its `defaultValue` when it has none) in scope for every option beneath
  /// it, components included; an option marked `selected` when its value, or
  /// its content as written with each `<!---->` taken out, is that value or
  /// is among them for a `multiple` select; `<!>` closing rich content.
  fn listed(&mut self, node: &Node) -> Lowered<Tmpl> {
    let Node::Element { name, svg, mathml, scoped, attrs, fragment, .. } = node else { unreachable!("an element") };
    let namespaced = *svg || *mathml;
    let mut fields: Vec<Entry> = Vec::new();
    let mut class_directives: Vec<(String, Expr)> = Vec::new();
    let mut style_directives: Vec<StyleDirective> = Vec::new();
    for attr in attrs {
      match attr {
        Attr::Attr { name: a, value, .. } => {
          if is_event(a, value) {
            continue;
          }
          let n = if namespaced { a.clone() } else { a.to_lowercase() };
          let n = if name == "select" && n == "defaultvalue" { "defaultValue".to_owned() } else { n };
          let value = match n.as_str() {
            "class" => self.class_value(value)?,
            "style" => self.ss(value, true)?,
            _ => self.ss(value, false)?,
          };
          fields.push(Entry::Field(n, value));
        }
        Attr::Spread { code, at } => fields.push(Entry::Spread(self.expr(code, *at)?)),
        Attr::Attach { .. } => {}
        Attr::Directive { kind, name: directive, expression, value, at, .. } => match kind.as_str() {
          "bind" => {
            let (value, _) = self.bound(expression, directive, *at)?;
            fields.push(Entry::Field(if namespaced { directive.clone() } else { directive.to_lowercase() }, value));
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
            style_directives.push(StyleDirective { name: directive.clone(), value, important: false });
          }
          "on" | "use" | "transition" | "in" | "out" | "animate" | "let" => {}
          other => return Err(self.at(at.0, at.1, format!("`{other}:`, a directive the build does not read"))),
        },
      }
    }
    let merged = merged_attributes(fields, scoped.as_deref(), class_directives, style_directives);
    let flags = if namespaced { 1 | 2 } else if attrs.iter().any(|a| matches!(a, Attr::Attr { name, .. } if name == "is")) { 2 } else { 0 };
    let rich = is_rich(name, &fragment.nodes);
    let mut children = self.fragment(fragment)?;
    if name == "select" {
      if rich {
        children.push(Tmpl::Marker(RICH.to_owned()));
      }
      let owned = Expr::Block(vec![
        bind(LISTED, call(Builtin::Omit, vec![var(SELECT), text("value"), text("defaultValue")])),
        when(cmp(CompareOp::Eq, var(LISTED).field("multiple"), text("")), vec![set(LISTED, Expr::Object(vec![Entry::Spread(var(LISTED)), Entry::Field("multiple".to_owned(), Expr::Lit(Lit::Bool(true)))]))], Vec::new()),
        Stmt::Return(var(LISTED)),
      ]);
      let has = |key: &str| call(Builtin::HasKey, vec![var(SELECT), text(key)]);
      let scope = Expr::Object(vec![
        Entry::Field("has".to_owned(), logic(LogicOp::Or, has("value"), has("defaultValue"))),
        Entry::Field("value".to_owned(), Expr::Ternary(Box::new(has("value")), Box::new(var(SELECT).field("value")), Box::new(var(SELECT).field("defaultValue")))),
        Entry::Field("multiple".to_owned(), logic(LogicOp::Or, cmp(CompareOp::Eq, var(SELECT).field("multiple"), text("")), var(SELECT).field("multiple"))),
      ]);
      let mut entries = vec![Entry::Spread(owned)];
      if flags != 0 {
        entries.push(Entry::Field(SVELTE_FLAGS.to_owned(), int(flags)));
      }
      let element = Tmpl::Element { tag: name.clone(), attrs: entries, children };
      let scoped = Tmpl::Let { name: format!("{CONTEXT_PREFIX}{SELECT_CONTEXT}"), expr: scope, then: Box::new(element) };
      return Ok(Tmpl::Let { name: SELECT.to_owned(), expr: merged, then: Box::new(scoped) });
    }
    strip_empty_markers(&mut children);
    if rich {
      children.push(Tmpl::Marker(RICH.to_owned()));
    }
    let written_value = attrs.iter().any(|a| matches!(a, Attr::Attr { name, .. } if name == "value"));
    let content = match (fragment.nodes.as_slice(), written_value) {
      ([Node::Expr { code, at }], false) => Some(self.expr(code, *at)?),
      (nodes, _) => self.option_text(nodes)?,
    };
    let scope = Expr::Context(SELECT_CONTEXT.to_owned());
    let has_value = call(Builtin::HasKey, vec![var(OPTION), text("value")]);
    let known = match content.is_some() {
      true => Expr::Lit(Lit::Bool(true)),
      false => has_value.clone(),
    };
    let value = Expr::Ternary(Box::new(has_value), Box::new(var(OPTION).field("value")), Box::new(content.unwrap_or(Expr::Lit(Lit::Null))));
    let chosen = scope.clone().field("value");
    let among = logic(LogicOp::And, scope.clone().field("multiple"), call(Builtin::IsArray, vec![chosen.clone()]));
    let matched = Expr::Ternary(Box::new(among), Box::new(call(Builtin::Includes, vec![chosen.clone(), value.clone()])), Box::new(cmp(CompareOp::Eq, value, chosen)));
    let selected = logic(LogicOp::And, cmp(CompareOp::Ne, scope.clone(), Expr::Lit(Lit::Null)), logic(LogicOp::And, scope.field("has"), logic(LogicOp::And, known, matched)));
    let mut entries = vec![Entry::Spread(var(OPTION))];
    if flags != 0 {
      entries.push(Entry::Field(SVELTE_FLAGS.to_owned(), int(flags)));
    }
    entries.push(Entry::Field(SVELTE_SELECTED.to_owned(), selected));
    let element = Tmpl::Element { tag: name.clone(), attrs: entries, children };
    Ok(Tmpl::Let { name: OPTION.to_owned(), expr: merged, then: Box::new(element) })
  }

  /// An option's content as the string its body renders to with each
  /// `<!---->` taken out: text escaped, an expression escaped, `{@html}` as it
  /// is. `None` for content holding markup, whose string the build does not form.
  fn option_text(&mut self, nodes: &[Node]) -> Lowered<Option<Expr>> {
    let mut parts = Vec::new();
    for node in nodes {
      match node {
        Node::Text { data } => parts.push(text(&escape_text(data))),
        Node::Comment { .. } => {}
        Node::Expr { code, at } => {
          let shown = Expr::Str(Box::new(Expr::Coalesce(Box::new(self.expr(code, *at)?), Box::new(text("")))));
          parts.push(call(Builtin::ReplaceAll, vec![call(Builtin::ReplaceAll, vec![shown, text("&"), text("&amp;")]), text("<"), text("&lt;")]));
        }
        Node::Html { code, at } => parts.push(Expr::Str(Box::new(Expr::Coalesce(Box::new(self.expr(code, *at)?), Box::new(text("")))))),
        _ => return Ok(None),
      }
    }
    Ok(Some(Expr::Template(parts)))
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
      js::Expr::Seq(seq) if !seq.exprs.is_empty() => match unwrap_types(&seq.exprs[0]) {
        js::Expr::Arrow(arrow) if arrow.params.is_empty() => match &*arrow.body {
          js::ArrowFunctionBody::Expr(body) => Ok((self.lowerer.expr(body)?, true)),
          js::ArrowFunctionBody::FunctionBody(block) => Ok((crate::component::block_to_expr(&mut self.lowerer, &block.stmts)?, true)),
        },
        _ => {
          let call = js::Expr::Call(js::CallExpr { callee: js::Callee::Expr(seq.exprs[0].clone()), ..Default::default() });
          Ok((self.lowerer.expr(&call)?, true))
        }
      },
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
  fn class_attr(&mut self, class: Option<Expr>, hash: Option<&str>, directives: Vec<(String, Expr)>) -> Lowered<Option<Expr>> {
    let class = match (class, hash) {
      (Some(Expr::Lit(Lit::Str(given))), Some(hash)) => Some(text(format!("{given} {hash}").trim())),
      (class, None) => class,
      (class, Some(hash)) => return Ok(Some(to_class(class, Some(hash), directives))),
    };
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
    Ok(Some(to_class(class, None, directives)))
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

/// `escape_html(value)`.
fn escape_text(text: &str) -> String {
  text.replace('&', "&amp;").replace('<', "&lt;")
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
/// The local name `<svelte:self>` is placed under until the build maps it to the component's own module.
pub(crate) const SELF: &str = "svelte:self";
/// The `SVELTE_FLAGS` bit of `<svelte:element>`, whose void tag `$.element` closes with `>`.
const DYNAMIC_ELEMENT: i128 = 8;

/// `$.css_props`: a component with CSS custom properties inside
/// `<svelte-css-wrapper style="display: contents; …">` (`<g style="…">`
/// where Svelte takes the namespace for SVG) holding each property with a
/// value as `name: value;`. A static component takes an anchor after it.
fn css_props(svg: bool, css: Vec<(String, Expr)>, placed: Tmpl, dynamic: bool) -> Vec<Tmpl> {
  let declarations = Expr::Array(
    css
      .into_iter()
      .map(|(name, value)| {
        let written = logic(LogicOp::And, cmp(CompareOp::Ne, value.clone(), Expr::Lit(Lit::Null)), cmp(CompareOp::Ne, value.clone(), text("")));
        let escaped = [("&", "&amp;"), ("\"", "&quot;"), ("<", "&lt;")].iter().fold(Expr::Str(Box::new(value)), |e, (from, to)| call(Builtin::ReplaceAll, vec![e, text(from), text(to)]));
        Entry::Item(Expr::Ternary(Box::new(written), Box::new(Expr::Template(vec![text(&format!("{name}: ")), escaped, text(";")])), Box::new(Expr::Lit(Lit::Null))))
      })
      .collect(),
  );
  let styles = call(Builtin::Join, vec![Expr::Filter(Box::new(declarations), Box::new(Expr::lambda(&["d@"], cmp(CompareOp::Ne, var("d@"), Expr::Lit(Lit::Null))))), text(" ")]);
  let (open, close) = match svg {
    false => (Expr::Template(vec![text("<svelte-css-wrapper style=\"display: contents; "), styles, text("\">")]), "</svelte-css-wrapper>"),
    true => (Expr::Template(vec![text("<g style=\""), styles, text("\">")]), "</g>"),
  };
  let mut out = vec![Tmpl::Html(open), placed];
  if !dynamic {
    out.push(Tmpl::Marker(EMPTY.to_owned()));
  }
  out.push(Tmpl::Marker(close.to_owned()));
  out
}

/// A component chosen at run time, Svelte's `if` block around it: the
/// chosen-tag element the renderer places the component the value
/// references through, the else branch's markers when it holds nothing.
fn chosen_component(choice: Expr, props: Vec<Entry>, children: Vec<Tmpl>) -> Tmpl {
  let mut attrs = vec![Entry::Field(CHOSEN_TAG.to_owned(), choice.clone())];
  attrs.extend(props);
  let chosen = Tmpl::Element { tag: CHOSEN_TAG.to_owned(), attrs, children };
  Tmpl::If {
    cond: choice,
    then: Box::new(Tmpl::Fragment(vec![Tmpl::Marker(BLOCK_OPEN.to_owned()), chosen, Tmpl::Marker(BLOCK_CLOSE.to_owned())])),
    r#else: Some(Box::new(Tmpl::Fragment(vec![Tmpl::Marker(BLOCK_OPEN_ELSE.to_owned()), Tmpl::Marker(BLOCK_CLOSE.to_owned())]))),
  }
}
/// The props a Svelte component reads, a slot the caller filled standing in
/// for a prop it did not pass, as the snippet Svelte passes would.
const PROPS: &str = "props@";
/// `$$slots`, `sanitize_slots`: each slot the caller filled as a slot rather than as a prop.
const SLOTS: &str = "slots@";

fn props_with_slots() -> Expr {
  Expr::Object(vec![
    Entry::Spread(call(Builtin::Omit, vec![var(SLOTS_GIVEN), text("default")])),
    Entry::Field("children".to_owned(), var(SLOTS_GIVEN).field("default")),
    Entry::Spread(Expr::Var("$props".to_owned())),
  ])
}

/// The slot a snippet prop fills: `children` is the default slot.
fn slot_name(prop: &str) -> &str {
  if prop == "children" { "default" } else { prop }
}

fn unwrap_assign(pat: &js::Pat) -> &js::Pat {
  match pat {
    js::Pat::Assign(assign) => &assign.left,
    other => other,
  }
}
const SELECT: &str = "select@";
const OPTION: &str = "option@";
const LISTED: &str = "listed@";
/// The context a `<select>` with a value provides, Svelte's `renderer.local`.
const SELECT_CONTEXT: &str = "svelte:select";
/// What `$$renderer.select` and `$$renderer.option` write before closing rich content.
const RICH: &str = "<!>";

/// `is_customizable_select_element`: a `<select>` holding anything but options
/// and groups, a group holding anything but options, an option holding
/// anything but text, looking through blocks.
fn is_rich(name: &str, nodes: &[Node]) -> bool {
  fn flattened<'n>(nodes: &'n [Node], out: &mut Vec<&'n Node>) {
    for node in nodes {
      match node {
        Node::Snippet { .. } | Node::Const { .. } | Node::Comment { .. } | Node::Expr { .. } => {}
        Node::Other { kind, .. } if kind == "DebugTag" => {}
        Node::Text { data } => {
          if !data.trim().is_empty() {
            out.push(node);
          }
        }
        Node::If { consequent, alternate, .. } => {
          flattened(&consequent.nodes, out);
          if let Some(alternate) = alternate {
            flattened(&alternate.nodes, out);
          }
        }
        Node::Each { body, fallback, .. } => {
          flattened(&body.nodes, out);
          if let Some(fallback) = fallback {
            flattened(&fallback.nodes, out);
          }
        }
        Node::Key { fragment, .. } => flattened(&fragment.nodes, out),
        Node::Special { kind, fragment, .. } if kind == "SvelteBoundary" => flattened(&fragment.nodes, out),
        Node::Await { pending, then, catch, .. } => {
          for part in [pending, then, catch].into_iter().flatten() {
            flattened(&part.nodes, out);
          }
        }
        other => out.push(other),
      }
    }
  }
  if !matches!(name, "select" | "optgroup" | "option") {
    return false;
  }
  let mut found = Vec::new();
  flattened(nodes, &mut found);
  found.iter().any(|node| match node {
    Node::Element { name: child, .. } => match name {
      "select" => child != "option" && child != "optgroup",
      "optgroup" => child != "option",
      _ => true,
    },
    Node::Text { .. } => name != "option",
    _ => true,
  })
}

/// Takes out every `<!---->` an option's content holds, as `$$renderer.option` does.
fn strip_empty_markers(children: &mut Vec<Tmpl>) {
  children.retain(|child| !matches!(child, Tmpl::Marker(m) if m == EMPTY));
  for child in children.iter_mut() {
    match child {
      Tmpl::Element { children, .. } | Tmpl::Fragment(children) => strip_empty_markers(children),
      Tmpl::If { then, r#else, .. } => {
        strip_one(then);
        if let Some(other) = r#else {
          strip_one(other);
        }
      }
      Tmpl::For { body, .. } => strip_one(body),
      Tmpl::Let { then, .. } => strip_one(then),
      _ => {}
    }
  }
}

fn strip_one(tmpl: &mut Tmpl) {
  if matches!(tmpl, Tmpl::Marker(m) if m == EMPTY) {
    *tmpl = Tmpl::Fragment(Vec::new());
    return;
  }
  let mut held = vec![std::mem::replace(tmpl, Tmpl::Fragment(Vec::new()))];
  strip_empty_markers(&mut held);
  *tmpl = held.pop().unwrap_or(Tmpl::Fragment(Vec::new()));
}
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
fn merged_attributes(fields: Vec<Entry>, hash: Option<&str>, class_directives: Vec<(String, Expr)>, style_directives: Vec<StyleDirective>) -> Expr {
  let with = |name: &str, value: Expr| Expr::Object(vec![Entry::Spread(var(ATTRS)), Entry::Field(name.to_owned(), value)]);
  let field = |name: &str| var(ATTRS).field(name);
  let mut body = vec![bind(ATTRS, Expr::Object(fields))];
  if !style_directives.is_empty() {
    let reserved: Vec<String> = style_directives.iter().map(|d| css_name(&d.name)).collect();
    body.push(set(ATTRS, with("style", to_style(field("style"), &reserved, style_directives))));
  }
  body.push(when(field("class"), vec![set(ATTRS, with("class", svelte_clsx(field("class"))))], Vec::new()));
  if hash.is_some() || !class_directives.is_empty() {
    body.push(set(ATTRS, with("class", to_class(Some(field("class")), hash, class_directives))));
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

/// Whether a Svelte 4 component reads `$$props` or `$$restProps`.
fn reads_props_whole(template: &serde_json::Value, script: Option<&str>) -> bool {
  let text = template.to_string();
  ["$$props", "$$restProps"].iter().any(|name| text.contains(name) || script.is_some_and(|s| s.contains(name)))
}

/// `$inspect(…).with(…)`, which only the browser runs.
fn inspects(call: &js::CallExpr) -> bool {
  let js::Callee::Expr(callee) = &call.callee else { return false };
  let js::Expr::Member(member) = &**callee else { return false };
  matches!(&*member.obj, js::Expr::Call(inner) if callee_name(inner).as_deref() == Some("$inspect"))
}

/// `name.subscribe(fn)`: the store and the function.
fn subscribed(call: &js::CallExpr) -> Option<(&js::Expr, &js::Expr)> {
  let js::Callee::Expr(callee) = &call.callee else { return None };
  let js::Expr::Member(member) = &**callee else { return None };
  let js::MemberProp::Ident(prop) = &member.prop else { return None };
  if prop.sym != "subscribe" || call.args.len() != 1 || !matches!(&*member.obj, js::Expr::Ident(_)) {
    return None;
  }
  Some((&member.obj, &call.args[0].expr))
}

/// Each name an assignment in `stmt` writes, its patterns included.
fn assigned_names(stmt: &js::Stmt, out: &mut Vec<String>) {
  struct Assigned<'o>(&'o mut Vec<String>);
  impl swc_core::ecma::visit::Visit for Assigned<'_> {
    fn visit_assign_expr(&mut self, assign: &js::AssignExpr) {
      match &assign.left {
        js::AssignTarget::Simple(js::SimpleAssignTarget::Ident(id)) => self.0.push(id.id.sym.to_string()),
        js::AssignTarget::Pat(pat) => {
          struct Bound<'o>(&'o mut Vec<String>);
          impl swc_core::ecma::visit::Visit for Bound<'_> {
            fn visit_binding_ident(&mut self, id: &js::BindingIdent) {
              self.0.push(id.id.sym.to_string());
            }
            fn visit_expr(&mut self, _: &js::Expr) {}
          }
          pat.visit_with(&mut Bound(self.0));
        }
        _ => {}
      }
      assign.right.visit_with(self);
    }
  }
  stmt.visit_with(&mut Assigned(out));
}

/// Gives a template expression's free names the script's top-level context,
/// which is what they name in Svelte's scope, so the library ones read by
/// what they are bound to. A name the template binds keeps its own.
struct TopLevel {
  top: swc_core::common::SyntaxContext,
  local: Vec<String>,
}

impl VisitMut for TopLevel {
  /// What Svelte's server writes for a rune a template reads:
  /// `$state.snapshot(x)` is `x`, `$effect.tracking()` is `false` and
  /// `$effect.pending()` is `0`.
  fn visit_mut_expr(&mut self, expr: &mut js::Expr) {
    if let js::Expr::Call(call) = expr {
      let rune = callee_name(call);
      match rune.as_deref() {
        Some("$state.snapshot") if call.args.len() == 1 => {
          *expr = *call.args[0].expr.clone();
        }
        Some("$effect.tracking") => *expr = js::Expr::Lit(js::Lit::Bool(js::Bool { span: call.span, value: false })),
        Some("$effect.pending") => *expr = js::Expr::Lit(js::Lit::Num(js::Number { span: call.span, value: 0.0, raw: None })),
        _ => {}
      }
    }
    expr.visit_mut_children_with(self);
  }

  fn visit_mut_ident(&mut self, id: &mut js::Ident) {
    if id.ctxt == swc_core::common::SyntaxContext::empty() && !self.local.iter().any(|name| *name == *id.sym) {
      id.ctxt = self.top;
    }
  }
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
fn to_class(class: Option<Expr>, hash: Option<&str>, directives: Vec<(String, Expr)>) -> Expr {
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
  if let Some(hash) = hash {
    body.push(set(CLASS, Expr::Ternary(Box::new(eq(var(CLASS), text(""))), Box::new(text(hash)), Box::new(Expr::Template(vec![var(CLASS), text(&format!(" {hash}"))])))));
  }
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
