//! Reads a `.tsx` module and lowers a component to a render tree: the JSX
//! becomes `Tmpl`, the expressions inside it become the IR and a component it
//! renders is lowered in turn under its own module id. Local imports are
//! followed on first use, so a helper module that also imports a browser
//! library costs nothing until a render reads from it. Event handlers, inner
//! functions, effects and `useState` setters are dropped, since the browser
//! mounts the same module over the output; `useState(x)` reads as `x`, which
//! is what a first render sees, `useMemo(f)` as `f()` and `useRef(x)` as
//! `{ current: x }`. A caller's children become a `Slot` where the callee
//! writes `{children}`.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::rc::Rc;

use snapfire_compiler_wire::Described;
use snapfire_fsr_ir::ast::{ArithOp, Builtin, CompareOp, Component, Consts, Entry, Expr, Handler, Lit, LogicOp, Owner, Stmt, Tmpl};
use snapfire_fsr_ir::render::{html_attr_name, CONTEXT_PREFIX, HANDLER_ATTR, KEY_ATTR, RAW_ATTR, SERVER_MODE, TYPES_PROP, UNLOWERED_ATTR};
use snapfire_fsr_ir::Reach;
use oxc_allocator::Allocator;
use oxc_span::{GetSpan, Span};
use oxc_ast::ast as js;
use crate::syntax::Walk;

use crate::assets::{self, AssetResolver, ImageFacts, ImageRequest, NoAssets};
use crate::hoist::{self, Candidates, Hook, Rewrite};
use crate::placements::{self, At, Attr, Placer, Value};
use crate::{lower_actions_in, lower_handlers_in, lower_loader_in, lower_middleware_in, lower_of_data_in, lower_paths_in, parse_with, prop_name, Lowered, LowerError, Lowerer, LoweredAction, LoweredHandler, Parsed, Placement, Resolved, Residue, SessionDefaults, Unresolved, EXT_DIR};
use snapfire_fsr_ir::Body;

mod block;
mod body;
mod classes;
mod control;
mod derive;
mod effects;
mod elements;
mod find;
mod functions;
mod globals;
mod handlers;
mod hooks;
mod jsx;
mod load;
mod loaded;
mod lowerer;
mod markup;
mod meanings;
mod places;
mod premake;
mod rebind;
mod tree;
mod values;

pub(crate) use self::block::*;
pub(crate) use self::control::*;
use self::derive::*;
pub(crate) use self::find::*;
pub(crate) use self::functions::*;
pub(crate) use self::jsx::*;
use self::lowerer::*;
pub(crate) use self::rebind::*;
use self::tree::*;
pub(crate) use self::values::*;

/// An import of components another framework mounts: the declaration as
/// written and each binding it makes, as the local name and the module id.
#[non_exhaustive]
pub struct ForeignImport {
  pub text: String,
  pub bindings: Vec<(String, String)>,
}

/// The cursor over one application: parsed files, finished components and the
/// resolution stack that turns recursion into a diagnostic.
pub(crate) trait HeapMarked {
  fn mark(&mut self);
}

impl HeapMarked for Body {
  fn mark(&mut self) {
    self.insert(0, crate::heap_marker());
  }
}

impl HeapMarked for Option<Body> {
  fn mark(&mut self) {
    if let Some(body) = self {
      body.mark();
    }
  }
}

impl HeapMarked for Vec<crate::LoweredAction> {
  fn mark(&mut self) {
    self.iter_mut().for_each(|action| action.body.mark());
  }
}

impl HeapMarked for Vec<crate::LoweredHandler> {
  fn mark(&mut self) {
    self.iter_mut().for_each(|handler| handler.body.mark());
  }
}

impl HeapMarked for Vec<String> {
  fn mark(&mut self) {}
}

/// What a module is parsed from: the name it is parsed under, its text as the
/// build reads it, whether it holds JSX, the file it was written from and
/// whether it is a Vue script whose `this` is named.
struct Source {
  name: String,
  text: String,
  tsx: bool,
  origin: Option<Rc<crate::Origin>>,
  vue_this: bool,
}

impl Source {
  fn parse<'a>(&self, arena: &'a Allocator) -> Result<Parsed<'a>, LowerError> {
    let vue_this = self.vue_this;
    crate::parse_into(arena, &self.name, &self.text, self.tsx, self.origin.clone(), |arena, _, program| {
      if vue_this {
        crate::vue::name_this(arena, program);
      }
    })
  }
}

#[non_exhaustive]
pub struct ComponentSet {
  app: PathBuf,
  /// The reaches these pages may use besides `Render` and `Body`: [`Reach::Terminal`], [`Reach::Later`] and [`Reach::Process`] for a terminal page, none for a web page.
  reaches: Vec<Reach>,
  /// What each loaded module is parsed from; a module is parsed again each time a call reads it.
  sources: HashMap<String, Rc<Source>>,
  /// Where an imported image or font is looked up.
  assets: Rc<dyn AssetResolver>,
  /// Whether an `<img>` whose `src` is an imported asset's is lowered as a
  /// `Picture`.
  rewrite_images: bool,
  /// The head rows each lowered module asks for, its own and those of every
  /// component it places, by module id.
  pub heads: HashMap<String, Vec<HeadRow>>,
  /// Modules the caller supplies rather than the app directory holding, so a
  /// body may call into one the build has not written yet.
  provided: HashMap<String, String>,
  /// The module-level constants the plan names, so a body reading one carries
  /// a reference rather than a copy of it.
  pub consts: Consts,
  defaults: SessionDefaults,
  pub components: Vec<(String, Component)>,
  /// Modules that did not lower and files that did not parse, by the error
  /// they gave, so a second page reaching one is answered without reading or
  /// lowering it again.
  failed: HashMap<String, LowerError>,
  resolving: Vec<String>,
  /// Module functions found calling themselves while their body lowered, wrapped as `Expr::Fix` once lowered.
  recursive: Vec<String>,
  /// Modules that are layouts: their `children` is the child segment, placed
  /// inside `<sf-s>` so the browser can adopt it without reconciling it.
  pub layouts: Vec<String>,
  /// Per layout module, the slots its `slots/` directory declares, so a prop
  /// of that name is a region rather than a value.
  pub slots: Vec<(String, Vec<String>)>,
  /// The source rewrites the bundle needs, one per component with a hoist.
  pub rewrites: Vec<Rewrite>,
  /// Per lowered module, whether it is pure: no state, no handler, no island,
  /// no slot, nothing ambient and every component it renders pure too. A
  /// pure component inside a static subtree is rendered into the chunk.
  pub pure: HashMap<String, bool>,
  /// The native pairs declared under `ext/`, `module.member` and reach, as
  /// their declarations are met.
  pub natives: Vec<(String, Reach)>,
  /// Per lowered module, the render-path calls the browser still makes:
  /// `file:line:column` of each candidate that was not hoisted.
  pub remaining: Vec<(String, String)>,
  /// Components in a language the build does not read, a `.vue` file among
  /// them, as `file#export`: placed as islands, mounted rather than hydrated,
  /// compiled for the browser by whichever plugin claims the extension.
  pub foreign: Vec<String>,
  /// Per module, the kinds of the props the first file to place it passed, where every placement there agreed; read when the module is lowered.
  placement_kinds: HashMap<String, Vec<(String, crate::Kind)>>,
  /// Per lowered module, the kinds of its props: from the props parameter's type and from the placements, by prop name.
  prop_kinds: HashMap<String, Vec<(String, crate::Kind)>>,
  /// Per lowered module, the prop kinds its lowering read, so a placement passing another kind for one is reported at that placement.
  relied: HashMap<String, Vec<(String, crate::Kind)>>,
  /// Per lowered module, each value only the browser knows that the server left to it, as `file:line:column: reason`: text the browser fills in at hydration and a prop the browser computes for a placement.
  pub browser_texts: Vec<(String, String)>,
  /// Whether a component is being lowered, which is where a module-level `let` a function writes has a binding; a loader or an action reading one is residue.
  lowering_component: bool,
  /// Each module-level `let` a function writes that a component read: its store key, `file#name`, with the file and name declaring it.
  module_lets: Vec<(String, String, String)>,
  /// The continuations of a terminal page's module helpers as they lower, which the component being lowered takes as handlers.
  continuations: Vec<Handler>,
  /// Each arrow passed as a prop to a component that renders in the browser: its file, its byte range and the name the rewrite exports it under.
  fn_exports: Vec<(String, std::ops::Range<usize>, String)>,
  /// Each function a file declares without exporting that was passed as a prop to a component that renders in the browser: file and name, which the rewrite exports.
  fn_names: Vec<(String, String)>,
  /// Every file the browser imports a function prop from.
  fn_files: Vec<String>,
  /// Every class a file this set read declares, shared with each lowerer so a call site dispatches on the classes known when it lowers.
  pub(crate) classes: Rc<std::cell::RefCell<crate::classes::ClassTable>>,
  /// The classes whose members are lowered into `consts`, or are being.
  classes_lowered: HashSet<String>,
  /// The files whose imports were read for the classes they declare.
  class_graphs: HashSet<String>,
  /// What each type alias resolved so far stands for as a kind, by `file#name`.
  aliases: HashMap<String, Option<crate::AliasKind>>,
  parses: std::cell::RefCell<HashMap<String, Rc<Cached>>>,
  /// The markup helpers placed by their own recursion, by `file#name`, whose props are their arguments by position.
  positional: HashSet<String>,
  /// Components of another framework a React component renders inline, as
  /// `file#export`: lowered to the empty region `Mount` writes, which the
  /// browser fills through the module's registry entry.
  pub mounted: Vec<String>,
  /// Each file's module-level names and exports resolved, built once per file.
  meanings: HashMap<String, Rc<crate::meaning::Meanings>>,
  /// Files whose meanings are being built, so an import cycle resolves to the module's own bindings.
  meaning_in_progress: Vec<String>,
  /// Foreign components the plugin described, by file, which the set lowers
  /// through the framework's own front end instead of leaving foreign.
  described: HashMap<String, Described>,
  /// Described components that did not lower, with why: each stays foreign
  /// and the report says so.
  pub foreign_residue: Vec<(String, Residue)>,
  /// Components that did not lower, each with the residue that stopped it,
  /// placed by one that did. Only the browser renders one: from composition
  /// it is an island that mounts fresh.
  pub browser_only: Vec<(String, Residue)>,
  /// Files the plugin refused to describe, with what it said, so a placement
  /// of one is foreign for that reason.
  undescribed: HashMap<String, String>,
  /// The scripts of components the plugin would not describe, so the
  /// modules they import still reach the browser.
  scripts_only: HashMap<String, snapfire_compiler_wire::Script>,
  /// Set when a component was met while it was still being lowered. Every
  /// hydrate and purity verdict read across that cycle is provisional until
  /// [`Self::settle`].
  cyclic: bool,
  /// Per module with a purity verdict, whether it holds no browser state: the
  /// half of the verdict that does not depend on the components it renders.
  stateless: HashMap<String, bool>,
  /// Modules some placement passes a prop the browser can change, so nothing
  /// they hoist or chunk may read their props.
  moving_props: HashSet<String>,
  /// Per lowered module, whether it keys a hoist, a chunk or a region
  /// anywhere below it, which makes a placement of it keyed. A module met in
  /// a cycle before its verdict is in counts as keyed, which both halves
  /// agree on since the flag is decided once.
  keys: HashMap<String, bool>,
  /// Custom element tags with a shadow template, to the template's module. A
  /// placement of one is marked, so the server writes the template inside it.
  elements: Rc<HashMap<String, String>>,
  /// Writes the TSX module the set lowers for a file in [`TRANSFORMED`].
  transform: Option<Rc<Transform>>,
  /// A transformed file lowered again with some elements placed as other
  /// components, for a placement that passes it `components`: by the id the
  /// placement names, the file and the elements it replaces.
  variants: HashMap<String, (String, Vec<Override>)>,
}

