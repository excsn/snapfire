//! Reads a Vue single-file component the plugin described and lowers it to a
//! render tree, the way `component` reads a `.tsx` module. The `<script
//! setup>` block is TypeScript and goes through the body lowerer: `ref` and
//! `computed` bind the way `useState` and a `const` do, `defineProps` is
//! `$props` and a function is a handler body an `@event` can name. The
//! template is Vue's own parse tree, its expressions parsed one at a time in
//! the file's coordinates. An `@event` lowers as a handler a server island
//! steps or leaves its reason on the element. Anything else outside that is
//! residue and the component stays foreign.

use std::borrow::Cow;
use std::collections::HashMap;
use std::rc::Rc;

use serde::Deserialize;
use snapfire_compiler_wire::Described;
use snapfire_fsr_ir::ast::{Builtin, CompareOp, Component, Entry, Expr, Handler, Lit, LogicOp, Stmt, Tmpl};
use snapfire_fsr_ir::render::{CONTEXT_PREFIX, HANDLER_ATTR, RAW_ATTR, SLOT_CONTENT_PREFIX, SLOT_OUT_PREFIX, SLOT_PROPS, UNLOWERED_ATTR};
use snapfire_fsr_ir::Owner;
use swc_core::common::Spanned;
use swc_core::ecma::ast as js;

use crate::assets::AssetResolver;
use crate::component::{action_alias_of, arrow_body, bind_object, bind_pattern, block_to_expr, find_import, generated_action_of, holds_act, imported_as, is_template_source, patterns, reach_under, FunctionBody, HandlerWalk, HeadRow};
use crate::placements::{self, At, Attr, Placer, Value};
use crate::{Lowered, Lowerer, Residue};

/// The client's Vue adapter, where `useStore` and `useLocale` come from.
pub const VUE_CLIENT: &str = "@snapfire/fsr-client/vue";