/// The extensions of files the set lowers through the module a [`Transform`]
/// writes from them, rather than reading them as TypeScript.
pub const TRANSFORMED: &[&str] = &["mdx", "md"];

/// Whether `file` is written in a language a [`Transform`] turns into TSX.
pub fn is_transformed(file: &str) -> bool {
  let file = file.split_once('#').map(|(file, _)| file).unwrap_or(file);
  let file = file.rsplit_once('~').map(|(file, _)| file).unwrap_or(file);
  Path::new(file).extension().and_then(|e| e.to_str()).is_some_and(|ext| TRANSFORMED.contains(&ext.to_ascii_lowercase().as_str()))
}

/// Writes the TSX module for `file`, whose text is `source`, with each
/// element in `overrides` placed as the component it names.
pub type Transform = dyn Fn(&str, &str, &[Override]) -> Result<Transformed, String>;

/// One element of a transformed file placed as a component other than the
/// application's map gives: `export` of `specifier`, relative to the file.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub struct Override {
  pub key: String,
  pub specifier: String,
  pub export: String,
}

/// The module a [`Transform`] wrote and where its runs came from.
#[non_exhaustive]
pub struct Transformed {
  pub source: String,
  pub origin: Option<crate::Origin>,
}

impl Transformed {
  pub fn new(source: String, origin: Option<crate::Origin>) -> Self {
    Self { source, origin }
  }
}

impl ComponentSet {
  pub fn new(app: &Path) -> Self {
    Self { app: app.to_path_buf(), reaches: Vec::new(), sources: HashMap::new(), assets: Rc::new(NoAssets), rewrite_images: true, heads: HashMap::new(), provided: HashMap::new(), consts: Consts::new(), defaults: SessionDefaults::new(), components: Vec::new(), failed: HashMap::new(), resolving: Vec::new(), recursive: Vec::new(), layouts: Vec::new(), slots: Vec::new(), rewrites: Vec::new(), pure: HashMap::new(), natives: Vec::new(), remaining: Vec::new(), foreign: Vec::new(), mounted: Vec::new(), described: HashMap::new(), meanings: HashMap::new(), meaning_in_progress: Vec::new(), foreign_residue: Vec::new(), browser_only: Vec::new(), undescribed: HashMap::new(), scripts_only: HashMap::new(), cyclic: false, stateless: HashMap::new(), moving_props: HashSet::new(), keys: HashMap::new(), elements: Rc::default(), transform: None, variants: HashMap::new(), placement_kinds: HashMap::new(), prop_kinds: HashMap::new(), relied: HashMap::new(), browser_texts: Vec::new(), lowering_component: false, module_lets: Vec::new(), continuations: Vec::new(), fn_exports: Vec::new(), fn_names: Vec::new(), fn_files: Vec::new(), classes: Rc::default(), classes_lowered: HashSet::new(), class_graphs: HashSet::new(), aliases: HashMap::new(), parses: std::cell::RefCell::new(HashMap::new()), positional: HashSet::new() }
  }

  /// Where an imported image or font is looked up, and whether a plain
  /// `<img>` of one is rewritten.
  /// Lowers for `fsr dev --terminal`, where the process is the viewer's: a
  /// render may read the clock, which a web render leaves to the browser.
  pub fn for_terminal(mut self) -> Self {
    self.reaches = vec![Reach::Terminal, Reach::Later, Reach::Process];
    self.provided.insert(crate::terminal::INK_FILE.to_owned(), crate::terminal::INK_SOURCE.to_owned());
    for (_, file, source) in crate::terminal::COMPANIONS {
      self.provided.insert(file.to_owned(), source.to_owned());
    }
    self
  }

  /// Whether `file` draws in a terminal: it or a local file it reaches imports a value from Ink's API, which only a terminal renders.
  pub fn draws_in_terminal(&mut self, file: &str) -> bool {
    self.local_reach(file).iter().any(|reached| self.imports_value_from(reached, crate::terminal::INK_MODULE))
  }

  /// Whether these pages may use `reach`.
  fn reaches(&self, reach: Reach) -> bool {
    reach.needs().is_none() || self.reaches.contains(&reach)
  }

  pub fn with_assets(mut self, resolver: Rc<dyn AssetResolver>, rewrite_images: bool) -> Self {
    self.assets = resolver;
    self.rewrite_images = rewrite_images;
    self
  }

  /// What writes the module for an `.mdx` or `.md` file. Without one, such a
  /// file does not parse and the error says which feature reads it.
  pub fn with_transform(mut self, transform: Rc<Transform>) -> Self {
    self.transform = Some(transform);
    self
  }

  /// The custom elements whose shadow template the build lowers, by tag.
  pub fn with_elements(mut self, elements: HashMap<String, String>) -> Self {
    self.elements = Rc::new(elements);
    self
  }

  /// What the framework's plugin said `file` is, so a placement of it lowers
  /// through that framework's front end rather than staying foreign.
  pub fn describe(&mut self, file: impl Into<String>, described: Described) {
    self.described.insert(file.into(), described);
  }

  /// Every file the plugin described, with its description.
  pub fn described(&self) -> impl Iterator<Item = (&str, &Described)> {
    self.described.iter().map(|(file, described)| (file.as_str(), described))
  }

  /// The plugin refused to describe `file` and said `why`: a placement of it
  /// stays foreign for that reason.
  pub fn undescribed(&mut self, file: impl Into<String>, why: impl Into<String>) {
    self.undescribed.insert(file.into(), why.into());
  }

  /// The script of `file`, which the plugin would not describe, read only
  /// for what it imports.
  pub fn script_only(&mut self, file: impl Into<String>, script: snapfire_compiler_wire::Script) {
    self.scripts_only.insert(file.into(), script);
  }

  /// Whether `module`'s file is one the plugin described.
  pub fn is_described(&self, module: &str) -> bool {
    let file = module.split_once('#').map(|(file, _)| file).unwrap_or(module);
    self.described.contains_key(file)
  }

  /// A module the set resolves and reads from `source` rather than from disk.
  /// `generated/head.ts` is provided this way: its content is fixed, so a body
  /// calls its helpers on a checkout where nothing has been generated yet.
  pub fn provide(mut self, file: impl Into<String>, source: impl Into<String>) -> Self {
    self.provided.insert(file.into(), source.into());
    self
  }

  /// The session defaults every body lowers with.
  pub fn with_defaults(mut self, defaults: SessionDefaults) -> Self {
    self.defaults = defaults;
    self
  }

  /// Lowers the exported `load` of a loader module under the app, following
  /// the imports it calls.
  pub fn lower_loader(&mut self, file: &str) -> Result<Body, LowerError> {
    self.resolving_loop(file, |parsed, defaults, resolved| lower_loader_in(parsed, defaults, resolved))
  }

  /// The loader module's `meta`, when it exports one.
  pub fn lower_meta(&mut self, file: &str) -> Result<Option<Body>, LowerError> {
    self.lower_of_data(file, "meta")
  }

  /// The `meta` or `store` export of a loader module, as a function of `{ data }`.
  pub(crate) fn lower_of_data(&mut self, file: &str, export: &str) -> Result<Option<Body>, LowerError> {
    self.resolving_loop(file, |parsed, defaults, resolved| lower_of_data_in(parsed, defaults, resolved, export))
  }

  /// The loader module's `store`, when it exports one.
  pub fn lower_store(&mut self, file: &str) -> Result<Option<Body>, LowerError> {
    self.lower_of_data(file, "store")
  }

  /// The loader module's `defaults`, when it exports one: seeds a browser write outranks.
  pub fn lower_defaults(&mut self, file: &str) -> Result<Option<Body>, LowerError> {
    self.lower_of_data(file, "defaults")
  }

  /// The store keys `file` reads through `useStore`, for an island the server
  /// does not render, so a first wave can hold its mount while a key it reads
  /// is pending. A key is a literal or a `key()` the build follows through an
  /// import; a module with a key it cannot read reads nothing the build knows.
  pub fn store_reads(&mut self, file: &str) -> Vec<String> {
    if let Some(described) = self.described.get(file).cloned() {
      if self.parse_described(file, &described).is_err() {
        return Vec::new();
      }
    }
    let meanings = self.meanings(file);
    let read = self.resolving_loop(file, |parsed, defaults, resolved| {
      let mut calls = StoreCalls { meanings: &meanings, build: crate::syntax::Build::new(parsed.arena()), args: Vec::new() };
      parsed.module().walk(&mut calls);
      let mut keys = Vec::new();
      for arg in &calls.args {
        let mut lowerer = Lowerer::new(parsed, defaults).resolved(resolved);
        match lowerer.expr(arg) {
          Ok(Expr::Lit(Lit::Str(key))) => keys.push(key),
          Ok(_) => return Ok(Vec::new()),
          Err(residue) => return Err((residue.into(), lowerer.unbound.take())),
        }
      }
      keys.sort();
      keys.dedup();
      Ok(keys)
    });
    read.unwrap_or_default()
  }

  /// The `derive` calls of `file`. One written at module level whose compute
  /// lowers is the host's to apply. One inside a function is the browser's
  /// alone and says where and why, as is one whose compute does not lower. A
  /// key or a source the build cannot read fails the build, since no island
  /// reading the key could be held for it.
  pub fn lower_derives(&mut self, file: &str) -> Result<Vec<LoweredDerive>, LowerError> {
    self.load(file)?;
    let meanings = self.meanings(file);
    let parsed_cached = self.parsed(file);
    let parsed = parsed_cached.get();
    let module_level: Vec<Span> = parsed
      .module()
      .body
      .iter()
      .filter_map(|item| match item {
        js::Statement::ExpressionStatement(stmt) => match &stmt.expression {
          js::Expression::CallExpression(call) => Some(call.span),
          _ => None,
        },
        _ => None,
      })
      .collect();
    let mut calls = DeriveCalls { meanings: &meanings, build: crate::syntax::Build::new(parsed.arena()), calls: Vec::new() };
    parsed.module().walk(&mut calls);
    let mut resolved = Resolved { globals: Vec::new(), global_kinds: Vec::new(), global_formatters: Vec::new(), natives: self.natives.clone(), classes: Some(self.classes.clone()), type_kinds: self.alias_kinds(file), reaches: self.reaches.clone() };
    let mut rows = Vec::new();
    for call in &calls.calls {
      let at = parsed.residue(call.span, "");
      let at = format!("{}:{}:{}", at.file, at.line, at.column);
      let nested = !module_level.contains(&call.span);
      loop {
        let defaults = self.defaults.clone();
        match lower_derive(&parsed, &defaults, &resolved, call, nested) {
          Ok(row) => {
            rows.push(LoweredDerive { at: (row.body.is_none()).then(|| at.clone()), ..row });
            break;
          }
          Err(failed) => {
            if let Some(name) = failed.unbound.filter(|name| meanings.bindings.contains_key(name) && !resolved.globals.iter().any(|(n, _)| n == name)) {
              if let Some((expr, key)) = self.global(file, &name)? {
                if let Some(kind) = self.global_kind(file, &name)? {
                  resolved.global_kinds.push((name.clone(), kind));
                }
                if let Some(kind) = self.global_return_kind(file, &name)? {
                  resolved.global_kinds.push((crate::returned_by(&name), kind));
                }
                if let Some(made) = self.global_formatter(file, &name)? {
                  resolved.global_formatters.push((name.clone(), made));
                }
                resolved.globals.push((name, self.name_if_large(key, expr)));
                resolved.natives = self.natives.clone();
                continue;
              }
            }
            match failed.compute {
              Some((key, sources)) => rows.push(LoweredDerive { key, sources, body: None, at: Some(at.clone()), why: Some(failed.residue.message) }),
              None => return Err(failed.residue.into()),
            }
            break;
          }
        }
      }
    }
    Ok(rows)
  }

  /// A described component's script, parsed under its own file name with its
  /// lines where the file has them.
  fn parse_described(&mut self, file: &str, described: &Described) -> Result<(), LowerError> {
    if self.sources.contains_key(file) {
      return Ok(());
    }
    let (source, line) = match &described.script {
      Some(script) => (script.content.clone(), script.line as usize),
      None => (String::new(), 1),
    };
    let padded = format!("{}{source}", "\n".repeat(line.saturating_sub(1)));
    let vue_this = described.script.as_ref().is_some_and(|script| script.plain && !script.setup) && file.ends_with(".vue");
    self.keep(file, Source { name: file.to_owned(), text: padded, tsx: false, origin: None, vue_this })
  }

  /// The page loader module's `paths`, when it exports one.
  pub fn lower_paths(&mut self, file: &str) -> Result<Option<Body>, LowerError> {
    self.resolving_loop(file, |parsed, defaults, resolved| lower_paths_in(parsed, defaults, resolved))
  }

  /// A card image module's `images` export, or `generateImageMetadata`.
  pub fn lower_images(&mut self, file: &str) -> Result<Option<Body>, LowerError> {
    self.resolving_loop(file, |parsed, defaults, resolved| crate::lower_images_in(parsed, defaults, resolved))
  }

  pub fn lower_actions(&mut self, file: &str) -> Result<Vec<LoweredAction>, LowerError> {
    self.resolving_loop(file, |parsed, defaults, resolved| lower_actions_in(parsed, defaults, resolved))
  }

  pub fn lower_handlers(&mut self, file: &str) -> Result<Vec<LoweredHandler>, LowerError> {
    self.resolving_loop(file, |parsed, defaults, resolved| lower_handlers_in(parsed, defaults, resolved))
  }

  pub fn lower_theme(&mut self, file: &str) -> Result<Body, LowerError> {
    self.resolving_loop(file, |parsed, defaults, resolved| crate::lower_theme_in(parsed, defaults, resolved))
  }