/// The template tree as the plugin's `describeNode` writes it.
#[derive(Debug, Deserialize)]
pub struct Template {
  #[serde(default)]
  pub children: Vec<Node>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Node {
  pub node: String,
  #[serde(default)]
  pub tag: String,
  /// Vue's `ElementTypes`: element 0, component 1, slot 2, template 3.
  #[serde(default)]
  pub kind: u8,
  #[serde(default)]
  pub props: Vec<Prop>,
  #[serde(default)]
  pub children: Vec<Node>,
  #[serde(default)]
  pub content: String,
  /// Vue's `NodeTypes` number of a node the plugin passed through unread.
  #[serde(default, rename = "type")]
  pub other: u32,
  #[serde(default)]
  pub line: usize,
  #[serde(default)]
  pub column: usize,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Prop {
  pub prop: String,
  pub name: String,
  #[serde(default)]
  pub value: Option<String>,
  #[serde(default)]
  pub arg: Option<String>,
  #[serde(default, rename = "argStatic")]
  pub arg_static: bool,
  #[serde(default)]
  pub exp: Option<String>,
  #[serde(default)]
  pub modifiers: Vec<String>,
  #[serde(default)]
  pub line: usize,
  #[serde(default)]
  pub column: usize,
  /// Where a directive's expression starts, for a residue inside it.
  #[serde(default, rename = "expLine")]
  pub exp_line: usize,
  #[serde(default, rename = "expColumn")]
  pub exp_column: usize,
}

impl Prop {
  fn is_directive(&self, name: &str) -> bool {
    self.prop == "directive" && self.name == name
  }

  fn binds(&self, arg: &str) -> bool {
    self.is_directive("bind") && self.arg_static && self.arg.as_deref() == Some(arg)
  }
}

/// `@vue/shared`'s `isBooleanAttr`, which decides how a bound value prints.
const BOOLEAN: &[&str] = &["itemscope", "allowfullscreen", "formnovalidate", "ismap", "nomodule", "novalidate", "readonly", "async", "autofocus", "autoplay", "controls", "default", "defer", "disabled", "hidden", "inert", "loop", "open", "required", "reversed", "scoped", "seamless", "checked", "muted", "multiple", "selected"];

/// Calls at the top of `<script setup>` that are the browser's alone.
const BROWSER_CALLS: &[&str] = &["onMounted", "onBeforeMount", "onUnmounted", "onBeforeUnmount", "onUpdated", "onBeforeUpdate", "onActivated", "onDeactivated", "onErrorCaptured", "onRenderTracked", "onRenderTriggered", "onServerPrefetch", "watch", "watchEffect", "watchPostEffect", "watchSyncEffect", "nextTick", "defineEmits", "defineExpose", "defineOptions", "defineSlots", "defineProps"];

/// An element placed among its siblings or a branch that joins the `v-if`
/// before it.
enum Placed {
  Node(Tmpl),
  Else { cond: Option<Expr>, body: Tmpl },
}

/// A Vue child the template placed: the import's local name, the attributes it was given by their prop spelling and where.
pub(crate) struct ChildRef {
  pub(crate) local: String,
  pub(crate) attrs: Vec<String>,
  pub(crate) line: usize,
  pub(crate) column: usize,
}

/// The props a `<script setup>` declares through `defineModel`: its name, `modelValue` when it names none.
pub(crate) fn model_props(parsed: &crate::Parsed) -> Vec<String> {
  let mut out = Vec::new();
  for item in &parsed.module.body {
    let js::ModuleItem::Stmt(js::Stmt::Decl(js::Decl::Var(var))) = item else { continue };
    for decl in &var.decls {
      let Some(js::Expr::Call(call)) = decl.init.as_deref().map(unwrap_types) else { continue };
      let js::Callee::Expr(callee) = &call.callee else { continue };
      if !matches!(&**callee, js::Expr::Ident(id) if id.sym.as_ref() == "defineModel") {
        continue;
      }
      out.push(match call.args.first().map(|a| unwrap_types(&a.expr)) {
        Some(js::Expr::Lit(js::Lit::Str(s))) => s.value.to_atom_lossy().to_string(),
        _ => "modelValue".to_owned(),
      });
    }
  }
  out
}

/// A property key's name when it is written out.
fn prop_name_of_key(key: &js::PropName) -> Option<String> {
  match key {
    js::PropName::Ident(i) => Some(i.sym.to_string()),
    js::PropName::Str(s) => Some(s.value.to_atom_lossy().to_string()),
    _ => None,
  }
}

/// `child-card` as `ChildCard`, the binding a kebab-case tag resolves to.
fn pascal(tag: &str) -> String {
  tag.split('-').map(|part| {
    let mut chars = part.chars();
    match chars.next() {
      Some(first) => first.to_uppercase().chain(chars).collect::<String>(),
      None => String::new(),
    }
  }).collect()
}

/// `item-count` as `itemCount`, the prop a kebab-case attribute names.
fn camel(name: &str) -> String {
  let mut out = String::with_capacity(name.len());
  let mut upper = false;
  for c in name.chars() {
    if c == '-' {
      upper = true;
    } else if upper {
      out.extend(c.to_uppercase());
      upper = false;
    } else {
      out.push(c);
    }
  }
  out
}

pub(crate) struct VueLowerer<'a, 'p> {
  pub(crate) lowerer: Lowerer<'p>,
  described: &'a Described,
  lets: Vec<Stmt>,
  state: Vec<String>,
  /// The `useStore` bindings among `state`, with the key each reads.
  stores: Vec<(String, String)>,
  /// How the template reads each name the script bound: a `ref` unwrapped,
  /// a store holder through `.value`.
  template_scope: Vec<(String, Expr)>,
  /// The scope at the end of `<script setup>`, which a setup function's body reads.
  script_scope: Vec<(String, Expr)>,
  /// Setup functions by name, which a template handler names or calls.
  handler_fns: HashMap<String, (Vec<js::Pat>, FunctionBody<'p>)>,
  handlers: Vec<Handler>,
  /// The setup functions a handler is inside, outermost first.
  calling: Vec<String>,
  /// The Vue children the template placed, for the set to resolve and lower.
  pub(crate) child_refs: Vec<ChildRef>,
  /// The model of the `<select v-model>` the options being lowered sit in.
  select_model: Option<Expr>,
  /// What `provide` calls in `<script setup>` hand down, by context, in order.
  provides: Vec<(String, Expr)>,
  assets: Rc<dyn AssetResolver>,
  /// The head rows the template asks for, a priority image's preload.
  pub(crate) heads: Vec<HeadRow>,
}

impl<'p> Placer<'p> for VueLowerer<'_, 'p> {
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

impl<'a, 'p> VueLowerer<'a, 'p> {
  pub(crate) fn new(lowerer: Lowerer<'p>, described: &'a Described, assets: Rc<dyn AssetResolver>) -> Self {
    Self { lowerer, described, lets: Vec::new(), state: Vec::new(), stores: Vec::new(), template_scope: Vec::new(), script_scope: Vec::new(), handler_fns: HashMap::new(), handlers: Vec::new(), calling: Vec::new(), child_refs: Vec::new(), select_model: None, provides: Vec::new(), assets, heads: Vec::new() }
  }

  pub(crate) fn component(&mut self) -> Lowered<Component> {
    self.script()?;
    let render = self.template()?;
    Ok(Component { body: std::mem::take(&mut self.lets), render, state: std::mem::take(&mut self.state), stores: std::mem::take(&mut self.stores), handlers: std::mem::take(&mut self.handlers), owner: Owner::Vue, shadow: None })
  }

  fn at(&self, line: usize, column: usize, message: impl Into<String>) -> Residue {
    Residue { file: self.lowerer.parsed.file.clone(), line, column, message: message.into(), hint: None, via: Vec::new() }
  }

  fn at_with(&self, line: usize, column: usize, message: impl Into<String>, hint: impl Into<String>) -> Residue {
    Residue { hint: Some(hint.into()), ..self.at(line, column, message) }
  }

  /// Binds `name` for the rest of the script as `script` and for the template as `template`.
  fn bind(&mut self, name: String, script: Expr, template: Expr) {
    self.lowerer.scope.push((name.clone(), script));
    self.template_scope.push((name, template));
  }

  /// A ref's holder: `x.value` reads the binding.
  fn holder(name: &str) -> Expr {
    Expr::Object(vec![Entry::Field("value".to_owned(), Expr::Var(name.to_owned()))])
  }

  fn script(&mut self) -> Lowered<()> {
    let Some(script) = &self.described.script else { return Ok(()) };
    if script.plain {
      let (line, column) = (script.line as usize, script.column as usize);
      return Err(match script.setup {
        true => self.at(line, column, "a `<script>` beside `<script setup>`; the build reads one block"),
        false => self.at_with(line, column, "a `<script>` without `setup`", "the build reads `<script setup>`, where the bindings are declarations rather than an options object"),
      });
    }
    let parsed = self.lowerer.parsed;
    for item in &parsed.module.body {
      match item {
        js::ModuleItem::ModuleDecl(js::ModuleDecl::Import(_)) => {}
        js::ModuleItem::ModuleDecl(other) => return Err(self.lowerer.residue(other.span(), "an export in `<script setup>`")),
        js::ModuleItem::Stmt(stmt) => self.setup_stmt(stmt)?,
      }
    }
    self.script_scope = self.lowerer.scope.clone();
    Ok(())
  }

  fn setup_stmt(&mut self, stmt: &'p js::Stmt) -> Lowered<()> {
    match stmt {
      js::Stmt::Decl(js::Decl::Var(var)) => {
        for decl in &var.decls {
          self.setup_decl(decl)?;
        }
        Ok(())
      }
      js::Stmt::Decl(js::Decl::Fn(f)) => {
        if let Some(body) = &f.function.body {
          self.handler_fns.insert(f.ident.sym.to_string(), (patterns(&f.function), FunctionBody::Block(&body.stmts)));
        }
        Ok(())
      }
      js::Stmt::Decl(js::Decl::TsInterface(_) | js::Decl::TsTypeAlias(_)) => Ok(()),
      js::Stmt::Decl(js::Decl::TsEnum(e)) => Err(self.lowerer.residue(e.span, "an enum in `<script setup>`")),
      js::Stmt::Decl(js::Decl::Class(c)) => Err(self.lowerer.residue(c.class.span, "a class in `<script setup>`")),
      js::Stmt::Expr(e) => {
        if let js::Expr::Call(call) = &*e.expr {
          if matches!(self.callee(call), Some((name, Some(source))) if name == "provide" && source == "vue") {
            let (Some(key), Some(value)) = (call.args.first(), call.args.get(1)) else { return Err(self.lowerer.residue(call.span, "`provide` takes a key and a value")) };
            let key = self.injection_key(&key.expr)?;
            let value = match self.lowerer.expr(&value.expr)? {
              Expr::Object(entries) if matches!(entries.as_slice(), [Entry::Field(field, _)] if field == "value") => match entries.into_iter().next() {
                Some(Entry::Field(_, inner)) => inner,
                _ => unreachable!("matched one field"),
              },
              other => other,
            };
            self.provides.push((key, value));
            return Ok(());
          }
          match self.callee(call) {
            Some((name, _)) if BROWSER_CALLS.contains(&name.as_str()) => return Ok(()),
            Some((name, _)) if name == "defineModel" => return Ok(()),
            _ => {}
          }
        }
        Err(self.setup_residue(e.span))
      }
      other => Err(self.setup_residue(other.span())),
    }
  }

  fn setup_residue(&self, span: swc_core::common::Span) -> Residue {
    self.lowerer.residue_with(span, "a statement in `<script setup>` the build cannot follow", "a setup block the build reads is `const` bindings, `ref`, `computed`, `defineProps`, functions and the lifecycle calls the browser alone runs")
  }

  /// The name a call's callee reaches, with the package it was imported from
  /// when it was; a compiler macro like `defineProps` has no import.
  fn callee(&self, call: &js::CallExpr) -> Option<(String, Option<String>)> {
    let js::Callee::Expr(callee) = &call.callee else { return None };
    let js::Expr::Ident(id) = &**callee else { return None };
    let name = id.sym.to_string();
    match find_import(self.lowerer.parsed, &name) {
      Some((source, imported)) => Some((imported, Some(source))),
      None => Some((name, None)),
    }
  }

  fn setup_decl(&mut self, decl: &'p js::VarDeclarator) -> Lowered<()> {
    let init = decl.init.as_deref().ok_or_else(|| self.lowerer.residue(decl.span, "a declaration without a value"))?;
    let init = unwrap_types(init);
    match &decl.name {
      js::Pat::Ident(name) => self.setup_binding(name.id.sym.to_string(), init, decl.span),
      pattern @ (js::Pat::Object(_) | js::Pat::Array(_)) => {
        let before = self.lowerer.scope.len();
        match (pattern, self.props_call(init)) {
          (js::Pat::Object(obj), Some((call, defaults))) => {
            self.props_defaults(call, defaults)?;
            bind_object(&mut self.lowerer, obj, Expr::Var("$props".to_owned()))?;
          }
          _ => {
            let expr = self.lowerer.expr(init)?;
            let name = self.lowerer.temp();
            self.lets.push(Stmt::Let { name: name.clone(), expr });
            bind_pattern(&mut self.lowerer, pattern, Expr::Var(name))?;
          }
        }
        for (name, expr) in self.lowerer.scope[before..].to_vec() {
          self.template_scope.push((name, expr));
        }
        Ok(())
      }
      other => Err(self.lowerer.residue(other.span(), "a declaration pattern the build does not read")),
    }
  }

  /// `defineProps(...)` or `withDefaults(defineProps(...), { ... })`: the
  /// call and the defaults object when there is one.
  fn props_call<'e>(&self, init: &'e js::Expr) -> Option<(&'e js::CallExpr, Option<&'e js::Expr>)> {
    let js::Expr::Call(call) = init else { return None };
    match self.callee(call)?.0.as_str() {
      "defineProps" => Some((call, None)),
      "withDefaults" => {
        let inner = call.args.first().map(|a| unwrap_types(&a.expr))?;
        let js::Expr::Call(inner) = inner else { return None };
        (self.callee(inner)?.0 == "defineProps").then(|| (inner, call.args.get(1).map(|a| &*a.expr)))
      }
      _ => None,
    }
  }

  /// Rebinds `$props` with the defaults under it, so a prop the placement
  /// left out reads its default wherever it is read.
  fn props_defaults(&mut self, _call: &js::CallExpr, defaults: Option<&js::Expr>) -> Lowered<()> {
    let Some(defaults) = defaults else { return Ok(()) };
    let js::Expr::Object(_) = defaults else {
      return Err(self.lowerer.residue(defaults.span(), "`withDefaults` takes its defaults as an object literal"));
    };
    let defaults = self.lowerer.expr(defaults)?;
    self.lets.push(Stmt::Let { name: "$props".to_owned(), expr: Expr::Object(vec![Entry::Spread(defaults), Entry::Spread(Expr::Var("$props".to_owned()))]) });
    Ok(())
  }

  fn setup_binding(&mut self, name: String, init: &'p js::Expr, span: swc_core::common::Span) -> Lowered<()> {
    if let Some((call, defaults)) = self.props_call(init) {
      self.props_defaults(call, defaults)?;
      self.bind(name, Expr::Var("$props".to_owned()), Expr::Var("$props".to_owned()));
      return Ok(());
    }
    if let js::Expr::Call(call) = init {
      if let Some((callee, source)) = self.callee(call) {
        let from_vue = source.as_deref() == Some("vue");
        match callee.as_str() {
          "ref" | "shallowRef" if from_vue => {
            let expr = match call.args.first() {
              Some(a) => self.lowerer.expr(&a.expr)?,
              None => Expr::Lit(Lit::Null),
            };
            self.lets.push(Stmt::Let { name: name.clone(), expr });
            self.state.push(name.clone());
            self.bind(name.clone(), Self::holder(&name), Expr::Var(name));
            return Ok(());
          }
          "computed" if from_vue => {
            let Some(arg) = call.args.first() else { return Err(self.lowerer.residue(call.span, "`computed` without a getter")) };
            let js::Expr::Arrow(arrow) = unwrap_types(&arg.expr) else {
              return Err(self.lowerer.residue(arg.expr.span(), "`computed` of something other than an arrow; a writable computed is not lowered"));
            };
            if !arrow.params.is_empty() {
              return Err(self.lowerer.residue(arrow.span, "a `computed` getter with a parameter"));
            }
            let expr = match &*arrow.body {
              js::ArrowFunctionBody::Expr(e) => self.lowerer.expr(e)?,
              js::ArrowFunctionBody::FunctionBody(b) => block_to_expr(&mut self.lowerer, &b.stmts)?,
            };
            self.lets.push(Stmt::Let { name: name.clone(), expr });
            self.bind(name.clone(), Self::holder(&name), Expr::Var(name));
            return Ok(());
          }
          "reactive" | "shallowReactive" | "readonly" if from_vue => {
            let Some(arg) = call.args.first() else { return Err(self.lowerer.residue(call.span, format!("`{callee}` without a value"))) };
            let expr = self.lowerer.expr(&arg.expr)?;
            self.lets.push(Stmt::Let { name: name.clone(), expr });
            self.bind(name.clone(), Expr::Var(name.clone()), Expr::Var(name));
            return Ok(());
          }
          "useStore" if source.as_deref() == Some(VUE_CLIENT) => {
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
            self.bind(name.clone(), Self::holder(&name), Self::holder(&name));
            return Ok(());
          }
          "useLocale" if source.as_deref() == Some(VUE_CLIENT) => {
            self.lets.push(Stmt::Let { name: name.clone(), expr: Expr::Locale });
            self.bind(name.clone(), Self::holder(&name), Expr::Var(name));
            return Ok(());
          }
          "defineEmits" => return Ok(()),
          "defineModel" => {
            let (prop, options) = match call.args.first().map(|a| unwrap_types(&a.expr)) {
              Some(js::Expr::Lit(js::Lit::Str(s))) => (s.value.to_atom_lossy().to_string(), call.args.get(1).map(|a| unwrap_types(&a.expr))),
              Some(js::Expr::Object(_)) => ("modelValue".to_owned(), call.args.first().map(|a| unwrap_types(&a.expr))),
              None => ("modelValue".to_owned(), None),
              Some(other) => return Err(self.lowerer.residue(other.span(), "a `defineModel` name that is not a string literal")),
            };
            let read = Expr::Var("$props".to_owned()).field(prop);
            let expr = match options {
              Some(js::Expr::Object(obj)) => {
                let default = obj.props.iter().find_map(|p| match p {
                  js::PropOrSpread::Prop(p) => match &**p {
                    js::Prop::KeyValue(kv) if prop_name_of_key(&kv.key).as_deref() == Some("default") => Some(&*kv.value),
                    _ => None,
                  },
                  _ => None,
                });
                match default {
                  Some(default) => Expr::Coalesce(Box::new(read), Box::new(self.lowerer.expr(default)?)),
                  None => read,
                }
              }
              _ => read,
            };
            self.lets.push(Stmt::Let { name: name.clone(), expr });
            self.bind(name.clone(), Self::holder(&name), Expr::Var(name));
            return Ok(());
          }
          "toRef" | "toRefs" | "customRef" | "useTemplateRef" | "useId" if from_vue => return Err(self.lowerer.residue(call.span, format!("`{callee}`"))),
          "inject" if from_vue => {
            let key = match call.args.first() {
              Some(arg) => self.injection_key(&arg.expr)?,
              None => return Err(self.lowerer.residue(call.span, "`inject` without a key")),
            };
            let default = match call.args.get(1) {
              Some(arg) if call.args.len() > 2 => return Err(self.lowerer.residue(arg.expr.span(), "`inject` with a default factory")),
              Some(arg) => self.lowerer.expr(&arg.expr)?,
              None => Expr::Lit(Lit::Null),
            };
            self.lets.push(Stmt::Let { name: name.clone(), expr: Expr::Coalesce(Box::new(Expr::Context(key)), Box::new(default)) });
            self.bind(name.clone(), Self::holder(&name), Expr::Var(name));
            return Ok(());
          }
          "useAttrs" | "useSlots" if from_vue => return Err(self.lowerer.residue(call.span, format!("`{callee}`, which reads what a parent component passes beyond its props"))),
          _ => {}
        }
      }
    }
    match init {
      js::Expr::Arrow(arrow) => {
        self.handler_fns.insert(name, (arrow.params.clone(), arrow_body(arrow)));
        return Ok(());
      }
      js::Expr::Fn(f) => {
        if let Some(body) = &f.function.body {
          self.handler_fns.insert(name, (patterns(&f.function), FunctionBody::Block(&body.stmts)));
        }
        return Ok(());
      }
      _ => {}
    }
    let expr = self.lowerer.expr(init).map_err(|residue| match residue.line {
      0 => self.lowerer.residue(span, residue.message),
      _ => residue,
    })?;
    self.lets.push(Stmt::Let { name: name.clone(), expr });
    self.bind(name.clone(), Expr::Var(name.clone()), Expr::Var(name.clone()));
    self.lowerer.note_kind(&name, init);
    Ok(())
  }

  fn template(&mut self) -> Lowered<Tmpl> {
    let Some(value) = &self.described.template else { return Ok(Tmpl::Fragment(Vec::new())) };
    let template: Template = serde_json::from_value(value.clone()).map_err(|e| self.at(1, 1, format!("the template description did not read: {e}")))?;
    let mut scope: Vec<(String, Expr)> = self.described.bindings.iter().filter(|(_, kind)| kind.as_str() == "props").map(|(name, _)| (name.clone(), Expr::Var("$props".to_owned()).field(name.clone()))).collect();
    scope.extend(std::mem::take(&mut self.template_scope));
    self.lowerer.scope = scope;
    let mut children = self.children(&template.children)?;
    let mut tree = match children.len() {
      1 => children.pop().expect("one child"),
      _ => Tmpl::Fragment(children),
    };
    for (key, value) in std::mem::take(&mut self.provides).into_iter().rev() {
      tree = Tmpl::Let { name: format!("{CONTEXT_PREFIX}{key}"), expr: value, then: Box::new(tree) };
    }
    Ok(tree)
  }

  /// The context an `inject` or a `provide` key names: `vue:` and a string key, `vue:` and the name a key binding is declared under for a name, followed through its import.
  fn injection_key(&self, key: &js::Expr) -> Lowered<String> {
    match unwrap_types(key) {
      js::Expr::Lit(js::Lit::Str(s)) => Ok(format!("vue:{}", s.value.to_atom_lossy())),
      js::Expr::Ident(id) => Ok(format!("vue:{}", find_import(self.lowerer.parsed, id.sym.as_ref()).map(|(_, imported)| imported).unwrap_or_else(|| id.sym.to_string()))),
      other => Err(self.lowerer.residue(other.span(), "an injection key that is not a string or a name")),
    }
  }

  fn expr_at(&mut self, source: &str, line: usize, column: usize) -> Lowered<Expr> {
    let expr = self.lowerer.parsed.parse_expr_at(source, line, column).map_err(|message| self.at(line, column, format!("`{}`: {message}", source.trim())))?;
    self.lowerer.expr(&expr)
  }

  fn children(&mut self, nodes: &[Node]) -> Lowered<Vec<Tmpl>> {
    let mut out: Vec<Tmpl> = Vec::new();
    for node in nodes {
      match node.node.as_str() {
        "text" => out.push(Tmpl::Text(node.content.clone())),
        "interpolation" => out.push(Tmpl::Expr(self.expr_at(&node.content, node.line, node.column)?)),
        "element" => match self.element(node)? {
          Placed::Node(tmpl) => out.push(tmpl),
          Placed::Else { cond, body } => {
            while matches!(out.last(), Some(Tmpl::Text(text)) if text.trim().is_empty()) {
              out.pop();
            }
            let Some(chain @ Tmpl::If { .. }) = out.last_mut() else {
              return Err(self.at(node.line, node.column, "`v-else` with no `v-if` on the element before it"));
            };
            let branch = match cond {
              Some(cond) => Tmpl::If { cond, then: Box::new(body), r#else: None },
              None => body,
            };
            if !attach(chain, branch) {
              return Err(self.at(node.line, node.column, "`v-else` after a `v-else`"));
            }
          }
        },
        _ => return Err(self.at(node.line, node.column, format!("a template node the build does not read (Vue node type {})", node.other))),
      }
    }
    Ok(out)
  }

  /// An element with its structural directives: `v-if` outermost, then
  /// `v-for`, then the element itself, which is the order Vue gives them. A
  /// `v-if` condition is lowered before the loop's names are in scope, since
  /// it cannot see them.
  fn element(&mut self, node: &Node) -> Lowered<Placed> {
    let mut branch: Option<Branch> = None;
    let mut each: Option<(Expr, Vec<String>)> = None;
    for prop in node.props.iter().filter(|p| p.prop == "directive") {
      match prop.name.as_str() {
        "if" | "else-if" => {
          let Some(exp) = &prop.exp else { return Err(self.at(prop.line, prop.column, format!("`v-{}` without a condition", prop.name))) };
          let cond = self.expr_at(exp, prop.exp_line, prop.exp_column)?;
          branch = Some(if prop.name == "if" { Branch::If(cond) } else { Branch::ElseIf(cond) });
        }
        "else" => branch = Some(Branch::Else),
        "for" => {
          let Some(exp) = &prop.exp else { return Err(self.at(prop.line, prop.column, "`v-for` without an expression")) };
          let (aliases, source) = for_parts(exp).ok_or_else(|| self.at(prop.line, prop.column, format!("`v-for=\"{exp}\"`; write `item in items` or `(item, index) in items`")))?;
          let over = self.expr_at(source, prop.exp_line, prop.exp_column)?;
          let mut params = Vec::new();
          for alias in aliases.split(',') {
            let alias = alias.trim();
            if alias.is_empty() || !alias.chars().all(|c| c.is_alphanumeric() || c == '_' || c == '$') {
              return Err(self.at(prop.line, prop.column, format!("`v-for` over `{alias}`, a pattern; name the item and read its fields")));
            }
            params.push(alias.to_owned());
          }
          if params.len() > 2 {
            return Err(self.at(prop.line, prop.column, "`v-for` with three aliases, which iterates an object; a list has an item and an index"));
          }
          each = Some((over, params));
        }
        _ => {}
      }
    }
    let depth = self.lowerer.scope.len();
    if let Some((_, params)) = &each {
      for param in params {
        self.lowerer.scope.push((param.clone(), Expr::Var(param.clone())));
      }
    }
    let body = match node.kind {
      3 => self.template_element(node),
      2 => self.slot(node),
      1 => self.component_node(node),
      _ => self.plain_element(node),
    };
    self.lowerer.scope.truncate(depth);
    let body = body?;
    let body = match each {
      Some((over, params)) => Tmpl::For { over, params, body: Box::new(body) },
      None => body,
    };
    Ok(match branch {
      None => Placed::Node(body),
      Some(Branch::If(cond)) => Placed::Node(Tmpl::If { cond, then: Box::new(body), r#else: None }),
      Some(Branch::ElseIf(cond)) => Placed::Else { cond: Some(cond), body },
      Some(Branch::Else) => Placed::Else { cond: None, body },
    })
  }

  /// A component in the template: a placement the client's adapters share,
  /// lowered the way every front end lowers it, or any other component,
  /// which the browser mounts.
  fn component_node(&mut self, node: &Node) -> Lowered<Tmpl> {
    match imported_as(self.lowerer.parsed, &node.tag, is_template_source).as_deref() {
      Some("Link") => {
        let attrs = self.placement_attrs(node)?;
        let children = self.children(&node.children)?;
        placements::link(self, &attrs, children)
      }
      Some("Picture") => {
        let attrs = self.placement_attrs(node)?;
        placements::picture(self, &attrs, At::Line(node.line, node.column), false)
      }
      Some("Mount") => {
        let attrs = self.placement_attrs(node)?;
        placements::mount(self, &attrs, At::Line(node.line, node.column))
      }
      _ => self.child_component(node),
    }
  }

  /// A child's slot contents: each `<template #name="params">` as a `SLOT_CONTENT_PREFIX` wrapper, the rest as the default slot, which a `v-slot` on the child itself gives params to.
  fn slot_contents(&mut self, node: &Node) -> Lowered<Vec<Tmpl>> {
    let mut defaults: Vec<&Node> = Vec::new();
    let mut named = Vec::new();
    for child in &node.children {
      let slot = (child.node == "element" && child.kind == 3).then(|| child.props.iter().find(|p| p.is_directive("slot"))).flatten();
      match slot {
        Some(prop) => {
          let name = match (&prop.arg, prop.arg_static) {
            (None, _) => "default".to_owned(),
            (Some(arg), true) => arg.clone(),
            (Some(_), false) => return Err(self.at(prop.line, prop.column, "a slot whose name is an expression")),
          };
          let mut when = Expr::Lit(Lit::Bool(true));
          for other in child.props.iter().filter(|p| p.prop == "directive") {
            match other.name.as_str() {
              "slot" => {}
              "if" => {
                let Some(exp) = &other.exp else { return Err(self.at(other.line, other.column, "`v-if` without a condition")) };
                when = self.expr_at(exp, other.exp_line, other.exp_column)?;
              }
              name => return Err(self.at(other.line, other.column, format!("`v-{name}` on a slot's `<template>`"))),
            }
          }
          let content = self.slot_body(&child.children, prop)?;
          named.push(Tmpl::Let { name: format!("{SLOT_CONTENT_PREFIX}{name}"), expr: when, then: Box::new(Tmpl::Fragment(content)) });
        }
        None => defaults.push(child),
      }
    }
    let own = node.props.iter().find(|p| p.is_directive("slot"));
    let defaults: Vec<Node> = defaults.into_iter().cloned().collect();
    let mut out = match own {
      Some(prop) => {
        let content = self.slot_body(&defaults, prop)?;
        vec![Tmpl::Let { name: format!("{SLOT_CONTENT_PREFIX}default"), expr: Expr::Lit(Lit::Bool(true)), then: Box::new(Tmpl::Fragment(content)) }]
      }
      None => self.children(&defaults)?,
    };
    out.extend(named);
    Ok(out)
  }

  /// A slot's content with its params, `v-slot="{ item }"`, bound to the props the child hands it.
  fn slot_body(&mut self, nodes: &[Node], prop: &Prop) -> Lowered<Vec<Tmpl>> {
    let depth = self.lowerer.scope.len();
    if let Some(exp) = prop.exp.as_deref().filter(|e| !e.trim().is_empty()) {
      let arrow = self.lowerer.parsed.parse_expr_at(&format!("({exp}) => 0"), prop.exp_line, prop.exp_column.saturating_sub(1)).map_err(|message| self.at(prop.exp_line, prop.exp_column, format!("`{}`: {message}", exp.trim())))?;
      let js::Expr::Arrow(arrow) = &*arrow else { return Err(self.at(prop.exp_line, prop.exp_column, "slot params that are not a pattern")) };
      if let Some(param) = arrow.params.first() {
        bind_pattern(&mut self.lowerer, param, Expr::Var(SLOT_PROPS.to_owned()))?;
      }
    }
    let content = self.children(nodes);
    self.lowerer.scope.truncate(depth);
    content
  }

  /// An element's own `value`: the static attribute as text or `:value` as its expression.
  fn own_value(&mut self, node: &Node) -> Lowered<Option<Expr>> {
    for prop in &node.props {
      if prop.prop == "attribute" && prop.name == "value" {
        return Ok(Some(Expr::lit_str(prop.value.clone().unwrap_or_default())));
      }
      if prop.binds("value") {
        let Some(exp) = &prop.exp else { return Ok(None) };
        return Ok(Some(self.expr_at(exp, prop.exp_line, prop.exp_column)?));
      }
    }
    Ok(None)
  }

  /// `<Child :prop="x">content</Child>` with `Child` a `.vue` file the script imports: a `Tmpl::Component` named by the import's local name, which the set resolves, lowers and checks the attributes of.
  fn child_component(&mut self, node: &Node) -> Lowered<Tmpl> {
    let local = pascal(&node.tag);
    let source = find_import(self.lowerer.parsed, &local).map(|(source, _)| source);
    if !source.as_deref().is_some_and(|s| s.ends_with(".vue")) {
      return Err(self.at(node.line, node.column, format!("`<{}>`, a component the script does not import from a `.vue` file; the build lowers a Vue child and the browser mounts anything else", node.tag)));
    }
    let mut props = Vec::new();
    let mut names = Vec::new();
    for prop in &node.props {
      if prop.prop == "attribute" {
        let name = camel(&prop.name);
        props.push(Entry::Field(name.clone(), match &prop.value {
          Some(value) => Expr::lit_str(value.clone()),
          None => Expr::Lit(Lit::Bool(true)),
        }));
        names.push(name);
        continue;
      }
      match prop.name.as_str() {
        "if" | "else-if" | "else" | "for" | "on" | "slot" => {}
        "model" => {
          let Some(exp) = &prop.exp else { return Err(self.at(prop.line, prop.column, "`v-model` without a binding")) };
          let value = self.expr_at(exp, prop.exp_line, prop.exp_column)?;
          let name = match (&prop.arg, prop.arg_static) {
            (None, _) => "modelValue".to_owned(),
            (Some(arg), true) => camel(arg),
            (Some(_), false) => return Err(self.at(prop.line, prop.column, "a `v-model` whose name is an expression")),
          };
          props.push(Entry::Field(name.clone(), value));
          names.push(name);
        }
        "bind" => {
          let Some(exp) = &prop.exp else { return Err(self.at(prop.line, prop.column, "`v-bind` without a value")) };
          let value = self.expr_at(exp, prop.exp_line, prop.exp_column)?;
          match (&prop.arg, prop.arg_static) {
            (Some(arg), true) if arg == "key" => {}
            (Some(arg), true) => {
              let name = camel(arg);
              props.push(Entry::Field(name.clone(), value));
              names.push(name);
            }
            (None, _) => props.push(Entry::Spread(value)),
            (Some(_), false) => return Err(self.at(prop.line, prop.column, "a bound prop whose name is an expression")),
          }
        }
        other => return Err(self.at(prop.line, prop.column, format!("`v-{other}` on `<{}>`", node.tag))),
      }
    }
    if let Some(scope) = &self.described.scope {
      props.push(Entry::Field(snapfire_fsr_ir::render::PARENT_SCOPE_PROP.to_owned(), Expr::lit_str(scope.clone())));
    }
    let children = self.slot_contents(node)?;
    self.child_refs.push(ChildRef { local: local.clone(), attrs: names, line: node.line, column: node.column });
    Ok(Tmpl::Component { module: local, props, children, id: 0, keyed: false })
  }

  /// A placement's props as the shared placements read them: a static
  /// attribute as text, `:name` as its expression and `v-bind` as a spread.
  /// Event handlers belong to the browser and the structural directives were
  /// read by the element already.
  fn placement_attrs(&mut self, node: &Node) -> Lowered<Vec<Attr<'static>>> {
    let mut out = Vec::new();
    for prop in &node.props {
      let at = At::Line(prop.line, prop.column);
      if prop.prop == "attribute" {
        let value = match &prop.value {
          Some(text) => Value::Text(text.clone()),
          None => Value::Present,
        };
        out.push(Attr { name: prop.name.clone(), value, at });
        continue;
      }
      match prop.name.as_str() {
        "if" | "else-if" | "else" | "for" | "on" => {}
        "bind" => {
          let Some(exp) = &prop.exp else { return Err(self.at(prop.line, prop.column, "`v-bind` without an expression")) };
          let expr = self.lowerer.parsed.parse_expr_at(exp, prop.exp_line, prop.exp_column).map_err(|message| self.at(prop.exp_line, prop.exp_column, format!("`{}`: {message}", exp.trim())))?;
          match (&prop.arg, prop.arg_static) {
            (Some(name), true) => out.push(Attr { name: name.clone(), value: Value::Script(Cow::Owned(*expr)), at }),
            (None, _) => out.push(Attr { name: String::new(), value: Value::Spread(Cow::Owned(*expr)), at }),
            (Some(_), false) => return Err(self.at(prop.line, prop.column, "a bound attribute whose name is an expression")),
          }
        }
        other => return Err(self.at(prop.line, prop.column, format!("`v-{other}` on `<{}>`", node.tag))),
      }
    }
    Ok(out)
  }

  /// `<template v-if>` or `<template v-for>`: its children as one node when
  /// there is one element, else a fragment Vue wraps in anchors.
  fn template_element(&mut self, node: &Node) -> Lowered<Tmpl> {
    for prop in &node.props {
      match (prop.prop.as_str(), prop.name.as_str()) {
        ("directive", "if" | "else-if" | "else" | "for") => {}
        ("directive", "bind") if prop.binds("key") => {}
        ("directive", "slot") => return Err(self.at(prop.line, prop.column, "`v-slot`, which fills a component's slot; the build places no component inside a Vue template")),
        ("directive", other) => return Err(self.at(prop.line, prop.column, format!("`v-{other}` on `<template>`"))),
        (_, other) => return Err(self.at(prop.line, prop.column, format!("`{other}` on `<template>`, which is not an element"))),
      }
    }
    let mut children = self.children(&node.children)?;
    Ok(match children.len() {
      1 if matches!(children[0], Tmpl::Element { .. }) => children.pop().expect("one child"),
      _ => Tmpl::Fragment(children),
    })
  }

  /// `<slot />`: the island's children. A name or a bound value is refused,
  /// since an island's children fill the default slot alone and carry no
  /// scope. The fallback is never written: the server always writes the
  /// children region, empty or not.
  /// `<slot />` as the island's children; a named slot, one handing props or one with a fallback as a `SLOT_OUT_PREFIX` wrapper the renderer fills from the caller's content for it.
  fn slot(&mut self, node: &Node) -> Lowered<Tmpl> {
    let mut name = "default".to_owned();
    let mut props = Vec::new();
    for prop in &node.props {
      match (prop.prop.as_str(), prop.name.as_str()) {
        ("attribute", "name") => name = prop.value.clone().unwrap_or_else(|| "default".to_owned()),
        ("attribute", other) => props.push(Entry::Field(camel(other), match &prop.value {
          Some(value) => Expr::lit_str(value.clone()),
          None => Expr::Lit(Lit::Bool(true)),
        })),
        ("directive", "bind") => {
          let Some(exp) = &prop.exp else { return Err(self.at(prop.line, prop.column, "`v-bind` without a value")) };
          let value = self.expr_at(exp, prop.exp_line, prop.exp_column)?;
          match (&prop.arg, prop.arg_static) {
            (Some(arg), true) if arg == "name" => return Err(self.at(prop.line, prop.column, "a slot whose name is an expression")),
            (Some(arg), true) => props.push(Entry::Field(camel(arg), value)),
            (None, _) => props.push(Entry::Spread(value)),
            (Some(_), false) => return Err(self.at(prop.line, prop.column, "a slot prop whose name is an expression")),
          }
        }
        (_, other) => return Err(self.at(prop.line, prop.column, format!("`{other}` on `<slot>`"))),
      }
    }
    let fallback = self.children(&node.children)?;
    if name == "default" && props.is_empty() && fallback.is_empty() {
      return Ok(Tmpl::Slot("content".to_owned()));
    }
    Ok(Tmpl::Let { name: format!("{SLOT_OUT_PREFIX}{name}"), expr: Expr::Object(props), then: Box::new(Tmpl::Fragment(fallback)) })
  }

  fn plain_element(&mut self, node: &Node) -> Lowered<Tmpl> {
    let custom = node.tag.contains('-');
    let dynamic_class = node.props.iter().any(|p| p.binds("class"));
    let static_class = node.props.iter().find(|p| p.prop == "attribute" && p.name == "class").and_then(|p| p.value.clone());
    let mut attrs: Vec<Entry> = Vec::new();
    let mut styles: Vec<Expr> = Vec::new();
    let mut style_at: Option<usize> = None;
    let mut text: Option<Tmpl> = None;
    let mut select_model: Option<Expr> = None;
    for prop in &node.props {
      if prop.prop == "attribute" {
        match prop.name.as_str() {
          "class" if dynamic_class => {}
          "class" => attrs.push(Entry::Field("class".to_owned(), Expr::lit_str(prop.value.clone().unwrap_or_default()))),
          "style" => {
            let entries = parse_style(prop.value.as_deref().unwrap_or("")).into_iter().map(|(name, value)| Entry::Field(name, Expr::lit_str(value))).collect();
            styles.push(Expr::Object(entries));
            style_at.get_or_insert_with(|| {
              attrs.push(Entry::Field("style".to_owned(), Expr::Lit(Lit::Null)));
              attrs.len() - 1
            });
          }
          "key" | "ref" => {}
          "value" if node.tag == "textarea" => return Err(self.at(prop.line, prop.column, "`value` on a `<textarea>`, which Vue writes as its content")),
          name => attrs.push(Entry::Field(name.to_owned(), match &prop.value {
            Some(value) => Expr::lit_str(value.clone()),
            None => Expr::Lit(Lit::Bool(true)),
          })),
        }
        continue;
      }
      match prop.name.as_str() {
        "if" | "else-if" | "else" | "for" | "once" | "memo" | "cloak" => {}
        "on" => match self.on(prop) {
          Ok((event, index)) => attrs.push(Entry::Field(format!("{HANDLER_ATTR}{event}"), Expr::Lit(Lit::Int(index as i128)))),
          Err(residue) => {
            if !attrs.iter().any(|e| matches!(e, Entry::Field(n, _) if n == UNLOWERED_ATTR)) {
              attrs.push(Entry::Field(UNLOWERED_ATTR.to_owned(), Expr::lit_str(format!("{}:{}: {}", residue.line, residue.column, residue.message))));
            }
          }
        },
        "bind" => {
          if !prop.arg_static {
            return Err(self.at(prop.line, prop.column, "a bound attribute whose name is an expression"));
          }
          if prop.modifiers.iter().any(|m| m == "prop") {
            return Err(self.at(prop.line, prop.column, "`.prop`, which sets a property the markup never shows"));
          }
          let Some(exp) = &prop.exp else { return Err(self.at(prop.line, prop.column, "`v-bind` without a value")) };
          let Some(arg) = &prop.arg else {
            let value = self.expr_at(exp, prop.exp_line, prop.exp_column)?;
            attrs.push(Entry::Spread(value));
            continue;
          };
          let value = self.expr_at(exp, prop.exp_line, prop.exp_column)?;
          match arg.as_str() {
            "class" => {
              let mut items = vec![Entry::Item(value)];
              if let Some(held) = &static_class {
                items.push(Entry::Item(Expr::lit_str(held.clone())));
              }
              attrs.push(Entry::Field("class".to_owned(), Expr::Array(items)));
            }
            "style" => {
              styles.push(value);
              style_at.get_or_insert_with(|| {
                attrs.push(Entry::Field("style".to_owned(), Expr::Lit(Lit::Null)));
                attrs.len() - 1
              });
            }
            "key" | "ref" => {}
            "is" => return Err(self.at(prop.line, prop.column, "`:is`, a tag chosen at run time")),
            "value" if node.tag == "textarea" => return Err(self.at(prop.line, prop.column, "`:value` on a `<textarea>`, which Vue writes as its content")),
            other => {
              let (name, boolean) = attr_name(other, custom);
              let value = bound_attr(boolean, value);
              attrs.push(Entry::Field(name, value));
            }
          }
        }
        "html" => {
          let Some(exp) = &prop.exp else { return Err(self.at(prop.line, prop.column, "`v-html` without a value")) };
          let value = self.expr_at(exp, prop.exp_line, prop.exp_column)?;
          attrs.push(Entry::Field(RAW_ATTR.to_owned(), value));
        }
        "text" => {
          let Some(exp) = &prop.exp else { return Err(self.at(prop.line, prop.column, "`v-text` without a value")) };
          text = Some(Tmpl::Expr(self.expr_at(exp, prop.exp_line, prop.exp_column)?));
        }
        "show" => {
          let Some(exp) = &prop.exp else { return Err(self.at(prop.line, prop.column, "`v-show` without a condition")) };
          let cond = self.expr_at(exp, prop.exp_line, prop.exp_column)?;
          let hidden = Expr::Object(vec![Entry::Field("display".to_owned(), Expr::lit_str("none"))]);
          styles.push(Expr::Ternary(Box::new(cond), Box::new(Expr::Lit(Lit::Null)), Box::new(hidden)));
          style_at.get_or_insert_with(|| {
            attrs.push(Entry::Field("style".to_owned(), Expr::Lit(Lit::Null)));
            attrs.len() - 1
          });
        }
        "model" => {
          let Some(exp) = &prop.exp else { return Err(self.at(prop.line, prop.column, "`v-model` without a binding")) };
          let model = self.expr_at(exp, prop.exp_line, prop.exp_column)?;
          let kind = node.props.iter().find(|p| p.prop == "attribute" && p.name == "type").and_then(|p| p.value.clone());
          if node.props.iter().any(|p| p.binds("type")) {
            return Err(self.at(prop.line, prop.column, "`v-model` on an input whose `type` is bound"));
          }
          match (node.tag.as_str(), kind.as_deref()) {
            ("input", Some("checkbox")) => {
              let own = self.own_value(node)?.unwrap_or(Expr::Lit(Lit::Null));
              attrs.push(Entry::Field("checked".to_owned(), bound_attr(true, Expr::Builtin { name: Builtin::Checked, args: vec![model, own] })));
            }
            ("input", Some("radio")) => {
              let Some(own) = self.own_value(node)? else { return Err(self.at(prop.line, prop.column, "a radio's `v-model` without a `value`")) };
              attrs.push(Entry::Field("checked".to_owned(), bound_attr(true, Expr::Builtin { name: Builtin::LooseMatch, args: vec![model, own] })));
            }
            ("input", _) => attrs.push(Entry::Field("value".to_owned(), bound_attr(false, model))),
            ("textarea", _) => text = Some(Tmpl::Expr(model)),
            ("select", _) => select_model = Some(model),
            (tag, _) => return Err(self.at(prop.line, prop.column, format!("`v-model` on `<{tag}>`"))),
          }
        }
        "slot" => return Err(self.at(prop.line, prop.column, "`v-slot` on an element")),
        "pre" => return Err(self.at(prop.line, prop.column, "`v-pre`")),
        other => return Err(self.at(prop.line, prop.column, format!("`v-{other}`, a directive the build does not read"))),
      }
    }
    if let Some(at) = style_at {
      attrs[at] = Entry::Field("style".to_owned(), Expr::Array(styles.into_iter().map(Entry::Item).collect()));
    }
    if node.tag == "option" {
      if let (Some(model), Some(own)) = (self.select_model.clone(), self.own_value(node)?) {
        attrs.push(Entry::Field("selected".to_owned(), bound_attr(true, Expr::Builtin { name: Builtin::LooseMatch, args: vec![model, own] })));
      }
    }
    if let Some(scope) = &self.described.scope {
      attrs.push(Entry::Field(scope.clone(), Expr::Lit(Lit::Bool(true))));
    }
    let inner = match select_model {
      Some(model) => Some(model),
      None if node.tag == "select" => None,
      None => self.select_model.clone(),
    };
    let outer = std::mem::replace(&mut self.select_model, inner);
    let children = match text {
      Some(text) => Ok(vec![text]),
      None => self.children(&node.children),
    };
    self.select_model = outer;
    Ok(Tmpl::Element { tag: node.tag.clone(), attrs, children: children? })
  }
}

/// A handler's walk: the patch and statements, plus each state name written so far with the `let` holding its latest value, which later reads see.
#[derive(Default)]
struct VueWalk {
  walk: HandlerWalk,
  written: Vec<(String, String)>,
  values: usize,
}

impl VueLowerer<'_, '_> {
  /// `@event` on an element as a lowered handler: its event and index. A handler is a setup function's name, an arrow or statements over `$event`; its statements are `const`s, branches, writes to a `ref` or a store, calls to setup functions and calls to actions.
  fn on(&mut self, prop: &Prop) -> Lowered<(String, usize)> {
    let event = match (&prop.arg, prop.arg_static) {
      (Some(arg), true) => arg.to_ascii_lowercase(),
      _ => return Err(self.at(prop.line, prop.column, "`v-on` without an event name written out")),
    };
    if let Some(modifier) = prop.modifiers.iter().find(|m| !matches!(m.as_str(), "prevent" | "stop")) {
      return Err(self.at(prop.line, prop.column, format!("`.{modifier}` on `@{event}`, a modifier the build does not lower")));
    }
    let Some(exp) = &prop.exp else { return Err(self.at(prop.line, prop.column, format!("`@{event}` without a handler"))) };
    let expr = self.lowerer.parsed.parse_expr_at(exp, prop.exp_line, prop.exp_column).map_err(|message| self.at(prop.exp_line, prop.exp_column, format!("`{}`: {message}", exp.trim())))?;
    let hoisting = self.lowerer.hoisting.take();
    let was_handler = std::mem::replace(&mut self.lowerer.in_handler, true);
    let depth = self.lowerer.scope.len();
    let result = self.on_body(&expr);
    self.lowerer.scope.truncate(depth);
    self.lowerer.hoisting = hoisting;
    self.lowerer.in_handler = was_handler;
    let body = result?;
    self.handlers.push(Handler { event: event.clone(), body });
    Ok((event, self.handlers.len() - 1))
  }

  fn on_body(&mut self, expr: &js::Expr) -> Lowered<Vec<Stmt>> {
    let mut walk = VueWalk::default();
    let event = || vec![Expr::Var("$event".to_owned())];
    match unwrap_types(expr) {
      js::Expr::Arrow(arrow) => self.handler_fn(&arrow.params, arrow_body(arrow), event(), &mut walk)?,
      js::Expr::Fn(f) => {
        let Some(body) = &f.function.body else { return Err(self.lowerer.residue(f.function.span, "a handler without a body")) };
        self.handler_fn(&patterns(&f.function), FunctionBody::Block(&body.stmts), event(), &mut walk)?
      }
      js::Expr::Ident(id) if self.handler_fns.contains_key(id.sym.as_ref()) => self.call_setup(id.sym.as_ref(), event(), id.span, &mut walk)?,
      other => {
        self.lowerer.scope.push(("$event".to_owned(), Expr::Var("$event".to_owned())));
        self.handler_stmt(other, &mut walk)?;
      }
    }
    if walk.walk.patch.is_empty() && !holds_act(&walk.walk.out) {
      return Err(self.lowerer.residue(expr.span(), "a handler that sets no state and calls no action"));
    }
    walk.walk.out.push(Stmt::Return(Expr::Object(walk.walk.patch)));
    Ok(walk.walk.out)
  }

  /// A function's body with its parameters bound to `args`, in the scope that holds it.
  fn handler_fn(&mut self, params: &[js::Pat], body: FunctionBody<'_>, args: Vec<Expr>, walk: &mut VueWalk) -> Lowered<()> {
    let depth = self.lowerer.scope.len();
    for (param, arg) in params.iter().zip(args.into_iter().chain(std::iter::repeat(Expr::Lit(Lit::Null)))) {
      let js::Pat::Ident(id) = param else { return Err(self.lowerer.residue(param.span(), "a handler parameter that is not a name")) };
      self.lowerer.scope.push((id.id.sym.to_string(), arg));
    }
    let result = match body {
      FunctionBody::Expr(e) => self.handler_stmt(e, walk),
      FunctionBody::Block(stmts) => self.handler_block(stmts, walk).map(|_| ()),
    };
    self.lowerer.scope.truncate(depth);
    self.rebind(walk);
    result
  }

  /// A setup function inlined: its body reads the script's scope, with what the handler wrote so far.
  fn call_setup(&mut self, name: &str, args: Vec<Expr>, span: swc_core::common::Span, walk: &mut VueWalk) -> Lowered<()> {
    if self.calling.iter().any(|c| c == name) {
      return Err(self.lowerer.residue(span, format!("`{name}` calls itself")));
    }
    let (params, body) = self.handler_fns.get(name).cloned().expect("a setup function");
    let outer = std::mem::replace(&mut self.lowerer.scope, self.script_scope.clone());
    self.rebind(walk);
    self.calling.push(name.to_owned());
    let result = self.handler_fn(&params, body, args, walk);
    self.calling.pop();
    self.lowerer.scope = outer;
    self.rebind(walk);
    result
  }

  /// Binds each written state name to its latest value, as a holder where the scope reads it through `.value`.
  fn rebind(&mut self, walk: &VueWalk) {
    for (state, value) in &walk.written {
      let expr = match self.bound(state) {
        Some(bound) if is_holder(bound) => Self::holder(value),
        _ => Expr::Var(value.clone()),
      };
      self.lowerer.scope.push((state.clone(), expr));
    }
  }

  fn bound(&self, name: &str) -> Option<&Expr> {
    self.lowerer.scope.iter().rev().find(|(n, _)| n == name).map(|(_, e)| e)
  }

  /// The state an assignment target writes: a `ref` by its name where the scope unwraps it, `x.value` where the scope holds it.
  fn target_state(&self, target: &js::Expr, walk: &VueWalk) -> Option<String> {
    let (name, held) = match unwrap_types(target) {
      js::Expr::Ident(id) => (id.sym.to_string(), false),
      js::Expr::Member(m) => match (&*m.obj, &m.prop) {
        (js::Expr::Ident(id), js::MemberProp::Ident(prop)) if prop.sym.as_ref() == "value" => (id.sym.to_string(), true),
        _ => return None,
      },
      _ => return None,
    };
    if !self.state.contains(&name) {
      return None;
    }
    let latest = walk.written.iter().find(|(s, _)| *s == name).map_or(name.as_str(), |(_, v)| v.as_str());
    let reads = match self.bound(&name)? {
      Expr::Object(entries) if held => match entries.as_slice() {
        [Entry::Field(field, Expr::Var(v))] if field == "value" => v,
        _ => return None,
      },
      Expr::Var(v) if !held => v,
      _ => return None,
    };
    (reads == latest).then_some(name)
  }

  fn write(&mut self, state: String, value: Expr, walk: &mut VueWalk) {
    walk.walk.set(state.clone(), value);
    let Some(Entry::Field(_, held)) = walk.walk.patch.iter_mut().find(|e| matches!(e, Entry::Field(n, _) if *n == state)) else { return };
    walk.values += 1;
    let name = format!("{state}$v{}", walk.values);
    let expr = std::mem::replace(held, Expr::Var(name.clone()));
    walk.walk.out.push(Stmt::Let { name: name.clone(), expr });
    let shape = self.bound(&state).is_some_and(is_holder);
    walk.written.retain(|(s, _)| *s != state);
    walk.written.push((state.clone(), name.clone()));
    self.lowerer.scope.push((state, if shape { Self::holder(&name) } else { Expr::Var(name) }));
  }

  fn handler_block(&mut self, stmts: &[js::Stmt], walk: &mut VueWalk) -> Lowered<bool> {
    for stmt in stmts {
      match stmt {
        js::Stmt::Expr(e) => self.handler_stmt(&e.expr, walk)?,
        js::Stmt::Decl(js::Decl::Var(var)) => {
          for decl in &var.decls {
            let js::Pat::Ident(name) = &decl.name else { return Err(self.lowerer.residue(decl.span, "a destructuring in a handler")) };
            let init = decl.init.as_deref().ok_or_else(|| self.lowerer.residue(decl.span, "a declaration without a value"))?;
            let expr = self.lowerer.expr(init)?;
            let local = name.id.sym.to_string();
            walk.walk.locals += 1;
            let bound = format!("{local}${}", walk.walk.locals);
            let expr = match &walk.walk.reach {
              None => expr,
              Some(reach) => Expr::Ternary(Box::new(reach.clone()), Box::new(expr), Box::new(Expr::Lit(Lit::Null))),
            };
            self.lowerer.scope.push((local, Expr::Var(bound.clone())));
            walk.walk.out.push(Stmt::Let { name: bound, expr });
          }
        }
        js::Stmt::Return(r) if r.arg.is_none() => return Ok(true),
        js::Stmt::If(branch) => self.handler_if(branch, walk)?,
        js::Stmt::Block(block) => {
          let depth = self.lowerer.scope.len();
          let returned = self.handler_block(&block.stmts, walk);
          self.lowerer.scope.truncate(depth);
          self.rebind(walk);
          if returned? {
            return Ok(true);
          }
        }
        other => return Err(self.lowerer.residue(other.span(), "a statement a handler cannot hold; a handler is `const`s, branches, writes to a `ref` or a store, calls to setup functions and calls to actions")),
      }
    }
    Ok(false)
  }

  fn handler_if(&mut self, branch: &js::IfStmt, walk: &mut VueWalk) -> Lowered<()> {
    let cond = self.lowerer.expr(&branch.test)?;
    let outer = walk.walk.reach.clone();
    let depth = self.lowerer.scope.len();
    walk.walk.reach = Some(reach_under(&outer, cond.clone()));
    let then_returned = self.handler_branch(&branch.cons, walk);
    self.lowerer.scope.truncate(depth);
    self.rebind(walk);
    let then_returned = then_returned?;
    let mut else_returned = false;
    if let Some(alt) = &branch.alt {
      walk.walk.reach = Some(reach_under(&outer, Expr::Not(Box::new(cond.clone()))));
      let returned = self.handler_branch(alt, walk);
      self.lowerer.scope.truncate(depth);
      self.rebind(walk);
      else_returned = returned?;
    }
    walk.walk.reach = match (then_returned, else_returned) {
      (false, false) => outer,
      (true, false) => Some(reach_under(&outer, Expr::Not(Box::new(cond)))),
      (false, true) => Some(reach_under(&outer, cond)),
      (true, true) => Some(Expr::Lit(Lit::Bool(false))),
    };
    Ok(())
  }

  fn handler_branch(&mut self, stmt: &js::Stmt, walk: &mut VueWalk) -> Lowered<bool> {
    match stmt {
      js::Stmt::Block(block) => self.handler_block(&block.stmts, walk),
      other => self.handler_block(std::slice::from_ref(other), walk),
    }
  }

  fn handler_stmt(&mut self, e: &js::Expr, walk: &mut VueWalk) -> Lowered<()> {
    match e {
      js::Expr::Paren(p) => self.handler_stmt(&p.expr, walk),
      js::Expr::Unary(u) if u.op == js::UnaryOp::Void => self.handler_stmt(&u.arg, walk),
      js::Expr::Await(a) => self.handler_stmt(&a.arg, walk),
      js::Expr::Seq(seq) => seq.exprs.iter().try_for_each(|e| self.handler_stmt(e, walk)),
      js::Expr::Assign(assign) => {
        let target = match &assign.left {
          js::AssignTarget::Simple(js::SimpleAssignTarget::Ident(id)) => js::Expr::Ident(id.id.clone()),
          js::AssignTarget::Simple(js::SimpleAssignTarget::Member(m)) => js::Expr::Member(m.clone()),
          js::AssignTarget::Simple(js::SimpleAssignTarget::Paren(p)) => (*p.expr).clone(),
          _ => return Err(self.lowerer.residue(assign.span, "an assignment a handler cannot make")),
        };
        let Some(state) = self.target_state(&target, walk) else {
          return Err(self.lowerer.residue(assign.span, "an assignment to something other than a `ref` or a store this component holds"));
        };
        let value = match assign.op.to_update() {
          None => self.lowerer.expr(&assign.right)?,
          Some(op) => self.lowerer.expr(&js::Expr::Bin(js::BinExpr { span: assign.span, op, left: Box::new(target), right: assign.right.clone() }))?,
        };
        self.write(state, value, walk);
        Ok(())
      }
      js::Expr::Update(update) => {
        let Some(state) = self.target_state(&update.arg, walk) else {
          return Err(self.lowerer.residue(update.span, "an update to something other than a `ref` or a store this component holds"));
        };
        let op = match update.op {
          js::UpdateOp::PlusPlus => js::BinaryOp::Add,
          js::UpdateOp::MinusMinus => js::BinaryOp::Sub,
        };
        let one = js::Expr::Lit(js::Lit::Num(js::Number { span: update.span, value: 1.0, raw: None }));
        let value = self.lowerer.expr(&js::Expr::Bin(js::BinExpr { span: update.span, op, left: update.arg.clone(), right: Box::new(one) }))?;
        self.write(state, value, walk);
        Ok(())
      }
      js::Expr::Call(call) => {
        let js::Callee::Expr(callee) = &call.callee else { return Err(self.lowerer.residue(call.span, "a call a handler cannot make")) };
        match unwrap_types(callee) {
          js::Expr::Ident(id) => {
            let name = id.sym.to_string();
            if self.handler_fns.contains_key(&name) {
              let args = call.args.iter().map(|a| self.lowerer.expr(&a.expr)).collect::<Lowered<Vec<_>>>()?;
              return self.call_setup(&name, args, call.span, walk);
            }
            if let Some(action) = action_alias_of(self.lowerer.parsed, &name) {
              let input = match call.args.first() {
                Some(arg) => self.lowerer.expr(&arg.expr)?,
                None => Expr::Object(Vec::new()),
              };
              walk.walk.act(action, input);
              return Ok(());
            }
            Err(self.lowerer.residue(id.span, format!("a call to `{name}`, which is not an action or a function this component declares")))
          }
          js::Expr::Member(m) => {
            let method = match &m.prop {
              js::MemberProp::Ident(i) => i.sym.to_string(),
              _ => String::new(),
            };
            if method == "preventDefault" || method == "stopPropagation" {
              return Ok(());
            }
            if let Some(action) = generated_action_of(self.lowerer.parsed, callee) {
              let input = match call.args.first() {
                Some(arg) => self.lowerer.expr(&arg.expr)?,
                None => Expr::Object(Vec::new()),
              };
              walk.walk.act(action, input);
              return Ok(());
            }
            Err(self.lowerer.residue(call.span, format!("`.{method}()` in a handler; a handler is `const`s, branches, writes to a `ref` or a store, calls to setup functions and calls to actions")))
          }
          other => Err(self.lowerer.residue(other.span(), "a call a handler cannot make")),
        }
      }
      other => Err(self.lowerer.residue(other.span(), "a statement a handler cannot hold; a handler is `const`s, branches, writes to a `ref` or a store, calls to setup functions and calls to actions")),
    }
  }
}

fn is_holder(expr: &Expr) -> bool {
  matches!(expr, Expr::Object(entries) if matches!(entries.as_slice(), [Entry::Field(field, _)] if field == "value"))
}

enum Branch {
  If(Expr),
  ElseIf(Expr),
  Else,
}

/// The attribute a bound name prints as and whether Vue treats it as a
/// boolean. On an ordinary element Vue maps the few camelCase spellings it
/// knows and lowercases the rest to decide that; a boolean prints under the
/// mapped name and anything else as written. A custom element's attributes
/// are taken as given.
fn attr_name(arg: &str, custom: bool) -> (String, bool) {
  if custom {
    return (arg.to_owned(), false);
  }
  let mapped = match arg {
    "className" => "class".to_owned(),
    "htmlFor" => "for".to_owned(),
    "httpEquiv" => "http-equiv".to_owned(),
    "acceptCharset" => "accept-charset".to_owned(),
    other => other.to_ascii_lowercase(),
  };
  match BOOLEAN.contains(&mapped.as_str()) {
    true => (mapped, true),
    false => (arg.to_owned(), false),
  }
}

/// A bound value as Vue prints it: a boolean attribute is present when the
/// value is truthy or the empty string; any other attribute takes the value's
/// text and is absent for `null`.
fn bound_attr(boolean: bool, value: Expr) -> Expr {
  if boolean {
    let empty = Expr::Compare(CompareOp::Eq, Box::new(Expr::Str(Box::new(value.clone()))), Box::new(Expr::lit_str("")));
    let present = Expr::Logic(LogicOp::Or, Box::new(value), Box::new(empty));
    return Expr::Ternary(Box::new(present), Box::new(Expr::Lit(Lit::Bool(true))), Box::new(Expr::Lit(Lit::Null)));
  }
  let absent = Expr::Compare(CompareOp::Eq, Box::new(value.clone()), Box::new(Expr::Lit(Lit::Null)));
  Expr::Ternary(Box::new(absent), Box::new(Expr::Lit(Lit::Null)), Box::new(Expr::Str(Box::new(value))))
}

/// A static `style` as Vue's `parseStringStyle` reads it: declarations
/// split on `;` outside parentheses, each `name:value` trimmed.
fn parse_style(text: &str) -> Vec<(String, String)> {
  let mut out = Vec::new();
  let mut depth = 0usize;
  let mut start = 0;
  let mut pieces: Vec<&str> = Vec::new();
  for (i, b) in text.bytes().enumerate() {
    match b {
      b'(' => depth += 1,
      b')' => depth = depth.saturating_sub(1),
      b';' if depth == 0 => {
        pieces.push(&text[start..i]);
        start = i + 1;
      }
      _ => {}
    }
  }
  pieces.push(&text[start..]);
  for piece in pieces {
    let Some((name, value)) = piece.split_once(':') else { continue };
    let (name, value) = (name.trim(), value.trim());
    if !name.is_empty() {
      out.push((name.to_owned(), value.to_owned()));
    }
  }
  out
}

/// Puts `branch` at the end of a `v-if` chain; false when the chain already ends in a `v-else`.
fn attach(chain: &mut Tmpl, branch: Tmpl) -> bool {
  let Tmpl::If { r#else, .. } = chain else { return false };
  match r#else {
    None => {
      *r#else = Some(Box::new(branch));
      true
    }
    Some(next) => attach(next, branch),
  }
}

/// `v-for`'s two halves: the aliases before `in` or `of`, parentheses
/// stripped and the source after it.
fn for_parts(exp: &str) -> Option<(&str, &str)> {
  let trimmed = exp.trim();
  let (lhs, rhs) = [" in ", " of "].iter().filter_map(|word| trimmed.find(word).map(|at| (&trimmed[..at], &trimmed[at + word.len()..]))).min_by_key(|(lhs, _)| lhs.len())?;
  let lhs = lhs.trim();
  let lhs = lhs.strip_prefix('(').and_then(|l| l.strip_suffix(')')).unwrap_or(lhs);
  let rhs = rhs.trim();
  (!lhs.is_empty() && !rhs.is_empty()).then_some((lhs, rhs))
}

/// The expression under the TypeScript forms that change nothing at run time.
fn unwrap_types(expr: &js::Expr) -> &js::Expr {
  match expr {
    js::Expr::TsAs(a) => unwrap_types(&a.expr),
    js::Expr::TsNonNull(a) => unwrap_types(&a.expr),
    js::Expr::TsSatisfies(a) => unwrap_types(&a.expr),
    js::Expr::TsTypeAssertion(a) => unwrap_types(&a.expr),
    js::Expr::Paren(p) => unwrap_types(&p.expr),
    other => other,
  }
}