  /// A layout's or a page's `export const theme`: a theme's name or `{ name, force: true }`
  /// for one the levels below it and the request's `theme.ts` keep. `None` when it sets none.
  /// Anything but a literal is refused, since the build reads it rather than running it.
  pub fn theme_export(&mut self, file: &str) -> Result<Option<ThemeExport>, LowerError> {
    self.load(file)?;
    let parsed_cached = self.parsed(file);
    let parsed = parsed_cached.get();
    for item in &parsed.module().body {
      let js::Statement::ExportDeclaration(export) = item else { continue };
      let js::Declaration::VariableDeclaration(var) = &export.declaration else { continue };
      for decl in &var.declarations {
        let (js::BindingPattern::BindingIdentifier(name), Some(init)) = (&decl.id, decl.init.as_ref()) else { continue };
        if name.name.as_str() != "theme" {
          continue;
        }
        let init = unwrap_literal(init);
        let refused = || LowerError::from(parsed.residue(init.span(), "`theme` is a theme's name or `{ name: \"<name>\", force: true }`, written as a literal"));
        let text = |expr: &js::Expression| match unwrap_literal(expr) {
          js::Expression::StringLiteral(s) => Some(crate::lossy(s.value)),
          js::Expression::TemplateLiteral(tpl) if tpl.expressions.is_empty() && tpl.quasis.len() == 1 => tpl.quasis[0].value.cooked.map(crate::lossy),
          _ => None,
        };
        if let Some(name) = text(init) {
          return Ok(Some(ThemeExport { name, force: false }));
        }
        let js::Expression::ObjectExpression(object) = init else { return Err(refused()) };
        let mut out = ThemeExport { name: String::new(), force: false };
        for prop in &object.properties {
          let js::ObjectPropertyKind::ObjectProperty(prop) = prop else { return Err(refused()) };
          let crate::syntax::Prop::KeyValue(kv) = crate::syntax::prop_of(prop) else { return Err(refused()) };
          let Some(key) = prop_name(&kv.key, kv.computed) else { return Err(refused()) };
          match (key.as_str(), unwrap_literal(&kv.value)) {
            ("name", value) => out.name = text(value).ok_or_else(refused)?,
            ("force", js::Expression::BooleanLiteral(b)) => out.force = b.value,
            _ => return Err(refused()),
          }
        }
        if out.name.is_empty() {
          return Err(refused());
        }
        return Ok(Some(out));
      }
    }
    Ok(None)
  }

  pub fn lower_middleware(&mut self, file: &str) -> Result<Body, LowerError> {
    self.resolving_loop(file, |parsed, defaults, resolved| lower_middleware_in(parsed, defaults, resolved))
  }

  /// Runs `lower` over `file`, binding each module-level name it could not
  /// resolve through this set and trying again, until it lowers or a name
  /// cannot be bound.
  fn resolving_loop<T: HeapMarked>(&mut self, file: &str, lower: impl Fn(&Parsed, &SessionDefaults, &Resolved) -> Result<T, Unresolved>) -> Result<T, LowerError> {
    crate::with_heap(|| self.resolving_once(file, &lower), HeapMarked::mark)
  }

  fn resolving_once<T>(&mut self, file: &str, lower: &impl Fn(&Parsed, &SessionDefaults, &Resolved) -> Result<T, Unresolved>) -> Result<T, LowerError> {
    self.load(file)?;
    self.load_class_graph(file);
    self.meanings(file);
    let mut resolved = Resolved { globals: Vec::new(), global_kinds: Vec::new(), global_formatters: Vec::new(), natives: self.natives.clone(), classes: Some(self.classes.clone()), type_kinds: self.alias_kinds(file), reaches: self.reaches.clone() };
    loop {
      let parsed_cached = self.parsed(file);
      let parsed = parsed_cached.get();
      let defaults = self.defaults.clone();
      match lower(&parsed, &defaults, &resolved) {
        Ok(done) => return Ok(done),
        Err((error, Some(name))) if !resolved.globals.iter().any(|(n, _)| *n == name) => match self.global(file, &name)? {
          Some((expr, key)) => {
            if let Some(kind) = self.global_kind(file, &name)? {
              resolved.global_kinds.push((name.clone(), kind));
            }
            if let Some(kind) = self.global_return_kind(file, &name)? {
              resolved.global_kinds.push((crate::returned_by(&name), kind));
            }
            if let Some(made) = self.global_formatter(file, &name)? {
              resolved.global_formatters.push((name.clone(), made));
            }
            resolved.globals.push((name, self.name_if_large(key, expr)));
            resolved.natives = self.natives.clone();
          }
          None => return Err(error),
        },
        Err((error, _)) => return Err(error),
      }
    }
  }

  /// Lowers every export of a module under `ext/`: each is an extension,
  /// lowered or native; one that does not lower fails the build. Returns
  /// `(file#export, kind)` rows, the kind `lowered`, `native render` or
  /// `native body`.
  pub fn lower_extensions(&mut self, file: &str) -> Result<Vec<(String, String)>, LowerError> {
    self.load(file)?;
    self.meanings(file);
    let parsed_cached = self.parsed(file);
    let parsed = parsed_cached.get();
    let names: Vec<String> = parsed.exports().map(|(name, _)| name.to_owned()).collect();
    let mut rows = Vec::new();
    for name in names {
      let expr = match self.global(file, &name)?.map(|(expr, _)| expr) {
        Some(expr) => expr,
        None => return Err(LowerError::Extension(Residue { file: file.to_owned(), line: 1, column: 1, message: format!("`{name}` is not a value the build can follow"), hint: None, via: Vec::new() })),
      };
      let kind = match &expr {
        Expr::Ext { module, name: member, args } if args.is_empty() => {
          let key = format!("{module}.{member}");
          let reach = self.natives.iter().find(|(n, _)| *n == key).map(|(_, r)| *r).unwrap_or(Reach::Render);
          format!("native {}", reach.as_str())
        }
        _ => "lowered".to_owned(),
      };
      rows.push((format!("{file}#{name}"), kind));
    }
    Ok(rows)
  }

  /// Whether `file` writes a JSX element or fragment anywhere, which needs a
  /// JSX runtime to compile. A file that does not parse writes none.
  pub fn writes_jsx(&mut self, file: &str) -> bool {
    struct Found(bool);
    impl<'a> oxc_ast_visit::Visit<'a> for Found {
      fn visit_jsx_element(&mut self, _: &js::JSXElement<'a>) {
        self.0 = true;
      }
      fn visit_jsx_fragment(&mut self, _: &js::JSXFragment<'a>) {
        self.0 = true;
      }
    }
    if self.load(file).is_err() {
      return false;
    }
    let mut found = Found(false);
    self.parsed(file).get().module().walk(&mut found);
    found.0
  }

  /// Whether `module`'s file imports a value from `source`. A `file#export`
  /// module is asked about its file.
  pub fn imports_value_from(&self, module: &str, source: &str) -> bool {
    let file = module.split_once('#').map(|(file, _)| file).unwrap_or(module);
    let Some(parsed_cached) = self.parsed_if(file) else { return false };
    let parsed = parsed_cached.get();
    parsed.module().body.iter().any(|item| match item {
      js::Statement::ImportDeclaration(import) => !import.import_kind.is_type() && crate::lossy(import.source.value) == source && import.specifiers.iter().flatten().any(|spec| !matches!(spec, js::ImportDeclarationSpecifier::ImportSpecifier(named) if named.import_kind.is_type())),
      _ => false,
    })
  }

  /// `file` and every local file it reaches through value imports and
  /// re-exports, in the order first reached. A file that does not parse is
  /// listed and not followed.
  pub fn local_reach(&mut self, file: &str) -> Vec<String> {
    let mut reached = vec![file.to_owned()];
    let mut next = 0;
    while next < reached.len() {
      let from = reached[next].clone();
      next += 1;
      for target in self.local_imports(&from) {
        if !reached.contains(&target) {
          reached.push(target);
        }
      }
    }
    reached
  }

  /// The line of the first import or re-export in `file` from `@snapfire/fsr/host` or a module under it, which the browser cannot load.
  pub fn host_import(&mut self, file: &str) -> Option<u32> {
    let parsed_cached = self.script_of(file)?;
    let parsed = parsed_cached.get();
    let host = crate::meaning::HOST_ROOT;
    let found = value_sources(&parsed.module()).find(|src| {
      let source = crate::lossy(src.value);
      source == host || source.starts_with(&format!("{host}/"))
    })?;
    Some(parsed.residue(found.span, "").line as u32)
  }

  /// The local script files and described foreign components `file`
  /// imports a value from or re-exports, in source order: a described foreign
  /// component's from its script. None when it does not parse. An imported asset is not a
  /// script and is left out.
  pub fn local_imports(&mut self, file: &str) -> Vec<String> {
    let Some(parsed_cached) = self.script_of(file) else { return Vec::new() };
    let parsed = parsed_cached.get();
    let sources: Vec<String> = value_sources(&parsed.module()).map(|src| crate::lossy(src.value)).chain(dynamic_sources(&parsed.module())).collect();
    sources
      .iter()
      .filter_map(|src| self.resolve_import(file, src))
      .filter(|target| [".tsx", ".ts", ".jsx", ".js"].iter().any(|ext| target.ends_with(ext)) || is_transformed(target) || self.described.contains_key(target))
      .collect()
  }

  /// The stylesheets `file` brings, in source order: each local one it imports for its effect
  /// (`import "./card.css"`) plus, for each Vue or Svelte component it imports, the sheet compiling
  /// that component writes beside it (`Card.vue.css`), which exists only when it has a style block.
  pub fn sheets(&mut self, file: &str) -> Vec<String> {
    let Some(parsed_cached) = self.script_of(file) else { return Vec::new() };
    let parsed = parsed_cached.get();
    let sources: Vec<(bool, String)> = parsed
      .module()
      .body
      .iter()
      .filter_map(|item| match item {
        js::Statement::ImportDeclaration(import) if !import.import_kind.is_type() => Some((import.specifiers.as_ref().is_none_or(|s| s.is_empty()), crate::lossy(import.source.value))),
        _ => None,
      })
      .collect();
    let mut out = Vec::new();
    for (bare, src) in sources {
      let Some(target) = self.resolve_import(file, &src) else { continue };
      let ext = Path::new(&target).extension().and_then(|e| e.to_str()).unwrap_or("").to_ascii_lowercase();
      if bare && ext == "css" {
        out.push(target);
      } else if snapfire_compiler_wire::claimed(&ext).is_some() {
        out.push(format!("{target}.css"));
      }
    }
    out
  }

  /// The value imports in `file` of components a plugin's language holds, a
  /// `.vue` file among them; none when it does not parse. A namespace import
  /// is left out, since it names no component.
  pub fn foreign_imports(&mut self, file: &str) -> Vec<ForeignImport> {
    if self.load(file).is_err() {
      return Vec::new();
    }
    let parsed_cached = self.parsed(file);
    let parsed = parsed_cached.get();
    let source = parsed.text();
    let mut out = Vec::new();
    for item in &parsed.module().body {
      let js::Statement::ImportDeclaration(import) = item else { continue };
      if import.import_kind.is_type() || import.specifiers.iter().flatten().any(|spec| matches!(spec, js::ImportDeclarationSpecifier::ImportNamespaceSpecifier(_))) {
        continue;
      }
      let Some(target) = self.resolve_import(file, crate::lossy(import.source.value).as_ref()) else { continue };
      let ext = Path::new(&target).extension().and_then(|e| e.to_str()).unwrap_or("").to_ascii_lowercase();
      if snapfire_compiler_wire::claimed(&ext).is_none() {
        continue;
      }
      let bindings = import
        .specifiers
        .iter()
        .flatten()
        .filter_map(|spec| match spec {
          js::ImportDeclarationSpecifier::ImportDefaultSpecifier(default) => Some((default.local.name.to_string(), format!("{target}#default"))),
          js::ImportDeclarationSpecifier::ImportSpecifier(named) if !named.import_kind.is_type() => {
            let imported = crate::schema::export_name(&named.imported);
            Some((named.local.name.to_string(), format!("{target}#{imported}")))
          }
          _ => None,
        })
        .collect();
      out.push(ForeignImport { text: source[parsed.range(import.span)].to_owned(), bindings });
    }
    out
  }

  /// Splits route module `module` into composition and an island module
  /// beside it when it holds state, handlers or effects; afterwards the set
  /// reads both from what the split wrote. `Ok(None)` when there is nothing to
  /// move, `Err` with the reason when what there is cannot be moved.
  pub fn extract_route(&mut self, module: &str) -> Result<Option<Extracted>, String> {
    let file = module.split_once('#').map(|(file, _)| file).unwrap_or(module).to_owned();
    if self.load(&file).is_err() {
      return Ok(None);
    }
    let parsed_cached = self.parsed(&file);
    let parsed = parsed_cached.get();
    let export = module.split_once('#').map(|(_, export)| export).unwrap_or("default");
    if async_function_span(&parsed, export).is_some() && !find_function(&parsed, export).is_some_and(|f| awaits_only_imports(&f)) {
      return Ok(None);
    }
    let slots = self.slots.iter().find(|(m, _)| m == module).map(|(_, names)| names.clone()).unwrap_or_default();
    match crate::extract::extract(&parsed, &file, &slots, None, false) {
      Ok(Some(extraction)) => {
        self.forget(&file);
        self.provided.insert(file.clone(), extraction.page.clone());
        self.provided.insert(extraction.island_file.clone(), extraction.island.clone());
        Ok(Some(Extracted { island: format!("{}#default", extraction.island_file), file: extraction.island_file, source: extraction.island, page: (file, extraction.page), holds: extraction.holds, started: None }))
      }
      Ok(None) => Ok(None),
      Err(crate::extract::Kept(why)) => Err(why),
    }
  }

  pub fn lower_route(&mut self, module: &str) -> Result<Option<Extracted>, LowerError> {
    let error = match self.lower(module) {
      Ok(()) => return Ok(None),
      Err(error) => error,
    };
    let file = module.split_once('#').map(|(file, _)| file).unwrap_or(module).to_owned();
    let LowerError::Residue(residue) = &error else { return Err(error) };
    if residue.file != file || !residue.via.is_empty() || residue.message == ASYNC_COMPONENT {
      return Err(error);
    }
    let Some(parsed_cached) = self.parsed_if(&file) else { return Err(error) };
    let parsed = parsed_cached.get();
    let Some(offset) = parsed.offset(residue.line, residue.column).and_then(|offset| crate::extract::page_offset(&parsed, offset)) else { return Err(error) };
    let slots = self.slots.iter().find(|(m, _)| m == module).map(|(_, names)| names.clone()).unwrap_or_default();
    let forget = |set: &mut Self| {
      set.failed.remove(module);
      set.forget(&file);
    };
    // The cut around the first residue first; the whole markup when the page
    // still holds another one.
    for whole in [false, true] {
      let Ok(Some(extraction)) = crate::extract::extract(&parsed, &file, &slots, Some(offset), whole) else { return Err(error) };
      forget(self);
      let before = self.provided.insert(file.clone(), extraction.page.clone());
      let island_before = self.provided.insert(extraction.island_file.clone(), extraction.island.clone());
      let again = match self.lower(module) {
        Ok(()) => return Ok(Some(Extracted { island: format!("{}#default", extraction.island_file), file: extraction.island_file, source: extraction.island, page: (file, extraction.page), holds: extraction.holds, started: Some(residue.clone()) })),
        Err(LowerError::Residue(left)) => left.file == file,
        Err(_) => false,
      };
      forget(self);
      self.forget(&extraction.island_file);
      match island_before {
        Some(source) => self.provided.insert(extraction.island_file.clone(), source),
        None => self.provided.remove(&extraction.island_file),
      };
      match before {
        Some(source) => self.provided.insert(file.clone(), source),
        None => self.provided.remove(&file),
      };
      if !again {
        break;
      }
    }
    Err(error)
  }

  /// Settles which chunks keep their fallback once every component is
  /// lowered: those of a component a `foreign` placement mounts, whose props
  /// the browser supplies, and of every component when a `<Mount>` names its
  /// island at run time, since any island can then be mounted that way.
  pub fn settle_proofs(&mut self) {
    let dynamic = self.components.iter().any(|(_, c)| holds_mount(&c.render));
    // A component the browser renders can place another in a branch the
    // server left closed, where nothing recorded that one's markup.
    let mut reached: Vec<String> = Vec::new();
    let mut pending: Vec<String> = self.components.iter().filter(|(_, c)| c.owner.hydrates()).map(|(m, _)| m.clone()).collect();
    while let Some(module) = pending.pop() {
      let Some((_, component)) = self.components.iter().find(|(m, _)| *m == module) else { continue };
      let mut placed = Vec::new();
      placed_modules(&component.render, &mut placed);
      for module in placed {
        if !reached.contains(&module) {
          reached.push(module.clone());
          pending.push(module);
        }
      }
    }
    // A component only the browser renders has no lowered tree to follow, so
    // whatever its file imports may mount fresh with no table beside it.
    let browser_roots: Vec<String> = self.browser_only.iter().map(|(module, _)| module.split('#').next().unwrap_or(module).to_owned()).collect();
    let mut browser_files: Vec<String> = Vec::new();
    for root in browser_roots {
      for file in self.local_reach(&root) {
        if !browser_files.contains(&file) {
          browser_files.push(file);
        }
      }
    }
    for rewrite in &mut self.rewrites {
      if dynamic || self.mounted.contains(&rewrite.module) || reached.contains(&rewrite.module) || browser_files.contains(&rewrite.file) {
        rewrite.chunks.iter_mut().for_each(|chunk| chunk.proven = false);
      }
    }
  }

  pub fn rewritten(&self) -> Vec<(String, String)> {
    let mut files: Vec<&str> = Vec::new();
    for file in self.rewrites.iter().map(|r| r.file.as_str()).chain(self.fn_exports.iter().map(|(f, _, _)| f.as_str())).chain(self.fn_names.iter().map(|(f, _)| f.as_str())) {
      if !files.contains(&file) {
        files.push(file);
      }
    }
    files
      .into_iter()
      .filter_map(|file| {
        let source = self.sources.get(file)?.text.clone();
        let rewrites: Vec<&Rewrite> = self.rewrites.iter().filter(|r| r.file == file).collect();
        let mut text = match rewrites.is_empty() {
          true => source.clone(),
          false => hoist::apply(&source, &rewrites),
        };
        for (_, range, name) in self.fn_exports.iter().filter(|(f, _, _)| f == file) {
          text.push_str(&format!("\nexport const {name} = {};\n", &source[range.clone()]));
        }
        for (_, name) in self.fn_names.iter().filter(|(f, _)| f == file) {
          if crate::declares_unexported(file, &text, name) {
            text.push_str(&format!("\nexport {{ {name} }};\n"));
          }
        }
        Some((file.to_owned(), text))
      })
      .collect()
  }

  /// Every file the browser imports a function prop from: the files of the exports the rewrite added and those of the module-level functions passed by name.
  pub fn function_modules(&self) -> Vec<String> {
    let mut files: Vec<String> = Vec::new();
    for file in self.fn_exports.iter().map(|(f, _, _)| f).chain(self.fn_names.iter().map(|(f, _)| f)).chain(self.fn_files.iter()) {
      if !files.contains(file) {
        files.push(file.clone());
      }
    }
    files.sort();
    files
  }

  /// The `size`, `alt` and `contentType` exports of a generated card image's
  /// module, each a literal as Next writes them: `{ width: 1200, height: 630 }`,
  /// a string and `"image/png"` or `"image/jpeg"`. One that is not a literal
  /// is refused, since the build reads them rather than running them.
  pub fn image_exports(&mut self, file: &str) -> Result<ImageExports, LowerError> {
    self.load(file)?;
    let parsed_cached = self.parsed(file);
    let parsed = parsed_cached.get();
    let mut out = ImageExports::default();
    for item in &parsed.module().body {
      let js::Statement::ExportDeclaration(export) = item else { continue };
      let js::Declaration::VariableDeclaration(var) = &export.declaration else { continue };
      for decl in &var.declarations {
        let (js::BindingPattern::BindingIdentifier(name), Some(init)) = (&decl.id, decl.init.as_ref()) else { continue };
        let init = unwrap_literal(init);
        match name.name.as_str() {
          "size" => {
            let refused = || LowerError::from(parsed.residue(init.span(), "`size` is `{ width: <number>, height: <number> }` with literal numbers"));
            let js::Expression::ObjectExpression(object) = init else { return Err(refused()) };
            for prop in &object.properties {
              let js::ObjectPropertyKind::ObjectProperty(prop) = prop else { return Err(refused()) };
              let crate::syntax::Prop::KeyValue(kv) = crate::syntax::prop_of(prop) else { return Err(refused()) };
              let Some(key) = prop_name(&kv.key, kv.computed) else { return Err(refused()) };
              let js::Expression::NumericLiteral(n) = unwrap_literal(&kv.value) else { return Err(refused()) };
              if n.value < 1.0 || n.value.fract() != 0.0 {
                return Err(refused());
              }
              match key.as_str() {
                "width" => out.width = Some(n.value as u32),
                "height" => out.height = Some(n.value as u32),
                _ => return Err(refused()),
              }
            }
            if out.width.is_none() || out.height.is_none() {
              return Err(refused());
            }
          }
          "alt" | "contentType" => {
            let which = name.name.to_string();
            let text = match init {
              js::Expression::StringLiteral(s) => crate::lossy(s.value),
              js::Expression::TemplateLiteral(tpl) if tpl.expressions.is_empty() && tpl.quasis.len() == 1 => tpl.quasis[0].value.cooked.map(crate::lossy).unwrap_or_default(),
              other => return Err(parsed.residue(other.span(), format!("`{which}` is a literal string")).into()),
            };
            if which == "alt" {
              out.alt = Some(text);
            } else {
              out.content_type = Some(text);
            }
          }
          _ => {}
        }
      }
    }
    Ok(out)
  }

  /// Lowers `module` (a `path#export` under the app) and everything it
  /// renders. A module already lowered is not read again and one that did
  /// not lower answers with the same error. A module still
  /// being lowered is a component that renders itself, directly or through
  /// others: the call is the renderer's, which looks the module up by name.
  /// Makes each `{prop}` read that no placement in this set fills a plain
  /// read again, once every component is lowered, so a component nobody
  /// passes markup to renders as it would have without fills.
  pub fn settle_fills(&mut self) {
    let mut filled: HashSet<(String, String)> = HashSet::new();
    for (_, component) in &self.components {
      fills::collect(&component.render, &mut filled);
    }
    for (module, component) in &mut self.components {
      fills::settle(&mut component.render, module, &filled);
    }
  }

  pub fn lower(&mut self, module: &str) -> Result<(), LowerError> {
    let (file, export) = module.split_once('#').ok_or_else(|| LowerError::MissingExport { file: module.to_owned(), export: String::new() })?;
    if self.components.iter().any(|(m, _)| m == module) {
      return Ok(());
    }
    if let Some(error) = self.failed.get(module) {
      return Err(error.clone());
    }
    if self.resolving.iter().any(|m| m == module) {
      self.cyclic = true;
      return Ok(());
    }
    let result = match is_foreign(file) && self.described.contains_key(file) {
      true => {
        self.resolving.push(module.to_owned());
        let result = match file.ends_with(".svelte") {
          true => self.lower_svelte(file, export),
          false => self.lower_vue(file, export),
        };
        self.resolving.pop();
        result
      }
      false => self.load(file).and_then(|()| {
        self.resolving.push(module.to_owned());
        let result = self.lower_loaded(file, export);
        self.resolving.pop();
        result
      }),
    };
    let component = match result {
      Ok(component) => component,
      Err(error) => {
        self.failed.insert(module.to_owned(), error.clone());
        return Err(error);
      }
    };
    self.components.push((module.to_owned(), component));
    if self.cyclic && self.resolving.is_empty() {
      self.settle();
    }
    Ok(())
  }

}

/// Points every component reference at the module the set lowered it under.
/// An island alias's `when` and `mode`.
type IslandTiming = (Option<String>, Option<String>);

/// The modules `Island`, `island`, `Slot` and `Link` are read from: the
/// client's React module, which a React application also mounts with or the
/// dialect's own declarations, which an application without React types
/// against.
pub const TEMPLATE_SOURCES: &[&str] = &["@snapfire/fsr/template", "@snapfire/fsr/client/react", "@snapfire/fsr/client/vue", "@snapfire/fsr/client/jsx-runtime"];

/// The client library's root, where `action` comes from.
pub const CLIENT_SOURCE: &str = "@snapfire/fsr/client";

/// Every file under `dir` as its path relative to it, `/`-separated, leaving
/// out a directory whose name starts with `.` and `node_modules`.
pub fn files_under(dir: &std::path::Path) -> Vec<String> {
  let mut files = Vec::new();
  let mut pending = vec![(dir.to_path_buf(), String::new())];
  while let Some((full, rel)) = pending.pop() {
    let Ok(entries) = std::fs::read_dir(&full) else { continue };
    for entry in entries.filter_map(|e| e.ok()) {
      let Some(name) = entry.file_name().to_str().map(str::to_owned) else { continue };
      let path = entry.path();
      let joined = if rel.is_empty() { name.clone() } else { format!("{rel}/{name}") };
      if path.is_dir() {
        if !name.starts_with('.') && name != "node_modules" {
          pending.push((path, joined));
        }
      } else if path.is_file() {
        files.push(joined);
      }
    }
  }
  files.sort();
  files
}

/// Whether a file or a `file#export` module, is written in a language the
/// build does not read. Such a component has no server body: a plugin
/// compiles it for the browser and the page places it as an island.
pub fn is_foreign(module: &str) -> bool {
  let file = module.split_once('#').map(|(file, _)| file).unwrap_or(module);
  let ext = std::path::Path::new(file).extension().and_then(|e| e.to_str()).unwrap_or("");
  !matches!(ext, "ts" | "tsx" | "js" | "jsx" | "mts" | "mjs") && !is_transformed(file)
}

#[derive(Clone, Copy)]
pub(crate) enum FunctionBody<'a> {
  Block(&'a [js::Statement<'a>]),
  Expr(&'a js::Expression<'a>),
}

/// A function a handler calls by name. One an inlined custom hook declared carries the hook's scope, since its body reads the hook's names wherever the caller runs it.
#[derive(Clone)]
pub(crate) struct HandlerFn<'p> {
  params: Vec<crate::Param<'p>>,
  body: FunctionBody<'p>,
  hook: Option<Rc<HookScope<'p>>>,
}

/// What one inlined custom hook call bound, as its body saw it: its module, its parameters, its locals and the names the inlining renamed.
pub(crate) struct HookScope<'p> {
  parsed: &'p Parsed<'p>,
  meanings: Option<Rc<crate::meaning::Meanings>>,
  scope: Vec<(String, Expr)>,
  declared_below: Vec<(String, String)>,
  renames: Vec<(String, String)>,
}

/// What `const Lazy = island(Chart, { when, mode })` at module scope names.
pub(crate) struct IslandAlias {
  pub target: String,
  pub when: Option<String>,
  pub mode: Option<String>,
}

#[derive(Clone)]
enum Global<'a> {
  Const(&'a js::Expression<'a>),
  /// Its parameters, its body and whether it is `async`.
  Function(Vec<crate::Param<'a>>, FunctionBody<'a>, bool),
  /// A member of the class `id`, `parent` the class it extends.
  Member { id: String, parent: Option<String>, class: &'a js::Class<'a>, name: String, member: Member<'a> },
}

/// One member of a class as `lower_global` lowers it.
#[derive(Clone)]
pub(crate) enum Member<'a> {
  Constructor,
  /// A method or a getter, which takes the instance first.
  Instance(&'a js::Function<'a>),
  /// A static method or getter, `this` the class itself.
  Static(&'a js::Function<'a>),
  StaticField(&'a js::Expression<'a>),
}

/// What a class component with no `render` method is told.
pub(crate) const CLASS_COMPONENT: &str = "a class component without a `render` method";

pub(crate) enum Found<'a> {
  Declared(&'a js::FormalParameters<'a>, &'a [js::Statement<'a>], Span),
  Arrow(&'a js::ArrowFunctionExpression<'a>),
}

/// What an `async` component is told: the IR has no await in a render and the route's loader runs before it.
pub(crate) const ASYNC_COMPONENT: &str = "an `async` component; what it awaits belongs in the route's loader, `page.loader.ts`, which runs before the page renders";

/// A route module split by [`ComponentSet::extract_route`].
#[non_exhaustive]
pub struct Extracted {
  /// The island's module id.
  pub island: String,
  /// The island's file and source, which only the bundle overlay holds.
  pub file: String,
  pub source: String,
  /// The page's file and its source as composition.
  pub page: (String, String),
  /// The names the island took from the page's body.
  pub holds: Vec<String>,
  /// What the page stopped on that made the split, in the page as written.
  pub started: Option<Residue>,
}

/// A file parsed once for the build: the arena and the program parsed into it, kept together so every lookup reads the same parse.
pub(crate) struct Cached {
  // Borrows `_arena`. Declared first so it drops before the arena does.
  parsed: Parsed<'static>,
  // Boxed so the arena stays at one address when `Cached` moves.
  _arena: Box<Allocator>,
}

/// The name `local` was imported under when its source satisfies `source_is`,
/// which is how a call or a tag is recognised however it was spelled here.
/// The attribute that keeps an `<img>` of an imported asset out of the
/// rewrite. Stripped from the markup.
pub const RAW_ESCAPE: &str = "data-sf-raw";

struct Mutations(Vec<String>);

struct BareWrites {
  names: Vec<String>,
  through_functions: bool,
  /// Whether a field written through a name or a name handed to a call counts, as it does for a module `let` a function may change by [`Builtin::WriteBack`].
  through_calls: bool,
}

/// Reads and declarations of a function, by name and what each is bound to.
pub(crate) struct Free<'s> {
  scope: &'s crate::scope::Scope,
  reads: Vec<(String, crate::Ctxt)>,
  declared: Vec<(String, crate::Ctxt)>,
}

/// `{ a, b = x, c: d, e: { f }, ...rest }` over `target`: each name reads a field, a nested pattern reads within it, a default applies when the field is null or absent.
/// The body `let`, null, a value left to the browser reads through. It is listed with the state hoisting taints, so no chunk or value holding one is proven and the browser renders it.
pub(crate) const BROWSER_VAR: &str = "$browser";

/// A value only the browser knows that the server left to it: the expression the browser copy wraps, so it is null through hydration and computed once mounted, and the line it was found at.
pub(crate) struct BrowserText {
  wrap: Option<std::ops::Range<usize>>,
  line: usize,
  column: usize,
  reason: String,
}

/// The globals only a browser holds.
const BROWSER_GLOBALS: [&str; 7] = ["document", "window", "navigator", "location", "history", "localStorage", "sessionStorage"];

/// The name a continuation binds what it was run with, `{ value, ok, carry }`.
const RESUME: &str = "$resume";

/// The prefix of the name a continuation reads a state value under, carried in as it was when the act that runs the continuation ran.
const CAPTURED: &str = "$captured$";

/// The name the `catch` binds when an awaited call fails before it hands back a promise.
const REJECTED: &str = "$rejected";

/// What an async handler's statements run inside: the `catch` a rejection runs and the join every way through ends with.
#[derive(Clone, Default)]
struct Within<'p> {
  catch: Option<Catch<'p>>,
  then: Option<Join>,
}

/// A `catch` around an `await`: its parameter, its statements and the join after its `try`, where it goes on once it has run.
#[derive(Clone)]
struct Catch<'p> {
  param: Option<String>,
  body: &'p [js::Statement<'p>],
  after: Option<Join>,
}

/// A continuation to run once a way through an `if` or a `try` is done: its event and what the act passes it.
#[derive(Clone)]
struct Join {
  event: String,
  carry: Expr,
}

/// Where the ids a terminal component's inlined hooks take start, past any its hoisting candidates number.
const HOOK_IDS: u32 = 1 << 20;

/// The residue of a handler that sets no state and calls no action.
const NO_EFFECT: &str = "a handler that sets no state and calls no action";

/// A `switch` as its arms: the tests that select each body in order and the `default` body. A body ends with a `break` (dropped), a `return` or the end of the last arm; cases with no body share the next one's. A body written as one block is that block's statements.
pub(crate) type SwitchArms<'s> = (Vec<(Vec<&'s js::Expression<'s>>, &'s [js::Statement<'s>])>, Option<&'s [js::Statement<'s>]>);

/// Where a straight-line body may write: names bound at or above `floor` in the scope, except those `frozen` (state a setter owns). `setters` are the component's own state setters with the state each sets, which a component body may call while it renders.
pub(crate) struct Writable<'w> {
  pub(crate) floor: usize,
  pub(crate) frozen: &'w [String],
  pub(crate) setters: &'w [(String, String)],
}

/// The tag of a field alias record under [`alias_key`]: `[tag, root, keys, bound]`, where an element alias is `[array, index, bound]`.
const FIELD_ALIAS: &str = "$field";

enum Alias {
  None,
  Of { array: String, index: Expr },
  Field { root: String, keys: Vec<Expr> },
  Stale(String),
  StaleField(String),
}

/// The names a `for…of` lowers under, taken from where it is written so the markup lowered ahead of it and the loop agree: its item, its accumulator and the outer locals its body writes, first write first.
pub(crate) struct LoopNames {
  pub(crate) item: String,
  pub(crate) acc: String,
  pub(crate) written: Vec<String>,
}

/// The names a loop body writes and the names it declares itself, outside nested functions.
#[derive(Default)]
pub(crate) struct Writes {
  written: Vec<String>,
  declared: Vec<String>,
}

/// `for (let i = a; i < b; i++)` (also `<=`, `++i` and `i += s`) whose body does not write `i` and whose bound and step read nothing the body writes.
pub(crate) struct Counted<'e> {
  pub(crate) name: String,
  from: &'e js::Expression<'e>,
  to: &'e js::Expression<'e>,
  inclusive: bool,
  /// What `i += s` adds each pass; `None` for one.
  step: Option<&'e js::Expression<'e>>,
}

/// How many times a terminal's `while` runs its body at most.
pub(crate) const WHILE_CAP: f64 = 10_000.0;

/// A function body as one expression: `const`s inline into what follows and
/// `if (c) return a;` followed by more becomes `c ? a : rest`.
/// A handler's statements as they accumulate: the statements in order, the
/// patch as one entry per state key and the condition the statement being
/// lowered runs under, `None` at the top of the body. A branch's declarations
/// are hoisted under a name of their own, since the patch is evaluated after
/// every branch has bound.
#[derive(Default)]
pub(crate) struct HandlerWalk {
  pub(crate) out: Vec<Stmt>,
  pub(crate) patch: Vec<Entry>,
  pub(crate) reach: Option<Expr>,
  pub(crate) locals: usize,
  /// Inside a `try`: each state key set there, the variable its value is written to where the set runs and what that variable starts from. The patch then reads the variable.
  pub(crate) in_place: Option<Vec<(String, String, Expr)>>,
}

/// What an inlined custom hook returned: a value, a handler the browser runs or an object or array literal of either, part by part.
enum Returned {
  Value(Expr),
  Handler(String),
  Object(Vec<(String, Part)>),
  Array(Vec<Option<Part>>),
}

enum Part {
  Value(Expr),
  Handler(String),
}

struct ComponentLowerer<'a, 'p> {
  lowerer: Lowerer<'p>,
  file: &'a str,
  /// Names whose calls the browser owns: inner functions and state setters.
  handlers: Vec<String>,
  /// Capitalised tags met so far, with where, resolved by the set after.
  refs: Vec<(String, (usize, usize))>,
  /// The `value` of the enclosing `<select>`, which its options compare against.
  select_value: Option<Expr>,
  /// The props parameter when bound whole, so `props.children` is the slot.
  props_name: Option<String>,
  /// The rest of a destructured props parameter that leaves `children` in it.
  props_rest: Option<String>,
  /// The local name `children` was destructured to.
  children_name: Option<String>,
  /// Whether the component is wrapped in `forwardRef`, so its second parameter is the `ref` prop.
  forwards_ref: bool,
  /// The class whose `render()` this is, which binds `this`.
  class: Option<&'p js::Class<'p>>,
  /// A layout: its slot is wrapped in `<sf-s>`.
  layout_root: bool,
  /// The slots a layout's `slots/` directory declares.
  slot_names: Vec<String>,
  /// Destructured props of a layout that name a slot: the local name and the slot.
  slot_props: Vec<(String, String)>,
  /// Bindings the browser can change without a new payload: state, store
  /// and ref hooks. A hoist reading one is not props only.
  state: Vec<String>,
  /// Where the rewrite binds the hoist reader in this component.
  hook: Option<Hook>,
  /// The `useState` and `useStore` bindings, in order: a server-mode island's state.
  state_bindings: Vec<String>,
  /// The `useStore` bindings among `state_bindings`, with the key each reads.
  store_bindings: Vec<(String, String)>,
  /// A `let` per store key a handler writes without the component reading it: bound to the key the way a `useStore` is, so the patch carries it.
  store_lets: Vec<Stmt>,
  /// Each setter and the state it sets.
  setters: Vec<(String, String)>,
  /// Handler functions declared in the component, by name, for `onClick={add}`.
  handler_fns: HashMap<String, HandlerFn<'p>>,
  /// The handlers lowered so far; an element's `$on:` marker holds an index into it.
  lowered_handlers: Vec<Handler>,
  /// What a handler's parameters stand for in place of `$event` alone: `useInput`'s `input` and `key`.
  event_binds: Option<Vec<Expr>>,
  /// Ink's `write` from `useStdout` or `useStderr`, by local name, with the action a call to it is: `$stdout` or `$stderr`.
  writers: Vec<(String, &'static str)>,
  /// How many `useAnimation` calls the component has made, which names each one's frame state.
  animations: usize,
  /// Custom element tags with a shadow template, to the template's module.
  elements: Rc<HashMap<String, String>>,
  /// Custom hooks resolved so far: the name a call uses, the module that declares it and its export there.
  hooks: Vec<(String, &'p Parsed<'p>, String)>,
  /// A custom hook the body called that the set has not read yet, with where its meaning says it is declared.
  wanted_hook: Option<(String, Option<(String, String)>)>,
  /// Lets an inlined custom hook bound, which the body places before the declaration that called it.
  pending_lets: Vec<Stmt>,
  /// How many custom hook calls have been inlined, which names each call's bindings apart.
  hook_calls: usize,
  /// The file a name the lowerer could not bind was met in, when that is a hook's rather than the component's.
  unbound_in: Option<String>,
  /// The functions returning markup being inlined, innermost last, so one that calls itself is placed as a component rather than unrolled for ever.
  inlining: Vec<String>,
  /// Whether this is a markup helper placed by its own recursion, its props its arguments by position.
  positional: bool,
  /// The module-level markup helpers this component placed by their own recursion with positional props.
  positional_refs: Vec<String>,
  /// The functions [`Self::markup_call`] is reading the body of, so helpers calling each other end.
  checking: std::cell::RefCell<Vec<String>>,
  /// The child counts the component reads, `render::CHILD_COUNT` and `render::CHILD_ITEMS`, which its body then opens with.
  counts_children: Vec<&'static str>,
  /// `<X.Provider>` tags met, by the name of `X` and where, checked by the
  /// set after to be `createContext` values.
  providers: Vec<(String, (usize, usize), Option<bool>)>,
  assets: Rc<dyn AssetResolver>,
  rewrite_images: bool,
  /// The head rows this component asks for, a priority image's preload.
  heads: Vec<HeadRow>,
  /// Each `import(`...${x}...`)` in the file, by where its call starts: the
  /// files it can load and the value of `x` that picks each.
  imports: Rc<HashMap<u32, Vec<(String, String)>>>,
  /// Names bound to the default export of such an import: the key that picks
  /// a file and the choices.
  chosen: HashMap<String, (Expr, Vec<(String, String)>)>,
  /// The transformed files placed with a `components` prop: the id each
  /// placement names, the file and the elements it replaces.
  variants: Vec<(String, String, Vec<Override>)>,
  /// Per placement, by the name `refs` records it under, the props passed whose kind the build knows.
  placed_kinds: Vec<(String, Vec<(String, crate::Kind)>)>,
  /// Each value only the browser knows that the server left to it, so the component hydrates and its browser copy computes the value once mounted.
  browser_texts: Vec<BrowserText>,
  /// Each module-level `let` a function writes that this component reads: its name, its store key and the value the module's initialisation leaves it at, the seed the store falls back to.
  module_lets: Vec<(String, String, Expr)>,
  /// Each function value passed as a prop, with where the browser gets it from.
  fn_refs: Vec<FnRef>,
  /// The `useRef` bindings a terminal holds as state, so a handler's write to `.current` is a patch.
  held_refs: Vec<String>,
  /// Each terminal `useReducer` dispatcher with the state it sets and the reducer it runs.
  dispatchers: Vec<(String, String, Expr)>,
  /// A terminal's `startTransition` functions, whose callback a handler runs in place.
  transitions: Vec<String>,
  /// While a terminal effect lowers, each `let` it declares and the hidden state that holds it.
  effect_locals: Vec<(String, String)>,
  /// While a terminal effect lowers, each function it declares and the event of the handler it lowered to.
  effect_fns: Vec<(String, String)>,
  /// The `useCallback` functions whose dependencies never change, which an effect may depend on as a mount effect.
  stable_callbacks: Vec<String>,
  /// While a terminal effect lowers, each process it spawns: the local holding it and the id its handlers are named by.
  spawns: Vec<(String, String)>,
  /// While a terminal effect whose dependencies change lowers, its event: a function it returns is its cleanup, [`snapfire_fsr_ir::host::Act::Cleanup`].
  cleanup_for: Option<String>,
  /// While an effect or an input handler of an inlined custom hook lowers, each name the hook declared and what the inlining renamed it to.
  hook_renames: Vec<(String, String)>,
  /// The locals a terminal handler bound to an arrow it passes to `on` or `off`, each holding `{ event, carry }` of the continuation the arrow is, with whether the arrow takes parameters.
  named_listeners: Vec<(String, bool)>,
  /// How many ids the custom hooks a terminal component inlines have taken with `useId`.
  hook_ids: u32,
}

/// Where the browser gets a function passed as a prop: the placement and prop it went to, the value the server holds and the module id `file#export` the browser imports it from or else why the browser cannot reach it. `needs` is what the rewrite and the bundle add for the id to resolve.
struct FnRef {
  module: String,
  prop: String,
  value: Expr,
  id: Result<Expr, String>,
  needs: Vec<Need>,
  /// The `UNLOWERED_ATTR` text the placement carries for this prop when the server does not lower the function, which the browser's reference makes moot once the placement is known to render in the browser.
  unlowered: Option<String>,
}

enum Need {
  /// An arrow hoisted into an export of this file: its byte range and the export's name.
  Export(std::ops::Range<usize>, String),
  /// A function this file declares that the rewrite exports.
  Name(String),
  /// A file the browser imports.
  File(String),
}

/// One head element a component asks for: a tag and its attributes in order.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct HeadRow {
  pub tag: String,
  pub attrs: Vec<(String, String)>,
}

const EFFECT_HOOKS: &[&str] = &["useEffect", "useLayoutEffect", "useInsertionEffect", "useDebugValue", "useImperativeHandle"];

/// The hooks FSR's client library declares, which another package's hook of the same name is not.
const FSR_HOOKS: &[&str] = &["useStore", "useLocale", "useEffectEvent", "useAnimation"];

#[cfg(test)]
mod tests;

/// Where copying a constant into every body that reads it starts to cost more
/// than a table entry and an indirection.
const CONST_WEIGHT: usize = 32;

/// The first argument of every call to the client's `useStore`, however the module named it.
/// A `derive` call the build read: the key, its sources and the compute
/// lowered over `Expr::Store` of each. One the browser alone runs has no
/// compute and carries `at` and `why`.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub struct LoweredDerive {
  pub key: String,
  pub sources: Vec<String>,
  pub body: Option<Expr>,
  pub at: Option<String>,
  pub why: Option<String>,
}

struct DeriveFailure {
  residue: Residue,
  unbound: Option<String>,
  /// The key and sources, when only the compute failed.
  compute: Option<(String, Vec<String>)>,
}

struct DeriveCalls<'m, 'b> {
  meanings: &'m crate::meaning::Meanings,
  build: crate::syntax::Build<'b>,
  calls: Vec<js::CallExpression<'b>>,
}

struct StoreCalls<'m, 'b> {
  meanings: &'m crate::meaning::Meanings,
  build: crate::syntax::Build<'b>,
  args: Vec<js::Expression<'b>>,
}

/// A layout's or a page's `export const theme`.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct ThemeExport {
  pub name: String,
  pub force: bool,
}

/// What a generated card image's module says about the image it draws.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
#[non_exhaustive]
pub struct ImageExports {
  pub width: Option<u32>,
  pub height: Option<u32>,
  pub alt: Option<String>,
  pub content_type: Option<String>,
}

/// Markup written where a value goes, moved into the tree. A local holding
/// markup is substituted where it renders, a placement's markup prop becomes
/// a `$slot:<prop>` fill among its children and every `{$props.name}` becomes
/// a `$slotRead:<name>` that takes such a fill before the value.
mod fills;
