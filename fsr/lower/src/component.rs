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
    Self { app: app.to_path_buf(), reaches: Vec::new(), sources: HashMap::new(), assets: Rc::new(NoAssets), rewrite_images: true, heads: HashMap::new(), provided: HashMap::new(), consts: Consts::new(), defaults: SessionDefaults::new(), components: Vec::new(), failed: HashMap::new(), resolving: Vec::new(), recursive: Vec::new(), layouts: Vec::new(), slots: Vec::new(), rewrites: Vec::new(), pure: HashMap::new(), natives: Vec::new(), remaining: Vec::new(), foreign: Vec::new(), mounted: Vec::new(), described: HashMap::new(), meanings: HashMap::new(), meaning_in_progress: Vec::new(), foreign_residue: Vec::new(), browser_only: Vec::new(), undescribed: HashMap::new(), scripts_only: HashMap::new(), cyclic: false, stateless: HashMap::new(), moving_props: HashSet::new(), keys: HashMap::new(), elements: Rc::default(), transform: None, variants: HashMap::new(), placement_kinds: HashMap::new(), prop_kinds: HashMap::new(), relied: HashMap::new(), browser_texts: Vec::new(), lowering_component: false, module_lets: Vec::new(), continuations: Vec::new(), fn_exports: Vec::new(), fn_names: Vec::new(), fn_files: Vec::new(), classes: Rc::default(), classes_lowered: HashSet::new(), class_graphs: HashSet::new(), aliases: HashMap::new(), positional: HashSet::new() }
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
    let arena = Allocator::default();
    let parsed = self.parsed(&arena, file);
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
    let arena = Allocator::default();
    let parsed = self.parsed(&arena, file);
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
      let arena = Allocator::default();
      let parsed = self.parsed(&arena, file);
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
    let arena = Allocator::default();
    let parsed = self.parsed(&arena, file);
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
    let arena = Allocator::default();
    self.parsed(&arena, file).module().walk(&mut found);
    found.0
  }

  /// Whether `module`'s file imports a value from `source`. A `file#export`
  /// module is asked about its file.
  pub fn imports_value_from(&self, module: &str, source: &str) -> bool {
    let file = module.split_once('#').map(|(file, _)| file).unwrap_or(module);
    let arena = Allocator::default();
    let Some(parsed) = self.parsed_if(&arena, file) else { return false };
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
    let arena = Allocator::default();
    let parsed = self.script_of(&arena, file)?;
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
    let arena = Allocator::default();
    let Some(parsed) = self.script_of(&arena, file) else { return Vec::new() };
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
    let arena = Allocator::default();
    let Some(parsed) = self.script_of(&arena, file) else { return Vec::new() };
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

  /// `file` parsed, a described foreign component's script for one. None when it does not parse.
  fn script_of<'a>(&mut self, arena: &'a Allocator, file: &str) -> Option<Parsed<'a>> {
    match self.described.get(file).and_then(|described| described.script.as_ref()).or_else(|| self.scripts_only.get(file)) {
      Some(script) => parse_with(arena, file, &script.content, false).ok(),
      None => {
        self.load(file).ok()?;
        Some(self.parsed(arena, file))
      }
    }
  }

  /// `file` parsed into `arena`, with what its names mean once the set has read them. The file has loaded.
  fn parsed<'a>(&self, arena: &'a Allocator, file: &str) -> Parsed<'a> {
    self.parsed_if(arena, file).unwrap_or_else(|| panic!("`{file}` is read only once it has loaded"))
  }

  /// `file` parsed into `arena`, `None` when it has not loaded.
  fn parsed_if<'a>(&self, arena: &'a Allocator, file: &str) -> Option<Parsed<'a>> {
    let source = self.sources.get(file)?;
    let parsed = source.parse(arena).ok()?;
    if let Some(meanings) = self.meanings.get(file) {
      let _ = parsed.meanings.set(Rc::new(meanings.with_scope(parsed.scope.clone())));
    }
    Some(parsed)
  }

  /// Holds `source` as what `file` is parsed from once it parses, and registers the classes it declares.
  fn keep(&mut self, file: &str, source: Source) -> Result<(), LowerError> {
    source.parse(&Allocator::default())?;
    self.sources.insert(file.to_owned(), Rc::new(source));
    self.register_classes(file);
    Ok(())
  }

  /// The value imports in `file` of components a plugin's language holds, a
  /// `.vue` file among them; none when it does not parse. A namespace import
  /// is left out, since it names no component.
  pub fn foreign_imports(&mut self, file: &str) -> Vec<ForeignImport> {
    if self.load(file).is_err() {
      return Vec::new();
    }
    let arena = Allocator::default();
    let parsed = self.parsed(&arena, file);
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
    let arena = Allocator::default();
    let parsed = self.parsed(&arena, &file);
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

  /// Lowers route module `module`, and when its own body holds what the
  /// lowerer cannot read, moves the part that holds it into an island beside
  /// it and lowers the page again. The island is then a component that does
  /// not lower, which the browser renders alone.
  /// Drops what the set holds for `file` so it is read again: its parse, its
  /// failure and every file's meanings, which copy what `file` exports.
  fn forget(&mut self, file: &str) {
    self.sources.remove(file);
    self.failed.remove(file);
    self.meanings.clear();
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
    let arena = Allocator::default();
    let Some(parsed) = self.parsed_if(&arena, &file) else { return Err(error) };
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
    let arena = Allocator::default();
    let parsed = self.parsed(&arena, file);
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

  /// Marks `module` as placed with props the browser can change and, when it
  /// is already lowered, unwraps every hoist and chunk of it that reads its
  /// props, trims its rewrite to what stays and does the same for what it
  /// places with those props. One not lowered yet reads the mark when it is.
  fn unhoist_props(&mut self, module: &str) {
    if !self.moving_props.insert(module.to_owned()) {
      return;
    }
    let pure = self.pure.clone();
    let Some((_, component)) = self.components.iter_mut().find(|(m, _)| m == module) else { return };
    let taint = vec![hoist::PROPS.to_owned()];
    let mut kept = hoist::decide(component, &taint);
    let chunks = hoist::rechunk(component, &taint, &pure);
    let covered = hoist::cover(component, &chunks);
    kept.retain(|id| !covered.contains(id));
    let placed = hoist::placed_with_state(component, &taint);
    if let Some(rewrite) = self.rewrites.iter_mut().find(|r| r.module == module) {
      rewrite.sites.retain(|(id, _)| kept.contains(id));
      rewrite.chunks.retain(|site| chunks.iter().any(|c| c.id == site.id));
    }
    for module in placed {
      self.unhoist_props(&module);
    }
  }

  /// Brings every purity verdict to what the whole graph says. Lowering read
  /// a component still in progress as impure, which is low for anything on a
  /// cycle: purity falls from `stateless` to the greatest verdict that holds.
  /// The chunks hoisting chose with the provisional verdicts stay, since a
  /// component read as impure only keeps a subtree out of a chunk.
  fn settle(&mut self) {
    let mut pure = self.stateless.clone();
    loop {
      let falling: Vec<String> = pure
        .iter()
        .filter(|(module, held)| **held && self.components.iter().find(|(m, _)| m == *module).is_some_and(|(_, c)| !hoist::static_tree(&c.render, &pure)))
        .map(|(module, _)| module.clone())
        .collect();
      if falling.is_empty() {
        break;
      }
      for module in falling {
        pure.insert(module, false);
      }
    }
    self.pure = pure;
    self.cyclic = false;
  }

  fn load(&mut self, file: &str) -> Result<(), LowerError> {
    if self.sources.contains_key(file) {
      return Ok(());
    }
    if let Some(error) = self.failed.get(file) {
      return Err(error.clone());
    }
    if let Some((base, overrides)) = self.variants.get(file).cloned() {
      let source = match self.provided.get(&base) {
        Some(source) => Ok(source.clone()),
        None => std::fs::read_to_string(self.app.join(&base)).map_err(|e| LowerError::Parse { file: base.clone(), message: e.to_string() }),
      };
      let kept = source.and_then(|source| self.transformed(&base, &source, &overrides)).and_then(|source| self.keep(file, source));
      if let Err(error) = &kept {
        self.failed.insert(file.to_owned(), error.clone());
      }
      return kept;
    }
    let kept = match self.provided.get(file) {
      Some(source) => Ok(source.clone()),
      None => std::fs::read_to_string(self.app.join(file)).map_err(|e| LowerError::Parse { file: file.to_owned(), message: e.to_string() }),
    }
    .and_then(|source: String| match is_transformed(file) {
      true => self.transformed(file, &source, &[]),
      false if file.ends_with(".json") => Ok(Source { name: file.to_owned(), text: format!("export default {source};"), tsx: false, origin: None, vue_this: false }),
      false => Ok(Source { name: file.to_owned(), text: source, tsx: crate::jsx_parses(file), origin: None, vue_this: false }),
    })
    .and_then(|source| self.keep(file, source));
    if let Err(error) = &kept {
      self.failed.insert(file.to_owned(), error.clone());
    }
    kept
  }

  /// Every `import(`prefix${x}suffix`)` in `file`: by where its call starts,
  /// the files under the app it can load, each with the value of `x` that
  /// names it, and as globals every value each of those files exports, keyed
  /// `file#export`, so a destructured `meta` picks among them.
  fn template_imports(&mut self, file: &str) -> Result<(HashMap<u32, Vec<(String, String)>>, Vec<(String, Expr)>), LowerError> {
    let arena = Allocator::default();
    let parsed = self.parsed(&arena, file);
    let mut imports = HashMap::new();
    let mut globals = Vec::new();
    for (lo, prefix, suffix) in template_import_calls(&parsed.module()) {
      let choices = self.import_choices(file, &prefix, &suffix);
      for (_, target) in &choices {
        self.load(target)?;
        let exports: Vec<String> = self.meanings(target).exports.keys().filter(|name| *name != "default").cloned().collect();
        for export in exports {
          let key = format!("{target}#{export}");
          if globals.iter().any(|(k, _)| *k == key) {
            continue;
          }
          if let Ok(Some((expr, named))) = self.global(target, &export) {
            let expr = self.name_if_large(named, expr);
            globals.push((key, expr));
          }
        }
      }
      imports.insert(lo, choices);
    }
    Ok((imports, globals))
  }

  /// The files `prefix${x}suffix` names from `file`, anywhere under the
  /// directory the prefix ends in, each with the text that stands for `x`,
  /// which holds a `/` for a file in a subdirectory as webpack's context does.
  fn import_choices(&self, file: &str, prefix: &str, suffix: &str) -> Vec<(String, String)> {
    let (dir_spec, start) = match prefix.rsplit_once('/') {
      Some((dir, start)) => (dir.to_owned(), start.to_owned()),
      None => (".".to_owned(), prefix.to_owned()),
    };
    let Some(probe) = crate::resolve_specifier(file, &format!("{dir_spec}/x")) else { return Vec::new() };
    let dir = probe.rsplit_once('/').map(|(d, _)| d.to_owned()).unwrap_or_default();
    let mut names = files_under(&self.app.join(&dir));
    for provided in self.provided.keys() {
      if let Some(name) = provided.strip_prefix(&format!("{dir}/")) {
        names.push(name.to_owned());
      }
    }
    names.sort();
    names.dedup();
    names
      .into_iter()
      .filter_map(|name| {
        let middle = name.strip_prefix(start.as_str())?.strip_suffix(suffix)?;
        (!middle.is_empty() && !is_foreign(&name)).then(|| (middle.to_owned(), if dir.is_empty() { name.clone() } else { format!("{dir}/{name}") }))
      })
      .collect()
  }

  /// `source`, the text of `file`, as the module the transform writes from it.
  fn transformed(&self, file: &str, source: &str, overrides: &[Override]) -> Result<Source, LowerError> {
    let Some(transform) = &self.transform else {
      return Err(LowerError::Parse { file: file.to_owned(), message: "an MDX or markdown module, which this fsr reads only when built with its `mdx` feature".to_owned() });
    };
    let written = transform(file, source, overrides).map_err(|message| LowerError::Parse { file: file.to_owned(), message })?;
    Ok(Source { name: file.to_owned(), text: written.source, tsx: true, origin: written.origin.map(Rc::new), vue_this: false })
  }

  /// The file an import names relative to the app, through an alias or a
  /// relative path; `None` for a bare or generated specifier the build does
  /// not follow.
  /// Adds each class `file` declares to the table, its parent resolved through the import that binds it.
  fn register_classes(&mut self, file: &str) {
    let arena = Allocator::default();
    let parsed = self.parsed(&arena, file);
    for (name, class) in crate::classes::declared(&parsed.module()) {
      let id = format!("{file}#{name}");
      if self.classes.borrow().get(&id).is_some() {
        continue;
      }
      let mut info = crate::classes::describe(id, class);
      info.parent = class.heritage.as_ref().and_then(|parent| match crate::meaning::bare(&parent.expression) {
        js::Expression::Identifier(parent) => self.class_id(file, parent.name.as_str()),
        _ => None,
      });
      self.classes.borrow_mut().classes.push(info);
    }
  }

  /// The id of the class `name` names in `file`: declared there, or imported from a file the set can read.
  fn class_id(&mut self, file: &str, name: &str) -> Option<String> {
    let arena = Allocator::default();
    let parsed = self.parsed_if(&arena, file)?;
    if crate::classes::declared(&parsed.module()).iter().any(|(n, _)| n == name) {
      return Some(format!("{file}#{name}"));
    }
    for item in &parsed.module().body {
      let js::Statement::ImportDeclaration(import) = item else { continue };
      for spec in import.specifiers.iter().flatten() {
        let (local, imported) = match spec {
          js::ImportDeclarationSpecifier::ImportSpecifier(named) => (named.local.name.to_string(), crate::schema::export_name(&named.imported)),
          js::ImportDeclarationSpecifier::ImportDefaultSpecifier(d) => (d.local.name.to_string(), "default".to_owned()),
          js::ImportDeclarationSpecifier::ImportNamespaceSpecifier(_) => continue,
        };
        if local != name {
          continue;
        }
        let source = crate::lossy(import.source.value);
        let target = self.resolve_import(file, &source)?;
        if is_foreign(&target) || self.load(&target).is_err() {
          return None;
        }
        return self.class_id(&target, &imported).or_else(|| self.reexported_class(&target, &imported));
      }
    }
    None
  }

  /// The class `export { name } from "…"` or `export * from "…"` in `file` hands on.
  fn reexported_class(&mut self, file: &str, name: &str) -> Option<String> {
    let arena = Allocator::default();
    let parsed = self.parsed_if(&arena, file)?;
    for item in &parsed.module().body {
      let (source, named) = match item {
        js::Statement::ExportFromDeclaration(e) => (crate::lossy(e.source.value), e.specifiers.iter().find_map(|n| (crate::schema::export_name(&n.exported) == name).then(|| crate::schema::export_name(&n.local)))),
        js::Statement::ExportAllDeclaration(e) => (crate::lossy(e.source.value), Some(name.to_owned())),
        _ => continue,
      };
      let Some(orig) = named else { continue };
      let Some(target) = self.resolve_import(file, &source) else { continue };
      if is_foreign(&target) || self.load(&target).is_err() {
        continue;
      }
      if let Some(found) = self.class_id(&target, &orig).or_else(|| self.reexported_class(&target, &orig)) {
        return Some(found);
      }
    }
    None
  }

  /// Reads every file `file` reaches through its imports, so the classes they declare are in the table.
  fn load_class_graph(&mut self, file: &str) {
    let mut pending = vec![file.to_owned()];
    while let Some(at) = pending.pop() {
      if !self.class_graphs.insert(at.clone()) || self.load(&at).is_err() {
        continue;
      }
      let arena = Allocator::default();
      let parsed = self.parsed(&arena, &at);
      for item in &parsed.module().body {
        let source = match item {
          js::Statement::ImportDeclaration(import) => crate::lossy(import.source.value),
          js::Statement::ExportAllDeclaration(e) => crate::lossy(e.source.value),
          js::Statement::ExportFromDeclaration(e) => crate::lossy(e.source.value),
          _ => continue,
        };
        if let Some(target) = self.resolve_import(&at, &source).filter(|t| !is_foreign(t)) {
          pending.push(target);
        }
      }
    }
  }

  fn resolve_import(&self, from: &str, source: &str) -> Option<String> {
    if self.reaches(Reach::Terminal) {
      if let Some((_, file, _)) = crate::terminal::COMPANIONS.iter().find(|(specifier, _, _)| *specifier == source) {
        return Some((*file).to_owned());
      }
    }
    let joined = crate::resolve_specifier(from, source)?;
    // TypeScript resolves a `.js` or `.jsx` specifier to the source it compiles to that name.
    let compiled: Vec<String> = match (joined.strip_suffix(".js"), joined.strip_suffix(".jsx")) {
      (Some(stem), _) => vec![format!("{stem}.ts"), format!("{stem}.tsx"), format!("{stem}.jsx")],
      (_, Some(stem)) => vec![format!("{stem}.tsx")],
      _ => Vec::new(),
    };
    let named = [joined.clone(), format!("{joined}.tsx"), format!("{joined}.ts"), format!("{joined}.jsx"), format!("{joined}.js")];
    let index = ["index.tsx", "index.ts", "index.jsx", "index.js"].map(|name| format!("{joined}/{name}"));
    for candidate in named.into_iter().chain(index).chain(compiled) {
      if candidate.starts_with("generated/") && candidate != crate::HEAD_MODULE && candidate != crate::CONTENT_MODULE {
        continue;
      }
      if self.provided.contains_key(&candidate) || self.app.join(&candidate).is_file() {
        return Some(candidate);
      }
    }
    None
  }

  fn lower_loaded(&mut self, file: &str, export: &str) -> Result<Component, LowerError> {
    let outer = std::mem::replace(&mut self.lowering_component, true);
    let (rewrites, continuations) = (self.rewrites.len(), self.continuations.len());
    let result = crate::with_heap_fresh(|| { self.rewrites.truncate(rewrites); self.continuations.truncate(continuations); self.lower_loaded_in(file, export) }, |component: &mut Component| component.body.insert(0, crate::heap_marker()));
    self.lowering_component = outer;
    result
  }

  fn lower_loaded_in(&mut self, file: &str, export: &str) -> Result<Component, LowerError> {
    self.load_class_graph(file);
    let mut globals: Vec<(String, Expr)> = Vec::new();
    let mut global_kinds = Vec::new();
    let mut global_formatters: Vec<(String, String)> = Vec::new();
    let continued = self.continuations.len();
    let mut module_bindings: Vec<(String, String, Expr)> = Vec::new();
    if self.reaches(Reach::Later) {
      let arena = Allocator::default();
      let parsed = self.parsed(&arena, file);
      for (name, _, written) in module_lets_of(&parsed) {
        if written {
          let seed = self.module_init(file, &name)?;
          module_bindings.push((name.clone(), format!("{file}#{name}"), seed));
        }
      }
    }
    let mut unfollowed: Vec<(String, Option<LowerError>)> = Vec::new();
    let module = format!("{file}#{export}");
    // An element template has no browser half, so nothing would read what
    // hoisting records or rewrites.
    let element_template = self.elements.values().any(|m| *m == module);
    let meanings = self.meanings(file);
    let arena = Allocator::default();
    let parsed = self.parsed(&arena, file);
    if let Some(span) = tree_call(&parsed, &meanings).filter(|_| export == "default") {
      return Err(LowerError::Retired(parsed.residue(span, "`tree(...)` is gone: a layout is composition and its page renders in a region of its own, so `export default` the layout itself and share state with the page through the store")));
    }
    let (imports, chosen_globals) = self.template_imports(file)?;
    let imports = Rc::new(imports);
    globals.extend(chosen_globals);
    let mut hook_files: Vec<(String, String, String)> = Vec::new();
    let mut variants: Vec<(String, String, Vec<Override>)>;
    let mut placed_kinds: Vec<(String, Vec<(String, crate::Kind)>)>;
    let mut prop_kinds: Vec<(String, crate::Kind)>;
    let mut relied: Vec<String>;
    let mut browser_texts: Vec<BrowserText>;
    let mut fn_refs: Vec<FnRef>;
    let mut positional_refs: Vec<String>;
    let ((component, refs, providers, own_heads), hoisting) = loop {
      let meanings = self.meanings(file);
      let hook_arena = Allocator::default();
      let hook_sources: Vec<(String, _, String)> = hook_files.iter().map(|(name, at, export)| (name.clone(), self.parsed(&hook_arena, at), export.clone())).collect();
      let (result, unbound, hoisting) = {
        let arena = Allocator::default();
        let parsed = self.parsed(&arena, file);
        let defaults = self.defaults.clone();
        let (function, class) = match find_function(&parsed, export) {
          Some(function) => (function, None),
          None => match find_class(&parsed, export) {
            Some((class, render)) => (render, Some(class)),
            None => {
              if let Some(span) = class_component_span(&parsed, export) {
                return Err(Lowerer::new(&parsed, &defaults).residue(span, CLASS_COMPONENT).into());
              }
              if let Some((span, how)) = made_component(&parsed, export) {
                return Err(Lowerer::new(&parsed, &defaults).residue(span, how).into());
              }
              if let Some(crate::meaning::Meaning::Own { file: declared, name: declared_name }) = default_ident(&parsed).filter(|_| export == "default").and_then(|id| meanings.bindings.get(id.name.as_str()).cloned()) {
                if !is_foreign(&declared) && declared != file {
                  self.load(&declared)?;
                  return self.lower_loaded(&declared, &declared_name);
                }
              }
              if let Some((source, orig, span)) = reexported(&parsed, export) {
                let declared = self.resolve_import(file, &source);
                match declared {
                  Some(declared) if !is_foreign(&declared) && declared != file => {
                    self.load(&declared)?;
                    return self.lower_loaded(&declared, &orig);
                  }
                  _ => return Err(Lowerer::new(&parsed, &defaults).residue(span, format!("`{export}` comes from `{source}`, which the build cannot follow")).into()),
                }
              }
              if let Some(id) = default_ident(&parsed).filter(|id| export == "default" && matches!(meanings.bindings.get(id.name.as_str()), Some(crate::meaning::Meaning::Foreign))) {
                let source = find_import(&parsed, id.name.as_str()).map(|(source, _)| source).unwrap_or_default();
                return Err(Lowerer::new(&parsed, &defaults).residue(id.span, format!("`{}` comes from `{source}`, which the build cannot follow", id.name)).into());
              }
              return Err(LowerError::MissingExport { file: file.to_owned(), export: export.to_owned() });
            }
          },
        };
        if let Some(span) = async_function_span(&parsed, export).filter(|_| !awaits_only_imports(&function)) {
          return Err(Lowerer::new(&parsed, &defaults).residue(span, ASYNC_COMPONENT).into());
        }
        let mut lowerer = Lowerer::new(&parsed, &defaults);
        lowerer.reaches = self.reaches.clone();
        lowerer.globals = globals.clone();
        lowerer.global_kinds = global_kinds.clone();
        lowerer.formatters_from(&global_formatters);
        lowerer.unfollowed = unfollowed.iter().map(|(name, _)| name.clone()).collect();
        lowerer.natives = self.natives.clone();
        if self.reaches(Reach::Later) {
          lowerer.cells = module_bindings.iter().map(|(name, _, _)| (name.clone(), Expr::Var(name.clone()))).collect();
        }
        // The browser half reads hoisted values through a hook, which a class's `render()` may not call.
        lowerer.hoisting = (!element_template && class.is_none()).then(Candidates::default);
        lowerer.classes = Some(self.classes.clone());
        lowerer.type_kinds = self.alias_kinds(file);
      lowerer.type_kinds = self.alias_kinds(file);
        lowerer.meanings = Some(meanings);
        lowerer.prop_kinds = self.placement_kinds.get(&module).cloned().unwrap_or_default();
        let layout_root = self.layouts.iter().any(|m| *m == module);
        let slot_names = self.slots.iter().find(|(m, _)| *m == module).map(|(_, names)| names.clone()).unwrap_or_default();
        let mut cl = ComponentLowerer { lowerer, file, handlers: Vec::new(), refs: Vec::new(), select_value: None, props_name: None, props_rest: None, children_name: None, forwards_ref: forwards_ref(&parsed, export), class, layout_root, slot_names, slot_props: Vec::new(), state: Vec::new(), hook: None, state_bindings: Vec::new(), store_bindings: Vec::new(), store_lets: Vec::new(), setters: Vec::new(), handler_fns: HashMap::new(), lowered_handlers: Vec::new(), event_binds: None, writers: Vec::new(), animations: 0, elements: self.elements.clone(), providers: Vec::new(), assets: self.assets.clone(), rewrite_images: self.rewrite_images, heads: Vec::new(), hooks: hook_sources.iter().map(|(name, parsed, export)| (name.clone(), parsed, export.clone())).collect(), wanted_hook: None, pending_lets: Vec::new(), hook_calls: 0, unbound_in: None, inlining: Vec::new(), checking: std::cell::RefCell::new(Vec::new()), counts_children: Vec::new(), imports: imports.clone(), chosen: HashMap::new(), variants: Vec::new(), placed_kinds: Vec::new(), browser_texts: Vec::new(), module_lets: module_bindings.clone(), fn_refs: Vec::new(), held_refs: Vec::new(), dispatchers: Vec::new(), transitions: Vec::new(), effect_locals: Vec::new(), effect_fns: Vec::new(), stable_callbacks: Vec::new(), spawns: Vec::new(), hook_ids: 0, cleanup_for: None, hook_renames: Vec::new(), named_listeners: Vec::new(), positional: self.positional.contains(&module), positional_refs: Vec::new() };
        let result = cl.component(&function);
        let consts = &self.consts;
        let counts_children = std::mem::take(&mut cl.counts_children);
        let leaves_to_browser = !cl.browser_texts.is_empty();
        let result = result.and_then(|(mut component, refs)| match fills::place_markup(&mut component).and_then(|()| fills::place_tags(&mut component, consts)) {
          Ok(()) => {
            for kind in counts_children.iter().rev() {
              component.body.insert(0, Stmt::Let { name: (*kind).to_owned(), expr: Expr::Field(Box::new(Expr::Var("$props".to_owned())), (*kind).to_owned()) });
            }
            if leaves_to_browser {
              component.body.insert(0, Stmt::Let { name: BROWSER_VAR.to_owned(), expr: Expr::Lit(Lit::Null) });
            }
            Ok((component, refs))
          }
          Err(why) => Err(cl.lowerer.residue(match &function { Found::Declared(_, _, block) => *block, Found::Arrow(arrow) => arrow.span }, why)),
        });
        let result = result.map(|(component, refs)| (component, refs, std::mem::take(&mut cl.providers), std::mem::take(&mut cl.heads)));
        variants = std::mem::take(&mut cl.variants);
        placed_kinds = std::mem::take(&mut cl.placed_kinds);
        browser_texts = std::mem::take(&mut cl.browser_texts);
        fn_refs = std::mem::take(&mut cl.fn_refs);
        positional_refs = std::mem::take(&mut cl.positional_refs);
        prop_kinds = std::mem::take(&mut cl.lowerer.prop_kinds);
        relied = std::mem::take(&mut *cl.lowerer.consulted.borrow_mut());
        if leaves_to_browser {
          cl.state.push(BROWSER_VAR.to_owned());
        }
        let hoisting = cl.lowerer.hoisting.take().map(|candidates| (candidates, std::mem::take(&mut cl.state), cl.hook.take()));
        let result = match result {
          Err(residue) if cl.lowerer.reach_violation => return Err(LowerError::Reach(residue)),
          other => other,
        };
        (result, (cl.lowerer.unbound.take(), cl.wanted_hook.take(), cl.unbound_in.take()), hoisting)
      };
      let (unbound, wanted_hook, unbound_in) = unbound;
      match result {
        Ok(done) => break (done, hoisting),
        Err(residue) => {
          if let Some((name, origin)) = wanted_hook {
            if let Some((declared, declared_name)) = origin.filter(|_| !hook_files.iter().any(|(n, _, _)| *n == name)) {
              if self.load(&declared).is_ok() && find_function(&self.parsed(&Allocator::default(), &declared), &declared_name).is_some() {
                self.meanings(&declared);
                hook_files.push((name, declared, declared_name));
                continue;
              }
            }
            return Err(residue.into());
          }
          let Some(name) = unbound else { return Err(residue.into()) };
          let file = unbound_in.as_deref().unwrap_or(file);
          if globals.iter().any(|(n, _)| *n == name) {
            return Err(residue.into());
          }
          let (found, why) = match self.global(file, &name) {
            Ok(found) => (found, None),
            Err(why @ LowerError::Residue(_)) => (None, Some(why)),
            Err(other) => return Err(other),
          };
          let Some((expr, key)) = found else {
            if let Some(at) = unfollowed.iter().position(|(n, _)| *n == name) {
              return Err(unfollowed.swap_remove(at).1.unwrap_or_else(|| residue.into()));
            }
            unfollowed.push((name, why));
            continue;
          };
          if let Some(kind) = self.global_kind(file, &name)? {
            global_kinds.push((name.clone(), kind));
          }
          if let Some(kind) = self.global_return_kind(file, &name)? {
            global_kinds.push((crate::returned_by(&name), kind));
          }
          if let Some(made) = self.global_formatter(file, &name)? {
            global_formatters.push((name.clone(), made));
          }
          if let Some((_, declared_file, declared_name)) = self.module_lets.iter().find(|(k, _, _)| *k == key).cloned().filter(|_| !module_bindings.iter().any(|(n, _, _)| *n == name)) {
            let seed = self.module_init(&declared_file, &declared_name)?;
            module_bindings.push((name.clone(), key.clone(), seed));
          }
          {
            let declared_file = key.split_once('#').map(|(f, _)| f.to_owned()).unwrap_or_default();
            let arena = Allocator::default();
            let parsed = self.parsed(&arena, &declared_file);
            let mut read = Vec::new();
            expr.free_vars(&mut read);
            for (held, _, written) in module_lets_of(&parsed) {
              if written && (read.contains(&held) || held_in_a_cell(&expr, &held)) && !module_bindings.iter().any(|(n, _, _)| *n == held) {
                let seed = self.module_init(&declared_file, &held)?;
                module_bindings.push((held.clone(), format!("{declared_file}#{held}"), seed));
              }
            }
          }
          if crate::writes_outer(&expr) {
            let declared_file = key.split_once('#').map(|(f, _)| f.to_owned()).unwrap_or_default();
            if let Expr::Lambda { body, .. } = &expr {
              if let Expr::Block(block) = &**body {
                for written in snapfire_fsr_ir::ast::set_targets(block) {
                  let written_key = format!("{declared_file}#{written}");
                  if module_bindings.iter().any(|(n, _, _)| *n == written) || !self.module_lets.iter().any(|(k, _, _)| *k == written_key) {
                    continue;
                  }
                  let seed = self.module_init(&declared_file, &written)?;
                  module_bindings.push((written.clone(), written_key, seed));
                }
              }
            }
          }
          let expr = self.name_if_large(key, expr);
          globals.push((name, expr));
        }
      }
    };
    for name in &positional_refs {
      self.positional.insert(format!("{file}#{name}"));
    }
    for (variant, base, overrides) in variants {
      self.variants.insert(variant, (base, overrides));
    }
    relied.sort();
    relied.dedup();
    self.relied.insert(module.clone(), prop_kinds.iter().filter(|(prop, _)| relied.contains(prop)).cloned().collect());
    self.prop_kinds.insert(module.clone(), prop_kinds);
    for text in &browser_texts {
      self.browser_texts.push((module.clone(), format!("{file}:{}:{}: {}", text.line, text.column, text.reason)));
    }
    let marks: Vec<std::ops::Range<usize>> = browser_texts.iter().filter_map(|text| text.wrap.clone()).collect();
    let leaves_to_browser = !browser_texts.is_empty();
    let mut modules: HashMap<String, String> = HashMap::new();
    let mut islands: HashMap<String, IslandTiming> = HashMap::new();
    let refs_positions: Vec<(String, (usize, usize))> = refs.clone();
    for (name, (line, column)) in refs {
      let (module, island) = self.component_module(file, &name).map_err(|message| Residue { file: file.to_owned(), line, column, message, hint: None, via: Vec::new() })?;
      let passed = agreed_kinds(&placed_kinds, &name);
      if !passed.is_empty() {
        self.placement_kinds.entry(module.clone()).or_insert_with(|| passed.clone());
      }
      if is_foreign(&module) {
        let target = module.split_once('#').map(|(f, _)| f).unwrap_or(&module).to_owned();
        let residue = match (self.described.contains_key(&target), self.undescribed.get(&target)) {
          (true, _) => match self.lower(&module) {
            Ok(()) => None,
            Err(LowerError::Residue(residue)) => Some(residue),
            Err(LowerError::Parse { file, message }) => Some(Residue { file, line: 1, column: 1, message, hint: None, via: Vec::new() }),
            Err(other) => return Err(other),
          },
          (false, Some(why)) => Some(Residue { file: target.clone(), line: 1, column: 1, message: why.clone(), hint: None, via: Vec::new() }),
          (false, None) => None,
        };
        let lowered = self.components.iter().any(|(m, _)| *m == module);
        if !lowered && !self.foreign.contains(&module) {
          self.foreign.push(module.clone());
        }
        if let Some(residue) = residue {
          if !self.foreign_residue.iter().any(|(m, _)| *m == module) {
            self.foreign_residue.push((module.clone(), residue));
          }
        }
      } else {
        match self.lower(&module) {
          Ok(()) => {}
          Err(LowerError::Residue(residue)) => {
            let residue = residue.placed_at(Placement { file: file.to_owned(), line, column, tag: name.clone() });
            if residue.message == ASYNC_COMPONENT {
              return Err(residue.into());
            }
            if !self.browser_only.iter().any(|(m, _)| *m == module) {
              self.browser_only.push((module.clone(), residue));
            }
          }
          Err(other) => return Err(other),
        }
        if let Some((prop, kind, given)) = self.relied.get(&module).into_iter().flatten().find_map(|(prop, kind)| {
          let given = passed.iter().find(|(p, _)| p == prop).map(|(_, k)| *k)?;
          (given != *kind).then_some((prop.clone(), *kind, given))
        }) {
          return Err(LowerError::Residue(Residue {
            file: file.to_owned(),
            line,
            column,
            message: format!("`{name}` reads its `{prop}` prop as a `{}` and this placement passes a `{}`", kind.name(), given.name()),
            hint: Some(format!("pass `{name}` a `{}` or make `{prop}` a prop of its own kind", kind.name())),
            via: Vec::new(),
          }));
        }
      }
      let placed = format!("{file}#{name}");
      if let Some(timing) = island {
        islands.insert(placed.clone(), timing);
      }
      modules.insert(placed, module);
    }
    // A placement names its component `<this file>#<tag>` until the tree is
    // rewritten to the modules `modules` resolved.
    let renders_in_browser = |placed: &str| {
      let module = modules.get(placed).map(String::as_str).unwrap_or(placed);
      self.components.iter().find(|(m, _)| m == module).is_none_or(|(_, c)| c.owner.hydrates())
    };
    let mut component = component;
    write_types(&mut component.render, &|placed: &str| {
      let tag = placed.rsplit_once('#').map(|(_, tag)| tag).unwrap_or(placed);
      let mut kinds = agreed_kinds(&placed_kinds, tag);
      for (prop, kind) in modules.get(placed).and_then(|module| self.prop_kinds.get(module)).into_iter().flatten() {
        if !kinds.iter().any(|(p, _)| p == prop) {
          kinds.push((prop.clone(), *kind));
        }
      }
      kinds
    });
    fills::unfill(&mut component.render, &renders_in_browser);
    let mut used: Vec<usize> = Vec::new();
    if let Err((placed, prop, reason)) = fills::reference_functions(&mut component.render, &renders_in_browser, &fn_refs, &mut used) {
      let tag = placed.rsplit_once('#').map(|(_, tag)| tag.to_owned()).unwrap_or_else(|| placed.clone());
      let (line, column) = refs_positions.iter().find(|(name, _)| *name == tag).map(|(_, pos)| *pos).unwrap_or((1, 1));
      let message = match reason {
        Some(reason) => format!("a function passed as `{prop}` to `{tag}`, which renders in the browser, {reason}"),
        None => format!("a function passed as `{prop}` to `{tag}`, which renders in the browser; the server holds no function the browser can call"),
      };
      return Err(LowerError::Residue(Residue { file: file.to_owned(), line, column, message, hint: Some(format!("move what `{prop}` does into `{tag}` or make `{tag}` render on the server")), via: Vec::new() }));
    }
    for index in used {
      for need in &fn_refs[index].needs {
        match need {
          Need::Export(range, name) => {
            if !self.fn_exports.iter().any(|(f, r, _)| f == file && *r == *range) {
              self.fn_exports.push((file.to_owned(), range.clone(), name.clone()));
            }
          }
          Need::Name(name) => {
            if !self.fn_names.iter().any(|(f, n)| f == file && n == name) {
              self.fn_names.push((file.to_owned(), name.clone()));
            }
          }
          Need::File(from) => {
            if !self.fn_files.contains(from) {
              self.fn_files.push(from.clone());
            }
          }
        }
      }
    }
    let mut heads: Vec<HeadRow> = own_heads;
    for placed in modules.values() {
      for row in self.heads.get(placed).cloned().unwrap_or_default() {
        if !heads.contains(&row) {
          heads.push(row);
        }
      }
    }
    if !heads.is_empty() {
      self.heads.insert(module.clone(), heads);
    }
    // A template with nothing for the browser to change is never mounted, so
    // it has no browser twin and pulls no framework into the page. A component
    // it renders inline is part of its markup, so that component's state and
    // handlers are its own to hydrate; an island's are the island's.
    let mut provides = false;
    for (name, (line, column), react) in providers {
      let Some(react) = react else {
        return Err(LowerError::Residue(Residue { file: file.to_owned(), line, column, message: format!("`<{name}.Provider>`: `{name}` is not a `createContext` value this file declares or imports"), hint: None, via: Vec::new() }));
      };
      provides |= react;
    }
    let render = rewrite_modules(component.render, &modules, &islands);
    // A React provider is React state the page's islands read, so the
    // component is a root even when nothing else in it needs the browser. The
    // server resolves a template dialect's whole.
    let hydrates = !component.state.is_empty() || !component.handlers.is_empty() || provides || leaves_to_browser || self.inline_hydrates(&render);
    let owner = if hydrates { Owner::React } else { Owner::Fsr };
    let render = match owner {
      Owner::React => match mount_foreign(render, &mut self.mounted) {
        Ok(render) => render,
        Err(placed) => {
          let (name, (line, column)) = refs_by_module(&modules, &placed, &refs_positions).unwrap_or((placed.clone(), (1, 1)));
          return Err(LowerError::ForeignChildren(Residue { file: file.to_owned(), line, column, message: format!("`{name}` is a component another framework mounts, so it takes no children from a React component"), hint: None, via: Vec::new() }));
        }
      },
      _ => render,
    };
    let mut component = Component { body: component.body, render, state: component.state, stores: component.stores, handlers: component.handlers, owner, shadow: component.shadow, scope: component.scope };
    if let Some(placed) = inline_foreign(&component.render) {
      let (name, (line, column)) = refs_by_module(&modules, &placed, &refs_positions).unwrap_or((placed.clone(), (1, 1)));
      return Err(LowerError::Residue(Residue {
        file: file.to_owned(),
        line,
        column,
        message: format!("`{name}` is a component another framework mounts, so it can only be an island; place it inside `<Island>`"),
        hint: None,
        via: Vec::new(),
      }));
    }
    if let Some((candidates, state, Some(hook))) = hoisting {
      let mut taint = state.clone();
      if self.moving_props.contains(&module) {
        taint.push(hoist::PROPS.to_owned());
      }
      let mut kept = hoist::decide(&mut component, &taint);
      let pure = state.is_empty() && component.handlers.is_empty() && !leaves_to_browser && hoist::static_tree(&component.render, &self.pure);
      let chunks = hoist::chunks(&mut component, &taint, &self.pure);
      let covered = hoist::cover(&mut component, &chunks);
      kept.retain(|id| !covered.contains(id));
      for placed in hoist::placed_with_state(&component, &taint) {
        self.unhoist_props(&placed);
      }
      self.pure.insert(module.clone(), pure);
      self.stateless.insert(module.clone(), state.is_empty() && !leaves_to_browser);
      let arena = Allocator::default();
      let parsed = self.parsed(&arena, file);
      for range in candidates.remaining(&kept, &chunks) {
        let (line, column) = parsed.position(range.start);
        self.remaining.push((module.clone(), format!("{file}:{line}:{column}")));
      }
      let mut islands = Vec::new();
      island_ids(&component.render, &mut islands);
      let placed: Vec<u32> = candidates.island_sites.iter().map(|(id, ..)| *id).collect();
      let mut keyed = Vec::new();
      mark_keyed(&mut component.render, &placed, &|callee| self.keys.get(callee).copied().unwrap_or(true), &|callee| !self.pure.get(callee).copied().unwrap_or(false), &mut keyed);
      let marks_hook = hook.clone();
      let rewrite = candidates.rewrite(&kept, &chunks, &islands, &keyed, file, &module, hook);
      self.keys.insert(module.clone(), rewrite.is_some());
      match rewrite {
        Some(mut rewrite) => {
          rewrite.browser = marks;
          self.rewrites.push(rewrite);
        }
        None if !marks.is_empty() => self.rewrites.push(Rewrite { file: file.to_owned(), module: module.clone(), hook: marks_hook, sites: Vec::new(), chunks: Vec::new(), islands: Vec::new(), placements: Vec::new(), ids: Vec::new(), loops: Vec::new(), statement_loops: Vec::new(), browser: marks }),
        None => {}
      }
    } else {
      self.keys.insert(module.clone(), false);
    }
    let mut component = component;
    for handler in self.continuations.drain(continued..).collect::<Vec<_>>() {
      if !component.handlers.iter().any(|held| held.event == handler.event) {
        component.handlers.push(handler);
      }
    }
    Ok(component)
  }

  /// Lowers a described `.vue` file through the Vue front end. The script
  /// block is parsed padded to its line in the file, so every position a
  /// residue names is the file's own.
  fn lower_vue(&mut self, file: &str, export: &str) -> Result<Component, LowerError> {
    if export != "default" {
      return Err(LowerError::MissingExport { file: file.to_owned(), export: export.to_owned() });
    }
    let module = format!("{file}#{export}");
    let described = self.described.get(file).cloned().expect("a described file");
    self.parse_described(file, &described)?;
    self.meanings(file);
    let mut globals: Vec<(String, Expr)> = Vec::new();
    let mut global_kinds = Vec::new();
    let component = loop {
      let (result, unbound) = {
        let arena = Allocator::default();
        let parsed = self.parsed(&arena, file);
        let defaults = self.defaults.clone();
        let mut lowerer = Lowerer::new(&parsed, &defaults);
        lowerer.reaches = self.reaches.clone();
        lowerer.globals = globals.clone();
        lowerer.global_kinds = global_kinds.clone();
        lowerer.natives = self.natives.clone();
        lowerer.render_path = true;
        let mut vue = crate::vue::VueLowerer::new(lowerer, &described, self.assets.clone());
        let result = vue.component();
        let result = match result {
          Err(residue) if vue.lowerer.reach_violation => return Err(LowerError::Reach(residue)),
          other => other,
        };
        if result.is_ok() && !vue.heads.is_empty() {
          self.heads.insert(module.clone(), std::mem::take(&mut vue.heads));
        }
        let result = result.map(|component| (component, std::mem::take(&mut vue.child_refs)));
        (result, vue.lowerer.unbound.take())
      };
      match result {
        Ok(component) => break component,
        Err(residue) => {
          let Some(name) = unbound else { return Err(residue.into()) };
          if globals.iter().any(|(n, _)| *n == name) {
            return Err(residue.into());
          }
          let Some((expr, key)) = self.global(file, &name)? else { return Err(residue.into()) };
          if let Some(kind) = self.global_kind(file, &name)? {
            global_kinds.push((name.clone(), kind));
          }
          if let Some(kind) = self.global_return_kind(file, &name)? {
            global_kinds.push((crate::returned_by(&name), kind));
          }
          let expr = self.name_if_large(key, expr);
          globals.push((name, expr));
        }
      }
    };
    let (mut component, mut children) = component;
    let mut referenced: Vec<String> = Vec::new();
    component.visit(&mut |e| {
      if let Expr::Builtin { name: Builtin::ComponentRef, args } = e {
        if let [Expr::Lit(Lit::Str(local))] = args.as_slice() {
          if !referenced.contains(local) {
            referenced.push(local.clone());
          }
        }
      }
    });
    children.extend(referenced.into_iter().map(|local| crate::vue::ChildRef { local, attrs: Vec::new(), written: Vec::new(), line: 1, column: 1 }));
    let mut modules: HashMap<String, String> = HashMap::new();
    let own_root = crate::vue::root_kind(&component.render);
    let mut fallthrough: HashMap<String, Vec<(String, String)>> = HashMap::new();
    for child in children {
      let at = |message: String| LowerError::Residue(Residue { file: file.to_owned(), line: child.line, column: child.column, message, hint: None, via: Vec::new() });
      let target = match modules.get(&child.local) {
        Some(target) => target.clone(),
        None if child.local == crate::svelte::SELF => module.clone(),
        None => {
          let (target, _) = self.component_module(file, &child.local).map_err(at)?;
          match self.lower(&target) {
            Ok(()) => {}
            Err(LowerError::Residue(residue)) => return Err(at(format!("`<{}>` does not lower: {}:{}:{}: {}", child.local, residue.file, residue.line, residue.column, residue.message))),
            Err(other) => return Err(other),
          }
          target
        }
      };
      let child_file = target.split_once('#').map(|(f, _)| f.to_owned()).unwrap_or_else(|| target.clone());
      let mut declared: Vec<String> = self.described.get(&child_file).map(|d| d.bindings.iter().filter(|(_, kind)| kind.as_str() == "props").map(|(name, _)| name.clone()).collect()).unwrap_or_default();
      let child_arena = Allocator::default();
      let child_parsed = self.parsed_if(&child_arena, &child_file);
      if let Some(parsed) = &child_parsed {
        declared.extend(crate::vue::model_props(parsed));
        declared.extend(crate::vue::option_props(parsed));
      }
      let strays: Vec<(String, String)> = child.attrs.iter().zip(&child.written).filter(|(name, _)| !declared.contains(name)).map(|(name, written)| (name.clone(), written.clone())).collect();
      if !strays.is_empty() {
        let tag = if child.local == crate::svelte::SELF { "itself" } else { child.local.as_str() };
        if strays.iter().any(|(name, _)| name == "style") {
          return Err(at(format!("`style` on `<{tag}>`, which it does not declare as a prop; Vue merges it into its root element's style, which the build does not")));
        }
        let root = match child.local == crate::svelte::SELF {
          true => own_root,
          false => self.components.iter().find(|(m, _)| *m == target).map_or(crate::vue::RootKind::Element, |(_, c)| crate::vue::root_kind(&c.render)),
        };
        let inherits = child_parsed.as_ref().is_none_or(crate::vue::inherits_attrs);
        match root {
          _ if !inherits => {}
          crate::vue::RootKind::Element => {}
          crate::vue::RootKind::Several => return Err(at(format!("`{}` on `<{tag}>`, which it does not declare as a prop; Vue falls it through onto a single root element and this one has several", strays[0].1))),
          crate::vue::RootKind::Component => return Err(at(format!("`{}` on `<{tag}>`, which it does not declare as a prop; its root is a component, which Vue would hand it to", strays[0].1))),
        }
        fallthrough.entry(child.local.clone()).or_default().extend(strays.into_iter().map(|(name, written)| (name, if inherits { written } else { String::new() })));
      }
      modules.insert(child.local, target);
    }
    if !fallthrough.is_empty() {
      crate::vue::fall_through(&mut component.render, &fallthrough);
    }
    if !modules.is_empty() {
      component.render = rewrite_modules(component.render, &modules, &HashMap::new());
      let mut referenced = |e: &mut Expr| {
        if let Expr::Builtin { name: Builtin::ComponentRef, args } = e {
          if let [Expr::Lit(Lit::Str(local))] = args.as_mut_slice() {
            if let Some(target) = modules.get(local.as_str()) {
              *local = target.clone();
            }
          }
        }
      };
      snapfire_fsr_ir::ast::body_visit_mut(&mut component.body, &mut referenced);
      component.render.visit_mut(&mut referenced);
    }
    self.keys.insert(module.clone(), false);
    self.pure.insert(module.clone(), false);
    self.stateless.insert(module, component.state.is_empty());
    Ok(component)
  }

  /// Whether `name` in `file` is a function of the application whose every statement the browser alone runs: a `svelte` lifecycle call or a timer it starts.
  fn runs_in_browser_only(&mut self, file: &str, name: &str) -> bool {
    let Some((declared, declared_name)) = self.meanings(file).bindings.get(name).and_then(|meant| meant.origin()).map(|(f, n)| (f.to_owned(), n.to_owned())) else { return false };
    if self.load(&declared).is_err() {
      return false;
    }
    let arena = Allocator::default();
    let parsed = self.parsed(&arena, &declared);
    let Some(Found::Declared(_, stmts, _)) = find_function(&parsed, &declared_name) else { return false };
    let lifecycle = |callee: &js::Expression| match callee {
      js::Expression::Identifier(id) if parsed.ctxt(id) == crate::Ctxt::Top => find_import(&parsed, id.name.as_str()).is_some_and(|(source, imported)| source == "svelte" && matches!(imported.as_str(), "onMount" | "onDestroy" | "beforeUpdate" | "afterUpdate")),
      _ => false,
    };
    let timer = |expr: &js::Expression| match expr {
      js::Expression::CallExpression(call) => matches!(&call.callee, js::Expression::Identifier(id) if parsed.ctxt(id) == crate::Ctxt::Unresolved && matches!(id.name.as_str(), "setInterval" | "setTimeout" | "requestAnimationFrame")),
      _ => false,
    };
    !stmts.is_empty()
      && stmts.iter().all(|stmt| match stmt {
        js::Statement::ExpressionStatement(e) => matches!(&e.expression, js::Expression::CallExpression(call) if lifecycle(&call.callee)) || timer(&e.expression),
        js::Statement::VariableDeclaration(var) => var.declarations.iter().all(|decl| matches!(&decl.id, js::BindingPattern::BindingIdentifier(_)) && decl.init.as_ref().is_some_and(timer)),
        js::Statement::EmptyStatement(_) => true,
        _ => false,
      })
  }

  /// A Svelte component the plugin described, lowered through the Svelte front end the way `lower_vue` lowers a Vue one.
  fn lower_svelte(&mut self, file: &str, export: &str) -> Result<Component, LowerError> {
    if export != "default" {
      return Err(LowerError::MissingExport { file: file.to_owned(), export: export.to_owned() });
    }
    let module = format!("{file}#{export}");
    let described = self.described.get(file).cloned().expect("a described file");
    self.parse_described(file, &described)?;
    self.meanings(file);
    let mut globals: Vec<(String, Expr)> = Vec::new();
    let mut global_kinds = Vec::new();
    let mut browser_effects: Vec<String> = Vec::new();
    let component = loop {
      let (result, unbound) = {
        let arena = Allocator::default();
        let parsed = self.parsed(&arena, file);
        let defaults = self.defaults.clone();
        let mut lowerer = Lowerer::new(&parsed, &defaults);
        lowerer.reaches = self.reaches.clone();
        lowerer.globals = globals.clone();
        lowerer.global_kinds = global_kinds.clone();
        lowerer.natives = self.natives.clone();
        lowerer.render_path = true;
        lowerer.browser_effects = browser_effects.clone();
        let mut vue = crate::svelte::SvelteLowerer::new(lowerer, &described, self.assets.clone());
        let result = vue.component();
        let result = match result {
          Err(residue) if vue.lowerer.reach_violation => return Err(LowerError::Reach(residue)),
          other => other,
        };
        if result.is_ok() && !vue.heads.is_empty() {
          self.heads.insert(module.clone(), std::mem::take(&mut vue.heads));
        }
        let result = result.map(|component| (component, std::mem::take(&mut vue.child_refs)));
        (result, vue.lowerer.unbound.take())
      };
      match result {
        Ok(component) => break component,
        Err(residue) => {
          let Some(name) = unbound else { return Err(residue.into()) };
          if globals.iter().any(|(n, _)| *n == name) || browser_effects.contains(&name) {
            return Err(residue.into());
          }
          if self.runs_in_browser_only(file, &name) {
            browser_effects.push(name);
            continue;
          }
          let store = name.strip_prefix('$').filter(|base| !base.starts_with('$'));
          let Some((expr, key)) = (match store {
            Some(base) => self.global(file, base)?,
            None => self.global(file, &name)?,
          }) else {
            return Err(residue.into());
          };
          if let Some(kind) = self.global_kind(file, &name)? {
            global_kinds.push((name.clone(), kind));
          }
          if let Some(kind) = self.global_return_kind(file, &name)? {
            global_kinds.push((crate::returned_by(&name), kind));
          }
          let expr = self.name_if_large(key, expr);
          globals.push((name, expr));
        }
      }
    };
    let (mut component, mut children) = component;
    let mut referenced: Vec<String> = Vec::new();
    component.visit(&mut |e| {
      if let Expr::Builtin { name: Builtin::ComponentRef, args } = e {
        if let [Expr::Lit(Lit::Str(local))] = args.as_slice() {
          if !referenced.contains(local) {
            referenced.push(local.clone());
          }
        }
      }
    });
    children.extend(referenced.into_iter().map(|local| crate::vue::ChildRef { local, attrs: Vec::new(), written: Vec::new(), line: 1, column: 1 }));
    let mut modules: HashMap<String, String> = HashMap::new();
    for child in children {
      if child.local == crate::svelte::SELF {
        modules.insert(child.local, module.clone());
        continue;
      }
      if modules.contains_key(&child.local) {
        continue;
      }
      let at = |message: String| LowerError::Residue(Residue { file: file.to_owned(), line: child.line, column: child.column, message, hint: None, via: Vec::new() });
      let (target, _) = self.component_module(file, &child.local).map_err(at)?;
      match self.lower(&target) {
        Ok(()) => {}
        Err(LowerError::Residue(residue)) => return Err(at(format!("`<{}>` does not lower: {}:{}:{}: {}", child.local, residue.file, residue.line, residue.column, residue.message))),
        Err(other) => return Err(other),
      }
      modules.insert(child.local, target);
    }
    if !modules.is_empty() {
      component.render = rewrite_modules(component.render, &modules, &HashMap::new());
      let mut referenced = |e: &mut Expr| {
        if let Expr::Builtin { name: Builtin::ComponentRef, args } = e {
          if let [Expr::Lit(Lit::Str(local))] = args.as_mut_slice() {
            if let Some(target) = modules.get(local.as_str()) {
              *local = target.clone();
            }
          }
        }
      };
      snapfire_fsr_ir::ast::body_visit_mut(&mut component.body, &mut referenced);
      component.render.visit_mut(&mut referenced);
    }
    self.keys.insert(module.clone(), false);
    self.pure.insert(module.clone(), false);
    self.stateless.insert(module, component.state.is_empty());
    Ok(component)
  }

  /// Whether a component's own markup is something the browser mounts: an
  /// element with an event handler, lowered or not. A component it renders
  /// that a framework hydrates does not count: rendered from composition it
  /// is placed as an island of its own.
  fn inline_hydrates(&self, tmpl: &Tmpl) -> bool {
    match tmpl {
      Tmpl::Component { children, .. } => children.iter().any(|c| self.inline_hydrates(c)),
      Tmpl::Element { attrs, children, .. } => attrs.iter().any(|a| matches!(a, Entry::Field(n, _) if n.starts_with(HANDLER_ATTR) || n == UNLOWERED_ATTR)) || children.iter().any(|c| self.inline_hydrates(c)),
      Tmpl::Island { children, .. } | Tmpl::Fragment(children) | Tmpl::Place { children, .. } => children.iter().any(|c| self.inline_hydrates(c)),
      Tmpl::Baked { children, .. } => children.iter().any(|c| self.inline_hydrates(c)),
      Tmpl::If { then, r#else, .. } => self.inline_hydrates(then) || r#else.as_ref().is_some_and(|e| self.inline_hydrates(e)),
      Tmpl::For { body, .. } => self.inline_hydrates(body),
      Tmpl::Let { then, .. } => self.inline_hydrates(then),
      Tmpl::Text(_) | Tmpl::Marker(_) | Tmpl::Outlet { .. } | Tmpl::Html(_) | Tmpl::Expr(_) | Tmpl::Slot(_) => false,
    }
  }

  /// The module id a capitalised JSX tag names: a function in this file or a
  /// local import, lowered as its own component; `Ns.Name` is `Name` from the
  /// file a namespace import binds as `Ns`.
  fn component_module(&mut self, file: &str, name: &str) -> Result<(String, Option<IslandTiming>), String> {
    use crate::meaning::Meaning;
    if let Some((target, export)) = name.split_once('#') {
      self.load(target).map_err(|e| e.to_string())?;
      return self.exported_component(target, export);
    }
    let meanings = self.meanings(file);
    let (looked, meant) = match name.split_once('.') {
      Some((namespace, member)) => match meanings.bindings.get(namespace) {
        Some(ns @ Meaning::Namespace(_)) => (namespace, ns.member(member)),
        Some(Meaning::Foreign) => (namespace, Some(Meaning::Foreign)),
        _ => return Err(format!("`{namespace}` is not a namespace import; `<{name}>` needs `import * as {namespace}`")),
      },
      None => (name, meanings.bindings.get(name).cloned()),
    };
    match meant {
      Some(Meaning::Own { file: declared, name: declared_name }) => {
        if is_foreign(&declared) {
          return Ok((format!("{declared}#{declared_name}"), None));
        }
        self.load(&declared).map_err(|e| e.to_string())?;
        self.exported_component(&declared, &declared_name)
      }
      Some(Meaning::Foreign) => {
        let arena = Allocator::default();
        let parsed = self.parsed(&arena, file);
        let lazy = || match find_value(&parsed, looked) {
          Some(Global::Const(js::Expression::CallExpression(call))) => lazy_import_source(call),
          _ => None,
        };
        let source = find_import(&parsed, looked).map(|(source, _)| source).or_else(lazy).unwrap_or_default();
        Err(format!("`{looked}` comes from `{source}`, which the build cannot follow"))
      }
      _ => Err(format!("`{name}` is not a component this file declares or imports")),
    }
  }

  /// `export` of `file` as a component module: the function itself, or, when
  /// the export is `island(Comp, { when, mode })`, the component the alias
  /// names with the timing and mode it set, so a page importing the alias
  /// places the island the way the aliasing module declared it.
  fn exported_component(&mut self, file: &str, export: &str) -> Result<(String, Option<IslandTiming>), String> {
    let arena = Allocator::default();
    let parsed = self.parsed(&arena, file);
    let meanings = self.meanings(file);
    match island_alias_of(&parsed, &meanings, export) {
      Ok(Some(alias)) => {
        let (module, _) = self.component_module(file, &alias.target)?;
        Ok((module, Some((alias.when, alias.mode))))
      }
      Ok(None) => Ok((format!("{file}#{export}"), None)),
      Err((_, message)) => Err(message),
    }
  }

  /// What every module-level name of `file` and every name it exports means,
  /// resolved through imports, namespaces, re-exports and module-level `const`
  /// aliases, once per file.
  pub(crate) fn meanings(&mut self, file: &str) -> Rc<crate::meaning::Meanings> {
    use crate::meaning::{self, Meaning, Meanings};
    if let Some(found) = self.meanings.get(file) {
      return found.clone();
    }
    if let Some(described) = self.described.get(file).cloned() {
      let _ = self.parse_described(file, &described);
    }
    if self.load(file).is_err() || self.meaning_in_progress.iter().any(|f| f == file) {
      return Rc::new(Meanings::default());
    }
    self.meaning_in_progress.push(file.to_owned());
    let arena = Allocator::default();
    let parsed = self.parsed(&arena, file);
    let mut meanings = Meanings { scope: Some(parsed.scope.clone()), ..Meanings::default() };
    for item in &parsed.module().body {
      let js::Statement::ImportDeclaration(import) = item else { continue };
      if import.import_kind.is_type() {
        continue;
      }
      let source = crate::lossy(import.source.value);
      for spec in import.specifiers.iter().flatten() {
        let (local, meant) = match spec {
          js::ImportDeclarationSpecifier::ImportSpecifier(named) if !named.import_kind.is_type() => (&named.local, self.imported_meaning(file, &source, &crate::schema::export_name(&named.imported))),
          js::ImportDeclarationSpecifier::ImportSpecifier(_) => continue,
          js::ImportDeclarationSpecifier::ImportDefaultSpecifier(d) if meaning::is_library(&source) => match meaning::library(&source, "default") {
            Some(construct) => (&d.local, Meaning::Library(construct)),
            None => (&d.local, Meaning::Namespace(meaning::library_namespace(&source))),
          },
          js::ImportDeclarationSpecifier::ImportDefaultSpecifier(d) if assets::kind(&source).is_some() => (&d.local, Meaning::Asset { source: source.clone() }),
          js::ImportDeclarationSpecifier::ImportDefaultSpecifier(d) => (&d.local, self.imported_meaning(file, &source, "default")),
          js::ImportDeclarationSpecifier::ImportNamespaceSpecifier(ns) => (&ns.local, self.namespace_meaning(file, &source)),
        };
        meanings.bindings.insert(local.name.to_string(), meant);
      }
    }
    for item in &parsed.module().body {
      if let js::Statement::ExportDefaultDeclaration(d) = item {
        let named = match &d.declaration {
          js::ExportDefaultDeclarationKind::FunctionDeclaration(f) => f.id.as_ref(),
          js::ExportDefaultDeclarationKind::ClassDeclaration(c) => c.id.as_ref(),
          _ => None,
        };
        if let Some(id) = named {
          meanings.bindings.insert(id.name.to_string(), Meaning::Own { file: file.to_owned(), name: "default".to_owned() });
        }
        continue;
      }
      let decl = match item {
        js::Statement::ExportDeclaration(export) => &export.declaration,
        other => match other.as_declaration() {
          Some(decl) => decl,
          None => continue,
        },
      };
      let var = match decl {
        js::Declaration::VariableDeclaration(var) => var,
        decl => {
          let name = match decl {
            js::Declaration::FunctionDeclaration(f) => f.id.as_ref().map(|id| id.name.to_string()),
            js::Declaration::ClassDeclaration(c) => c.id.as_ref().map(|id| id.name.to_string()),
            _ => None,
          };
          if let Some(name) = name {
            meanings.bindings.insert(name.clone(), Meaning::Own { file: file.to_owned(), name });
          }
          continue;
        }
      };
      for decl in &var.declarations {
        let js::BindingPattern::BindingIdentifier(name) = &decl.id else { continue };
        let own = || Meaning::Own { file: file.to_owned(), name: name.name.to_string() };
        let meant = match decl.init.as_ref().map(crate::meaning::bare) {
          Some(js::Expression::CallExpression(call)) => match meanings.of_expr(&call.callee) {
            Some(Meaning::Library(meaning::Construct::CreateContext { react })) => {
              let defaults = self.defaults.clone();
              let default = match call.arguments.first() {
                Some(arg) => Lowerer::new(&parsed, &defaults).expr(crate::syntax::arg_expr(arg)).unwrap_or(Expr::Lit(Lit::Null)),
                None => Expr::Lit(Lit::Null),
              };
              Meaning::Context { id: format!("{file}#{}", name.name), default, react }
            }
            Some(Meaning::Library(meaning::Construct::AsyncComponent)) => match lazy_import_source(call) {
              Some(source) => self.imported_meaning(file, &source, "default"),
              None => own(),
            },
            _ => own(),
          },
          Some(init @ js::Expression::Identifier(_)) => meanings.of_expr(init).unwrap_or_else(own),
          Some(init) if init.is_member_expression() => meanings.of_expr(init).unwrap_or_else(own),
          _ => own(),
        };
        meanings.bindings.insert(name.name.to_string(), meant);
      }
    }
    for item in &parsed.module().body {
      match item {
        js::Statement::ExportDeclaration(export) => {
          let names: Vec<String> = match &export.declaration {
            js::Declaration::FunctionDeclaration(f) => f.id.iter().map(|id| id.name.to_string()).collect(),
            js::Declaration::ClassDeclaration(c) => c.id.iter().map(|id| id.name.to_string()).collect(),
            js::Declaration::VariableDeclaration(var) => var.declarations.iter().filter_map(|d| if let js::BindingPattern::BindingIdentifier(id) = &d.id { Some(id.name.to_string()) } else { None }).collect(),
            _ => Vec::new(),
          };
          for name in names {
            let meant = meanings.bindings.get(&name).cloned().unwrap_or_else(|| Meaning::Own { file: file.to_owned(), name: name.clone() });
            meanings.exports.insert(name, meant);
          }
        }
        js::Statement::ExportNamedDeclaration(named) if !named.export_kind.is_type() => {
          for n in named.specifiers.iter().filter(|n| !n.export_kind.is_type()) {
            let orig = crate::schema::export_name(&n.local);
            let meant = meanings.bindings.get(&orig).cloned().unwrap_or_else(|| Meaning::Own { file: file.to_owned(), name: orig.clone() });
            meanings.exports.insert(crate::schema::export_name(&n.exported), meant);
          }
        }
        js::Statement::ExportFromDeclaration(named) if !named.export_kind.is_type() => {
          let source = crate::lossy(named.source.value);
          for n in named.specifiers.iter().filter(|n| !n.export_kind.is_type()) {
            let meant = self.imported_meaning(file, &source, &crate::schema::export_name(&n.local));
            meanings.exports.insert(crate::schema::export_name(&n.exported), meant);
          }
        }
        js::Statement::ExportAllDeclaration(all) if !all.export_kind.is_type() => {
          let source = crate::lossy(all.source.value);
          match &all.exported {
            Some(name) => {
              let meant = self.namespace_meaning(file, &source);
              meanings.exports.insert(crate::schema::export_name(name), meant);
            }
            None => {
              if let Meaning::Namespace(members) = self.namespace_meaning(file, &source) {
                for (name, meant) in members.iter() {
                  meanings.exports.entry(name.clone()).or_insert_with(|| meant.clone());
                }
              }
            }
          }
        }
        js::Statement::ExportDefaultDeclaration(e) => {
          let meant = match e.declaration.as_expression().map(|expr| meanings.of_expr(expr)) {
            Some(Some(Meaning::Own { file: declared, .. })) if declared == file => Meaning::Own { file: file.to_owned(), name: "default".to_owned() },
            Some(Some(meant)) => meant,
            _ => Meaning::Own { file: file.to_owned(), name: "default".to_owned() },
          };
          meanings.exports.insert("default".to_owned(), meant);
        }
        _ => {}
      }
    }
    self.meaning_in_progress.retain(|f| f != file);
    let meanings = Rc::new(meanings);
    let _ = parsed.meanings.set(meanings.clone());
    self.meanings.insert(file.to_owned(), meanings.clone());
    meanings
  }

  /// What `export` of `source`, imported from `file`, means.
  fn imported_meaning(&mut self, file: &str, source: &str, export: &str) -> crate::meaning::Meaning {
    use crate::meaning::{self, Construct, Meaning};
    if export == "actions" && is_generated_client(file, source) {
      return Meaning::Library(Construct::GeneratedActions);
    }
    if self.reaches(Reach::Terminal) && file != crate::terminal::INK_FILE && source == crate::terminal::INK_MODULE {
      if let Some(written) = self.meanings(crate::terminal::INK_FILE).exports.get(export).cloned() {
        return written;
      }
    }
    if source == meaning::CONFIG_MODULE {
      return Meaning::Config((export != "config").then(|| export.to_owned()));
    }
    if let Some((side, module)) = meaning::side_module_of(source) {
      return meaning::library(source, export).map(Meaning::Library).unwrap_or_else(|| Meaning::StdAbsent { side, module, name: export.to_owned() });
    }
    if meaning::is_library(source) {
      return meaning::library(source, export).map(Meaning::Library).unwrap_or(Meaning::Foreign);
    }
    match self.resolve_import(file, source) {
      Some(target) if is_foreign(&target) => Meaning::Own { file: target, name: export.to_owned() },
      Some(target) => self.meanings(&target).exports.get(export).cloned().unwrap_or(Meaning::Own { file: target, name: export.to_owned() }),
      None => Meaning::Foreign,
    }
  }

  /// What a namespace import of `source` from `file` holds.
  fn namespace_meaning(&mut self, file: &str, source: &str) -> crate::meaning::Meaning {
    use crate::meaning::{self, Meaning};
    if let Some((side, module)) = meaning::side_module_of(source) {
      return Meaning::Library(meaning::Construct::StdModule(side, module));
    }
    if meaning::is_library(source) {
      return Meaning::Namespace(meaning::library_namespace(source));
    }
    match self.resolve_import(file, source) {
      Some(target) if !is_foreign(&target) => Meaning::Namespace(Rc::new(self.meanings(&target).exports.clone())),
      _ => Meaning::Foreign,
    }
  }

  /// A module-level name as an expression: a `const` inlined, a function as a
  /// lambda, an import resolved in its own file. `None` when the file has no
  /// such name.
  fn global(&mut self, file: &str, name: &str) -> Result<Option<(Expr, String)>, LowerError> {
    let key = format!("{file}#{name}");
    if self.resolving.iter().any(|m| *m == key) {
      // The function's own name inside its body: the binding `Expr::Fix` gives it while it runs.
      if !self.recursive.iter().any(|k| *k == key) {
        self.recursive.push(key.clone());
      }
      return Ok(Some((Expr::Var(name.to_owned()), key)));
    }
    let meanings = self.meanings(file);
    let arena = Allocator::default();
    let parsed = self.parsed(&arena, file);
    if let Some(id) = crate::classes::declared(&parsed.module()).iter().any(|(n, _)| n == name).then(|| format!("{file}#{name}")) {
      self.lower_class(&id)?;
      return Ok(Some((crate::classes::class_value(&id), key)));
    }
    match meanings.bindings.get(name) {
      Some(crate::meaning::Meaning::Asset { source }) => {
        let Some(path) = asset_path(file, source) else { return Ok(None) };
        return Ok(match assets::kind(source) {
          Some(assets::Kind::Image) => self.assets.image(&path, &ImageRequest::default()).map(|facts| (asset_object(&facts), key)),
          Some(assets::Kind::Font) => self.assets.font(&path).map(|url| (Expr::lit_str(url), key)),
          Some(assets::Kind::Media) => self.assets.media(&path).map(|url| (Expr::lit_str(url), key)),
          None => None,
        });
      }
      Some(crate::meaning::Meaning::Own { file: declared, name: declared_name }) if declared != file => {
        let (declared, declared_name) = (declared.clone(), declared_name.clone());
        self.load(&declared)?;
        return self.global(&declared, &declared_name);
      }
      Some(crate::meaning::Meaning::Context { id, .. }) if !id.starts_with(&format!("{file}#")) => {
        let Some((declared, declared_name)) = id.rsplit_once('#').map(|(f, n)| (f.to_owned(), n.to_owned())) else { return Ok(None) };
        self.load(&declared)?;
        return self.global(&declared, &declared_name);
      }
      Some(crate::meaning::Meaning::Config(Some(field))) => return Ok(Some((Expr::Config(field.clone()), key))),
      Some(crate::meaning::Meaning::Config(None)) => return Err(LowerError::Residue(parsed.residue(parsed.module().span, format!("`{name}` from `{}` as a whole; read one of its fields", crate::meaning::CONFIG_MODULE)))),
      Some(crate::meaning::Meaning::Library(_) | crate::meaning::Meaning::Foreign | crate::meaning::Meaning::Namespace(_)) => return Ok(None),
      _ => {}
    }
    if let Some(span) = written_module_let(&parsed, name) {
      if !self.lowering_component {
        return Err(LowerError::Residue(parsed.residue(span, format!("`{name}`, a module-level `let` a function writes; a body reads module state through the store"))));
      }
      if !self.module_lets.iter().any(|(k, _, _)| *k == key) {
        self.module_lets.push((key.clone(), file.to_owned(), name.to_owned()));
      }
      // A terminal page holds the module `let` in a cell that lasts from one step to the next; a read is the cell's value.
      let read = match self.reaches(Reach::Later) {
        true => Expr::ReadCell(Box::new(Expr::Var(name.to_owned()))),
        false => Expr::Var(name.to_owned()),
      };
      return Ok(Some((read, key)));
    }
    let Some(item) = find_value(&parsed, name) else { return Ok(None) };
    if matches!(item, Global::Function(..)) && name.starts_with(|c: char| c.is_ascii_uppercase()) {
      if let Ok((module, _)) = self.component_module(file, name) {
        match self.lower(&module) {
          Ok(()) | Err(LowerError::Residue(_)) => {}
          Err(other) => return Err(other),
        }
        return Ok(Some((Expr::Builtin { name: Builtin::ComponentRef, args: vec![Expr::lit_str(module)] }, key)));
      }
    }
    if let Global::Const(init) = &item {
      // The formatter itself has no value; `global_formatter` hands the lowerer the `new` that made it.
      if matches!(crate::strip_parens(init), js::Expression::NewExpression(n) if crate::platform_object(n).is_some()) {
        return Ok(Some((Expr::Lit(Lit::Null), key)));
      }
      if let Some((module, member, reach)) = native_declaration(&parsed, init)? {
        let native = format!("{module}.{member}");
        if !self.natives.iter().any(|(n, _)| *n == native) {
          self.natives.push((native, reach));
        }
        return Ok(Some((Expr::Ext { module, name: member, args: Vec::new() }, key)));
      }
    }
    let replays = matches!(item, Global::Const(_)) && module_lets_of(&parsed).into_iter().any(|(_, _, written)| written);
    let constant = matches!(item, Global::Const(_));
    self.resolving.push(key.clone());
    let result = self.lower_global(file, item);
    self.resolving.pop();
    let result = match result {
      Ok(expr) if self.recursive.iter().any(|k| *k == key) => {
        self.recursive.retain(|k| *k != key);
        Ok(Expr::Fix { name: name.to_owned(), f: Box::new(expr) })
      }
      other => other,
    };
    match result {
      Err(LowerError::Residue(residue)) if file.starts_with(&format!("{EXT_DIR}/")) => Err(LowerError::Extension(residue)),
      // A constant that reads the machine, a file or the clock is what the module's initialisation left it at: a cell the session makes once, as Node runs a module once.
      Ok(expr) if constant && self.lowering_component && self.reaches(Reach::Later) && reads_host(&expr, &self.natives) => {
        if !self.module_lets.iter().any(|(k, _, _)| *k == key) {
          self.module_lets.push((key.clone(), file.to_owned(), name.to_owned()));
        }
        Ok(Some((Expr::ReadCell(Box::new(Expr::Var(name.to_owned()))), key)))
      }
      // A constant of a module whose `let`s its functions write is what the module's initialisation leaves it at, so it is read by replaying that initialisation.
      Ok(_) if replays => self.module_init(file, name).map(|expr| Some((expr, key))),
      other => other.map(|expr| Some((expr, key))),
    }
  }

  /// The module's initialisation up to `upto` as a block: each top-level `let` and `const` bound in order, with every call a `const` makes applied, then `upto`'s value returned. A `let` a function writes is advanced by those calls, so what comes out is the value the browser's copy of the module starts with.
  fn module_init(&mut self, file: &str, upto: &str) -> Result<Expr, LowerError> {
    let arena = Allocator::default();
    let parsed = self.parsed(&arena, file);
    let mut body = Vec::new();
    for (name, init, written) in module_lets_of(&parsed) {
      let expr = match init {
        Some(init) => self.lower_global(file, Global::Const(init))?,
        None => Expr::Lit(Lit::Undefined),
      };
      body.push(Stmt::Let { name: name.clone(), expr });
      // A written `let` is what the whole initialisation leaves it at; a constant is fixed once bound.
      if name == upto && !written {
        break;
      }
    }
    body.push(Stmt::Return(Expr::Var(upto.to_owned())));
    Ok(Expr::Apply { f: Box::new(Expr::Lambda { params: Vec::new(), body: Box::new(Expr::Block(body)) }), args: Vec::new() })
  }

  /// The collection a module-level `name` holds when it is declared as a `new Map(...)`, `new Set(...)` or `new URLSearchParams(...)`.
  fn global_kind(&mut self, file: &str, name: &str) -> Result<Option<crate::Kind>, LowerError> {
    if let Some(crate::meaning::Meaning::Own { file: declared, name: declared_name }) = self.meanings(file).bindings.get(name).cloned() {
      if declared != file {
        self.load(&declared)?;
        return self.global_kind(&declared, &declared_name);
      }
    }
    let arena = Allocator::default();
    let parsed = self.parsed(&arena, file);
    Ok(match find_value(&parsed, name) {
      Some(Global::Const(init)) => crate::made_kind(init),
      _ => None,
    })
  }

  /// The kind a module-level function `name` is declared to return, a type alias read through to what it stands for.
  fn global_return_kind(&mut self, file: &str, name: &str) -> Result<Option<crate::Kind>, LowerError> {
    if let Some(crate::meaning::Meaning::Own { file: declared, name: declared_name }) = self.meanings(file).bindings.get(name).cloned() {
      if declared != file {
        self.load(&declared)?;
        return self.global_return_kind(&declared, &declared_name);
      }
    }
    let arena = Allocator::default();
    let parsed = self.parsed(&arena, file);
    let Some(returned) = declared_return(&parsed.module(), name) else { return Ok(None) };
    let aliases = self.alias_kinds(file);
    Ok(crate::Kind::of_type_in(returned, &aliases))
  }

  /// Every type alias `file` declares or imports that stands for a kind, by its local name.
  pub(crate) fn alias_kinds(&mut self, file: &str) -> Vec<(String, crate::AliasKind)> {
    let arena = Allocator::default();
    let Some(parsed) = self.parsed_if(&arena, file) else { return Vec::new() };
    let mut names: Vec<String> = parsed.module().body.iter().filter_map(|item| type_alias_decl(item).map(|a| a.id.name.to_string())).collect();
    for item in &parsed.module().body {
      let js::Statement::ImportDeclaration(import) = item else { continue };
      for spec in import.specifiers.iter().flatten() {
        if let js::ImportDeclarationSpecifier::ImportSpecifier(named) = spec {
          names.push(named.local.name.to_string());
        }
      }
    }
    names.into_iter().filter_map(|name| self.resolve_alias(file, &name).map(|kind| (name, kind))).collect()
  }

  /// What the type `name` stands for in `file` as a kind: an alias declared there, or one imported or handed on from a file the set can read.
  fn resolve_alias(&mut self, file: &str, name: &str) -> Option<crate::AliasKind> {
    let key = format!("{file}#{name}");
    if let Some(known) = self.aliases.get(&key) {
      return *known;
    }
    self.aliases.insert(key.clone(), None);
    let found = self.resolve_alias_uncached(file, name);
    self.aliases.insert(key, found);
    found
  }

  fn resolve_alias_uncached(&mut self, file: &str, name: &str) -> Option<crate::AliasKind> {
    let arena = Allocator::default();
    let parsed = self.parsed_if(&arena, file)?;
    if let Some(alias) = parsed.module().body.iter().filter_map(type_alias_decl).find(|a| a.id.name.as_str() == name) {
      let params: Vec<String> = alias.type_parameters.as_ref().map(|p| p.params.iter().map(|p| p.name.name.to_string()).collect()).unwrap_or_default();
      return self.alias_body_kind(file, &alias.type_annotation, &params);
    }
    let mut sources = Vec::new();
    for item in &parsed.module().body {
      match item {
        js::Statement::ImportDeclaration(import) => {
          for spec in import.specifiers.iter().flatten() {
            let js::ImportDeclarationSpecifier::ImportSpecifier(named) = spec else { continue };
            if named.local.name.as_str() != name {
              continue;
            }
            let imported = crate::schema::export_name(&named.imported);
            sources.push((crate::lossy(import.source.value), imported));
          }
        }
        js::Statement::ExportFromDeclaration(e) => {
          for n in &e.specifiers {
            if crate::schema::export_name(&n.exported) == name {
              sources.push((crate::lossy(e.source.value), crate::schema::export_name(&n.local)));
            }
          }
        }
        js::Statement::ExportAllDeclaration(e) if e.exported.is_none() => sources.push((crate::lossy(e.source.value), name.to_owned())),
        _ => {}
      }
    }
    for (source, imported) in sources {
      let Some(target) = self.resolve_import(file, &source).filter(|t| !is_foreign(t)) else { continue };
      if self.load(&target).is_err() {
        continue;
      }
      if let Some(found) = self.resolve_alias(&target, &imported) {
        return Some(found);
      }
    }
    None
  }

  /// The kind an alias body `ty` stands for, `params` the alias's own type parameters.
  fn alias_body_kind(&mut self, file: &str, ty: &js::TSType, params: &[String]) -> Option<crate::AliasKind> {
    match ty {
      js::TSType::TSParenthesizedType(p) => self.alias_body_kind(file, &p.type_annotation, params),
      js::TSType::TSIntersectionType(i) => i.types.iter().find_map(|t| self.alias_body_kind(file, t, params)),
      js::TSType::TSTypeReference(r) => {
        let js::TSTypeName::IdentifierReference(id) = &r.type_name else { return None };
        if let Some(i) = params.iter().position(|p| p == id.name.as_str()) {
          return Some(crate::AliasKind::Param(i));
        }
        if let Some(kind) = crate::Kind::of_type(ty) {
          return Some(crate::AliasKind::Fixed(kind));
        }
        match self.resolve_alias(file, id.name.as_str())? {
          crate::AliasKind::Fixed(kind) => Some(crate::AliasKind::Fixed(kind)),
          crate::AliasKind::Param(j) => self.alias_body_kind(file, r.type_arguments.as_ref()?.params.get(j)?, params),
        }
      }
      other => crate::Kind::of_type(other).map(crate::AliasKind::Fixed),
    }
  }

  /// The `Intl` formatter a module-level `name` holds when it is declared as `new Intl.NumberFormat(...)` or `new Intl.DateTimeFormat(...)`.
  fn global_formatter(&mut self, file: &str, name: &str) -> Result<Option<String>, LowerError> {
    if let Some(crate::meaning::Meaning::Own { file: declared, name: declared_name }) = self.meanings(file).bindings.get(name).cloned() {
      if declared != file {
        self.load(&declared)?;
        return self.global_formatter(&declared, &declared_name);
      }
    }
    let arena = Allocator::default();
    let parsed = self.parsed(&arena, file);
    Ok(match find_value(&parsed, name) {
      Some(Global::Const(init)) => match crate::strip_parens(init) {
        js::Expression::NewExpression(n) if crate::platform_object(n).is_some() => Some(parsed.text()[parsed.range(n.span)].to_owned()),
        _ => None,
      },
      _ => None,
    })
  }

  /// Lowers every member of the class `id`, and of each class it extends, into `consts`. A member that does not lower is a constant that fails with its residue when it runs.
  fn lower_class(&mut self, id: &str) -> Result<(), LowerError> {
    if !self.classes_lowered.insert(id.to_owned()) {
      return Ok(());
    }
    let Some((file, name)) = id.rsplit_once('#') else { return Ok(()) };
    let parent = self.classes.borrow().get(id).and_then(|c| c.parent.clone());
    if let Some(parent) = &parent {
      self.lower_class(parent)?;
    }
    if let Some(parent) = &parent {
      self.consts.insert(crate::classes::class_parent_key(id), Expr::lit_str(parent));
    }
    let arena = Allocator::default();
    let parsed = self.parsed(&arena, file);
    let Some((_, class)) = crate::classes::declared(&parsed.module()).into_iter().find(|(n, _)| n == name) else { return Ok(()) };
    let mut members: Vec<(String, Member<'_>)> = vec![(crate::classes::constructor_key(id), Member::Constructor)];
    for member in &class.body.body {
      match member {
        js::ClassElement::MethodDefinition(m) if !matches!(m.kind, js::MethodDefinitionKind::Set | js::MethodDefinitionKind::Constructor) => {
          let Some(member_name) = crate::classes::member_name(&m.key, m.computed) else { continue };
          match (m.r#static, m.kind) {
            (true, _) => members.push((crate::classes::static_key(id, &member_name), Member::Static(&m.value))),
            (false, js::MethodDefinitionKind::Get) => members.push((crate::classes::class_getter_key(id, &member_name), Member::Instance(&m.value))),
            (false, _) => members.push((crate::classes::class_method_key(id, &member_name), Member::Instance(&m.value))),
          }
        }
        js::ClassElement::PropertyDefinition(p) if p.r#static => {
          let (Some(member_name), Some(value)) = (crate::classes::member_name(&p.key, p.computed), p.value.as_ref()) else { continue };
          members.push((crate::classes::static_key(id, &member_name), Member::StaticField(value)));
        }
        _ => {}
      }
    }
    for (key, member) in members {
      let item = Global::Member { id: id.to_owned(), parent: parent.clone(), class, name: name.to_owned(), member };
      let lowered = match self.lower_global(file, item) {
        Ok(expr) => expr,
        Err(LowerError::Residue(residue)) => Expr::Builtin { name: Builtin::Throw, args: vec![Expr::lit_str(residue.to_string())] },
        Err(other) => return Err(other),
      };
      self.consts.insert(key, lowered);
    }
    Ok(())
  }

  /// A constant the plan names once or the expression itself when naming it would cost more than copying it. `CONST_WEIGHT` is where a copy per reference starts to dominate the plan. A program that writes objects names every constant that builds one, so all its readers in a request hold one object.
  fn name_if_large(&mut self, key: String, expr: Expr) -> Expr {
    let shared = data_only(&expr) && asset_facts(&expr).is_none() && !is_asset_record(&expr);
    if weight(&expr) < CONST_WEIGHT && !shared || matches!(expr, Expr::Lambda { .. } | Expr::Fix { .. } | Expr::Ext { .. }) || is_asset_record(&expr) {
      return expr;
    }
    let key = if crate::heap_pass() && !shared { format!("{key}$heap") } else { key };
    self.consts.entry(key.clone()).or_insert(expr);
    Expr::Const(key)
  }



  fn lower_global(&mut self, file: &str, item: Global<'_>) -> Result<Expr, LowerError> {
    self.load_class_graph(file);
    let mut globals: Vec<(String, Expr)> = Vec::new();
    let mut global_kinds = Vec::new();
    let mut global_formatters = Vec::new();
    loop {
      let arena = Allocator::default();
      let parsed = self.parsed(&arena, file);
      let defaults = self.defaults.clone();
      let meanings = self.meanings(file);
      let mut lowerer = Lowerer::new(&parsed, &defaults);
      lowerer.reaches = self.reaches.clone();
      lowerer.meanings = Some(meanings);
      lowerer.globals = globals.clone();
      lowerer.global_kinds = global_kinds.clone();
      lowerer.formatters_from(&global_formatters);
      lowerer.natives = self.natives.clone();
      if self.reaches(Reach::Later) {
        lowerer.cells = module_lets_of(&parsed).into_iter().filter(|(_, _, written)| *written).map(|(name, _, _)| (name.clone(), Expr::Var(name))).collect();
      }
      lowerer.classes = Some(self.classes.clone());
      lowerer.type_kinds = self.alias_kinds(file);
      let result = match &item {
        Global::Const(init) => lowerer.expr(init),
        Global::Function(params, body, is_async) => function_to_lambda(&mut lowerer, params, *body, *is_async),
        Global::Member { id, parent, class, name, member } => {
          lowerer.class_parent = parent.clone();
          crate::classes::lower_member(&mut lowerer, id, parent.as_deref(), class, name, member)
        }
      };
      match result {
        Ok(expr) => {
          self.continuations.append(&mut lowerer.continuations);
          return Ok(expr);
        }
        Err(residue) => {
          let Some(name) = lowerer.unbound.take() else { return Err(residue.into()) };
          if globals.iter().any(|(n, _)| *n == name) {
            return Err(residue.into());
          }
          let Some((expr, key)) = self.global(file, &name)? else { return Err(residue.into()) };
          if let Some(kind) = self.global_kind(file, &name)? {
            global_kinds.push((name.clone(), kind));
          }
          if let Some(kind) = self.global_return_kind(file, &name)? {
            global_kinds.push((crate::returned_by(&name), kind));
          }
          if let Some(made) = self.global_formatter(file, &name)? {
            global_formatters.push((name.clone(), made));
          }
          let expr = self.name_if_large(key, expr);
          globals.push((name, expr));
        }
      }
    }
  }
}

/// Points every component reference at the module the set lowered it under.
/// An island alias's `when` and `mode`.
type IslandTiming = (Option<String>, Option<String>);

/// Replaces each placed `file#Name` with the module it resolved to; a
/// placement whose name is an island alias becomes the island the alias
/// declared.
/// The ids of the placements that became islands, whose sites the rewrite keeps.
fn island_ids(tmpl: &Tmpl, out: &mut Vec<u32>) {
  match tmpl {
    // `Baked` is what a loaded component becomes, so the lowerer never meets one.
    Tmpl::Baked { .. } => {}
    Tmpl::Island { id, children, .. } => {
      out.push(*id);
      children.iter().for_each(|c| island_ids(c, out));
    }
    Tmpl::Component { children, .. } | Tmpl::Element { children, .. } | Tmpl::Fragment(children) | Tmpl::Place { children, .. } => children.iter().for_each(|c| island_ids(c, out)),
    Tmpl::If { then, r#else, .. } => {
      island_ids(then, out);
      if let Some(other) = r#else {
        island_ids(other, out);
      }
    }
    Tmpl::For { body, .. } => island_ids(body, out),
    Tmpl::Let { then, .. } => island_ids(then, out),
    Tmpl::Text(_) | Tmpl::Marker(_) | Tmpl::Outlet { .. } | Tmpl::Html(_) | Tmpl::Expr(_) | Tmpl::Slot(_) => {}
  }
}

/// Marks each placement in `placed` whose component `keys` as keyed and
/// collects its id. A placement with no site stays unkeyed, since the
/// rewrite could not wrap it to match. So does one inside a kept chunk: the
/// browser takes the chunk's markup whole and a chunk holds no island. A
/// placement whose component is `stateful` is keyed on the server's side
/// alone, which gives each instance inside a server island the address its
/// state and handlers ride under; nothing hoists below it, so the browser's
/// path needs no step there.
fn mark_keyed(tmpl: &mut Tmpl, placed: &[u32], keys: &dyn Fn(&str) -> bool, stateful: &dyn Fn(&str) -> bool, out: &mut Vec<u32>) {
  match tmpl {
    Tmpl::Baked { .. } => {}
    Tmpl::Element { attrs, .. } if attrs.iter().any(|a| matches!(a, Entry::Field(name, _) if name == hoist::CHUNK_ATTR)) => {}
    Tmpl::Component { module, children, id, keyed, .. } => {
      if placed.contains(id) && keys(module) {
        *keyed = true;
        out.push(*id);
      } else if stateful(module) {
        *keyed = true;
      }
      children.iter_mut().for_each(|c| mark_keyed(c, placed, keys, stateful, out));
    }
    Tmpl::Island { children, .. } | Tmpl::Element { children, .. } | Tmpl::Fragment(children) | Tmpl::Place { children, .. } => children.iter_mut().for_each(|c| mark_keyed(c, placed, keys, stateful, out)),
    Tmpl::If { then, r#else, .. } => {
      mark_keyed(then, placed, keys, stateful, out);
      if let Some(other) = r#else {
        mark_keyed(other, placed, keys, stateful, out);
      }
    }
    Tmpl::For { body, .. } => mark_keyed(body, placed, keys, stateful, out),
    Tmpl::Let { then, .. } => mark_keyed(then, placed, keys, stateful, out),
    Tmpl::Text(_) | Tmpl::Marker(_) | Tmpl::Outlet { .. } | Tmpl::Html(_) | Tmpl::Expr(_) | Tmpl::Slot(_) => {}
  }
}

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

/// Every module `tmpl` places, inline or as an island.
fn placed_modules(tmpl: &Tmpl, out: &mut Vec<String>) {
  match tmpl {
    Tmpl::Component { module, children, .. } | Tmpl::Island { module, children, .. } => {
      out.push(module.clone());
      children.iter().for_each(|c| placed_modules(c, out));
    }
    Tmpl::Element { children, .. } | Tmpl::Fragment(children) | Tmpl::Place { children, .. } | Tmpl::Baked { children, .. } => children.iter().for_each(|c| placed_modules(c, out)),
    Tmpl::If { then, r#else, .. } => {
      placed_modules(then, out);
      if let Some(e) = r#else {
        placed_modules(e, out);
      }
    }
    Tmpl::For { body, .. } => placed_modules(body, out),
    Tmpl::Let { then, .. } => placed_modules(then, out),
    Tmpl::Text(_) | Tmpl::Marker(_) | Tmpl::Outlet { .. } | Tmpl::Html(_) | Tmpl::Expr(_) | Tmpl::Slot(_) => {}
  }
}

/// True when `tmpl` holds the region a `<Mount>` writes.
fn holds_mount(tmpl: &Tmpl) -> bool {
  match tmpl {
    Tmpl::Element { attrs, children, .. } => attrs.iter().any(|a| matches!(a, Entry::Field(name, _) if name == placements::MOUNT_ATTR)) || children.iter().any(holds_mount),
    Tmpl::Fragment(children) | Tmpl::Place { children, .. } | Tmpl::Component { children, .. } | Tmpl::Island { children, .. } => children.iter().any(holds_mount),
    Tmpl::If { then, r#else, .. } => holds_mount(then) || r#else.as_ref().is_some_and(|e| holds_mount(e)),
    Tmpl::For { body, .. } => holds_mount(body),
    Tmpl::Let { then, .. } => holds_mount(then),
    Tmpl::Text(_) | Tmpl::Marker(_) | Tmpl::Outlet { .. } | Tmpl::Html(_) | Tmpl::Expr(_) | Tmpl::Slot(_) | Tmpl::Baked { .. } => false,
  }
}

/// `tmpl` with each component of another framework it renders inline as the
/// empty region `Mount` writes, its module added to `mounted`; `Err` with the
/// module of one that is given children, which cannot cross into it.
fn mount_foreign(tmpl: Tmpl, mounted: &mut Vec<String>) -> Result<Tmpl, String> {
  let walk = |children: Vec<Tmpl>, mounted: &mut Vec<String>| children.into_iter().map(|c| mount_foreign(c, mounted)).collect::<Result<Vec<_>, _>>();
  Ok(match tmpl {
    Tmpl::Component { module, children, .. } if is_foreign(&module) => {
      if !children.is_empty() {
        return Err(module);
      }
      if !mounted.contains(&module) {
        mounted.push(module);
      }
      Tmpl::Element { tag: "sf-s".to_owned(), attrs: vec![Entry::Field("data-sf-island".to_owned(), Expr::lit_str(""))], children: Vec::new() }
    }
    Tmpl::Component { module, props, children, id, keyed } => Tmpl::Component { module, props, children: walk(children, mounted)?, id, keyed },
    Tmpl::Element { tag, attrs, children } => Tmpl::Element { tag, attrs, children: walk(children, mounted)? },
    Tmpl::Fragment(children) => Tmpl::Fragment(walk(children, mounted)?),
    Tmpl::If { cond, then, r#else } => Tmpl::If { cond, then: Box::new(mount_foreign(*then, mounted)?), r#else: r#else.map(|e| mount_foreign(*e, mounted).map(Box::new)).transpose()? },
    Tmpl::For { over, params, body } => Tmpl::For { over, params, body: Box::new(mount_foreign(*body, mounted)?) },
    Tmpl::Let { name, expr, then } => Tmpl::Let { name, expr, then: Box::new(mount_foreign(*then, mounted)?) },
    other => other,
  })
}

/// The first foreign component rendered inline rather than as an island.
fn inline_foreign(tmpl: &Tmpl) -> Option<String> {
  match tmpl {
    Tmpl::Component { module, children, .. } => {
      if is_foreign(module) {
        return Some(module.clone());
      }
      children.iter().find_map(inline_foreign)
    }
    Tmpl::Island { children, .. } | Tmpl::Element { children, .. } | Tmpl::Fragment(children) | Tmpl::Place { children, .. } => children.iter().find_map(inline_foreign),
    Tmpl::Baked { children, .. } => children.iter().find_map(inline_foreign),
    Tmpl::If { then, r#else, .. } => inline_foreign(then).or_else(|| r#else.as_ref().and_then(|e| inline_foreign(e))),
    Tmpl::For { body, .. } => inline_foreign(body),
    Tmpl::Let { then, .. } => inline_foreign(then),
    Tmpl::Text(_) | Tmpl::Marker(_) | Tmpl::Outlet { .. } | Tmpl::Html(_) | Tmpl::Expr(_) | Tmpl::Slot(_) => None,
  }
}

/// The tag and position that placed `module`, for a message about it.
fn refs_by_module(modules: &HashMap<String, String>, module: &str, refs: &[(String, (usize, usize))]) -> Option<(String, (usize, usize))> {
  let (placed, _) = modules.iter().find(|(_, m)| m.as_str() == module)?;
  let name = placed.split_once('#').map(|(_, n)| n).unwrap_or(placed);
  refs.iter().find(|(n, _)| n == name).cloned()
}

fn rewrite_modules(tmpl: Tmpl, modules: &HashMap<String, String>, islands: &HashMap<String, IslandTiming>) -> Tmpl {
  let mut tmpl = rewrite_tree(tmpl, modules, islands);
  tmpl.visit_mut(&mut |e| {
    if let Expr::Markup(markup) = e {
      *markup = std::sync::Arc::new(rewrite_tree((**markup).clone(), modules, islands));
    }
  });
  tmpl
}

fn rewrite_tree(tmpl: Tmpl, modules: &HashMap<String, String>, islands: &HashMap<String, IslandTiming>) -> Tmpl {
  let walk = |children: Vec<Tmpl>| children.into_iter().map(|c| rewrite_tree(c, modules, islands)).collect();
  match tmpl {
    Tmpl::Component { module, props, children, id, keyed } => match islands.get(&module) {
      Some((when, mode)) => Tmpl::Island { module: modules.get(&module).cloned().unwrap_or(module), props, children: walk(children), when: when.clone(), mode: mode.clone(), id, define: false },
      None => Tmpl::Component { module: modules.get(&module).cloned().unwrap_or(module), props, children: walk(children), id, keyed },
    },
    Tmpl::Island { module, props, children, when, mode, id, define } => Tmpl::Island { module: modules.get(&module).cloned().unwrap_or(module), props, children: walk(children), when, mode, id, define },
    Tmpl::Element { tag, attrs, children } => Tmpl::Element { tag, attrs, children: walk(children) },
    Tmpl::Fragment(children) => Tmpl::Fragment(walk(children)),
    Tmpl::If { cond, then, r#else } => Tmpl::If { cond, then: Box::new(rewrite_tree(*then, modules, islands)), r#else: r#else.map(|e| Box::new(rewrite_tree(*e, modules, islands))) },
    Tmpl::For { over, params, body } => Tmpl::For { over, params, body: Box::new(rewrite_tree(*body, modules, islands)) },
    Tmpl::Let { name, expr, then } => Tmpl::Let { name, expr, then: Box::new(rewrite_tree(*then, modules, islands)) },
    other => other,
  }
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

impl<'p> From<(Vec<crate::Param<'p>>, FunctionBody<'p>)> for HandlerFn<'p> {
  fn from((params, body): (Vec<crate::Param<'p>>, FunctionBody<'p>)) -> Self {
    Self { params, body, hook: None }
  }
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

/// The alias `name` is in `parsed`, when it is one: `island` must come from
/// the client's React package and the options are string literals.
fn island_alias_of(parsed: &Parsed, meanings: &crate::meaning::Meanings, name: &str) -> Result<Option<IslandAlias>, (Span, String)> {
  let Some(Global::Const(js::Expression::CallExpression(call))) = find_value(parsed, name) else { return Ok(None) };
  let callee = &call.callee;
  if meanings.of_expr(callee).and_then(|meant| meant.placement()) != Some("island") {
    return Ok(None);
  }
  let Some(js::Expression::Identifier(target)) = call.arguments.first().map(|a| crate::syntax::arg_expr(a)) else {
    return Err((call.span, "`island(...)` takes a component name first".to_owned()));
  };
  let mut when = None;
  let mut mode = None;
  if let Some(options) = call.arguments.get(1) {
    let js::Expression::ObjectExpression(obj) = crate::syntax::arg_expr(options) else { return Err((options.span(), "`island(...)` options must be an object literal".to_owned())) };
    for prop in &obj.properties {
      let js::ObjectPropertyKind::ObjectProperty(prop) = prop else { return Err((options.span(), "a spread in `island(...)` options".to_owned())) };
      let crate::syntax::Prop::KeyValue(kv) = crate::syntax::prop_of(prop) else { return Err((options.span(), "`island(...)` options are `when` and `mode`".to_owned())) };
      let value = match &kv.value {
        js::Expression::StringLiteral(s) => Some(crate::lossy(s.value)),
        _ => None,
      };
      match prop_name(&kv.key, kv.computed).as_deref() {
        Some("when") => match value.as_deref() {
          Some("load" | "visible" | "idle") => when = value,
          _ => return Err((options.span(), "an island's `when` is \"load\", \"visible\" or \"idle\", written out".to_owned())),
        },
        Some("mode") => match value.as_deref() {
          Some("browser") => mode = None,
          Some("server") => mode = value,
          _ => return Err((options.span(), "an island's `mode` is \"browser\" or \"server\", written out".to_owned())),
        },
        _ => return Err((options.span(), "`island(...)` options are `when` and `mode`".to_owned())),
      }
    }
  }
  Ok(Some(IslandAlias { target: target.name.to_string(), when, mode }))
}

/// The action id of `actions.<path>(input)`, when `actions` is the object the
/// build generates: the property path is the id, `$root.blip` for
/// `actions.$root.blip`. The generated module re-exports the client library's
/// `action` under another name, so there is nothing here to follow by import.
/// Whether `source`, imported from `file`, is the client module the build generates.
fn is_generated_client(file: &str, source: &str) -> bool {
  crate::resolve_specifier(file, source).is_some_and(|joined| {
    let stem = joined.rsplit_once('.').filter(|(_, ext)| matches!(*ext, "ts" | "js")).map(|(stem, _)| stem).unwrap_or(&joined);
    stem.ends_with("generated/client")
  })
}

pub(crate) fn generated_action_of(parsed: &Parsed, callee: &js::Expression) -> Option<String> {
  let mut path: Vec<String> = Vec::new();
  let mut cursor = callee;
  loop {
    match cursor {
      member_expr if member_expr.is_member_expression() => {
let member = member_expr.as_member_expression().expect("matched as a member");
        let crate::syntax::MemberProp::Ident(name) = crate::syntax::prop(member) else { return None };
        path.push(name.name.to_string());
        cursor = member.object();
      }
      js::Expression::Identifier(root) => {
        if parsed.meanings.get()?.of_ident(root) != Some(&crate::meaning::Meaning::Library(crate::meaning::Construct::GeneratedActions)) {
          return None;
        }
        path.reverse();
        return (!path.is_empty()).then(|| path.join("."));
      }
      _ => return None,
    }
  }
}

/// `const save = action("desk.save")` at module scope, `action` imported from
/// the client library, when `name` is such a `save`: the action's id.
pub(crate) fn action_alias_of(parsed: &Parsed, name: &str) -> Option<String> {
  let Some(Global::Const(js::Expression::CallExpression(call))) = find_value(parsed, name) else { return None };
  let callee = &call.callee;
  if parsed.meanings.get().and_then(|meanings| meanings.of_expr(callee)) != Some(crate::meaning::Meaning::Library(crate::meaning::Construct::ClientAction)) {
    return None;
  }
  match call.arguments.first().map(|a| crate::syntax::arg_expr(a)) {
    Some(js::Expression::StringLiteral(id)) => Some(crate::lossy(id.value)),
    _ => None,
  }
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

/// `Date.now` named rather than called, as `useState(Date.now)` passes it to be called once.
fn is_date_now(member: &js::MemberExpression) -> bool {
  matches!((member.object(), crate::syntax::prop(member)), (js::Expression::Identifier(date), crate::syntax::MemberProp::Ident(now)) if date.name == "Date" && now.name == "now")
}

pub(crate) fn arrow_body<'a>(arrow: &'a js::ArrowFunctionExpression<'a>) -> FunctionBody<'a> {
  match crate::syntax::arrow_body_of(arrow) {
    crate::syntax::ArrowBody::Block(b) => FunctionBody::Block(b.body),
    crate::syntax::ArrowBody::Expr(e) => FunctionBody::Expr(e),
  }
}

/// The exported or declared function named `export`; `default` is the
/// default export.
/// The name `export default name;` exports.
fn default_ident<'a>(parsed: &'a Parsed<'a>) -> Option<&'a js::IdentifierReference<'a>> {
  parsed.module().body.iter().find_map(|item| match crate::syntax::top(item) {
    crate::syntax::Top::DefaultExpr(js::Expression::Identifier(id)) => Some(&**id),
    _ => None,
  })
}

/// `export { orig as export } from "source"`: the source, the name it exports there and the specifier's span.
fn reexported(parsed: &Parsed, export: &str) -> Option<(String, String, Span)> {
  parsed.module().body.iter().find_map(|item| match item {
    js::Statement::ExportFromDeclaration(e) => {
      let source = crate::lossy(e.source.value);
      e.specifiers.iter().find_map(|n| (crate::schema::export_name(&n.exported) == export).then(|| (source.clone(), crate::schema::export_name(&n.local), n.span)))
    }
    _ => None,
  })
}

pub(crate) fn find_function<'a>(parsed: &'a Parsed<'a>, export: &str) -> Option<Found<'a>> {
  for item in &parsed.module().body {
    match crate::syntax::top(item) {
      crate::syntax::Top::DefaultFunction(f) if export == "default" => {
        let body = f.body.as_ref()?;
        return Some(Found::Declared(&f.params, &body.statements, body.span));
      }
      crate::syntax::Top::DefaultExpr(e) if export == "default" => match e {
        js::Expression::ArrowFunctionExpression(arrow) => return Some(Found::Arrow(arrow)),
        js::Expression::Identifier(id) => return find_function(parsed, id.name.as_str()),
        js::Expression::CallExpression(call) => return unwrapped(parsed, call),
        _ => {}
      },
      crate::syntax::Top::Decl(decl) => {
        if let Some(found) = decl_function(parsed, decl, export) {
          return Some(found);
        }
      }
      _ => {}
    }
  }
  None
}

/// The React export `expr` names: `memo` imported from `react` under any name, or `React.memo` through its default or namespace import.
fn react_export(parsed: &Parsed, expr: &js::Expression) -> Option<String> {
  let imports = parsed.module().body.iter().filter_map(|item| match item {
    js::Statement::ImportDeclaration(import) if crate::lossy(import.source.value) == "react" => Some(import),
    _ => None,
  });
  match crate::meaning::bare(expr) {
    js::Expression::Identifier(id) => imports.flat_map(|import| import.specifiers.iter().flatten()).find_map(|spec| match spec {
      js::ImportDeclarationSpecifier::ImportSpecifier(named) if named.local.name == id.name => Some(crate::schema::export_name(&named.imported)),
      _ => None,
    }),
    js::Expression::StaticMemberExpression(member) => {
      let js::Expression::Identifier(object) = crate::meaning::bare(&member.object) else { return None };
      let mut imports = imports;
      imports
        .any(|import| import.specifiers.iter().flatten().any(|spec| matches!(spec, js::ImportDeclarationSpecifier::ImportDefaultSpecifier(d) if d.local.name == object.name) || matches!(spec, js::ImportDeclarationSpecifier::ImportNamespaceSpecifier(n) if n.local.name == object.name)))
        .then(|| member.property.name.to_string())
    }
    _ => None,
  }
}

/// The function `memo(f)` or `forwardRef(f)` wraps, through any nesting of the two.
fn unwrapped<'a>(parsed: &'a Parsed<'a>, call: &'a js::CallExpression<'a>) -> Option<Found<'a>> {
  if !matches!(react_export(parsed, &call.callee).as_deref(), Some("memo" | "forwardRef")) {
    return None;
  }
  match crate::meaning::bare(crate::syntax::arg_expr(call.arguments.first()?)) {
    js::Expression::ArrowFunctionExpression(arrow) => Some(Found::Arrow(arrow)),
    js::Expression::FunctionExpression(f) => {
      let body = f.body.as_ref()?;
      Some(Found::Declared(&f.params, &body.statements, body.span))
    }
    js::Expression::CallExpression(inner) => unwrapped(parsed, inner),
    js::Expression::Identifier(id) => find_function(parsed, id.name.as_str()),
    _ => None,
  }
}

/// Whether `export` is wrapped in `forwardRef`, whose function takes the ref as its second parameter.
fn forwards_ref(parsed: &Parsed, export: &str) -> bool {
  fn wraps(parsed: &Parsed, expr: &js::Expression) -> bool {
    let js::Expression::CallExpression(call) = crate::meaning::bare(expr) else { return false };
    match react_export(parsed, &call.callee).as_deref() {
      Some("forwardRef") => true,
      Some("memo") => call.arguments.first().is_some_and(|arg| wraps(parsed, crate::syntax::arg_expr(arg))),
      _ => false,
    }
  }
  parsed.module().body.iter().any(|item| {
    let decl = match crate::syntax::top(item) {
      crate::syntax::Top::DefaultExpr(e) if export == "default" => {
        return match crate::meaning::bare(e) {
          js::Expression::Identifier(id) => forwards_ref(parsed, id.name.as_str()),
          other => wraps(parsed, other),
        };
      }
      crate::syntax::Top::Decl(decl) => decl,
      _ => return false,
    };
    let js::Declaration::VariableDeclaration(var) = decl else { return false };
    var.declarations.iter().any(|d| matches!(&d.id, js::BindingPattern::BindingIdentifier(id) if id.name.as_str() == export) && d.init.as_ref().is_some_and(|init| wraps(parsed, init)))
  })
}

/// What a class component with no `render` method is told.
pub(crate) const CLASS_COMPONENT: &str = "a class component without a `render` method";

/// Whether `class` extends React's `Component` or `PureComponent`.
fn extends_react(parsed: &Parsed, class: &js::Class) -> bool {
  class.heritage.as_ref().is_some_and(|base| matches!(react_export(parsed, &base.expression).as_deref(), Some("Component" | "PureComponent")))
}

/// `export` when it is a class extending React's `Component` or `PureComponent` with a `render()` method, and that method's body.
fn find_class<'a>(parsed: &'a Parsed<'a>, export: &str) -> Option<(&'a js::Class<'a>, Found<'a>)> {
  let render = |class: &'a js::Class<'a>| {
    class.body.body.iter().find_map(|member| match member {
      js::ClassElement::MethodDefinition(m) if !m.r#static && prop_name(&m.key, m.computed).as_deref() == Some("render") => {
        let body = m.value.body.as_ref()?;
        Some((class, Found::Declared(&m.value.params, &body.statements, body.span)))
      }
      _ => None,
    })
  };
  for item in &parsed.module().body {
    let decl = match crate::syntax::top(item) {
      crate::syntax::Top::DefaultClass(c) if export == "default" => match extends_react(parsed, c) {
        true => return render(c),
        false => continue,
      },
      crate::syntax::Top::DefaultExpr(e) if export == "default" => match crate::meaning::bare(e) {
        js::Expression::Identifier(id) => return find_class(parsed, id.name.as_str()),
        js::Expression::ClassExpression(c) if extends_react(parsed, c) => return render(c),
        _ => continue,
      },
      crate::syntax::Top::Decl(decl) => decl,
      _ => continue,
    };
    if let js::Declaration::ClassDeclaration(c) = decl {
      if c.id.as_ref().is_some_and(|id| id.name.as_str() == export) && extends_react(parsed, c) {
        return render(c);
      }
    }
  }
  None
}

/// The span of `export` when it is a class extending React's `Component` or `PureComponent`.
fn class_component_span(parsed: &Parsed, export: &str) -> Option<Span> {
  for item in &parsed.module().body {
    let decl = match crate::syntax::top(item) {
      crate::syntax::Top::DefaultClass(c) if export == "default" => match extends_react(parsed, c) {
        true => return Some(c.span),
        false => continue,
      },
      crate::syntax::Top::DefaultExpr(e) if export == "default" => match crate::meaning::bare(e) {
        js::Expression::Identifier(id) => return class_component_span(parsed, id.name.as_str()),
        js::Expression::ClassExpression(c) if extends_react(parsed, c) => return Some(c.span),
        _ => continue,
      },
      crate::syntax::Top::Decl(decl) => decl,
      _ => continue,
    };
    if let js::Declaration::ClassDeclaration(c) = decl {
      if c.id.as_ref().is_some_and(|id| id.name.as_str() == export) && extends_react(parsed, c) {
        return Some(c.span);
      }
    }
  }
  None
}

/// Where an export the build cannot read as a function is made and what made
/// it: a call another library wraps a component in (`observer(...)`) or a
/// tagged template (`styled.h1`).
fn made_component(parsed: &Parsed, export: &str) -> Option<(Span, String)> {
  let made = |e: &js::Expression| match crate::meaning::bare(e) {
    js::Expression::CallExpression(call) => {
      let name = match crate::meaning::bare(&call.callee) {
        js::Expression::Identifier(id) => id.name.to_string(),
        js::Expression::StaticMemberExpression(m) => match &m.object {
          js::Expression::Identifier(obj) => format!("{}.{}", obj.name, m.property.name),
          _ => return None,
        },
        _ => return None,
      };
      Some((call.span, format!("a component made by `{name}`, which the build does not lower")))
    }
    js::Expression::TaggedTemplateExpression(t) => Some((t.span, "a component made by a tagged template, which the build does not lower".to_owned())),
    _ => None,
  };
  for item in &parsed.module().body {
    let var = match crate::syntax::top(item) {
      crate::syntax::Top::DefaultExpr(e) if export == "default" => {
        return match crate::meaning::bare(e) {
          js::Expression::Identifier(id) => made_component(parsed, id.name.as_str()),
          other => made(other),
        };
      }
      crate::syntax::Top::Decl(js::Declaration::VariableDeclaration(var)) => var,
      _ => continue,
    };
    for decl in &var.declarations {
      match (&decl.id, &decl.init) {
        (js::BindingPattern::BindingIdentifier(name), Some(init)) if name.name.as_str() == export => return made(init),
        (pattern @ (js::BindingPattern::ObjectPattern(_) | js::BindingPattern::ArrayPattern(_)), Some(init)) if binds(pattern, export) => return made(init),
        _ => {}
      }
    }
  }
  None
}

/// Whether a destructuring pattern declares `name`.
fn binds(pattern: &js::BindingPattern, name: &str) -> bool {
  let mut names = Vec::new();
  crate::pattern_names(pattern, &mut names);
  names.iter().any(|id| id.name.as_str() == name)
}

pub(crate) enum Found<'a> {
  Declared(&'a js::FormalParameters<'a>, &'a [js::Statement<'a>], Span),
  Arrow(&'a js::ArrowFunctionExpression<'a>),
}

/// What an `async` component is told: the IR has no await in a render and the route's loader runs before it.
pub(crate) const ASYNC_COMPONENT: &str = "an `async` component; what it awaits belongs in the route's loader, `page.loader.ts`, which runs before the page renders";

/// The span of `export`'s function when it is `async`, found the way [`find_function`] finds it.
pub(crate) fn async_function_span(parsed: &Parsed, export: &str) -> Option<Span> {
  for item in &parsed.module().body {
    match crate::syntax::top(item) {
      crate::syntax::Top::DefaultFunction(f) if export == "default" => return f.r#async.then_some(f.span),
      crate::syntax::Top::DefaultExpr(e) if export == "default" => match e {
        js::Expression::ArrowFunctionExpression(arrow) => return arrow.r#async.then_some(arrow.span),
        js::Expression::Identifier(id) => return async_function_span(parsed, id.name.as_str()),
        _ => {}
      },
      crate::syntax::Top::Decl(decl) => {
        if let Some(span) = decl_async_span(decl, export) {
          return Some(span);
        }
      }
      _ => {}
    }
  }
  None
}

fn decl_async_span(decl: &js::Declaration, name: &str) -> Option<Span> {
  match decl {
    js::Declaration::FunctionDeclaration(f) if f.id.as_ref().is_some_and(|id| id.name.as_str() == name) => f.r#async.then_some(f.span),
    js::Declaration::VariableDeclaration(var) => {
      let decl = var.declarations.iter().find(|d| matches!(&d.id, js::BindingPattern::BindingIdentifier(id) if id.name.as_str() == name))?;
      match decl.init.as_ref()? {
        js::Expression::ArrowFunctionExpression(arrow) => arrow.r#async.then_some(arrow.span),
        _ => None,
      }
    }
    _ => None,
  }
}

/// The specifier a `lazy(() => import("x"))` or `defineAsyncComponent(() => import("x"))` loads, from an arrow whose body is the import or returns it.
fn lazy_import_source(call: &js::CallExpression) -> Option<String> {
  let js::Expression::ArrowFunctionExpression(arrow) = crate::syntax::arg_expr(call.arguments.first()?) else { return None };
  let expr: &js::Expression = match crate::syntax::arrow_body_of(arrow) {
    crate::syntax::ArrowBody::Block(body) => match body.body {
      [js::Statement::ReturnStatement(ret)] => ret.argument.as_ref()?,
      _ => return None,
    },
    crate::syntax::ArrowBody::Expr(e) => e,
  };
  let js::Expression::ImportExpression(import) = crate::meaning::bare(expr) else { return None };
  match &import.source {
    js::Expression::StringLiteral(s) => Some(crate::lossy(s.value)),
    _ => None,
  }
}

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

/// The span of `export default tree(...)` with `tree` from FSR's React
/// module or its dialect, a spelling the build refuses.
fn tree_call(parsed: &Parsed, meanings: &crate::meaning::Meanings) -> Option<Span> {
  parsed.module().body.iter().find_map(|item| {
    let crate::syntax::Top::DefaultExpr(js::Expression::CallExpression(call)) = crate::syntax::top(item) else { return None };
    (meanings.of_expr(&call.callee).and_then(|meant| meant.placement()) == Some("tree")).then_some(call.span)
  })
}

fn decl_function<'a>(parsed: &'a Parsed<'a>, decl: &'a js::Declaration<'a>, name: &str) -> Option<Found<'a>> {
  match decl {
    js::Declaration::FunctionDeclaration(f) if f.id.as_ref().is_some_and(|id| id.name.as_str() == name) => {
      let body = f.body.as_ref()?;
      Some(Found::Declared(&f.params, &body.statements, body.span))
    }
    js::Declaration::VariableDeclaration(var) => {
      let decl = var.declarations.iter().find(|d| matches!(&d.id, js::BindingPattern::BindingIdentifier(id) if id.name.as_str() == name))?;
      match decl.init.as_ref()? {
        js::Expression::ArrowFunctionExpression(arrow) => Some(Found::Arrow(arrow)),
        js::Expression::CallExpression(call) => unwrapped(parsed, call),
        _ => None,
      }
    }
    _ => None,
  }
}

/// A module-level value: a `const` with an initialiser or a function.
/// The `type X<…> = …` a module item declares, exported or not.
fn type_alias_decl<'e>(item: &'e js::Statement<'e>) -> Option<&'e js::TSTypeAliasDeclaration<'e>> {
  match crate::syntax::top(item) {
    crate::syntax::Top::Decl(js::Declaration::TSTypeAliasDeclaration(a)) => Some(a),
    _ => None,
  }
}

/// The return type a module-level function `name` declares: a `function` or a `const` holding an arrow or a function expression.
fn declared_return<'a>(module: &'a js::Program<'a>, name: &str) -> Option<&'a js::TSType<'a>> {
  module.body.iter().find_map(|item| {
    let crate::syntax::Top::Decl(decl) = crate::syntax::top(item) else { return None };
    match decl {
      js::Declaration::FunctionDeclaration(f) if f.id.as_ref().is_some_and(|id| id.name.as_str() == name) => f.return_type.as_deref().map(|t| &t.type_annotation),
      js::Declaration::VariableDeclaration(v) => v.declarations.iter().find_map(|d| match (&d.id, d.init.as_ref().map(crate::strip_parens)) {
        (js::BindingPattern::BindingIdentifier(id), Some(js::Expression::ArrowFunctionExpression(a))) if id.name.as_str() == name => a.return_type.as_deref().map(|t| &t.type_annotation),
        (js::BindingPattern::BindingIdentifier(id), Some(js::Expression::FunctionExpression(f))) if id.name.as_str() == name => f.return_type.as_deref().map(|t| &t.type_annotation),
        _ => None,
      }),
      _ => None,
    }
  })
}

fn find_value<'a>(parsed: &'a Parsed<'a>, name: &str) -> Option<Global<'a>> {
  for item in &parsed.module().body {
    let decl = match crate::syntax::top(item) {
      crate::syntax::Top::Decl(decl) => decl,
      crate::syntax::Top::DefaultFunction(f) => {
        if name == "default" || f.id.as_ref().is_some_and(|id| id.name.as_str() == name) {
          let body = f.body.as_ref()?;
          return Some(Global::Function(crate::params_of(&f.params), FunctionBody::Block(&body.statements), f.r#async));
        }
        continue;
      }
      crate::syntax::Top::DefaultExpr(e) if name == "default" => {
        return match crate::meaning::bare(e) {
          js::Expression::Identifier(id) => find_value(parsed, id.name.as_str()),
          js::Expression::ArrowFunctionExpression(arrow) => Some(Global::Function(crate::params_of(&arrow.params), arrow_body(arrow), arrow.r#async)),
          value => Some(Global::Const(value)),
        };
      }
      _ => continue,
    };
    match decl {
      js::Declaration::FunctionDeclaration(f) if f.id.as_ref().is_some_and(|id| id.name.as_str() == name) => {
        let body = f.body.as_ref()?;
        return Some(Global::Function(crate::params_of(&f.params), FunctionBody::Block(&body.statements), f.r#async));
      }
      js::Declaration::VariableDeclaration(var) => {
        for d in &var.declarations {
          if matches!(&d.id, js::BindingPattern::BindingIdentifier(id) if id.name.as_str() == name) {
            return match d.init.as_ref()? {
              js::Expression::ArrowFunctionExpression(arrow) => Some(Global::Function(crate::params_of(&arrow.params), arrow_body(arrow), arrow.r#async)),
              init => Some(Global::Const(init)),
            };
          }
        }
      }
      _ => {}
    }
  }
  None
}

/// The module's top-level `let` and `const` declarations with a value that is not a function, in order: each name, its initialiser and whether a function in the module writes it.
fn module_lets_of<'e>(parsed: &'e Parsed<'e>) -> Vec<(String, Option<&'e js::Expression<'e>>, bool)> {
  let writes = module_writes(&parsed.module());
  let mutations = module_mutations(&parsed.module());
  let mut out = Vec::new();
  for item in &parsed.module().body {
    let var = match item {
      js::Statement::VariableDeclaration(var) => var,
      js::Statement::ExportDeclaration(export) => match &export.declaration {
        js::Declaration::VariableDeclaration(var) => var,
        _ => continue,
      },
      _ => continue,
    };
    for decl in &var.declarations {
      let js::BindingPattern::BindingIdentifier(id) = &decl.id else { continue };
      let init = decl.init.as_ref();
      if matches!(init, Some(js::Expression::ArrowFunctionExpression(_) | js::Expression::FunctionExpression(_))) {
        continue;
      }
      let name = id.name.to_string();
      // A module-level collection a function changes in place, `cache.set(k, v)` on a `new Map()`, is state too.
      let mutated = init.is_some_and(|init| crate::made_kind(init).is_some()) && mutations.contains(&name);
      let written = (var.kind != js::VariableDeclarationKind::Const && writes.contains(&name)) || mutated;
      out.push((name, init, written));
    }
  }
  out
}

/// Whether `expr` reads the machine, a file or the clock: an extension member past `Render` reach, or `now`.
fn reads_host(expr: &Expr, natives: &[(String, Reach)]) -> bool {
  let mut found = false;
  expr.visit(&mut |e| match e {
    Expr::Ext { module, name, .. } => {
      let key = format!("{module}.{name}");
      let reach = snapfire_fsr_ir::standard_reach(module, name).or_else(|| natives.iter().find(|(n, _)| *n == key).map(|(_, r)| *r));
      found |= reach.is_some_and(|r| r != Reach::Render);
    }
    Expr::Now => found = true,
    _ => {}
  });
  found
}

/// Whether `expr` reads any cell.
fn reads_a_cell(expr: &Expr) -> bool {
  let mut found = false;
  expr.visit(&mut |e| found |= matches!(e, Expr::ReadCell(_)));
  found
}

/// Whether `expr` reads or writes the cell bound to `name`.
fn held_in_a_cell(expr: &Expr, name: &str) -> bool {
  let mut found = false;
  expr.visit(&mut |e| {
    if let Expr::ReadCell(handle) | Expr::WriteCell(handle, _) = e {
      found |= matches!(&**handle, Expr::Var(n) if n == name);
    }
  });
  found
}

/// The declaration span of `name` when it is a module-level `let` or `var` that a function in the module writes.
fn written_module_let(parsed: &Parsed, name: &str) -> Option<Span> {
  if !module_lets_of(parsed).iter().any(|(n, _, written)| n == name && *written) {
    return None;
  }
  for item in &parsed.module().body {
    let var = match item {
      js::Statement::VariableDeclaration(var) => var,
      js::Statement::ExportDeclaration(export) => match &export.declaration {
        js::Declaration::VariableDeclaration(var) => var,
        _ => continue,
      },
      _ => continue,
    };
    if let Some(decl) = var.declarations.iter().find(|d| matches!(&d.id, js::BindingPattern::BindingIdentifier(id) if id.name.as_str() == name)) {
      return Some(decl.span);
    }
  }
  None
}

/// The specifier string of every value import, `export * from` and
/// `export { .. } from` in `module`.
fn value_sources<'e>(module: &'e js::Program<'e>) -> impl Iterator<Item = &'e js::StringLiteral<'e>> {
  module.body.iter().filter_map(|item| {
    let decl = item;
    match decl {
      js::Statement::ImportDeclaration(import) if !import.import_kind.is_type() && (import.specifiers.as_ref().is_none_or(|s| s.is_empty()) || import.specifiers.iter().flatten().any(|spec| !matches!(spec, js::ImportDeclarationSpecifier::ImportSpecifier(named) if named.import_kind.is_type()))) => Some(&import.source),
      js::Statement::ExportAllDeclaration(export) if !export.export_kind.is_type() => Some(&export.source),
      js::Statement::ExportFromDeclaration(export) if !export.export_kind.is_type() => Some(&export.source),
      _ => None,
    }
  })
}

/// Every `import(`prefix${x}suffix`)` in `module`: where the call starts and
/// the literal text on either side of its one substitution.
fn template_import_calls(module: &js::Program) -> Vec<(u32, String, String)> {
  struct Found(Vec<(u32, String, String)>);
  impl<'a> oxc_ast_visit::Visit<'a> for Found {
    fn visit_import_expression(&mut self, call: &js::ImportExpression<'a>) {
      if let js::Expression::TemplateLiteral(tpl) = &call.source {
        if let ([before, after], [_]) = (tpl.quasis.as_slice(), tpl.expressions.as_slice()) {
          let text = |q: &js::TemplateElement| q.value.cooked.map(crate::lossy).unwrap_or_else(|| q.value.raw.to_string());
          self.0.push((crate::lo(call.span), text(before), text(after)));
        }
      }
      oxc_ast_visit::walk::walk_import_expression(self, call);
    }
  }
  let mut found = Found(Vec::new());
  module.walk(&mut found);
  found.0
}

/// Whether every `await` in a component's body awaits an `import(...)`, which
/// the build resolves itself, so the component may be `async`.
fn awaits_only_imports(found: &Found<'_>) -> bool {
  struct Awaits(bool);
  impl<'a> oxc_ast_visit::Visit<'a> for Awaits {
    fn visit_await_expression(&mut self, awaited: &js::AwaitExpression<'a>) {
      let mut arg = &awaited.argument;
      while let js::Expression::ParenthesizedExpression(inner) = arg {
        arg = &inner.expression;
      }
      if !matches!(arg, js::Expression::ImportExpression(_)) {
        self.0 = false;
      }
      oxc_ast_visit::walk::walk_await_expression(self, awaited);
    }
    fn visit_for_of_statement(&mut self, stmt: &js::ForOfStatement<'a>) {
      if stmt.r#await {
        self.0 = false;
      }
      oxc_ast_visit::walk::walk_for_of_statement(self, stmt);
    }
  }
  let mut awaits = Awaits(true);
  match found {
    Found::Declared(_, stmts, _) => stmts.iter().for_each(|s| s.walk(&mut awaits)),
    Found::Arrow(arrow) => match crate::syntax::arrow_body_of(arrow) {
      crate::syntax::ArrowBody::Block(block) => block.body.iter().for_each(|s| s.walk(&mut awaits)),
      crate::syntax::ArrowBody::Expr(e) => e.walk(&mut awaits),
    },
  }
  awaits.0
}

/// The call an `await import(...)` awaits, through parentheses.
fn awaited_import<'e>(init: &'e js::Expression<'e>) -> Option<&'e js::ImportExpression<'e>> {
  let mut expr = init;
  while let js::Expression::ParenthesizedExpression(inner) = expr {
    expr = &inner.expression;
  }
  let js::Expression::AwaitExpression(awaited) = expr else { return None };
  let mut arg = &awaited.argument;
  while let js::Expression::ParenthesizedExpression(inner) = arg {
    arg = &inner.expression;
  }
  match arg {
    js::Expression::ImportExpression(call) => Some(call),
    _ => None,
  }
}

/// Every `import("literal")` in `module`, wherever it sits: a module the browser loads on demand, which the build compiles like one it imports statically and never preloads.
fn dynamic_sources(module: &js::Program) -> Vec<String> {
  struct Dynamic(Vec<String>);
  impl<'a> oxc_ast_visit::Visit<'a> for Dynamic {
    fn visit_import_expression(&mut self, call: &js::ImportExpression<'a>) {
      if let js::Expression::StringLiteral(s) = &call.source {
        self.0.push(crate::lossy(s.value));
      }
      oxc_ast_visit::walk::walk_import_expression(self, call);
    }
  }
  let mut found = Dynamic(Vec::new());
  module.walk(&mut found);
  found.0
}

/// The name `local` was imported under when its source satisfies `source_is`,
/// which is how a call or a tag is recognised however it was spelled here.
/// The attribute that keeps an `<img>` of an imported asset out of the
/// rewrite. Stripped from the markup.
pub const RAW_ESCAPE: &str = "data-sf-raw";

/// The app-relative path an asset specifier names from `file`, or `None`
/// for one that is not relative.
pub(crate) fn asset_path(file: &str, specifier: &str) -> Option<String> {
  let clean = specifier.split(['?', '#']).next().unwrap_or(specifier);
  crate::resolve_specifier(file, clean)
}

/// `{ src, width, height, sources }`, what an imported image is bound to on
/// the server, so a component may read any field of it and a `meta` may
/// preload it. The browser's binding has the first three.
fn asset_object(facts: &ImageFacts) -> Expr {
  Expr::Object(vec![
    Entry::Field("src".to_owned(), Expr::lit_str(facts.src.clone())),
    Entry::Field("width".to_owned(), Expr::Lit(Lit::Int(i128::from(facts.width)))),
    Entry::Field("height".to_owned(), Expr::Lit(Lit::Int(i128::from(facts.height)))),
    Entry::Field(
      "sources".to_owned(),
      Expr::Array(
        facts
          .sources
          .iter()
          .map(|(mime, srcset)| Entry::Item(Expr::Object(vec![Entry::Field("type".to_owned(), Expr::lit_str(mime.clone())), Entry::Field("srcset".to_owned(), Expr::lit_str(srcset.clone()))])))
          .collect(),
      ),
    ),
  ])
}

/// The facts back out of what `asset_object` wrote, so a record of imports
/// bound as a global reads as the images it holds.
pub(crate) fn asset_facts(expr: &Expr) -> Option<ImageFacts> {
  let Expr::Object(entries) = expr else { return None };
  let field = |name: &str| entries.iter().find_map(|e| match e {
    Entry::Field(n, v) if n == name => Some(v),
    _ => None,
  });
  let Expr::Lit(Lit::Str(src)) = field("src")? else { return None };
  let Expr::Lit(Lit::Int(width)) = field("width")? else { return None };
  let Expr::Lit(Lit::Int(height)) = field("height")? else { return None };
  let Expr::Array(items) = field("sources")? else { return None };
  let mut sources = Vec::new();
  for item in items {
    let Entry::Item(Expr::Object(fields)) = item else { return None };
    let get = |name: &str| fields.iter().find_map(|e| match e {
      Entry::Field(n, Expr::Lit(Lit::Str(v))) if n == name => Some(v.clone()),
      _ => None,
    });
    sources.push((get("type")?, get("srcset")?));
  }
  Some(ImageFacts { src: src.clone(), width: *width as u32, height: *height as u32, sources })
}

/// Whether a lowered value is a record of imported images, which stays
/// inline rather than named as a constant, so a `Picture` indexing it can
/// read the entries.
pub(crate) fn is_asset_record(expr: &Expr) -> bool {
  let Expr::Object(entries) = expr else { return false };
  !entries.is_empty() && entries.iter().all(|e| matches!(e, Entry::Field(_, value) if asset_facts(value).is_some()))
}

/// `native("module.member", f?)` from the standard library: the pair's
/// name and its reach, `render` with a browser half and `body` without.
/// `None` for any other initialiser.
fn native_declaration(parsed: &Parsed, init: &js::Expression) -> Result<Option<(String, String, Reach)>, LowerError> {
  let js::Expression::CallExpression(call) = init else { return Ok(None) };
  let callee = &call.callee;
  if parsed.meanings.get().and_then(|meanings| meanings.of_expr(callee)).and_then(|meant| meant.std()) != Some("native") {
    return Ok(None);
  }
  let name = match call.arguments.first().map(|a| crate::syntax::arg_expr(a)) {
    Some(js::Expression::StringLiteral(s)) => crate::lossy(s.value),
    Some(other) => return Err(LowerError::Extension(parsed.residue(other.span(), "the name of a native pair must be a string literal"))),
    None => return Err(LowerError::Extension(parsed.residue(call.span, "`native` takes the pair's name, `module.member`"))),
  };
  let Some((module, member)) = name.split_once('.') else {
    return Err(LowerError::Extension(parsed.residue(call.span, format!("`{name}` is not a pair name; write `module.member`"))));
  };
  if module.is_empty() || member.is_empty() || module.contains('.') || member.contains('.') {
    return Err(LowerError::Extension(parsed.residue(call.span, format!("`{name}` is not a pair name; write `module.member`"))));
  }
  let reach = if call.arguments.len() >= 2 { Reach::Render } else { Reach::Body };
  Ok(Some((module.to_owned(), member.to_owned(), reach)))
}

/// `(source, imported name)` for a value import binding `local`.
/// A JSX tag's name as written, `Ns.Theme.Provider` for a member chain.
fn jsx_tag_name(name: &js::JSXElementName) -> String {
  match name {
    js::JSXElementName::Identifier(id) => id.name.to_string(),
    js::JSXElementName::IdentifierReference(id) => id.name.to_string(),
    js::JSXElementName::MemberExpression(m) => format!("{}.{}", jsx_object_name(&m.object), m.property.name),
    js::JSXElementName::NamespacedName(n) => format!("{}:{}", n.namespace.name, n.name.name),
    js::JSXElementName::ThisExpression(_) => "this".to_owned(),
  }
}

fn jsx_object_name(obj: &js::JSXMemberExpressionObject) -> String {
  match obj {
    js::JSXMemberExpressionObject::IdentifierReference(id) => id.name.to_string(),
    js::JSXMemberExpressionObject::MemberExpression(m) => format!("{}.{}", jsx_object_name(&m.object), m.property.name),
    js::JSXMemberExpressionObject::ThisExpression(_) => "this".to_owned(),
  }
}

pub(crate) fn find_import(parsed: &Parsed, local: &str) -> Option<(String, String)> {
  for item in &parsed.module().body {
    let js::Statement::ImportDeclaration(import) = item else { continue };
    if import.import_kind.is_type() {
      continue;
    }
    for spec in import.specifiers.iter().flatten() {
      match spec {
        js::ImportDeclarationSpecifier::ImportSpecifier(named) if !named.import_kind.is_type() && named.local.name.as_str() == local => {
          let imported = crate::schema::export_name(&named.imported);
          return Some((crate::lossy(import.source.value), imported));
        }
        js::ImportDeclarationSpecifier::ImportDefaultSpecifier(d) if d.local.name.as_str() == local => {
          return Some((crate::lossy(import.source.value), "default".to_owned()));
        }
        _ => {}
      }
    }
  }
  None
}

/// A module-level function as a lambda: parameters bind as in `Lowerer::lambda`;
/// `const`s inline; `if (c) return a;` chains become ternaries.
/// A module function as a lambda. An `async` one in a terminal page runs its statements as an async function and returns the promise they settle.
pub(crate) fn function_to_lambda(lowerer: &mut Lowerer<'_>, params: &[crate::Param], body: FunctionBody<'_>, is_async: bool) -> Lowered<Expr> {
  let depth = lowerer.scope.len();
  let names = bind_params(lowerer, params)?;
  let roots = lowerer.bind_services_params(params, depth);
  let lowered = function_to_lambda_bound(lowerer, params, names, depth, body, is_async);
  lowerer.roots.truncate(roots);
  lowered
}

/// [`function_to_lambda`] once its parameters `names` are bound above `depth`, `Ctx["services"]` ones as the services root.
fn function_to_lambda_bound(lowerer: &mut Lowerer<'_>, params: &[crate::Param], names: Vec<String>, depth: usize, body: FunctionBody<'_>, is_async: bool) -> Lowered<Expr> {
  let written = match body {
    FunctionBody::Block(stmts) => written_params(params, stmts),
    FunctionBody::Expr(_) => Vec::new(),
  };
  if let (FunctionBody::Block(stmts), false) = (body, written.is_empty()) {
    if is_async {
      lowerer.scope.truncate(depth);
      return Err(lowerer.residue(stmts[0].span(), format!("an async function that writes a field of its parameter `{}`; write the field before the function is called or return the new value", written[0].1)));
    }
    let packed = match packing_returns(lowerer, stmts, &written) {
      Ok(packed) => packed,
      Err(residue) => {
        lowerer.scope.truncate(depth);
        return Err(residue);
      }
    };
    let lambda = lambda_of(lowerer, names, depth, FunctionBody::Block(&packed), false)?;
    let Expr::Lambda { params, body } = lambda else { unreachable!("a function lowers to a lambda") };
    return Ok(Expr::Lambda { params, body: Box::new(write_back(*body, &written)) });
  }
  lambda_of(lowerer, names, depth, body, is_async)
}

/// [`function_to_lambda`] once its parameters `names` are bound above `depth`.
fn lambda_of(lowerer: &mut Lowerer<'_>, names: Vec<String>, depth: usize, body: FunctionBody<'_>, is_async: bool) -> Lowered<Expr> {
  let outer = std::mem::take(&mut lowerer.breakable);
  let lambda = lambda_in(lowerer, names, depth, body, is_async);
  lowerer.breakable = outer;
  lambda
}

fn lambda_in(lowerer: &mut Lowerer<'_>, names: Vec<String>, depth: usize, body: FunctionBody<'_>, is_async: bool) -> Lowered<Expr> {
  let defaults = if !(is_async && lowerer.reaches(Reach::Later)) {
    held_frame(lowerer, |lowerer| settle(lowerer, depth)).0
  } else {
    Vec::new()
  };
  if is_async && lowerer.reaches(Reach::Later) {
    let done = lowerer.temp();
    let b = lowerer.build();
    let ran = match body {
      FunctionBody::Block(stmts) => lowerer.async_to(stmts, Some(Expr::Var(done.clone()))),
      FunctionBody::Expr(e) => lowerer.async_to(&[b.return_stmt(Some(b.copy(e)), e.span())], Some(Expr::Var(done.clone()))),
    };
    lowerer.scope.truncate(depth);
    let body = Expr::Block(vec![Stmt::Let { name: done.clone(), expr: lowerer.new_deferred() }, Stmt::Expr(ran?), Stmt::Return(Expr::Var(done))]);
    return Ok(Expr::Lambda { params: names, body: Box::new(body) });
  }
  let result = match body {
    FunctionBody::Expr(e) => lowerer.expr(e),
    FunctionBody::Block(stmts) => lowerer.block(stmts).map(Expr::Block),
  };
  lowerer.scope.truncate(depth);
  Ok(Expr::Lambda { params: names, body: Box::new(sequenced(defaults, result?)) })
}

/// The locals declared outside a function that an arrow or a function inside `stmts` writes, by name and syntax context: a bare name, a path into one or an update.
/// The locals `stmts` write from inside a function or a `try`, whose writes outlive the path that makes them.
fn function_writes(scope: &crate::scope::Scope, stmts: &[&js::Statement]) -> Vec<(String, crate::Ctxt)> {
  shared_writes(scope, stmts, false)
}

pub(crate) fn closure_shared(scope: &crate::scope::Scope, stmts: &[js::Statement]) -> Vec<(String, crate::Ctxt)> {
  let refs: Vec<&js::Statement> = stmts.iter().collect();
  shared_writes(scope, &refs, true)
}

fn shared_writes(scope: &crate::scope::Scope, stmts: &[&js::Statement], closures_only: bool) -> Vec<(String, crate::Ctxt)> {
  struct Writes<'s> {
    scope: &'s crate::scope::Scope,
    depth: usize,
    found: Vec<(String, crate::Ctxt)>,
    closures: usize,
    read: Vec<(String, crate::Ctxt)>,
    all: Vec<(String, crate::Ctxt)>,
  }
  impl Writes<'_> {
    fn note(&mut self, id: &js::IdentifierReference) {
      let ctxt = self.scope.of(id);
      self.all.push((id.name.to_string(), ctxt));
      if self.depth > 0 && !self.found.iter().any(|(name, c)| *name == id.name.as_str() && *c == ctxt) {
        self.found.push((id.name.to_string(), ctxt));
      }
    }
  }
  impl<'a> oxc_ast_visit::Visit<'a> for Writes<'_> {
    fn visit_assignment_expression(&mut self, assign: &js::AssignmentExpression<'a>) {
      match &assign.left {
        js::AssignmentTarget::AssignmentTargetIdentifier(id) => self.note(id),
        target if target.is_member_expression() && !crate::heap_pass() => {
          if let Some((id, _)) = target.as_member_expression().and_then(crate::write_root) {
            self.note(id);
          }
        }
        _ => {}
      }
      oxc_ast_visit::walk::walk_assignment_expression(self, assign);
    }
    fn visit_update_expression(&mut self, update: &js::UpdateExpression<'a>) {
      if let js::SimpleAssignmentTarget::AssignmentTargetIdentifier(id) = &update.argument {
        self.note(id);
      }
      oxc_ast_visit::walk::walk_update_expression(self, update);
    }
    fn visit_arrow_function_expression(&mut self, arrow: &js::ArrowFunctionExpression<'a>) {
      self.depth += 1;
      self.closures += 1;
      oxc_ast_visit::walk::walk_arrow_function_expression(self, arrow);
      self.closures -= 1;
      self.depth -= 1;
    }
    fn visit_function(&mut self, function: &js::Function<'a>, flags: oxc_syntax::scope::ScopeFlags) {
      self.depth += 1;
      self.closures += 1;
      oxc_ast_visit::walk::walk_function(self, function, flags);
      self.closures -= 1;
      self.depth -= 1;
    }
    fn visit_try_statement(&mut self, t: &js::TryStatement<'a>) {
      self.depth += 1;
      oxc_ast_visit::walk::walk_try_statement(self, t);
      self.depth -= 1;
    }
    fn visit_identifier_reference(&mut self, id: &js::IdentifierReference<'a>) {
      if self.closures > 0 {
        self.read.push((id.name.to_string(), self.scope.of(id)));
      }
    }
  }
  let mut writes = Writes { scope, depth: 0, found: Vec::new(), closures: 0, read: Vec::new(), all: Vec::new() };
  stmts.iter().for_each(|stmt| stmt.walk(&mut writes));
  struct Bound<'s>(&'s crate::scope::Scope, Vec<(String, crate::Ctxt)>);
  impl<'a> oxc_ast_visit::Visit<'a> for Bound<'_> {
    fn visit_binding_identifier(&mut self, id: &js::BindingIdentifier<'a>) {
      self.1.push((id.name.to_string(), self.0.of_binding(id)));
    }
  }
  let mut bound = Bound(scope, Vec::new());
  stmts.iter().for_each(|stmt| stmt.walk(&mut bound));
  let shared: Vec<_> = writes.read.iter().filter(|name| writes.all.contains(name) && bound.1.contains(name)).cloned().collect();
  if closures_only {
    return shared;
  }
  writes.found.extend(shared);
  writes.found
}

/// The bare names `stmts` call as a statement of their own, `f()`, `void f()` or `await f()`.
fn called_names(stmts: &[js::Statement]) -> Vec<String> {
  stmts
    .iter()
    .filter_map(|stmt| {
      let js::Statement::ExpressionStatement(e) = stmt else { return None };
      let mut called = crate::meaning::bare(&e.expression);
      loop {
        match called {
          js::Expression::UnaryExpression(u) if u.operator == js::UnaryOperator::Void => called = crate::meaning::bare(&u.argument),
          js::Expression::AwaitExpression(a) => called = crate::meaning::bare(&a.argument),
          _ => break,
        }
      }
      let js::Expression::CallExpression(call) = called else { return None };
      let callee = &call.callee;
      let js::Expression::Identifier(id) = crate::meaning::bare(callee) else { return None };
      Some(id.name.to_string())
    })
    .collect()
}

/// The bare names `stmts` pass as the function of a `.on(event, f)` or `.off(event, f)` call, nested arrows included.
fn listened_names(stmts: &[js::Statement]) -> Vec<String> {
  struct Listened(Vec<String>);
  impl<'a> oxc_ast_visit::Visit<'a> for Listened {
    fn visit_call_expression(&mut self, call: &js::CallExpression<'a>) {
      if let Some(f) = call.arguments.get(1) {
        if let (Some(member), js::Expression::Identifier(name)) = (crate::meaning::bare(&call.callee).as_member_expression(), crate::meaning::bare(crate::syntax::arg_expr(f))) {
          if matches!(crate::syntax::prop(member), crate::syntax::MemberProp::Ident(method) if method.name == "on" || method.name == "off") {
            self.0.push(name.name.to_string());
          }
        }
      }
      oxc_ast_visit::walk::walk_call_expression(self, call);
    }
  }
  let mut listened = Listened(Vec::new());
  stmts.iter().for_each(|stmt| stmt.walk(&mut listened));
  listened.0
}

/// The bare names `stmts` write through `=`, a compound assignment or `++`, in source order; nested functions and arrows are walked when `through_functions` says so.
pub(crate) fn written_names(stmts: &[js::Statement], through_functions: bool) -> Vec<String> {
  let mut writes = BareWrites { names: Vec::new(), through_functions, through_calls: false };
  for stmt in stmts {
    stmt.walk(&mut writes);
  }
  writes.names
}

/// The bare names a module writes anywhere, its functions included.
/// The names `module` calls `set`, `delete`, `add` or `clear` on, which change a collection in place.
fn module_mutations(module: &js::Program) -> Vec<String> {
  let mut found = Mutations(Vec::new());
  module.walk(&mut found);
  found.0
}

/// The parameters, by position, whose fields `stmts` write: what a call writes back to the arguments it passed, as JavaScript's caller sees the objects it passed changed.
pub(crate) fn written_params(params: &[crate::Param], stmts: &[js::Statement]) -> Vec<(usize, String)> {
  if crate::heap_pass() {
    return Vec::new();
  }
  params
    .iter()
    .enumerate()
    .filter_map(|(i, param)| match param.pattern {
      js::BindingPattern::BindingIdentifier(id) if param.default.is_none() && !param.rest && crate::writes_a_member_of(stmts, std::slice::from_ref(&id.name.to_string())) => Some((i, id.name.to_string())),
      _ => None,
    })
    .collect()
}

/// `stmts` with each `return e` outside a function of their own made `return [e, p, ...]` and a last `return [undefined, p, ...]`, `p` each written parameter, so the value answers with each parameter's last value where it returns. A written parameter the body also assigns whole is refused: the caller would keep the object it passed and the field writes after the assignment would reach nothing it holds.
pub(crate) fn packing_returns<'a>(lowerer: &Lowerer<'a>, stmts: &[js::Statement], written: &[(usize, String)]) -> Lowered<Vec<js::Statement<'a>>> {
  use crate::syntax::{Walk, WalkMut};
  struct Replaced<'w>(&'w [(usize, String)], Option<(Span, String)>);
  impl<'a> oxc_ast_visit::Visit<'a> for Replaced<'_> {
    fn visit_assignment_expression(&mut self, assign: &js::AssignmentExpression<'a>) {
      if let js::AssignmentTarget::AssignmentTargetIdentifier(id) = &assign.left {
        if self.1.is_none() && self.0.iter().any(|(_, p)| *p == id.name.as_str()) {
          self.1 = Some((assign.span, id.name.to_string()));
        }
      }
      oxc_ast_visit::walk::walk_assignment_expression(self, assign);
    }
  }
  let mut replaced = Replaced(written, None);
  stmts.iter().for_each(|stmt| stmt.walk(&mut replaced));
  if let Some((span, name)) = replaced.1 {
    return Err(lowerer.residue_with(span, format!("a write to `{name}`, a parameter whose fields the function also writes"), "write the fields of the object passed, or return a new object and have the caller use it"));
  }
  let b = lowerer.build();
  let pack = |value: Option<js::Expression<'a>>, span: Span| -> js::Expression<'a> {
    let mut elems = vec![js::ArrayExpressionElement::from(value.unwrap_or_else(|| b.ident("undefined", span)))];
    elems.extend(written.iter().map(|(_, p)| js::ArrayExpressionElement::from(b.ident(p, span))));
    js::Expression::new_array_expression(span, b.vec(elems), &b.ast())
  };
  struct Returns<'p, 'a>(&'p dyn Fn(Option<js::Expression<'a>>, Span) -> js::Expression<'a>);
  impl<'a> oxc_ast_visit::VisitMut<'a> for Returns<'_, 'a> {
    fn visit_return_statement(&mut self, ret: &mut js::ReturnStatement<'a>) {
      ret.argument = Some((self.0)(ret.argument.take(), ret.span));
    }
    fn visit_function(&mut self, _: &mut js::Function<'a>, _: oxc_syntax::scope::ScopeFlags) {}
    fn visit_arrow_function_expression(&mut self, _: &mut js::ArrowFunctionExpression<'a>) {}
    fn visit_class(&mut self, _: &mut js::Class<'a>) {}
  }
  let mut out: Vec<js::Statement<'a>> = stmts.iter().map(|stmt| b.copy(stmt)).collect();
  out.iter_mut().for_each(|stmt| stmt.walk_mut(&mut Returns(&pack)));
  if !matches!(out.last(), Some(js::Statement::ReturnStatement(_))) {
    let span = stmts.last().map(|s| s.span()).unwrap_or_default();
    out.push(js::Statement::new_return_statement(span, Some(pack(None, span)), &b.ast()));
  }
  Ok(out)
}

/// A body lowered from [`packing_returns`]' statements, `[value, last, ...]`, as what [`Builtin::WriteBack`] answers.
pub(crate) fn write_back(packed: Expr, written: &[(usize, String)]) -> Expr {
  let indexes = written.iter().map(|(i, _)| Entry::Item(Expr::Lit(Lit::Int(*i as i128)))).collect();
  Expr::Builtin { name: Builtin::WriteBack, args: vec![packed, Expr::Array(indexes)] }
}

/// Whether `expr` applies a function written in place that writes back to its arguments.
fn writes_back(expr: &Expr) -> bool {
  matches!(expr, Expr::Apply { f, .. } if matches!(&**f, Expr::Lambda { body, .. } if matches!(&**body, Expr::Builtin { name: Builtin::WriteBack, .. })))
}

struct Mutations(Vec<String>);

impl<'a> oxc_ast_visit::Visit<'a> for Mutations {
  fn visit_call_expression(&mut self, call: &js::CallExpression<'a>) {
    if let Some(member) = call.callee.as_member_expression() {
      if let (js::Expression::Identifier(id), Some("set" | "delete" | "add" | "clear" | "run")) = (member.object(), prop_name_of(member).as_deref()) {
        if !self.0.iter().any(|n| *n == id.name.as_str()) {
          self.0.push(id.name.to_string());
        }
      }
    }
    oxc_ast_visit::walk::walk_call_expression(self, call);
  }
}

pub(crate) fn module_writes(module: &js::Program) -> Vec<String> {
  let mut writes = BareWrites { names: Vec::new(), through_functions: true, through_calls: true };
  module.walk(&mut writes);
  writes.names
}

struct BareWrites {
  names: Vec<String>,
  through_functions: bool,
  /// Whether a field written through a name or a name handed to a call counts, as it does for a module `let` a function may change by [`Builtin::WriteBack`].
  through_calls: bool,
}

impl BareWrites {
  fn note(&mut self, name: &str) {
    if !self.names.iter().any(|n| n == name) {
      self.names.push(name.to_owned());
    }
  }
}

impl<'a> oxc_ast_visit::Visit<'a> for BareWrites {
  fn visit_assignment_expression(&mut self, assign: &js::AssignmentExpression<'a>) {
    match &assign.left {
      js::AssignmentTarget::AssignmentTargetIdentifier(id) => self.note(&id.name),
      target if target.is_member_expression() && self.through_calls => {
        if let Some((id, _)) = target.as_member_expression().and_then(crate::write_root) {
          self.note(&id.name);
        }
      }
      _ => {}
    }
    oxc_ast_visit::walk::walk_assignment_expression(self, assign);
  }
  fn visit_call_expression(&mut self, call: &js::CallExpression<'a>) {
    if self.through_calls {
      for arg in &call.arguments {
        if let Some(js::Expression::Identifier(id)) = arg.as_expression() {
          self.note(&id.name);
        }
      }
    }
    oxc_ast_visit::walk::walk_call_expression(self, call);
  }
  fn visit_update_expression(&mut self, update: &js::UpdateExpression<'a>) {
    if let js::SimpleAssignmentTarget::AssignmentTargetIdentifier(id) = &update.argument {
      let name = id.name.to_string();
      if !self.names.contains(&name) {
        self.names.push(name);
      }
    }
    oxc_ast_visit::walk::walk_update_expression(self, update);
  }
  fn visit_arrow_function_expression(&mut self, arrow: &js::ArrowFunctionExpression<'a>) {
    if self.through_functions {
      oxc_ast_visit::walk::walk_arrow_function_expression(self, arrow);
    }
  }
  fn visit_function(&mut self, function: &js::Function<'a>, flags: oxc_syntax::scope::ScopeFlags) {
    if self.through_functions {
      oxc_ast_visit::walk::walk_function(self, function, flags);
    }
  }
}

/// Reads and declarations of a function, by name and what each is bound to.
pub(crate) struct Free<'s> {
  scope: &'s crate::scope::Scope,
  reads: Vec<(String, crate::Ctxt)>,
  declared: Vec<(String, crate::Ctxt)>,
}

impl<'a> oxc_ast_visit::Visit<'a> for Free<'_> {
  fn visit_identifier_reference(&mut self, id: &js::IdentifierReference<'a>) {
    self.reads.push((id.name.to_string(), self.scope.of(id)));
  }
  fn visit_binding_identifier(&mut self, id: &js::BindingIdentifier<'a>) {
    self.declared.push((id.name.to_string(), self.scope.of_binding(id)));
  }
}

/// The first name `node` reads that is neither its own, a module-level binding nor a global: a name of the component it sits in. A function reading one cannot be imported from its module.
pub(crate) fn closes_over<'a>(parsed: &Parsed, node: &impl crate::syntax::Walk<'a>) -> Option<String> {
  let mut free = Free { scope: &parsed.scope, reads: Vec::new(), declared: Vec::new() };
  node.walk(&mut free);
  free.reads.into_iter().find(|(name, ctxt)| !matches!(ctxt, crate::Ctxt::Top | crate::Ctxt::Unresolved) && !free.declared.contains(&(name.clone(), *ctxt))).map(|(name, _)| name)
}

/// Whether `e` is an object literal with at least one field, every field a name holding an arrow or a `function`.
fn functions_only(e: &js::Expression) -> bool {
  let js::Expression::ObjectExpression(obj) = e else { return false };
  !obj.properties.is_empty()
    && obj.properties.iter().all(|prop| match prop {
      js::ObjectPropertyKind::ObjectProperty(p) => matches!(crate::syntax::prop_of(p), crate::syntax::Prop::KeyValue(kv) if matches!(crate::meaning::bare(&kv.value), js::Expression::ArrowFunctionExpression(_) | js::Expression::FunctionExpression(_))),
      js::ObjectPropertyKind::SpreadProperty(_) => false,
    })
}

/// Each `x++`, `x--`, `++x` or `--x` on a bare name that `stmt` reaches unconditionally, in source order: its span, the name, whether it is a prefix and whether it adds. One under a condition, a short-circuit or inside a function is left out, since lifting it would run it too soon.
pub(crate) fn updates_in(stmt: &js::Statement) -> Vec<(Span, String, bool, bool)> {
  struct Updates {
    found: Vec<(Span, String, bool, bool)>,
    guarded: usize,
  }
  impl<'a> oxc_ast_visit::Visit<'a> for Updates {
    fn visit_update_expression(&mut self, update: &js::UpdateExpression<'a>) {
      if self.guarded == 0 {
        if let js::SimpleAssignmentTarget::AssignmentTargetIdentifier(id) = &update.argument {
          self.found.push((update.span, id.name.to_string(), update.prefix, update.operator == js::UpdateOperator::Increment));
        }
      }
      oxc_ast_visit::walk::walk_update_expression(self, update);
    }
    fn visit_conditional_expression(&mut self, cond: &js::ConditionalExpression<'a>) {
      cond.test.walk(self);
      self.guarded += 1;
      cond.consequent.walk(self);
      cond.alternate.walk(self);
      self.guarded -= 1;
    }
    fn visit_logical_expression(&mut self, logical: &js::LogicalExpression<'a>) {
      logical.left.walk(self);
      self.guarded += 1;
      logical.right.walk(self);
      self.guarded -= 1;
    }
    fn visit_arrow_function_expression(&mut self, _: &js::ArrowFunctionExpression<'a>) {}
    fn visit_function(&mut self, _: &js::Function<'a>, _: oxc_syntax::scope::ScopeFlags) {}
    fn visit_if_statement(&mut self, _: &js::IfStatement<'a>) {}
    fn visit_for_of_statement(&mut self, _: &js::ForOfStatement<'a>) {}
    fn visit_for_statement(&mut self, _: &js::ForStatement<'a>) {}
    fn visit_while_statement(&mut self, _: &js::WhileStatement<'a>) {}
    fn visit_switch_statement(&mut self, _: &js::SwitchStatement<'a>) {}
  }
  let mut updates = Updates { found: Vec::new(), guarded: 0 };
  stmt.walk(&mut updates);
  updates.found
}

pub(crate) fn bind_params(lowerer: &mut Lowerer<'_>, params: &[crate::Param]) -> Lowered<Vec<String>> {
  let mut names = Vec::new();
  for (i, param) in params.iter().enumerate() {
    match (param.pattern, param.default, param.rest) {
      (js::BindingPattern::BindingIdentifier(id), None, false) => {
        let name = id.name.to_string();
        lowerer.scope.push((name.clone(), Expr::Var(name.clone())));
        if let Some(kind) = param.type_annotation.and_then(|t| crate::Kind::of_type_in(&t.type_annotation, &lowerer.type_kinds)) {
          lowerer.kinds.push((name.clone(), Expr::Var(name.clone()), kind));
        }
        names.push(name);
      }
      (js::BindingPattern::BindingIdentifier(id), Some(default), false) => {
        let name = id.name.to_string();
        let default = lowerer.expr(default)?;
        lowerer.scope.push((name.clone(), Expr::Coalesce(Box::new(Expr::Var(name.clone())), Box::new(default))));
        names.push(name);
      }
      (pattern, _, true) => {
        let js::BindingPattern::BindingIdentifier(id) = pattern else { return Err(lowerer.residue(param.span, "a rest parameter that is not a name")) };
        if i + 1 != params.len() {
          return Err(lowerer.residue(param.span, "a rest parameter that is not last"));
        }
        let name = id.name.to_string();
        let gathered = format!("...{name}");
        lowerer.scope.push((name, Expr::Var(gathered.clone())));
        names.push(gathered);
      }
      (pattern, default, false) => {
        let positional = format!("${i}");
        let mut target = Expr::Var(positional.clone());
        if let Some(default) = default {
          target = Expr::Coalesce(Box::new(target), Box::new(lowerer.expr(default)?));
        }
        bind_pattern(lowerer, pattern, target)?;
        names.push(positional);
      }
    }
  }
  Ok(names)
}

/// Binds each name `pat` declares to the part of `target` it reads: a field, an item, the rest of an object or of an array. A default applies when the part is null or absent.
pub(crate) fn bind_pattern(lowerer: &mut Lowerer<'_>, pat: &js::BindingPattern, target: Expr) -> Lowered<()> {
  match pat {
    js::BindingPattern::BindingIdentifier(id) => {
      lowerer.scope.push((id.name.to_string(), target));
      Ok(())
    }
    js::BindingPattern::AssignmentPattern(assign) => {
      let default = lowerer.expr(&assign.right)?;
      bind_pattern(lowerer, &assign.left, Expr::Coalesce(Box::new(target), Box::new(default)))
    }
    js::BindingPattern::ObjectPattern(obj) => bind_object(lowerer, obj, target),
    js::BindingPattern::ArrayPattern(arr) => {
      for (j, elem) in crate::syntax::pat_elems(arr).into_iter().enumerate() {
        match elem {
          None => {}
          Some(crate::syntax::Elem::Rest(rest)) => {
            let tail = Expr::Builtin { name: Builtin::Slice, args: vec![target.clone(), Expr::Lit(Lit::Float(j as f64))] };
            bind_pattern(lowerer, &rest.argument, tail)?;
          }
          Some(crate::syntax::Elem::Pat(pat)) => bind_pattern(lowerer, pat, target.clone().index(Expr::Lit(Lit::Float(j as f64))))?,
        }
      }
      Ok(())
    }
  }
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

/// The kinds the props parameter's type names, `{ map: Map<string, number> }` written in place or through a `type` or `interface` this file declares, by prop name.
fn prop_kinds_of(parsed: &Parsed, param: &crate::Param) -> Vec<(String, crate::Kind)> {
  let ann = match param.pattern {
    js::BindingPattern::BindingIdentifier(_) | js::BindingPattern::ObjectPattern(_) if param.default.is_none() && !param.rest => param.type_annotation,
    _ => None,
  };
  ann.map(|ann| member_kinds(parsed, &ann.type_annotation, 0)).unwrap_or_default()
}

fn member_kinds(parsed: &Parsed, ty: &js::TSType, depth: usize) -> Vec<(String, crate::Kind)> {
  if depth > 8 {
    return Vec::new();
  }
  let of_members = |members: &[js::TSSignature]| -> Vec<(String, crate::Kind)> {
    members
      .iter()
      .filter_map(|member| {
        let js::TSSignature::TSPropertySignature(prop) = member else { return None };
        let (js::PropertyKey::StaticIdentifier(key), false) = (&prop.key, prop.computed) else { return None };
        crate::Kind::of_type(&prop.type_annotation.as_ref()?.type_annotation).map(|kind| (key.name.to_string(), kind))
      })
      .collect()
  };
  match ty {
    js::TSType::TSParenthesizedType(p) => member_kinds(parsed, &p.type_annotation, depth + 1),
    js::TSType::TSTypeLiteral(lit) => of_members(&lit.members),
    js::TSType::TSIntersectionType(i) => i.types.iter().flat_map(|t| member_kinds(parsed, t, depth + 1)).collect(),
    js::TSType::TSTypeReference(r) => {
      let js::TSTypeName::IdentifierReference(id) = &r.type_name else { return Vec::new() };
      let name = id.name.as_str();
      for item in &parsed.module().body {
        let crate::syntax::Top::Decl(decl) = crate::syntax::top(item) else { continue };
        match decl {
          js::Declaration::TSTypeAliasDeclaration(alias) if alias.id.name.as_str() == name => return member_kinds(parsed, &alias.type_annotation, depth + 1),
          js::Declaration::TSInterfaceDeclaration(interface) if interface.id.name.as_str() == name => return of_members(&interface.body.body),
          _ => {}
        }
      }
      Vec::new()
    }
    _ => Vec::new(),
  }
}

/// The kinds every placement recorded under `name` agrees on: a prop one placement passes as a `Map` and another as something else has no kind.
fn agreed_kinds(placed: &[(String, Vec<(String, crate::Kind)>)], name: &str) -> Vec<(String, crate::Kind)> {
  let placements: Vec<&Vec<(String, crate::Kind)>> = placed.iter().filter(|(n, _)| n == name).map(|(_, kinds)| kinds).collect();
  let Some(first) = placements.first() else { return Vec::new() };
  first
    .iter()
    .filter(|(prop, kind)| placements.iter().all(|kinds| kinds.iter().any(|(p, k)| p == prop && k == kind)))
    .cloned()
    .collect()
}

/// Writes each placement's typed props under `TYPES_PROP`, an object of prop name to variant tag, so the island mounts with the real types.
fn write_types(tmpl: &mut Tmpl, kinds_of: &dyn Fn(&str) -> Vec<(String, crate::Kind)>) {
  match tmpl {
    Tmpl::Component { module, props, children, .. } => {
      let fields: Vec<Entry> = kinds_of(module).into_iter().filter_map(|(prop, kind)| kind.tag().map(|tag| Entry::Field(prop, Expr::lit_str(tag)))).collect();
      if !fields.is_empty() && !props.iter().any(|e| matches!(e, Entry::Field(n, _) if n == TYPES_PROP)) {
        props.push(Entry::Field(TYPES_PROP.to_owned(), Expr::Object(fields)));
      }
      children.iter_mut().for_each(|c| write_types(c, kinds_of));
    }
    Tmpl::Element { children, .. } | Tmpl::Fragment(children) | Tmpl::Place { children, .. } | Tmpl::Island { children, .. } | Tmpl::Baked { children, .. } => children.iter_mut().for_each(|c| write_types(c, kinds_of)),
    Tmpl::If { then, r#else, .. } => {
      write_types(then, kinds_of);
      if let Some(e) = r#else {
        write_types(e, kinds_of);
      }
    }
    Tmpl::For { body, .. } => write_types(body, kinds_of),
    Tmpl::Let { then, .. } => write_types(then, kinds_of),
    Tmpl::Text(_) | Tmpl::Marker(_) | Tmpl::Outlet { .. } | Tmpl::Html(_) | Tmpl::Expr(_) | Tmpl::Slot(_) => {}
  }
}

pub(crate) fn bind_object(lowerer: &mut Lowerer<'_>, obj: &js::ObjectPattern, target: Expr) -> Lowered<()> {
  let mut taken: Vec<String> = Vec::new();
  for prop in crate::syntax::pat_props(obj) {
    match prop {
      crate::syntax::PatProp::Assign(a) => {
        let name = a.key.name.to_string();
        taken.push(name.clone());
        let read = target.clone().field(name.clone());
        let bound = match &a.value {
          Some(default) => Expr::Coalesce(Box::new(read), Box::new(lowerer.expr(default)?)),
          None => read,
        };
        lowerer.scope.push((name, bound));
      }
      crate::syntax::PatProp::KeyValue(kv) => {
        let key = prop_name(&kv.key, kv.computed).ok_or_else(|| lowerer.residue(kv.key.span(), "a computed field in a pattern"))?;
        taken.push(key.clone());
        bind_pattern(lowerer, &kv.value, target.clone().field(key))?;
      }
      crate::syntax::PatProp::Rest(r) => {
        let mut args = vec![target.clone()];
        args.extend(taken.iter().map(|key| Expr::lit_str(key.clone())));
        bind_pattern(lowerer, &r.argument, Expr::Builtin { name: Builtin::Omit, args })?;
      }
    }
  }
  Ok(())
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

/// The act running `join`, when there is one.
fn run_join(join: Option<&Join>, walk: &mut HandlerWalk) {
  if let Some(join) = join {
    walk.act(snapfire_fsr_ir::host::Act::Run.name().to_owned(), Expr::Object(vec![Entry::Field("event".to_owned(), Expr::lit_str(join.event.clone())), Entry::Field("carry".to_owned(), join.carry.clone())]));
  }
}

/// A branch's statements: a block's, or the one statement it is.
fn arm<'s>(stmt: &'s js::Statement<'s>) -> &'s [js::Statement<'s>] {
  match stmt {
    js::Statement::BlockStatement(block) => &block.body,
    other => std::slice::from_ref(other),
  }
}

/// Where the ids a terminal component's inlined hooks take start, past any its hoisting candidates number.
const HOOK_IDS: u32 = 1 << 20;

/// The residue of a handler that sets no state and calls no action.
const NO_EFFECT: &str = "a handler that sets no state and calls no action";

/// A `switch` as its arms: the tests that select each body in order and the `default` body. A body ends with a `break` (dropped), a `return` or the end of the last arm; cases with no body share the next one's. A body written as one block is that block's statements.
pub(crate) type SwitchArms<'s> = (Vec<(Vec<&'s js::Expression<'s>>, &'s [js::Statement<'s>])>, Option<&'s [js::Statement<'s>]>);

pub(crate) fn switch_arms<'s>(lowerer: &Lowerer<'_>, sw: &'s js::SwitchStatement<'s>) -> Lowered<SwitchArms<'s>> {
  let mut arms = Vec::new();
  let mut default = None;
  let mut tests: Vec<&js::Expression> = Vec::new();
  let last = sw.cases.len().saturating_sub(1);
  for (i, case) in sw.cases.iter().enumerate() {
    let is_default = case.test.is_none();
    if is_default && i != last {
      return Err(lowerer.residue(case.span, "a `default` before the last `case`"));
    }
    if let Some(test) = &case.test {
      tests.push(test);
    }
    let cons: &[js::Statement] = match case.consequent.as_slice() {
      [js::Statement::BlockStatement(block)] => &block.body,
      other => other,
    };
    if cons.is_empty() {
      continue;
    }
    let body: &[js::Statement] = match cons.last() {
      Some(js::Statement::BreakStatement(b)) if b.label.is_none() => &cons[..cons.len() - 1],
      Some(js::Statement::ReturnStatement(_) | js::Statement::ThrowStatement(_)) => cons,
      Some(js::Statement::ContinueStatement(c)) if c.label.is_none() && lowerer.breakable.contains(&crate::Breakable::Loop) => cons,
      _ if i == last => cons,
      _ => return Err(lowerer.residue_with(case.span, "a `case` that falls through into the next", "end it with `break` or `return`")),
    };
    if let Some(inner) = body.iter().find(|s| holds_break(s)) {
      return Err(lowerer.residue(inner.span(), "a `break` before the end of its `case`"));
    }
    if is_default {
      default = Some(body);
      tests.clear();
    } else {
      arms.push((std::mem::take(&mut tests), body));
    }
  }
  Ok((arms, default))
}

/// `throw Error(message)`, `throw new Error(message)` or `throw value` as a render that fails with the message.
pub(crate) fn throw_expr(lowerer: &mut Lowerer<'_>, thrown: &js::ThrowStatement) -> Lowered<Expr> {
  fn error_message<'e>(lowerer: &Lowerer<'_>, callee: &js::Expression, args: &'e [js::Argument<'e>]) -> Option<(String, Option<&'e js::Expression<'e>>)> {
    match crate::meaning::bare(callee) {
    js::Expression::Identifier(id) if crate::ERROR_CONSTRUCTORS.contains(&id.name.as_str()) && lowerer.meanings.as_ref().is_none_or(|m| m.ctxt(id) == crate::Ctxt::Unresolved) => Some((id.name.to_string(), args.first().map(crate::syntax::arg_expr))),
      _ => None,
    }
  }
  let message = match crate::meaning::bare(&thrown.argument) {
    js::Expression::CallExpression(call) => error_message(lowerer, &call.callee, &call.arguments),
    js::Expression::NewExpression(new) => error_message(lowerer, &new.callee, &new.arguments),
    _ => None,
  };
  let (message, error) = match message {
    Some((name, arg)) => {
      let message = match arg {
        Some(arg) => lowerer.expr(&arg)?,
        None => Expr::lit_str(""),
      };
      let error = Expr::Object(vec![Entry::Field("name".to_owned(), Expr::lit_str(name)), Entry::Field("message".to_owned(), Expr::Str(Box::new(message.clone())))]);
      (message, Some(error))
    }
    None => {
      let value = lowerer.expr(&thrown.argument)?;
      if matches!(value, Expr::Lit(_) | Expr::Template(..)) {
        (value, None)
      } else {
        let t = lowerer.temp();
        let read = Expr::Var(t.clone());
        let message = Expr::Coalesce(Box::new(Expr::Field(Box::new(read.clone()), "message".to_owned())), Box::new(read.clone()));
        return Ok(Expr::Block(vec![Stmt::Let { name: t, expr: value }, Stmt::Return(Expr::Builtin { name: Builtin::Throw, args: vec![message, read] })]));
      }
    }
  };
  Ok(Expr::Builtin { name: Builtin::Throw, args: std::iter::once(message).chain(error).collect() })
}

/// A `try` statement as a block whose value is how it ended: `lower` gives each of its blocks a value, a `catch` binds its parameter and a `finally` whose value is not `null` replaces the others'.
fn try_paths<'t>(lowerer: &mut Lowerer<'_>, t: &'t js::TryStatement<'t>, lower: &mut dyn FnMut(&mut Lowerer<'_>, Vec<&'t js::Statement<'t>>) -> Lowered<Expr>) -> Lowered<Expr> {
  let body = lower(lowerer, t.block.body.iter().collect())?;
  let (name, handler) = match &t.handler {
    None => (None, None),
    Some(catch) => {
      let depth = lowerer.scope.len();
      let name = catch_param(lowerer, catch.param.as_ref());
      let handler = name.and_then(|name| Ok((name, lower(lowerer, catch.body.body.iter().collect())?)));
      lowerer.scope.truncate(depth);
      let (name, handler) = handler?;
      (name, Some(vec![Stmt::Return(handler)]))
    }
  };
  let finally = match &t.finalizer {
    None => Vec::new(),
    Some(finalizer) => {
      let ended = lower(lowerer, finalizer.body.iter().collect())?;
      let held = lowerer.temp();
      let returned = Expr::Compare(CompareOp::Ne, Box::new(Expr::Var(held.clone())), Box::new(Expr::Lit(Lit::Null)));
      vec![Stmt::Let { name: held.clone(), expr: ended }, Stmt::If { cond: returned, then: vec![Stmt::Return(Expr::Var(held))], r#else: Vec::new() }]
    }
  };
  Ok(Expr::Block(vec![Stmt::Try { body: vec![Stmt::Return(body)], name, handler, finally }]))
}

/// The `return`s a `try` holds outside the functions inside it.
fn returns_in<'e>(t: &'e js::TryStatement<'e>) -> Vec<&'e js::ReturnStatement<'e>> {
  struct Returns<'t>(Vec<&'t js::ReturnStatement<'t>>);
  fn walk<'t>(stmt: &'t js::Statement<'t>, out: &mut Returns<'t>) {
    match stmt {
      js::Statement::ReturnStatement(ret) => out.0.push(ret),
      js::Statement::BlockStatement(b) => b.body.iter().for_each(|s| walk(s, out)),
      js::Statement::IfStatement(i) => {
        walk(&i.consequent, out);
        if let Some(alt) = &i.alternate {
          walk(alt, out);
        }
      }
      js::Statement::SwitchStatement(sw) => sw.cases.iter().flat_map(|c| &c.consequent).for_each(|s| walk(s, out)),
      js::Statement::ForOfStatement(f) => walk(&f.body, out),
      js::Statement::TryStatement(t) => {
        t.block.body.iter().for_each(|s| walk(s, out));
        t.handler.iter().flat_map(|c| &c.body.body).for_each(|s| walk(s, out));
        t.finalizer.iter().flat_map(|f| &f.body).for_each(|s| walk(s, out));
      }
      _ => {}
    }
  }
  let mut out = Returns(Vec::new());
  t.block.body.iter().for_each(|s| walk(s, &mut out));
  t.handler.iter().flat_map(|c| &c.body.body).for_each(|s| walk(s, &mut out));
  t.finalizer.iter().flat_map(|f| &f.body).for_each(|s| walk(s, &mut out));
  out.0
}

/// `run` inside a frame of its own for the `let`s a statement holds, which it answers with what `run` answered.
pub(crate) fn held_frame<T>(lowerer: &mut Lowerer<'_>, run: impl FnOnce(&mut Lowerer<'_>) -> T) -> (Vec<Stmt>, T) {
  lowerer.held.push(Vec::new());
  lowerer.held_reach.push(lowerer.rebind_reach.len());
  let result = run(lowerer);
  lowerer.held_reach.pop();
  (lowerer.held.pop().expect("the statement's frame"), result)
}

/// A `try` statement that does not `return`, run as statements over the locals it writes: each is a variable for the run, bound to where it stood before. The run's value is an object holding where each ended. Answers the run and the names it wrote.
fn try_run(lowerer: &mut Lowerer<'_>, t: &js::TryStatement, writable: &Writable<'_>) -> Lowered<(Expr, Vec<String>)> {
  let mut writes = Writes::default();
  t.walk(&mut writes);
  let Writes { written, declared } = writes;
  let written: Vec<String> = written.into_iter().filter(|name| !declared.contains(name) && !lowerer.cells.iter().any(|(cell, _)| cell == name)).collect();
  let mut run = Vec::new();
  for name in &written {
    if !writable.local(lowerer, name) {
      return Err(lowerer.residue_with(t.span, format!("a write to `{name}`, which this body did not declare"), "declare a local with `let` and write that; state changes through its setter"));
    }
    run.push(Stmt::Let { name: name.clone(), expr: current(lowerer, name) });
  }
  let depth = lowerer.scope.len();
  for name in &written {
    lowerer.scope.push((name.clone(), Expr::Var(name.clone())));
  }
  let lowered = lowerer.try_stmt(t);
  lowerer.scope.truncate(depth);
  run.push(lowered?);
  run.push(Stmt::Return(Expr::Object(written.iter().map(|name| Entry::Field(name.clone(), Expr::Var(name.clone()))).collect())));
  Ok((Expr::Block(run), written))
}

/// A `catch` parameter put in scope: a name as itself, a pattern read from a temporary that holds what was thrown.
pub(crate) fn catch_param(lowerer: &mut Lowerer<'_>, param: Option<&js::CatchParameter>) -> Lowered<Option<String>> {
  match param.map(|p| &p.pattern) {
    None => Ok(None),
    Some(js::BindingPattern::BindingIdentifier(id)) => {
      let name = id.name.to_string();
      lowerer.scope.push((name.clone(), Expr::Var(name.clone())));
      Ok(Some(name))
    }
    Some(pattern) => {
      let name = lowerer.temp();
      bind_pattern(lowerer, pattern, Expr::Var(name.clone()))?;
      Ok(Some(name))
    }
  }
}

fn holds_break(stmt: &js::Statement) -> bool {
  match stmt {
    js::Statement::BreakStatement(_) => true,
    js::Statement::BlockStatement(b) => b.body.iter().any(holds_break),
    js::Statement::IfStatement(i) => holds_break(&i.consequent) || i.alternate.as_ref().is_some_and(holds_break),
    _ => false,
  }
}

/// Whether a statement returns or throws somewhere inside it, either of which ends the path it is on.
pub(crate) fn holds_return(stmt: &js::Statement) -> bool {
  match stmt {
    js::Statement::ReturnStatement(_) | js::Statement::ThrowStatement(_) => true,
    js::Statement::BlockStatement(b) => b.body.iter().any(holds_return),
    js::Statement::IfStatement(i) => holds_return(&i.consequent) || i.alternate.as_ref().is_some_and(holds_return),
    js::Statement::TryStatement(t) => t.block.body.iter().chain(t.handler.iter().flat_map(|c| &c.body.body)).chain(t.finalizer.iter().flat_map(|f| &f.body)).any(holds_return),
    js::Statement::SwitchStatement(sw) => sw.cases.iter().any(|c| c.consequent.iter().any(holds_return)),
    _ => false,
  }
}

/// Where a straight-line body may write: names bound at or above `floor` in the scope, except those `frozen` (state a setter owns). `setters` are the component's own state setters with the state each sets, which a component body may call while it renders.
pub(crate) struct Writable<'w> {
  pub(crate) floor: usize,
  pub(crate) frozen: &'w [String],
  pub(crate) setters: &'w [(String, String)],
}

impl Writable<'_> {
  fn local(&self, lowerer: &Lowerer<'_>, name: &str) -> bool {
    !self.frozen.iter().any(|f| f == name) && lowerer.scope.iter().rposition(|(n, _)| n == name).is_some_and(|at| at >= self.floor)
  }
}

/// The state a call to one of the component's own setters sets.
fn setter_called(call: &js::CallExpression, writable: &Writable<'_>) -> Option<String> {
  let callee = &call.callee;
  let js::Expression::Identifier(id) = callee else { return None };
  writable.setters.iter().find(|(setter, _)| *setter == id.name.as_str()).map(|(_, state)| state.clone())
}

/// A setter called while the component renders: `RERUN_PREFIX` and the state hold `[value]`, the next pass's state. An updater applies to what an earlier call in the pass asked for, else to the state this pass read, as React applies queued updates in order.
fn rebind_setter(lowerer: &mut Lowerer<'_>, call: &js::CallExpression, state: &str, assigned: &mut Vec<String>) -> Lowered<()> {
  let [arg] = call.arguments.as_slice() else { return Err(lowerer.residue(call.span, "a setter called while rendering with other than one argument")) };
  let key = format!("{}{state}", snapfire_fsr_ir::render::RERUN_PREFIX);
  let asked = current(lowerer, &key);
  let value = match crate::syntax::arg_expr(arg) {
    js::Expression::ArrowFunctionExpression(_) | js::Expression::FunctionExpression(_) => {
      let base = Expr::Ternary(Box::new(asked.clone()), Box::new(Expr::Index(Box::new(asked), Box::new(Expr::Lit(Lit::Int(0))))), Box::new(current(lowerer, state)));
      let f = match crate::syntax::arg_expr(arg) {
        js::Expression::ArrowFunctionExpression(arrow) => lowerer.lambda(arrow)?,
        other => lowerer.expr(other)?,
      };
      Expr::Apply { f: Box::new(f), args: vec![base] }
    }
    other => lowerer.expr(other)?,
  };
  lowerer.scope.push((key.clone(), Expr::Array(vec![Entry::Item(value)])));
  if !assigned.contains(&key) {
    assigned.push(key);
  }
  Ok(())
}

fn current(lowerer: &Lowerer<'_>, name: &str) -> Expr {
  lowerer.scope.iter().rev().find(|(n, _)| n == name).map(|(_, e)| e.clone()).unwrap_or(Expr::Lit(Lit::Null))
}

fn alias_key(name: &str) -> String {
  format!("$alias:{name}")
}

/// `test` with each `x++`, `x--`, `++x` or `--x` of a name `counted` accepts replaced by the value JavaScript reads there, and each such write in `updates`, in order.
fn counted_test<'a>(b: crate::syntax::Build<'a>, test: &js::Expression, counted: &dyn Fn(&str) -> bool, updates: &mut Vec<(String, js::BinaryOperator, Span)>) -> js::Expression<'a> {
  match test {
    js::Expression::UpdateExpression(update) => match &update.argument {
      js::SimpleAssignmentTarget::AssignmentTargetIdentifier(id) if counted(id.name.as_str()) => {
        let op = if update.operator == js::UpdateOperator::Increment { js::BinaryOperator::Addition } else { js::BinaryOperator::Subtraction };
        updates.push((id.name.to_string(), op, update.span));
        match update.prefix {
          false => b.read(id),
          true => b.binary(b.read(id), op, b.number(1.0, update.span), update.span),
        }
      }
      _ => b.copy(test),
    },
    js::Expression::BinaryExpression(bin) => b.binary(counted_test(b, &bin.left, counted, updates), bin.operator, counted_test(b, &bin.right, counted, updates), bin.span),
    js::Expression::LogicalExpression(l) => js::Expression::new_logical_expression(l.span, counted_test(b, &l.left, counted, updates), l.operator, counted_test(b, &l.right, counted, updates), &b.ast()),
    js::Expression::UnaryExpression(unary) => js::Expression::new_unary_expression(unary.span, unary.operator, counted_test(b, &unary.argument, counted, updates), &b.ast()),
    js::Expression::ParenthesizedExpression(paren) => b.paren(counted_test(b, &paren.expression, counted, updates), paren.span),
    other => b.copy(other),
  }
}

/// `setInterval(callback, ms)` or `setTimeout(callback, ms)` with its delay written out: the timer's kind, its callback and its delay.
fn timer_call<'e>(e: &'e js::Expression<'e>) -> Option<(bool, &'e js::Expression<'e>, &'e js::Expression<'e>)> {
  let js::Expression::CallExpression(call) = crate::meaning::bare(e) else { return None };
  let callee = &call.callee;
  let js::Expression::Identifier(id) = crate::meaning::bare(callee) else { return None };
  let repeats = match id.name.as_str() {
    "setInterval" => true,
    "setTimeout" => false,
    _ => return None,
  };
  let [callback, delay] = call.arguments.as_slice() else { return None };
  Some((repeats, crate::syntax::arg_expr(callback), crate::syntax::arg_expr(delay)))
}

/// Records `const name = array[index]` or `array.at(index)` on a writable local array as an alias:
/// the array's name, the index as it was then and where the array was bound,
/// so a write through `name` reaches the element JavaScript shares.
fn note_alias(lowerer: &mut Lowerer<'_>, name: &str, init: &js::Expression, writable: &Writable<'_>) -> Lowered<()> {
  if crate::heap_pass() {
    return Ok(());
  }
  let (array, index, from_end) = match init {
    member_expr if member_expr.is_member_expression() => { let member = member_expr.as_member_expression().expect("matched as a member"); match (crate::syntax::prop(member), member.object()) {
      (crate::syntax::MemberProp::Computed(prop), js::Expression::Identifier(array)) => (array, prop, false),
      _ => return note_field_alias(lowerer, name, init, writable),
    } },
    js::Expression::ChainExpression(_) => return note_field_alias(lowerer, name, init, writable),
    js::Expression::CallExpression(call) => match (&call.callee, call.arguments.as_slice()) {
      (callee, [at]) if !crate::syntax::is_spread(at) => match callee {
        member_expr if member_expr.is_member_expression() => { let member = member_expr.as_member_expression().expect("matched as a member"); match (crate::syntax::prop(member), member.object()) {
          (crate::syntax::MemberProp::Ident(method), js::Expression::Identifier(array)) if method.name.as_str() == "at" => (array, crate::syntax::arg_expr(at), true),
          _ => return Ok(()),
        } },
        _ => return Ok(()),
      },
      _ => return Ok(()),
    },
    _ => return Ok(()),
  };
  let array = array.name.to_string();
  if !writable.local(lowerer, &array) {
    return Ok(());
  }
  let Some(at) = lowerer.scope.iter().rposition(|(n, _)| *n == array) else { return Ok(()) };
  let index = lowerer.expr(index)?;
  let index = match from_end {
    false => index,
    true => {
      let counted = Expr::Arith(ArithOp::Add, Box::new(Expr::Length(Box::new(current(lowerer, &array)))), Box::new(index.clone()));
      Expr::Ternary(Box::new(Expr::Compare(CompareOp::Lt, Box::new(index.clone()), Box::new(Expr::Lit(Lit::Float(0.0))))), Box::new(counted), Box::new(index))
    }
  };
  lowerer.scope.push((alias_key(name), Expr::Array(vec![Entry::Item(Expr::lit_str(array)), Entry::Item(index), Entry::Item(Expr::Lit(Lit::Int(at as i128)))])));
  Ok(())
}

/// The tag of a field alias record under [`alias_key`]: `[tag, root, keys, bound]`, where an element alias is `[array, index, bound]`.
const FIELD_ALIAS: &str = "$field";

/// A local bound to a field of another local, `const verbs = config.settings.verbs`, through `.`, `?.` or a computed key: recorded so a write through it rewrites the root at that path too. The root must be a local the body may write.
fn note_field_alias(lowerer: &mut Lowerer<'_>, name: &str, init: &js::Expression, writable: &Writable<'_>) -> Lowered<()> {
  let mut keys = Vec::new();
  let mut at = init;
  let root = loop {
    match crate::meaning::bare(at) {
      js::Expression::Identifier(id) => break id.name.to_string(),
      js::Expression::ChainExpression(chain) => match chain.expression.as_member_expression() {
        Some(member) => {
          keys.push(crate::syntax::prop(member));
          at = member.object();
        }
        None => return Ok(()),
      },
      member_expr if member_expr.is_member_expression() => {
        let member = member_expr.as_member_expression().expect("matched as a member");
        keys.push(crate::syntax::prop(member));
        at = member.object();
      }
      _ => return Ok(()),
    }
  };
  if keys.is_empty() || !writable.local(lowerer, &root) {
    return Ok(());
  }
  let Some(bound) = lowerer.scope.iter().rposition(|(n, _)| *n == root) else { return Ok(()) };
  keys.reverse();
  let mut lowered = Vec::with_capacity(keys.len());
  for key in keys {
    lowered.push(Entry::Item(match key {
      crate::syntax::MemberProp::Ident(field) => Expr::lit_str(field.name.as_str()),
      crate::syntax::MemberProp::Computed(key) => lowerer.expr(key)?,
      crate::syntax::MemberProp::Private(_) => return Ok(()),
    }));
  }
  lowerer.scope.push((alias_key(name), field_alias(&root, lowered, bound)));
  Ok(())
}

fn field_alias(root: &str, keys: Vec<Entry>, bound: usize) -> Expr {
  Expr::Array(vec![Entry::Item(Expr::lit_str(FIELD_ALIAS)), Entry::Item(Expr::lit_str(root)), Entry::Item(Expr::Array(keys)), Entry::Item(Expr::Lit(Lit::Int(bound as i128)))])
}

/// `current` with `value` written at `keys`, a name a field and anything else an index, as [`Lowerer::written_at`] writes along a member chain.
fn write_keys(current: Expr, keys: &[Expr], value: Expr) -> Expr {
  let Some((key, rest)) = keys.split_first() else { return value };
  match key {
    Expr::Lit(Lit::Str(field)) => {
      let inner = write_keys(current.clone().field(field.to_string()), rest, value);
      Expr::Object(vec![Entry::Spread(current), Entry::Field(field.to_string(), inner)])
    }
    key => {
      let inner = write_keys(Expr::Index(Box::new(current.clone()), Box::new(key.clone())), rest, value);
      Expr::Builtin { name: Builtin::SetAt, args: vec![current, key.clone(), inner] }
    }
  }
}

enum Alias {
  None,
  Of { array: String, index: Expr },
  Field { root: String, keys: Vec<Expr> },
  Stale(String),
  StaleField(String),
}

/// What a write to `name` reaches besides `name`. An alias whose array was
/// rebound after it stands for an element the build no longer knows.
fn alias_of(lowerer: &Lowerer<'_>, name: &str) -> Alias {
  let key = alias_key(name);
  let Some(at) = lowerer.scope.iter().rposition(|(n, _)| *n == key) else { return Alias::None };
  if lowerer.scope.iter().rposition(|(n, _)| n == name).is_none_or(|bound| bound > at) {
    return Alias::None;
  }
  let Expr::Array(entries) = &lowerer.scope[at].1 else { return Alias::None };
  if let [Entry::Item(Expr::Lit(Lit::Str(tag))), Entry::Item(Expr::Lit(Lit::Str(root))), Entry::Item(Expr::Array(keys)), Entry::Item(Expr::Lit(Lit::Int(bound)))] = entries.as_slice() {
    if tag.as_str() == FIELD_ALIAS {
      let keys = keys.iter().filter_map(|key| match key { Entry::Item(key) => Some(key.clone()), _ => None }).collect();
      return match lowerer.scope.iter().rposition(|(n, _)| n == root) {
        Some(now) if now as i128 == *bound => Alias::Field { root: root.to_string(), keys },
        _ => Alias::StaleField(root.to_string()),
      };
    }
  }
  let [Entry::Item(Expr::Lit(Lit::Str(array))), Entry::Item(index), Entry::Item(Expr::Lit(Lit::Int(bound)))] = entries.as_slice() else { return Alias::None };
  match lowerer.scope.iter().rposition(|(n, _)| n == array) {
    Some(now) if now as i128 == *bound => Alias::Of { array: array.clone(), index: index.clone() },
    _ => Alias::Stale(array.clone()),
  }
}

/// Rebinds the local `name` to `value` after a write to one of its fields,
/// and the array element or the field of another local it aliases with it.
/// The kind `name` had while bound to `from`, noted for it bound to `to`.
fn keep_kind(lowerer: &mut Lowerer<'_>, name: &str, from: &Expr, to: Expr) {
  if let Some(kind) = lowerer.kinds.iter().rev().find(|(n, held, _)| n == name && held == from).map(|(_, _, kind)| *kind) {
    lowerer.kinds.push((name.to_owned(), to, kind));
  }
}

/// `name` rebound to `value`, which keeps the kind `name` had.
/// The kind of the value a name holds now. A write that replaces the value leaves it with none.
fn kind_held(lowerer: &Lowerer<'_>, name: &str) -> Option<crate::Kind> {
  let bound = lowerer.scope.iter().rev().find(|(n, _)| n == name).map(|(_, bound)| bound)?;
  lowerer.kinds.iter().rev().find(|(n, held, _)| n == name && held == bound).map(|(_, _, kind)| *kind)
}

fn rebind_kind(lowerer: &mut Lowerer<'_>, name: &str, value: Expr, kind: crate::Kind) {
  lowerer.scope.push((name.to_owned(), value.clone()));
  lowerer.kinds.push((name.to_owned(), value, kind));
}

/// `m.set(k, v)`, `s.add(v)`, `.delete(k)` or `.clear()` with the receiver a local `Map` or `Set`: the local, what it holds after and its kind.
fn map_or_set_write(lowerer: &mut Lowerer<'_>, call: &js::CallExpression, writable: &Writable<'_>) -> Lowered<Option<(String, Expr, crate::Kind)>> {
  let callee = &call.callee;
  let Some(m) = (callee).as_member_expression() else { return Ok(None) };
  let js::Expression::Identifier(id) = m.object() else { return Ok(None) };
  let Some(method) = prop_name_of(m).filter(|method| matches!(method.as_ref(), "set" | "add" | "delete" | "clear")) else { return Ok(None) };
  let Some(kind @ (crate::Kind::Map | crate::Kind::Set)) = lowerer.kind_of(m.object()) else { return Ok(None) };
  let name = id.name.to_string();
  if !writable.local(lowerer, &name) {
    return Ok(None);
  }
  let held = current(lowerer, &name);
  let value = lowerer.collection_written(call, kind, &method, held)?;
  Ok(Some((name, value, kind)))
}

/// `q.set(k, v)`, `q.append(k, v)` or `q.delete(k)` with `q` a local `URLSearchParams` (or the same on `url.searchParams` with `url` a local `URL`): the local, what it holds after the write and its kind.
fn search_params_write(lowerer: &mut Lowerer<'_>, call: &js::CallExpression, writable: &Writable<'_>) -> Lowered<Option<(String, Expr, crate::Kind)>> {
  let callee = &call.callee;
  let Some(m) = (callee).as_member_expression() else { return Ok(None) };
  let method = match prop_name_of(m).as_deref() {
    Some(method @ ("set" | "append" | "delete")) => method.to_owned(),
    _ => return Ok(None),
  };
  let (name, kind) = match m.object() {
    js::Expression::Identifier(id) if lowerer.kind_of(m.object()) == Some(crate::Kind::Query) => (id.name.to_string(), crate::Kind::Query),
    params_expr if params_expr.is_member_expression() => { let params = params_expr.as_member_expression().expect("matched as a member"); match (params.object(), prop_name_of(params).as_deref()) {
      (js::Expression::Identifier(id), Some("searchParams")) if lowerer.kind_of(params.object()) == Some(crate::Kind::Url) => (id.name.to_string(), crate::Kind::Url),
      _ => return Ok(None),
    } },
    _ => return Ok(None),
  };
  if !writable.local(lowerer, &name) {
    return Ok(None);
  }
  let held = current(lowerer, &name);
  let search = match kind {
    crate::Kind::Url => Expr::Builtin { name: Builtin::UrlPart, args: vec![held.clone(), Expr::lit_str("search")] },
    _ => held.clone(),
  };
  let written = lowerer.collection_written(call, crate::Kind::Query, &method, search)?;
  Ok(Some(match kind {
    crate::Kind::Url => (name, Expr::Builtin { name: Builtin::UrlSet, args: vec![held, Expr::lit_str("search"), written] }, crate::Kind::Url),
    _ => (name, written, crate::Kind::Query),
  }))
}

fn write_field(lowerer: &mut Lowerer<'_>, span: Span, name: &str, value: Expr, assigned: &mut Vec<String>) -> Lowered<()> {
  let alias = alias_of(lowerer, name);
  if let Alias::Stale(array) = &alias {
    return Err(lowerer.residue(span, format!("a write through `{name}`, an element of `{array}`, after `{array}` was written")));
  }
  if let Alias::StaleField(root) = &alias {
    return Err(lowerer.residue(span, format!("a write through `{name}`, a field of `{root}`, after `{root}` was written")));
  }
  if let Alias::Field { root, keys } = &alias {
    lowerer.scope.push((name.to_owned(), value.clone()));
    if !assigned.iter().any(|a| a == name) {
      assigned.push(name.to_owned());
    }
    let rewritten = write_keys(current(lowerer, root), keys, value);
    write_field(lowerer, span, root, rewritten, assigned)?;
    let bound = lowerer.scope.iter().rposition(|(n, _)| n == root).expect("the root was just bound");
    lowerer.scope.push((alias_key(name), field_alias(root, keys.iter().cloned().map(Entry::Item).collect(), bound)));
    return Ok(());
  }
  let mut note = |name: &str| {
    if !assigned.iter().any(|a| a == name) {
      assigned.push(name.to_owned());
    }
  };
  lowerer.scope.push((name.to_owned(), value.clone()));
  note(name);
  if let Alias::Of { array, index } = alias {
    let held = current(lowerer, &array);
    let after = Expr::Arith(ArithOp::Add, Box::new(index.clone()), Box::new(Expr::Lit(Lit::Int(1))));
    let rewritten = Expr::Array(vec![
      Entry::Spread(Expr::Builtin { name: Builtin::Slice, args: vec![held.clone(), Expr::Lit(Lit::Int(0)), index.clone()] }),
      Entry::Item(value),
      Entry::Spread(Expr::Builtin { name: Builtin::Slice, args: vec![held, after] }),
    ]);
    lowerer.scope.push((array.clone(), rewritten));
    note(&array);
    let at = lowerer.scope.len() - 1;
    lowerer.scope.push((alias_key(name), Expr::Array(vec![Entry::Item(Expr::lit_str(array)), Entry::Item(index), Entry::Item(Expr::Lit(Lit::Int(at as i128)))])));
  }
  Ok(())
}

/// Statements with no `return` that bind and assign, run at build time over the scope: a write rebinds its name to the new value, an `if` rebinds each name a branch wrote to the branch's value under the condition. `assigned` collects the names written, first write first.
pub(crate) fn rebind_stmt(lowerer: &mut Lowerer<'_>, stmt: &js::Statement, writable: &Writable<'_>, assigned: &mut Vec<String>) -> Lowered<()> {
  if let js::Statement::ExpressionStatement(e) = stmt {
    if lowerer.object_write(&e.expression) {
      if crate::heap_pass() {
        let effect = lowerer.heap_write(&e.expression)?;
        if lowerer.held.is_empty() {
          let held = lowerer.temp();
          lowerer.scope.push((held.clone(), effect));
          if !assigned.iter().any(|a| *a == held) {
            assigned.push(held);
          }
        } else {
          hold_in_reach(lowerer, effect);
        }
        return Ok(());
      }
      crate::want_heap();
    }
  }
  if !lowerer.held.is_empty() {
    if let js::Statement::ExpressionStatement(e) = stmt {
      let to_cell = match &e.expression {
        js::Expression::AssignmentExpression(assign) => lowerer.cell_of_target(&assign.left).is_some(),
        js::Expression::UpdateExpression(update) => matches!(&update.argument, js::SimpleAssignmentTarget::AssignmentTargetIdentifier(id) if lowerer.cells.iter().any(|(name, _)| *name == id.name.as_str())),
        js::Expression::CallExpression(call) => matches!(&call.callee, callee if (callee).as_member_expression().is_some_and(|m| matches!(m.object(), js::Expression::Identifier(id) if lowerer.cells.iter().any(|(name, _)| *name == id.name.as_str())))),
        _ => false,
      };
      if to_cell {
        let effect = lowerer.expr(&e.expression)?;
        hold_in_reach(lowerer, effect);
        return Ok(());
      }
    }
  }
  let from = lowerer.scope.len();
  rebind_stmt_inner(lowerer, stmt, writable, assigned)?;
  if !lowerer.held.is_empty() {
    settle(lowerer, from);
  }
  Ok(())
}

fn rebind_stmt_inner(lowerer: &mut Lowerer<'_>, stmt: &js::Statement, writable: &Writable<'_>, assigned: &mut Vec<String>) -> Lowered<()> {
  let note = |assigned: &mut Vec<String>, name: &str| {
    if !assigned.iter().any(|a| a == name) {
      assigned.push(name.to_owned());
    }
  };
  match stmt {
    js::Statement::VariableDeclaration(var) => {
      for decl in &var.declarations {
        let mut expr = match decl.init.as_ref() {
          Some(init) => lowerer.expr(init)?,
          None => Expr::Lit(Lit::Undefined),
        };
        if !lowerer.held.is_empty() && lowerer.body_extension_in(&expr).is_some() {
          let name = lowerer.temp();
          lowerer.held.last_mut().expect("a frame").push(Stmt::Let { name: name.clone(), expr });
          expr = Expr::Var(name);
        } else if !lowerer.held.is_empty() && !plain(&expr) {
          expr = hold_in_reach(lowerer, expr);
        }
        bind_pattern(lowerer, &decl.id, expr)?;
        if let (js::BindingPattern::BindingIdentifier(id), Some(init)) = (&decl.id, decl.init.as_ref()) {
          lowerer.note_kind(id.name.as_str(), init);
          note_alias(lowerer, id.name.as_str(), init, writable)?;
        }
      }
      Ok(())
    }
    js::Statement::ExpressionStatement(e) => match &e.expression {
      js::Expression::AssignmentExpression(assign) => {
        let name = match &assign.left {
          js::AssignmentTarget::AssignmentTargetIdentifier(id) => id.name.to_string(),
          target => match target.as_member_expression().and_then(crate::write_root) {
            Some((id, _)) => id.name.to_string(),
            None => return Err(lowerer.residue(assign.span, "a write to something other than a local")),
          },
        };
        let b = lowerer.build();
        let read = crate::syntax::target_expr(b, &assign.left).expect("a name or a member");
        if !writable.local(lowerer, &name) {
          return Err(lowerer.residue_with(assign.span, format!("a write to `{name}`, which this body did not declare"), "declare a local with `let` and write that; state changes through its setter"));
        }
        let value = match crate::syntax::compound(b, assign.operator, b.copy(&read), b.copy(&assign.right), assign.span) {
          None => lowerer.expr(&assign.right)?,
          Some(computed) => lowerer.expr(&computed)?,
        };
        match read.as_member_expression() {
          Some(member) if matches!(member.object(), js::Expression::Identifier(_)) && lowerer.kind_of(member.object()) == Some(crate::Kind::Url) => {
            let crate::syntax::MemberProp::Ident(part) = crate::syntax::prop(member) else { return Err(lowerer.residue(member.span(), "a computed part of a `URL`")) };
            let part = part.name.to_string();
            if part == "origin" || !crate::URL_PARTS.contains(&part.as_str()) && part != "href" {
              return Err(lowerer.residue(member.span(), format!("a write to `.{part}` of a `URL`")));
            }
            let value = Expr::Builtin { name: Builtin::UrlSet, args: vec![current(lowerer, &name), Expr::lit_str(&part), value] };
            rebind_kind(lowerer, &name, value, crate::Kind::Url);
            note(assigned, &name);
            Ok(())
          }
          Some(member) => {
            let (_, path) = crate::write_root(member).expect("the root found above");
            let value = lowerer.written_at(current(lowerer, &name), &path, value)?;
            write_field(lowerer, assign.span, &name, value, assigned)
          }
          None => {
            lowerer.scope.push((name.clone(), value));
            note(assigned, &name);
            Ok(())
          }
        }
      }
      js::Expression::UpdateExpression(update) => {
        let js::SimpleAssignmentTarget::AssignmentTargetIdentifier(id) = &update.argument else { return Err(lowerer.residue(update.span, "an update of something other than a local")) };
        let name = id.name.to_string();
        if !writable.local(lowerer, &name) {
          return Err(lowerer.residue(update.span, format!("an update of `{name}`, which this body did not declare")));
        }
        let op = match update.operator {
          js::UpdateOperator::Increment => js::BinaryOperator::Addition,
          js::UpdateOperator::Decrement => js::BinaryOperator::Subtraction,
        };
        let b = lowerer.build();
        let value = lowerer.expr(&b.binary(b.updated(&update.argument), op, b.number(1.0, update.span), update.span))?;
        lowerer.scope.push((name.clone(), value));
        note(assigned, &name);
        Ok(())
      }
      js::Expression::UnaryExpression(unary) if unary.operator == js::UnaryOperator::Delete => {
        let Some(member) = (crate::meaning::bare(&unary.argument)).as_member_expression() else { return Err(lowerer.residue(unary.span, "`delete` of something other than a field of a local")) };
        let Some((id, path)) = crate::write_root(member) else { return Err(lowerer.residue(unary.span, "`delete` of something other than a field of a local")) };
        let name = id.name.to_string();
        if !writable.local(lowerer, &name) {
          return Err(lowerer.residue_with(unary.span, format!("a write to `{name}`, which this body did not declare"), "declare a local with `let` and write that; state changes through its setter"));
        }
        let key = match crate::syntax::prop(member) {
          crate::syntax::MemberProp::Ident(key) => Expr::lit_str(key.name.as_str()),
          crate::syntax::MemberProp::Computed(computed) => lowerer.expr(computed)?,
          crate::syntax::MemberProp::Private(_) => return Err(lowerer.residue(member.span(), "`delete` of a private field")),
        };
        let parent = lowerer.expr(member.object())?;
        let omitted = Expr::Builtin { name: Builtin::Omit, args: vec![parent, key] };
        let value = lowerer.written_at(current(lowerer, &name), &path[..path.len() - 1], omitted)?;
        write_field(lowerer, unary.span, &name, value, assigned)
      }
      js::Expression::CallExpression(call) => {
        if let Some(logged) = console_call(lowerer, call)? {
          let name = format!("$log{}", crate::lo(call.span));
          lowerer.scope.push((name.clone(), logged));
          note(assigned, &name);
          return Ok(());
        }
        if let Some(looped) = for_each_loop(lowerer.build(), call, lowerer.meanings.as_deref()) {
          return rebind_stmt(lowerer, &looped, writable, assigned);
        }
        if let Some(state) = setter_called(call, writable) {
          return rebind_setter(lowerer, call, &state, assigned);
        }
        {
      let callee = &call.callee;
          if let Some(m) = (callee).as_member_expression() {
            if let (js::Expression::Identifier(id), Some(method @ ("reverse" | "sort" | "pop" | "shift"))) = (m.object(), prop_name_of(m).as_deref()) {
              let name = id.name.to_string();
              if writable.local(lowerer, &name) {
                let held = current(lowerer, &name);
                let value = match method {
                  "reverse" => Expr::Builtin { name: Builtin::Reverse, args: vec![held] },
                  "sort" => {
                    let by = match call.arguments.first().map(|a| crate::syntax::arg_expr(a)) {
                      None => Expr::Lit(Lit::Null),
                      Some(js::Expression::ArrowFunctionExpression(arrow)) => lowerer.lambda(arrow)?,
                      Some(other) => lowerer.expr(other)?,
                    };
                    Expr::Sort(Box::new(held), Box::new(by))
                  }
                  "pop" => Expr::Builtin { name: Builtin::Slice, args: vec![held, Expr::Lit(Lit::Int(0)), Expr::Lit(Lit::Int(-1))] },
                  _ => Expr::Builtin { name: Builtin::Slice, args: vec![held, Expr::Lit(Lit::Int(1))] },
                };
                lowerer.scope.push((name.clone(), value));
                note(assigned, &name);
                return Ok(());
              }
            }
          }
        }
        if let Some((name, value, kind)) = map_or_set_write(lowerer, call, writable)? {
          rebind_kind(lowerer, &name, value, kind);
          note(assigned, &name);
          return Ok(());
        }
        if let Some((name, value, kind)) = search_params_write(lowerer, call, writable)? {
          rebind_kind(lowerer, &name, value, kind);
          note(assigned, &name);
          return Ok(());
        }
        let target = match call.callee.as_member_expression() {
          Some(m) => match (m.object(), prop_name_of(m).as_deref()) {
            (js::Expression::Identifier(id), Some(method @ ("push" | "unshift"))) if writable.local(lowerer, id.name.as_str()) => Some((id.name.to_string(), None, method == "push")),
            (object, Some(method @ ("push" | "unshift"))) => match object.as_member_expression().map(|field| (field.object(), prop_name_of(field))) {
              Some((js::Expression::Identifier(id), Some(name))) if writable.local(lowerer, id.name.as_str()) => Some((id.name.to_string(), Some((name, m.object())), method == "push")),
              _ => None,
            },
            _ => None,
          },
          None => None,
        };
        if lowerer.reaches(Reach::Terminal) && target.is_none() {
          // A terminal page runs a render's side effects as JavaScript does, in order: a call that emits, acts or writes a cell is held where it stands.
          let effect = lowerer.expr(&e.expression)?;
          if effectful(&effect) {
            let held = lowerer.temp();
            lowerer.scope.push((held.clone(), effect));
            note(assigned, &held);
            return Ok(());
          }
        }
        if target.is_none() && crate::heap_pass() {
          let effect = lowerer.expr(&e.expression)?;
          if lowerer.held.is_empty() {
            let held = lowerer.temp();
            lowerer.scope.push((held.clone(), effect));
            note(assigned, &held);
          } else {
            hold_in_reach(lowerer, effect);
          }
          return Ok(());
        }
        let Some((name, field, at_end)) = target else { return Err(lowerer.residue(call.span, "a call for its effect, which a straight-line body cannot hold")) };
        let mut items = Vec::with_capacity(call.arguments.len() + 1);
        for arg in &call.arguments {
          let value = lowerer.expr(crate::syntax::arg_expr(arg))?;
          items.push(if crate::syntax::is_spread(arg) { Entry::Spread(value) } else { Entry::Item(value) });
        }
        let held = Entry::Spread(match &field {
          Some((_, read)) => lowerer.expr(read)?,
          None => current(lowerer, &name),
        });
        if at_end {
          items.insert(0, held);
        } else {
          items.push(held);
        }
        match field {
          Some((field, _)) => {
            let value = Expr::Object(vec![Entry::Spread(current(lowerer, &name)), Entry::Field(field, Expr::Array(items))]);
            write_field(lowerer, call.span, &name, value, assigned)
          }
          None => {
            lowerer.scope.push((name.clone(), Expr::Array(items)));
            note(assigned, &name);
            Ok(())
          }
        }
      }
      other => Err(lowerer.residue(other.span(), "an expression statement other than a write to a local")),
    },
    js::Statement::IfStatement(branch) => {
      let mut cond = lowerer.expr(&branch.test)?;
      if !lowerer.held.is_empty() && !plain(&cond) {
        cond = hold_in_reach(lowerer, cond);
      }
      let arms = [Some(&branch.consequent), branch.alternate.as_ref()];
      rebind_arms(lowerer, vec![(Some(cond), arms[0].map(std::slice::from_ref).unwrap_or(&[]))], arms[1].map(std::slice::from_ref).unwrap_or(&[]), writable, assigned)
    }
    js::Statement::SwitchStatement(sw) => {
      let mut disc = lowerer.expr(&sw.discriminant)?;
      if !lowerer.held.is_empty() && !plain(&disc) {
        disc = hold_in_reach(lowerer, disc);
      }
      let (arms, default) = switch_arms(lowerer, sw)?;
      let mut lowered = Vec::with_capacity(arms.len());
      for (tests, body) in arms {
        let mut cond: Option<Expr> = None;
        for test in tests {
          let eq = Expr::Compare(CompareOp::Eq, Box::new(disc.clone()), Box::new(lowerer.expr(test)?));
          cond = Some(match cond {
            None => eq,
            Some(c) => Expr::Logic(LogicOp::Or, Box::new(c), Box::new(eq)),
          });
        }
        lowered.push((cond, body));
      }
      rebind_arms(lowerer, lowered, default.unwrap_or(&[]), writable, assigned)
    }
    js::Statement::BlockStatement(block) => {
      let depth = lowerer.scope.len();
      let mut inner = Vec::new();
      for stmt in &block.body {
        rebind_stmt(lowerer, stmt, writable, &mut inner)?;
      }
      let values: Vec<(String, Expr)> = inner.iter().map(|n| (n.clone(), current(lowerer, n))).collect();
      lowerer.scope.truncate(depth);
      for (name, value) in values {
        lowerer.scope.push((name.clone(), value));
        note(assigned, &name);
      }
      Ok(())
    }
    js::Statement::ForOfStatement(for_of) => rebind_for_of(lowerer, for_of, writable, assigned),
    js::Statement::ForStatement(counted) => rebind_counted(lowerer, counted, writable, assigned),
    js::Statement::WhileStatement(looped) if lowerer.reaches(Reach::Terminal) => rebind_while(lowerer, looped, writable, assigned),
    js::Statement::EmptyStatement(_) => Ok(()),
    js::Statement::TryStatement(t) if !holds_return(stmt) && !lowerer.held_reach.is_empty() => {
      let (ran, written) = try_run(lowerer, t, writable)?;
      let base = *lowerer.held_reach.last().expect("checked");
      let reach = lowerer.rebind_reach[base..].iter().cloned().reduce(|a, b| Expr::Logic(LogicOp::And, Box::new(a), Box::new(b)));
      let ran = match reach {
        Some(reach) => Expr::Ternary(Box::new(reach), Box::new(ran), Box::new(Expr::Lit(Lit::Null))),
        None => ran,
      };
      let held = lowerer.temp();
      lowerer.held.last_mut().expect("checked").push(Stmt::Let { name: held.clone(), expr: ran });
      for name in written {
        let before = current(lowerer, &name);
        let after = Expr::Field(Box::new(Expr::Var(held.clone())), name.clone());
        keep_kind(lowerer, &name, &before, after.clone());
        lowerer.scope.push((name.clone(), after));
        note(assigned, &name);
      }
      Ok(())
    }
    other => Err(lowerer.residue(other.span(), "a statement a straight-line body cannot hold; it holds `const`s, `let`s, writes to them, `if`, `switch`, `for…of`, `for (let i = a; i < b; i += s)` and `try`")),
  }
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

impl Writes {
  fn wrote(&mut self, name: &str) {
    if !self.written.iter().any(|w| w == name) {
      self.written.push(name.to_owned());
    }
  }
}

impl<'a> oxc_ast_visit::Visit<'a> for Writes {
  fn visit_assignment_expression(&mut self, assign: &js::AssignmentExpression<'a>) {
    match &assign.left {
      js::AssignmentTarget::AssignmentTargetIdentifier(id) => self.wrote(id.name.as_str()),
      target if target.is_member_expression() => {
        if let Some(js::Expression::Identifier(id)) = target.as_member_expression().map(|m| m.object()) {
          self.wrote(id.name.as_str());
        }
      }
      _ => {}
    }
    assign.right.walk(self);
  }
  fn visit_update_expression(&mut self, update: &js::UpdateExpression<'a>) {
    if let js::SimpleAssignmentTarget::AssignmentTargetIdentifier(id) = &update.argument {
      self.wrote(id.name.as_str());
    }
    oxc_ast_visit::walk::walk_update_expression(self, update);
  }
  fn visit_call_expression(&mut self, call: &js::CallExpression<'a>) {
    if let Some(m) = call.callee.as_member_expression() {
      match (m.object(), prop_name_of(m).as_deref()) {
        (js::Expression::Identifier(id), Some("push" | "unshift" | "set" | "append" | "delete" | "add" | "clear")) => self.wrote(id.name.as_str()),
        (object, Some("set" | "append" | "delete")) => {
          if let Some(params) = object.as_member_expression() {
            if let (js::Expression::Identifier(id), Some("searchParams")) = (params.object(), prop_name_of(params).as_deref()) {
              self.wrote(id.name.as_str());
            }
          }
        }
        _ => {}
      }
    }
    oxc_ast_visit::walk::walk_call_expression(self, call);
  }
  fn visit_binding_identifier(&mut self, id: &js::BindingIdentifier<'a>) {
    self.declared.push(id.name.to_string());
  }
  fn visit_function(&mut self, _: &js::Function<'a>, _: oxc_syntax::scope::ScopeFlags) {}
  fn visit_arrow_function_expression(&mut self, _: &js::ArrowFunctionExpression<'a>) {}
}

impl LoopNames {
  pub(crate) fn of(for_of: &js::ForOfStatement) -> Self {
    Self::over(for_of.span, &for_of.left, &for_of.body)
  }

  pub(crate) fn over<'a>(span: Span, head: &impl crate::syntax::Walk<'a>, body: &js::Statement<'a>) -> Self {
    let mut writes = Writes::default();
    head.walk(&mut writes);
    body.walk(&mut writes);
    let Writes { written, declared } = writes;
    let at = crate::lo(span);
    Self { item: format!("$item{at}"), acc: format!("$acc{at}"), written: written.into_iter().filter(|w| !declared.contains(w)).collect() }
  }

  /// What a written name reads inside the loop: the accumulator itself for one name, its field for several.
  pub(crate) fn read(&self, name: &str) -> Expr {
    match self.written.len() {
      1 => Expr::Var(self.acc.clone()),
      _ => Expr::Field(Box::new(Expr::Var(self.acc.clone())), name.to_owned()),
    }
  }

  /// The pattern the loop binds each item to: its head's one `const` or `let`.
  pub(crate) fn head<'e>(for_of: &'e js::ForOfStatement<'e>) -> Option<&'e js::BindingPattern<'e>> {
    match &for_of.left {
      js::ForStatementLeft::VariableDeclaration(var) => match var.declarations.as_slice() {
        [decl] if decl.init.is_none() => Some(&decl.id),
        _ => None,
      },
      _ => None,
    }
  }
}

/// A `for…of` as a fold: each outer local its body writes is the `reduce` over the items of the body's writes, starting from its value before the loop.
fn rebind_for_of(lowerer: &mut Lowerer<'_>, for_of: &js::ForOfStatement, writable: &Writable<'_>, assigned: &mut Vec<String>) -> Lowered<()> {
  if for_of.r#await {
    return Err(lowerer.residue(for_of.span, "`for await`, which a render cannot wait on"));
  }
  let Some(head) = LoopNames::head(for_of) else { return Err(lowerer.residue(for_of.span, "a `for…of` whose head is not one `const` or `let`")) };
  let names = LoopNames::of(for_of);
  let over = lowerer.expr(&for_of.right)?;
  let item = Expr::Var(names.item.clone());
  fold(lowerer, for_of.span, &names, over, &for_of.body, None, |lowerer| bind_pattern(lowerer, head, item), writable, assigned)
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

impl<'e> Counted<'e> {
  pub(crate) fn of(lowerer: &Lowerer<'_>, f: &'e js::ForStatement<'e>) -> Lowered<Self> {
    let shape = |why: &str| lowerer.residue_with(f.span, format!("a `for` loop {why}"), "a loop the build reads is `for (const x of items)` or `for (let i = a; i < b; i += s)`");
    let Some(js::ForStatementInit::VariableDeclaration(var)) = f.init.as_ref() else { return Err(shape("that does not declare its counter")) };
    let [decl] = var.declarations.as_slice() else { return Err(shape("that declares more than one counter")) };
    let (js::BindingPattern::BindingIdentifier(id), Some(from)) = (&decl.id, decl.init.as_ref()) else { return Err(shape("whose counter has no starting value")) };
    let name = id.name.to_string();
    let is_counter = |e: &js::Expression| matches!(e, js::Expression::Identifier(i) if i.name.as_str() == name);
    let (to, inclusive) = match f.test.as_ref() {
      Some(js::Expression::BinaryExpression(b)) if is_counter(&b.left) && b.operator == js::BinaryOperator::LessThan => (&b.right, false),
      Some(js::Expression::BinaryExpression(b)) if is_counter(&b.left) && b.operator == js::BinaryOperator::LessEqualThan => (&b.right, true),
      _ => return Err(shape("whose test is not `i < b` or `i <= b`")),
    };
    let step = match f.update.as_ref() {
      Some(js::Expression::UpdateExpression(u)) if u.operator == js::UpdateOperator::Increment && matches!(&u.argument, js::SimpleAssignmentTarget::AssignmentTargetIdentifier(i) if i.name.as_str() == name) => None,
      Some(js::Expression::AssignmentExpression(a)) if a.operator == js::AssignmentOperator::Addition && matches!(&a.left, js::AssignmentTarget::AssignmentTargetIdentifier(i) if i.name.as_str() == name) => match &a.right {
        js::Expression::NumericLiteral(n) if n.value == 1.0 => None,
        step => Some(step),
      },
      _ => return Err(shape("that does not step up with `i++` or `i += s`")),
    };
    let mut writes = Writes::default();
    f.body.walk(&mut writes);
    if writes.written.contains(&name) {
      return Err(shape(&format!("whose body writes `{name}`")));
    }
    struct Reads(Vec<String>);
    impl<'a> oxc_ast_visit::Visit<'a> for Reads {
      fn visit_identifier_reference(&mut self, id: &js::IdentifierReference<'a>) {
        self.0.push(id.name.to_string());
      }
    }
    let mut reads = Reads(Vec::new());
    to.walk(&mut reads);
    if let Some(read) = reads.0.iter().find(|r| writes.written.contains(r)) {
      return Err(shape(&format!("whose bound reads `{read}`, which its body writes")));
    }
    if let Some(step) = &step {
      let mut reads = Reads(Vec::new());
      step.walk(&mut reads);
      if let Some(read) = reads.0.iter().find(|r| writes.written.contains(r) || **r == name) {
        return Err(shape(&format!("whose step reads `{read}`, which the loop writes")));
      }
    }
    Ok(Self { name, from, to, inclusive, step })
  }

  /// What one pass adds to the counter.
  pub(crate) fn step_expr(&self, lowerer: &mut Lowerer<'_>) -> Lowered<Expr> {
    match self.step {
      Some(step) => lowerer.expr(step),
      None => Ok(Expr::Lit(Lit::Float(1.0))),
    }
  }

  /// How many passes the loop makes from `from` to `to`. A step that is not a positive number counts without end, which `Range` refuses, since JavaScript would loop for ever.
  pub(crate) fn count(&self, lowerer: &mut Lowerer<'_>, from: Expr, to: Expr) -> Lowered<Expr> {
    use snapfire_fsr_ir::ast::ArithOp;
    let diff = Expr::Arith(ArithOp::Sub, Box::new(to), Box::new(from));
    let Some(step) = &self.step else {
      let count = if self.inclusive { Expr::Arith(ArithOp::Add, Box::new(diff), Box::new(Expr::Lit(Lit::Float(1.0)))) } else { diff };
      return Ok(Expr::Builtin { name: Builtin::Ceil, args: vec![count] });
    };
    let positive = matches!(step, js::Expression::NumericLiteral(n) if n.value > 0.0);
    let step = lowerer.expr(step)?;
    let passes = Expr::Arith(ArithOp::Div, Box::new(diff), Box::new(step.clone()));
    let count = match self.inclusive {
      true => Expr::Arith(ArithOp::Add, Box::new(Expr::Builtin { name: Builtin::Floor, args: vec![passes] }), Box::new(Expr::Lit(Lit::Float(1.0)))),
      false => Expr::Builtin { name: Builtin::Ceil, args: vec![passes] },
    };
    if positive {
      return Ok(count);
    }
    let without_end = Expr::Arith(ArithOp::Div, Box::new(Expr::Lit(Lit::Float(1.0))), Box::new(Expr::Lit(Lit::Float(0.0))));
    Ok(Expr::Ternary(Box::new(Expr::Compare(snapfire_fsr_ir::ast::CompareOp::Gt, Box::new(step), Box::new(Expr::Lit(Lit::Float(0.0))))), Box::new(count), Box::new(without_end)))
  }

  /// The counter on the pass `item` counts, from the loop's start `start`.
  pub(crate) fn counter(&self, lowerer: &mut Lowerer<'_>, start: Expr, item: Expr) -> Lowered<Expr> {
    use snapfire_fsr_ir::ast::ArithOp;
    let offset = match &self.step {
      None => item,
      Some(step) => Expr::Arith(ArithOp::Mul, Box::new(item), Box::new(lowerer.expr(step)?)),
    };
    Ok(Expr::Arith(ArithOp::Add, Box::new(start), Box::new(offset)))
  }

  /// The counter's start, its bound and whether the bound is inclusive.
  pub(crate) fn bounds(&self) -> (&js::Expression<'_>, &js::Expression<'_>, bool) {
    (&self.from, &self.to, self.inclusive)
  }

  /// The items the loop runs over (`0..count`) with what the counter is for the item `k`.
  pub(crate) fn lowered(&self, lowerer: &mut Lowerer<'_>, item: &str) -> Lowered<(Expr, Expr)> {
    let from = lowerer.expr(&self.from)?;
    let to = lowerer.expr(&self.to)?;
    let over = Expr::Builtin { name: Builtin::Range, args: vec![self.count(lowerer, from.clone(), to)?] };
    let counter = self.counter(lowerer, from, Expr::Var(item.to_owned()))?;
    Ok((over, counter))
  }
}

fn rebind_counted(lowerer: &mut Lowerer<'_>, f: &js::ForStatement, writable: &Writable<'_>, assigned: &mut Vec<String>) -> Lowered<()> {
  let counted = Counted::of(lowerer, f)?;
  let names = LoopNames::over(f.span, &f.init, &f.body);
  let (over, counter) = counted.lowered(lowerer, &names.item)?;
  let name = counted.name.clone();
  fold(lowerer, f.span, &names, over, &f.body, None, |lowerer| {
    lowerer.scope.push((name, counter));
    Ok(())
  }, writable, assigned)
}

/// How many times a terminal's `while` runs its body at most.
pub(crate) const WHILE_CAP: f64 = 10_000.0;

/// A terminal's `while (test) body` as a fold over [`WHILE_CAP`] items whose
/// step runs only while `test` holds, so an item past the first false test
/// reads the test alone. A loop with an empty body only spends time and lowers to nothing.
fn rebind_while(lowerer: &mut Lowerer<'_>, looped: &js::WhileStatement, writable: &Writable<'_>, assigned: &mut Vec<String>) -> Lowered<()> {
  if matches!(&looped.body, js::Statement::BlockStatement(block) if block.body.is_empty()) || matches!(&looped.body, js::Statement::EmptyStatement(_)) {
    return Ok(());
  }
  let b = lowerer.build();
  let guarded = js::Statement::new_if_statement(looped.span, b.copy(&looped.test), b.copy(&looped.body), None, &b.ast());
  let names = LoopNames::over(looped.span, &looped.test, &guarded);
  let over = Expr::Builtin { name: Builtin::Range, args: vec![Expr::Lit(Lit::Float(WHILE_CAP))] };
  fold(lowerer, looped.span, &names, over, &looped.body, Some(&looped.test), |_| Ok(()), writable, assigned)
}

/// A loop as a fold: each outer local the body writes is the `reduce` over `over` of the body's writes, `bind` binding the item, starting from its value before the loop. A step under `gate` leaves the locals as they were once the gate is false.
#[allow(clippy::too_many_arguments)]
fn fold(lowerer: &mut Lowerer<'_>, span: Span, names: &LoopNames, over: Expr, body: &js::Statement, gate: Option<&js::Expression>, bind: impl FnOnce(&mut Lowerer<'_>) -> Lowered<()>, writable: &Writable<'_>, assigned: &mut Vec<String>) -> Lowered<()> {
  for name in &names.written {
    if !crate::heap_pass() && !writable.local(lowerer, name) {
      return Err(lowerer.residue_with(span, format!("a write to `{name}` inside a loop, which this body did not declare"), "declare a local with `let` before the loop and write that"));
    }
  }
  let body: &[js::Statement] = match body {
    js::Statement::BlockStatement(block) => &block.body,
    other => std::slice::from_ref(other),
  };
  let body = iteration(lowerer, body)?;
  let cell_written = names.written.iter().any(|name| lowerer.cells.iter().any(|(cell, _)| cell == name));
  let kept;
  let names = if cell_written {
    kept = LoopNames { item: names.item.clone(), acc: names.acc.clone(), written: names.written.iter().filter(|name| !lowerer.cells.iter().any(|(cell, _)| cell == *name)).cloned().collect() };
    &kept
  } else {
    names
  };
  if (cell_written || crate::heap_pass()) && names.written.is_empty() && !lowerer.held.is_empty() {
    let depth = lowerer.scope.len();
    let mut inner = Vec::new();
    lowerer.held.push(Vec::new());
    lowerer.held_reach.push(lowerer.rebind_reach.len());
    let mut test = None;
    let run = bind(lowerer).and_then(|_| {
      test = gate.map(|g| lowerer.expr(g)).transpose()?;
      body.iter().try_for_each(|s| rebind_stmt(lowerer, s, writable, &mut inner))
    });
    let held = lowerer.held.pop().expect("the fold's frame");
    lowerer.held_reach.pop();
    lowerer.scope.truncate(depth);
    run?;
    let stepped = sequenced(held, Expr::Var(names.acc.clone()));
    let stepped = match test {
      Some(test) => Expr::Ternary(Box::new(test), Box::new(stepped), Box::new(Expr::Var(names.acc.clone()))),
      None => stepped,
    };
    let step = Expr::Lambda { params: vec![names.acc.clone(), names.item.clone()], body: Box::new(stepped) };
    hold_in_reach(lowerer, Expr::Reduce(Box::new(over), Box::new(Expr::Lit(Lit::Null)), Box::new(step)));
    return Ok(());
  }
  if names.written.is_empty() {
    // Nothing outlives the loop, though a statement the build cannot run is still refused.
    let depth = lowerer.scope.len();
    let mut inner = Vec::new();
    lowerer.held.push(Vec::new());
    lowerer.held_reach.push(lowerer.rebind_reach.len());
    let run = bind(lowerer).and_then(|_| body.iter().try_for_each(|s| rebind_stmt(lowerer, s, writable, &mut inner)));
    lowerer.held.pop();
    lowerer.held_reach.pop();
    lowerer.scope.truncate(depth);
    return run;
  }
  let before: Vec<Expr> = names.written.iter().map(|n| current(lowerer, n)).collect();
  let kinds: Vec<Option<crate::Kind>> = names.written.iter().map(|n| kind_held(lowerer, n)).collect();
  let depth = lowerer.scope.len();
  for (name, kind) in names.written.iter().zip(&kinds) {
    match kind {
      Some(kind) => rebind_kind(lowerer, name, names.read(name), *kind),
      None => lowerer.scope.push((name.clone(), names.read(name))),
    }
  }
  let mut inner = Vec::new();
  lowerer.held.push(Vec::new());
  lowerer.held_reach.push(lowerer.rebind_reach.len());
  let mut test = None;
  let run = bind(lowerer).and_then(|_| {
    test = gate.map(|g| lowerer.expr(g)).transpose()?;
    body.iter().try_for_each(|s| rebind_stmt(lowerer, s, writable, &mut inner))
  });
  let after: Vec<Expr> = names.written.iter().map(|n| current(lowerer, n)).collect();
  let kinds: Vec<Option<crate::Kind>> = names.written.iter().zip(kinds).map(|(n, kind)| kind.filter(|_| kind_held(lowerer, n) == kind)).collect();
  let held = lowerer.held.pop().expect("the fold's frame");
  lowerer.held_reach.pop();
  lowerer.scope.truncate(depth);
  run?;
  let single = names.written.len() == 1;
  let gather = |values: Vec<Expr>| match single {
    true => values.into_iter().next().expect("one name"),
    false => Expr::Object(names.written.iter().cloned().zip(values).map(|(n, v)| Entry::Field(n, v)).collect()),
  };
  let stepped = sequenced(held, gather(after));
  let stepped = match test {
    Some(test) => Expr::Ternary(Box::new(test), Box::new(stepped), Box::new(Expr::Var(names.acc.clone()))),
    None => stepped,
  };
  let step = Expr::Lambda { params: vec![names.acc.clone(), names.item.clone()], body: Box::new(stepped) };
  let folded = Expr::Reduce(Box::new(over), Box::new(gather(before)), Box::new(step));
  let folded = if !lowerer.held.is_empty() { hold_in_reach(lowerer, folded) } else { folded };
  for (name, kind) in names.written.iter().zip(kinds) {
    let value = match single {
      true => folded.clone(),
      false => Expr::Field(Box::new(folded.clone()), name.clone()),
    };
    match kind {
      Some(kind) => rebind_kind(lowerer, name, value, kind),
      None => lowerer.scope.push((name.clone(), value)),
    }
    if !assigned.contains(name) {
      assigned.push(name.clone());
    }
  }
  Ok(())
}

/// One pass of a loop body with each `if (...) continue;` turned into a branch around the rest; a `break`, a `return` or a `continue` anywhere else is residue.
fn iteration<'a>(lowerer: &Lowerer<'a>, stmts: &[js::Statement]) -> Lowered<Vec<js::Statement<'a>>> {
  let b = lowerer.build();
  let mut out = Vec::new();
  for (at, stmt) in stmts.iter().enumerate() {
    match stmt {
      js::Statement::ContinueStatement(c) if c.label.is_none() => return Ok(out),
      js::Statement::IfStatement(branch) if branch.alternate.is_none() && is_continue(&branch.consequent) => {
        let rest = iteration(lowerer, &stmts[at + 1..])?;
        let test = js::Expression::new_unary_expression(branch.test.span(), js::UnaryOperator::LogicalNot, b.copy(&branch.test), &b.ast());
        out.push(js::Statement::new_if_statement(branch.span, test, b.block(rest, branch.span), None, &b.ast()));
        return Ok(out);
      }
      other => {
        if let Some((span, what)) = leaves_loop(other) {
          return Err(lowerer.residue_with(span, format!("{what} inside a loop"), "a loop the build reads runs over every item; filter the items first or put `if (...) continue;` at the top of the loop"));
        }
        out.push(b.copy(other));
      }
    }
  }
  Ok(out)
}

fn is_continue(stmt: &js::Statement) -> bool {
  match stmt {
    js::Statement::ContinueStatement(c) => c.label.is_none(),
    js::Statement::BlockStatement(block) => matches!(block.body.as_slice(), [js::Statement::ContinueStatement(c)] if c.label.is_none()),
    _ => false,
  }
}

/// The first `break`, `continue` or `return` in `stmt` that leaves the enclosing loop's iteration, outside a nested function, loop or (for `break`) `switch`.
fn leaves_loop(stmt: &js::Statement) -> Option<(Span, &'static str)> {
  struct Leaves {
    found: Option<(Span, &'static str)>,
    switches: usize,
  }
  impl<'a> oxc_ast_visit::Visit<'a> for Leaves {
    fn visit_break_statement(&mut self, b: &js::BreakStatement<'a>) {
      if self.switches == 0 || b.label.is_some() {
        self.found.get_or_insert((b.span, "`break`"));
      }
    }
    fn visit_continue_statement(&mut self, c: &js::ContinueStatement<'a>) {
      self.found.get_or_insert((c.span, "`continue`"));
    }
    fn visit_return_statement(&mut self, r: &js::ReturnStatement<'a>) {
      self.found.get_or_insert((r.span, "`return`"));
    }
    fn visit_switch_statement(&mut self, sw: &js::SwitchStatement<'a>) {
      self.switches += 1;
      oxc_ast_visit::walk::walk_switch_statement(self, sw);
      self.switches -= 1;
    }
    fn visit_for_of_statement(&mut self, _: &js::ForOfStatement<'a>) {}
    fn visit_function(&mut self, _: &js::Function<'a>, _: oxc_syntax::scope::ScopeFlags) {}
    fn visit_arrow_function_expression(&mut self, _: &js::ArrowFunctionExpression<'a>) {}
  }
  let mut leaves = Leaves { found: None, switches: 0 };
  stmt.walk(&mut leaves);
  leaves.found
}

/// Arms tried in order, each a condition and its statements, then the statements when none holds: each name any arm wrote is rebound to the value of the arm that ran.
fn rebind_arms(lowerer: &mut Lowerer<'_>, arms: Vec<(Option<Expr>, &[js::Statement])>, otherwise: &[js::Statement], writable: &Writable<'_>, assigned: &mut Vec<String>) -> Lowered<()> {
  let run = |lowerer: &mut Lowerer<'_>, stmts: &[js::Statement]| -> Lowered<Vec<(String, Expr)>> {
    let depth = lowerer.scope.len();
    let mut inner = Vec::new();
    let result = stmts.iter().try_for_each(|s| rebind_stmt(lowerer, s, writable, &mut inner));
    let values = inner.iter().map(|n| (n.clone(), current(lowerer, n))).collect();
    lowerer.scope.truncate(depth);
    result.map(|_| values)
  };
  let mut written: Vec<Vec<(String, Expr)>> = Vec::with_capacity(arms.len());
  let mut conds = Vec::with_capacity(arms.len());
  let reached = |lowerer: &mut Lowerer<'_>, cond: Expr, stmts: &[js::Statement]| {
    lowerer.rebind_reach.push(cond);
    let values = run(lowerer, stmts);
    lowerer.rebind_reach.pop();
    values
  };
  for (cond, stmts) in &arms {
    let cond = cond.clone().unwrap_or(Expr::Lit(Lit::Bool(false)));
    written.push(reached(lowerer, cond.clone(), stmts)?);
    conds.push(cond);
  }
  let none = conds.iter().cloned().reduce(|a, b| Expr::Logic(LogicOp::Or, Box::new(a), Box::new(b))).map(|any| Expr::Not(Box::new(any))).unwrap_or(Expr::Lit(Lit::Bool(true)));
  let fallback = reached(lowerer, none, otherwise)?;
  let mut names: Vec<String> = Vec::new();
  for values in written.iter().chain(std::iter::once(&fallback)) {
    for (name, _) in values {
      if !names.contains(name) {
        names.push(name.clone());
      }
    }
  }
  for name in names {
    let before = current(lowerer, &name);
    let pick = |values: &Vec<(String, Expr)>| values.iter().find(|(n, _)| *n == name).map(|(_, e)| e.clone()).unwrap_or_else(|| before.clone());
    let mut value = pick(&fallback);
    for (cond, values) in conds.iter().zip(&written).rev() {
      value = Expr::Ternary(Box::new(cond.clone()), Box::new(pick(values)), Box::new(value));
    }
    lowerer.scope.push((name.clone(), value));
    if !assigned.contains(&name) {
      assigned.push(name);
    }
  }
  Ok(())
}

fn prop_name_of(member: &js::MemberExpression) -> Option<String> {
  match crate::syntax::prop(member) {
    crate::syntax::MemberProp::Ident(i) => Some(i.name.to_string()),
    crate::syntax::MemberProp::Computed(c) => match c {
      js::Expression::StringLiteral(s) => Some(crate::lossy(s.value)),
      _ => None,
    },
    _ => None,
  }
}

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

impl HandlerWalk {
  /// `stmt`, a write to a cell or a call that makes one, run where the handler reaches it.
  pub(crate) fn effect(&mut self, stmt: Stmt) {
    match &self.reach {
      None => self.out.push(stmt),
      Some(reach) => self.out.push(Stmt::If { cond: reach.clone(), then: vec![stmt], r#else: Vec::new() }),
    }
  }

  pub(crate) fn set(&mut self, state: String, value: Expr) {
    if let Some(in_place) = &mut self.in_place {
      let var = match in_place.iter().find(|(key, _, _)| *key == state) {
        Some((_, var, _)) => var.clone(),
        None => {
          let var = format!("$set{}${state}", in_place.len());
          let start = self.patch.iter().find_map(|entry| match entry {
            Entry::Field(name, held) if *name == state => Some(held.clone()),
            _ => None,
          });
          in_place.push((state.clone(), var.clone(), start.unwrap_or_else(|| Expr::Var(state.clone()))));
          var
        }
      };
      let value = match &self.reach {
        None => value,
        Some(reach) => Expr::Ternary(Box::new(reach.clone()), Box::new(value), Box::new(Expr::Var(var.clone()))),
      };
      self.out.push(Stmt::Set { name: var.clone(), expr: value });
      self.patch.retain(|entry| !matches!(entry, Entry::Field(name, _) if *name == state));
      self.patch.push(Entry::Field(state, Expr::Var(var)));
      return;
    }
    let value = match &self.reach {
      None => value,
      Some(reach) => {
        let prior = self
          .patch
          .iter()
          .find_map(|entry| match entry {
            Entry::Field(name, held) if *name == state => Some(held.clone()),
            _ => None,
          })
          .unwrap_or_else(|| Expr::Var(state.clone()));
        Expr::Ternary(Box::new(reach.clone()), Box::new(value), Box::new(prior))
      }
    };
    self.patch.retain(|entry| !matches!(entry, Entry::Field(name, _) if *name == state));
    self.patch.push(Entry::Field(state, value));
  }

  pub(crate) fn act(&mut self, action: String, input: Expr) {
    let act = Stmt::Act { action, input };
    match &self.reach {
      None => self.out.push(act),
      Some(reach) => self.out.push(Stmt::If { cond: reach.clone(), then: vec![act], r#else: Vec::new() }),
    }
  }
}

pub(crate) fn reach_under(outer: &Option<Expr>, cond: Expr) -> Expr {
  match outer {
    None => cond,
    Some(outer) => Expr::Logic(LogicOp::And, Box::new(outer.clone()), Box::new(cond)),
  }
}

/// Whether a handler's statements do something besides setting state: an act, or a write to a cell.
/// Whether `expr` is a call of a function it holds as a value, a prop among them: a handler reference the parent passed runs that handler.
fn calls_a_value(expr: &Expr) -> bool {
  matches!(expr, Expr::Apply { f, .. } if !matches!(**f, Expr::Lambda { .. }))
}

/// `undefined` or `void 0`, which a `return` hands back as a bare `return` does.
fn is_undefined(expr: &js::Expression) -> bool {
  match crate::meaning::bare(expr) {
    js::Expression::Identifier(id) => id.name.as_str() == "undefined",
    js::Expression::UnaryExpression(u) => u.operator == js::UnaryOperator::Void && u.argument.is_literal(),
    _ => false,
  }
}

/// Whether `expr` writes the store wherever it is evaluated.
fn writes_store(expr: &Expr) -> bool {
  let mut found = false;
  expr.visit(&mut |e| found |= matches!(e, Expr::WriteStore(..)));
  found
}

fn holds_effect(stmts: &[Stmt]) -> bool {
  stmts.iter().any(|stmt| match stmt {
    Stmt::Act { .. } | Stmt::Expr(_) => true,
    Stmt::If { then, r#else, .. } => holds_effect(then) || holds_effect(r#else),
    Stmt::Try { body, handler, finally, .. } => holds_effect(body) || handler.as_deref().is_some_and(holds_effect) || holds_effect(finally),
    _ => false,
  })
}

pub(crate) fn holds_act(stmts: &[Stmt]) -> bool {
  stmts.iter().any(|stmt| match stmt {
    Stmt::Act { .. } => true,
    Stmt::If { then, r#else, .. } => holds_act(then) || holds_act(r#else),
    Stmt::Try { body, handler, finally, .. } => holds_act(body) || handler.as_deref().is_some_and(holds_act) || holds_act(finally),
    _ => false,
  })
}

pub(crate) fn block_to_expr(lowerer: &mut Lowerer<'_>, stmts: &[js::Statement]) -> Lowered<Expr> {
  let depth = lowerer.scope.len();
  block_to_expr_from(lowerer, stmts, depth)
}

/// `block_to_expr` with the names bound at or above `floor` writable, a function's parameters among them.
pub(crate) fn block_to_expr_from(lowerer: &mut Lowerer<'_>, stmts: &[js::Statement], floor: usize) -> Lowered<Expr> {
  let (depth, cells) = (lowerer.scope.len(), lowerer.cells.len());
  let wrapped = std::mem::replace(&mut lowerer.wrapped_returns, false);
  let refs: Vec<&js::Statement> = stmts.iter().collect();
  let result = block_to_expr_inner(lowerer, &refs, floor);
  lowerer.wrapped_returns = wrapped;
  lowerer.scope.truncate(depth);
  lowerer.cells.truncate(cells);
  result
}

/// Statements as the value each path returns: a branch with a `return` in it is a ternary over the paths through it and what follows, a write to a local rebinds it, a path that ends without a `return` is `null`.
fn block_to_expr_inner(lowerer: &mut Lowerer<'_>, stmts: &[&js::Statement], floor: usize) -> Lowered<Expr> {
  let Some((first, rest)) = stmts.split_first() else {
    return Ok(Expr::Lit(Lit::Null));
  };
  fn joined<'s, 'a>(head: &'s js::Statement<'a>, rest: &[&'s js::Statement<'a>]) -> Vec<&'s js::Statement<'a>> {
    let mut out: Vec<&'s js::Statement<'a>> = match head {
      js::Statement::BlockStatement(b) => b.body.iter().collect(),
      other => vec![other],
    };
    out.extend(rest.iter().copied());
    out
  }
  let path = |lowerer: &mut Lowerer<'_>, stmts: Vec<&js::Statement>| -> Lowered<Expr> {
    let depth = lowerer.scope.len();
    let value = block_to_expr_inner(lowerer, &stmts, floor);
    lowerer.scope.truncate(depth);
    value
  };
  match first {
    js::Statement::VariableDeclaration(var) if { let written = function_writes(&lowerer.parsed.scope, rest); var.declarations.iter().any(|decl| matches!(&decl.id, js::BindingPattern::BindingIdentifier(id) if written.contains(&(id.name.to_string(), lowerer.parsed.binding_ctxt(id))))) } => {
      let written = function_writes(&lowerer.parsed.scope, rest);
      let mut lets = Vec::new();
      for decl in &var.declarations {
        let mut expr = match decl.init.as_ref() {
          Some(init) => lowerer.expr(init)?,
          None => Expr::Lit(Lit::Undefined),
        };
        match &decl.id {
          js::BindingPattern::BindingIdentifier(id) if written.contains(&(id.name.to_string(), lowerer.parsed.binding_ctxt(id))) => {
            let handle = lowerer.temp();
            let name = id.name.to_string();
            lets.push(Stmt::Let { name: handle.clone(), expr: Expr::Cell(Box::new(expr)) });
            lowerer.cells.push((name.clone(), Expr::Var(handle.clone())));
            lowerer.scope.push((name.clone(), Expr::ReadCell(Box::new(Expr::Var(handle)))));
            if let Some(init) = decl.init.as_ref() {
              lowerer.note_kind(&name, init);
            }
          }
          pattern => {
            if computed_once(lowerer, &expr) || !plain(&expr) {
              let held = lowerer.temp();
              lets.push(Stmt::Let { name: held.clone(), expr });
              expr = Expr::Var(held);
            }
            bind_pattern(lowerer, pattern, expr)?;
          }
        }
      }
      let value = block_to_expr_inner(lowerer, rest, floor)?;
      Ok(sequenced(lets, value))
    }
    js::Statement::VariableDeclaration(var) if !lowerer.cells.is_empty() => {
      let mut lets = Vec::new();
      for decl in &var.declarations {
        let mut expr = match decl.init.as_ref() {
          Some(init) => lowerer.expr(init)?,
          None => Expr::Lit(Lit::Undefined),
        };
        if let (js::BindingPattern::BindingIdentifier(id), Some(js::Expression::CallExpression(call))) = (&decl.id, decl.init.as_ref().map(crate::meaning::bare)) {
          if lowerer.reaches(Reach::Process) && lowerer.spawns(call) {
            lowerer.spawned.push(id.name.to_string());
          }
        }
        if computed_once(lowerer, &expr) || !plain(&expr) {
          let held = lowerer.temp();
          lets.push(Stmt::Let { name: held.clone(), expr });
          expr = Expr::Var(held);
        }
        bind_pattern(lowerer, &decl.id, expr)?;
      }
      let value = block_to_expr_inner(lowerer, rest, floor)?;
      Ok(sequenced(lets, value))
    }
    js::Statement::VariableDeclaration(var) if var.declarations.iter().filter_map(|decl| decl.init.as_ref()).any(|init| matches!(crate::meaning::bare(init), js::Expression::NewExpression(_) | js::Expression::CallExpression(_))) && lowerer.reaches(Reach::Terminal) => {
      let mut lets = Vec::new();
      for decl in &var.declarations {
        let mut expr = match decl.init.as_ref() {
          Some(init) => lowerer.expr(init)?,
          None => Expr::Lit(Lit::Undefined),
        };
        if let (js::BindingPattern::BindingIdentifier(id), Some(js::Expression::CallExpression(call))) = (&decl.id, decl.init.as_ref().map(crate::meaning::bare)) {
          if lowerer.reaches(Reach::Process) && lowerer.spawns(call) {
            lowerer.spawned.push(id.name.to_string());
          }
        }
        if computed_once(lowerer, &expr) || !plain(&expr) {
          let held = lowerer.temp();
          lets.push(Stmt::Let { name: held.clone(), expr });
          expr = Expr::Var(held);
        }
        bind_pattern(lowerer, &decl.id, expr)?;
        if let (js::BindingPattern::BindingIdentifier(id), Some(init)) = (&decl.id, decl.init.as_ref()) {
          lowerer.note_kind(id.name.as_str(), init);
        }
      }
      let value = block_to_expr_inner(lowerer, rest, floor)?;
      Ok(sequenced(lets, value))
    }
    js::Statement::VariableDeclaration(var) => {
      let mut lets = Vec::new();
      for decl in &var.declarations {
        let mut expr = match decl.init.as_ref() {
          Some(init) => lowerer.expr(init)?,
          None => Expr::Lit(Lit::Undefined),
        };
        if !plain(&expr) {
          let held = lowerer.temp();
          lets.push(Stmt::Let { name: held.clone(), expr });
          expr = Expr::Var(held);
        }
        let from = lowerer.scope.len();
        bind_pattern(lowerer, &decl.id, expr)?;
        let (held, ()) = held_frame(lowerer, |lowerer| settle(lowerer, from));
        lets.extend(held);
        if let (js::BindingPattern::BindingIdentifier(id), Some(init)) = (&decl.id, decl.init.as_ref()) {
          lowerer.note_kind(id.name.as_str(), init);
          note_alias(lowerer, id.name.as_str(), init, &Writable { floor, frozen: &[], setters: &[] })?;
        }
      }
      let value = block_to_expr_inner(lowerer, rest, floor)?;
      Ok(sequenced(lets, value))
    }
    js::Statement::ReturnStatement(ret) => {
      let value = match &ret.argument {
        Some(arg) => lowerer.expr(arg)?,
        None => Expr::Lit(Lit::Undefined),
      };
      Ok(match lowerer.wrapped_returns {
        true => Expr::Array(vec![Entry::Item(value)]),
        false if ret.argument.is_none() => Expr::Lit(Lit::Null),
        false => value,
      })
    }
    js::Statement::ThrowStatement(thrown) => throw_expr(lowerer, thrown),
    js::Statement::EmptyStatement(_) => block_to_expr_inner(lowerer, rest, floor),
    js::Statement::TryStatement(t) => {
      let outer = std::mem::replace(&mut lowerer.wrapped_returns, true);
      let lowered = try_paths(lowerer, t, &mut |lowerer, stmts| path(lowerer, stmts));
      lowerer.wrapped_returns = outer;
      let ended = lowered?;
      let held = lowerer.temp();
      let after = block_to_expr_inner(lowerer, rest, floor)?;
      let returned = match outer {
        true => Expr::Var(held.clone()),
        false => Expr::Index(Box::new(Expr::Var(held.clone())), Box::new(Expr::Lit(Lit::Int(0)))),
      };
      let fell = Expr::Compare(CompareOp::Eq, Box::new(Expr::Var(held.clone())), Box::new(Expr::Lit(Lit::Null)));
      Ok(Expr::Block(vec![Stmt::Let { name: held, expr: ended }, Stmt::Return(Expr::Ternary(Box::new(fell), Box::new(after), Box::new(returned)))]))
    }
    js::Statement::BlockStatement(_) if holds_return(first) => path(lowerer, joined(first, rest)),
    js::Statement::IfStatement(if_stmt) if holds_return(first) => {
      let cond = lowerer.expr(&if_stmt.test)?;
      let then = path(lowerer, joined(&if_stmt.consequent, rest))?;
      let otherwise = match &if_stmt.alternate {
        Some(alt) => path(lowerer, joined(alt, rest))?,
        None => path(lowerer, rest.to_vec())?,
      };
      Ok(Expr::Ternary(Box::new(cond), Box::new(then), Box::new(otherwise)))
    }
    js::Statement::SwitchStatement(sw) if holds_return(first) && { let d = lowerer.expr(&sw.discriminant)?; !plain(&d) } => {
      let disc = lowerer.expr(&sw.discriminant)?;
      let held = lowerer.temp();
      let name = format!("$sw{}", crate::lo(sw.span));
      lowerer.scope.push((name.clone(), Expr::Var(held.clone())));
      let b = lowerer.build();
      let mut replaced = b.copy(&**sw);
      replaced.discriminant = b.ident(&name, sw.span);
      let stmt = js::Statement::SwitchStatement(b.boxed(replaced));
      let mut all: Vec<&js::Statement> = vec![&stmt];
      all.extend(rest.iter().copied());
      let value = block_to_expr_inner(lowerer, &all, floor)?;
      Ok(sequenced(vec![Stmt::Let { name: held, expr: disc }], value))
    }
    js::Statement::SwitchStatement(sw) if holds_return(first) => {
      let disc = lowerer.expr(&sw.discriminant)?;
      let (arms, default) = switch_arms(lowerer, sw)?;
      let mut fallback: Vec<&js::Statement> = default.map(|d| d.iter().collect()).unwrap_or_default();
      fallback.extend(rest.iter().copied());
      let mut value = path(lowerer, fallback)?;
      for (tests, body) in arms.into_iter().rev() {
        let mut cond: Option<Expr> = None;
        for test in tests {
          let eq = Expr::Compare(CompareOp::Eq, Box::new(disc.clone()), Box::new(lowerer.expr(test)?));
          cond = Some(match cond {
            None => eq,
            Some(c) => Expr::Logic(LogicOp::Or, Box::new(c), Box::new(eq)),
          });
        }
        let mut stmts: Vec<&js::Statement> = body.iter().collect();
        stmts.extend(rest.iter().copied());
        let then = path(lowerer, stmts)?;
        value = Expr::Ternary(Box::new(cond.unwrap_or(Expr::Lit(Lit::Bool(false)))), Box::new(then), Box::new(value));
      }
      Ok(value)
    }
    js::Statement::ForStatement(_) | js::Statement::ForOfStatement(_) | js::Statement::WhileStatement(_) if returns_from_loop(first) => {
      let looped = loop_return(lowerer.build(), first, rest);
      path(lowerer, looped.iter().collect())
    }
    js::Statement::IfStatement(branch) if lowerer.reaches(Reach::Terminal) && holds_acts(lowerer, first) => {
      let cond = lowerer.expr(&branch.test)?;
      let arm = |lowerer: &mut Lowerer<'_>, arm: &js::Statement| -> Lowered<Expr> {
        let depth = lowerer.scope.len();
        let stmts: &[js::Statement] = match arm {
          js::Statement::BlockStatement(block) => &block.body,
          other => std::slice::from_ref(other),
        };
        let lowered = block_to_expr(lowerer, stmts);
        lowerer.scope.truncate(depth);
        lowered
      };
      let then = arm(lowerer, &branch.consequent)?;
      let otherwise = match &branch.alternate {
        Some(alt) => arm(lowerer, alt)?,
        None => Expr::Lit(Lit::Null),
      };
      let value = block_to_expr_inner(lowerer, rest, floor)?;
      Ok(sequenced(vec![Stmt::If { cond, then: vec![Stmt::Expr(then)], r#else: vec![Stmt::Expr(otherwise)] }], value))
    }
    js::Statement::ExpressionStatement(stmt) if matches!(&stmt.expression, js::Expression::CallExpression(call) if lowerer.issues_act(call)) => {
      let act = lowerer.effect(&stmt.expression)?;
      let value = block_to_expr_inner(lowerer, rest, floor)?;
      Ok(sequenced(vec![act], value))
    }
    js::Statement::ExpressionStatement(stmt) if writes_a_cell(lowerer, &stmt.expression) => {
      let Stmt::Expr(effect) = lowerer.effect(&stmt.expression)? else { return Err(lowerer.residue(stmt.span, "a write to a cell the build cannot order")) };
      let held = lowerer.temp();
      let value = block_to_expr_inner(lowerer, rest, floor)?;
      Ok(sequenced(vec![Stmt::Let { name: held, expr: effect }], value))
    }
    js::Statement::ExpressionStatement(stmt) if matches!(crate::meaning::bare(&stmt.expression), js::Expression::CallExpression(call) if console_method(lowerer, call).is_some()) => {
      let logged = lowerer.effect(&stmt.expression)?;
      let value = block_to_expr_inner(lowerer, rest, floor)?;
      Ok(sequenced(vec![logged], value))
    }
    js::Statement::ExpressionStatement(stmt) if matches!(&stmt.expression, js::Expression::CallExpression(_)) && {
      let depth = lowerer.scope.len();
      let probed = lowerer.expr(&stmt.expression).map(|e| writes_store(&e) || (lowerer.reaches(Reach::Terminal) && effectful(&e))).unwrap_or(false);
      lowerer.scope.truncate(depth);
      probed
    } => {
      let effect = lowerer.expr(&stmt.expression)?;
      let value = block_to_expr_inner(lowerer, rest, floor)?;
      Ok(sequenced(vec![Stmt::Expr(effect)], value))
    }
    straight => {
      let writable = Writable { floor, frozen: &[], setters: &[] };
      let (held, run) = held_frame(lowerer, |lowerer| rebind_stmt(lowerer, straight, &writable, &mut Vec::new()));
      run?;
      let value = block_to_expr_inner(lowerer, rest, floor)?;
      Ok(sequenced(held, value))
    }
  }
}

/// Whether a statement's expression writes a cell or calls something that may, which it must do in order where the lowerer holds cells.
fn writes_a_cell(lowerer: &Lowerer<'_>, expr: &js::Expression) -> bool {
  if lowerer.cells.is_empty() {
    return false;
  }
  let held = |name: &str| lowerer.cells.iter().any(|(cell, _)| cell == name);
  match expr {
    js::Expression::AssignmentExpression(assign) => lowerer.cell_of_target(&assign.left).is_some(),
    js::Expression::UpdateExpression(update) => matches!(&update.argument, js::SimpleAssignmentTarget::AssignmentTargetIdentifier(id) if held(id.name.as_str())),
    js::Expression::CallExpression(_) => true,
    _ => false,
  }
}

/// Whether evaluating `expr` does something besides answer: it emits, issues an act, writes a cell or the store, writes the session or calls an extension that reaches the process or runs later.
fn effectful(expr: &Expr) -> bool {
  let mut found = false;
  expr.visit(&mut |e| {
    found |= match e {
      Expr::Builtin { name: Builtin::Emit, .. } | Expr::WriteCell(..) | Expr::WriteStore(..) => true,
      Expr::Ext { module, name, .. } => matches!(snapfire_fsr_ir::standard_reach(module, name), Some(Reach::Process | Reach::Later)),
      Expr::Block(body) => acts(body),
      _ => false,
    };
  });
  found
}

/// Whether `body` holds an act or a session write at any depth of its own statements.
fn acts(body: &[Stmt]) -> bool {
  body.iter().any(|stmt| match stmt {
    Stmt::Act { .. } | Stmt::SessionSet { .. } | Stmt::SessionDelete { .. } | Stmt::SessionExtend { .. } => true,
    Stmt::If { then, r#else, .. } => acts(then) || acts(r#else),
    Stmt::ForOf { body, .. } => acts(body),
    Stmt::Try { body, handler, finally, .. } => acts(body) || handler.as_deref().is_some_and(acts) || acts(finally),
    _ => false,
  })
}

/// Whether `stmt` holds a call that issues an act, [`Lowerer::issues_act`], outside a function of its own.
fn holds_acts(lowerer: &Lowerer<'_>, stmt: &js::Statement) -> bool {
  struct Acts<'l, 'p>(&'l Lowerer<'p>, bool);
  impl<'a> oxc_ast_visit::Visit<'a> for Acts<'_, '_> {
    fn visit_call_expression(&mut self, call: &js::CallExpression<'a>) {
      self.1 |= self.0.issues_act(call);
      oxc_ast_visit::walk::walk_call_expression(self, call);
    }
    fn visit_arrow_function_expression(&mut self, _: &js::ArrowFunctionExpression<'a>) {}
    fn visit_function(&mut self, _: &js::Function<'a>, _: oxc_syntax::scope::ScopeFlags) {}
  }
  let mut found = Acts(lowerer, false);
  stmt.walk(&mut found);
  found.1
}

/// Whether a declaration's value must be computed once, where it is declared, rather than wherever it is read: it touches a cell, it issues an act or, in a terminal page, it calls a server-only extension such as the randomness a promise's id is drawn from.
fn computed_once(lowerer: &Lowerer<'_>, expr: &Expr) -> bool {
  let mut acts = false;
  expr.visit(&mut |e| {
    if let Expr::Block(body) = e {
      acts |= body.iter().any(|stmt| matches!(stmt, Stmt::Act { .. }));
    }
  });
  acts || touches_cells(expr) || (lowerer.reaches(Reach::Terminal) && lowerer.body_extension_in(expr).is_some())
}

/// Whether an expression reads or writes a cell or calls a function, which may: a value computed from one is taken where it is declared, not where it is used.
fn touches_cells(expr: &Expr) -> bool {
  let mut found = false;
  expr.visit(&mut |e| found |= matches!(e, Expr::ReadCell(_) | Expr::WriteCell(..) | Expr::Apply { .. }));
  found
}

/// An object or array literal holding only literals and other such literals: data a body can compare and write by identity, so every read of a module constant holding it must reach the one object. A `$` field is a record the lowerer reads in place, a class or a marker, and is not data.
fn data_only(expr: &Expr) -> bool {
  match expr {
    Expr::Object(entries) | Expr::Array(entries) => entries.iter().all(|entry| match entry {
      Entry::Field(name, _) if name.starts_with('$') => false,
      Entry::Field(_, value) | Entry::Item(value) => matches!(value, Expr::Lit(_)) || data_only(value),
      _ => false,
    }),
    _ => false,
  }
}

pub(crate) fn plain(expr: &Expr) -> bool {
  match expr {
    Expr::Object(entries) | Expr::Array(entries) if !entries.iter().any(|entry| matches!(entry, Entry::Field(_, Expr::Lambda { .. } | Expr::Fix { .. }) | Entry::Item(Expr::Lambda { .. } | Expr::Fix { .. }))) => false,
    Expr::Lit(_) | Expr::Var(_) | Expr::Const(_) | Expr::Lambda { .. } | Expr::Fix { .. } | Expr::Handler(_) | Expr::HoistId(_) | Expr::Param(_) | Expr::Query(_) | Expr::ParamsAll | Expr::QueryAll | Expr::Session(_) | Expr::Store(_) | Expr::Context(_) | Expr::Identity(_) | Expr::Locale | Expr::Locales | Expr::Path | Expr::Host | Expr::Origin | Expr::Address | Expr::Config(_) | Expr::Input | Expr::Document => true,
    Expr::Field(e, _) => plain(e),
    Expr::Index(a, b) => plain(a) && plain(b),
    Expr::Object(entries) | Expr::Array(entries) => entries.iter().all(|entry| match entry {
      Entry::Field(_, e) | Entry::Item(e) => plain(e),
      Entry::Baked(_) => true,
      _ => false,
    }),
    _ => false,
  }
}

pub(crate) fn settle(lowerer: &mut Lowerer<'_>, from: usize) {
  for at in from..lowerer.scope.len() {
    let (name, value) = lowerer.scope[at].clone();
    if name.starts_with('$') || plain(&value) || lowerer.cells.iter().any(|(cell, _)| *cell == name) {
      continue;
    }
    let kind = lowerer.kinds.iter().rev().find(|(n, held, _)| *n == name && *held == value).map(|(_, _, kind)| *kind);
    let held = hold_in_reach(lowerer, value);
    lowerer.scope[at].1 = held.clone();
    if let Some(kind) = kind {
      lowerer.kinds.push((name, held, kind));
    }
  }
}

fn hold_in_reach(lowerer: &mut Lowerer<'_>, expr: Expr) -> Expr {
  let base = *lowerer.held_reach.last().expect("a frame");
  let reach = lowerer.rebind_reach[base..].iter().cloned().reduce(|a, b| Expr::Logic(LogicOp::And, Box::new(a), Box::new(b)));
  let ran = match reach {
    Some(reach) => Expr::Ternary(Box::new(reach), Box::new(expr), Box::new(Expr::Lit(Lit::Null))),
    None => expr,
  };
  let held = lowerer.temp();
  lowerer.held.last_mut().expect("a frame").push(Stmt::Let { name: held.clone(), expr: ran });
  Expr::Var(held)
}

/// `lets` run in order before `value` is computed.
pub(crate) fn sequenced(mut lets: Vec<Stmt>, value: Expr) -> Expr {
  if let Expr::Block(stmts) = &value {
    if !lets.is_empty() {
      lets.extend(stmts.iter().cloned());
      return Expr::Block(lets);
    }
  }
  if lets.is_empty() {
    return value;
  }
  lets.push(Stmt::Return(value));
  Expr::Block(lets)
}

/// The method of `console.log(...)`, `info`, `debug`, `warn` or `error` on the global `console`; `None` for any other call.
fn console_method(lowerer: &Lowerer<'_>, call: &js::CallExpression) -> Option<String> {
  let callee = &call.callee;
  let Some(m) = (crate::meaning::bare(callee)).as_member_expression() else { return None };
  let js::Expression::Identifier(console) = crate::meaning::bare(m.object()) else { return None };
  let global = lowerer.meanings.as_ref().is_none_or(|meanings| meanings.ctxt(console) == crate::Ctxt::Unresolved);
  let method = prop_name_of(m).unwrap_or_default();
  (global && console.name.as_str() == "console" && matches!(method.as_str(), "log" | "info" | "debug" | "warn" | "error")).then_some(method)
}

/// `console.log(...)`, `info`, `debug`, `warn` or `error` on the global `console` as the log it writes on the server, as a server renderer running the component writes it, or as nothing when an argument is one only the browser can compute.
pub(crate) fn console_call(lowerer: &mut Lowerer<'_>, call: &js::CallExpression) -> Lowered<Option<Expr>> {
  let Some(method) = console_method(lowerer, call) else { return Ok(None) };
  let spread = call.arguments.iter().any(|arg| crate::syntax::is_spread(arg));
  let mut values = Vec::with_capacity(call.arguments.len());
  for arg in &call.arguments {
    match lowerer.expr(crate::syntax::arg_expr(arg)) {
      Ok(value) => values.push((crate::syntax::is_spread(arg), value)),
      // An argument the server cannot compute, the viewer's clock or zone among them, leaves the call to the browser, which logs it when the component hydrates; the markup is the same either way.
      Err(residue) => {
        let followable = lowerer.unbound.as_ref().is_some_and(|name| !lowerer.unfollowed.contains(name) && matches!(lowerer.meanings.as_ref().and_then(|m| m.bindings.get(name)), Some(crate::meaning::Meaning::Own { .. })));
        if followable || lowerer.reach_violation {
          return Err(residue);
        }
        lowerer.unbound = None;
        return Ok(Some(Expr::Lit(Lit::Null)));
      }
    }
  }
  if spread {
    let entries = values.into_iter().map(|(spread, value)| if spread { Entry::Spread(value) } else { Entry::Item(value) }).collect();
    return Ok(Some(Expr::Builtin { name: Builtin::LogOf, args: vec![Expr::lit_str(method), Expr::Array(entries)] }));
  }
  let mut args = vec![Expr::lit_str(method)];
  args.extend(values.into_iter().map(|(_, value)| value));
  Ok(Some(Expr::Builtin { name: Builtin::Log, args }))
}

/// `xs.forEach((x, i) => body)` as `for (let i = 0; i < xs.length; i++) { const x = xs[i]; body }`, a bare `return` in the callback a `continue`; `xs` a name or a chain of fields read off one, so reading it each pass reads the same array.
fn for_each_loop<'a>(b: crate::syntax::Build<'a>, call: &js::CallExpression, meanings: Option<&crate::meaning::Meanings>) -> Option<js::Statement<'a>> {
  let js::Expression::StaticMemberExpression(m) = crate::meaning::bare(&call.callee) else { return None };
  if m.property.name != "forEach" {
    return None;
  }
  fn steady(expr: &js::Expression) -> bool {
    match crate::meaning::bare(expr) {
      js::Expression::Identifier(_) => true,
      js::Expression::StaticMemberExpression(m) => steady(&m.object),
      _ => false,
    }
  }
  let span = call.span;
  // `Children.forEach(children, f)` walks the values the placement passed, which `ComponentLowerer::premake_for_each` binds.
  let children_of = meanings.and_then(|meanings| meanings.of_expr(&m.object)) == Some(crate::meaning::Meaning::Library(crate::meaning::Construct::Children)) && call.arguments.len() == 2;
  let (over, callback) = if children_of {
    (b.ident(snapfire_fsr_ir::render::CHILD_VALUES, span), &call.arguments[1])
  } else if call.arguments.len() == 1 && steady(&m.object) {
    (b.copy(&m.object), &call.arguments[0])
  } else {
    return None;
  };
  let (params, stmts): (Vec<crate::Param>, Vec<js::Statement<'a>>) = match crate::meaning::bare(crate::syntax::arg_expr(callback)) {
    js::Expression::ArrowFunctionExpression(arrow) => (
      crate::params_of(&arrow.params),
      match crate::syntax::arrow_body_of(arrow) {
        crate::syntax::ArrowBody::Block(body) => body.body.iter().map(|s| b.copy(s)).collect(),
        crate::syntax::ArrowBody::Expr(e) => vec![b.expr_stmt(b.copy(e))],
      },
    ),
    js::Expression::FunctionExpression(f) => (crate::params_of(&f.params), f.body.as_ref()?.statements.iter().map(|s| b.copy(s)).collect()),
    _ => return None,
  };
  let counter = match params.get(1) {
    Some(param) => param.ident()?.name.to_string(),
    None => format!("$each{}", crate::lo(span)),
  };
  fn continued<'a>(b: crate::syntax::Build<'a>, stmts: Vec<js::Statement<'a>>) -> Vec<js::Statement<'a>> {
    stmts
      .into_iter()
      .map(|stmt| match stmt {
        js::Statement::ReturnStatement(ret) if ret.argument.is_none() => js::Statement::new_continue_statement(ret.span, None, &b.ast()),
        js::Statement::IfStatement(branch) => {
          let branch = branch.unbox();
          let arm = |s: js::Statement<'a>| match s {
            js::Statement::BlockStatement(block) => {
              let block = block.unbox();
              b.block(continued(b, block.body.into_iter().collect()), block.span)
            }
            other => continued(b, vec![other]).remove(0),
          };
          js::Statement::new_if_statement(branch.span, branch.test, arm(branch.consequent), branch.alternate.map(arm), &b.ast())
        }
        js::Statement::BlockStatement(block) => {
          let span = block.span;
          b.block(continued(b, block.unbox().body.into_iter().collect()), span)
        }
        other => other,
      })
      .collect()
  }
  let mut body = Vec::new();
  if let Some(item) = params.first() {
    if item.rest {
      return None;
    }
    let read = js::Expression::new_computed_member_expression(span, b.copy(&over), b.ident(&counter, span), false, &b.ast());
    let pattern = match item.default {
      Some(default) => js::BindingPattern::new_assignment_pattern(span, b.copy(item.pattern), b.copy(default), &b.ast()),
      None => b.copy(item.pattern),
    };
    let declarator = js::VariableDeclarator::new(span, pattern, None, Some(read), false, &b.ast());
    body.push(js::Statement::new_variable_declaration(span, js::VariableDeclarationKind::Const, b.vec([declarator]), false, &b.ast()));
  }
  body.extend(continued(b, stmts));
  let length = js::Expression::new_static_member_expression(span, b.copy(&over), js::IdentifierName::new(span, b.str("length"), &b.ast()), false, &b.ast());
  let start = js::VariableDeclarator::new(span, b.binding(&counter, span), None, Some(js::Expression::new_numeric_literal(span, 0.0, None, js::NumberBase::Decimal, &b.ast())), false, &b.ast());
  let init = js::ForStatementInit::new_variable_declaration(span, js::VariableDeclarationKind::Let, b.vec([start]), false, &b.ast());
  let test = js::Expression::new_binary_expression(span, b.ident(&counter, span), js::BinaryOperator::LessThan, length, &b.ast());
  let counted = js::SimpleAssignmentTarget::new_assignment_target_identifier(span, b.str(&counter), &b.ast());
  let update = js::Expression::new_update_expression(span, js::UpdateOperator::Increment, false, counted, &b.ast());
  Some(js::Statement::new_for_statement(span, Some(init), Some(test), Some(update), b.block(body, span), &b.ast()))
}

/// Whether a loop's body returns from the function, outside a nested loop or function.
fn returns_from_loop(stmt: &js::Statement) -> bool {
  let body = match stmt {
    js::Statement::ForStatement(f) => &f.body,
    js::Statement::ForOfStatement(f) => &f.body,
    js::Statement::WhileStatement(w) => &w.body,
    _ => return false,
  };
  match &*body {
    js::Statement::BlockStatement(block) => block.body.iter().any(holds_return),
    other => holds_return(other),
  }
}

/// A loop that returns or throws as one that does not: `returned`, `value` and `thrown` declared before it, each `return e` in its body a write of `value` and `returned`, each `throw e` a write of `thrown` and `returned`, the rest of an iteration run only while nothing returned, then `if (thrown) throw thrown; if (returned) return value;` ahead of `rest`.
fn loop_return<'a>(b: crate::syntax::Build<'a>, looped: &js::Statement, rest: &[&js::Statement]) -> Vec<js::Statement<'a>> {
  let span = looped.span();
  let returned = format!("$returned{}", crate::lo(span));
  let value = format!("$value{}", crate::lo(span));
  let thrown = format!("$thrown{}", crate::lo(span));
  let ast = b.ast();
  let read = |name: &str| b.ident(name, span);
  let declare = |name: &str, init: js::Expression<'a>| {
    let declarator = js::VariableDeclarator::new(span, b.binding(name, span), None, Some(init), false, &ast);
    js::Statement::new_variable_declaration(span, js::VariableDeclarationKind::Let, b.vec([declarator]), false, &ast)
  };
  let set = |name: &str, to: js::Expression<'a>| {
    let target = js::AssignmentTarget::from(js::SimpleAssignmentTarget::new_assignment_target_identifier(span, b.str(name), &ast));
    b.expr_stmt(js::Expression::new_assignment_expression(span, js::AssignmentOperator::Assign, target, to, &ast))
  };
  let not_returned = || js::Expression::new_unary_expression(span, js::UnaryOperator::LogicalNot, read(&returned), &ast);
  fn rewrite<'a>(b: crate::syntax::Build<'a>, stmts: &[js::Statement], on_return: &dyn Fn(Option<js::Expression<'a>>) -> Vec<js::Statement<'a>>, on_throw: &dyn Fn(js::Expression<'a>) -> Vec<js::Statement<'a>>, unless: &dyn Fn(Vec<js::Statement<'a>>) -> js::Statement<'a>) -> Vec<js::Statement<'a>> {
    let mut out = Vec::new();
    for (at, stmt) in stmts.iter().enumerate() {
      match stmt {
        js::Statement::ReturnStatement(ret) => {
          out.extend(on_return(ret.argument.as_ref().map(|a| b.copy(a))));
          return out;
        }
        js::Statement::ThrowStatement(t) => {
          out.extend(on_throw(b.copy(&t.argument)));
          return out;
        }
        js::Statement::IfStatement(branch) if holds_return(stmt) => {
          let arm = |s: &js::Statement| -> js::Statement<'a> {
            let stmts = match s {
              js::Statement::BlockStatement(block) => block.body.as_slice(),
              other => std::slice::from_ref(other),
            };
            b.block(rewrite(b, stmts, on_return, on_throw, unless), s.span())
          };
          out.push(js::Statement::new_if_statement(branch.span, b.copy(&branch.test), arm(&branch.consequent), branch.alternate.as_ref().map(|a| arm(a)), &b.ast()));
        }
        js::Statement::BlockStatement(block) if holds_return(stmt) => out.push(b.block(rewrite(b, &block.body, on_return, on_throw, unless), block.span)),
        other => {
          out.push(b.copy(other));
          continue;
        }
      }
      let after = rewrite(b, &stmts[at + 1..], on_return, on_throw, unless);
      if !after.is_empty() {
        out.push(unless(after));
      }
      return out;
    }
    out
  }
  let on_return = |arg: Option<js::Expression<'a>>| vec![set(&value, arg.unwrap_or_else(|| read("undefined"))), set(&returned, b.boolean(true, span))];
  let on_throw = |arg: js::Expression<'a>| vec![set(&thrown, arg), set(&returned, b.boolean(true, span))];
  let unless = |stmts: Vec<js::Statement<'a>>| js::Statement::new_if_statement(span, not_returned(), b.block(stmts, span), None, &ast);
  let guard = |body: &js::Statement| -> js::Statement<'a> {
    let stmts = match body {
      js::Statement::BlockStatement(block) => block.body.as_slice(),
      other => std::slice::from_ref(other),
    };
    b.block(vec![unless(rewrite(b, stmts, &on_return, &on_throw, &unless))], span)
  };
  let looped = match looped {
    js::Statement::ForStatement(f) => {
      let mut copy = b.copy(&**f);
      copy.body = guard(&f.body);
      js::Statement::ForStatement(b.boxed(copy))
    }
    js::Statement::ForOfStatement(f) => {
      let mut copy = b.copy(&**f);
      copy.body = guard(&f.body);
      js::Statement::ForOfStatement(b.boxed(copy))
    }
    js::Statement::WhileStatement(w) => {
      let mut copy = b.copy(&**w);
      copy.test = js::Expression::new_logical_expression(span, not_returned(), js::LogicalOperator::And, b.copy(&w.test), &ast);
      copy.body = guard(&w.body);
      js::Statement::WhileStatement(b.boxed(copy))
    }
    other => b.copy(other),
  };
  let mut out = vec![declare(&returned, b.boolean(false, span)), declare(&value, read("undefined")), declare(&thrown, read("undefined")), looped];
  let threw = b.binary(read(&thrown), js::BinaryOperator::StrictInequality, read("undefined"), span);
  out.push(js::Statement::new_if_statement(span, threw, js::Statement::new_throw_statement(span, read(&thrown), &ast), None, &ast));
  out.push(js::Statement::new_if_statement(span, read(&returned), js::Statement::new_return_statement(span, Some(read(&value)), &ast), None, &ast));
  out.extend(rest.iter().map(|s| b.copy(*s)));
  out
}

/// The `return` an `if` with no `else` holds as its only statement, braced or bare.
fn early_return<'e>(branch: &'e js::IfStatement<'e>) -> Option<&'e js::ReturnStatement<'e>> {
  if branch.alternate.is_some() {
    return None;
  }
  match &branch.consequent {
    js::Statement::ReturnStatement(ret) => Some(ret),
    js::Statement::BlockStatement(block) => match block.body.as_slice() {
      [js::Statement::ReturnStatement(ret)] => Some(ret),
      _ => None,
    },
    _ => None,
  }
}

/// The `return` or `throw` an `if` with no `else` ends in when it holds nothing else: a hook's early exit.
fn early_exit<'e>(branch: &'e js::IfStatement<'e>) -> Option<&'e js::Statement<'e>> {
  if branch.alternate.is_some() {
    return None;
  }
  let only = match &branch.consequent {
    js::Statement::BlockStatement(block) => match block.body.as_slice() {
      [only] => only,
      _ => return None,
    },
    other => other,
  };
  matches!(only, js::Statement::ReturnStatement(_) | js::Statement::ThrowStatement(_)).then_some(only)
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

impl<'a, 'p> ComponentLowerer<'a, 'p> {
  /// What the file's module-level names mean.
  fn meanings(&self) -> Rc<crate::meaning::Meanings> {
    self.lowerer.meanings.clone().unwrap_or_default()
  }

  /// Whether a `use` call names one of the application's own hooks rather than React's or FSR client's.
  /// Whether `call`'s callee is an import of a package the build does not follow.
  fn foreign_callee(&self, call: &js::CallExpression) -> bool {
    let callee = &call.callee;
    matches!(self.meanings().of_expr(callee), Some(crate::meaning::Meaning::Foreign))
  }

  fn custom_hook(&self, name: &str) -> bool {
    use crate::meaning::{Construct, Meaning, LIBRARY};
    match self.meanings().bindings.get(name) {
      Some(Meaning::Own { .. } | Meaning::Foreign) => true,
      Some(_) => false,
      None => !LIBRARY.iter().any(|(_, _, construct)| matches!(construct, Construct::Hook(hook) if *hook == name)),
    }
  }

  /// A custom hook's call inlined: its body binds under a prefix of its own so two calls hold two sets of state. What it returns binds at the call site. A returned setter or function is a handler there, run by the browser.
  fn inline_hook(&mut self, hook: &str, call: &'p js::CallExpression<'p>, pattern: Option<&'p js::BindingPattern<'p>>) -> Lowered<Option<Stmt>> {
    let Some((_, parsed, export)) = self.hooks.iter().find(|(n, _, _)| n == hook).cloned() else {
      let origin = self.meanings().of_expr(&call.callee).and_then(|meant| meant.origin().map(|(f, n)| (f.to_owned(), n.to_owned())));
      self.wanted_hook = Some((hook.to_owned(), origin));
      return Err(self.lowerer.residue(call.span, format!("`{hook}`, a hook the build cannot follow")));
    };
    let Some(found) = find_function(parsed, &export) else { return Err(self.lowerer.residue(call.span, format!("`{hook}` is not a function the build can read"))) };
    let (params, body): (Vec<crate::Param<'p>>, FunctionBody<'p>) = match found {
      Found::Declared(params, stmts, _) => (crate::params_of(params), FunctionBody::Block(stmts)),
      Found::Arrow(arrow) => (crate::params_of(&arrow.params), arrow_body(arrow)),
    };
    let mut args = Vec::with_capacity(call.arguments.len());
    for arg in &call.arguments {
      match self.lowerer.expr(crate::syntax::arg_expr(arg)) {
        Ok(value) => args.push(value),
        // A callback that writes state or calls out is one only the browser runs, as an effect or a listener does.
        Err(_) if matches!(crate::meaning::bare(crate::syntax::arg_expr(arg)), js::Expression::ArrowFunctionExpression(_) | js::Expression::FunctionExpression(_)) && !self.unbound_is_followable() => {
          self.lowerer.unbound = None;
          args.push(Expr::Handler(snapfire_fsr_ir::render::BROWSER_HANDLER));
        }
        Err(residue) => return Err(residue),
      }
    }
    self.hook_calls += 1;
    let prefix = format!("{hook}${}$", self.hook_calls);
    let outer = self.lowerer.parsed;
    let hoisting = self.lowerer.hoisting.take();
    let depth = self.lowerer.scope.len();
    let declared = self.lowerer.declared_below.len();
    self.lowerer.parsed = parsed;
    let outer_meanings = std::mem::replace(&mut self.lowerer.meanings, parsed.meanings.get().cloned());
    let fns: std::collections::HashSet<String> = self.handler_fns.keys().cloned().collect();
    let mut renames = Vec::new();
    let returned = self.hook_body(&prefix, &params, body, args, &mut renames);
    let scope = Rc::new(HookScope { parsed, meanings: self.lowerer.meanings.clone(), scope: self.lowerer.scope[depth..].to_vec(), declared_below: self.lowerer.declared_below[declared..].to_vec(), renames });
    for (name, f) in self.handler_fns.iter_mut() {
      if f.hook.is_none() && !fns.contains(name) {
        f.hook = Some(scope.clone());
      }
    }
    self.lowerer.parsed = outer;
    self.lowerer.meanings = outer_meanings;
    self.lowerer.hoisting = hoisting;
    self.lowerer.scope.truncate(depth);
    self.lowerer.declared_below.truncate(declared);
    let returned = match returned {
      Ok(returned) => returned,
      Err(residue) => {
        if self.lowerer.unbound.is_some() && self.unbound_in.is_none() {
          self.unbound_in = Some(parsed.file.clone());
        }
        return Err(residue);
      }
    };
    if let Some(pattern) = pattern {
      self.bind_returned(pattern, returned, &prefix)?;
    }
    Ok(None)
  }

  fn hook_body(&mut self, prefix: &str, params: &[crate::Param], body: FunctionBody<'p>, args: Vec<Expr>, renames: &mut Vec<(String, String)>) -> Lowered<Returned> {
    for (i, param) in params.iter().enumerate() {
      let mut arg = args.get(i).cloned().unwrap_or(Expr::Lit(Lit::Null));
      if !plain(&arg) {
        let held = self.lowerer.temp();
        self.pending_lets.push(Stmt::Let { name: held.clone(), expr: arg });
        arg = Expr::Var(held);
      }
      bind_param(&mut self.lowerer, param, arg)?;
    }
    let stmts = match body {
      FunctionBody::Expr(e) => return self.returned(e, &[]),
      FunctionBody::Block(stmts) => stmts,
    };
    let floor = self.lowerer.scope.len();
    let mut early: Vec<(Expr, Expr)> = Vec::new();
    let guard = |early: &[(Expr, Expr)], expr: Expr| match early.iter().map(|(cond, _)| cond.clone()).reduce(|a, b| Expr::Logic(LogicOp::Or, Box::new(a), Box::new(b))) {
      None => expr,
      Some(fired) => Expr::Ternary(Box::new(fired), Box::new(Expr::Lit(Lit::Null)), Box::new(expr)),
    };
    // A function declaration is hoisted: a handler above it calls it by name.
    for stmt in stmts {
      if let js::Statement::FunctionDeclaration(f) = stmt {
        let Some(id) = &f.id else { continue };
        let bound = format!("{prefix}{}", id.name);
        if let Some(body) = &f.body {
          self.handler_fns.insert(bound.clone(), (crate::params_of(&f.params), FunctionBody::Block(&body.statements)).into());
          self.lowerer.local_fns.push((id.name.to_string(), &**f));
        }
        self.lowerer.browser_functions.push(id.name.to_string());
        self.browser_function(bound.clone());
        renames.push((id.name.to_string(), bound));
      }
    }
    renames.extend(self.declare_below(stmts, prefix).into_iter().filter(|(from, to)| from != to));
    for stmt in stmts {
      match stmt {
        js::Statement::VariableDeclaration(var) => {
          for decl in &var.declarations {
            let from = self.pending_lets.len();
            self.hook_let(decl, prefix, renames)?;
            for pending in &mut self.pending_lets[from..] {
              if let Stmt::Let { expr, .. } = pending {
                *expr = guard(&early, std::mem::replace(expr, Expr::Lit(Lit::Null)));
              }
            }
          }
        }
        js::Statement::FunctionDeclaration(_) => {}
        js::Statement::ExpressionStatement(e) if library_hook(&self.meanings(), &e.expression).is_some_and(|(name, _)| name == "useInput") => {
          let (_, call) = library_hook(&self.meanings(), &e.expression).expect("checked");
          let depth = self.hook_renames.len();
          self.hook_renames.extend(renames.iter().cloned());
          let result = self.input_hook(call);
          self.hook_renames.truncate(depth);
          result?;
        }
        js::Statement::ExpressionStatement(e) if hook_call(&self.meanings(), &e.expression).is_some_and(|(name, _)| EFFECT_HOOKS.contains(&name)) => {
          let (hook, call) = hook_call(&self.meanings(), &e.expression).expect("checked");
          if hook == "useEffect" || hook == "useLayoutEffect" {
            let depth = self.hook_renames.len();
            self.hook_renames.extend(renames.iter().cloned());
            let result = self.effect_timers(call);
            self.hook_renames.truncate(depth);
            result?;
          }
        }
        js::Statement::ExpressionStatement(e) if hook_call(&self.meanings(), &e.expression).is_some_and(|(name, _)| self.custom_hook(name)) => {
          let (hook, call) = hook_call(&self.meanings(), &e.expression).expect("checked");
          self.inline_hook(hook, call, None)?;
        }
        js::Statement::ReturnStatement(ret) => {
          let returned = match ret.argument.as_ref() {
            Some(arg) => self.returned(arg, renames)?,
            None => Returned::Value(Expr::Lit(Lit::Null)),
          };
          return self.after_early(early, returned, ret.span);
        }
        js::Statement::IfStatement(branch) if early_exit(branch).is_some() => {
          let cond = guard(&early, self.lowerer.expr(&branch.test)?);
          let then = match early_exit(branch).expect("checked") {
            js::Statement::ThrowStatement(thrown) => throw_expr(&mut self.lowerer, thrown)?,
            js::Statement::ReturnStatement(ret) => match ret.argument.as_ref() {
              None => Expr::Lit(Lit::Null),
              Some(arg) => match self.returned(arg, renames)? {
                Returned::Value(value) => value,
                _ => return Err(self.lowerer.residue(ret.span, "an early `return` of handlers from a custom hook")),
              },
            },
            _ => unreachable!("an early exit is a `return` or a `throw`"),
          };
          early.push((cond, then));
        }
        js::Statement::IfStatement(_) | js::Statement::SwitchStatement(_) | js::Statement::BlockStatement(_) if holds_return(stmt) => {
          return Err(self.lowerer.residue(stmt.span(), "a `return` inside a branch of a custom hook"));
        }
        js::Statement::ExpressionStatement(_) | js::Statement::IfStatement(_) | js::Statement::SwitchStatement(_) | js::Statement::BlockStatement(_) => {
          let mut held = std::mem::take(&mut self.pending_lets);
          let result = self.rebound(stmt, floor, &[], &mut held, &|_, expr| guard(&early, expr));
          self.pending_lets = held;
          result?;
        }
        other => return Err(self.lowerer.residue(other.span(), "a statement a custom hook cannot hold; it holds hooks, `const`s, `let`s, functions and a `return`")),
      }
    }
    self.after_early(early, Returned::Value(Expr::Lit(Lit::Null)), Span::default())
  }

  /// Declares each `const` and `let` of `stmts` for what runs after the body, as JavaScript binds them from the start of it: a value under the name it binds, `prefix` and its own in a custom hook, and a function as a handler function. Of a hook's result, `useState` and `useStore` declare their value and their setter, `useRef` its ref, `useMemo` its value and `useCallback` its function; any other hook's is declared where it is, since what it binds is known once the hook lowers. Returns each name with the one it binds.
  fn declare_below(&mut self, stmts: &'p [js::Statement], prefix: &str) -> Vec<(String, String)> {
    let mut bound = Vec::new();
    let value = |this: &mut Self, bound: &mut Vec<(String, String)>, name: &str| {
      let to = format!("{prefix}{name}");
      this.lowerer.declared_below.push((name.to_owned(), to.clone()));
      bound.push((name.to_owned(), to));
    };
    for stmt in stmts {
      let js::Statement::VariableDeclaration(var) = stmt else { continue };
      for decl in &var.declarations {
        let init = decl.init.as_ref().map(crate::meaning::bare);
        if let Some((hook, call)) = init.and_then(|init| hook_call(&self.meanings(), init)) {
          match (hook, &decl.id) {
            ("useState" | "useStore", js::BindingPattern::ArrayPattern(arr)) => {
              let mut names = arr.elements.iter().map(|e| match e {
                Some(js::BindingPattern::BindingIdentifier(id)) => Some(id.name.to_string()),
                _ => None,
              });
              let (Some(Some(state)), setter) = (names.next(), names.next().flatten()) else { continue };
              value(self, &mut bound, &state);
              if let Some(setter) = setter {
                bound.push((setter.clone(), format!("{prefix}{setter}")));
                if !self.setters.iter().any(|(s, _)| *s == format!("{prefix}{setter}")) {
                  self.setters.push((format!("{prefix}{setter}"), format!("{prefix}{state}")));
                }
              }
            }
            ("useRef", js::BindingPattern::BindingIdentifier(id)) => {
              value(self, &mut bound, &id.name);
              if self.lowerer.reaches(Reach::Terminal) && !self.held_refs.contains(&format!("{prefix}{}", id.name)) {
                self.held_refs.push(format!("{prefix}{}", id.name));
              }
            }
            ("useMemo", js::BindingPattern::BindingIdentifier(id)) => value(self, &mut bound, &id.name),
            ("useCallback", js::BindingPattern::BindingIdentifier(id)) => {
              if let Some(js::Expression::ArrowFunctionExpression(arrow)) = call.arguments.first().map(|a| crate::meaning::bare(crate::syntax::arg_expr(a))) {
                self.handler_fns.entry(format!("{prefix}{}", id.name)).or_insert((crate::params_of(&arrow.params), arrow_body(arrow)).into());
                bound.push((id.name.to_string(), format!("{prefix}{}", id.name)));
              }
            }
            _ => {}
          }
          continue;
        }
        match (&decl.id, init) {
          (js::BindingPattern::BindingIdentifier(id), Some(js::Expression::ArrowFunctionExpression(arrow))) => {
            self.handler_fns.entry(format!("{prefix}{}", id.name)).or_insert((crate::params_of(&arrow.params), arrow_body(arrow)).into());
          }
          (js::BindingPattern::BindingIdentifier(id), Some(js::Expression::FunctionExpression(f))) => {
            if let Some(body) = &f.body {
              self.handler_fns.entry(format!("{prefix}{}", id.name)).or_insert((crate::params_of(&f.params), FunctionBody::Block(&body.statements)).into());
            }
          }
          (pattern, _) => {
            let mut names = Vec::new();
            crate::pattern_names(pattern, &mut names);
            for name in names {
              value(self, &mut bound, &name.name);
            }
          }
        }
      }
    }
    bound
  }

  /// A hook's return taken when none of its early exits fired, each `(cond, value)` checked in order before it.
  fn after_early(&mut self, early: Vec<(Expr, Expr)>, returned: Returned, span: Span) -> Lowered<Returned> {
    if early.is_empty() {
      return Ok(returned);
    }
    let Returned::Value(last) = returned else {
      return Err(self.lowerer.residue(span, "a custom hook that returns handlers after an early `return`"));
    };
    Ok(Returned::Value(early.into_iter().rev().fold(last, |r#else, (cond, then)| Expr::Ternary(Box::new(cond), Box::new(then), Box::new(r#else)))))
  }

  /// One declaration inside a hook: lowered as the component's are, then every name it bound renamed under the call's prefix.
  fn hook_let(&mut self, decl: &'p js::VariableDeclarator<'p>, prefix: &str, renames: &mut Vec<(String, String)>) -> Lowered<()> {
    let (state, bindings, stores, setters, handlers, scope) = (self.state.len(), self.state_bindings.len(), self.store_bindings.len(), self.setters.len(), self.handlers.len(), self.lowerer.scope.len());
    let fns: Vec<String> = self.handler_fns.keys().cloned().collect();
    let stmt = self.let_stmt(decl)?;
    let mut map: Vec<(String, String)> = Vec::new();
    let rename = |map: &mut Vec<(String, String)>, name: &str| -> String {
      if let Some((_, to)) = map.iter().find(|(from, _)| from == name) {
        return to.clone();
      }
      let to = format!("{prefix}{name}");
      map.push((name.to_owned(), to.clone()));
      to
    };
    for name in self.state[state..].iter_mut() {
      *name = rename(&mut map, name);
    }
    for name in self.state_bindings[bindings..].iter_mut() {
      *name = rename(&mut map, name);
    }
    for (name, _) in self.store_bindings[stores..].iter_mut() {
      *name = rename(&mut map, name);
    }
    for (setter, held) in self.setters[setters..].iter_mut() {
      *setter = rename(&mut map, setter);
      *held = rename(&mut map, held);
    }
    for name in self.handlers[handlers..].iter_mut() {
      *name = rename(&mut map, name);
    }
    let added: Vec<String> = self.handler_fns.keys().filter(|k| !fns.contains(k)).cloned().collect();
    for key in added {
      if let Some(f) = self.handler_fns.remove(&key) {
        let to = rename(&mut map, &key);
        self.handler_fns.insert(to, f);
      }
    }
    if let Some(Stmt::Let { name, expr }) = stmt {
      let to = if name.starts_with("$d") { name } else { rename(&mut map, &name) };
      self.pending_lets.push(Stmt::Let { name: to, expr });
    }
    for (_, bound) in self.lowerer.scope[scope..].iter_mut() {
      if let Expr::Var(var) = bound {
        if let Some((_, to)) = map.iter().find(|(from, _)| from == var) {
          *var = to.clone();
        }
      }
    }
    renames.extend(map);
    Ok(())
  }

  /// A hook's return as values and handlers, part by part where it is an object or array literal.
  fn returned(&mut self, e: &'p js::Expression<'p>, renames: &[(String, String)]) -> Lowered<Returned> {
    let handler = |this: &Self, e: &js::Expression| -> Option<String> {
      let js::Expression::Identifier(id) = e else { return None };
      let bound = renames.iter().rev().find(|(from, _)| from == id.name.as_str()).map(|(_, to)| to.clone())?;
      this.handlers.contains(&bound).then_some(bound)
    };
    let part = |this: &mut Self, e: &'p js::Expression<'p>| -> Lowered<Part> {
      if let js::Expression::ArrowFunctionExpression(arrow) = e {
        this.hook_calls += 1;
        let bound = format!("$hook{}", this.hook_calls);
        this.handler_fns.insert(bound.clone(), (crate::params_of(&arrow.params), arrow_body(arrow)).into());
        this.browser_function(bound.clone());
        return Ok(Part::Handler(bound));
      }
      Ok(match handler(this, e) {
        Some(bound) => Part::Handler(bound),
        None => Part::Value(this.lowerer.expr(e)?),
      })
    };
    match e {
      js::Expression::ParenthesizedExpression(p) => self.returned(&p.expression, renames),
      js::Expression::TSAsExpression(a) => self.returned(&a.expression, renames),
      js::Expression::TSSatisfiesExpression(a) => self.returned(&a.expression, renames),
      js::Expression::ObjectExpression(obj) => {
        let mut parts = Vec::with_capacity(obj.properties.len());
        for prop in &obj.properties {
          let js::ObjectPropertyKind::ObjectProperty(prop) = prop else { return Err(self.lowerer.residue(obj.span, "a spread in what a custom hook returns")) };
          match crate::syntax::prop_of(prop) {
            crate::syntax::Prop::Shorthand(id) => {
              let ident = self.lowerer.build().read(id);
              let p = match handler(self, &ident) {
                Some(bound) => Part::Handler(bound),
                None => Part::Value(self.lowerer.expr(&ident)?),
              };
              parts.push((id.name.to_string(), p));
            }
            crate::syntax::Prop::KeyValue(kv) => {
              let key = prop_name(&kv.key, kv.computed).ok_or_else(|| self.lowerer.residue(kv.key.span(), "a computed key in what a custom hook returns"))?;
              parts.push((key, part(self, &kv.value)?));
            }
            other => return Err(self.lowerer.residue(other.span(), "a method in what a custom hook returns")),
          }
        }
        Ok(Returned::Object(parts))
      }
      js::Expression::ArrayExpression(arr) => {
        let mut parts = Vec::with_capacity(arr.elements.len());
        for elem in &arr.elements {
          match elem {
            js::ArrayExpressionElement::Elision(_) => parts.push(None),
            js::ArrayExpressionElement::SpreadElement(s) => return Err(self.lowerer.residue(s.argument.span(), "a spread in what a custom hook returns")),
            other => parts.push(Some(part(self, other.to_expression())?)),
          }
        }
        Ok(Returned::Array(parts))
      }
      other => Ok(match handler(self, other) {
        Some(bound) => Returned::Handler(bound),
        None => Returned::Value(self.lowerer.expr(other)?),
      }),
    }
  }

  /// `local` at the call site is the handler `bound` inside the hook: the same function, the same state when it is a setter.
  fn alias_handler(&mut self, local: &str, bound: &str) {
    self.browser_function(local.to_owned());
    if let Some(f) = self.handler_fns.get(bound).cloned() {
      self.handler_fns.insert(local.to_owned(), f);
    }
    if let Some((_, state)) = self.setters.iter().find(|(s, _)| s == bound).cloned() {
      self.setters.push((local.to_owned(), state));
    }
  }

  fn bind_value(&mut self, pattern: &js::BindingPattern, value: Expr) -> Lowered<()> {
    if snapfire_fsr_ir::host::act_ref(&value).is_some() || crate::terminal::primitive_ref(&value).is_some() || (self.lowerer.reaches(Reach::Terminal) && matches!(value, Expr::Lambda { .. })) {
      return bind_pattern(&mut self.lowerer, pattern, value);
    }
    let name = self.lowerer.temp();
    self.pending_lets.push(Stmt::Let { name: name.clone(), expr: value });
    bind_pattern(&mut self.lowerer, pattern, Expr::Var(name))
  }

  fn bind_returned(&mut self, pattern: &'p js::BindingPattern<'p>, returned: Returned, prefix: &str) -> Lowered<()> {
    let part_into = |this: &mut Self, pattern: &js::BindingPattern, part: Option<Part>| -> Lowered<()> {
      match part {
        Some(Part::Handler(bound)) => match pattern {
          js::BindingPattern::BindingIdentifier(id) => {
            this.alias_handler(id.name.as_str(), &bound);
            Ok(())
          }
          other => Err(this.lowerer.residue(other.span(), "a function a custom hook returns bound to a pattern")),
        },
        Some(Part::Value(value)) => this.bind_value(pattern, value),
        None => this.bind_value(pattern, Expr::Lit(Lit::Null)),
      }
    };
    match (pattern, returned) {
      (js::BindingPattern::BindingIdentifier(id), Returned::Handler(bound)) => {
        self.alias_handler(id.name.as_str(), &bound);
        Ok(())
      }
      (js::BindingPattern::BindingIdentifier(id), Returned::Value(value)) => {
        let name = format!("{prefix}{}", id.name);
        self.pending_lets.push(Stmt::Let { name: name.clone(), expr: value });
        self.lowerer.scope.push((id.name.to_string(), Expr::Var(name)));
        Ok(())
      }
      (js::BindingPattern::ObjectPattern(obj), Returned::Object(mut parts)) => {
        for prop in crate::syntax::pat_props(obj) {
          match prop {
            crate::syntax::PatProp::Assign(a) => {
              let key = a.key.name.to_string();
              let part = parts.iter().position(|(k, _)| *k == key).map(|at| parts.remove(at).1);
              let b = self.lowerer.build();
              let target = js::BindingPattern::BindingIdentifier(b.boxed(b.copy(a.key)));
              match (part, &a.value) {
                (Some(Part::Value(value)), Some(default)) => {
                  let default = self.lowerer.expr(default)?;
                  self.bind_value(&target, Expr::Coalesce(Box::new(value), Box::new(default)))?;
                }
                (part, _) => part_into(self, &target, part)?,
              }
            }
            crate::syntax::PatProp::KeyValue(kv) => {
              let key = prop_name(&kv.key, kv.computed).ok_or_else(|| self.lowerer.residue(kv.key.span(), "a computed field in a pattern"))?;
              let part = parts.iter().position(|(k, _)| *k == key).map(|at| parts.remove(at).1);
              part_into(self, &kv.value, part)?;
            }
            crate::syntax::PatProp::Rest(r) => return Err(self.lowerer.residue(r.span, "a rest of what a custom hook returns")),
          }
        }
        Ok(())
      }
      (js::BindingPattern::ArrayPattern(arr), Returned::Array(mut parts)) => {
        if let Some(r) = &arr.rest {
          return Err(self.lowerer.residue(r.span, "a rest of what a custom hook returns"));
        }
        for (i, elem) in arr.elements.iter().enumerate() {
          let part = if i < parts.len() { parts[i].take() } else { None };
          if let Some(pattern) = elem {
            part_into(self, pattern, part)?;
          }
        }
        Ok(())
      }
      (pattern, Returned::Object(parts)) => {
        let mut entries = Vec::new();
        for (key, part) in parts {
          if let Part::Value(value) = part {
            entries.push(Entry::Field(key, value));
          }
        }
        self.bind_value(pattern, Expr::Object(entries))
      }
      (pattern, Returned::Array(parts)) => {
        let items = parts.into_iter().map(|p| Entry::Item(match p {
          Some(Part::Value(value)) => value,
          _ => Expr::Lit(Lit::Null),
        })).collect();
        self.bind_value(pattern, Expr::Array(items))
      }
      (pattern, Returned::Value(value)) => self.bind_value(pattern, value),
      (pattern, Returned::Handler(_)) => Err(self.lowerer.residue(pattern.span(), "a function a custom hook returns bound to a pattern")),
    }
  }
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

impl<'p> Placer<'p> for ComponentLowerer<'_, 'p> {
  fn lowerer(&mut self) -> &mut Lowerer<'p> {
    &mut self.lowerer
  }

  fn residue_at(&self, at: At, message: String) -> Residue {
    match at {
      At::Span(span) => self.lowerer.residue(span, message),
      At::Line(line, column) => Residue { file: self.lowerer.parsed.file.clone(), line, column, message, hint: None, via: Vec::new() },
    }
  }

  fn assets(&self) -> Rc<dyn AssetResolver> {
    self.assets.clone()
  }

  fn head(&mut self, row: HeadRow) {
    self.remember_head(row);
  }
}

impl ComponentLowerer<'_, '_> {
  fn slot(&self, name: &str) -> Tmpl {
    if self.layout_root {
      let attrs = if name == "content" { Vec::new() } else { vec![Entry::Field("data-sf-name".to_owned(), Expr::lit_str(name))] };
      Tmpl::Element { tag: "sf-s".to_owned(), attrs, children: vec![Tmpl::Slot(name.to_owned())] }
    } else {
      Tmpl::Slot(name.to_owned())
    }
  }

  /// A named slot with the markup to show while the plan leaves it unfilled:
  /// the region holds the segment when `$props.$slots` names the slot and
  /// the fallback otherwise.
  fn slot_with_fallback(&self, name: &str, fallback: Vec<Tmpl>) -> Tmpl {
    let filled = Expr::Builtin {
      name: Builtin::Includes,
      args: vec![Expr::Coalesce(Box::new(Expr::Var("$props".to_owned()).field("$slots")), Box::new(Expr::Array(Vec::new()))), Expr::lit_str(name)],
    };
    let inner = Tmpl::If { cond: filled, then: Box::new(Tmpl::Slot(name.to_owned())), r#else: Some(Box::new(Tmpl::Fragment(fallback))) };
    let attrs = if name == "content" { Vec::new() } else { vec![Entry::Field("data-sf-name".to_owned(), Expr::lit_str(name))] };
    Tmpl::Element { tag: "sf-s".to_owned(), attrs, children: vec![inner] }
  }

  /// True when `attrs` spread the props, or their rest, which carry the caller's `children`.
  fn spreads_props(&self, attrs: &[js::JSXAttributeItem]) -> bool {
    attrs.iter().any(|attr| match attr {
      js::JSXAttributeItem::SpreadAttribute(spread) => match &spread.argument {
        js::Expression::Identifier(id) => [&self.props_name, &self.props_rest].iter().any(|name| name.as_deref() == Some(id.name.as_str())),
        _ => false,
      },
      js::JSXAttributeItem::Attribute(_) => false,
    })
  }

  /// The slot an expression stands for: `children` or `props.children` is
  /// `content`; in a layout, a prop named after one of its slots is that slot.
  fn slot_of_expr(&self, expr: &js::Expression) -> Option<String> {
    match expr {
      js::Expression::Identifier(id) => {
        let name = id.name.as_str();
        if self.children_name.as_deref() == Some(name) {
          return Some("content".to_owned());
        }
        self.slot_props.iter().find(|(local, _)| local == name).map(|(_, slot)| slot.clone())
      }
      m_expr if m_expr.is_member_expression() => {
let m = m_expr.as_member_expression().expect("matched as a member");
        let crate::syntax::MemberProp::Ident(prop) = crate::syntax::prop(m) else { return None };
        if self.class.is_some() && prop.name.as_str() == "children" && is_this_props(m.object()) {
          return Some("content".to_owned());
        }
        let js::Expression::Identifier(obj) = m.object() else { return None };
        if self.props_name.as_deref() != Some(obj.name.as_str()) {
          return None;
        }
        let prop = prop.name.as_str();
        if prop == "children" {
          return Some("content".to_owned());
        }
        (self.layout_root && self.slot_names.iter().any(|n| n == prop)).then(|| prop.to_owned())
      }
      _ => None,
    }
  }
}

/// `this.properties`, which a class component's `render()` reads its props through.
fn is_this_props(expr: &js::Expression) -> bool {
  (expr).as_member_expression().is_some_and(|m| matches!(m.object(), js::Expression::ThisExpression(_)) && matches!(crate::syntax::prop(m), crate::syntax::MemberProp::Ident(p) if p.name.as_str() == "props"))
}

const EFFECT_HOOKS: &[&str] = &["useEffect", "useLayoutEffect", "useInsertionEffect", "useDebugValue", "useImperativeHandle"];

/// The hook a call names when its callee is a bare identifier.
/// The hook `expr` calls: a library's by the library's own name, however the
/// file reaches it, and an application's by the name the file calls it. `None`
/// for a call that is no hook, a local that shadows a hook's name among them.
/// A call to one of a library's own hooks, by what its callee means rather than its name: a hook an application or FSR's Ink module declares under the same name is not one.
fn library_hook<'a>(meanings: &crate::meaning::Meanings, expr: &'a js::Expression<'a>) -> Option<(&'static str, &'a js::CallExpression<'a>)> {
  let js::Expression::CallExpression(call) = expr else { return None };
  let callee = &call.callee;
  meanings.of_expr(callee).and_then(|meant| meant.hook()).map(|hook| (hook, &**call))
}

/// The hooks FSR's client library declares, which another package's hook of the same name is not.
const FSR_HOOKS: &[&str] = &["useStore", "useLocale", "useEffectEvent", "useAnimation"];

fn hook_call<'a>(meanings: &crate::meaning::Meanings, expr: &'a js::Expression<'a>) -> Option<(&'a str, &'a js::CallExpression<'a>)> {
  let js::Expression::CallExpression(call) = crate::meaning::bare(expr) else { return None };
  let callee = &call.callee;
  let meant = meanings.of_expr(callee);
  if let Some(hook) = meant.as_ref().and_then(|meant| meant.hook()) {
    return Some((hook, call));
  }
  let declared_hook = meant.as_ref().and_then(|meant| meant.origin()).is_some_and(|(_, name)| name.starts_with("use"));
  let name = match callee {
    js::Expression::Identifier(id) if matches!(meanings.ctxt(id), crate::Ctxt::Top | crate::Ctxt::Unresolved) => id.name.as_str(),
    member_expr if member_expr.is_member_expression() => {
let member = member_expr.as_member_expression().expect("matched as a member");
      let js::Expression::Identifier(ns) = member.object() else { return None };
      let crate::syntax::MemberProp::Ident(name) = crate::syntax::prop(member) else { return None };
      if !matches!(meanings.of_ident(ns), Some(crate::meaning::Meaning::Namespace(_))) {
        return None;
      }
      name.name.as_str()
    }
    _ => return None,
  };
  (declared_hook || name.starts_with("use")).then_some((name, call))
}

impl<'a, 'p> ComponentLowerer<'a, 'p> {
  /// `name` as a function only the browser calls: a setter, a dispatcher or an inner function.
  fn browser_function(&mut self, name: String) {
    self.lowerer.browser_functions.push(name.clone());
    self.handlers.push(name);
  }

  /// Each JSX-holding expression under `e` outside a function, lowered to
  /// markup ahead of the statement or attribute that holds it, so
  /// `Lowerer::expr` takes it made. `fills` later places it in the tree.
  fn premake_expr(&mut self, e: &'p js::Expression<'p>) -> Lowered<()> {
    if let Some(kind) = self.counts_own_children(e) {
      let span = e.span();
      self.lowerer.premade.insert((crate::lo(span), crate::hi(span)), Expr::Var(kind.to_owned()));
      if !self.counts_children.contains(&kind) {
        self.counts_children.push(kind);
      }
      return Ok(());
    }
    self.premake_children(e)?;
    if let js::Expression::ArrayExpression(a) = e {
      for item in array_items(a) {
        self.premake_expr(item)?;
      }
      return Ok(());
    }
    if self.holds_markup(e) {
      let tmpl = self.child_expr(e)?;
      let span = e.span();
      self.lowerer.premade.insert((crate::lo(span), crate::hi(span)), Expr::Markup(std::sync::Arc::new(tmpl)));
      return Ok(());
    }
    match e {
      js::Expression::ParenthesizedExpression(p) => self.premake_expr(&p.expression),
      js::Expression::ObjectExpression(o) => {
        for prop in &o.properties {
          if let js::ObjectPropertyKind::ObjectProperty(prop) = prop {
            if let crate::syntax::Prop::KeyValue(kv) = crate::syntax::prop_of(prop) {
              self.premake_expr(&kv.value)?;
            }
          }
        }
        Ok(())
      }
      js::Expression::ArrayExpression(a) => {
        for item in array_items(a) {
          self.premake_expr(item)?;
        }
        Ok(())
      }
      js::Expression::ConditionalExpression(c) => {
        self.premake_expr(&c.consequent)?;
        self.premake_expr(&c.alternate)
      }
      js::Expression::BinaryExpression(b) => {
        self.premake_expr(&b.left)?;
        self.premake_expr(&b.right)
      }
      js::Expression::LogicalExpression(l) => {
        self.premake_expr(&l.left)?;
        self.premake_expr(&l.right)
      }
      js::Expression::AssignmentExpression(a) => self.premake_expr(&a.right),
      js::Expression::ArrowFunctionExpression(arrow) => {
        let depth = self.lowerer.scope.len();
        let result = bind_params(&mut self.lowerer, &crate::params_of(&arrow.params)).and_then(|_| match crate::syntax::arrow_body_of(arrow) {
          crate::syntax::ArrowBody::Expr(e) => self.premake_expr(e),
          crate::syntax::ArrowBody::Block(b) => self.premake_returning(&b.body),
        });
        self.lowerer.scope.truncate(depth);
        result
      }
      js::Expression::CallExpression(call) => {
        {
      let callee = &call.callee;
          if let Some(member) = (callee).as_member_expression() {
            self.premake_expr(member.object())?;
          }
        }
        for arg in &call.arguments {
          self.premake_expr(crate::syntax::arg_expr(arg))?;
        }
        Ok(())
      }
      _ => Ok(()),
    }
  }

  /// [`Self::premake_expr`] over a statement whose expressions reach
  /// `Lowerer::expr`: a declaration, a write or a branch holding them.
  /// Marks the loop at `span` open for the hoist candidates its body holds, with the `push` calls its body makes onto `written`, so the browser's copy keys them by iteration; whether there was a candidate set to mark.
  fn open_statement_loop(&mut self, span: Span, body: &js::Statement, written: &[String]) -> bool {
    let range = self.lowerer.parsed.range(span);
    let body_range = self.lowerer.parsed.range(body.span());
    struct Pushes<'w> {
      written: &'w [String],
      args: Vec<Span>,
    }
    impl<'a> oxc_ast_visit::Visit<'a> for Pushes<'_> {
      fn visit_call_expression(&mut self, call: &js::CallExpression<'a>) {
        {
      let callee = &call.callee;
          if let Some(m) = (callee).as_member_expression() {
            if let (js::Expression::Identifier(id), Some("push")) = (m.object(), prop_name_of(m).as_deref()) {
              if self.written.iter().any(|w| w == id.name.as_str()) {
                self.args.extend(call.arguments.iter().filter(|a| !crate::syntax::is_spread(a)).map(|a| crate::syntax::arg_expr(a).span()));
              }
            }
          }
        }
        oxc_ast_visit::walk::walk_call_expression(self, call);
      }
      fn visit_function(&mut self, _: &js::Function<'a>, _: oxc_syntax::scope::ScopeFlags) {}
      fn visit_arrow_function_expression(&mut self, _: &js::ArrowFunctionExpression<'a>) {}
      fn visit_for_of_statement(&mut self, _: &js::ForOfStatement<'a>) {}
      fn visit_for_statement(&mut self, _: &js::ForStatement<'a>) {}
    }
    let mut pushes = Pushes { written, args: Vec::new() };
    body.walk(&mut pushes);
    let args: Vec<std::ops::Range<usize>> = pushes.args.into_iter().map(|span| self.lowerer.parsed.range(span)).collect();
    let Some(candidates) = &mut self.lowerer.hoisting else { return false };
    if !candidates.statement_loops.iter().any(|l| l.range == range) {
      candidates.statement_loops.push(hoist::StatementLoop::new(range.clone(), body_range, args));
    }
    candidates.open_loops.push(range);
    true
  }

  fn close_statement_loop(&mut self, open: bool) {
    if let (true, Some(candidates)) = (open, &mut self.lowerer.hoisting) {
      candidates.open_loops.pop();
    }
  }

  /// [`Self::premake_stmt`] over a block's statements with each of its declarations bound as it goes, so markup after one reads it; the bindings end with the block.
  /// [`Self::premake_stmt`] for `xs.forEach(f)` under the names the counted loop [`for_each_loop`] reads it as, so the markup its callback pushes and the fold agree.
  fn premake_for_each(&mut self, call: &'p js::CallExpression<'p>) -> Lowered<()> {
    let Some(js::Statement::ForStatement(f)) = for_each_loop(self.lowerer.build(), call, self.lowerer.meanings.as_deref()) else { return Ok(()) };
    let over_children = (&call.callee).as_member_expression().is_some_and(|m| self.meanings().of_expr(m.object()) == Some(crate::meaning::Meaning::Library(crate::meaning::Construct::Children)));
    if over_children {
      if self.slot_of_expr(&crate::syntax::arg_expr(&call.arguments[0])).as_deref() != Some("content") {
        return Ok(());
      }
      let values = snapfire_fsr_ir::render::CHILD_VALUES;
      if !self.counts_children.contains(&values) {
        self.counts_children.push(values);
      }
      if !self.lowerer.scope.iter().any(|(n, _)| n == values) {
        self.lowerer.scope.push((values.to_owned(), Expr::Var(values.to_owned())));
      }
    }
    let Ok(counted) = Counted::of(&self.lowerer, &f) else { return Ok(()) };
    let names = LoopNames::over(f.span, &f.init, &f.body);
    let callback = if over_children { &call.arguments[1] } else { &call.arguments[0] };
    let (params, body): (Vec<crate::Param<'p>>, FunctionBody<'p>) = match crate::meaning::bare(crate::syntax::arg_expr(callback)) {
      js::Expression::ArrowFunctionExpression(arrow) => (crate::params_of(&arrow.params), arrow_body(arrow)),
      js::Expression::FunctionExpression(func) => match &func.body {
        Some(b) => (crate::params_of(&func.params), FunctionBody::Block(&b.statements)),
        None => return Ok(()),
      },
      _ => return Ok(()),
    };
    let js::Statement::BlockStatement(looped) = &f.body else { return Ok(()) };
    let item = looped.body.first().and_then(|s| match s {
      js::Statement::VariableDeclaration(var) if params.first().is_some() => var.declarations.first().and_then(|d| d.init.as_ref()),
      _ => None,
    });
    let depth = self.lowerer.scope.len();
    let result = counted.lowered(&mut self.lowerer, &names.item).and_then(|(_, counter)| {
      for name in &names.written {
        self.lowerer.scope.push((name.clone(), names.read(name)));
      }
      self.lowerer.scope.push((counted.name.clone(), counter));
      if let (Some(head), Some(item)) = (params.first(), item) {
        let value = self.lowerer.expr(item)?;
        bind_param(&mut self.lowerer, head, value)?;
      }
      let open = self.open_statement_loop(f.span, &f.body, &names.written);
      let result = match body {
        FunctionBody::Block(stmts) => self.premake_block(stmts),
        FunctionBody::Expr(e) => self.premake_expr(e),
      };
      self.close_statement_loop(open);
      result
    });
    self.lowerer.scope.truncate(depth);
    result
  }

  fn premake_block(&mut self, stmts: &'p [js::Statement]) -> Lowered<()> {
    let depth = self.lowerer.scope.len();
    let result = stmts.iter().try_for_each(|stmt| {
      self.premake_stmt(stmt)?;
      if let js::Statement::VariableDeclaration(var) = stmt {
        for decl in &var.declarations {
          let value = match decl.init.as_ref() {
            Some(init) => self.premade_value(&decl.id, init)?,
            None => Expr::Lit(Lit::Undefined),
          };
          bind_pattern(&mut self.lowerer, &decl.id, value)?;
        }
      }
      Ok(())
    });
    self.lowerer.scope.truncate(depth);
    result
  }

  /// A declaration's value for the markup [`Self::premake_block`] prepares. A value the build cannot follow outside the application's own code, an awaited browser call among them, is bound as a variable: the pass that lowers the function itself decides what becomes of it.
  fn premade_value(&mut self, name: &js::BindingPattern, init: &'p js::Expression<'p>) -> Lowered<Expr> {
    match self.lowerer.expr(init) {
      Ok(value) => Ok(value),
      Err(residue) if self.unbound_is_followable() || self.lowerer.reach_violation => Err(residue),
      Err(_) => {
        self.lowerer.unbound = None;
        Ok(match name {
          js::BindingPattern::BindingIdentifier(id) => Expr::Var(id.name.to_string()),
          _ => Expr::Lit(Lit::Undefined),
        })
      }
    }
  }

  /// [`Self::premake_block`] over a function's body, whose `return`s hold values the way its declarations do.
  fn premake_returning(&mut self, stmts: &'p [js::Statement]) -> Lowered<()> {
    let depth = self.lowerer.scope.len();
    let result = stmts.iter().try_for_each(|stmt| {
      match stmt {
        js::Statement::ReturnStatement(r) => {
          if let Some(arg) = r.argument.as_ref() {
            self.premake_expr(arg)?;
          }
        }
        other => self.premake_stmt(other)?,
      }
      if let js::Statement::VariableDeclaration(var) = stmt {
        for decl in &var.declarations {
          let value = match decl.init.as_ref() {
            Some(init) => self.premade_value(&decl.id, init)?,
            None => Expr::Lit(Lit::Undefined),
          };
          bind_pattern(&mut self.lowerer, &decl.id, value)?;
        }
      }
      Ok(())
    });
    self.lowerer.scope.truncate(depth);
    result
  }

  fn premake_stmt(&mut self, stmt: &'p js::Statement<'p>) -> Lowered<()> {
    match stmt {
      js::Statement::VariableDeclaration(var) => {
        for decl in &var.declarations {
          if let Some(init) = decl.init.as_ref() {
            self.premake_expr(init)?;
          }
        }
        Ok(())
      }
      js::Statement::ExpressionStatement(e) => match &e.expression {
        js::Expression::CallExpression(call) if for_each_loop(self.lowerer.build(), call, self.lowerer.meanings.as_deref()).is_some() => self.premake_for_each(call),
        other => self.premake_expr(other),
      },
      js::Statement::IfStatement(branch) => {
        self.premake_stmt(&branch.consequent)?;
        match branch.alternate.as_ref() {
          Some(alt) => self.premake_stmt(alt),
          None => Ok(()),
        }
      }
      js::Statement::BlockStatement(b) => self.premake_block(&b.body),
      js::Statement::TryStatement(t) => self.premake_stmt_try(t),
      js::Statement::SwitchStatement(sw) => {
        for case in &sw.cases {
          self.premake_block(&case.consequent)?;
        }
        Ok(())
      }
      js::Statement::ForOfStatement(for_of) => {
        let Some(head) = LoopNames::head(for_of) else { return Ok(()) };
        let names = LoopNames::of(for_of);
        let depth = self.lowerer.scope.len();
        for name in &names.written {
          self.lowerer.scope.push((name.clone(), names.read(name)));
        }
        let open = self.open_statement_loop(for_of.span, &for_of.body, &names.written);
        let result = bind_pattern(&mut self.lowerer, head, Expr::Var(names.item.clone())).and_then(|_| self.premake_stmt(&for_of.body));
        self.close_statement_loop(open);
        self.lowerer.scope.truncate(depth);
        result
      }
      js::Statement::ForStatement(f) => {
        let Ok(counted) = Counted::of(&self.lowerer, f) else { return Ok(()) };
        let names = LoopNames::over(f.span, &f.init, &f.body);
        let depth = self.lowerer.scope.len();
        let result = counted.lowered(&mut self.lowerer, &names.item).and_then(|(_, counter)| {
          for name in &names.written {
            self.lowerer.scope.push((name.clone(), names.read(name)));
          }
          self.lowerer.scope.push((counted.name.clone(), counter));
          let open = self.open_statement_loop(f.span, &f.body, &names.written);
          let result = self.premake_stmt(&f.body);
          self.close_statement_loop(open);
          result
        });
        self.lowerer.scope.truncate(depth);
        result
      }
      _ => Ok(()),
    }
  }

  fn component(&mut self, found: &Found<'p>) -> Lowered<(Component, Vec<(String, (usize, usize))>)> {
    let depth = self.lowerer.scope.len();
    let (params, body): (Vec<crate::Param<'p>>, FunctionBody<'p>) = match found {
      Found::Declared(params, stmts, block) => {
        self.hook = Some(Hook::Block { after: self.lowerer.parsed.range(*block).start + 1 });
        (crate::params_of(params), FunctionBody::Block(stmts))
      }
      Found::Arrow(arrow) => {
        self.hook = Some(match crate::syntax::arrow_body_of(arrow) {
          crate::syntax::ArrowBody::Block(b) => Hook::Block { after: self.lowerer.parsed.range(b.span).start + 1 },
          crate::syntax::ArrowBody::Expr(e) => Hook::Expression(self.lowerer.parsed.range(e.span())),
        });
        (crate::params_of(&arrow.params), arrow_body(arrow))
      }
    };
    let floor = self.lowerer.scope.len();
    self.bind_props(&params)?;
    if self.forwards_ref {
      if let Some(js::BindingPattern::BindingIdentifier(id)) = params.get(1).map(|p| p.pattern) {
        self.lowerer.scope.push((id.name.to_string(), Expr::Field(Box::new(Expr::Var("$props".to_owned())), "ref".to_owned())));
      }
    }
    let mut lets = Vec::new();
    if let Some(class) = self.class {
      lets.extend(self.bind_this(class)?);
    }
    let render = match body {
      FunctionBody::Expr(e) => self.child_expr(e)?,
      FunctionBody::Block(stmts) => {
        let mut render = None;
        let mut early: Vec<(Expr, Tmpl)> = Vec::new();
        // A value bound after an early `return` is computed only where none of them fired, since the body binds every value before the tree picks a branch.
        let guarded = |early: &[(Expr, Tmpl)], expr: Expr| match early.iter().map(|(cond, _)| cond.clone()).reduce(|a, b| Expr::Logic(LogicOp::Or, Box::new(a), Box::new(b))) {
          None => expr,
          Some(fired) => Expr::Ternary(Box::new(fired), Box::new(Expr::Lit(Lit::Null)), Box::new(expr)),
        };
        // A function declaration is hoisted: a handler above it calls it by name.
        self.lowerer.local_fns.clear();
        for stmt in stmts.iter() {
          if let (js::Statement::FunctionDeclaration(f), Some(id)) = (stmt, function_name(stmt)) {
            if let Some(body) = &f.body {
              self.handler_fns.insert(id.to_owned(), (crate::params_of(&f.params), FunctionBody::Block(&body.statements)).into());
              self.lowerer.local_fns.push((id.to_owned(), &**f));
            }
          }
        }
        self.declare_below(stmts, "");
        for (at, stmt) in stmts.iter().enumerate() {
          match stmt {
            js::Statement::ReturnStatement(ret) => {
              render = Some(match ret.argument.as_ref() {
                Some(arg) => self.child_expr(arg)?,
                None => Tmpl::Fragment(Vec::new()),
              });
              break;
            }
            js::Statement::IfStatement(branch) if holds_return(stmt) && early_return(branch).is_none() => {
              let rest: Vec<&'p js::Statement<'p>> = stmts[at..].iter().collect();
              render = Some(self.tree_of(&rest, floor)?);
              break;
            }
            js::Statement::SwitchStatement(_) | js::Statement::BlockStatement(_) | js::Statement::TryStatement(_) if holds_return(stmt) => {
              let rest: Vec<&'p js::Statement<'p>> = stmts[at..].iter().collect();
              render = Some(self.tree_of(&rest, floor)?);
              break;
            }
            js::Statement::IfStatement(branch) if early_return(branch).is_some() => {
              let ret = early_return(branch).expect("checked");
              let cond = self.lowerer.expr(&branch.test)?;
              let then = match ret.argument.as_ref() {
                Some(arg) => self.child_expr(arg)?,
                None => Tmpl::Fragment(Vec::new()),
              };
              early.push((cond, then));
            }
            js::Statement::VariableDeclaration(var) if !early.is_empty() => {
              self.premake_stmt(stmt)?;
              for decl in &var.declarations {
                if let Some(init) = decl.init.as_ref() {
                  if let Some((hook, _)) = hook_call(&self.meanings(), init).filter(|(hook, _)| *hook != "use") {
                    return Err(self.lowerer.residue(decl.span, format!("`{hook}` after an early `return`, which React refuses since the hooks would change between renders")));
                  }
                }
                let stmt = self.let_stmt(decl)?;
                self.note_alias_of(decl, floor)?;
                for pending in std::mem::take(&mut self.pending_lets) {
                  if let Stmt::Let { name, expr } = pending {
                    lets.push(Stmt::Let { name, expr: guarded(&early, expr) });
                  }
                }
                if let Some(Stmt::Let { name, expr }) = stmt {
                  lets.push(Stmt::Let { name, expr: guarded(&early, expr) });
                }
              }
            }
            js::Statement::ExpressionStatement(e) if library_hook(&self.meanings(), &e.expression).is_some_and(|(name, _)| name == "useInput") => {
              let (_, call) = library_hook(&self.meanings(), &e.expression).expect("checked");
              self.input_hook(call)?;
            }
            js::Statement::ExpressionStatement(e) if hook_call(&self.meanings(), &e.expression).is_some_and(|(name, _)| self.custom_hook(name)) => {
              let (hook, call) = hook_call(&self.meanings(), &e.expression).expect("checked");
              self.inline_hook(hook, call, None)?;
              lets.append(&mut self.pending_lets);
            }
            js::Statement::ExpressionStatement(e) if !hook_call(&self.meanings(), &e.expression).is_some_and(|(name, _)| EFFECT_HOOKS.contains(&name)) => {
              self.premake_stmt(stmt)?;
              self.rebound(stmt, floor, &early, &mut lets, &guarded)?
            }
            js::Statement::IfStatement(_) | js::Statement::SwitchStatement(_) | js::Statement::BlockStatement(_) | js::Statement::ForOfStatement(_) | js::Statement::ForStatement(_) | js::Statement::TryStatement(_) => {
              self.premake_stmt(stmt)?;
              self.rebound(stmt, floor, &early, &mut lets, &guarded)?
            }
            js::Statement::FunctionDeclaration(f) => {
              let name = f.id.as_ref().map(|id| id.name.to_string()).unwrap_or_default();
              self.browser_function(name.clone());
              if let Some(body) = &f.body {
                self.handler_fns.insert(name, (crate::params_of(&f.params), FunctionBody::Block(&body.statements)).into());
              }
            }
            js::Statement::ExpressionStatement(e) if hook_call(&self.meanings(), &e.expression).is_some_and(|(name, _)| EFFECT_HOOKS.contains(&name)) => {
              let (hook, call) = hook_call(&self.meanings(), &e.expression).expect("checked");
              if hook == "useEffect" || hook == "useLayoutEffect" {
                self.effect_timers(call)?;
                lets.append(&mut self.pending_lets);
              }
            }
            js::Statement::VariableDeclaration(var) => {
              self.premake_stmt(stmt)?;
              for decl in &var.declarations {
                let stmt = self.let_stmt(decl)?;
                self.note_alias_of(decl, floor)?;
                lets.append(&mut self.pending_lets);
                if let Some(stmt) = stmt {
                  lets.push(stmt);
                }
              }
            }
            js::Statement::WhileStatement(_) if self.lowerer.reaches(Reach::Terminal) => {
              self.premake_stmt(stmt)?;
              self.rebound(stmt, floor, &early, &mut lets, &guarded)?
            }
            other => return Err(self.lowerer.residue(other.span(), "a statement a component cannot hold before its `return`")),
          }
        }
        let mut asked: Vec<String> = Vec::new();
        for (_, state) in &self.setters {
          let key = format!("{}{state}", snapfire_fsr_ir::render::RERUN_PREFIX);
          if !asked.contains(&key) && self.lowerer.scope.iter().any(|(name, _)| *name == key) {
            lets.push(Stmt::Let { name: key.clone(), expr: current(&self.lowerer, &key) });
            asked.push(key);
          }
        }
        let render = render.ok_or_else(|| self.lowerer.residue(Span::default(), "a component must return its tree"))?;
        early.into_iter().rev().fold(render, |r#else, (cond, then)| Tmpl::If { cond, then: Box::new(then), r#else: Some(Box::new(r#else)) })
      }
    };
    self.lowerer.scope.truncate(depth);
    // A module-level `let` a function writes is state the store holds per request, seeded by the module's own initialisation; a handler's write to it reaches the store as any store write does.
    let mut bound = Vec::new();
    for (name, key, seed) in std::mem::take(&mut self.module_lets) {
      if self.lowerer.reaches(Reach::Later) {
        bound.push(Stmt::Let { name, expr: Expr::ModuleCell { key: Box::new(Expr::lit_str(key)), init: Box::new(seed) } });
        continue;
      }
      self.state.push(name.clone());
      self.state_bindings.push(name.clone());
      self.store_bindings.push((name.clone(), key.clone()));
      bound.push(Stmt::Let { name, expr: Expr::Coalesce(Box::new(Expr::Store(key)), Box::new(seed)) });
    }
    bound.extend(lets);
    let mut lets = bound;
    lets.extend(std::mem::take(&mut self.store_lets));
    for (bound, span) in std::mem::take(&mut self.lowerer.read_below) {
      if !lets.iter().any(|stmt| matches!(stmt, Stmt::Let { name, .. } if *name == bound)) {
        return Err(self.lowerer.residue(span, format!("`{bound}`, declared below what reads it, is not a value the body binds")));
      }
    }
    self.lowerer.declared_below.clear();
    self.lowered_handlers.append(&mut self.lowerer.continuations);
    Ok((Component { body: lets, render, state: std::mem::take(&mut self.state_bindings), stores: std::mem::take(&mut self.store_bindings), handlers: std::mem::take(&mut self.lowered_handlers), owner: Owner::React, shadow: None, scope: None }, std::mem::take(&mut self.refs)))
  }

  /// Records `decl` as an alias when it binds a name to an element of a local array (`note_alias`).
  fn note_alias_of(&mut self, decl: &js::VariableDeclarator, floor: usize) -> Lowered<()> {
    let (js::BindingPattern::BindingIdentifier(id), Some(init)) = (&decl.id, decl.init.as_ref()) else { return Ok(()) };
    let frozen = self.state.clone();
    note_alias(&mut self.lowerer, id.name.as_str(), init, &Writable { floor, frozen: &frozen, setters: &[] })
  }

  /// The statements from a branching `return` on, as the tree each path returns: a branch is a `Tmpl::If`, a declaration a `Tmpl::Let` over what follows it, a write to a local a `Tmpl::Let` of its new value. A path that ends without a `return` renders nothing.
  fn tree_of(&mut self, stmts: &[&'p js::Statement<'p>], floor: usize) -> Lowered<Tmpl> {
    let Some((first, rest)) = stmts.split_first() else { return Ok(Tmpl::Fragment(Vec::new())) };
    let joined = |head: &'p js::Statement<'p>, rest: &[&'p js::Statement<'p>]| -> Vec<&'p js::Statement<'p>> {
      let mut out: Vec<&'p js::Statement<'p>> = match head {
        js::Statement::BlockStatement(b) => b.body.iter().collect(),
        other => vec![other],
      };
      out.extend_from_slice(rest);
      out
    };
    match first {
      js::Statement::ReturnStatement(ret) => match ret.argument.as_ref() {
        Some(arg) => self.child_expr(arg),
        None => Ok(Tmpl::Fragment(Vec::new())),
      },
      js::Statement::ThrowStatement(thrown) => Ok(Tmpl::Expr(throw_expr(&mut self.lowerer, thrown)?)),
      js::Statement::EmptyStatement(_) => self.tree_of(rest, floor),
      js::Statement::BlockStatement(_) if holds_return(first) => {
        let depth = self.lowerer.scope.len();
        let tree = self.tree_of(&joined(first, rest), floor);
        self.lowerer.scope.truncate(depth);
        tree
      }
      js::Statement::IfStatement(branch) if holds_return(first) => {
        let cond = self.lowerer.expr(&branch.test)?;
        let depth = self.lowerer.scope.len();
        let then = self.tree_of(&joined(&branch.consequent, rest), floor);
        self.lowerer.scope.truncate(depth);
        let r#else = match branch.alternate.as_ref() {
          Some(alt) => self.tree_of(&joined(alt, rest), floor),
          None => self.tree_of(rest, floor),
        };
        self.lowerer.scope.truncate(depth);
        Ok(Tmpl::If { cond, then: Box::new(then?), r#else: Some(Box::new(r#else?)) })
      }
      js::Statement::SwitchStatement(sw) if holds_return(first) => {
        let disc = self.lowerer.expr(&sw.discriminant)?;
        let (arms, default) = switch_arms(&self.lowerer, sw)?;
        let depth = self.lowerer.scope.len();
        let mut fallback: Vec<&'p js::Statement<'p>> = default.map(|d| d.iter().collect()).unwrap_or_default();
        fallback.extend_from_slice(rest);
        let tree = self.tree_of(&fallback, floor);
        self.lowerer.scope.truncate(depth);
        let mut tree = tree?;
        for (tests, body) in arms.into_iter().rev() {
          let mut cond: Option<Expr> = None;
          for test in tests {
            let eq = Expr::Compare(CompareOp::Eq, Box::new(disc.clone()), Box::new(self.lowerer.expr(test)?));
            cond = Some(match cond {
              None => eq,
              Some(c) => Expr::Logic(LogicOp::Or, Box::new(c), Box::new(eq)),
            });
          }
          let mut path: Vec<&'p js::Statement<'p>> = body.iter().collect();
          path.extend_from_slice(rest);
          let then = self.tree_of(&path, floor);
          self.lowerer.scope.truncate(depth);
          let cond = cond.unwrap_or(Expr::Lit(Lit::Bool(false)));
          tree = Tmpl::If { cond, then: Box::new(then?), r#else: Some(Box::new(tree)) };
        }
        Ok(tree)
      }
      js::Statement::VariableDeclaration(var) => {
        self.premake_stmt(first)?;
        let depth = self.lowerer.scope.len();
        let mut lets = Vec::new();
        for decl in &var.declarations {
          if let Some((hook, _)) = decl.init.as_ref().and_then(|init| hook_call(&self.meanings(), init)).filter(|(hook, _)| !matches!(*hook, "use" | "useContext")) {
            return Err(self.lowerer.residue(decl.span, format!("`{hook}` after a `return`, which React refuses since the hooks would change between renders")));
          }
          let expr = match decl.init.as_ref() {
            Some(init) => self.lowerer.expr(init)?,
            None => Expr::Lit(Lit::Undefined),
          };
          let name = match &decl.id {
            js::BindingPattern::BindingIdentifier(id) => {
              let name = id.name.to_string();
              self.lowerer.scope.push((name.clone(), Expr::Var(name.clone())));
              if let Some(init) = decl.init.as_ref() {
                self.lowerer.note_kind(&name, init);
              }
              self.note_alias_of(decl, floor)?;
              name
            }
            pattern => {
              let name = self.lowerer.temp();
              bind_pattern(&mut self.lowerer, pattern, Expr::Var(name.clone()))?;
              name
            }
          };
          lets.push((name, expr));
        }
        let then = self.tree_of(rest, floor);
        self.lowerer.scope.truncate(depth);
        let mut tree = then?;
        for (name, expr) in lets.into_iter().rev() {
          tree = Tmpl::Let { name, expr, then: Box::new(tree) };
        }
        Ok(tree)
      }
      js::Statement::FunctionDeclaration(f) => Err(self.lowerer.residue(f.span, "a function declared after a branching `return`; declare it before")),
      js::Statement::TryStatement(t) if holds_return(first) => self.try_tree(t, rest, floor),
      straight => {
        self.premake_stmt(straight)?;
        let depth = self.lowerer.scope.len();
        let frozen = self.state.clone();
        let writable = Writable { floor, frozen: &frozen, setters: &[] };
        let mut assigned = Vec::new();
        let (held, run) = held_frame(&mut self.lowerer, |lowerer| rebind_stmt(lowerer, straight, &writable, &mut assigned));
        run?;
        let mut lets: Vec<(String, Expr)> = held.into_iter().filter_map(|stmt| match stmt {
          Stmt::Let { name, expr } => Some((name, expr)),
          _ => None,
        }).collect();
        for name in assigned {
          let value = current(&self.lowerer, &name);
          let fresh = self.lowerer.temp();
          keep_kind(&mut self.lowerer, &name, &value, Expr::Var(fresh.clone()));
          self.lowerer.scope.push((name, Expr::Var(fresh.clone())));
          lets.push((fresh, value));
        }
        let then = self.tree_of(rest, floor);
        self.lowerer.scope.truncate(depth);
        let mut tree = then?;
        for (name, expr) in lets.into_iter().rev() {
          tree = Tmpl::Let { name, expr, then: Box::new(tree) };
        }
        Ok(tree)
      }
    }
  }

  /// A `try` that returns markup, then what follows it. The `try` runs as statements. It ends with which of its `return`s it reached and the locals in scope there. The tree picks that `return`'s markup over those locals. When it runs to its end it holds the locals it wrote and the tree goes on with what follows. A failure while the markup itself evaluates is not the `try`'s to catch.
  fn try_tree(&mut self, t: &'p js::TryStatement<'p>, rest: &[&'p js::Statement<'p>], floor: usize) -> Lowered<Tmpl> {
    self.premake_stmt_try(t)?;
    let frozen: Vec<String> = self.state.clone();
    let writable = Writable { floor, frozen: &frozen, setters: &[] };
    let base = self.lowerer.scope.len();
    let outer = (self.lowerer.markup_base.replace(base), std::mem::take(&mut self.lowerer.markup_returns));
    let ran = try_run(&mut self.lowerer, t, &writable);
    let returns = std::mem::replace(&mut self.lowerer.markup_returns, outer.1);
    self.lowerer.markup_base = outer.0;
    let (ran, written) = ran?;
    let held = self.lowerer.temp();
    let read = |key: &str| Expr::Field(Box::new(Expr::Var(held.clone())), key.to_owned());
    let depth = self.lowerer.scope.len();
    for name in &written {
      self.lowerer.scope.push((name.clone(), read(name)));
    }
    let after = self.tree_of(rest, floor);
    self.lowerer.scope.truncate(depth);
    let mut tree = after?;
    let found = returns_in(t);
    for (path, (at, names)) in returns.into_iter().enumerate().rev() {
      let Some(ret) = found.iter().find(|ret| crate::lo(ret.span) == at) else { return Err(self.lowerer.residue(t.span, "a `return` inside a `try` the build could not place")) };
      for (k, name) in names.iter().enumerate() {
        self.lowerer.scope.push((name.clone(), read(&format!("${k}"))));
      }
      let then = match ret.argument.as_ref() {
        Some(arg) => self.child_expr(arg),
        None => Ok(Tmpl::Fragment(Vec::new())),
      };
      self.lowerer.scope.truncate(depth);
      let cond = Expr::Compare(CompareOp::Eq, Box::new(read("$path")), Box::new(Expr::Lit(Lit::Int(path as i128))));
      tree = Tmpl::If { cond, then: Box::new(then?), r#else: Some(Box::new(tree)) };
    }
    Ok(Tmpl::Let { name: held, expr: ran, then: Box::new(tree) })
  }

  fn premake_stmt_try(&mut self, t: &'p js::TryStatement<'p>) -> Lowered<()> {
    self.premake_block(&t.block.body)?;
    if let Some(catch) = &t.handler {
      let depth = self.lowerer.scope.len();
      let made = catch_param(&mut self.lowerer, catch.param.as_ref()).and_then(|_| self.premake_block(&catch.body.body));
      self.lowerer.scope.truncate(depth);
      made?;
    }
    match &t.finalizer {
      Some(finalizer) => self.premake_block(&finalizer.body),
      None => Ok(()),
    }
  }

  /// A statement that writes locals, run over the scope: each name it wrote is bound to its final value by a `let` of its own.
  fn rebound(&mut self, stmt: &js::Statement, floor: usize, early: &[(Expr, Tmpl)], lets: &mut Vec<Stmt>, guarded: &dyn Fn(&[(Expr, Tmpl)], Expr) -> Expr) -> Lowered<()> {
    let frozen: Vec<String> = self.state.iter().filter(|name| !self.held_refs.contains(name)).cloned().collect();
    let setters = self.setters.clone();
    let writable = Writable { floor, frozen: &frozen, setters: &setters };
    let mut assigned = Vec::new();
    let (held, run) = held_frame(&mut self.lowerer, |lowerer| rebind_stmt(lowerer, stmt, &writable, &mut assigned));
    run?;
    for held in held {
      if let Stmt::Let { name, expr } = held {
        lets.push(Stmt::Let { name, expr: guarded(early, expr) });
      }
    }
    for name in assigned {
      let value = current(&self.lowerer, &name);
      let fresh = self.lowerer.temp();
      keep_kind(&mut self.lowerer, &name, &value, Expr::Var(fresh.clone()));
      lets.push(Stmt::Let { name: fresh.clone(), expr: guarded(early, value) });
      if self.held_refs.contains(&name) {
        lets.push(Stmt::Let { name: format!("{}{name}", snapfire_fsr_ir::render::HOLD_PREFIX), expr: Expr::Var(fresh.clone()) });
      }
      self.lowerer.scope.push((name, Expr::Var(fresh)));
    }
    Ok(())
  }

  /// `components={{ h2: Heading }}` on a placed MDX file: each element and the
  /// component this file's name for it resolves to.
  fn component_overrides(&mut self, attr: &'p js::JSXAttribute<'p>) -> Lowered<Vec<Override>> {
    let object = match attr_expression(attr) {
      Some(e) => match e {
        js::Expression::ObjectExpression(object) => object,
        other => return Err(self.lowerer.residue(other.span(), "`components` that is not an object written out, as `{{ h2: Heading }}`")),
      },
      _ => return Err(self.lowerer.residue(attr.span, "`components` that is not an object written out, as `{{ h2: Heading }}`")),
    };
    let mut out = Vec::new();
    for prop in &object.properties {
      let (key, local) = match prop {
        js::ObjectPropertyKind::ObjectProperty(p) => match crate::syntax::prop_of(p) {
          crate::syntax::Prop::KeyValue(kv) => match (prop_name(&kv.key, kv.computed), &kv.value) {
            (Some(key), js::Expression::Identifier(local)) => (key, local.name.to_string()),
            _ => return Err(self.lowerer.residue(prop.span(), "a `components` entry that is not a component's name")),
          },
          crate::syntax::Prop::Shorthand(id) => (id.name.to_string(), id.name.to_string()),
          _ => return Err(self.lowerer.residue(prop.span(), "a `components` entry that is not a component's name")),
        },
        js::ObjectPropertyKind::SpreadProperty(spread) => return Err(self.lowerer.residue(spread.argument.span(), "a spread in `components`")),
      };
      match self.meanings().bindings.get(&local) {
        Some(crate::meaning::Meaning::Own { file, name }) => out.push(Override { key, specifier: format!("@app/{file}"), export: name.clone() }),
        Some(meant @ crate::meaning::Meaning::Library(_)) if meant.placement().is_some() => {
          let Some((source, imported)) = find_import(self.lowerer.parsed, &local) else { return Err(self.lowerer.residue(prop.span(), format!("`{local}` is not imported"))) };
          out.push(Override { key, specifier: source, export: imported });
        }
        _ => return Err(self.lowerer.residue(prop.span(), format!("`{local}` is not a component this file declares or imports"))),
      }
    }
    out.sort_by(|a, b| a.key.cmp(&b.key));
    Ok(out)
  }

  /// The id a placement of `target` names: the file itself, or with
  /// `overrides` a variant of it the set lowers with those elements replaced.
  fn variant_of(&mut self, target: &str, overrides: &[Override]) -> String {
    if overrides.is_empty() {
      return target.to_owned();
    }
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for o in overrides {
      for byte in format!("{}={}:{};", o.key, o.specifier, o.export).bytes() {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(0x0100_0000_01b3);
      }
    }
    let variant = format!("{target}~{:08x}", hash as u32);
    if !self.variants.iter().any(|(v, _, _)| *v == variant) {
      self.variants.push((variant.clone(), target.to_owned(), overrides.to_vec()));
    }
    variant
  }

  /// `const { default: Post, meta } = await import(`../content/${slug}.mdx`)`:
  /// `Post` places whichever file the key names and `meta` is that file's
  /// export, each chosen by comparing the key with the text that names a file.
  fn template_import(&mut self, decl: &'p js::VariableDeclarator<'p>, call: &'p js::ImportExpression<'p>) -> Lowered<()> {
    let Some(choices) = self.imports.get(&crate::lo(call.span)).cloned() else {
      return Err(self.lowerer.residue(call.span, "an `import()` of something other than a template with one substitution, such as `../content/${slug}.mdx`"));
    };
    let js::Expression::TemplateLiteral(tpl) = &call.source else { unreachable!("a template import") };
    let key = self.lowerer.expr(&tpl.expressions[0])?;
    let js::BindingPattern::ObjectPattern(pattern) = &decl.id else {
      return Err(self.lowerer.residue(decl.span, "an imported module bound whole; destructure it, as `const { default: Post, meta } = await import(...)`"));
    };
    if choices.is_empty() {
      return Err(self.lowerer.residue(call.span, "an `import()` no file under the app matches"));
    }
    let picks = |value: &str| Expr::Compare(CompareOp::Eq, Box::new(Expr::Str(Box::new(key.clone()))), Box::new(Expr::lit_str(value)));
    for prop in crate::syntax::pat_props(pattern) {
      let (export, local) = match prop {
        crate::syntax::PatProp::KeyValue(kv) => match (prop_name(&kv.key, kv.computed), &kv.value) {
          (Some(export), js::BindingPattern::BindingIdentifier(local)) => (export, local.name.to_string()),
          _ => return Err(self.lowerer.residue(prop.span(), "a nested pattern in an imported module's destructuring")),
        },
        crate::syntax::PatProp::Assign(a) if a.value.is_none() => (a.key.name.to_string(), a.key.name.to_string()),
        other => return Err(self.lowerer.residue(other.span(), "a default or a rest in an imported module's destructuring")),
      };
      if export == "default" {
        if !local.starts_with(|c: char| c.is_ascii_uppercase()) {
          return Err(self.lowerer.residue(prop.span(), format!("`{local}` names a component; capitalise it so JSX places it")));
        }
        self.chosen.insert(local, (key.clone(), choices.clone()));
        continue;
      }
      let mut value = Expr::Lit(Lit::Null);
      for (text, target) in choices.iter().rev() {
        let Some((_, export_value)) = self.lowerer.globals.iter().find(|(k, _)| *k == format!("{target}#{export}")).cloned() else {
          return Err(self.lowerer.residue(prop.span(), format!("`{export}`, which `{target}` does not export as a value the build reads")));
        };
        value = Expr::Ternary(Box::new(picks(text)), Box::new(export_value), Box::new(value));
      }
      let fresh = self.lowerer.temp();
      self.pending_lets.push(Stmt::Let { name: fresh.clone(), expr: value });
      self.lowerer.scope.push((local, Expr::Var(fresh)));
    }
    Ok(())
  }

  /// `this` in a class's `render()`: `this.properties` the props, `this.state` the initial state as component state, each method and arrow field a function the browser runs, `createRef()` a ref holding null and each other field its value.
  fn bind_this(&mut self, class: &'p js::Class<'p>) -> Lowered<Option<Stmt>> {
    let mut entries = vec![Entry::Field("props".to_owned(), Expr::Var("$props".to_owned()))];
    let mut state: Option<Expr> = None;
    let browser = || Expr::Handler(snapfire_fsr_ir::render::BROWSER_HANDLER);
    for member in &class.body.body {
      match member {
        js::ClassElement::PropertyDefinition(p) if !p.r#static => {
          let Some(key) = prop_name(&p.key, p.computed) else { return Err(self.lowerer.residue(p.span, "a computed class field")) };
          let value = match p.value.as_ref().map(crate::meaning::bare) {
            None => Expr::Lit(Lit::Null),
            Some(js::Expression::ArrowFunctionExpression(_) | js::Expression::FunctionExpression(_)) => browser(),
            Some(js::Expression::CallExpression(call)) if matches!(&call.callee, c if react_export(self.lowerer.parsed, c).as_deref() == Some("createRef")) => Expr::Object(vec![Entry::Field("current".to_owned(), Expr::Lit(Lit::Null))]),
            Some(value) => self.lowerer.expr(value)?,
          };
          if key == "state" {
            state = Some(value);
          } else {
            entries.push(Entry::Field(key, value));
          }
        }
        js::ClassElement::MethodDefinition(m) if !m.r#static && m.kind != js::MethodDefinitionKind::Constructor => {
          if let Some(key) = prop_name(&m.key, m.computed).filter(|k| k != "render") {
            entries.push(Entry::Field(key, browser()));
          }
        }
        js::ClassElement::MethodDefinition(c) if c.kind == js::MethodDefinitionKind::Constructor => {
          let depth = self.lowerer.scope.len();
          if let Some(id) = crate::first_param(&c.value.params).and_then(|p| p.ident()) {
            self.lowerer.scope.push((id.name.to_string(), Expr::Var("$props".to_owned())));
          }
          for stmt in c.value.body.iter().flat_map(|b| &b.statements) {
            let js::Statement::ExpressionStatement(e) = stmt else { return Err(self.lowerer.residue(stmt.span(), "a constructor statement other than `super(props)` or a write to `this`")) };
            match &e.expression {
              js::Expression::CallExpression(call) if matches!(call.callee, js::Expression::Super(_)) => {}
              js::Expression::AssignmentExpression(a) if a.operator == js::AssignmentOperator::Assign => {
                let field = match &a.left {
                  target if target.as_member_expression().is_some_and(|m| matches!(m.object(), js::Expression::ThisExpression(_))) => target.as_member_expression().and_then(prop_name_of),
                  _ => None,
                };
                let Some(field) = field else { return Err(self.lowerer.residue(a.span, "a constructor write to something other than `this`")) };
                if field == "state" {
                  state = Some(self.lowerer.expr(&a.right)?);
                } else if !entries.iter().any(|e| matches!(e, Entry::Field(n, _) if *n == field)) {
                  let value = match crate::meaning::bare(&a.right) {
                    js::Expression::ArrowFunctionExpression(_) | js::Expression::FunctionExpression(_) | js::Expression::CallExpression(_) => browser(),
                    other => self.lowerer.expr(other)?,
                  };
                  entries.push(Entry::Field(field, value));
                }
              }
              other => return Err(self.lowerer.residue(other.span(), "a constructor statement other than `super(props)` or a write to `this`")),
            }
          }
          self.lowerer.scope.truncate(depth);
        }
        _ => {}
      }
    }
    let stmt = state.map(|expr| {
      let name = "this$state".to_owned();
      self.state.push(name.clone());
      self.state_bindings.push(name.clone());
      entries.push(Entry::Field("state".to_owned(), Expr::Var(name.clone())));
      Stmt::Let { name, expr }
    });
    self.lowerer.scope.push(("this".to_owned(), Expr::Object(entries)));
    Ok(stmt)
  }

  fn bind_props(&mut self, params: &[crate::Param]) -> Lowered<()> {
    if self.positional {
      for (i, param) in params.iter().enumerate() {
        bind_param(&mut self.lowerer, param, Expr::Field(Box::new(Expr::Var("$props".to_owned())), i.to_string()))?;
      }
      return Ok(());
    }
    let Some(first) = params.first() else { return Ok(()) };
    for (key, kind) in prop_kinds_of(self.lowerer.parsed, first) {
      match self.lowerer.prop_kinds.iter_mut().find(|(k, _)| *k == key) {
        Some(entry) => entry.1 = kind,
        None => self.lowerer.prop_kinds.push((key, kind)),
      }
    }
    if first.default.is_some() || first.rest {
      return Err(self.lowerer.residue(first.span, "the props parameter must be a name or a destructuring"));
    }
    match first.pattern {
      js::BindingPattern::BindingIdentifier(id) => {
        self.props_name = Some(id.name.to_string());
        self.lowerer.scope.push((id.name.to_string(), Expr::Var("$props".to_owned())));
        Ok(())
      }
      js::BindingPattern::ObjectPattern(obj) => {
        self.children_name = crate::syntax::pat_props(obj).into_iter().find_map(|prop| match prop {
          crate::syntax::PatProp::Assign(a) if a.key.name.as_str() == "children" => Some("children".to_owned()),
          crate::syntax::PatProp::KeyValue(kv) if prop_name(&kv.key, kv.computed).as_deref() == Some("children") => match &kv.value {
            js::BindingPattern::BindingIdentifier(local) => Some(local.name.to_string()),
            _ => None,
          },
          _ => None,
        });
        self.props_rest = match (&self.children_name, &obj.rest) {
          (None, Some(rest)) => match &rest.argument {
            js::BindingPattern::BindingIdentifier(local) => Some(local.name.to_string()),
            _ => None,
          },
          _ => None,
        };
        if self.layout_root {
          for prop in crate::syntax::pat_props(obj) {
            let (local, key) = match prop {
              crate::syntax::PatProp::Assign(a) => (a.key.name.to_string(), a.key.name.to_string()),
              crate::syntax::PatProp::KeyValue(kv) => match (prop_name(&kv.key, kv.computed), &kv.value) {
                (Some(key), js::BindingPattern::BindingIdentifier(local)) => (local.name.to_string(), key),
                _ => continue,
              },
              crate::syntax::PatProp::Rest(_) => continue,
            };
            if self.slot_names.contains(&key) {
              self.slot_props.push((local, key));
            }
          }
        }
        bind_object(&mut self.lowerer, obj, Expr::Var("$props".to_owned()))?;
        for prop in crate::syntax::pat_props(obj) {
          let (local, key) = match prop {
            crate::syntax::PatProp::Assign(a) => (a.key.name.to_string(), a.key.name.to_string()),
            crate::syntax::PatProp::KeyValue(kv) => match (prop_name(&kv.key, kv.computed), &kv.value) {
              (Some(key), js::BindingPattern::BindingIdentifier(local)) => (local.name.to_string(), key),
              _ => continue,
            },
            crate::syntax::PatProp::Rest(_) => continue,
          };
          let Some(kind) = self.lowerer.prop_kinds.iter().find(|(k, _)| *k == key).map(|(_, kind)| *kind) else { continue };
          let Some((_, bound)) = self.lowerer.scope.iter().rev().find(|(n, _)| *n == local) else { continue };
          let bound = bound.clone();
          self.lowerer.kinds.push((local, bound, kind));
        }
        Ok(())
      }
      other => Err(self.lowerer.residue(other.span(), "the props parameter must be a name or a destructuring")),
    }
  }

  /// `const x = e` as a `let`; `const [x, setX] = useState(e)` as `let x = e`
  /// with `setX` a handler; `const { a, b } = e` as one `let` plus field reads.
  /// A function value, `useCallback` included, is a handler.
  fn let_stmt(&mut self, decl: &'p js::VariableDeclarator<'p>) -> Lowered<Option<Stmt>> {
    let Some(init) = decl.init.as_ref() else {
      let js::BindingPattern::BindingIdentifier(name) = &decl.id else { return Err(self.lowerer.residue(decl.span, "a destructuring without a value")) };
      let local = name.name.to_string();
      self.lowerer.scope.push((local.clone(), Expr::Var(local.clone())));
      return Ok(Some(Stmt::Let { name: local, expr: Expr::Lit(Lit::Undefined) }));
    };
    if let Some(call) = awaited_import(init) {
      self.template_import(decl, call)?;
      return Ok(None);
    }
    if let Some(("useAnimation", call)) = library_hook(&self.meanings(), init) {
      return self.animation_hook(call, &decl.id, decl.span).map(Some);
    }
    match &decl.id {
      js::BindingPattern::BindingIdentifier(name) => {
        let local = name.name.to_string();
        if self.lowerer.bind_formatter(&local, init) {
          return Ok(None);
        }
        let expr = match hook_call(&self.meanings(), init) {
          Some((hook, call)) if FSR_HOOKS.contains(&hook) && self.foreign_callee(call) => {
            return Err(self.lowerer.residue(call.span, format!("`{hook}`, a hook the build cannot follow")));
          }
          Some(("useCallback", call)) => {
            if let Some(js::Expression::ArrowFunctionExpression(arrow)) = call.arguments.first().map(|a| crate::syntax::arg_expr(a)) {
              self.handler_fns.insert(local.clone(), (crate::params_of(&arrow.params), arrow_body(arrow)).into());
            }
            if matches!(call.arguments.get(1).map(|a| crate::syntax::arg_expr(a)), Some(js::Expression::ArrayExpression(deps)) if deps.elements.iter().all(|d| self.never_changes(d))) {
              self.stable_callbacks.push(local.clone());
            }
            self.browser_function(local);
            return Ok(None);
          }
          Some(("useMemo", call)) => {
            let Some(js::Expression::ArrowFunctionExpression(arrow)) = call.arguments.first().map(|a| crate::syntax::arg_expr(a)) else {
              return Err(self.lowerer.residue(decl.span, "`useMemo` of something other than an arrow"));
            };
            match crate::syntax::arrow_body_of(arrow) {
              crate::syntax::ArrowBody::Expr(e) => self.lowerer.expr(e)?,
              crate::syntax::ArrowBody::Block(b) => block_to_expr(&mut self.lowerer, &b.body)?,
            }
          }
          Some(("useRef", call)) => {
            let current = match call.arguments.first() {
              Some(a) => self.lowerer.expr(crate::syntax::arg_expr(a))?,
              None => Expr::Lit(Lit::Null),
            };
            self.state.push(local.clone());
            let mut fields = vec![Entry::Field("current".to_owned(), current)];
            if self.lowerer.reaches(Reach::Terminal) {
              self.state_bindings.push(local.clone());
              self.held_refs.push(local.clone());
              let range = self.lowerer.parsed.range(call.span);
              if let Some(candidates) = &mut self.lowerer.hoisting {
                fields.push(Entry::Field(crate::REF_ID.to_owned(), candidates.id(range)));
              }
            }
            Expr::Object(fields)
          }
          Some(("useStore", _)) => {
            return Err(self.lowerer.residue(decl.span, "`useStore` bound to one name; it is a pair, as `const [x, setX] = useStore(key, initial)`"))
          }
          Some(("useLocale", _)) => Expr::Locale,
          Some(("useEffectEvent", call)) => {
            if let Some(js::Expression::ArrowFunctionExpression(arrow)) = call.arguments.first().map(|a| crate::syntax::arg_expr(a)) {
              self.handler_fns.insert(local.clone(), (crate::params_of(&arrow.params), arrow_body(arrow)).into());
            }
            self.browser_function(local);
            return Ok(None);
          }
          Some(("useLinkStatus", call)) => {
            let expr = self.lowerer.server_hook_value("useLinkStatus", call)?;
            self.state.push(local.clone());
            self.state_bindings.push(local.clone());
            self.lowerer.scope.push((local.clone(), Expr::Var(local.clone())));
            return Ok(Some(Stmt::Let { name: local, expr }));
          }
          Some((hook @ ("useDeferredValue" | "useFormStatus" | "useSyncExternalStore"), call)) => self.lowerer.server_hook_value(hook, call)?,
          Some(("useId", call)) if !matches!(self.meanings().bindings.get("useId"), Some(crate::meaning::Meaning::Own { .. })) => {
            let range = self.lowerer.parsed.range(call.span);
            let viewer = self.lowerer.reaches(Reach::Terminal);
            match &mut self.lowerer.hoisting {
              Some(candidates) => candidates.id(range),
              // A terminal page has no browser copy whose ids must match, so a hook's own id is the component's to number.
              None if viewer => {
                self.hook_ids += 1;
                Expr::HoistId(HOOK_IDS + self.hook_ids)
              }
              None => return Err(self.lowerer.residue_with(decl.span, "`useId` inside a custom hook", "the id is keyed by the component that calls the hook; call `useId` there and pass the id in")),
            }
          }
          Some((hook, call)) if self.custom_hook(hook) => return self.inline_hook(hook, call, Some(&decl.id)),
          Some((hook, _)) if !matches!(hook, "useState" | "use" | "useContext") => return Err(self.lowerer.residue(decl.span, format!("`{hook}`"))),
          _ if matches!(init, js::Expression::ArrowFunctionExpression(_) | js::Expression::FunctionExpression(_)) => {
            match init {
              js::Expression::ArrowFunctionExpression(arrow) => {
                self.handler_fns.insert(local.clone(), (crate::params_of(&arrow.params), arrow_body(arrow)).into());
                if self.lowerer.reaches(Reach::Terminal) {
                  let unbound = self.lowerer.unbound.take();
                  match self.lowerer.lambda(arrow) {
                    Ok(f) => {
                      self.lowerer.scope.push((local.clone(), f));
                      return Ok(None);
                    }
                    Err(residue) if self.unbound_is_followable() => return Err(residue),
                    Err(_) => self.lowerer.unbound = unbound,
                  }
                }
              }
              js::Expression::FunctionExpression(f) => {
                if let Some(body) = &f.body {
                  self.handler_fns.insert(local.clone(), (crate::params_of(&f.params), FunctionBody::Block(&body.statements)).into());
                }
              }
              _ => {}
            }
            self.browser_function(local);
            return Ok(None);
          }
          _ if functions_only(init) => match self.lowerer.expr(init) {
            Ok(expr) => expr,
            Err(residue) if self.unbound_is_followable() => return Err(residue),
            Err(_) => {
              self.lowerer.unbound = None;
              let js::Expression::ObjectExpression(obj) = init else { unreachable!("checked") };
              let mut entries = Vec::new();
              for prop in &obj.properties {
                let js::ObjectPropertyKind::ObjectProperty(p) = prop else { unreachable!("checked") };
                let crate::syntax::Prop::KeyValue(kv) = crate::syntax::prop_of(p) else { unreachable!("checked") };
                let key = prop_name(&kv.key, kv.computed).ok_or_else(|| self.lowerer.residue(kv.key.span(), "a numeric property name"))?;
                let value = match self.handler_body(&kv.value) {
                  Ok(body) => {
                    self.lowered_handlers.push(Handler { event: format!("{local}.{key}"), body });
                    Expr::Handler((self.lowered_handlers.len() - 1) as u32)
                  }
                  Err(_) => {
                    self.lowerer.unbound = None;
                    Expr::Handler(snapfire_fsr_ir::render::BROWSER_HANDLER)
                  }
                };
                entries.push(Entry::Field(key, value));
              }
              self.lowerer.scope.push((local.clone(), Expr::Object(entries.clone())));
              return Ok(Some(Stmt::Let { name: local, expr: Expr::Object(entries) }));
            }
          },
          _ => {
            self.lowerer.browser_value = None;
            match self.lowerer.expr(init) {
              Ok(expr) => expr,
              Err(residue) => match self.lowerer.browser_value.take() {
                Some(reason) => {
                  let (line, column) = self.lowerer.parsed.loc(init.span().start);
                  self.browser_texts.push(BrowserText { wrap: Some(self.lowerer.parsed.range(init.span())), line, column, reason });
                  self.lowerer.scope.push((local.clone(), Expr::Var(local.clone())));
                  self.lowerer.kinds.push((local.clone(), Expr::Var(local.clone()), crate::Kind::Browser));
                  return Ok(Some(Stmt::Let { name: local, expr: Expr::Lit(Lit::Null) }));
                }
                None => return Err(residue),
              },
            }
          }
        };
        self.lowerer.scope.push((local.clone(), Expr::Var(local.clone())));
        self.lowerer.note_kind(&local, init);
        Ok(Some(Stmt::Let { name: local, expr }))
      }
      js::BindingPattern::ArrayPattern(arr) => {
        let called = hook_call(&self.meanings(), init).map(|(name, _)| name).unwrap_or_default();
        let js::Expression::CallExpression(call) = init else { return self.pattern_stmt(&decl.id, init) };
        if called == "useStore" {
          return self.store_stmt(decl, arr, call);
        }
        if called == "useReducer" {
          return self.reducer_stmt(arr, call);
        }
        if !called.is_empty() && called != "useState" && self.custom_hook(called) {
          return self.inline_hook(called, call, Some(&decl.id));
        }
        if matches!(called, "useTransition" | "useActionState" | "useOptimistic") {
          return self.pending_hook_stmt(called, arr, call);
        }
        if called != "useState" {
          if !called.is_empty() {
            return Err(self.lowerer.residue(decl.span, format!("`{called}`")));
          }
          return self.pattern_stmt(&decl.id, init);
        }
        self.lowerer.browser_value = None;
        let initial = match call.arguments.first() {
          Some(a) => match crate::syntax::arg_expr(a) {
            js::Expression::ArrowFunctionExpression(arrow) => match crate::syntax::arrow_body_of(arrow) {
              crate::syntax::ArrowBody::Expr(e) => self.lowerer.expr(e),
              crate::syntax::ArrowBody::Block(b) => block_to_expr(&mut self.lowerer, &b.body),
            },
            member_expr if member_expr.as_member_expression().is_some_and(|member| is_date_now(member)) => { let member = member_expr.as_member_expression().expect("matched as a member"); self.lowerer.date_now(member.span()) },
            e => self.lowerer.expr(e),
          },
          None => Ok(Expr::Lit(Lit::Null)),
        };
        // State the browser alone can start, `useState(() => new Date())`: null on the server and a browser value wherever it is read.
        let (expr, browser) = match initial {
          Ok(expr) => (expr, false),
          Err(residue) => match self.lowerer.browser_value.take() {
            Some(_) => (Expr::Lit(Lit::Null), true),
            None => return Err(residue),
          },
        };
        let mut names = arr.elements.iter().map(|e| match e {
          Some(js::BindingPattern::BindingIdentifier(id)) => Some(id.name.to_string()),
          _ => None,
        });
        let state = names.next().flatten();
        let setter = names.next().flatten();
        let Some(name) = state else { return Ok(None) };
        if let Some(setter) = setter {
          self.setters.push((setter.clone(), name.clone()));
          self.browser_function(setter);
        }
        self.state.push(name.clone());
        self.state_bindings.push(name.clone());
        self.lowerer.scope.push((name.clone(), Expr::Var(name.clone())));
        if browser {
          self.lowerer.kinds.push((name.clone(), Expr::Var(name.clone()), crate::Kind::Browser));
        } else if let Some(initial) = call.arguments.first() {
          self.lowerer.note_kind(&name, crate::syntax::arg_expr(initial));
        }
        Ok(Some(Stmt::Let { name, expr }))
      }
      pattern => self.pattern_stmt(pattern, init),
    }
  }

  /// `useTransition()` as `[false, startTransition]`, `useActionState(action, initial)` as `[initial, formAction, false]` and `useOptimistic(value, update)` as `[value, addOptimistic]`, which is what React's server renderer returns; each function is the browser's.
  fn pending_hook_stmt(&mut self, hook: &str, arr: &js::ArrayPattern, call: &js::CallExpression) -> Lowered<Option<Stmt>> {
    let arg = |this: &mut Self, i: usize| -> Lowered<Expr> {
      match call.arguments.get(i) {
        Some(a) => this.lowerer.expr(crate::syntax::arg_expr(a)),
        None => Ok(Expr::Lit(Lit::Null)),
      }
    };
    let pending = Some(Expr::Lit(Lit::Bool(false)));
    let parts: Vec<Option<Expr>> = match hook {
      "useTransition" => vec![pending, None],
      "useActionState" => vec![Some(arg(self, 1)?), None, pending],
      _ => vec![Some(arg(self, 0)?), None],
    };
    let mut stmt = None;
    for (at, (elem, part)) in arr.elements.iter().zip(parts).enumerate() {
      let Some(js::BindingPattern::BindingIdentifier(id)) = elem else { continue };
      let name = id.name.to_string();
      let state = hook == "useActionState" && at == 0;
      match part {
        None => {
          if hook == "useTransition" && self.lowerer.reaches(Reach::Terminal) {
            self.transitions.push(name.clone());
          }
          self.browser_function(name)
        }
        Some(lit @ Expr::Lit(_)) if !state => self.lowerer.scope.push((name, lit)),
        Some(expr) => {
          if state {
            self.state.push(name.clone());
            self.state_bindings.push(name.clone());
          }
          self.lowerer.scope.push((name.clone(), Expr::Var(name.clone())));
          stmt = Some(Stmt::Let { name, expr });
        }
      }
    }
    Ok(stmt)
  }

  /// `const [state, dispatch] = useReducer(reducer, initial, init?)`: state the way `useState` holds it, starting at `initial` or `init(initial)`, with `dispatch` a handler the browser runs.
  fn reducer_stmt(&mut self, arr: &js::ArrayPattern, call: &js::CallExpression) -> Lowered<Option<Stmt>> {
    let initial = match call.arguments.get(1) {
      Some(a) => self.lowerer.expr(crate::syntax::arg_expr(a))?,
      None => return Err(self.lowerer.residue(call.span, "`useReducer` without an initial state")),
    };
    let expr = match call.arguments.get(2).map(|a| crate::syntax::arg_expr(a)) {
      None => initial,
      Some(js::Expression::ArrowFunctionExpression(arrow)) => Expr::Apply { f: Box::new(self.lowerer.lambda(arrow)?), args: vec![initial] },
      Some(init) => Expr::Apply { f: Box::new(self.lowerer.expr(init)?), args: vec![initial] },
    };
    let mut names = arr.elements.iter().map(|e| match e {
      Some(js::BindingPattern::BindingIdentifier(id)) => Some(id.name.to_string()),
      _ => None,
    });
    let state = names.next().flatten();
    let dispatch = names.next().flatten();
    if let Some(dispatch) = &dispatch {
      self.browser_function(dispatch.clone());
    }
    let Some(name) = state else { return Ok(None) };
    if let (Some(dispatch), true) = (dispatch, self.lowerer.reaches(Reach::Terminal)) {
      let reducer = match call.arguments.first().map(|a| crate::syntax::arg_expr(a)) {
        Some(js::Expression::ArrowFunctionExpression(arrow)) => self.lowerer.lambda(arrow)?,
        Some(reducer) => self.lowerer.expr(reducer)?,
        None => return Err(self.lowerer.residue(call.span, "`useReducer` without a reducer")),
      };
      self.dispatchers.push((dispatch, name.clone(), reducer));
    }
    self.state.push(name.clone());
    self.state_bindings.push(name.clone());
    self.lowerer.scope.push((name.clone(), Expr::Var(name.clone())));
    Ok(Some(Stmt::Let { name, expr }))
  }

  /// A destructuring in a component body: the value under a temporary each name reads a part of.
  fn pattern_stmt(&mut self, pattern: &'p js::BindingPattern<'p>, init: &'p js::Expression<'p>) -> Lowered<Option<Stmt>> {
    if let (Some(_), js::BindingPattern::ObjectPattern(obj)) = (self.class, pattern) {
      if is_this_props(init) {
        self.children_name = crate::syntax::pat_props(obj).into_iter().find_map(|prop| match prop {
          crate::syntax::PatProp::Assign(a) if a.key.name.as_str() == "children" => Some("children".to_owned()),
          crate::syntax::PatProp::KeyValue(kv) if prop_name(&kv.key, kv.computed).as_deref() == Some("children") => match &kv.value {
            js::BindingPattern::BindingIdentifier(id) => Some(id.name.to_string()),
            _ => None,
          },
          _ => None,
        }).or(self.children_name.take());
      }
    }
    let expr = match hook_call(&self.meanings(), init) {
      Some((hook, call)) if FSR_HOOKS.contains(&hook) && self.foreign_callee(call) => {
        return Err(self.lowerer.residue(call.span, format!("`{hook}`, a hook the build cannot follow")));
      }
      Some((hook, call)) if self.custom_hook(hook) => return self.inline_hook(hook, call, Some(pattern)),
      Some(("useLinkStatus", call)) => {
        let expr = self.lowerer.server_hook_value("useLinkStatus", call)?;
        let name = self.lowerer.temp();
        self.state.push(name.clone());
        self.state_bindings.push(name.clone());
        bind_pattern(&mut self.lowerer, pattern, Expr::Var(name.clone()))?;
        return Ok(Some(Stmt::Let { name, expr }));
      }
      Some(("useFormStatus", call)) => {
        // Each field is a constant on the server, bound as one so a read of `data` is known to be null.
        let Expr::Object(fields) = self.lowerer.server_hook_value("useFormStatus", call)? else { unreachable!("useFormStatus is an object") };
        let js::BindingPattern::ObjectPattern(obj) = pattern else { return Err(self.lowerer.residue(pattern.span(), "`useFormStatus` destructured as other than an object")) };
        for prop in crate::syntax::pat_props(obj) {
          let (key, local) = match prop {
            crate::syntax::PatProp::Assign(a) => (a.key.name.to_string(), a.key.name.to_string()),
            crate::syntax::PatProp::KeyValue(kv) => match (prop_name(&kv.key, kv.computed), &kv.value) {
              (Some(key), js::BindingPattern::BindingIdentifier(id)) => (key, id.name.to_string()),
              _ => return Err(self.lowerer.residue(prop.span(), "a `useFormStatus` field bound to other than a name")),
            },
            crate::syntax::PatProp::Rest(r) => return Err(self.lowerer.residue(r.span, "a rest of `useFormStatus`")),
          };
          let value = fields.iter().find_map(|f| match f {
            Entry::Field(n, v) if *n == key => Some(v.clone()),
            _ => None,
          }).unwrap_or(Expr::Lit(Lit::Null));
          self.lowerer.scope.push((local, value));
        }
        return Ok(None);
      }
      Some(("useContext" | "use", _)) => self.lowerer.expr(init)?,
      Some((hook @ ("useDeferredValue" | "useSyncExternalStore"), call)) => self.lowerer.server_hook_value(hook, call)?,
      Some((hook, _)) => return Err(self.lowerer.residue(pattern.span(), format!("`{hook}`"))),
      None => self.lowerer.expr(init)?,
    };
    let name = self.lowerer.temp();
    bind_pattern(&mut self.lowerer, pattern, Expr::Var(name.clone()))?;
    Ok(Some(Stmt::Let { name, expr }))
  }

  /// `const [x, setX] = useStore(key, initial)` as `let x = <store key> ?? initial`,
  /// with `setX` a handler. The key must lower to a string: a literal or a
  /// `key()` from the client's store, wherever it is declared.
  fn store_stmt(&mut self, decl: &js::VariableDeclarator, arr: &js::ArrayPattern, call: &js::CallExpression) -> Lowered<Option<Stmt>> {
    let Some(first) = call.arguments.first() else {
      return Err(self.lowerer.residue(decl.span, "`useStore` without a key"));
    };
    let key = match self.lowerer.expr(crate::syntax::arg_expr(first))? {
      Expr::Lit(Lit::Str(key)) => key,
      _ => return Err(self.lowerer.residue(crate::syntax::arg_expr(first).span(), "a `useStore` key that is not a string the build can read")),
    };
    let initial = match call.arguments.get(1) {
      Some(a) => self.lowerer.expr(crate::syntax::arg_expr(a))?,
      None => Expr::Lit(Lit::Null),
    };
    let mut names = arr.elements.iter().map(|e| match e {
      Some(js::BindingPattern::BindingIdentifier(id)) => Some(id.name.to_string()),
      _ => None,
    });
    let held = names.next().flatten();
    let setter = names.next().flatten();
    let Some(name) = held else { return Ok(None) };
    if let Some(setter) = setter {
      self.setters.push((setter.clone(), name.clone()));
      self.browser_function(setter);
    }
    self.state.push(name.clone());
    self.state_bindings.push(name.clone());
    self.store_bindings.push((name.clone(), key.clone()));
    self.lowerer.scope.push((name.clone(), Expr::Var(name.clone())));
    Ok(Some(Stmt::Let { name, expr: Expr::Coalesce(Box::new(Expr::Store(key)), Box::new(initial)) }))
  }

  /// A JSX child or a component's return: a tree when the expression holds
  /// JSX anywhere a render would reach, text otherwise.
  fn child_expr(&mut self, expr: &'p js::Expression<'p>) -> Lowered<Tmpl> {
    self.premake_children(expr)?;
    if let Some(name) = self.slot_of_expr(expr) {
      return Ok(self.slot(&name));
    }
    match expr {
      js::Expression::ParenthesizedExpression(p) => self.child_expr(&p.expression),
      js::Expression::JSXElement(el) => self.element(el, false),
      js::Expression::JSXFragment(frag) => Ok(Tmpl::Fragment(self.children(&frag.children)?)),
      js::Expression::StringLiteral(s) => Ok(Tmpl::Text(crate::lossy(s.value))),
      js::Expression::ConditionalExpression(c) if self.holds_markup(&c.consequent) || self.holds_markup(&c.alternate) => {
        let cond = self.lowerer.expr(&c.test)?;
        let then = Box::new(self.child_expr(&c.consequent)?);
        let r#else = Some(Box::new(self.child_expr(&c.alternate)?));
        Ok(Tmpl::If { cond, then, r#else })
      }
      js::Expression::LogicalExpression(bin) if bin.operator == js::LogicalOperator::Coalesce && self.layout_root && self.slot_of_expr(&bin.left).is_some() => {
        let name = self.slot_of_expr(&bin.left).expect("checked by the guard");
        let fallback = self.child_expr(&bin.right)?;
        Ok(self.slot_with_fallback(&name, vec![fallback]))
      }
      js::Expression::LogicalExpression(bin) if bin.operator == js::LogicalOperator::And && self.holds_markup(&bin.right) => {
        let cond = self.lowerer.expr(&bin.left)?;
        let then = Box::new(self.child_expr(&bin.right)?);
        // `c && <b />` is `c` when `c` is falsy, so `0` and `NaN` print as React prints them and `false`, `null` and `""` print nothing.
        let r#else = Some(Box::new(Tmpl::Expr(cond.clone())));
        Ok(Tmpl::If { cond, then, r#else })
      }
      js::Expression::CallExpression(call) if map_call(call).is_some_and(|(_, arrow)| self.arrow_markup(arrow)) => {
        let (over, arrow) = map_call(call).expect("checked by the guard");
        self.map_tree(over, arrow)
      }
      js::Expression::ChainExpression(_) if optional_map(expr).is_some_and(|(_, arrow)| self.arrow_markup(arrow)) => {
        let (over, arrow) = optional_map(expr).expect("checked by the guard");
        let missing = Expr::Builtin { name: snapfire_fsr_ir::ast::Builtin::IsNullish, args: vec![self.lowerer.expr(over)?] };
        let each = self.map_tree(over, arrow)?;
        Ok(Tmpl::If { cond: missing, then: Box::new(Tmpl::Fragment(Vec::new())), r#else: Some(Box::new(each)) })
      }
      js::Expression::CallExpression(_) if self.markup_call(expr).is_some() => {
        let (call, name, params, body) = self.markup_call(expr).expect("checked by the guard");
        self.inline_markup_fn(call, name, &params, body)
      }
      js::Expression::ArrayExpression(array) if self.holds_markup(expr) => {
        let mut items = Vec::new();
        for item in array_items(array) {
          items.push(self.child_expr(item)?);
        }
        Ok(fills::items(items))
      }
      js::Expression::Identifier(id) if id.name.as_str() == "null" || id.name.as_str() == "undefined" => Ok(Tmpl::Fragment(Vec::new())),
      js::Expression::NullLiteral(_) => Ok(Tmpl::Fragment(Vec::new())),
      other => {
        self.premake_expr(other)?;
        Ok(Tmpl::Expr(self.lowerer.expr(other)?))
      }
    }
  }

  /// `over.map(arrow)` where `arrow` returns markup: a `Tmpl::For` over the items.
  fn map_tree(&mut self, over: &'p js::Expression<'p>, arrow: &'p js::ArrowFunctionExpression<'p>) -> Lowered<Tmpl> {
    let over = self.lowerer.expr(over)?;
    let depth = self.lowerer.scope.len();
    let params = bind_params(&mut self.lowerer, &crate::params_of(&arrow.params))?;
    let range = self.lowerer.parsed.range(arrow.span);
    if let Some(candidates) = &mut self.lowerer.hoisting {
      candidates.open_loops.push(range);
    }
    let body = match crate::syntax::arrow_body_of(arrow) {
      crate::syntax::ArrowBody::Expr(e) => self.child_expr(e),
      crate::syntax::ArrowBody::Block(b) => self.tree_of(&b.body.iter().collect::<Vec<_>>(), depth),
    };
    if let Some(candidates) = &mut self.lowerer.hoisting {
      candidates.open_loops.pop();
    }
    self.lowerer.scope.truncate(depth);
    Ok(Tmpl::For { over, params, body: Box::new(body?) })
  }

  /// Whether a child expression is markup: JSX or a call to a function the component declares that returns some, alone or inside a branch or `&&`.
  fn holds_markup(&self, expr: &'p js::Expression<'p>) -> bool {
    match expr {
      js::Expression::ParenthesizedExpression(p) => self.holds_markup(&p.expression),
      js::Expression::ConditionalExpression(c) => self.holds_markup(&c.consequent) || self.holds_markup(&c.alternate),
      js::Expression::LogicalExpression(b) if b.operator == js::LogicalOperator::And => self.holds_markup(&b.right),
      js::Expression::ArrayExpression(array) => array_items(array).any(|item| self.holds_markup(item)),
      js::Expression::JSXElement(_) | js::Expression::JSXFragment(_) => true,
      js::Expression::CallExpression(call) if map_call(call).is_some_and(|(_, arrow)| self.arrow_markup(arrow)) => true,
      js::Expression::ChainExpression(_) if optional_map(expr).is_some_and(|(_, arrow)| self.arrow_markup(arrow)) => true,
      other => self.markup_call(other).is_some(),
    }
  }

  /// Each `Children.toArray(children)`, `Children.map(children, f)` and `Children.only(children)` under `e` over the component's own children, made ahead as a read of `render::CHILD_VALUES`, which the placement passes.
  fn premake_children(&mut self, e: &'p js::Expression<'p>) -> Lowered<()> {
    match e {
      js::Expression::CallExpression(call) if matches!(&call.callee, callee if self.slot_of_expr(callee).as_deref() == Some("content")) => {
        let mut args = Vec::with_capacity(call.arguments.len());
        for arg in &call.arguments {
          self.premake_children(crate::syntax::arg_expr(arg))?;
          args.push(self.lowerer.expr(crate::syntax::arg_expr(arg))?);
        }
        let first = Expr::Builtin { name: Builtin::At, args: vec![Expr::Var(snapfire_fsr_ir::render::CHILD_VALUES.to_owned()), Expr::Lit(Lit::Int(0))] };
        let span = call.span;
        self.lowerer.premade.insert((crate::lo(span), crate::hi(span)), Expr::Apply { f: Box::new(first), args });
        if !self.counts_children.contains(&snapfire_fsr_ir::render::CHILD_VALUES) {
          self.counts_children.push(snapfire_fsr_ir::render::CHILD_VALUES);
        }
        Ok(())
      }
      js::Expression::CallExpression(call) => {
        if let Some((method, rest)) = self.own_children_call(call) {
          let values = Expr::Var(snapfire_fsr_ir::render::CHILD_VALUES.to_owned());
          let made = match (method, rest) {
            ("toArray", []) => Some(values),
            ("only", []) => Some(Expr::Builtin { name: Builtin::At, args: vec![values, Expr::Lit(Lit::Int(0))] }),
            ("map", [f]) => {
              self.premake_expr(crate::syntax::arg_expr(f))?;
              Some(Expr::Map(Box::new(values), Box::new(self.lowerer.expr(crate::syntax::arg_expr(f))?)))
            }
            _ => None,
          };
          if let Some(made) = made {
            let span = call.span;
            self.lowerer.premade.insert((crate::lo(span), crate::hi(span)), made);
            if !self.counts_children.contains(&snapfire_fsr_ir::render::CHILD_VALUES) {
              self.counts_children.push(snapfire_fsr_ir::render::CHILD_VALUES);
            }
            return Ok(());
          }
        }
        if let Some(member) = call.callee.as_member_expression() {
          self.premake_children(member.object())?;
        }
        call.arguments.iter().try_for_each(|arg| self.premake_children(crate::syntax::arg_expr(arg)))
      }
      member_expr if member_expr.is_member_expression() => { let member = member_expr.as_member_expression().expect("matched as a member"); self.premake_children(member.object()) },
      js::Expression::ParenthesizedExpression(p) => self.premake_children(&p.expression),
      js::Expression::TSAsExpression(a) => self.premake_children(&a.expression),
      js::Expression::ConditionalExpression(c) => {
        self.premake_children(&c.test)?;
        self.premake_children(&c.consequent)?;
        self.premake_children(&c.alternate)
      }
      js::Expression::BinaryExpression(b) => {
        self.premake_children(&b.left)?;
        self.premake_children(&b.right)
      }
      _ => Ok(()),
    }
  }

  /// `Children.<method>(children, ...rest)` over the component's own children.
  fn own_children_call<'c, 'e>(&self, call: &'c js::CallExpression<'e>) -> Option<(&'static str, &'c [js::Argument<'e>])> {
    let callee = &call.callee;
    let Some(member) = (callee).as_member_expression() else { return None };
    if self.meanings().of_expr(member.object()) != Some(crate::meaning::Meaning::Library(crate::meaning::Construct::Children)) {
      return None;
    }
    let (first, rest) = call.arguments.split_first()?;
    if crate::syntax::is_spread(first) || self.slot_of_expr(crate::syntax::arg_expr(first)).as_deref() != Some("content") {
      return None;
    }
    let method = match prop_name_of(member).as_deref()? {
      "toArray" => "toArray",
      "only" => "only",
      "map" => "map",
      _ => return None,
    };
    Some((method, rest))
  }

  /// `Children.count(children)` (`render::CHILD_COUNT`) or `Children.toArray(children).length` (`render::CHILD_ITEMS`) over the component's own children.
  fn counts_own_children(&self, e: &js::Expression) -> Option<&'static str> {
    let children_call = |call: &js::CallExpression, method: &str| -> bool {
      let callee = &call.callee;
      let Some(member) = (callee).as_member_expression() else { return false };
      prop_name_of(member).as_deref() == Some(method)
        && self.meanings().of_expr(member.object()) == Some(crate::meaning::Meaning::Library(crate::meaning::Construct::Children))
        && matches!(call.arguments.as_slice(), [arg] if !crate::syntax::is_spread(arg) && self.slot_of_expr(crate::syntax::arg_expr(arg)).as_deref() == Some("content"))
    };
    match unwrap_literal(e) {
      js::Expression::CallExpression(call) if children_call(call, "count") => Some(snapfire_fsr_ir::render::CHILD_COUNT),
      other if other.as_member_expression().is_some_and(|member| prop_name_of(member).as_deref() == Some("length") && matches!(unwrap_literal(member.object()), js::Expression::CallExpression(call) if children_call(call, "toArray"))) => Some(snapfire_fsr_ir::render::CHILD_ITEMS),
      _ => None,
    }
  }

  /// Whether an arrow returns markup: its expression holds some or its block returns some.
  fn arrow_markup(&self, arrow: &'p js::ArrowFunctionExpression<'p>) -> bool {
    match crate::syntax::arrow_body_of(arrow) {
      crate::syntax::ArrowBody::Expr(e) => self.holds_markup(e),
      crate::syntax::ArrowBody::Block(b) => self.returns_markup(&b.body),
    }
  }

  /// Whether a function body returns markup on some path: a `return` of markup or of a local a declaration in the body made markup, inside a branch, a block or a `switch` arm included.
  fn returns_markup(&self, stmts: &'p [js::Statement]) -> bool {
    let mut locals = Vec::new();
    self.stmts_return_markup(stmts, &mut locals)
  }

  fn stmts_return_markup(&self, stmts: &'p [js::Statement], locals: &mut Vec<String>) -> bool {
    stmts.iter().any(|stmt| self.stmt_returns_markup(stmt, locals))
  }

  fn stmt_returns_markup(&self, stmt: &'p js::Statement<'p>, locals: &mut Vec<String>) -> bool {
    match stmt {
      js::Statement::VariableDeclaration(var) => {
        for decl in &var.declarations {
          if let (js::BindingPattern::BindingIdentifier(id), Some(init)) = (&decl.id, decl.init.as_ref()) {
            if self.holds_markup(init) {
              locals.push(id.name.to_string());
            }
          }
        }
        false
      }
      js::Statement::ReturnStatement(r) => r.argument.as_ref().is_some_and(|arg| self.holds_markup(arg) || matches!(unwrap_literal(arg), js::Expression::Identifier(id) if locals.iter().any(|l| l == id.name.as_str()))),
      js::Statement::BlockStatement(b) => self.stmts_return_markup(&b.body, locals),
      js::Statement::IfStatement(branch) => self.stmt_returns_markup(&branch.consequent, locals) || branch.alternate.as_ref().is_some_and(|alt| self.stmt_returns_markup(alt, locals)),
      js::Statement::SwitchStatement(sw) => sw.cases.iter().any(|case| self.stmts_return_markup(&case.consequent, locals)),
      _ => false,
    }
  }

  /// A call by name to a function the component or its module declares, an arrow `const` or a `function`, whose body returns markup on some path.
  fn markup_call(&self, expr: &'p js::Expression<'p>) -> Option<(&'p js::CallExpression<'p>, String, Vec<crate::Param<'p>>, FunctionBody<'p>)> {
    let js::Expression::CallExpression(call) = expr else { return None };
    let callee = &call.callee;
    let js::Expression::Identifier(id) = callee else { return None };
    let name = id.name.to_string();
    let (params, body) = match self.handler_fns.get(&name) {
      Some(declared) => (declared.params.clone(), declared.body),
      None if !self.lowerer.scope.iter().any(|(n, _)| *n == name) => match find_function(self.lowerer.parsed, &name)? {
        Found::Declared(params, stmts, _) => (crate::params_of(params), FunctionBody::Block(stmts)),
        Found::Arrow(arrow) => (crate::params_of(&arrow.params), arrow_body(arrow)),
      },
      None => return None,
    };
    if self.checking.borrow().contains(&name) {
      return None;
    }
    self.checking.borrow_mut().push(name.clone());
    let markup = match body {
      FunctionBody::Expr(e) => self.holds_markup(e),
      FunctionBody::Block(stmts) => self.returns_markup(stmts),
    };
    self.checking.borrow_mut().pop();
    markup.then_some((call, name, params, body))
  }

  /// A call of a markup helper inside its own body, as a placement of the helper: a helper taking one object pattern is handed the argument as its props, any other its arguments by position. A helper declared inside the component closes over its locals and is refused.
  fn place_recursion(&mut self, call: &'p js::CallExpression<'p>, name: String, params: &[crate::Param]) -> Lowered<Tmpl> {
    if self.handler_fns.contains_key(&name) {
      return Err(self.lowerer.residue(call.span, format!("`{name}` returns markup by calling itself inside the component; declare it at module level so the build can place it")));
    }
    if let Some(spread) = call.arguments.iter().find(|a| crate::syntax::is_spread(a)) {
      return Err(self.lowerer.residue(crate::syntax::arg_expr(spread).span(), format!("a spread argument to `{name}`, which returns markup")));
    }
    let by_props = matches!(params, [crate::Param { pattern: js::BindingPattern::ObjectPattern(_), default: None, rest: false, .. }]);
    let mut props = Vec::with_capacity(params.len());
    for i in 0..params.len() {
      let value = match call.arguments.get(i) {
        Some(arg) => self.lowerer.expr(crate::syntax::arg_expr(arg))?,
        None => Expr::Lit(Lit::Undefined),
      };
      props.push(match by_props {
        true => Entry::Spread(value),
        false => Entry::Field(i.to_string(), value),
      });
    }
    if !by_props && !self.positional_refs.contains(&name) {
      self.positional_refs.push(name.clone());
    }
    let loc = self.lowerer.parsed.loc(call.span.start);
    self.refs.push((name.clone(), loc));
    Ok(Tmpl::Component { module: format!("{}#{name}", self.file), props, children: Vec::new(), id: 0, keyed: false })
  }

  /// A function returning markup, inlined where it is called: each argument is lowered in the caller's scope and bound to its parameter by a `Tmpl::Let` around the function's tree.
  fn inline_markup_fn(&mut self, call: &'p js::CallExpression<'p>, name: String, params: &[crate::Param], body: FunctionBody<'p>) -> Lowered<Tmpl> {
    if self.inlining.contains(&name) {
      return self.place_recursion(call, name, params);
    }
    if let Some(spread) = call.arguments.iter().find(|a| crate::syntax::is_spread(a)) {
      return Err(self.lowerer.residue(crate::syntax::arg_expr(spread).span(), format!("a spread argument to `{name}`, which returns markup")));
    }
    let mut args = Vec::with_capacity(params.len());
    for i in 0..params.len() {
      args.push(match call.arguments.get(i) {
        Some(arg) => self.lowerer.expr(crate::syntax::arg_expr(arg))?,
        None => Expr::Lit(Lit::Null),
      });
    }
    let depth = self.lowerer.scope.len();
    let mut lets = Vec::with_capacity(params.len());
    for (param, value) in params.iter().zip(args) {
      let held = self.lowerer.temp();
      if let Err(residue) = bind_param(&mut self.lowerer, param, Expr::Var(held.clone())) {
        self.lowerer.scope.truncate(depth);
        return Err(residue);
      }
      lets.push((held, value));
    }
    self.inlining.push(name);
    let tree = match body {
      FunctionBody::Expr(e) => self.child_expr(e),
      FunctionBody::Block(stmts) => self.tree_of(&stmts.iter().collect::<Vec<_>>(), depth),
    };
    self.inlining.pop();
    self.lowerer.scope.truncate(depth);
    let mut tree = tree?;
    for (held, value) in lets.into_iter().rev() {
      tree = Tmpl::Let { name: held, expr: value, then: Box::new(tree) };
    }
    Ok(tree)
  }

  /// `as_child` says the element sits directly among JSX children, where a
  /// rewrite of it must be braced, rather than in an expression.
  fn element(&mut self, el: &'p js::JSXElement<'p>, as_child: bool) -> Lowered<Tmpl> {
    let tagged = self.lowerer.meanings.as_ref().and_then(|meanings| meanings.of_tag(&el.opening_element.name));
    let provider = tagged.as_ref().and_then(|meant| meant.as_provider().map(|(id, react)| (id.to_owned(), react)));
    let named_provider = match &el.opening_element.name {
      js::JSXElementName::MemberExpression(m) if m.property.name.as_str() == "Provider" => Some(jsx_object_name(&m.object)),
      _ => None,
    };
    if provider.is_some() || named_provider.is_some() {
      let loc = self.lowerer.parsed.loc(el.span.start);
      let shown = named_provider.unwrap_or_else(|| jsx_tag_name(&el.opening_element.name));
      self.providers.push((shown, loc, provider.as_ref().map(|(_, react)| *react)));
      let children = Tmpl::Fragment(self.children(&el.children)?);
      let Some((id, _)) = provider else { return Ok(children) };
      let mut value = Expr::Lit(Lit::Null);
      for attr in &el.opening_element.attributes {
        if let js::JSXAttributeItem::Attribute(attr) = attr {
          if attr_name(&attr.name) == "value" {
            value = self.attr_value(attr)?;
          }
        }
      }
      return Ok(Tmpl::Let { name: format!("{CONTEXT_PREFIX}{id}"), expr: value, then: Box::new(children) });
    }
    let (name, member) = match &el.opening_element.name {
      js::JSXElementName::IdentifierReference(id) => (id.name.to_string(), false),
      js::JSXElementName::Identifier(id) => (id.name.to_string(), false),
      js::JSXElementName::ThisExpression(_) => ("this".to_owned(), false),
      js::JSXElementName::MemberExpression(m) => match &m.object {
        js::JSXMemberExpressionObject::IdentifierReference(obj) => (format!("{}.{}", obj.name, m.property.name), true),
        js::JSXMemberExpressionObject::ThisExpression(_) => (format!("this.{}", m.property.name), true),
        js::JSXMemberExpressionObject::MemberExpression(_) => return Err(self.lowerer.residue(m.span, "a member expression as a tag more than one level deep")),
      },
      js::JSXElementName::NamespacedName(n) => return Err(self.lowerer.residue(n.span, "a namespaced tag")),
    };
    let chosen_tag = match &el.opening_element.name {
      js::JSXElementName::IdentifierReference(id) if !member && self.chosen.get(&name).is_none() => {
        let meanings = self.meanings();
        let local = !matches!(meanings.ctxt(id), crate::Ctxt::Top | crate::Ctxt::Unresolved);
        local.then(|| self.lowerer.scope.iter().rev().find(|(n, _)| *n == name).map(|(_, e)| e.clone())).flatten()
      }
      _ => None,
    };
    let is_component = (member || name.chars().next().is_some_and(|c| c.is_ascii_uppercase())) && chosen_tag.is_none();
    if is_component {
      return self.component_ref(&name, el, as_child);
    }
    let name = if chosen_tag.is_some() { fills::CHOSEN_TAG.to_owned() } else { name };
    if name == "img" && self.rewrite_images && self.img_names_an_asset(el) {
      return self.picture_element(el, true);
    }
    let mut attrs = Vec::new();
    let mut select_value = None;
    let mut bound = false;
    let mut escaped_asset: Option<String> = None;
    let (mut text_value, mut text_default) = (None, None);
    for attr in &el.opening_element.attributes {
      let attr = match attr {
        js::JSXAttributeItem::Attribute(attr) => attr,
        js::JSXAttributeItem::SpreadAttribute(spread) => {
          bound = true;
          attrs.push(Entry::Spread(self.lowerer.expr(&spread.argument)?));
          continue;
        }
      };
      let raw = attr_name(&attr.name);
      if raw == "ref" {
        bound = true;
        continue;
      }
      // React's own: hydration reads it, the document never does.
      if raw == "suppressHydrationWarning" {
        continue;
      }
      if name == "img" && raw == RAW_ESCAPE {
        escaped_asset = self.img_asset_path(el);
        continue;
      }
      // React 19 attaches a function an `on…` prop of a custom element holds as a listener for that event, whatever its case.
      if is_handler_name(&raw) || (name.contains('-') && raw.len() > 2 && raw.starts_with("on") && self.passes_function(attr)) {
        bound = true;
        let event = raw[2..].to_ascii_lowercase();
        if let Some(reference) = self.handler_reference(attr) {
          attrs.push(Entry::Field(format!("{HANDLER_ATTR}{event}"), reference));
          continue;
        }
        match self.handler_attr(attr) {
          Ok(index) => attrs.push(Entry::Field(format!("{HANDLER_ATTR}{event}"), Expr::Lit(Lit::Int(index as i128)))),
          Err(residue) if self.unbound_is_followable() => return Err(residue),
          Err(residue) => {
            self.lowerer.unbound = None;
            if !attrs.iter().any(|e| matches!(e, Entry::Field(n, _) if n == UNLOWERED_ATTR)) {
              attrs.push(Entry::Field(UNLOWERED_ATTR.to_owned(), Expr::lit_str(format!("{}:{}: {}", residue.line, residue.column, residue.message))));
            }
          }
        }
        continue;
      }
      if raw == "key" {
        attrs.push(Entry::Field(KEY_ATTR.to_owned(), self.attr_value(attr)?));
        continue;
      }
      if (raw == "action" || raw == "formAction") && self.passes_function(attr) {
        bound = true;
        if !attrs.iter().any(|e| matches!(e, Entry::Field(n, _) if n == UNLOWERED_ATTR)) {
          let (line, column) = self.lowerer.parsed.loc(attr.span.start);
          attrs.push(Entry::Field(UNLOWERED_ATTR.to_owned(), Expr::lit_str(format!("{line}:{column}: a form action, which the browser runs"))));
        }
        attrs.push(Entry::Field(raw, Expr::lit_str(snapfire_fsr_ir::render::FUNCTION_FORM_ACTION)));
        continue;
      }
      let value = self.attr_value(attr)?;
      match raw.as_str() {
        "dangerouslySetInnerHTML" => attrs.push(Entry::Field(RAW_ATTR.to_owned(), inner_html(value))),
        "style" => attrs.push(Entry::Field("style".to_owned(), self.style(attr)?)),
        "value" | "defaultValue" if name == "select" => select_value = Some(value),
        "value" if name == "textarea" => text_value = Some(value),
        "defaultValue" if name == "textarea" => text_default = Some(value),
        "value" if name == "option" => {
          attrs.push(Entry::Field("value".to_owned(), value.clone()));
          if let Some(selected) = self.select_value.clone() {
            let option = Expr::Str(Box::new(value));
            let listed = self.lowerer.temp();
            let lists = Expr::Some(Box::new(selected.clone()), Box::new(Expr::Lambda { params: vec![listed.clone()], body: Box::new(Expr::Compare(CompareOp::Eq, Box::new(Expr::Str(Box::new(Expr::Var(listed)))), Box::new(option.clone()))) }));
            let names = Expr::Compare(CompareOp::Eq, Box::new(option), Box::new(Expr::Str(Box::new(selected.clone()))));
            attrs.push(Entry::Field("selected".to_owned(), Expr::Ternary(Box::new(Expr::Builtin { name: Builtin::IsArray, args: vec![selected] }), Box::new(lists), Box::new(names))));
          }
        }
        _ => attrs.push(Entry::Field(html_attr_name(&raw).to_owned(), value)),
      }
    }
    if let Some(path) = escaped_asset {
      if let Some(facts) = self.assets.image(&path, &ImageRequest::default()) {
        for (name, value) in [("width", facts.width), ("height", facts.height)] {
          if !attrs.iter().any(|e| matches!(e, Entry::Field(n, _) if n == name)) {
            attrs.push(Entry::Field(name.to_owned(), Expr::Lit(Lit::Int(i128::from(value)))));
          }
        }
      }
    }
    let outer = if name == "select" { std::mem::replace(&mut self.select_value, select_value) } else { self.select_value.take() };
    let children = self.children(&el.children);
    self.select_value = outer;
    let mut children = children?;
    if children.is_empty() && self.spreads_props(&el.opening_element.attributes) {
      children.push(self.slot("content"));
    }
    let text = match (text_value, text_default) {
      (Some(value), Some(default)) => Some(Expr::Coalesce(Box::new(value), Box::new(default))),
      (value, default) => value.or(default),
    };
    if let Some(text) = text.filter(|_| children.is_empty()) {
      children.push(Tmpl::Expr(text));
    }
    if bound {
      attrs.push(Entry::Field(hoist::BOUND_ATTR.to_owned(), Expr::Lit(Lit::Bool(true))));
    }
    if let Some(candidates) = &mut self.lowerer.hoisting {
      if !children.is_empty() && el.closing_element.is_some() {
        let range = self.lowerer.parsed.range(el.span);
        let open = self.lowerer.parsed.range(el.opening_element.span);
        attrs.push(candidates.chunk(range, open, as_child));
      }
    }
    if let Some(module) = self.elements.get(&name) {
      attrs.push(Entry::Field(snapfire_fsr_ir::render::SHADOW_ATTR.to_owned(), Expr::lit_str(module.clone())));
    }
    if let Some(tag) = chosen_tag {
      attrs.insert(0, Entry::Field(fills::CHOSEN_TAG.to_owned(), tag));
    }
    Ok(Tmpl::Element { tag: name, attrs, children })
  }

  /// True when `name` is imported from the client library's React adapter.
  /// The dialect tag `name` stands for, whatever it was imported as.
  /// The placement a tag names, however the file reaches it; `None` for a component, a local among them.
  fn placement(&self, el: &js::JSXElement) -> Option<&'static str> {
    self.meanings().of_tag(&el.opening_element.name).and_then(|meant| meant.placement())
  }

  /// `<Mount module=… props=… />`: the empty region the adapter's `Mount`
  /// renders and then fills in the browser, since the island it places is
  /// named at run time and the server renders none of it.
  fn mount_element(&mut self, el: &'p js::JSXElement<'p>) -> Lowered<Tmpl> {
    let attrs = self.jsx_attrs(el)?;
    placements::mount(self, &attrs, At::Span(el.span))
  }

  /// `<Island when="visible"><Chart … /></Island>`: the one component child
  /// as an island with that timing.
  fn island_element(&mut self, el: &'p js::JSXElement<'p>) -> Lowered<Tmpl> {
    let mut when = None;
    let mut mode = None;
    let mut define = None;
    for attr in &el.opening_element.attributes {
      let js::JSXAttributeItem::Attribute(attr) = attr else { return Err(self.lowerer.residue(el.span, "a spread on `<Island>`")) };
      match attr_name(&attr.name).as_str() {
        "when" => {
          let value = self.attr_value(attr)?;
          when = Some(self.island_timing(value, attr.span)?);
        }
        "mode" => {
          let value = self.attr_value(attr)?;
          mode = self.island_mode(value, attr.span)?;
        }
        "define" => {
          let value = self.attr_value(attr)?;
          let Expr::Lit(Lit::Str(specifier)) = value else {
            return Err(self.lowerer.residue(attr.span, "`define` is the module the definition lives in, written out"));
          };
          let resolved = crate::resolve_specifier(self.file, &specifier)
            .ok_or_else(|| self.lowerer.residue(attr.span, format!("`{specifier}`, which the build cannot follow")))?;
          let has_extension = matches!(resolved.rsplit_once('.'), Some((_, ext)) if matches!(ext, "ts" | "tsx" | "js" | "mjs"));
          define = Some(match has_extension {
            true => format!("{resolved}#default"),
            false => format!("{resolved}.ts#default"),
          });
        }
        _ => return Err(self.lowerer.residue(attr.span, "`<Island>` takes `when`, `mode` and `define` and nothing else")),
      }
    }
    let mut elements = el.children.iter().filter(|c| match c {
      js::JSXChild::Text(text) => !jsx_text(text.value.as_str()).is_empty(),
      _ => true,
    });
    let (Some(js::JSXChild::Element(child)), None) = (elements.next(), elements.next()) else {
      return Err(self.lowerer.residue(el.span, "`<Island>` wraps exactly one component"));
    };
    let lowered = self.element(child, false)?;
    if let Some(module) = define {
      if mode.is_some() {
        return Err(self.lowerer.residue(el.span, "`mode` on an `<Island define>`, which mounts nothing to round-trip"));
      }
      let Tmpl::Element { .. } = &lowered else {
        return Err(self.lowerer.residue(child.span, "`<Island define>` wraps an element, since its module defines one"));
      };
      let at = self.lowerer.parsed.range(el.opening_element.name.span()).end;
      let range = self.lowerer.parsed.range(el.span);
      let id = match &mut self.lowerer.hoisting {
        Some(candidates) => candidates.island(at, range, false),
        None => 0,
      };
      return Ok(Tmpl::Island { module, props: Vec::new(), children: vec![lowered], when, mode: None, id, define: true });
    }
    self.island_of(lowered, when, mode, child.span)
  }

  /// `"server"` for an island whose events round-trip to the server; `"browser"`, the default, is `None`.
  fn island_mode(&self, value: Expr, span: Span) -> Lowered<Option<String>> {
    match value {
      Expr::Lit(Lit::Str(mode)) if mode == SERVER_MODE => Ok(Some(mode)),
      Expr::Lit(Lit::Str(mode)) if mode == "browser" => Ok(None),
      _ => Err(self.lowerer.residue(span, "an island's `mode` is \"browser\" or \"server\", written out")),
    }
  }

  /// A prop whose value is a function (an arrow, a `function` or the name of one the component declares) lowered as a handler of this component: its index or why it does not lower. `None` for a prop holding anything else.
  /// An arrow passed as a prop that is not an event, `render={(row) => <b>{row}</b>}`, as the function value it is; `None` when it is something else or lowers only as a handler.
  fn function_prop(&mut self, attr: &'p js::JSXAttribute<'p>) -> Lowered<Option<Expr>> {
    let Some(js::JSXAttributeValue::ExpressionContainer(c)) = &attr.value else { return Ok(None) };
    let Some(e) = c.expression.as_expression() else { return Ok(None) };
    let mut inner: &'p js::Expression<'p> = e;
    while let js::Expression::ParenthesizedExpression(p) = inner {
      inner = &p.expression;
    }
    if !matches!(inner, js::Expression::ArrowFunctionExpression(_)) {
      return Ok(None);
    }
    self.premake_expr(inner)?;
    match self.lowerer.expr(inner) {
      Ok(value) => Ok(Some(value)),
      Err(_) => {
        self.lowerer.unbound = None;
        Ok(None)
      }
    }
  }

  /// Records where the browser gets the function `value` passed as `prop` to the placement `module`, when the build can place it.
  fn note_function(&mut self, module: &str, prop: &str, value: &Expr, attr: &'p js::JSXAttribute<'p>) {
    let Some(js::JSXAttributeValue::ExpressionContainer(c)) = &attr.value else { return };
    let Some(e) = c.expression.as_expression() else { return };
    let mut needs = Vec::new();
    if let Some(id) = self.function_reference(e, &mut needs) {
      self.fn_refs.push(FnRef { module: module.to_owned(), prop: prop.to_owned(), value: value.clone(), id, needs, unlowered: None });
    }
  }

  /// A function this file holds that the server does not lower but the browser imports by name, as a handler value with the `UNLOWERED_ATTR` text the placement carries for it: a component that calls it only in an effect or a handler never calls it on the server. `None` when the browser cannot reach it either.
  fn unlowered_function(&mut self, module: &str, prop: &str, attr: &'p js::JSXAttribute<'p>, residue: &Residue) -> Option<(Expr, String)> {
    let Some(js::JSXAttributeValue::ExpressionContainer(c)) = &attr.value else { return None };
    let Some(e) = c.expression.as_expression() else { return None };
    if !self.names_function(e) {
      return None;
    }
    let mut needs = Vec::new();
    let Some(Ok(id)) = self.function_reference(e, &mut needs) else { return None };
    let value = Expr::Handler(snapfire_fsr_ir::render::BROWSER_HANDLER);
    let why = format!("{}:{}: {}", residue.line, residue.column, residue.message);
    self.fn_refs.push(FnRef { module: module.to_owned(), prop: prop.to_owned(), value: value.clone(), id: Ok(id), needs, unlowered: Some(why.clone()) });
    Some((value, why))
  }

  /// Whether `e` is a function written in place, a function this file declares or a conditional over those. An import is left to the build's own following of it.
  fn names_function(&self, e: &js::Expression) -> bool {
    match crate::meaning::bare(e) {
      js::Expression::ArrowFunctionExpression(_) | js::Expression::FunctionExpression(_) => true,
      js::Expression::Identifier(id) => find_function(self.lowerer.parsed, id.name.as_str()).is_some(),
      js::Expression::ConditionalExpression(c) => self.names_function(&c.consequent) && self.names_function(&c.alternate),
      _ => false,
    }
  }

  /// The module id `file#export` the browser imports the function `e` from, as an expression, else why the browser cannot reach it. An arrow or a `function` that closes over nothing of this component is hoisted into an export of this file; a function a module declares is named by its file and export; a conditional chooses between two. `None` for anything else, a function held in a local among them.
  fn function_reference(&mut self, e: &'p js::Expression<'p>, needs: &mut Vec<Need>) -> Option<Result<Expr, String>> {
    match crate::meaning::bare(e) {
      js::Expression::ArrowFunctionExpression(arrow) => Some(self.hoisted_function(arrow.span, closes_over(self.lowerer.parsed, arrow), needs)),
      js::Expression::FunctionExpression(f) => Some(self.hoisted_function(f.span, closes_over(self.lowerer.parsed, &**f), needs)),
      js::Expression::Identifier(id) => {
        let name = id.name.as_str();
        if self.lowerer.scope.iter().any(|(n, _)| n == name) || self.handler_fns.contains_key(name) || self.handlers.iter().any(|h| h == name) {
          return Some(Err(format!("and `{name}` is a function only this component knows")));
        }
        match self.meanings().bindings.get(name).cloned() {
          Some(crate::meaning::Meaning::Own { file, name: export }) => {
            if file == self.file {
              needs.push(Need::Name(export.clone()));
            }
            needs.push(Need::File(file.clone()));
            Some(Ok(Expr::lit_str(format!("{file}#{export}"))))
          }
          _ => None,
        }
      }
      js::Expression::ConditionalExpression(c) => {
        let test = match self.lowerer.expr(&c.test) {
          Ok(test) => test,
          Err(residue) => return Some(Err(format!("and the choice between them reads {}", residue.message))),
        };
        let cons = self.function_reference(&c.consequent, needs)?;
        let alt = self.function_reference(&c.alternate, needs)?;
        match (cons, alt) {
          (Ok(cons), Ok(alt)) => Some(Ok(Expr::Ternary(Box::new(test), Box::new(cons), Box::new(alt)))),
          (Err(reason), _) | (_, Err(reason)) => Some(Err(reason)),
        }
      }
      _ => None,
    }
  }

  /// The id of a function written in place, once the rewrite exports it from this file under a name made of where it starts; `local` is a name of this component it reads, which keeps the browser from importing it.
  fn hoisted_function(&mut self, span: Span, local: Option<String>, needs: &mut Vec<Need>) -> Result<Expr, String> {
    if let Some(local) = local {
      return Err(format!("and it closes over `{local}`, which only this component knows"));
    }
    let range = self.lowerer.parsed.range(span);
    let name = format!("__sfF{}", range.start);
    needs.push(Need::Export(range, name.clone()));
    needs.push(Need::File(self.file.to_owned()));
    Ok(Expr::lit_str(format!("{}#{name}", self.file)))
  }

  /// Whether an attribute's value is a function: an arrow, a `function` or a name the component declares as one.
  fn passes_function(&self, attr: &js::JSXAttribute) -> bool {
    let Some(js::JSXAttributeValue::ExpressionContainer(c)) = &attr.value else { return false };
    let Some(e) = c.expression.as_expression() else { return false };
    match crate::meaning::bare(e) {
      js::Expression::ArrowFunctionExpression(_) | js::Expression::FunctionExpression(_) => true,
      js::Expression::Identifier(id) => self.handler_fns.contains_key(id.name.as_str()) || self.browser_function_named(attr).is_some(),
      _ => false,
    }
  }

  /// The setter, dispatcher or inner function an attribute passes by name.
  fn browser_function_named(&self, attr: &js::JSXAttribute) -> Option<String> {
    let Some(js::JSXAttributeValue::ExpressionContainer(c)) = &attr.value else { return None };
    let Some(e) = c.expression.as_expression() else { return None };
    let js::Expression::Identifier(id) = crate::meaning::bare(e) else { return None };
    let name = id.name.as_str();
    (self.handlers.iter().any(|h| h == name) && !self.lowerer.scope.iter().any(|(bound, _)| bound == name)).then(|| name.to_owned())
  }

  fn handler_prop(&mut self, attr: &'p js::JSXAttribute<'p>, name: &str) -> Option<Lowered<usize>> {
    let Some(js::JSXAttributeValue::ExpressionContainer(c)) = &attr.value else { return None };
    let Some(e) = c.expression.as_expression() else { return None };
    let mut inner: &'p js::Expression<'p> = e;
    while let js::Expression::ParenthesizedExpression(p) = inner {
      inner = &p.expression;
    }
    if let js::Expression::Identifier(id) = inner {
      if let Some((_, state)) = self.setters.iter().find(|(setter, _)| *setter == id.name.as_str()).cloned() {
        self.lowered_handlers.push(Handler { event: name.to_owned(), body: vec![Stmt::Return(Expr::Object(vec![Entry::Field(state, Expr::Var("$event".to_owned()))]))] });
        return Some(Ok(self.lowered_handlers.len() - 1));
      }
    }
    let function = match inner {
      js::Expression::ArrowFunctionExpression(_) | js::Expression::FunctionExpression(_) => true,
      js::Expression::Identifier(id) => self.handler_fns.contains_key(id.name.as_str()),
      _ => false,
    };
    if !function {
      return None;
    }
    Some(self.handler_body(inner).map(|body| {
      self.lowered_handlers.push(Handler { event: name.to_owned(), body });
      self.lowered_handlers.len() - 1
    }))
  }

  /// An `on*` attribute naming a handler the component was handed rather than one it declares: a prop or a local read by name or through a member, its value the reference the placement passed.
  fn handler_reference(&mut self, attr: &'p js::JSXAttribute<'p>) -> Option<Expr> {
    let Some(js::JSXAttributeValue::ExpressionContainer(c)) = &attr.value else { return None };
    let Some(e) = c.expression.as_expression() else { return None };
    let mut root: &js::Expression = e;
    loop {
      match root {
        js::Expression::ParenthesizedExpression(p) => root = &p.expression,
        m_expr if m_expr.is_member_expression() => { let m = m_expr.as_member_expression().expect("matched as a member"); root = m.object() },
        _ => break,
      }
    }
    let js::Expression::Identifier(id) = root else { return None };
    let name = id.name.as_str();
    if self.handler_fns.contains_key(name) || !self.lowerer.scope.iter().any(|(bound, _)| bound == name) {
      return None;
    }
    self.lowerer.expr(e).ok()
  }

  /// An `on*` attribute's handler as a lowered body or why it is not one.
  /// A handler is `const`s, calls to state setters, writes to the store and calls to actions, `e.preventDefault()`
  /// aside; the body returns the state it set.
  fn handler_attr(&mut self, attr: &'p js::JSXAttribute<'p>) -> Lowered<usize> {
    let Some(js::JSXAttributeValue::ExpressionContainer(c)) = &attr.value else { return Err(self.lowerer.residue(attr.span, "a handler that is not an expression")) };
    let Some(e) = c.expression.as_expression() else { return Err(self.lowerer.residue(attr.span, "an empty handler")) };
    let body = self.handler_body(e)?;
    self.lowered_handlers.push(Handler { event: attr_name(&attr.name)[2..].to_ascii_lowercase(), body });
    Ok(self.lowered_handlers.len() - 1)
  }

  /// Ink's `useAnimation({ interval, isActive })`: frame state of its own that
  /// starts at 0 and an `interval:<ms>` handler adding one to it while
  /// `isActive` holds, 100 ms and always active unless the options say so.
  /// `frame` reads the state and `time` the milliseconds it stands for.
  fn animation_hook(&mut self, call: &'p js::CallExpression<'p>, pattern: &js::BindingPattern, span: Span) -> Lowered<Stmt> {
    let (mut interval, mut active) = (100u64, Expr::Lit(Lit::Bool(true)));
    if let Some(options) = call.arguments.first() {
      let js::Expression::ObjectExpression(object) = crate::syntax::arg_expr(options) else { return Err(self.lowerer.residue(span, "`useAnimation` options that are not an object written out")) };
      for prop in &object.properties {
        let js::ObjectPropertyKind::ObjectProperty(prop) = prop else { return Err(self.lowerer.residue(span, "a spread in `useAnimation`'s options")) };
        let crate::syntax::Prop::KeyValue(kv) = crate::syntax::prop_of(prop) else { return Err(self.lowerer.residue(span, "a `useAnimation` option that is not `key: value`")) };
        let (js::PropertyKey::StaticIdentifier(key), false) = (&kv.key, kv.computed) else { return Err(self.lowerer.residue(span, "a `useAnimation` option named by an expression")) };
        match key.name.as_str() {
          "interval" => {
            let js::Expression::NumericLiteral(ms) = &kv.value else { return Err(self.lowerer.residue(span, "`useAnimation`'s `interval` is a number written out")) };
            interval = ms.value.max(1.0) as u64;
          }
          "isActive" => active = self.lowerer.expr(&kv.value)?,
          other => return Err(self.lowerer.residue(span, format!("`{other}` in `useAnimation`'s options"))),
        }
      }
    }
    let state = format!("$frame{}", self.animations);
    self.animations += 1;
    let js::BindingPattern::ObjectPattern(object) = pattern else { return Err(self.lowerer.residue(span, "`useAnimation` bound to one name; destructure `frame` and `time`")) };
    for prop in crate::syntax::pat_props(object) {
      let (key, local) = match prop {
        crate::syntax::PatProp::Assign(assign) => (assign.key.name.to_string(), assign.key.name.to_string()),
        crate::syntax::PatProp::KeyValue(kv) => {
          let (js::PropertyKey::StaticIdentifier(key), false, js::BindingPattern::BindingIdentifier(local)) = (&kv.key, kv.computed, &kv.value) else { return Err(self.lowerer.residue(span, "a pattern from `useAnimation`")) };
          (key.name.to_string(), local.name.to_string())
        }
        crate::syntax::PatProp::Rest(_) => return Err(self.lowerer.residue(span, "a rest pattern from `useAnimation`")),
      };
      let bound = match key.as_str() {
        "frame" => Expr::Var(state.clone()),
        "time" => Expr::Arith(ArithOp::Mul, Box::new(Expr::Var(state.clone())), Box::new(Expr::Lit(Lit::Int(interval.into())))),
        other => return Err(self.lowerer.residue(span, format!("`{other}` from `useAnimation`"))),
      };
      self.lowerer.scope.push((local, bound));
    }
    self.state.push(state.clone());
    self.state_bindings.push(state.clone());
    let next = Expr::Ternary(Box::new(active), Box::new(Expr::Arith(ArithOp::Add, Box::new(Expr::Var(state.clone())), Box::new(Expr::Lit(Lit::Int(1))))), Box::new(Expr::Var(state.clone())));
    self.lowered_handlers.push(Handler { event: snapfire_fsr_ir::host::Event::Interval(interval).name(), body: vec![Stmt::Return(Expr::Object(vec![Entry::Field(state.clone(), next)]))] });
    Ok(Stmt::Let { name: state, expr: Expr::Lit(Lit::Int(0)) })
  }

  /// The timers a mount effect starts, `useEffect(() => { setInterval(f, ms) }, [])`,
  /// each lowered as a handler named `interval:<ms>` or `timeout:<ms>`, which a
  /// terminal runtime schedules. An effect with dependencies that change, a
  /// timer whose delay is not written out and a callback that does not lower
  /// are dropped, as every effect is on the server; a terminal page refuses
  /// the first when it could change what the page shows, since it has no
  /// browser to run the effect in.
  fn effect_timers(&mut self, call: &'p js::CallExpression<'p>) -> Lowered<()> {
    let was_after = std::mem::replace(&mut self.lowerer.after_body, true);
    let result = self.effect_timers_in(call);
    self.lowerer.after_body = was_after;
    result
  }

  fn effect_timers_in(&mut self, call: &'p js::CallExpression<'p>) -> Lowered<()> {
    let [effect, deps] = &call.arguments[..] else { return Ok(()) };
    let js::Expression::ArrayExpression(deps) = crate::syntax::arg_expr(deps) else { return Ok(()) };
    if !deps.elements.iter().all(|d| self.never_changes(d)) {
      if self.lowerer.reaches(Reach::Terminal) {
        return self.rerun_effect(crate::syntax::arg_expr(effect), deps);
      }
      return Ok(());
    }
    let mut inner: &'p js::Expression<'p> = crate::syntax::arg_expr(effect);
    while let js::Expression::ParenthesizedExpression(p) = inner {
      inner = &p.expression;
    }
    let js::Expression::ArrowFunctionExpression(arrow) = inner else { return Ok(()) };
    if self.lowerer.reaches(Reach::Terminal) {
      return self.terminal_effect(arrow);
    }
    let mut started: Vec<&'p js::Expression<'p>> = Vec::new();
    match arrow_body(arrow) {
      FunctionBody::Expr(e) => started.push(e),
      FunctionBody::Block(stmts) => {
        for stmt in stmts {
          match stmt {
            js::Statement::ExpressionStatement(e) => started.push(&e.expression),
            js::Statement::VariableDeclaration(var) => started.extend(var.declarations.iter().filter_map(|decl| decl.init.as_ref())),
            _ => {}
          }
        }
      }
    }
    let meanings = self.meanings();
    for expr in started {
      let js::Expression::CallExpression(timer) = expr else { continue };
      let callee = &timer.callee;
      let js::Expression::Identifier(id) = callee else { continue };
      let repeats = match id.name.as_str() {
        "setInterval" => true,
        "setTimeout" => false,
        _ => continue,
      };
      if meanings.ctxt(id) != crate::Ctxt::Unresolved {
        continue;
      }
      let [callback, delay] = &timer.arguments[..] else { continue };
      let js::Expression::NumericLiteral(delay) = crate::syntax::arg_expr(delay) else { continue };
      let held = self.lowered_handlers.len();
      match self.handler_body(crate::syntax::arg_expr(callback)) {
        Ok(body) => self.lowered_handlers.push(Handler { event: snapfire_fsr_ir::host::timer(repeats, delay.value.max(0.0) as u64).name(), body }),
        Err(_) => self.lowered_handlers.truncate(held),
      }
    }
    Ok(())
  }

  /// `system.spawn(command, args, options)`.
  fn spawn_call(&self, e: &'p js::Expression<'p>) -> Option<&'p js::CallExpression<'p>> {
    let js::Expression::CallExpression(call) = crate::meaning::bare(e) else { return None };
    let callee = &call.callee;
    self.meanings().names_spawn(callee).then_some(call)
  }

  /// A mount handler that asks the terminal, with a `$spawn` act, to start `call`'s process as `id`.
  fn spawn_start(&mut self, call: &'p js::CallExpression<'p>, id: &str, stdin: bool) -> Lowered<()> {
    let mut fields = vec![Entry::Field("id".to_owned(), Expr::lit_str(id)), Entry::Field("stdin".to_owned(), Expr::Lit(Lit::Bool(stdin)))];
    for (i, key) in ["command", "args", "options"].iter().enumerate() {
      let value = match call.arguments.get(i) {
        Some(arg) if crate::syntax::is_spread(arg) => return Err(self.lowerer.residue(crate::syntax::arg_expr(arg).span(), "a spread into `spawn`")),
        Some(arg) => self.lowerer.expr(crate::syntax::arg_expr(arg))?,
        None => Expr::Lit(Lit::Null),
      };
      fields.push(Entry::Field((*key).to_owned(), value));
    }
    let body = vec![Stmt::Act { action: snapfire_fsr_ir::host::Act::Spawn.name().to_owned(), input: Expr::Object(fields) }, Stmt::Return(Expr::Object(Vec::new()))];
    self.lowered_handlers.push(Handler { event: snapfire_fsr_ir::host::Event::Mount.name(), body });
    Ok(())
  }

  /// A listener on a process the effect spawned, `sub.on("exit", f)`, `sub.stdout.on("data", f)` or `sub.stderr.on("data", f)`, as a handler named `spawn:<id>:<event>`; `setEncoding` is taken as read, since a terminal hands output over as text. False for anything else.
  fn spawn_listener(&mut self, e: &'p js::Expression<'p>) -> Lowered<bool> {
    if let Some((process, method, call)) = stdin_of(e) {
      let Some((_, id)) = self.spawns.iter().find(|(name, _)| *name == process).cloned() else { return Ok(false) };
      let mut input = vec![Entry::Field("id".to_owned(), Expr::lit_str(id))];
      match (method, call.arguments.as_slice()) {
        ("write", [text]) if !crate::syntax::is_spread(text) => input.push(Entry::Field("text".to_owned(), self.lowerer.expr(crate::syntax::arg_expr(text))?)),
        ("end", []) => input.push(Entry::Field("end".to_owned(), Expr::Lit(Lit::Bool(true)))),
        _ => return Err(self.lowerer.residue(call.span, "a spawned process's `stdin` other than `write(text)` and `end()`")),
      }
      let body = vec![Stmt::Act { action: snapfire_fsr_ir::host::Act::Stdin.name().to_owned(), input: Expr::Object(input) }, Stmt::Return(Expr::Object(Vec::new()))];
      self.lowered_handlers.push(Handler { event: snapfire_fsr_ir::host::Event::Mount.name(), body });
      return Ok(true);
    }
    let js::Expression::CallExpression(call) = crate::meaning::bare(e) else { return Ok(false) };
    let callee = &call.callee;
    let Some(method) = (crate::meaning::bare(callee)).as_member_expression() else { return Ok(false) };
    let (process, stream) = match crate::meaning::bare(method.object()) {
      js::Expression::Identifier(id) => (id.name.to_string(), None),
      inner_expr if inner_expr.is_member_expression() => { let inner = inner_expr.as_member_expression().expect("matched as a member"); match (crate::meaning::bare(inner.object()), prop_name_of(inner).as_deref()) {
        (js::Expression::Identifier(id), Some("stdout")) => (id.name.to_string(), Some("stdout")),
        (js::Expression::Identifier(id), Some("stderr")) => (id.name.to_string(), Some("stderr")),
        _ => return Ok(false),
      } },
      _ => return Ok(false),
    };
    let Some((_, id)) = self.spawns.iter().find(|(name, _)| *name == process).cloned() else { return Ok(false) };
    let event = match call.arguments.first().map(|a| crate::meaning::bare(crate::syntax::arg_expr(a))) {
      Some(js::Expression::StringLiteral(event)) => Some(crate::lossy(event.value)),
      _ => None,
    };
    let report = match (stream, prop_name_of(method).as_deref(), event.as_deref()) {
      (Some(_), Some("setEncoding"), _) => return Ok(true),
      (Some("stdout"), Some("on"), Some("data")) => snapfire_fsr_ir::host::Report::Stdout,
      (Some(_), Some("on"), Some("data")) => snapfire_fsr_ir::host::Report::Stderr,
      (None, Some("on"), Some("error")) => snapfire_fsr_ir::host::Report::Error,
      (None, Some("on"), Some("exit")) => snapfire_fsr_ir::host::Report::Exit,
      (None, Some("on"), Some("close")) => snapfire_fsr_ir::host::Report::Close,
      _ => return Err(self.lowerer.residue(call.span, "a use of a spawned process other than `on(\"error\" | \"exit\" | \"close\", f)`, `stdout.on(\"data\", f)`, `stderr.on(\"data\", f)` and `setEncoding`")),
    };
    let Some(listener) = call.arguments.get(1) else { return Err(self.lowerer.residue(call.span, "a process listener without a function")) };
    let (params, body) = match crate::meaning::bare(crate::syntax::arg_expr(listener)) {
      js::Expression::ArrowFunctionExpression(f) => (crate::params_of(&f.params), arrow_body(f)),
      js::Expression::FunctionExpression(f) => match &f.body {
        Some(body) => (crate::params_of(&f.params), FunctionBody::Block(&body.statements)),
        None => return Err(self.lowerer.residue(crate::syntax::arg_expr(listener).span(), "a process listener without a body")),
      },
      other => return Err(self.lowerer.residue(other.span(), "a process listener that is not a function written in place")),
    };
    if matches!(report, snapfire_fsr_ir::host::Report::Exit | snapfire_fsr_ir::host::Report::Close) {
      let event = || Expr::Var("$event".to_owned());
      self.event_binds = Some(vec![event().field("code".to_owned()), event().field("signal".to_owned())]);
    }
    let lowered = self.handler_fn(&params, body, call.span)?;
    self.lowered_handlers.push(Handler { event: snapfire_fsr_ir::host::Event::Process { id, report }.name(), body: lowered });
    Ok(true)
  }

  /// The act a local refers to in a terminal page, bound to an [`snapfire_fsr_ir::host::ACT_REF`].
  fn act_bound(&self, name: &str) -> Option<String> {
    if !self.lowerer.reaches(Reach::Terminal) {
      return None;
    }
    let (_, bound) = self.lowerer.scope.iter().rev().find(|(n, _)| n == name)?;
    snapfire_fsr_ir::host::act_ref(bound)
  }

  /// Whether an effect's dependency is the same on every render: a state setter or a writer; in a terminal also a `useReducer` dispatch, a held ref or a `useCallback` whose own dependencies never change.
  fn never_changes(&self, elem: &js::ArrayExpressionElement) -> bool {
    let item = match elem {
      js::ArrayExpressionElement::Elision(_) => return false,
      js::ArrayExpressionElement::SpreadElement(s) => &s.argument,
      other => other.to_expression(),
    };
    let js::Expression::Identifier(id) = item else { return false };
    let name = id.name.as_str();
    self.setters.iter().any(|(setter, _)| setter == name)
      || self.writers.iter().any(|(writer, _)| writer == name)
      || (self.lowerer.reaches(Reach::Terminal) && (self.dispatchers.iter().any(|(d, _, _)| d == name) || self.held_refs.iter().any(|r| r == name) || self.stable_callbacks.iter().any(|c| c == name) || self.act_bound(name).is_some()))
  }

  /// Whether `body` reads a browser global, the global itself rather than a local of that name: work for a browser, which a terminal page has none of.
  fn reads_the_browser(&self, arrow: &js::ArrowFunctionExpression) -> bool {
    struct Browser<'m>(bool, &'m crate::meaning::Meanings);
    impl<'a> oxc_ast_visit::Visit<'a> for Browser<'_> {
      fn visit_identifier_reference(&mut self, id: &js::IdentifierReference<'a>) {
        self.0 |= self.1.ctxt(id) == crate::Ctxt::Unresolved && BROWSER_GLOBALS.contains(&id.name.as_str());
      }
    }
    let meanings = self.meanings();
    let mut browser = Browser(false, &meanings);
    match crate::syntax::arrow_body_of(arrow) {
      crate::syntax::ArrowBody::Block(b) => b.body.walk(&mut browser),
      crate::syntax::ArrowBody::Expr(e) => e.walk(&mut browser),
    }
    browser.0
  }

  /// A terminal's effect whose dependencies change: its body a handler named [`snapfire_fsr_ir::host::Event::Effect`] and a [`crate::terminal::EFFECT`] the body emits with the dependencies' values, which the terminal compares from one render to the next. An effect that sets no state and calls no action changes nothing the page shows and is left out, as is one that does not lower and reads a browser global: that is the browser's work. An arrow it returns is its cleanup, [`snapfire_fsr_ir::host::Act::Cleanup`].
  fn rerun_effect(&mut self, effect: &'p js::Expression<'p>, deps: &'p js::ArrayExpression<'p>) -> Lowered<()> {
    let js::Expression::ArrowFunctionExpression(arrow) = crate::meaning::bare(effect) else { return Err(self.lowerer.residue(effect.span(), "an effect that is not an arrow written in place")) };
    let event = snapfire_fsr_ir::host::Event::Effect(self.lowered_handlers.len()).name();
    let cleanup_for = self.cleanup_for.replace(event.clone());
    let lowered = self.handler_fn(&[], arrow_body(arrow), arrow.span);
    self.cleanup_for = cleanup_for;
    let lowered = match lowered {
      Ok(lowered) => lowered,
      Err(residue) if residue.message == NO_EFFECT => return Ok(()),
      Err(_) if self.reads_the_browser(arrow) => return Ok(()),
      Err(residue) => return Err(residue),
    };
    self.lowered_handlers.push(Handler { event: event.clone(), body: lowered });
    let mut values = Vec::with_capacity(deps.elements.len());
    for dep in array_items(deps) {
      values.push(Entry::Item(self.lowerer.expr(dep)?));
    }
    let emitted = Expr::Object(vec![Entry::Field("event".to_owned(), Expr::lit_str(event)), Entry::Field("deps".to_owned(), Expr::Array(values))]);
    let held = self.lowerer.temp();
    self.pending_lets.push(Stmt::Let { name: held, expr: Expr::Builtin { name: Builtin::Emit, args: vec![Expr::lit_str(crate::terminal::EFFECT), emitted] } });
    Ok(())
  }

  /// A terminal's mount effect, run as Ink runs it: each `let` it declares is
  /// state of the component's own, each function it declares a handler named
  /// `call:<n>`, a call to one at the top of the effect a `mount` handler, and
  /// a `setInterval` or `setTimeout` it starts a handler named `interval:<ms>`
  /// or `timeout:<ms>`. Any other statement at its top, an `if` around
  /// setter calls for one, is a `mount` handler of its own. Inside a handler,
  /// a timer started on one of the effect's functions is a `$timer` act the
  /// terminal schedules. The cleanup it returns is left out, since a terminal
  /// stops every timer on exit.
  fn terminal_effect(&mut self, arrow: &'p js::ArrowFunctionExpression<'p>) -> Lowered<()> {
    let depth = self.lowerer.scope.len();
    let (locals, fns, spawns) = (self.effect_locals.len(), self.effect_fns.len(), self.spawns.len());
    let result = self.terminal_effect_in(arrow);
    self.lowerer.scope.truncate(depth);
    self.effect_locals.truncate(locals);
    self.effect_fns.truncate(fns);
    self.spawns.truncate(spawns);
    result
  }

  fn terminal_effect_in(&mut self, arrow: &'p js::ArrowFunctionExpression<'p>) -> Lowered<()> {
    let stmts: Vec<&'p js::Statement<'p>> = match arrow_body(arrow) {
      FunctionBody::Block(stmts) => stmts.iter().collect(),
      FunctionBody::Expr(_) => Vec::new(),
    };
    if let FunctionBody::Expr(e) = arrow_body(arrow) {
      return self.effect_start(e, arrow.span);
    }
    let fn_of = |init: &'p js::Expression<'p>| -> Option<(Vec<crate::Param<'p>>, FunctionBody<'p>)> {
      match crate::meaning::bare(init) {
        js::Expression::ArrowFunctionExpression(f) => Some((crate::params_of(&f.params), arrow_body(f))),
        js::Expression::FunctionExpression(f) => f.body.as_ref().map(|body| (crate::params_of(&f.params), FunctionBody::Block(&body.statements))),
        _ => None,
      }
    };
    for stmt in &stmts {
      match stmt {
        js::Statement::FunctionDeclaration(f) => {
          let (Some(body), Some(id)) = (&f.body, &f.id) else { continue };
          self.effect_fn(id.name.as_str(), crate::params_of(&f.params), FunctionBody::Block(&body.statements));
        }
        js::Statement::VariableDeclaration(var) => {
          for decl in &var.declarations {
            let js::BindingPattern::BindingIdentifier(id) = &decl.id else { return Err(self.lowerer.residue(decl.span, "a destructuring in an effect")) };
            let name = id.name.to_string();
            match decl.init.as_ref() {
              Some(init) if fn_of(init).is_some() => {
                let (params, body) = fn_of(init).expect("checked");
                self.effect_fn(&name, params, body);
              }
              Some(init) if timer_call(init).is_some() => {}
              Some(init) if self.spawn_call(init).is_some() => self.spawns.push((name, crate::lo(decl.span).to_string())),
              init => {
                let value = match init {
                  Some(init) => self.lowerer.expr(init)?,
                  None => Expr::Lit(Lit::Undefined),
                };
                let hidden = format!("$fx{}:{name}", crate::lo(decl.span));
                self.state.push(hidden.clone());
                self.state_bindings.push(hidden.clone());
                self.pending_lets.push(Stmt::Let { name: hidden.clone(), expr: value });
                self.lowerer.scope.push((name.clone(), Expr::Var(hidden.clone())));
                self.effect_locals.push((name, hidden));
              }
            }
          }
        }
        _ => {}
      }
    }
    let declared: Vec<(String, String)> = self.effect_fns.clone();
    for (name, event) in &declared {
      let f = self.handler_fns.get(name).cloned().expect("an effect function is a handler function");
      let lowered = self.in_hook(f.hook, |this| this.handler_fn(&f.params, f.body, arrow.span))?;
      self.lowered_handlers.push(Handler { event: event.clone(), body: lowered });
    }
    for stmt in &stmts {
      match stmt {
        js::Statement::ExpressionStatement(e) if self.spawn_listener(&e.expression)? => {}
        js::Statement::ExpressionStatement(e) => self.effect_start(&e.expression, e.span)?,
        js::Statement::VariableDeclaration(var) => {
          for decl in &var.declarations {
            let Some(init) = decl.init.as_ref() else { continue };
            if timer_call(init).is_some() {
              self.effect_start(init, init.span())?;
            }
            if let Some(call) = self.spawn_call(init) {
              let writes = matches!(&decl.id, js::BindingPattern::BindingIdentifier(id) if stmts.iter().any(|s| matches!(s, js::Statement::ExpressionStatement(e) if stdin_of(&e.expression).is_some_and(|(name, _, _)| name == id.name.as_str()))));
              self.spawn_start(call, &crate::lo(decl.span).to_string(), writes)?;
            }
          }
        }
        js::Statement::FunctionDeclaration(_) | js::Statement::ReturnStatement(_) | js::Statement::EmptyStatement(_) => {}
        other => {
          let body = self.handler_fn(&[], FunctionBody::Block(std::slice::from_ref(*stmt)), other.span())?;
          self.lowered_handlers.push(Handler { event: snapfire_fsr_ir::host::Event::Mount.name(), body });
        }
      }
    }
    Ok(())
  }

  /// An effect's function as a handler the terminal can call, `call:<n>`, and a function the effect's other handlers inline.
  fn effect_fn(&mut self, name: &str, params: Vec<crate::Param<'p>>, body: FunctionBody<'p>) {
    let event = snapfire_fsr_ir::host::Event::Call(self.lowered_handlers.len() + self.effect_fns.len()).name();
    self.handler_fns.insert(name.to_owned(), (params, body).into());
    self.effect_fns.push((name.to_owned(), event));
  }

  /// A statement at the top of a terminal effect: a timer it starts, from a `mount` handler as a timer a handler starts does, so its callback holds the values of the render that ran the effect; a call to one of its functions, which runs on mount; or a call made for its effect, which the terminal cannot run.
  fn effect_start(&mut self, e: &'p js::Expression<'p>, span: Span) -> Lowered<()> {
    if timer_call(e).is_some() {
      let body = self.handler_fn(&[], FunctionBody::Expr(e), span)?;
      self.lowered_handlers.push(Handler { event: snapfire_fsr_ir::host::Event::Mount.name(), body });
      return Ok(());
    }
    if let js::Expression::CallExpression(call) = crate::meaning::bare(e) {
      {
      let callee = &call.callee;
        if let js::Expression::Identifier(id) = crate::meaning::bare(callee) {
          if self.effect_fns.iter().any(|(name, _)| *name == id.name.as_str()) {
            let body = self.handler_body(callee)?;
            self.lowered_handlers.push(Handler { event: snapfire_fsr_ir::host::Event::Mount.name(), body });
            return Ok(());
          }
        }
      }
    }
    match e {
      js::Expression::CallExpression(_) => {
        let body = self.handler_fn(&[], FunctionBody::Expr(e), span)?;
        self.lowered_handlers.push(Handler { event: snapfire_fsr_ir::host::Event::Mount.name(), body });
        Ok(())
      }
      _ => Err(self.lowerer.residue(span, "an expression a terminal effect cannot run")),
    }
  }

  /// Ink's `useInput((input, key) => …)`: the component's `input` handler, run
  /// with each keypress as `$event`, `input` its text and `key` its flags. An
  /// empty handler, which keeps an Ink app reading keys, is no handler.
  fn input_hook(&mut self, call: &'p js::CallExpression<'p>) -> Lowered<()> {
    let Some(arg) = call.arguments.first() else { return Err(self.lowerer.residue(call.span, "`useInput` with no handler")) };
    if let js::Expression::ArrowFunctionExpression(arrow) = crate::syntax::arg_expr(arg) {
      if matches!(arrow_body(arrow), FunctionBody::Block(stmts) if stmts.is_empty()) {
        return Ok(());
      }
    }
    let active = match call.arguments.get(1).map(|options| crate::syntax::arg_expr(options)) {
      None => None,
      Some(js::Expression::ObjectExpression(object)) => {
        let mut active = None;
        for prop in &object.properties {
          let js::ObjectPropertyKind::ObjectProperty(prop) = prop else { return Err(self.lowerer.residue(call.span, "a spread in `useInput`'s options")) };
          let crate::syntax::Prop::KeyValue(kv) = crate::syntax::prop_of(prop) else { return Err(self.lowerer.residue(call.span, "a `useInput` option that is not `key: value`")) };
          match prop_name(&kv.key, kv.computed).as_deref() {
            Some("isActive") => active = Some(self.lowerer.expr(&kv.value)?),
            other => return Err(self.lowerer.residue(call.span, format!("`{}` in `useInput`'s options", other.unwrap_or("a computed key")))),
          }
        }
        active
      }
      Some(_) => return Err(self.lowerer.residue(call.span, "`useInput` options that are not an object written out")),
    };
    let event = Expr::Var("$event".to_owned());
    self.event_binds = Some(vec![Expr::Field(Box::new(event.clone()), "input".to_owned()), Expr::Field(Box::new(event), "key".to_owned())]);
    let body = self.handler_body(crate::syntax::arg_expr(arg));
    self.event_binds = None;
    let body = match active {
      Some(cond) => vec![Stmt::If { cond, then: body?, r#else: Vec::new() }],
      None => body?,
    };
    self.lowered_handlers.push(Handler { event: snapfire_fsr_ir::host::Event::Input.name(), body });
    Ok(())
  }

  fn handler_body(&mut self, e: &'p js::Expression<'p>) -> Lowered<Vec<Stmt>> {
    match e {
      js::Expression::ParenthesizedExpression(p) => self.handler_body(&p.expression),
      js::Expression::ArrowFunctionExpression(arrow) => {
        let params = crate::params_of(&arrow.params);
        let body = arrow_body(arrow);
        self.handler_fn(&params, body, arrow.span)
      }
      js::Expression::Identifier(id) => {
        let name = id.name.to_string();
        let Some(f) = self.handler_fns.get(&name).cloned() else {
          return Err(self.lowerer.residue(id.span, format!("`{name}` is not a handler this component declares")));
        };
        self.in_hook(f.hook, |this| this.handler_fn(&f.params, f.body, id.span))
      }
      other => Err(self.lowerer.residue(other.span(), "a handler that is not an arrow or a name")),
    }
  }

  fn handler_fn(&mut self, params: &[crate::Param], body: FunctionBody<'p>, span: Span) -> Lowered<Vec<Stmt>> {
    let hoisting = self.lowerer.hoisting.take();
    let was_handler = std::mem::replace(&mut self.lowerer.in_handler, true);
    let was_after = std::mem::replace(&mut self.lowerer.after_body, true);
    let result = self.handler_fn_in(params, body, span);
    self.lowerer.hoisting = hoisting;
    self.lowerer.in_handler = was_handler;
    self.lowerer.after_body = was_after;
    result
  }

  fn handler_fn_in(&mut self, params: &[crate::Param], body: FunctionBody<'p>, span: Span) -> Lowered<Vec<Stmt>> {
    let mut walk = HandlerWalk::default();
    self.handler_fn_into(params, body, &mut walk)?;
    self.patch_module_lets(&mut walk);
    if walk.patch.is_empty() && !holds_effect(&walk.out) {
      return Err(self.lowerer.residue(span, NO_EFFECT));
    }
    walk.out.push(Stmt::Return(Expr::Object(walk.patch)));
    Ok(walk.out)
  }

  /// A module-level `let` a function the handler calls writes is read back into the patch, so the step writes the store key that holds it.
  fn patch_module_lets(&mut self, walk: &mut HandlerWalk) {
    let mut written = Vec::new();
    let mut collect = |e: &Expr| {
      for name in crate::outer_set_targets(e) {
        if !written.contains(&name) {
          written.push(name);
        }
      }
    };
    snapfire_fsr_ir::ast::body_visit(&walk.out, &mut collect);
    for entry in &walk.patch {
      if let Entry::Field(_, value) = entry {
        value.visit(&mut collect);
      }
    }
    for name in written {
      if self.module_lets.iter().any(|(n, _, _)| *n == name) && !walk.patch.iter().any(|e| matches!(e, Entry::Field(n, _) if *n == name)) {
        walk.patch.push(Entry::Field(name.clone(), Expr::Var(name)));
      }
    }
  }

  /// Runs `lower` inside the custom hook call that declared the function it lowers, so the function's body reads the hook's module, parameters, locals and renamed names. A function no hook declared lowers where it stands.
  fn in_hook<R>(&mut self, hook: Option<Rc<HookScope<'p>>>, lower: impl FnOnce(&mut Self) -> R) -> R {
    let Some(hook) = hook else { return lower(self) };
    let parsed = std::mem::replace(&mut self.lowerer.parsed, hook.parsed);
    let meanings = std::mem::replace(&mut self.lowerer.meanings, hook.meanings.clone());
    let (scope, below, renames) = (self.lowerer.scope.len(), self.lowerer.declared_below.len(), self.hook_renames.len());
    self.lowerer.scope.extend(hook.scope.iter().cloned());
    self.lowerer.declared_below.extend(hook.declared_below.iter().cloned());
    self.hook_renames.extend(hook.renames.iter().cloned());
    let result = lower(self);
    self.lowerer.parsed = parsed;
    self.lowerer.meanings = meanings;
    self.lowerer.scope.truncate(scope);
    self.lowerer.declared_below.truncate(below);
    self.hook_renames.truncate(renames);
    result
  }

  /// A handler's statements into `walk`, under the reach it holds: an inlined handler runs under the branch that called it.
  fn handler_fn_into(&mut self, params: &[crate::Param], body: FunctionBody<'p>, walk: &mut HandlerWalk) -> Lowered<()> {
    let depth = self.lowerer.scope.len();
    let binds = self.event_binds.take().unwrap_or_else(|| vec![Expr::Var("$event".to_owned())]);
    for (at, param) in params.iter().enumerate() {
      let bind = binds.get(at).cloned().unwrap_or(Expr::Lit(Lit::Undefined));
      match (param.pattern, param.default, param.rest) {
        (js::BindingPattern::BindingIdentifier(id), None, false) => self.lowerer.scope.push((id.name.to_string(), bind)),
        (_, Some(_), false) => bind_param(&mut self.lowerer, param, bind)?,
        _ => return Err(self.lowerer.residue(param.span, "a handler's event parameter must be a name")),
      }
    }
    let (spawned, cells) = (self.lowerer.spawned.len(), self.lowerer.cells.len());
    let result = match body {
      FunctionBody::Expr(e) => self.handler_stmt(e, walk),
      FunctionBody::Block(stmts) if self.lowerer.reaches(Reach::Later) && stmts.iter().any(crate::awaits) => self.handler_async(stmts, walk),
      FunctionBody::Block(stmts) => self.handler_block(stmts, walk).map(|_| ()),
    };
    self.lowerer.scope.truncate(depth);
    self.lowerer.spawned.truncate(spawned);
    self.lowerer.cells.truncate(cells);
    result
  }

  /// Statements of a handler or of one of its branches. True when the block ended in a bare `return`, which makes what follows the branch reachable only when the branch was not taken.
  fn handler_block(&mut self, stmts: &'p [js::Statement], walk: &mut HandlerWalk) -> Lowered<bool> {
    let mut declared: Vec<(String, Option<HandlerFn<'p>>)> = Vec::new();
    let result = self.handler_stmts(stmts, walk, &mut declared);
    for (name, outer) in declared.into_iter().rev() {
      match outer {
        Some(f) => self.handler_fns.insert(name, f),
        None => self.handler_fns.remove(&name),
      };
    }
    result
  }

  /// [`Self::handler_block`]'s statements; a `const` arrow the rest calls by name is a function the calls inline, recorded in `declared` with what it shadowed.
  fn handler_stmts(&mut self, stmts: &'p [js::Statement], walk: &mut HandlerWalk, declared: &mut Vec<(String, Option<HandlerFn<'p>>)>) -> Lowered<bool> {
    for (at, stmt) in stmts.iter().enumerate() {
      match stmt {
        js::Statement::ExpressionStatement(e) => self.handler_stmt(&e.expression, walk)?,
        js::Statement::VariableDeclaration(var) => {
          for decl in &var.declarations {
            let js::BindingPattern::BindingIdentifier(name) = &decl.id else { return Err(self.lowerer.residue(decl.span, "a destructuring in a handler")) };
            let local = name.name.to_string();
            if let Some(js::Expression::ArrowFunctionExpression(arrow)) = decl.init.as_ref().map(crate::meaning::bare) {
              if var.kind == js::VariableDeclarationKind::Const && called_names(&stmts[at + 1..]).contains(&local) && !listened_names(&stmts[at + 1..]).contains(&local) {
                let outer = self.handler_fns.insert(local.clone(), (crate::params_of(&arrow.params), arrow_body(arrow)).into());
                declared.push((local, outer));
                continue;
              }
            }
            let listener = match decl.init.as_ref().map(crate::meaning::bare) {
              Some(js::Expression::ArrowFunctionExpression(arrow)) if self.lowerer.reaches(Reach::Process) && listened_names(&stmts[at + 1..]).contains(&local) => Some(arrow),
              _ => None,
            };
            let expr = match (decl.init.as_ref(), listener) {
              (Some(_), Some(arrow)) => self.named_listener(&crate::params_of(&arrow.params), arrow_body(arrow), decl.span)?,
              (Some(init), None) => match self.arrow_timer(init) {
                Some((repeats, callback, delay)) => self.handler_timer(repeats, callback, delay, decl.span, walk)?,
                None => self.lowerer.expr(init)?,
              },
              (None, _) => Expr::Lit(Lit::Null),
            };
            if let Some(js::Expression::CallExpression(call)) = decl.init.as_ref().map(crate::meaning::bare) {
              if self.lowerer.reaches(Reach::Process) && self.lowerer.spawns(call) {
                self.lowerer.spawned.push(local.clone());
              }
            }
            // A local a listener or a continuation writes lasts past this step, so it is a cell under a key fresh to this run.
            if var.kind != js::VariableDeclarationKind::Const && written_names(&stmts[at + 1..], true).contains(&local) {
              let key = self.lowerer.temp();
              walk.out.push(Stmt::Let { name: key.clone(), expr: Expr::Ext { module: "crypto".to_owned(), name: "random".to_owned(), args: vec![Expr::Lit(Lit::Int(8))] } });
              let handle = Expr::ModuleCell { key: Box::new(Expr::Var(key)), init: Box::new(Expr::Lit(Lit::Null)) };
              walk.effect(Stmt::Expr(Expr::WriteCell(Box::new(handle.clone()), Box::new(expr))));
              self.lowerer.cells.push((local.clone(), handle.clone()));
              self.lowerer.scope.push((local, Expr::ReadCell(Box::new(handle))));
              continue;
            }
            let (name, expr) = match &walk.reach {
              None => (local.clone(), expr),
              Some(reach) => {
                walk.locals += 1;
                (format!("{local}${}", walk.locals), Expr::Ternary(Box::new(reach.clone()), Box::new(expr), Box::new(Expr::Lit(Lit::Null))))
              }
            };
            if let Some(arrow) = listener {
              self.named_listeners.push((name.clone(), !arrow.params.is_empty()));
            }
            self.lowerer.scope.push((local, Expr::Var(name.clone())));
            walk.out.push(Stmt::Let { name, expr });
          }
        }
        js::Statement::FunctionDeclaration(f) if f.body.is_some() => {
          let local = function_name(&stmts[at]).unwrap_or_default().to_owned();
          let params: Vec<crate::Param<'p>> = crate::params_of(&f.params);
          let body = FunctionBody::Block(&f.body.as_ref().expect("checked").statements);
          if !(self.lowerer.reaches(Reach::Process) && listened_names(&stmts[at + 1..]).contains(&local)) {
            let outer = self.handler_fns.insert(local.clone(), (params, body).into());
            declared.push((local, outer));
            continue;
          }
          let expr = self.named_listener(&params, body, f.span)?;
          let (name, expr) = match &walk.reach {
            None => (local.clone(), expr),
            Some(reach) => {
              walk.locals += 1;
              (format!("{local}${}", walk.locals), Expr::Ternary(Box::new(reach.clone()), Box::new(expr), Box::new(Expr::Lit(Lit::Null))))
            }
          };
          self.named_listeners.push((name.clone(), !params.is_empty()));
          self.lowerer.scope.push((local, Expr::Var(name.clone())));
          walk.out.push(Stmt::Let { name, expr });
        }
        js::Statement::ReturnStatement(r) if r.argument.as_ref().is_none_or(is_undefined) => return Ok(true),
        js::Statement::ReturnStatement(ret) if ret.argument.is_some() && self.cleanup_for.is_some() => {
          let (Some(cleanup), span) = (&ret.argument, &ret.span) else { unreachable!("checked") };
          let effect = self.cleanup_for.clone().expect("checked");
          let js::Expression::ArrowFunctionExpression(arrow) = crate::meaning::bare(cleanup) else { return Err(self.lowerer.residue(*span, "an effect's cleanup that is not an arrow written in place")) };
          let (event, carry) = self.arrow_continuation(arrow, Vec::new(), *span)?;
          walk.act(snapfire_fsr_ir::host::Act::Cleanup.name().to_owned(), Expr::Object(vec![Entry::Field("effect".to_owned(), Expr::lit_str(effect)), Entry::Field("event".to_owned(), Expr::lit_str(event)), Entry::Field("carry".to_owned(), carry)]));
          return Ok(true);
        }
        js::Statement::IfStatement(branch) => self.handler_if(branch, walk)?,
        js::Statement::TryStatement(t) => {
          if self.handler_try(t, walk)? {
            return Ok(true);
          }
        }
        other => return Err(self.lowerer.residue(other.span(), "a statement a handler cannot hold; a handler is `const`s, branches, `try`, calls to state setters, writes to the store and calls to actions")),
      }
    }
    Ok(false)
  }

  /// An async function a terminal handler runs, `void (async () => { … })()`: its statements run in the handler as far as the first `await`, which hands the awaited promise a continuation, a handler of the component named [`snapfire_fsr_ir::host::Event::Resume`] holding the rest. The names the rest reads that the component does not hold itself are carried to it, the handler's own `$event` among them.
  fn handler_async(&mut self, stmts: &'p [js::Statement], walk: &mut HandlerWalk) -> Lowered<()> {
    self.async_within(stmts, walk, &Within::default())
  }

  /// `stmts` run as an async function's are, inside `within`. An `if` holding an `await` runs each branch under its condition, the statements after it a join each branch runs once it is done; a `try` holding one runs the statements after it as a join too, its `catch` where a promise inside rejects.
  fn async_within(&mut self, stmts: &'p [js::Statement], walk: &mut HandlerWalk, within: &Within<'p>) -> Lowered<()> {
    let Some(at) = stmts.iter().position(crate::awaits) else {
      self.handler_block(stmts, walk)?;
      run_join(within.then.as_ref(), walk);
      return Ok(());
    };
    self.handler_block(&stmts[..at], walk)?;
    let rest = &stmts[at + 1..];
    match &stmts[at] {
      js::Statement::IfStatement(branch) => {
        let inner = Within { catch: within.catch.clone(), then: self.join(rest, within)? };
        let cond = self.lowerer.expr(&branch.test)?;
        let cond = self.branch_cond(cond, walk);
        let outer = walk.reach.clone();
        let depth = self.lowerer.scope.len();
        walk.reach = Some(reach_under(&outer, cond.clone()));
        let then = self.async_within(arm(&branch.consequent), walk, &inner);
        self.lowerer.scope.truncate(depth);
        then?;
        walk.reach = Some(reach_under(&outer, Expr::Not(Box::new(cond))));
        let otherwise = match &branch.alternate {
          Some(alt) => self.async_within(arm(alt), walk, &inner),
          None => {
            run_join(inner.then.as_ref(), walk);
            Ok(())
          }
        };
        self.lowerer.scope.truncate(depth);
        walk.reach = outer;
        otherwise
      }
      js::Statement::TryStatement(t) => {
        let mut after = self.join(rest, within)?;
        if let Some(finalizer) = &t.finalizer {
          if t.block.body.iter().chain(t.handler.iter().flat_map(|c| &c.body.body)).any(holds_return) {
            return Err(self.lowerer.residue(t.span, "a `return` inside a `try` whose `finally` follows an `await`"));
          }
          after = self.join(&finalizer.body, &Within { catch: within.catch.clone(), then: after })?;
        }
        let caught = match (&t.handler, &t.finalizer) {
          (Some(catch), _) => {
            let param = match catch.param.as_ref().map(|p| &p.pattern) {
              Some(js::BindingPattern::BindingIdentifier(name)) => Some(name.name.to_string()),
              None => None,
              Some(other) => return Err(self.lowerer.residue(other.span(), "a `catch` parameter that is not a name")),
            };
            Some(Catch { param, body: &catch.body.body[..], after: after.clone() })
          }
          (None, Some(_)) if within.catch.is_some() => return Err(self.lowerer.residue(t.span, "a `try` with only a `finally` inside another `try`, around an `await`")),
          (None, Some(finalizer)) => Some(Catch { param: None, body: &finalizer.body[..], after: None }),
          (None, None) => within.catch.clone(),
        };
        let inner = Within { catch: caught, then: after };
        self.async_within(&t.block.body, walk, &inner)
      }
      stmt => {
        let (awaited, bound) = match stmt {
          js::Statement::ExpressionStatement(e) => match crate::meaning::bare(&e.expression) {
            js::Expression::AwaitExpression(a) => (&a.argument, None),
            _ => return Err(self.lowerer.residue(e.span, "an `await` inside an expression of a handler; await as a statement or a `const`'s value")),
          },
          js::Statement::VariableDeclaration(var) => match (var.declarations.as_slice(), var.declarations.first().and_then(|d| d.init.as_ref()).map(crate::meaning::bare)) {
            ([decl], Some(js::Expression::AwaitExpression(a))) => match &decl.id {
              js::BindingPattern::BindingIdentifier(name) => (&a.argument, Some(name.name.to_string())),
              _ => return Err(self.lowerer.residue(decl.span, "an awaited value bound to a pattern")),
            },
            _ => return Err(self.lowerer.residue(var.span, "an `await` inside an expression of a handler; await as a statement or a `const`'s value")),
          },
          other => return Err(self.lowerer.residue(other.span(), "an `await` inside a loop of a handler")),
        };
        let deferred = self.lowerer.expr(awaited)?;
        let resumed = Expr::Var(RESUME.to_owned());
        let mut cw = HandlerWalk::default();
        let depth = self.lowerer.scope.len();
        let captured = self.capture_states();
        // The settled value is a variable the continuation declares, so a continuation after the next await carries it instead of reading that await's value.
        if let Some(name) = &bound {
          cw.out.push(Stmt::Let { name: name.clone(), expr: resumed.clone().field("value") });
          self.lowerer.scope.push((name.clone(), Expr::Var(name.clone())));
        }
        let lowered = (|| -> Lowered<()> {
          let ok = resumed.clone().field("ok");
          if let Some(catch) = &within.catch {
            cw.reach = Some(Expr::Not(Box::new(ok.clone())));
            if let Some(param) = &catch.param {
              self.lowerer.scope.push((param.clone(), resumed.clone().field("value")));
            }
            self.handler_block(catch.body, &mut cw)?;
            run_join(catch.after.as_ref(), &mut cw);
          }
          cw.reach = Some(ok);
          self.async_within(rest, &mut cw, within)
        })();
        self.lowerer.scope.truncate(depth);
        lowered?;
        let (event, carry) = self.continuation_handler(stmt.span(), cw, &captured);
        let then = |field: &str, value: Expr| Stmt::Act {
          action: snapfire_fsr_ir::host::Act::Then.name().to_owned(),
          input: Expr::Object(vec![Entry::Field(field.to_owned(), value), Entry::Field("event".to_owned(), Expr::lit_str(event.clone())), Entry::Field("carry".to_owned(), carry.clone())]),
        };
        // A call that fails as it is awaited is a rejected promise, so a `catch` around the `await` runs.
        match within.catch {
          None => walk.effect(then("deferred", deferred)),
          Some(_) => walk.effect(Stmt::Try { body: vec![then("deferred", deferred)], name: Some(REJECTED.to_owned()), handler: Some(vec![then("rejected", Expr::Var(REJECTED.to_owned()))]), finally: Vec::new() }),
        }
        Ok(())
      }
    }
  }

  /// The statements after an `if` or a `try` that awaits, run inside `within` as a continuation every way through it ends with; `within`'s own join when there are none.
  fn join(&mut self, rest: &'p [js::Statement], within: &Within<'p>) -> Lowered<Option<Join>> {
    if rest.is_empty() {
      return Ok(within.then.clone());
    }
    let mut cw = HandlerWalk::default();
    let depth = self.lowerer.scope.len();
    let captured = self.capture_states();
    let lowered = self.async_within(rest, &mut cw, within);
    self.lowerer.scope.truncate(depth);
    lowered?;
    let (event, carry) = self.continuation_handler(rest[0].span(), cw, &captured);
    Ok(Some(Join { event, carry }))
  }

  /// A listener on a process the handler spawned as the act [`snapfire_fsr_ir::host::Act::Listen`], its callback a continuation of the component with `code` and `signal` bound from an exit's report; a promise's `resolve` or `reject` as the callback settles that promise instead; a function the effect declares runs as the handler it already is.
  fn handler_listen(&mut self, call: &'p js::CallExpression<'p>, walk: &mut HandlerWalk) -> Lowered<()> {
    let (process, report, callback, removes) = self.lowerer.listener(call).expect("checked");
    let Some(report) = report else { return Ok(()) };
    let Some(callback) = callback else { return Err(self.lowerer.residue(call.span, "a process listener without a function")) };
    let mut fields = vec![Entry::Field("process".to_owned(), self.lowerer.expr(process)?), Entry::Field("report".to_owned(), Expr::lit_str(report.name()))];
    let exits = matches!(report, snapfire_fsr_ir::host::Report::Exit | snapfire_fsr_ir::host::Report::Close);
    let effect_fn = match crate::meaning::bare(callback) {
      js::Expression::Identifier(id) => self.effect_fns.iter().find(|(f, _)| *f == id.name.as_str()).map(|(_, event)| event.clone()),
      _ => None,
    };
    let named = match (effect_fn, crate::meaning::bare(callback)) {
      (Some(event), _) => Some((Expr::Object(vec![Entry::Field("event".to_owned(), Expr::lit_str(event)), Entry::Field("carry".to_owned(), Expr::Object(Vec::new()))]), false)),
      (None, js::Expression::Identifier(_)) => match self.lowerer.expr(callback)? {
        Expr::Var(var) => self.named_listeners.iter().find(|(name, _)| *name == var).map(|(_, takes)| (Expr::Var(var.clone()), *takes)),
        _ => None,
      },
      _ => None,
    };
    if removes {
      let Some((named, _)) = named else { return Err(self.lowerer.residue(callback.span(), "an `off` whose function is not a `const` arrow of the handler that passes it to `on` or a function its effect declares")) };
      fields.push(Entry::Field("event".to_owned(), named.field("event")));
      walk.act(snapfire_fsr_ir::host::Act::Unlisten.name().to_owned(), Expr::Object(fields));
      return Ok(());
    }
    if let Some((named, takes)) = named {
      if exits && takes {
        return Err(self.lowerer.residue(callback.span(), "an `exit` or `close` listener taking parameters that is not written in place"));
      }
      fields.push(Entry::Field("event".to_owned(), named.clone().field("event")));
      fields.push(Entry::Field("carry".to_owned(), named.field("carry")));
      walk.act(snapfire_fsr_ir::host::Act::Listen.name().to_owned(), Expr::Object(fields));
      return Ok(());
    }
    if matches!(crate::meaning::bare(callback), js::Expression::Identifier(id) if self.lowerer.settlers.iter().any(|s| *s == id.name.as_str())) {
      fields.push(Entry::Field("settle".to_owned(), self.lowerer.expr(callback)?));
      walk.act(snapfire_fsr_ir::host::Act::Listen.name().to_owned(), Expr::Object(fields));
      return Ok(());
    }
    let js::Expression::ArrowFunctionExpression(arrow) = crate::meaning::bare(callback) else { return Err(self.lowerer.residue(callback.span(), "a process listener that is not an arrow written in place, an arrow a `const` names or a promise's `resolve` or `reject`")) };
    let value = Expr::Var(RESUME.to_owned()).field("value");
    let mut binds = Vec::new();
    for (i, param) in crate::params_of(&arrow.params).iter().enumerate() {
      let Some(name) = param.ident() else { return Err(self.lowerer.residue(param.span, "a process listener's parameter that is not a name")) };
      let bound = match (exits, i) {
        (true, 0) => value.clone().field("code"),
        (true, _) => value.clone().field("signal"),
        (false, _) => value.clone(),
      };
      binds.push((name.name.to_string(), bound));
    }
    let (event, carry) = self.arrow_continuation(arrow, binds, call.span)?;
    fields.push(Entry::Field("event".to_owned(), Expr::lit_str(event)));
    fields.push(Entry::Field("carry".to_owned(), carry));
    walk.act(snapfire_fsr_ir::host::Act::Listen.name().to_owned(), Expr::Object(fields));
    Ok(())
  }

  /// A `const` arrow or a function declaration a terminal handler passes to `on` or `off` as the value `{ event, carry }` of the continuation it is, its parameters bound to what the report carries.
  fn named_listener(&mut self, params: &[crate::Param], body: FunctionBody<'p>, at: Span) -> Lowered<Expr> {
    let value = Expr::Var(RESUME.to_owned()).field("value");
    let mut binds = Vec::new();
    for param in params {
      let Some(name) = param.ident() else { return Err(self.lowerer.residue(param.span, "a process listener's parameter that is not a name")) };
      binds.push((name.name.to_string(), value.clone()));
    }
    let (event, carry) = self.body_continuation(body, binds, at)?;
    Ok(Expr::Object(vec![Entry::Field("event".to_owned(), Expr::lit_str(event)), Entry::Field("carry".to_owned(), carry)]))
  }

  /// `arrow`'s body as a continuation of the component with `binds` in scope: its event and the carry the act that runs it passes.
  fn arrow_continuation(&mut self, arrow: &'p js::ArrowFunctionExpression<'p>, binds: Vec<(String, Expr)>, at: Span) -> Lowered<(String, Expr)> {
    self.body_continuation(arrow_body(arrow), binds, at)
  }

  /// Binds each name that reads a state value to [`CAPTURED`] and the value, so a continuation lowered under it reads the value the render that ran the act held, as a closure holds the render's `const`. An updater and a reducer still apply to the state as it is. Returns each captured name with what the act carries in for it.
  fn capture_states(&mut self) -> Vec<(String, Expr)> {
    let states: Vec<String> = self.setters.iter().map(|(_, state)| state.clone()).chain(self.dispatchers.iter().map(|(_, state, _)| state.clone())).collect();
    let mut captured: Vec<(String, Expr)> = Vec::new();
    let mut shadows: Vec<(String, Expr)> = Vec::new();
    for (at, (name, bound)) in self.lowerer.scope.iter().enumerate() {
      let latest = !self.lowerer.scope[at + 1..].iter().any(|(later, _)| later == name);
      if let (true, Expr::Var(state)) = (latest, bound) {
        if states.contains(state) {
          let held = format!("{CAPTURED}{state}");
          if !captured.iter().any(|(c, _)| *c == held) {
            captured.push((held.clone(), Expr::Var(state.clone())));
          }
          shadows.push((name.clone(), Expr::Var(held)));
        }
      }
    }
    self.lowerer.scope.extend(shadows);
    captured
  }

  /// A function's `body` as a continuation of the component with `binds` in scope: its event and the carry the act that runs it passes.
  fn body_continuation(&mut self, body: FunctionBody<'p>, binds: Vec<(String, Expr)>, at: Span) -> Lowered<(String, Expr)> {
    let depth = self.lowerer.scope.len();
    let captured = self.capture_states();
    self.lowerer.scope.extend(binds);
    let cleanup_for = self.cleanup_for.take();
    let mut cw = HandlerWalk::default();
    let lowered = match body {
      FunctionBody::Block(stmts) => self.async_within(stmts, &mut cw, &Within::default()),
      FunctionBody::Expr(e) => self.handler_stmt(e, &mut cw),
    };
    self.cleanup_for = cleanup_for;
    self.lowerer.scope.truncate(depth);
    lowered?;
    Ok(self.continuation_handler(at, cw, &captured))
  }

  /// `setInterval(f, ms)` or `setTimeout(f, ms)` in a terminal handler with `f` an arrow written in place.
  fn arrow_timer(&self, e: &'p js::Expression<'p>) -> Option<(bool, &'p js::ArrowFunctionExpression<'p>, &'p js::Expression<'p>)> {
    let (repeats, callback, delay) = timer_call(e).filter(|_| self.lowerer.reaches(Reach::Later))?;
    match crate::meaning::bare(callback) {
      js::Expression::ArrowFunctionExpression(arrow) => Some((repeats, arrow, delay)),
      _ => None,
    }
  }

  /// A timer a handler starts on an arrow, [`Self::arrow_timer`]: the act [`snapfire_fsr_ir::host::Act::Timer`] with a fresh id and the arrow as a continuation. Its value is `{ "$timer": id }`, [`snapfire_fsr_ir::host::TIMER`].
  fn handler_timer(&mut self, repeats: bool, callback: &'p js::ArrowFunctionExpression<'p>, delay: &'p js::Expression<'p>, at: Span, walk: &mut HandlerWalk) -> Lowered<Expr> {
    let continuation = self.arrow_continuation(callback, Vec::new(), at)?;
    self.start_timer(repeats, continuation, delay, walk)
  }

  /// The act [`snapfire_fsr_ir::host::Act::Timer`] with a fresh id, running `continuation`, its event and carry, after `delay`. Its value is `{ "$timer": id }`.
  fn start_timer(&mut self, repeats: bool, (event, carry): (String, Expr), delay: &'p js::Expression<'p>, walk: &mut HandlerWalk) -> Lowered<Expr> {
    let id = self.lowerer.temp();
    walk.out.push(Stmt::Let { name: id.clone(), expr: Expr::Ext { module: "crypto".to_owned(), name: "random".to_owned(), args: vec![Expr::Lit(Lit::Int(8))] } });
    let ms = self.lowerer.expr(delay)?;
    let input = vec![("id", Expr::Var(id.clone())), ("event", Expr::lit_str(event)), ("carry", carry), ("ms", ms), ("repeats", Expr::Lit(Lit::Bool(repeats)))];
    walk.act(snapfire_fsr_ir::host::Act::Timer.name().to_owned(), Expr::Object(input.into_iter().map(|(key, value)| Entry::Field(key.to_owned(), value)).collect()));
    Ok(Expr::Object(vec![Entry::Field(snapfire_fsr_ir::host::TIMER.to_owned(), Expr::Var(id))]))
  }

  /// `walk` as a handler of the component named [`snapfire_fsr_ir::host::Event::Resume`] for `at`: what it was run with bound as [`RESUME`], every name it reads that the component does not hold carried in, each of `captured` as the expression beside it. Returns the event and the carry the act that runs it passes.
  fn continuation_handler(&mut self, at: Span, walk: HandlerWalk, captured: &[(String, Expr)]) -> (String, Expr) {
    let mut body = walk.out;
    body.push(Stmt::Return(Expr::Object(walk.patch)));
    let held: Vec<String> = self.state.iter().chain(self.state_bindings.iter()).cloned().chain(["$state".to_owned(), "$props".to_owned(), RESUME.to_owned()]).chain(self.lowerer.cells.iter().map(|(name, _)| name.clone())).collect();
    let carried: Vec<String> = snapfire_fsr_ir::body_free_vars(&body).into_iter().filter(|name| !held.contains(name)).collect();
    let resumed = Expr::Var(RESUME.to_owned());
    let mut handler = vec![Stmt::Let { name: RESUME.to_owned(), expr: Expr::Var("$event".to_owned()) }];
    for name in &carried {
      handler.push(Stmt::Let { name: name.clone(), expr: resumed.clone().field("carry").field(name.clone()) });
    }
    handler.extend(body);
    let event = snapfire_fsr_ir::host::Event::Resume(format!("{}@{}", self.lowerer.parsed.file, crate::lo(at))).name();
    if !self.lowered_handlers.iter().any(|held| held.event == event) {
      self.lowered_handlers.push(Handler { event: event.clone(), body: handler });
    }
    let carry = |name: &String| captured.iter().find(|(held, _)| held == name).map_or_else(|| Expr::Var(name.clone()), |(_, value)| value.clone());
    (event, Expr::Object(carried.iter().map(|name| Entry::Field(name.clone(), carry(name))).collect()))
  }

  /// `if`, `else` and `else if`: each branch lowers under its condition joined to the reach around it and the names it declares go out of scope with it. A branch that returns leaves the rest of the handler reachable only when the other one ran.
  /// `c ? a() : b();` as a statement: each arm runs under its side of the condition, as `if (c) a(); else b();` does.
  fn handler_cond(&mut self, cond: &'p js::ConditionalExpression<'p>, walk: &mut HandlerWalk) -> Lowered<()> {
    let test = self.lowerer.expr(&cond.test)?;
    let test = self.branch_cond(test, walk);
    let outer = walk.reach.clone();
    let depth = self.lowerer.scope.len();
    walk.reach = Some(reach_under(&outer, test.clone()));
    self.handler_stmt(&cond.consequent, walk)?;
    self.lowerer.scope.truncate(depth);
    self.reread_effect_locals(walk);
    walk.reach = Some(reach_under(&outer, Expr::Not(Box::new(test))));
    self.handler_stmt(&cond.alternate, walk)?;
    self.lowerer.scope.truncate(depth);
    self.reread_effect_locals(walk);
    walk.reach = outer;
    Ok(())
  }

  fn handler_if(&mut self, branch: &'p js::IfStatement<'p>, walk: &mut HandlerWalk) -> Lowered<()> {
    let mut updates = Vec::new();
    let b = self.lowerer.build();
    let test = counted_test(b, &branch.test, &|name| self.effect_locals.iter().any(|(local, _)| local == name), &mut updates);
    let cond = self.lowerer.expr(&test)?;
    for (name, op, span) in updates {
      let value = self.lowerer.expr(&b.binary(b.ident(&name, span), op, b.number(1.0, span), span))?;
      self.effect_write(&name, value, walk);
    }
    let cond = self.branch_cond(cond, walk);
    let outer = walk.reach.clone();
    let depth = self.lowerer.scope.len();
    walk.reach = Some(reach_under(&outer, cond.clone()));
    let then_returned = self.handler_branch(&branch.consequent, walk)?;
    self.lowerer.scope.truncate(depth);
    self.reread_effect_locals(walk);
    let mut else_returned = false;
    if let Some(alt) = &branch.alternate {
      walk.reach = Some(reach_under(&outer, Expr::Not(Box::new(cond.clone()))));
      else_returned = self.handler_branch(alt, walk)?;
      self.lowerer.scope.truncate(depth);
      self.reread_effect_locals(walk);
    }
    walk.reach = match (then_returned, else_returned) {
      (false, false) => outer,
      (true, false) => Some(reach_under(&outer, Expr::Not(Box::new(cond)))),
      (false, true) => Some(reach_under(&outer, cond)),
      (true, true) => Some(Expr::Lit(Lit::Bool(false))),
    };
    Ok(())
  }

  /// A branch's condition as the statements after the branch read it. One that reads a cell is bound once, where the branch is, since a write to the cell after the branch must not change which way it went.
  fn branch_cond(&mut self, cond: Expr, walk: &mut HandlerWalk) -> Expr {
    if !reads_a_cell(&cond) {
      return cond;
    }
    let name = self.lowerer.temp();
    walk.out.push(Stmt::Let { name: name.clone(), expr: reach_under(&walk.reach, cond) });
    Expr::Var(name)
  }

  /// Writes `value` to the effect's `let` `name`: a patch of the state that holds it, which the rest of the handler reads.
  fn effect_write(&mut self, name: &str, value: Expr, walk: &mut HandlerWalk) {
    let Some((_, hidden)) = self.effect_locals.iter().find(|(local, _)| local == name).cloned() else { return };
    walk.set(hidden, value);
    self.reread_effect_locals(walk);
  }

  /// Binds each effect `let` and each held ref a handler has written to the value its patch holds, so a read after the write, or after the branch that made it, sees it, as JavaScript's `ref.current` does.
  fn reread_effect_locals(&mut self, walk: &HandlerWalk) {
    let refs: Vec<(String, String)> = self.held_refs.iter().map(|r| (r.clone(), r.clone())).collect();
    for (name, hidden) in self.effect_locals.clone().into_iter().chain(refs) {
      if let Some(value) = walk.patch.iter().find_map(|entry| match entry {
        Entry::Field(field, value) if *field == hidden => Some(value.clone()),
        _ => None,
      }) {
        self.lowerer.scope.push((name, value));
      }
    }
  }

  /// A `try` in a handler. Its block runs inside a `Try` whose `catch` notes that it failed and what was thrown; the `catch` block runs where it failed, inside a `Try` of its own; the `finally` block runs either way and then what failed and was not caught fails the handler. A state key set inside either block is written where the set runs, so a failure after it keeps it. True when the `finally` ended in a bare `return`.
  fn handler_try(&mut self, t: &'p js::TryStatement<'p>, walk: &mut HandlerWalk) -> Lowered<bool> {
    let (failed, thrown) = (self.lowerer.temp(), self.lowerer.temp());
    let noted = |caught: &str| vec![Stmt::Set { name: failed.clone(), expr: Expr::Lit(Lit::Bool(true)) }, Stmt::Set { name: thrown.clone(), expr: Expr::Var(caught.to_owned()) }];
    let mut run = vec![Stmt::Let { name: failed.clone(), expr: Expr::Lit(Lit::Bool(false)) }, Stmt::Let { name: thrown.clone(), expr: Expr::Lit(Lit::Undefined) }];
    let mut in_place: Vec<(String, String, Expr)> = Vec::new();
    let guarded = |this: &mut Self, stmts: &'p [js::Statement], walk: &mut HandlerWalk, in_place: &mut Vec<(String, String, Expr)>| -> Lowered<Vec<Stmt>> {
      let mut inner = HandlerWalk { patch: walk.patch.clone(), locals: walk.locals, in_place: Some(std::mem::take(in_place)), ..HandlerWalk::default() };
      let depth = this.lowerer.scope.len();
      let returned = this.handler_block(stmts, &mut inner);
      this.lowerer.scope.truncate(depth);
      walk.locals = inner.locals;
      *in_place = inner.in_place.take().unwrap_or_default();
      if returned? {
        return Err(this.lowerer.residue(t.span, "a `return` inside a handler's `try` or `catch`"));
      }
      let caught = this.lowerer.temp();
      Ok(vec![Stmt::Try { body: inner.out, name: Some(caught.clone()), handler: Some(noted(&caught)), finally: Vec::new() }])
    };
    let tried = guarded(self, &t.block.body, walk, &mut in_place)?;
    run.extend(tried);
    if let Some(catch) = &t.handler {
      let depth = self.lowerer.scope.len();
      let bound = match catch.param.as_ref().map(|p| &p.pattern) {
        None => Ok(()),
        Some(js::BindingPattern::BindingIdentifier(id)) => {
          self.lowerer.scope.push((id.name.to_string(), Expr::Var(thrown.clone())));
          Ok(())
        }
        Some(pattern) => bind_pattern(&mut self.lowerer, pattern, Expr::Var(thrown.clone())),
      };
      let handled = bound.and_then(|_| guarded(self, &catch.body.body, walk, &mut in_place));
      self.lowerer.scope.truncate(depth);
      let mut handled = handled?;
      handled.insert(0, Stmt::Set { name: failed.clone(), expr: Expr::Lit(Lit::Bool(false)) });
      run.push(Stmt::If { cond: Expr::Var(failed.clone()), then: handled, r#else: Vec::new() });
    }
    for (_, var, start) in in_place.iter().rev() {
      run.insert(0, Stmt::Let { name: var.clone(), expr: start.clone() });
    }
    match &walk.reach {
      None => walk.out.extend(run),
      Some(reach) => {
        let (lets, rest): (Vec<Stmt>, Vec<Stmt>) = run.into_iter().partition(|stmt| matches!(stmt, Stmt::Let { .. }));
        walk.out.extend(lets);
        walk.out.push(Stmt::If { cond: reach.clone(), then: rest, r#else: Vec::new() });
      }
    }
    for (state, var, _) in in_place {
      walk.set(state, Expr::Var(var));
    }
    let returned = match &t.finalizer {
      Some(finalizer) => self.handler_block(&finalizer.body, walk)?,
      None => false,
    };
    let message = Expr::Coalesce(Box::new(Expr::Field(Box::new(Expr::Var(thrown.clone())), "message".to_owned())), Box::new(Expr::Var(thrown.clone())));
    let rethrow = Stmt::Expr(Expr::Builtin { name: Builtin::Throw, args: vec![message, Expr::Var(thrown.clone())] });
    walk.effect(Stmt::If { cond: Expr::Var(failed), then: vec![rethrow], r#else: Vec::new() });
    Ok(returned)
  }

  fn handler_branch(&mut self, stmt: &'p js::Statement<'p>, walk: &mut HandlerWalk) -> Lowered<bool> {
    match stmt {
      js::Statement::BlockStatement(block) => self.handler_block(&block.body, walk),
      other => self.handler_block(std::slice::from_ref(other), walk),
    }
  }

  /// `setX(expr)` or `setX((prev) => expr)` adds to the patch; `e.preventDefault()`
  /// and `e.stopPropagation()` are the browser's; `void f()` or `f()` naming a
  /// declared handler inlines it. Under a branch, a key's value is the branch's
  /// where it ran and what the patch held before, or the state as it stands,
  /// where it did not; an action call runs inside an `if` of the same reach.
  fn handler_stmt(&mut self, e: &'p js::Expression<'p>, walk: &mut HandlerWalk) -> Lowered<()> {
    match e {
      js::Expression::ParenthesizedExpression(p) => self.handler_stmt(&p.expression, walk),
      js::Expression::ConditionalExpression(cond) => self.handler_cond(cond, walk),
      js::Expression::UnaryExpression(u) if u.operator == js::UnaryOperator::Void => self.handler_stmt(&u.argument, walk),
      js::Expression::AwaitExpression(a) => self.handler_stmt(&a.argument, walk),
      js::Expression::CallExpression(call) if self.lowerer.async_iife(call).is_some() => {
        let arrow = self.lowerer.async_iife(call).expect("checked");
        match crate::syntax::arrow_body_of(arrow) {
          crate::syntax::ArrowBody::Block(b) => self.handler_async(&b.body, walk),
          crate::syntax::ArrowBody::Expr(e) => self.handler_stmt(e, walk),
        }
      }
      js::Expression::AssignmentExpression(assign) if self.lowerer.cell_of_target(&assign.left).is_some() => {
        let stmt = self.lowerer.effect(e)?;
        walk.effect(stmt);
        Ok(())
      }
      js::Expression::UpdateExpression(update) if matches!(&update.argument, js::SimpleAssignmentTarget::AssignmentTargetIdentifier(id) if self.lowerer.cells.iter().any(|(name, _)| *name == id.name.as_str())) => {
        let stmt = self.lowerer.effect(e)?;
        walk.effect(stmt);
        Ok(())
      }
      js::Expression::CallExpression(call) if self.lowerer.listens(call) => self.handler_listen(call, walk),
      js::Expression::CallExpression(call) if console_call(&mut self.lowerer, call)?.is_some() => {
        let logged = console_call(&mut self.lowerer, call)?.expect("checked");
        walk.effect(Stmt::Expr(logged));
        Ok(())
      }
      js::Expression::CallExpression(call) => {
        let callee = &call.callee;
        match callee {
          js::Expression::Identifier(id) => {
            let name = self.hook_renames.iter().rev().find(|(from, _)| *from == id.name.as_str()).map_or_else(|| id.name.to_string(), |(_, to)| to.clone());
            if let Some((_, action)) = self.writers.iter().find(|(w, _)| *w == name).cloned() {
              let input = match call.arguments.first() {
                Some(arg) => self.lowerer.expr(crate::syntax::arg_expr(arg))?,
                None => Expr::lit_str(String::new()),
              };
              walk.act(action.to_owned(), input);
              return Ok(());
            }
            if let Some((repeats, callback, delay)) = self.arrow_timer(e) {
              self.handler_timer(repeats, callback, delay, call.span, walk)?;
              return Ok(());
            }
            if self.lowerer.reaches(Reach::Later) && (name == "clearInterval" || name == "clearTimeout") {
              let timer = match call.arguments.first() {
                Some(arg) => self.lowerer.expr(crate::syntax::arg_expr(arg))?,
                None => Expr::Lit(Lit::Null),
              };
              walk.act(snapfire_fsr_ir::host::Act::ClearTimer.name().to_owned(), Expr::Object(vec![Entry::Field("timer".to_owned(), timer)]));
              return Ok(());
            }
            if let Some((repeats, callback, delay)) = timer_call(e) {
              let js::Expression::Identifier(target) = crate::meaning::bare(callback) else { return Err(self.lowerer.residue(call.span, "a timer a handler starts on something other than a function its effect or its component declares")) };
              let held = self.handler_fns.get(target.name.as_str()).cloned();
              let Some((_, event)) = self.effect_fns.iter().find(|(f, _)| *f == target.name.as_str()).cloned() else {
                let Some(HandlerFn { params, body, hook }) = held else { return Err(self.lowerer.residue(call.span, format!("a timer on `{}`, which is not a function its effect or its component declares", target.name))) };
                let binds = params.iter().filter_map(|param| match param.pattern {
                  js::BindingPattern::BindingIdentifier(id) if param.default.is_none() && !param.rest => Some((id.name.to_string(), Expr::Lit(Lit::Undefined))),
                  _ => None,
                });
                let continuation = self.in_hook(hook, |this| this.body_continuation(body, binds.collect(), call.span))?;
                self.start_timer(repeats, continuation, delay, walk)?;
                return Ok(());
              };
              let input = Expr::Object(vec![Entry::Field("event".to_owned(), Expr::lit_str(event)), Entry::Field("ms".to_owned(), self.lowerer.expr(delay)?), Entry::Field("repeats".to_owned(), Expr::Lit(Lit::Bool(repeats)))]);
              walk.act(snapfire_fsr_ir::host::Act::Timer.name().to_owned(), input);
              return Ok(());
            }
            if self.transitions.contains(&name) {
              let Some(js::Expression::ArrowFunctionExpression(arrow)) = call.arguments.first().map(|a| crate::meaning::bare(crate::syntax::arg_expr(a))) else { return Err(self.lowerer.residue(call.span, "`startTransition` of something other than an arrow")) };
              return match crate::syntax::arrow_body_of(arrow) {
                crate::syntax::ArrowBody::Block(block) => self.handler_block(&block.body, walk).map(|_| ()),
                crate::syntax::ArrowBody::Expr(e) => self.handler_stmt(e, walk),
              };
            }
            if let Some((_, state, reducer)) = self.dispatchers.iter().find(|(d, _, _)| *d == name).cloned() {
              let action = match call.arguments.first() {
                Some(arg) => self.lowerer.expr(crate::syntax::arg_expr(arg))?,
                None => Expr::Lit(Lit::Null),
              };
              walk.set(state.clone(), Expr::Apply { f: Box::new(reducer), args: vec![Expr::Var(state), action] });
              return Ok(());
            }
            if let Some((_, state)) = self.setters.iter().find(|(s, _)| *s == name).cloned() {
              let arg = call.arguments.first().ok_or_else(|| self.lowerer.residue(call.span, format!("`{name}` without a value")))?;
              let value = match crate::syntax::arg_expr(arg) {
                js::Expression::ArrowFunctionExpression(arrow) => {
                  let depth = self.lowerer.scope.len();
                  if let Some(prev) = crate::first_param(&arrow.params).and_then(|p| p.ident()) {
                    self.lowerer.scope.push((prev.name.to_string(), Expr::Var(state.clone())));
                  }
                  let value = match crate::syntax::arrow_body_of(arrow) {
                    crate::syntax::ArrowBody::Expr(e) => self.lowerer.expr(e),
                    crate::syntax::ArrowBody::Block(b) => block_to_expr(&mut self.lowerer, &b.body),
                  };
                  self.lowerer.scope.truncate(depth);
                  value?
                }
                other => match self.lowerer.expr(other)? {
                  updater @ Expr::Lambda { .. } => Expr::Apply { f: Box::new(updater), args: vec![Expr::Var(state.clone())] },
                  value => value,
                },
              };
              walk.set(state, value);
              return Ok(());
            }
            if let Some(HandlerFn { params, body, hook }) = self.handler_fns.get(&name).cloned() {
              let before = (walk.out.len(), walk.patch.clone());
              let hoisting = self.lowerer.hoisting.take();
              let mut args = Vec::with_capacity(call.arguments.len());
              for arg in &call.arguments {
                args.push(self.lowerer.expr(crate::syntax::arg_expr(arg))?);
              }
              self.event_binds = Some(args);
              let result = self.in_hook(hook, |this| this.handler_fn_into(&params, body, walk));
              self.lowerer.hoisting = hoisting;
              result?;
              if walk.out.len() == before.0 && walk.patch == before.1 {
                return Err(self.lowerer.residue(call.span, "a handler that sets no state and calls no action"));
              }
              return Ok(());
            }
            if let Some(action) = self.act_bound(&name) {
              let input = match call.arguments.first() {
                Some(arg) => self.lowerer.expr(crate::syntax::arg_expr(arg))?,
                None => Expr::lit_str(String::new()),
              };
              walk.act(action, input);
              return Ok(());
            }
            if let Some(action) = action_alias_of(self.lowerer.parsed, &name) {
              let input = match call.arguments.first() {
                Some(arg) => self.lowerer.expr(crate::syntax::arg_expr(arg))?,
                None => Expr::Object(Vec::new()),
              };
              walk.act(action, input);
              return Ok(());
            }
            if let Some(store_call) = self.meanings().of_expr(callee).and_then(|meant| meant.store_call()) {
              return self.store_call(store_call, call, walk);
            }
            let lowered = self.lowerer.expr(e)?;
            if effectful(&lowered) || calls_a_value(&lowered) || writes_back(&lowered) || (self.lowerer.reaches(Reach::Later) && !self.lowerer.cells.is_empty() && touches_cells(&lowered)) {
              walk.effect(Stmt::Expr(lowered));
              return Ok(());
            }
            Err(self.lowerer.residue(id.span, format!("a call to `{name}`, which is not a state setter, a write to the store, an action or a handler this component declares")))
          }
          m_expr if m_expr.is_member_expression() => {
let m = m_expr.as_member_expression().expect("matched as a member");
            let method = match crate::syntax::prop(m) {
              crate::syntax::MemberProp::Ident(i) => i.name.to_string(),
              _ => String::new(),
            };
            if method == "preventDefault" || method == "stopPropagation" {
              return Ok(());
            }
            if let Some(action) = generated_action_of(self.lowerer.parsed, callee) {
              let input = match call.arguments.first() {
                Some(arg) => self.lowerer.expr(crate::syntax::arg_expr(arg))?,
                None => Expr::Object(Vec::new()),
              };
              walk.act(action, input);
              return Ok(());
            }
            let unbound = self.lowerer.unbound.clone();
            match self.lowerer.expr(e) {
              Ok(lowered) if effectful(&lowered) || writes_back(&lowered) => {
                walk.effect(Stmt::Expr(lowered));
                return Ok(());
              }
              _ => self.lowerer.unbound = unbound,
            }
            Err(self.lowerer.residue(call.span, format!("`.{method}()` in a handler; a handler is `const`s, branches, `try`, calls to state setters, writes to the store and calls to actions")))
          }
          other => Err(self.lowerer.residue(other.span(), "a call a handler cannot make")),
        }
      }
      js::Expression::AssignmentExpression(assign) if timer_call(&assign.right).is_none() && matches!(&assign.left, js::AssignmentTarget::AssignmentTargetIdentifier(id) if self.effect_locals.iter().any(|(local, _)| *local == id.name.as_str())) => {
        let js::AssignmentTarget::AssignmentTargetIdentifier(id) = &assign.left else { unreachable!("checked") };
        let b = self.lowerer.build();
        let value = match crate::syntax::compound(b, assign.operator, b.read(id), b.copy(&assign.right), assign.span) {
          None => self.lowerer.expr(&assign.right)?,
          Some(computed) => self.lowerer.expr(&computed)?,
        };
        self.effect_write(id.name.as_str(), value, walk);
        Ok(())
      }
      js::Expression::UpdateExpression(update) if matches!(&update.argument, js::SimpleAssignmentTarget::AssignmentTargetIdentifier(id) if self.effect_locals.iter().any(|(local, _)| *local == id.name.as_str())) => {
        let js::SimpleAssignmentTarget::AssignmentTargetIdentifier(id) = &update.argument else { unreachable!("checked") };
        let op = if update.operator == js::UpdateOperator::Increment { js::BinaryOperator::Addition } else { js::BinaryOperator::Subtraction };
        let b = self.lowerer.build();
        let value = self.lowerer.expr(&b.binary(b.read(id), op, b.number(1.0, update.span), update.span))?;
        self.effect_write(id.name.as_str(), value, walk);
        Ok(())
      }
      js::Expression::AssignmentExpression(assign) if assign.operator == js::AssignmentOperator::Assign && matches!(&assign.right, right if timer_call(right).is_some()) => self.handler_stmt(&assign.right, walk),
      js::Expression::AssignmentExpression(assign) if assign.operator == js::AssignmentOperator::Assign => {
        let held = match assign.left.as_member_expression() {
          Some(m) => match (m.object(), prop_name_of(m).as_deref()) {
            (js::Expression::Identifier(id), Some("current")) => self.held_refs.iter().find(|r| **r == id.name.as_str()).cloned(),
            _ => None,
          },
          None => None,
        };
        let Some(held) = held else { return Err(self.lowerer.residue(assign.span, "a statement a handler cannot hold; a handler is `const`s, branches, `try`, calls to state setters, writes to the store and calls to actions")) };
        let value = self.lowerer.expr(&assign.right)?;
        walk.set(held, Expr::Object(vec![Entry::Field("current".to_owned(), value)]));
        self.reread_effect_locals(walk);
        Ok(())
      }
      other => Err(self.lowerer.residue(other.span(), "a statement a handler cannot hold; a handler is `const`s, branches, `try`, calls to state setters, writes to the store and calls to actions")),
    }
  }

  /// Whether the name a handler's residue left unbound is one the set can
  /// read, an import of the application's own, so the residue is worth
  /// returning for the resolving loop to follow rather than recording as the
  /// browser's handler.
  fn unbound_is_followable(&self) -> bool {
    let Some(name) = &self.lowerer.unbound else { return false };
    !self.lowerer.unfollowed.contains(name) && matches!(self.meanings().bindings.get(name), Some(crate::meaning::Meaning::Own { .. }))
  }

  /// `set(key, value)`, `clear(key)`, `transaction(() => { … })` and
  /// `optimistic(key, guess, () => act(input))` in a handler. A write goes
  /// into the patch under the state bound to the key, the `useStore` binding
  /// when the component reads the key and a `let` of its own otherwise. A
  /// transaction is its closure's statements, since a patch is applied whole.
  /// `optimistic` is the write and the action; the host refuses the patch
  /// when the action fails, which is the restore the browser does by hand.
  fn store_call(&mut self, name: &'static str, call: &'p js::CallExpression<'p>, walk: &mut HandlerWalk) -> Lowered<()> {
    match name {
      "set" | "clear" => {
        let key = self.store_key_arg(call)?;
        let value = match (name, call.arguments.get(1)) {
          ("set", Some(arg)) => self.lowerer.expr(crate::syntax::arg_expr(arg))?,
          ("set", None) => return Err(self.lowerer.residue(call.span, "`set` without a value")),
          _ => Expr::Lit(Lit::Null),
        };
        let state = self.store_state(&key);
        walk.set(state, value);
        Ok(())
      }
      "transaction" => {
        let Some(arg) = call.arguments.first() else { return Err(self.lowerer.residue(call.span, "`transaction` without a function")) };
        match crate::syntax::arg_expr(arg) {
          js::Expression::ArrowFunctionExpression(arrow) => {
            let params = crate::params_of(&arrow.params);
            self.handler_fn_into(&params, arrow_body(arrow), walk)
          }
          other => Err(self.lowerer.residue(other.span(), "a `transaction` whose work is not an arrow")),
        }
      }
      _ => {
        let key = self.store_key_arg(call)?;
        let Some(guess) = call.arguments.get(1) else { return Err(self.lowerer.residue(call.span, "`optimistic` without a guess")) };
        let guess = self.lowerer.expr(&crate::syntax::arg_expr(&guess))?;
        let state = self.store_state(&key);
        walk.set(state, guess);
        let Some(remote) = call.arguments.get(2) else { return Err(self.lowerer.residue(call.span, "`optimistic` without the action it runs")) };
        match &*crate::syntax::arg_expr(&remote) {
          js::Expression::ArrowFunctionExpression(arrow) => {
            let params = crate::params_of(&arrow.params);
            self.handler_fn_into(&params, arrow_body(arrow), walk)
          }
          other => Err(self.lowerer.residue(other.span(), "an `optimistic` whose remote is not an arrow calling an action")),
        }
      }
    }
  }

  /// The key a store call names, a string the build can read.
  fn store_key_arg(&mut self, call: &'p js::CallExpression<'p>) -> Lowered<String> {
    let Some(first) = call.arguments.first() else { return Err(self.lowerer.residue(call.span, "a store call without a key")) };
    match self.lowerer.expr(crate::syntax::arg_expr(first))? {
      Expr::Lit(Lit::Str(key)) => Ok(key),
      _ => Err(self.lowerer.residue(crate::syntax::arg_expr(first).span(), "a store key that is not a string the build can read")),
    }
  }

  /// The state name bound to `key`: the `useStore` binding reading it, else a
  /// binding made here so the handler's patch has a name the key rides under.
  fn store_state(&mut self, key: &str) -> String {
    if let Some((name, _)) = self.store_bindings.iter().find(|(_, k)| k == key) {
      return name.clone();
    }
    let name = format!("$store{}", self.store_bindings.len());
    self.state.push(name.clone());
    self.state_bindings.push(name.clone());
    self.store_bindings.push((name.clone(), key.to_owned()));
    self.store_lets.push(Stmt::Let { name: name.clone(), expr: Expr::Store(key.to_owned()) });
    name
  }

  /// `const Lazy = island(Chart, { when: "visible" })` at module scope, when
  /// `name` is such a `Lazy`: the component and the timing.
  fn island_alias(&mut self, name: &str) -> Lowered<Option<(String, Option<String>, Option<String>)>> {
    match island_alias_of(self.lowerer.parsed, &self.meanings(), name) {
      Ok(alias) => Ok(alias.map(|a| (a.target, a.when, a.mode))),
      Err((span, message)) => Err(self.lowerer.residue(span, message)),
    }
  }

  /// `<Slot name="modal" />` in a layout: the plan child of that name, in a
  /// region navigation fills and empties.
  fn slot_element(&mut self, el: &'p js::JSXElement<'p>) -> Lowered<Tmpl> {
    if !self.layout_root {
      return Err(self.lowerer.residue(el.span, "`<Slot>` outside a layout"));
    }
    let mut name = None;
    for attr in &el.opening_element.attributes {
      let js::JSXAttributeItem::Attribute(attr) = attr else { return Err(self.lowerer.residue(el.span, "a spread on `<Slot>`")) };
      if attr_name(&attr.name) != "name" {
        return Err(self.lowerer.residue(attr.span, "`<Slot>` takes `name` and nothing else"));
      }
      match self.attr_value(attr)? {
        Expr::Lit(Lit::Str(value)) => name = Some(value),
        _ => return Err(self.lowerer.residue(attr.span, "a slot's `name` is written out")),
      }
    }
    let Some(name) = name else { return Err(self.lowerer.residue(el.span, "`<Slot>` needs a `name`")) };
    let fallback = self.children(&el.children)?;
    if fallback.is_empty() {
      return Ok(self.slot(&name));
    }
    Ok(self.slot_with_fallback(&name, fallback))
  }

  /// `<Link href="/x" full into="modal" prefetch="none" keep={false}>` from the
  /// client library: an `<a>` carrying the data attributes the navigator reads.
  fn link_element(&mut self, el: &'p js::JSXElement<'p>) -> Lowered<Tmpl> {
    let attrs = self.jsx_attrs(el)?;
    let children = self.children(&el.children)?;
    placements::link(self, &attrs, children)
  }

  /// Whether an `<img>`'s `src` is `<asset>.src` of an imported image, which
  /// the rewrite lowers as a `Picture`, and not escaped with `data-sf-raw`.
  fn img_names_an_asset(&self, el: &'p js::JSXElement<'p>) -> bool {
    let escaped = el.opening_element.attributes.iter().any(|attr| matches!(attr, js::JSXAttributeItem::Attribute(attr) if attr_name(&attr.name) == RAW_ESCAPE));
    !escaped && self.img_asset_path(el).is_some()
  }

  /// The imported image an `<img src={x.src}>` names, by its app-relative path.
  fn img_asset_path(&self, el: &'p js::JSXElement<'p>) -> Option<String> {
    for attr in &el.opening_element.attributes {
      let js::JSXAttributeItem::Attribute(attr) = attr else { continue };
      if attr_name(&attr.name) != "src" {
        continue;
      }
      let Some(js::JSXAttributeValue::ExpressionContainer(c)) = &attr.value else { return None };
      let Some(e) = c.expression.as_expression() else { return None };
      let Some(m) = (&*e).as_member_expression() else { return None };
      let (js::Expression::Identifier(obj), crate::syntax::MemberProp::Ident(prop)) = (m.object(), crate::syntax::prop(m)) else { return None };
      if prop.name.as_str() != "src" {
        return None;
      }
      return placements::asset_import(&self.lowerer, obj.name.as_str());
    }
    None
  }

  /// `<Picture src={hero} alt="…" sizes="…" priority widths={[…]} quality={…} source="cms">`,
  /// or an `<img>` of an imported asset under the rewrite: a `<picture>`
  /// with a `<source>` per format and the hashed original as its `<img>`,
  /// an `<img>` alone for an image served as it is, and for a string `src`
  /// an `<img>` whose `srcset` a named source's template writes.
  fn picture_element(&mut self, el: &'p js::JSXElement<'p>, from_img: bool) -> Lowered<Tmpl> {
    let attrs = self.jsx_attrs(el)?;
    placements::picture(self, &attrs, At::Span(el.span), from_img)
  }

  fn remember_head(&mut self, row: HeadRow) {
    if !self.heads.contains(&row) {
      self.heads.push(row);
    }
  }

  fn island_timing(&self, value: Expr, span: Span) -> Lowered<String> {
    placements::timing(self, value, At::Span(span))
  }

  /// `lowered` as an island placement, keeping the id its placement took, so
  /// the bundle's copy of this component carries the same region key.
  fn island_of(&self, lowered: Tmpl, when: Option<String>, mode: Option<String>, span: Span) -> Lowered<Tmpl> {
    match lowered {
      Tmpl::Component { module, props, children, id, .. } => Ok(Tmpl::Island { module, props, children, when, mode, id, define: false }),
      _ => Err(self.lowerer.residue(span, "an island must be a component, not an element")),
    }
  }

  /// `<Suspense>` as its children inside React's completed boundary, the fallback left out since nothing on the server waits; `<Activity>` as its children inside an Activity boundary unless `mode` is `"hidden"`, which React's server renders as nothing.
  fn boundary(&mut self, which: &str, el: &'p js::JSXElement<'p>) -> Lowered<Tmpl> {
    let children = Tmpl::Fragment(self.children(&el.children)?);
    if which == "Suspense" && self.lowerer.reaches(Reach::Terminal) {
      let fallback = el.opening_element.attributes.iter().find_map(|attr| match attr {
        js::JSXAttributeItem::Attribute(attr) if attr_name(&attr.name) == "fallback" => attr_expression(attr),
        _ => None,
      });
      let fallback = match fallback {
        Some(e) => self.child_expr(e)?,
        None => Tmpl::Fragment(Vec::new()),
      };
      return Ok(Tmpl::Element { tag: snapfire_fsr_ir::render::SUSPENSE_TAG.to_owned(), attrs: Vec::new(), children: vec![children, fallback] });
    }
    let kind = if which == "Suspense" { "$" } else { "&" };
    let boundary = Tmpl::Element { tag: snapfire_fsr_ir::render::BOUNDARY_TAG.to_owned(), attrs: vec![Entry::Field("kind".to_owned(), Expr::lit_str(kind))], children: vec![children] };
    if which == "Suspense" {
      return Ok(boundary);
    }
    let mode = el.opening_element.attributes.iter().find_map(|attr| match attr {
      js::JSXAttributeItem::Attribute(attr) if attr_name(&attr.name) == "mode" => Some(attr),
      _ => None,
    });
    let Some(mode) = mode else { return Ok(boundary) };
    let mode = self.attr_value(mode)?;
    Ok(Tmpl::If { cond: Expr::Compare(CompareOp::Ne, Box::new(mode), Box::new(Expr::lit_str("hidden"))), then: Box::new(boundary), r#else: None })
  }

  /// `as_child` says the element sits among JSX children, where a keyed
  /// placement's wrap must be braced.
  fn component_ref(&mut self, name: &str, el: &'p js::JSXElement<'p>, as_child: bool) -> Lowered<Tmpl> {
    if name == "Fragment" || name == "React.Fragment" {
      return Ok(Tmpl::Fragment(self.children(&el.children)?));
    }
    if let Some(crate::meaning::Meaning::Library(crate::meaning::Construct::Boundary(which))) = self.meanings().of_tag(&el.opening_element.name) {
      return self.boundary(which, el);
    }
    match self.placement(el) {
      Some("Island") => return self.island_element(el),
      Some("Slot") => return self.slot_element(el),
      Some("Link") => return self.link_element(el),
      Some("Picture") => return self.picture_element(el, false),
      Some("Mount") => return self.mount_element(el),
      Some(which @ ("Box" | "Text" | "Newline" | "Spacer")) => {
        let attrs = self.jsx_attrs(el)?;
        let children = self.children(&el.children)?;
        return placements::terminal(self, which, &attrs, children);
      }
      Some("Place") => {
        let attrs = self.jsx_attrs(el)?;
        let has_children = el.children.iter().any(|c| !matches!(c, js::JSXChild::Text(t) if jsx_text(t.value.as_str()).is_empty()));
        return placements::place(self, &attrs, has_children, At::Span(el.span));
      }
      Some("Embed") => {
        let attrs = self.jsx_attrs(el)?;
        return placements::embed(self, &attrs, At::Span(el.span));
      }
      _ => {}
    }
    let chosen = self.chosen.get(name).cloned();
    if let (js::JSXElementName::IdentifierReference(id), None) = (&el.opening_element.name, &chosen) {
      let meanings = self.meanings();
      if !matches!(meanings.ctxt(id), crate::Ctxt::Top | crate::Ctxt::Unresolved) {
        return Err(self.lowerer.residue(el.opening_element.name.span(), format!("`{name}` is not a component this file declares or imports")));
      }
    }
    if let Some((target, when, mode)) = self.island_alias(name)? {
      let lowered = self.component_ref(&target, el, false)?;
      return self.island_of(lowered, when, mode, el.span);
    }
    let transformed_target = match &chosen {
      Some(_) => true,
      None => matches!(self.meanings().bindings.get(name), Some(crate::meaning::Meaning::Own { file, name: export }) if is_transformed(file) && export == "default"),
    };
    let mut overrides: Vec<Override> = Vec::new();
    let mut props = Vec::new();
    let mut browser_props: Vec<String> = Vec::new();
    let placed_module = format!("{}#{name}", self.file);
    for attr in &el.opening_element.attributes {
      let attr = match attr {
        js::JSXAttributeItem::Attribute(attr) => attr,
        js::JSXAttributeItem::SpreadAttribute(spread) => {
          props.push(Entry::Spread(self.lowerer.expr(&spread.argument)?));
          continue;
        }
      };
      let raw = attr_name(&attr.name);
      if raw == "key" || raw == "ref" {
        continue;
      }
      if raw == "components" && transformed_target {
        overrides = self.component_overrides(attr)?;
        continue;
      }
      if raw == "children" {
        return Err(self.lowerer.residue(attr.span, "`children` as a prop; pass them between the tags"));
      }
      if let Some(lowered) = self.handler_prop(attr, &raw) {
        match lowered {
          Ok(index) => props.push(Entry::Field(raw, Expr::Handler(index as u32))),
          Err(residue) if self.unbound_is_followable() => return Err(residue),
          Err(_) if !is_handler_name(&raw) && self.function_prop(attr)?.is_some() => {
            self.lowerer.unbound = None;
            let value = self.function_prop(attr)?.expect("lowered just above");
            self.note_function(&placed_module, &raw, &value, attr);
            props.push(Entry::Field(raw, value));
          }
          Err(residue) => {
            self.lowerer.unbound = None;
            if !props.iter().any(|e| matches!(e, Entry::Field(n, _) if n == UNLOWERED_ATTR)) {
              props.push(Entry::Field(UNLOWERED_ATTR.to_owned(), Expr::lit_str(format!("{}:{}: {}", residue.line, residue.column, residue.message))));
            }
          }
        }
        continue;
      }
      if is_handler_name(&raw) {
        if let Some(handed) = self.handler_reference(attr) {
          props.push(Entry::Field(raw, handed));
        }
        continue;
      }
      if let Some(name) = self.browser_function_named(attr) {
        if !props.iter().any(|e| matches!(e, Entry::Field(n, _) if n == UNLOWERED_ATTR)) {
          let (line, column) = self.lowerer.parsed.loc(attr.span.start);
          props.push(Entry::Field(UNLOWERED_ATTR.to_owned(), Expr::lit_str(format!("{line}:{column}: `{name}`, a function only the browser calls"))));
        }
        props.push(Entry::Field(raw, Expr::Handler(snapfire_fsr_ir::render::BROWSER_HANDLER)));
        continue;
      }
      // A prop only the browser knows goes in as null; the child reads it as a browser value and this component hydrates to compute it.
      self.lowerer.browser_value = None;
      let value = match self.attr_value(attr) {
        Ok(value) => value,
        Err(residue) => match self.lowerer.browser_value.take() {
          Some(reason) => {
            let (line, column) = self.lowerer.parsed.loc(attr.span.start);
            let wrap = attr_expression(attr).map(|e| self.lowerer.parsed.range(e.span()));
            self.browser_texts.push(BrowserText { wrap, line, column, reason });
            browser_props.push(raw.clone());
            Expr::Var(BROWSER_VAR.to_owned())
          }
          None if self.unbound_is_followable() => return Err(residue),
          None => match self.unlowered_function(&placed_module, &raw, attr, &residue) {
            Some((value, why)) => {
              self.lowerer.unbound = None;
              if !props.iter().any(|e| matches!(e, Entry::Field(n, _) if n == UNLOWERED_ATTR)) {
                props.push(Entry::Field(UNLOWERED_ATTR.to_owned(), Expr::lit_str(why)));
              }
              value
            }
            None => return Err(residue),
          },
        },
      };
      if fills::holds_function(&value) {
        self.note_function(&placed_module, &raw, &value, attr);
      }
      props.push(Entry::Field(raw, value));
    }
    let mut passed = Vec::new();
    for attr in &el.opening_element.attributes {
      let js::JSXAttributeItem::Attribute(attr) = attr else { continue };
      let Some(e) = attr_expression(attr) else { continue };
      if let Some(kind) = self.lowerer.kind_of(e) {
        passed.push((attr_name(&attr.name), kind));
      }
    }
    for prop in browser_props {
      if !passed.iter().any(|(p, _)| *p == prop) {
        passed.push((prop, crate::Kind::Browser));
      }
    }
    let children = self.children(&el.children)?;
    let loc = self.lowerer.parsed.loc(el.span.start);
    let at = self.lowerer.parsed.range(el.opening_element.name.span()).end;
    let range = self.lowerer.parsed.range(el.span);
    let id = match &mut self.lowerer.hoisting {
      Some(candidates) => candidates.island(at, range, as_child),
      None => 0,
    };
    if let Some((key, choices)) = chosen {
      let mut tree = Tmpl::Fragment(Vec::new());
      for (text, target) in choices.iter().rev() {
        let placed = format!("{}#default", self.variant_of(target, &overrides));
        self.refs.push((placed.clone(), loc));
        self.placed_kinds.push((placed.clone(), passed.clone()));
        let cond = Expr::Compare(CompareOp::Eq, Box::new(Expr::Str(Box::new(key.clone()))), Box::new(Expr::lit_str(text.clone())));
        let then = Tmpl::Component { module: format!("{}#{placed}", self.file), props: props.clone(), children: children.clone(), id, keyed: false };
        tree = Tmpl::If { cond, then: Box::new(then), r#else: Some(Box::new(tree)) };
      }
      return Ok(tree);
    }
    if !overrides.is_empty() {
      let Some(crate::meaning::Meaning::Own { file: target, .. }) = self.meanings().bindings.get(name).cloned() else { unreachable!("a transformed target") };
      let placed = format!("{}#default", self.variant_of(&target, &overrides));
      self.refs.push((placed.clone(), loc));
      self.placed_kinds.push((placed.clone(), passed));
      return Ok(Tmpl::Component { module: format!("{}#{placed}", self.file), props, children, id, keyed: false });
    }
    self.refs.push((name.to_owned(), loc));
    self.placed_kinds.push((name.to_owned(), passed));
    Ok(Tmpl::Component { module: format!("{}#{name}", self.file), props, children, id, keyed: false })
  }

  fn attr_value(&mut self, attr: &'p js::JSXAttribute<'p>) -> Lowered<Expr> {
    match &attr.value {
      None => Ok(Expr::Lit(Lit::Bool(true))),
      Some(js::JSXAttributeValue::StringLiteral(s)) => Ok(Expr::Lit(Lit::Str(decode_entities(&crate::lossy(s.value))))),
      Some(js::JSXAttributeValue::ExpressionContainer(c)) => match c.expression.as_expression() {
        Some(e) if self.holds_markup(e) => Ok(Expr::Markup(std::sync::Arc::new(self.child_expr(e)?))),
        Some(e) => {
          self.premake_expr(e)?;
          self.lowerer.expr(e)
        }
        None => Err(self.lowerer.residue(c.expression.span(), "an empty attribute expression")),
      },
      Some(other) => Err(self.lowerer.residue(other.span(), "an element as an attribute value")),
    }
  }

  /// `style={{ a: x, bTwo: y }}` as an object keyed by CSS name, which the
  /// renderer serialises the way React does.
  fn style(&mut self, attr: &'p js::JSXAttribute<'p>) -> Lowered<Expr> {
    let attr = self.jsx_attr(attr)?;
    placements::style(self, &attr)
  }

  /// An attribute as the shared placements read it.
  fn jsx_attr(&self, attr: &'p js::JSXAttribute<'p>) -> Lowered<Attr<'p>> {
    let value = match &attr.value {
      None => Value::Present,
      Some(js::JSXAttributeValue::StringLiteral(s)) => Value::Text(decode_entities(&crate::lossy(s.value))),
      Some(js::JSXAttributeValue::ExpressionContainer(c)) => match c.expression.as_expression() {
        Some(e) => Value::Script(e),
        None => return Err(self.lowerer.residue(c.expression.span(), "an empty attribute expression")),
      },
      Some(other) => return Err(self.lowerer.residue(other.span(), "an element as an attribute value")),
    };
    Ok(Attr { name: attr_name(&attr.name), value, at: At::Span(attr.span) })
  }

  /// An element's attributes as the shared placements read them, handlers
  /// left out: they belong to the browser.
  fn jsx_attrs(&self, el: &'p js::JSXElement<'p>) -> Lowered<Vec<Attr<'p>>> {
    let mut out = Vec::new();
    for attr in &el.opening_element.attributes {
      match attr {
        js::JSXAttributeItem::Attribute(attr) if is_handler_name(&attr_name(&attr.name)) => {}
        js::JSXAttributeItem::Attribute(attr) => out.push(self.jsx_attr(attr)?),
        js::JSXAttributeItem::SpreadAttribute(spread) => out.push(Attr { name: String::new(), value: Value::Spread(&spread.argument), at: At::Span(spread.argument.span()) }),
      }
    }
    Ok(out)
  }

  fn children(&mut self, children: &'p [js::JSXChild<'p>]) -> Lowered<Vec<Tmpl>> {
    let mut out = Vec::new();
    for child in children {
      match child {
        js::JSXChild::Text(text) => {
          let cleaned = jsx_text(text.value.as_str());
          if !cleaned.is_empty() {
            out.push(Tmpl::Text(cleaned));
          }
        }
        js::JSXChild::ExpressionContainer(c) => match c.expression.as_expression() {
          None => {}
          Some(e) => {
            self.lowerer.browser_value = None;
            match self.child_expr(e) {
              Ok(child) => out.push(child),
              // Text only the browser knows: the server writes nothing and the browser copy computes it once mounted, so its first render matches. Markup the value would choose is structure, which stays a residue.
              Err(residue) => match self.lowerer.browser_value.take() {
                Some(reason) if !self.holds_markup(e) => {
                  let (line, column) = self.lowerer.parsed.loc(e.span().start);
                  self.browser_texts.push(BrowserText { wrap: Some(self.lowerer.parsed.range(e.span())), line, column, reason });
                  out.push(Tmpl::Expr(Expr::Var(BROWSER_VAR.to_owned())));
                }
                _ => return Err(residue),
              },
            }
          }
        },
        js::JSXChild::Element(el) => out.push(self.element(el, true)?),
        js::JSXChild::Fragment(frag) => out.push(Tmpl::Fragment(self.children(&frag.children)?)),
        js::JSXChild::Spread(s) => return Err(self.lowerer.residue(s.span, "a spread child")),
      }
    }
    Ok(out)
  }
}

/// The expression an attribute's value holds when it is `{expr}`.
fn attr_expression<'p>(attr: &'p js::JSXAttribute<'p>) -> Option<&'p js::Expression<'p>> {
  match &attr.value {
    Some(js::JSXAttributeValue::ExpressionContainer(c)) => c.expression.as_expression(),
    _ => None,
  }
}

fn attr_name(name: &js::JSXAttributeName) -> String {
  match name {
    js::JSXAttributeName::Identifier(id) => id.name.to_string(),
    js::JSXAttributeName::NamespacedName(n) => format!("{}:{}", n.namespace.name, n.name.name),
  }
}

fn is_handler_name(name: &str) -> bool {
  name.len() > 2 && name.starts_with("on") && name.as_bytes()[2].is_ascii_uppercase()
}

/// The markup a `dangerouslySetInnerHTML` value carries, taken from the object
/// when it is written inline and read as a field otherwise.
fn inner_html(value: Expr) -> Expr {
  if let Expr::Object(entries) = &value {
    if let [Entry::Field(name, html)] = entries.as_slice() {
      if name == "__html" {
        return html.clone();
      }
    }
  }
  Expr::Field(Box::new(value), "__html".to_owned())
}

/// JSX's whitespace rule: lines are trimmed, blank lines dropped, the rest
/// joined by one space; a single line keeps its inner spacing.
fn jsx_text(raw: &str) -> String {
  let decoded = decode_entities(raw);
  if !decoded.contains('\n') {
    return decoded;
  }
  let lines: Vec<&str> = decoded.lines().collect();
  let last = lines.len().saturating_sub(1);
  let mut parts = Vec::new();
  for (i, line) in lines.iter().enumerate() {
    let mut piece: &str = line;
    if i > 0 {
      piece = piece.trim_start();
    }
    if i < last {
      piece = piece.trim_end();
    }
    if !piece.is_empty() {
      parts.push(piece);
    }
  }
  parts.join(" ")
}

fn decode_entities(raw: &str) -> String {
  if !raw.contains('&') {
    return raw.to_owned();
  }
  let mut out = String::with_capacity(raw.len());
  let mut rest = raw;
  while let Some(start) = rest.find('&') {
    out.push_str(&rest[..start]);
    rest = &rest[start..];
    let Some(end) = rest.find(';').filter(|&e| e <= 10) else {
      out.push('&');
      rest = &rest[1..];
      continue;
    };
    let entity = &rest[1..end];
    let decoded = match entity {
      "amp" => Some('&'),
      "lt" => Some('<'),
      "gt" => Some('>'),
      "quot" => Some('"'),
      "apos" => Some('\''),
      "nbsp" => Some('\u{a0}'),
      "copy" => Some('\u{a9}'),
      "reg" => Some('\u{ae}'),
      "minus" => Some('\u{2212}'),
      "times" => Some('\u{d7}'),
      "middot" => Some('\u{b7}'),
      "hellip" => Some('\u{2026}'),
      "mdash" => Some('\u{2014}'),
      "ndash" => Some('\u{2013}'),
      "rarr" => Some('\u{2192}'),
      "larr" => Some('\u{2190}'),
      "laquo" => Some('\u{ab}'),
      "raquo" => Some('\u{bb}'),
      "bull" => Some('\u{2022}'),
      "trade" => Some('\u{2122}'),
      "euro" => Some('\u{20ac}'),
      "pound" => Some('\u{a3}'),
      "yen" => Some('\u{a5}'),
      "deg" => Some('\u{b0}'),
      _ => entity
        .strip_prefix('#')
        .and_then(|n| match n.strip_prefix('x').or_else(|| n.strip_prefix('X')) {
          Some(hex) => u32::from_str_radix(hex, 16).ok(),
          None => n.parse().ok(),
        })
        .and_then(char::from_u32),
    };
    match decoded {
      Some(c) => {
        out.push(c);
        rest = &rest[end + 1..];
      }
      None => {
        out.push('&');
        rest = &rest[1..];
      }
    }
  }
  out.push_str(rest);
  out
}

/// `xs?.map(f)` with `f` an arrow written in place: the array and the callback.
fn optional_map<'e>(expr: &'e js::Expression<'e>) -> Option<(&'e js::Expression<'e>, &'e js::ArrowFunctionExpression<'e>)> {
  let js::Expression::ChainExpression(chain) = expr else { return None };
  let js::ChainElement::CallExpression(call) = &chain.expression else { return None };
  let js::Expression::StaticMemberExpression(member) = &call.callee else { return None };
  if !member.optional || member.property.name != "map" {
    return None;
  }
  let js::Expression::ArrowFunctionExpression(arrow) = crate::syntax::arg_expr(call.arguments.first()?) else { return None };
  Some((&member.object, arrow))
}

/// `xs.map(f)` with `f` an arrow written in place: the array and the callback.
fn map_call<'e>(call: &'e js::CallExpression<'e>) -> Option<(&'e js::Expression<'e>, &'e js::ArrowFunctionExpression<'e>)> {
  let callee = &call.callee;
  let Some(member) = (callee).as_member_expression() else { return None };
  if !matches!(crate::syntax::prop(member), crate::syntax::MemberProp::Ident(id) if id.name.as_str() == "map") {
    return None;
  }
  let js::Expression::ArrowFunctionExpression(arrow) = crate::syntax::arg_expr(call.arguments.first()?) else { return None };
  Some((member.object(), arrow))
}

/// The name a function declaration statement declares.
fn function_name<'s>(stmt: &'s js::Statement<'_>) -> Option<&'s str> {
  match stmt {
    js::Statement::FunctionDeclaration(f) => f.id.as_ref().map(|id| id.name.as_str()),
    _ => None,
  }
}

/// The expressions an array literal lists, a spread's argument among them and a hole left out.
pub(crate) fn array_items<'s, 'a>(arr: &'s js::ArrayExpression<'a>) -> impl Iterator<Item = &'s js::Expression<'a>> {
  arr.elements.iter().filter_map(|elem| match elem {
    js::ArrayExpressionElement::Elision(_) => None,
    js::ArrayExpressionElement::SpreadElement(s) => Some(&s.argument),
    other => Some(other.to_expression()),
  })
}

/// Binds a parameter to `target`, its default applying when `target` is null or absent.
pub(crate) fn bind_param(lowerer: &mut Lowerer<'_>, param: &crate::Param, target: Expr) -> Lowered<()> {
  if param.rest {
    return Err(lowerer.residue(param.span, "a pattern the build does not read"));
  }
  let target = match param.default {
    Some(default) => Expr::Coalesce(Box::new(target), Box::new(lowerer.expr(default)?)),
    None => target,
  };
  bind_pattern(lowerer, param.pattern, target)
}

#[cfg(test)]
mod tests {
  use super::*;
  use snapfire_fsr_ir::ast::{ArithOp, CompareOp};

  fn app(files: &[(&str, &str)]) -> PathBuf {
    static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let dir = snapfire_fsr_test_internal::scratch_root().join(format!("fsr_component_{}_{}", std::process::id(), NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)));
    let _ = std::fs::remove_dir_all(&dir);
    for (name, source) in files {
      let path = dir.join(name);
      std::fs::create_dir_all(path.parent().unwrap()).unwrap();
      std::fs::write(path, source).unwrap();
    }
    dir
  }

  fn lower(files: &[(&str, &str)], module: &str) -> Result<Vec<(String, Component)>, LowerError> {
    let mut set = ComponentSet::new(&app(files));
    set.lower(module)?;
    Ok(set.components)
  }

  /// The attributes the markup prints: without the build's `$` markers.
  fn plain(attrs: &[Entry]) -> Vec<Entry> {
    attrs.iter().filter(|e| !matches!(e, Entry::Field(n, _) if n.starts_with('$'))).cloned().collect()
  }

  fn hoist_ids(component: &Component) -> Vec<u32> {
    let mut ids = Vec::new();
    component.visit(&mut |e| {
      if let Expr::Hoist { id, .. } = e {
        ids.push(*id);
      }
    });
    ids
  }

  fn chunk_ids(tmpl: &Tmpl, out: &mut Vec<(String, u32)>) {
    match tmpl {
      Tmpl::Baked { .. } => {}
      Tmpl::Element { tag, attrs, children } => {
        if let Some(Entry::Field(_, Expr::Lit(Lit::Int(id)))) = attrs.iter().find(|e| matches!(e, Entry::Field(n, _) if n == hoist::CHUNK_ATTR)) {
          out.push((tag.clone(), *id as u32));
        }
        children.iter().for_each(|c| chunk_ids(c, out));
      }
      Tmpl::Fragment(children) | Tmpl::Place { children, .. } | Tmpl::Component { children, .. } | Tmpl::Island { children, .. } => children.iter().for_each(|c| chunk_ids(c, out)),
      Tmpl::If { then, r#else, .. } => {
        chunk_ids(then, out);
        if let Some(e) = r#else {
          chunk_ids(e, out);
        }
      }
      Tmpl::For { body, .. } => chunk_ids(body, out),
      Tmpl::Let { then, .. } => chunk_ids(then, out),
      Tmpl::Text(_) | Tmpl::Marker(_) | Tmpl::Outlet { .. } | Tmpl::Html(_) | Tmpl::Expr(_) | Tmpl::Slot(_) => {}
    }
  }

  fn set(files: &[(&str, &str)], module: &str) -> ComponentSet {
    let mut set = ComponentSet::new(&app(files));
    set.lower(module).unwrap();
    set
  }

  fn placements(tmpl: &Tmpl, out: &mut Vec<(String, bool)>) {
    match tmpl {
      Tmpl::Component { module, children, keyed, .. } => {
        out.push((module.clone(), *keyed));
        children.iter().for_each(|c| placements(c, out));
      }
      Tmpl::Element { children, .. } | Tmpl::Fragment(children) | Tmpl::Island { children, .. } => children.iter().for_each(|c| placements(c, out)),
      Tmpl::If { then, r#else, .. } => {
        placements(then, out);
        if let Some(e) = r#else {
          placements(e, out);
        }
      }
      Tmpl::For { body, .. } => placements(body, out),
      Tmpl::Let { then, .. } => placements(then, out),
      _ => {}
    }
  }

  #[test]
  fn a_placement_of_a_component_that_keys_anything_is_keyed_and_wrapped() {
    let files = [
      (
        "routes/index/page.tsx",
        r#"
import Card from "@src/ui/Card";
import Plain from "@src/ui/Plain";
import Tree from "@src/ui/Tree";
export default function Index({ items, tree }: { items: string[]; tree: { label: string; kids: any[] } }) {
  return (
    <div>
      {items.map((item) => <Card label={item} />)}
      <Card label="last" />
      <Plain />
      <Tree node={tree} />
    </div>
  );
}
"#,
      ),
      (
        "src/ui/Card.tsx",
        r#"
import { Island } from "@snapfire/fsr/client/react";
import Body from "@src/ui/Body";
export default function Card({ label }: { label: string }) {
  return <Island when="load"><Body label={label} /></Island>;
}
"#,
      ),
      ("src/ui/Body.tsx", "import { useState } from \"react\";\nexport default function Body({ label }: { label: string }) {\n  const [n, setN] = useState(0);\n  return <button onClick={() => setN(n + 1)}>{label}{n}</button>;\n}\n"),
      ("src/ui/Plain.tsx", "export default function Plain() {\n  return <i>plain</i>;\n}\n"),
      ("src/ui/Tree.tsx", "export default function Tree({ node }: { node: { label: string; kids: any[] } }) {\n  return <ul><li>{node.label}</li>{node.kids.map((kid) => <Tree node={kid} />)}</ul>;\n}\n"),
    ];
    let set = set(&files, "routes/index/page.tsx#default");
    let page = &set.components.iter().find(|(m, _)| m == "routes/index/page.tsx#default").unwrap().1;
    let mut placed = Vec::new();
    placements(&page.render, &mut placed);
    assert_eq!(
      placed,
      [("src/ui/Card.tsx#default".to_owned(), true), ("src/ui/Card.tsx#default".to_owned(), true), ("src/ui/Plain.tsx#default".to_owned(), false), ("src/ui/Tree.tsx#default".to_owned(), true)],
      "Card keys a region and Tree renders itself; Plain keys nothing"
    );
    let tree = &set.components.iter().find(|(m, _)| m == "src/ui/Tree.tsx#default").unwrap().1;
    let mut placed = Vec::new();
    placements(&tree.render, &mut placed);
    assert_eq!(placed, [("src/ui/Tree.tsx#default".to_owned(), true)], "a component met in its own cycle counts as keyed");
    let rewritten: HashMap<String, String> = set.rewritten().into_iter().collect();
    let source = &rewritten["routes/index/page.tsx"];
    assert!(source.contains("{items.map(__sfh.l((item) => __sfh.p(0, <Card label={item} />)))}"), "{source}");
    assert!(source.contains("{__sfh.p(1, <Card label=\"last\" />)}"), "{source}");
    assert!(source.contains("      <Plain />\n"), "an unkeyed placement is left as written: {source}");
    assert!(source.contains("{__sfh.p(3, <Tree node={tree} />)}"), "{source}");
    assert!(rewritten["src/ui/Tree.tsx"].contains("{node.kids.map(__sfh.l((kid) => __sfh.p(0, <Tree node={kid} />)))}"), "{}", rewritten["src/ui/Tree.tsx"]);
    assert!(!rewritten.contains_key("src/ui/Plain.tsx"), "a component that keys nothing is not rewritten");
  }

  #[test]
  fn dangerously_set_inner_html_lowers_to_the_markup_it_names() {
    let files = [(
      "routes/index/page.tsx",
      r#"
export default function Page({ blip, note }: { blip: { body: string }; note: { __html: string } }) {
  return <article><div className="body" dangerouslySetInnerHTML={{ __html: blip.body }} /><aside dangerouslySetInnerHTML={note} /></article>;
}
"#,
    )];
    let set = set(&files, "routes/index/page.tsx#default");
    let component = &set.components[0].1;
    let Tmpl::Element { children, .. } = &component.render else { panic!("{:?}", component.render) };
    let Tmpl::Element { tag, attrs, children: inner } = &children[0] else { panic!("{:?}", children[0]) };
    assert_eq!(tag, "div");
    assert!(inner.is_empty(), "{inner:?}");
    assert_eq!(attrs[1], Entry::Field(RAW_ATTR.to_owned(), Expr::var("$props").field("blip").field("body")), "the object is written inline, so the IR holds what it named: {attrs:?}");
    let Tmpl::Element { attrs, .. } = &children[1] else { panic!("{:?}", children[1]) };
    assert_eq!(attrs[0], Entry::Field(RAW_ATTR.to_owned(), Expr::var("$props").field("note").field("__html")), "an object passed whole is read for its field: {attrs:?}");
    assert!(!format!("{component:?}").contains(UNLOWERED_ATTR), "the component lowers rather than falling to the browser: {component:?}");

    let map = |pairs: &[(&str, &str)]| {
      let mut out = snapfire_fsr_core::ValueMap::default();
      for (k, v) in pairs {
        out.insert((*k).to_owned(), snapfire_fsr_core::Value::str(*v));
      }
      out
    };
    let mut props = snapfire_fsr_core::ValueMap::default();
    props.insert("blip".to_owned(), snapfire_fsr_core::Value::Map(map(&[("body", "<p>a <b>markdown</b> blip</p>")])));
    props.insert("note".to_owned(), snapfire_fsr_core::Value::Map(map(&[("__html", "<i>&amp; a note</i>")])));
    let html = snapfire_fsr_ir::Interpreter::default().render(component, &props, &snapfire_fsr_ir::render::Components::new()).unwrap().html;
    assert_eq!(html, "<article><div class=\"body\"><p>a <b>markdown</b> blip</p></div><aside><i>&amp; a note</i></aside></article>", "the markdown a loader produced reaches the document as markup");
  }

  #[test]
  fn a_helper_call_on_props_is_hoisted_and_one_on_state_is_not() {
    let files = [
      (
        "routes/cart/page.tsx",
        r#"
import { useState } from "react";
import { useStore } from "@snapfire/fsr/client/react";
import { money } from "@src/ui/money";
export default function Cart({ lines, total }: { lines: { price: number }[]; total: number }) {
  const [qty, setQty] = useState(1);
  const [tip] = useStore("cart/tip", 0);
  const grand = total + tip;
  return (
    <div>
      <b>{money(total)}</b>
      <i>{money(total * qty)}</i>
      <u>{money(grand)}</u>
      <ul>{lines.map((l) => <li key={l.price}>{money(l.price)}{money(l.price * qty)}</li>)}</ul>
      <button onClick={() => setQty(qty + 1)}>{money(1)}</button>
    </div>
  );
}
"#,
      ),
      ("src/ui/money.ts", "export function money(cents: number): string {\n  return `$${(cents / 100).toFixed(2)}`;\n}\n"),
    ];
    let set = set(&files, "routes/cart/page.tsx#default");
    let component = &set.components[0].1;
    assert_eq!(hoist_ids(component), vec![6, 10], "props only: money(l.price) and money(1); not qty, tip or grand; money(total) sits in the chunk whose markup the browser adopts: {component:?}");
    let mut chunks = Vec::new();
    chunk_ids(&component.render, &mut chunks);
    assert_eq!(chunks, vec![("b".to_owned(), 1)], "the one static element that does work; the others read state, hold a handler or only literals: {component:?}");
    assert_eq!(set.rewrites.len(), 1);
    let rewrite = &set.rewrites[0];
    assert_eq!(rewrite.module, "routes/cart/page.tsx#default");
    assert_eq!(rewrite.sites.len(), 2);
    assert_eq!(rewrite.chunks.len(), 1);
    assert!(rewrite.chunks[0].proven);
    assert_eq!(rewrite.loops.len(), 1, "the one .map callback holding a survivor");
    let (file, source) = set.rewritten().pop().unwrap();
    assert_eq!(file, "routes/cart/page.tsx");
    assert!(source.starts_with(hoist::IMPORT), "{source}");
    assert!(source.contains("{ const __sfh = __sfUseHoisted(\"routes/cart/page.tsx#default\"); "), "{source}");
    assert!(source.contains("{__sfh.c(1, (__sfHtml) => <b dangerouslySetInnerHTML={__sfHtml} />)}"), "a proven chunk is its hit alone: {source}");
    assert!(!source.contains("money(total)"), "the chunk's own read goes with its fallback: {source}");
    assert!(source.contains("<i>{money(total * qty)}</i>"), "a state read stays a call: {source}");
    assert!(source.contains("<ul>{lines.map(__sfh.l((l) => <li key={l.price}>{__sfh.r(6, () => (money(l.price)))}{money(l.price * qty)}</li>))}</ul>"), "{source}");
    assert!(source.contains("{__sfh.r(10, () => (money(1)))}</button>"), "{source}");
  }

  #[test]
  fn a_component_a_browser_only_one_imports_keeps_its_chunks_jsx() {
    let card = "import { money } from \"@src/ui/money\";\nexport default function Card({ total }: { total: number }) {\n  return <div><b>{money(total)}</b></div>;\n}\n";
    let files = [
      ("routes/a/page.tsx", "import Card from \"@src/ui/Card\";\nexport default function A({ total }: { total: number }) {\n  return <Card total={total} />;\n}\n"),
      ("routes/b/page.tsx", "import Shell from \"@src/ui/Shell\";\nexport default function B({ total }: { total: number }) {\n  return <Shell total={total} />;\n}\n"),
      ("src/ui/Shell.tsx", "import Card from \"./Card\";\nexport default function Shell({ total }: { total: number }) {\n  const clock = new Clock();\n  return <section data-clock={String(clock)}><Card total={total} /></section>;\n}\n"),
      ("src/ui/Card.tsx", card),
      ("src/ui/money.ts", "export function money(cents: number): string {\n  return `$${(cents / 100).toFixed(2)}`;\n}\n"),
    ];
    let mut set = ComponentSet::new(&app(&files));
    set.lower("routes/a/page.tsx#default").unwrap();
    set.lower("routes/b/page.tsx#default").unwrap();
    assert!(set.browser_only.iter().any(|(m, _)| m == "src/ui/Shell.tsx#default"), "{:?}", set.browser_only);
    set.settle_proofs();
    let rewrite = set.rewrites.iter().find(|r| r.module == "src/ui/Card.tsx#default").expect("Card is rewritten");
    assert!(!rewrite.chunks.is_empty() && rewrite.chunks.iter().all(|c| !c.proven), "Shell mounts Card fresh in the browser: {:?}", rewrite.chunks);
    let (_, source) = set.rewritten().into_iter().find(|(f, _)| f == "src/ui/Card.tsx").unwrap();
    assert!(source.contains("money(total)"), "the chunk keeps the JSX it falls back to: {source}");
  }

  #[test]
  fn a_handler_calls_an_action_and_the_call_lowers_to_act() {
    let files = [
      (
        "routes/index/page.tsx",
        r#"
import { Island } from "@snapfire/fsr/client/react";
import { Lot } from "@src/Lot";
export default function Page() {
  return <Island mode="server"><Lot size={10} /></Island>;
}
"#,
      ),
      (
        "src/Lot.tsx",
        r#"
import { useState } from "react";
import { action } from "@snapfire/fsr/client";
import { actions } from "@generated/client";
const step = action("desk.lot");
const save = action("desk.save");
function shout() {}
export function Lot({ size }: { size: number }) {
  const [open, setOpen] = useState(false);
  async function keep() {
    await actions.$root.keep({ size });
  }
  return (
    <div>
      <button onClick={() => void step({ by: 10 })}>+</button>
      <button onClick={() => { setOpen(!open); void save({ size, open: !open }); }}>save</button>
      <button onClick={() => void save()}>bare</button>
      <button onClick={() => void keep()}>generated</button>
      <button onClick={() => void shout()}>shout</button>
    </div>
  );
}
"#,
      ),
    ];
    let set = set(&files, "routes/index/page.tsx#default");
    let lot = &set.components.iter().find(|(m, _)| m == "src/Lot.tsx#Lot").unwrap().1;
    assert_eq!(lot.handlers.len(), 4, "{:?}", lot.handlers);
    assert_eq!(lot.handlers[0].body, vec![Stmt::Act { action: "desk.lot".to_owned(), input: Expr::Object(vec![Entry::Field("by".to_owned(), Expr::Lit(Lit::Float(10.0)))]) }, Stmt::Return(Expr::Object(Vec::new()))], "a handler that only calls an action sets no state and is still a handler");
    let flipped = Expr::Not(Box::new(Expr::var("open")));
    assert_eq!(
      lot.handlers[1].body,
      vec![
        Stmt::Act { action: "desk.save".to_owned(), input: Expr::Object(vec![Entry::Field("size".to_owned(), Expr::var("$props").field("size")), Entry::Field("open".to_owned(), flipped.clone())]) },
        Stmt::Return(Expr::Object(vec![Entry::Field("open".to_owned(), flipped)])),
      ],
      "the input reads props and state; the patch follows"
    );
    assert_eq!(lot.handlers[2].body, vec![Stmt::Act { action: "desk.save".to_owned(), input: Expr::Object(Vec::new()) }, Stmt::Return(Expr::Object(Vec::new()))], "no argument is an empty input");
    assert_eq!(
      lot.handlers[3].body,
      vec![Stmt::Act { action: "$root.keep".to_owned(), input: Expr::Object(vec![Entry::Field("size".to_owned(), Expr::var("$props").field("size"))]) }, Stmt::Return(Expr::Object(Vec::new()))],
      "the generated client's own call is an act too, its id the property path"
    );
    let Tmpl::Element { children, .. } = &lot.render else { panic!() };
    let Tmpl::Element { attrs, .. } = &children[4] else { panic!() };
    let unlowered = attrs.iter().find_map(|e| match e {
      Entry::Field(n, Expr::Lit(Lit::Str(why))) if n == UNLOWERED_ATTR => Some(why.clone()),
      _ => None,
    });
    assert!(unlowered.is_some_and(|why| why.contains("not a state setter, a write to the store, an action or a handler")), "a call to something else is still residue: {attrs:?}");
  }

  #[test]
  fn a_branch_in_a_handler_conditions_each_key_it_sets() {
    let files = [
      (
        "routes/index/page.tsx",
        r#"
import { Island } from "@snapfire/fsr/client/react";
import { Gate } from "@src/Gate";
export default function Page() {
  return <Island mode="server"><Gate limit={3} /></Island>;
}
"#,
      ),
      (
        "src/Gate.tsx",
        r#"
import { useState } from "react";
import { action } from "@snapfire/fsr/client";
const save = action("desk.save");
export function Gate({ limit }: { limit: number }) {
  const [n, setN] = useState(0);
  const [open, setOpen] = useState(false);
  const [note, setNote] = useState("");
  function bump() {
    setN(n + 1);
  }
  return (
    <div>
      <button onClick={() => { if (n < limit) setN(n + 1); }}>a</button>
      <button onClick={() => { setOpen(true); if (n >= limit) { setOpen(false); setNote("full"); } else if (n === 0) { setNote("first"); } else { const rest = limit - n; setNote(String(rest)); } }}>b</button>
      <button onClick={() => { if (!open) return; setN(0); }}>c</button>
      <button onClick={() => { if (open) { void save({ n }); bump(); } setNote("saved"); }}>d</button>
      <button onClick={() => { if (n > 0) { const x = 1; setN(x); } else { const x = 2; setN(x); } }}>e</button>
      <button onClick={(e) => { if (open) e.preventDefault(); }}>f</button>
    </div>
  );
}
"#,
      ),
    ];
    let set = set(&files, "routes/index/page.tsx#default");
    let gate = &set.components.iter().find(|(m, _)| m == "src/Gate.tsx#Gate").unwrap().1;
    assert_eq!(gate.handlers.len(), 5, "{:?}", gate.handlers);
    let num = |v: f64| Expr::Lit(Lit::Float(v));
    let n = || Expr::var("n");
    let limit = || Expr::var("$props").field("limit");
    let cmp = |op: CompareOp, a: Expr, b: Expr| Expr::Compare(op, Box::new(a), Box::new(b));
    let not = |e: Expr| Expr::Not(Box::new(e));
    let and = |a: Expr, b: Expr| Expr::Logic(LogicOp::And, Box::new(a), Box::new(b));
    let pick = |c: Expr, a: Expr, b: Expr| Expr::Ternary(Box::new(c), Box::new(a), Box::new(b));
    let plus_one = || Expr::Arith(snapfire_fsr_ir::ArithOp::Add, Box::new(Expr::var("n")), Box::new(Expr::Lit(Lit::Float(1.0))));
    let field = |name: &str, value: Expr| Entry::Field(name.to_owned(), value);

    assert_eq!(
      gate.handlers[0].body,
      vec![Stmt::Return(Expr::Object(vec![field("n", pick(cmp(CompareOp::Lt, n(), limit()), plus_one(), n()))]))],
      "a key set only inside a branch keeps the state where the branch did not run"
    );

    let full = cmp(CompareOp::Ge, n(), limit());
    let first = cmp(CompareOp::Eq, n(), num(0.0));
    let rest_reach = and(not(full.clone()), not(first.clone()));
    let [Stmt::Let { name: rest, expr: rest_init }, Stmt::Return(Expr::Object(patch))] = gate.handlers[1].body.as_slice() else { panic!("{:?}", gate.handlers[1].body) };
    assert_eq!(rest, "rest$1", "a branch's const is hoisted under its own name");
    assert_eq!(*rest_init, pick(rest_reach.clone(), Expr::Arith(snapfire_fsr_ir::ArithOp::Sub, Box::new(limit()), Box::new(n())), Expr::Lit(Lit::Null)));
    assert_eq!(patch[0], field("open", pick(full.clone(), Expr::Lit(Lit::Bool(false)), Expr::Lit(Lit::Bool(true)))), "a set before the branch is the prior of the set inside it");
    let Entry::Field(name, Expr::Ternary(reach, _, prior)) = &patch[1] else { panic!("{:?}", patch[1]) };
    assert_eq!(name, "note");
    assert_eq!(**reach, rest_reach, "an else if chain nests its conditions");
    assert_eq!(**prior, pick(and(not(full.clone()), first), Expr::lit_str("first"), pick(full, Expr::lit_str("full"), Expr::var("note"))));
    assert_eq!(patch.len(), 2);

    assert_eq!(
      gate.handlers[2].body,
      vec![Stmt::Return(Expr::Object(vec![field("n", pick(not(not(Expr::var("open"))), num(0.0), n()))]))],
      "a bare return inside a branch leaves the rest reachable only when the branch did not run"
    );

    let open = || Expr::var("open");
    assert_eq!(
      gate.handlers[3].body,
      vec![
        Stmt::If { cond: open(), then: vec![Stmt::Act { action: "desk.save".to_owned(), input: Expr::Object(vec![field("n", n())]) }], r#else: Vec::new() },
        Stmt::Return(Expr::Object(vec![field("n", pick(open(), plus_one(), n())), field("note", Expr::lit_str("saved"))])),
      ],
      "an action inside a branch runs under it and an inlined handler's set is conditioned the same way"
    );

    let positive = cmp(CompareOp::Gt, n(), num(0.0));
    assert_eq!(
      gate.handlers[4].body,
      vec![
        Stmt::Let { name: "x$1".to_owned(), expr: pick(positive.clone(), num(1.0), Expr::Lit(Lit::Null)) },
        Stmt::Let { name: "x$2".to_owned(), expr: pick(not(positive.clone()), num(2.0), Expr::Lit(Lit::Null)) },
        Stmt::Return(Expr::Object(vec![field("n", pick(not(positive.clone()), Expr::var("x$2"), pick(positive, Expr::var("x$1"), n())))])),
      ],
      "the same name declared in both branches binds twice"
    );

    let Tmpl::Element { children, .. } = &gate.render else { panic!() };
    let Tmpl::Element { attrs, .. } = &children[5] else { panic!() };
    let unlowered = attrs.iter().find_map(|e| match e {
      Entry::Field(n, Expr::Lit(Lit::Str(why))) if n == UNLOWERED_ATTR => Some(why.clone()),
      _ => None,
    });
    assert!(unlowered.is_some_and(|why| why.contains("sets no state")), "a branch that sets nothing is still a handler that sets nothing: {attrs:?}");
  }

  #[test]
  fn a_react_handler_writes_the_store_through_set_transaction_and_optimistic() {
    let files = [
      (
        "routes/index/page.tsx",
        r#"
import { Island } from "@snapfire/fsr/client/react";
import { Sizes } from "@src/Sizes";
import { Stepper } from "@src/Stepper";
export default function Page() {
  return <main><Island mode="server"><Sizes /></Island><Island mode="server"><Stepper /></Island></main>;
}
"#,
      ),
      (
        "src/keys.ts",
        r#"
import { key } from "@snapfire/fsr/client/store";
export const size = key<string>("order/size");
export const price = key<number>("order/price");
export const qty = key<number>("order/qty");
"#,
      ),
      (
        "src/Sizes.tsx",
        r#"
import { useStore } from "@snapfire/fsr/client/react";
import { set, transaction } from "@snapfire/fsr/client/store";
import { price, size } from "@src/keys";
export function Sizes() {
  const [chosen] = useStore(size, "250g");
  return <button className={chosen} onClick={() => transaction(() => { set(size, "1kg"); set(price, 40); })}>1kg</button>;
}
"#,
      ),
      (
        "src/Stepper.tsx",
        r#"
import { useStore } from "@snapfire/fsr/client/react";
import { optimistic } from "@snapfire/fsr/client/store";
import { action } from "@snapfire/fsr/client";
import { qty } from "@src/keys";
const setQty = action("order.setQty");
export function Stepper() {
  const [count] = useStore(qty, 1);
  return (
    <div>
      <button onClick={() => optimistic(qty, count + 1, () => setQty({ qty: count + 1 }))}>+</button>
      <button onClick={async () => { try { await optimistic(qty, count - 1, () => setQty({ qty: count - 1 })); } catch (e) { console.error(e); } }}>-</button>
    </div>
  );
}
"#,
      ),
    ];
    let set = set(&files, "routes/index/page.tsx#default");
    let sizes = &set.components.iter().find(|(m, _)| m == "src/Sizes.tsx#Sizes").unwrap().1;
    assert_eq!(sizes.stores, [("chosen".to_owned(), "order/size".to_owned()), ("$store1".to_owned(), "order/price".to_owned())], "the key the component does not read gets a binding of its own: {:?}", sizes.stores);
    assert_eq!(sizes.body.last(), Some(&Stmt::Let { name: "$store1".to_owned(), expr: Expr::Store("order/price".to_owned()) }));
    assert_eq!(sizes.handlers.len(), 1);
    assert_eq!(sizes.handlers[0].body, vec![Stmt::Return(Expr::Object(vec![Entry::Field("chosen".to_owned(), Expr::lit_str("1kg")), Entry::Field("$store1".to_owned(), Expr::Lit(Lit::Float(40.0)))]))]);
    let stepper = &set.components.iter().find(|(m, _)| m == "src/Stepper.tsx#Stepper").unwrap().1;
    assert_eq!(stepper.stores, [("count".to_owned(), "order/qty".to_owned())]);
    assert_eq!(stepper.handlers.len(), 2);
    let up = Expr::Arith(snapfire_fsr_ir::ArithOp::Add, Box::new(Expr::var("count")), Box::new(Expr::Lit(Lit::Float(1.0))));
    assert_eq!(stepper.handlers[0].body, vec![Stmt::Act { action: "order.setQty".to_owned(), input: Expr::Object(vec![Entry::Field("qty".to_owned(), up.clone())]) }, Stmt::Return(Expr::Object(vec![Entry::Field("count".to_owned(), up)]))]);
    let down = Expr::Arith(snapfire_fsr_ir::ArithOp::Sub, Box::new(Expr::var("count")), Box::new(Expr::Lit(Lit::Float(1.0))));
    let body = &stepper.handlers[1].body;
    let act = Stmt::Act { action: "order.setQty".to_owned(), input: Expr::Object(vec![Entry::Field("qty".to_owned(), down.clone())]) };
    let tried = body.iter().find_map(|stmt| match stmt {
      Stmt::Try { body, .. } => Some(body),
      _ => None,
    });
    assert!(tried.is_some_and(|tried| tried.contains(&act)), "the action is dispatched inside the try: {body:?}");
    assert!(matches!(body.last(), Some(Stmt::Return(Expr::Object(fields))) if matches!(fields.as_slice(), [Entry::Field(name, Expr::Var(_))] if name == "count")), "the patch sets the count the try set: {body:?}");
  }

  #[test]
  fn handlers_lower_to_state_patches_and_an_island_takes_a_mode() {
    let files = [
      (
        "routes/index/page.tsx",
        r#"
import { Island, island } from "@snapfire/fsr/client/react";
import { Stepper } from "@src/Stepper";
const Lazy = island(Stepper, { when: "idle", mode: "server" });
export default function Page() {
  return <main><Island mode="server"><Stepper start={1} /></Island><Lazy start={2} /><Island mode="browser"><Stepper start={3} /></Island></main>;
}
"#,
      ),
      (
        "src/Stepper.tsx",
        r#"
import { useState } from "react";
export function Stepper({ start, max = 9 }: { start: number; max?: number }) {
  const [n, setN] = useState(start);
  const [open, setOpen] = useState(false);
  const room = max - n;
  function reset() {
    setN(start);
    setOpen(false);
  }
  return (
    <div key="stepper">
      <button onClick={() => setN(n + 1)} disabled={room === 0}>+</button>
      <button onClick={() => setN((prev) => prev - 1)}>-</button>
      <input value={String(n)} onChange={(e) => { e.preventDefault(); const next = Number(e.target.value); setN(next); }} />
      <button onClick={reset}>reset</button>
      <button onClick={() => void reset()}>reset too</button>
      <button onClick={() => alert(n)}>shout</button>
      <label onClick={() => setOpen(!open)}>{open ? "open" : "closed"}</label>
      <ul>{[1, 2].map((i) => <li key={i}>{i}</li>)}</ul>
    </div>
  );
}
"#,
      ),
    ];
    let set = set(&files, "routes/index/page.tsx#default");
    let page = &set.components.iter().find(|(m, _)| m == "routes/index/page.tsx#default").unwrap().1;
    let Tmpl::Element { children, .. } = &page.render else { panic!() };
    assert!(matches!(&children[0], Tmpl::Island { mode: Some(m), when: None, .. } if m == "server"), "{:?}", children[0]);
    assert!(matches!(&children[1], Tmpl::Island { mode: Some(m), when: Some(w), .. } if m == "server" && w == "idle"), "{:?}", children[1]);
    assert!(matches!(&children[2], Tmpl::Island { mode: None, .. }), "browser is the default and spells as none: {:?}", children[2]);

    let stepper = &set.components.iter().find(|(m, _)| m == "src/Stepper.tsx#Stepper").unwrap().1;
    assert_eq!(stepper.state, ["n", "open"]);
    assert_eq!(stepper.handlers.len(), 6, "{:?}", stepper.handlers.iter().map(|h| &h.event).collect::<Vec<_>>());
    let events: Vec<&str> = stepper.handlers.iter().map(|h| h.event.as_str()).collect();
    assert_eq!(events, ["click", "click", "change", "click", "click", "click"]);
    assert_eq!(stepper.handlers[0].body, vec![Stmt::Return(Expr::Object(vec![Entry::Field("n".to_owned(), Expr::Arith(snapfire_fsr_ir::ArithOp::Add, Box::new(Expr::var("n")), Box::new(Expr::Lit(Lit::Float(1.0)))))]))]);
    assert_eq!(stepper.handlers[1].body, vec![Stmt::Return(Expr::Object(vec![Entry::Field("n".to_owned(), Expr::Arith(snapfire_fsr_ir::ArithOp::Sub, Box::new(Expr::var("n")), Box::new(Expr::Lit(Lit::Float(1.0)))))]))], "a functional update reads the state as prev");
    assert_eq!(stepper.handlers[2].body, vec![Stmt::Let { name: "next".to_owned(), expr: Expr::Num(Box::new(Expr::var("$event").field("target").field("value"))) }, Stmt::Return(Expr::Object(vec![Entry::Field("n".to_owned(), Expr::var("next"))]))], "preventDefault is dropped, the event reads through $event");
    let reset = vec![Stmt::Return(Expr::Object(vec![Entry::Field("n".to_owned(), Expr::var("$props").field("start")), Entry::Field("open".to_owned(), Expr::Lit(Lit::Bool(false)))]))];
    assert_eq!(stepper.handlers[3].body, reset, "a named handler by name");
    assert_eq!(stepper.handlers[4].body, reset, "a named handler called");
    assert_eq!(stepper.handlers[5].body, vec![Stmt::Return(Expr::Object(vec![Entry::Field("open".to_owned(), Expr::Not(Box::new(Expr::var("open"))))]))]);

    let Tmpl::Element { attrs, children, .. } = &stepper.render else { panic!() };
    assert!(attrs.contains(&Entry::Field(KEY_ATTR.to_owned(), Expr::lit_str("stepper"))), "{attrs:?}");
    let on = |i: usize| -> Vec<(String, Expr)> {
      let Tmpl::Element { attrs, .. } = &children[i] else { panic!() };
      attrs.iter().filter_map(|e| match e {
        Entry::Field(n, v) if n.starts_with(HANDLER_ATTR) || n == UNLOWERED_ATTR => Some((n.clone(), v.clone())),
        _ => None,
      }).collect()
    };
    assert_eq!(on(0), vec![(format!("{HANDLER_ATTR}click"), Expr::Lit(Lit::Int(0)))]);
    assert_eq!(on(2), vec![(format!("{HANDLER_ATTR}change"), Expr::Lit(Lit::Int(2)))]);
    let shout = on(5);
    assert_eq!(shout.len(), 1);
    assert!(matches!(&shout[0], (n, Expr::Lit(Lit::Str(why))) if n == UNLOWERED_ATTR && why.contains("a call to `alert`")), "{shout:?}");
  }

  #[test]
  fn a_static_subtree_is_a_chunk_and_a_handler_state_read_island_or_impure_component_breaks_it() {
    let files = [
      (
        "routes/index/page.tsx",
        r#"
import { useState } from "react";
import { Island } from "@snapfire/fsr/client/react";
import { Price } from "@src/Price";
import { Counter } from "@src/Counter";
import { Chart } from "@src/Chart";
export default function Page({ items, note }: { items: { id: number; price: number }[]; note: string }) {
  const [open, setOpen] = useState(false);
  return (
    <main>
      <h1>Catalog</h1>
      <ul className="list">
        {items.map((it) => (
          <li key={it.id} title={String(it.id)}>
            <Price cents={it.price} />
          </li>
        ))}
      </ul>
      <section>
        <p>{note}</p>
        <Counter start={1} />
      </section>
      <aside>
        <p>{note}</p>
        <Island when="visible"><Chart series={note} /></Island>
      </aside>
      <div>
        <button onClick={() => setOpen(!open)}>{note}</button>
      </div>
      <footer>{open ? note : ""}</footer>
    </main>
  );
}
"#,
      ),
      ("src/Price.tsx", "export function Price({ cents }: { cents: number }) {\n  return <b>{(cents / 100).toFixed(2)}</b>;\n}\n"),
      ("src/Counter.tsx", "import { useState } from \"react\";\nexport function Counter({ start }: { start: number }) {\n  const [n, setN] = useState(start);\n  return <button onClick={() => setN(n + 1)}>{n}</button>;\n}\n"),
      ("src/Chart.tsx", "export function Chart({ series }: { series: string }) {\n  return <svg><title>{series}</title></svg>;\n}\n"),
    ];
    let set = set(&files, "routes/index/page.tsx#default");
    assert_eq!(set.pure.get("src/Price.tsx#Price"), Some(&true));
    assert_eq!(set.pure.get("src/Counter.tsx#Counter"), Some(&false), "state and a handler");
    assert_eq!(set.pure.get("routes/index/page.tsx#default"), Some(&false));
    let page = &set.components.iter().find(|(m, _)| m == "routes/index/page.tsx#default").unwrap().1;
    let mut chunks = Vec::new();
    chunk_ids(&page.render, &mut chunks);
    let tags: Vec<&str> = chunks.iter().map(|(t, _)| t.as_str()).collect();
    assert_eq!(tags, ["ul", "p", "p"], "the list with its pure Price cards is one chunk; the paragraphs beside the impure Counter and the island are chunks of their own; the h1 is literal, the button is bound and the footer reads state: {chunks:?}");
    let rewrite = set.rewrites.iter().find(|r| r.module == "routes/index/page.tsx#default").unwrap();
    assert_eq!(rewrite.chunks.len(), 3);
    assert!(rewrite.loops.is_empty(), "the loop sits inside the chunk, whose fallback renders it as written");
    let (_, source) = set.rewritten().into_iter().find(|(f, _)| f == "routes/index/page.tsx").unwrap();
    assert!(source.contains("{__sfh.c(2, (__sfHtml) => <ul className=\"list\" dangerouslySetInnerHTML={__sfHtml} />)}\n"), "a chunk among JSX children is braced and a proven one is its hit alone: {source}");
    assert!(!source.contains("<Price"), "the list's cards go with the fallback: {source}");
    assert!(source.contains("{__sfh.c(3, (__sfHtml) => <p dangerouslySetInnerHTML={__sfHtml} />)}"), "{source}");
    assert!(source.contains("<button onClick={() => setOpen(!open)}>{note}</button>"), "a bound element is untouched: {source}");
    assert_eq!(source.matches("__sfh.c(").count(), 3, "{source}");
  }

  const PANEL: &str = "export function Panel({ n }: { n: number }) {\n  return <div className=\"panel\"><b>{n}</b><i>{(n / 100).toFixed(2)}</i></div>;\n}\n";
  const PANEL_ON_PROPS: &str = "import { Panel } from \"@src/Panel\";\nexport default function Fixed({ n }: { n: number }) {\n  return <main><Panel n={n} /></main>;\n}\n";
  const PANEL_ON_STATE: &str = "import { useStore } from \"@snapfire/fsr/client/react\";\nimport { Panel } from \"@src/Panel\";\nexport default function Live() {\n  const [n] = useStore(\"demo/n\", 0);\n  return <main><Panel n={n} /></main>;\n}\n";

  fn panel(set: &ComponentSet) -> (Vec<(String, u32)>, Vec<u32>, usize) {
    let component = &set.components.iter().find(|(m, _)| m == "src/Panel.tsx#Panel").unwrap().1;
    let mut chunks = Vec::new();
    chunk_ids(&component.render, &mut chunks);
    let rewritten = set.rewrites.iter().find(|r| r.module == "src/Panel.tsx#Panel").map(|r| r.chunks.len() + r.sites.len()).unwrap_or(0);
    (chunks, hoist_ids(component), rewritten)
  }

  #[test]
  fn a_component_placed_only_with_props_keeps_its_chunks() {
    let set = set(&[("routes/fixed/page.tsx", PANEL_ON_PROPS), ("src/Panel.tsx", PANEL)], "routes/fixed/page.tsx#default");
    let (chunks, _, rewritten) = panel(&set);
    assert_eq!(chunks.iter().map(|(t, _)| t.as_str()).collect::<Vec<_>>(), ["div"], "props are fixed for the placement's life: {chunks:?}");
    assert!(rewritten > 0);
  }

  #[test]
  fn a_component_placed_with_state_hoists_nothing_its_props_reach() {
    let set = set(&[("routes/live/page.tsx", PANEL_ON_STATE), ("src/Panel.tsx", PANEL)], "routes/live/page.tsx#default");
    let (chunks, hoists, rewritten) = panel(&set);
    assert!(chunks.is_empty(), "the store moves `n` after hydration, so markup rendered from the seed goes stale: {chunks:?}");
    assert!(hoists.is_empty(), "{hoists:?}");
    assert_eq!(rewritten, 0, "the browser renders the panel from its props on every render");
  }

  #[test]
  fn a_placement_with_state_unhoists_a_component_another_placement_lowered_first() {
    let mut set = ComponentSet::new(&app(&[("routes/fixed/page.tsx", PANEL_ON_PROPS), ("routes/live/page.tsx", PANEL_ON_STATE), ("src/Panel.tsx", PANEL)]));
    set.lower("routes/fixed/page.tsx#default").unwrap();
    set.lower("routes/live/page.tsx#default").unwrap();
    let (chunks, hoists, rewritten) = panel(&set);
    assert!(chunks.is_empty() && hoists.is_empty() && rewritten == 0, "one placement with state decides for every placement, since the component is lowered once: {chunks:?} {hoists:?} {rewritten}");
  }

  #[test]
  fn hoists_stay_out_of_lambdas_nested_calls_and_client_components() {
    let files = [
      (
        "routes/index/page.tsx",
        r#"
import { money, wrap } from "@src/ui/money";
export const Page = ({ items, n }: { items: number[]; n: number }) => (
  <p title={items.map((i) => money(i)).join(", ")}>{wrap(money(n))}{items.length.toLocaleString()}</p>
);
"#,
      ),
      ("src/ui/money.ts", "export function money(cents: number): string {\n  return `$${(cents / 100).toFixed(2)}`;\n}\nexport function wrap(s: string): string {\n  return `[${s}]`;\n}\n"),
    ];
    let set = set(&files, "routes/index/page.tsx#Page");
    let component = &set.components[0].1;
    let mut hoisted = Vec::new();
    component.visit(&mut |e| {
      if let Expr::Hoist { id, .. } = e {
        hoisted.push(*id);
      }
    });
    assert_eq!(hoisted, Vec::<u32>::new(), "wrap(money(n)) and toLocaleString sit in the chunk whose markup the browser adopts; the lambda's money(i) was never one: {component:?}");
    let (_, source) = set.rewritten().pop().unwrap();
    assert!(source.contains("=> { const __sfh = __sfUseHoisted(\"routes/index/page.tsx#Page\"); return (("), "an expression body becomes a block: {source}");
    assert!(source.trim_end().ends_with(")); };"), "{source}");
    assert!(source.contains("__sfh.c(4, (__sfHtml) => <p title={items.map((i) => money(i)).join(\", \")} dangerouslySetInnerHTML={__sfHtml} />)"), "the whole paragraph is static, so it is a chunk whose hit keeps the attribute: {source}");
    assert!(!source.contains("__sfh.r("), "the reads inside the chunk go with its fallback: {source}");
  }

  #[test]
  fn jsx_lowers_to_elements_attributes_and_the_three_idioms() {
    let page = r#"
import type { Props } from "../../generated/client";
export default function Page({ items, q = "" }: Props) {
  const n = items.length;
  return (
    <main className="page" aria-label={`Results for ${q}`}>
      <h1>{n} result{n === 1 ? "" : "s"}</h1>
      {n === 0 ? <p>Nothing</p> : null}
      {q && <b>{q}</b>}
      <ul>
        {items.map((it) => (
          <li key={String(it.id)} onClick={() => go(it)}>
            {it.name}
          </li>
        ))}
      </ul>
    </main>
  );
}
"#;
    let lowered = lower(&[("routes/index/page.tsx", page)], "routes/index/page.tsx#default").unwrap();
    assert_eq!(lowered.len(), 1);
    let component = &lowered[0].1;
    assert_eq!(component.body, vec![Stmt::Let { name: "n".to_owned(), expr: Expr::Length(Box::new(Expr::var("$props").field("items"))) }]);
    let Tmpl::Element { tag, attrs, children } = &component.render else { panic!("{:?}", component.render) };
    assert_eq!(tag, "main");
    assert_eq!(attrs[0], Entry::Field("class".to_owned(), Expr::lit_str("page")));
    assert!(matches!(&attrs[1], Entry::Field(name, Expr::Template(_)) if name == "aria-label"));
    assert!(matches!(&children[1], Tmpl::If { r#else: Some(e), .. } if matches!(**e, Tmpl::Fragment(ref f) if f.is_empty())));
    assert!(matches!(&children[2], Tmpl::If { r#else: Some(e), .. } if matches!(**e, Tmpl::Expr(_))), "`c && <x />` renders `c` when it is falsy");
    let Tmpl::Element { children: ul, .. } = &children[3] else { panic!() };
    let Tmpl::For { params, body, .. } = &ul[0] else { panic!("{:?}", ul[0]) };
    assert_eq!(params, &["it".to_owned()]);
    let Tmpl::Element { attrs, children, .. } = &**body else { panic!() };
    assert!(plain(attrs).is_empty(), "key and handlers are dropped: {attrs:?}");
    assert!(attrs.contains(&Entry::Field(hoist::BOUND_ATTR.to_owned(), Expr::Lit(Lit::Bool(true)))), "a handler leaves its mark: {attrs:?}");
    assert_eq!(children, &vec![Tmpl::Expr(Expr::var("it").field("name"))]);
  }

  #[test]
  fn helpers_components_and_state_resolve_across_local_modules() {
    let files = [
      (
        "routes/product/page.tsx",
        r#"
import { useState } from "react";
import { money } from "../../src/ui/money";
import { Stars } from "../../src/ui/Stars";
import { actions } from "../../generated/client";
export default function Product({ product }: { product: { price: number; rating: number } }) {
  const [quantity, setQuantity] = useState(1);
  async function add() { await actions.cart.add({ quantity }); }
  return (
    <div style={{ background: product.color, marginTop: 4 }}>
      <Stars rating={product.rating} />
      <span>{money(product.price)}</span>
      <select value={quantity} onChange={(e) => setQuantity(Number(e.target.value))} />
      <button onClick={() => void add()} disabled={product.price === 0}>Add</button>
    </div>
  );
}
"#,
      ),
      ("src/ui/money.ts", "export function money(cents: number): string {\n  if (cents === 0) return \"free\";\n  const dollars = cents / 100;\n  return `$${dollars.toFixed(2)}`;\n}\n"),
      ("src/ui/Stars.tsx", "import Swal from \"sweetalert2\";\nexport function Stars({ rating }: { rating: number }) {\n  const full = Math.round(rating);\n  return <span>{\"★\".repeat(full)}</span>;\n}\nexport function toast() { Swal.fire(); }\n"),
    ];
    let lowered = lower(&files, "routes/product/page.tsx#default").unwrap();
    let modules: Vec<&str> = lowered.iter().map(|(m, _)| m.as_str()).collect();
    assert_eq!(modules, ["src/ui/Stars.tsx#Stars", "routes/product/page.tsx#default"], "the referenced component lands first");
    let page = &lowered[1].1;
    assert_eq!(page.body, vec![Stmt::Let { name: "quantity".to_owned(), expr: Expr::Lit(Lit::Float(1.0)) }], "useState(x) reads as x");
    let Tmpl::Element { attrs, children, .. } = &page.render else { panic!() };
    assert!(matches!(&attrs[0], Entry::Field(name, Expr::Object(entries)) if name == "style" && entries.len() == 2));
    assert!(matches!(&children[0], Tmpl::Component { module, props, .. } if module == "src/ui/Stars.tsx#Stars" && props.len() == 1));
    let Tmpl::Element { attrs: span_attrs, children: span, .. } = &children[1] else { panic!() };
    assert!(span_attrs.iter().any(|a| matches!(a, Entry::Field(name, _) if name == hoist::CHUNK_ATTR)), "the span reads props only, so the server renders it: {span_attrs:?}");
    let Tmpl::Expr(expr) = &span[0] else { panic!("{:?}", span[0]) };
    assert!(matches!(expr, Expr::Apply { f, args } if matches!(**f, Expr::Lambda { .. }) && args.len() == 1), "the helper is inlined and, carried by the chunk's markup, no longer a hoist: {expr:?}");
    let Tmpl::Element { attrs, .. } = &children[3] else { panic!() };
    assert_eq!(plain(attrs).len(), 1, "onClick is dropped, disabled stays: {attrs:?}");
  }

  #[test]
  fn children_spreads_and_hooks() {
    let files = [
      (
        "routes/index/page.tsx",
        r#"
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { Page } from "../../src/ui/Page";
export default function Catalog({ products, cartCount, q }: { products: { name: string }[]; cartCount: number; q: string }) {
  const [open, setOpen] = useState(false);
  const header = { cartCount, q };
  const count = useMemo(() => products.length, [products]);
  const box = useRef(null);
  const toggle = useCallback(() => setOpen(!open), [open]);
  const focus = () => box.current?.focus();
  useEffect(() => { document.title = q; }, [q]);
  return (
    <Page {...header} className="catalog">
      <h1 {...(open ? { hidden: true } : {})} className="title" ref={box}>{count}</h1>
      {products.map((p) => <p key={p.name}>{p.name}</p>)}
    </Page>
  );
}
"#,
      ),
      (
        "src/ui/Page.tsx",
        r#"
import { Header } from "./Header";
export function Page({ className, children, ...rest }: { className: string; children: React.ReactNode; cartCount: number; q: string }) {
  return <><Header {...rest} /><main className={`page ${className}`}>{children}</main></>;
}
"#,
      ),
      ("src/ui/Header.tsx", "export function Header(props: { cartCount: number; q: string }) {\n  return <header>{props.q}{props.children}</header>;\n}\n"),
    ];
    let lowered = lower(&files, "routes/index/page.tsx#default").unwrap();
    let layout = &lowered.iter().find(|(m, _)| m == "src/ui/Page.tsx#Page").unwrap().1;
    let Tmpl::Fragment(parts) = &layout.render else { panic!() };
    let rest = Entry::Spread(Expr::Builtin { name: Builtin::Omit, args: vec![Expr::var("$props"), Expr::lit_str("className"), Expr::lit_str("children")] });
    assert!(matches!(&parts[0], Tmpl::Component { props, .. } if props[0] == rest), "the rest is the props without what was named: {:?}", parts[0]);
    let mut files = files;
    files[1].1 = r#"
import { Header } from "./Header";
export function Page(props: { className: string; children: React.ReactNode; cartCount: number; q: string }) {
  return <><Header cartCount={props.cartCount} q={props.q} /><main className={`page ${props.className}`}>{props.children}</main></>;
}
"#;
    let lowered = lower(&files, "routes/index/page.tsx#default").unwrap();
    let page = &lowered.iter().find(|(m, _)| m == "routes/index/page.tsx#default").unwrap().1;
    let names: Vec<&str> = page.body.iter().map(|s| match s { Stmt::Let { name, .. } => name.as_str(), _ => "" }).collect();
    assert_eq!(names, ["open", "header", "count", "box"], "useCallback and the arrow are handlers, useEffect is dropped");
    assert_eq!(page.body[2], Stmt::Let { name: "count".to_owned(), expr: Expr::Length(Box::new(Expr::var("$props").field("products"))) });
    assert_eq!(page.body[3], Stmt::Let { name: "box".to_owned(), expr: Expr::Object(vec![Entry::Field("current".to_owned(), Expr::Lit(Lit::Null))]) });
    let Tmpl::Component { module, props, children, .. } = &page.render else { panic!("{:?}", page.render) };
    assert_eq!(module, "src/ui/Page.tsx#Page");
    assert!(matches!(&props[0], Entry::Spread(Expr::Var(v)) if v == "header"));
    assert_eq!(props[1], Entry::Field("className".to_owned(), Expr::lit_str("catalog")));
    let Tmpl::Element { attrs, children: h1, .. } = &children[0] else { panic!("{:?}", children[0]) };
    assert!(matches!(&attrs[0], Entry::Spread(Expr::Ternary(..))));
    assert_eq!(attrs[1], Entry::Field("class".to_owned(), Expr::lit_str("title")));
    assert_eq!(h1, &vec![Tmpl::Expr(Expr::var("count"))]);
    assert!(matches!(&children[1], Tmpl::For { .. }));
    let layout = &lowered.iter().find(|(m, _)| m == "src/ui/Page.tsx#Page").unwrap().1;
    let Tmpl::Fragment(parts) = &layout.render else { panic!() };
    let Tmpl::Element { children: main, .. } = &parts[1] else { panic!() };
    assert_eq!(main, &vec![Tmpl::Slot("content".to_owned())]);
    let header = &lowered.iter().find(|(m, _)| m == "src/ui/Header.tsx#Header").unwrap().1;
    let Tmpl::Element { children, .. } = &header.render else { panic!() };
    assert_eq!(children[1], Tmpl::Slot("content".to_owned()), "`props.children` is the slot when props are bound whole");
  }

  #[test]
  fn an_app_s_own_use_id_is_its_hook_not_react_s() {
    let hook = "export function useId(prefix: string) {\n  return `${prefix}-field`;\n}\n";
    let page = "import { useId } from \"../../src/useId\";\nexport default function Page() {\n  const id = useId(\"name\");\n  return <label htmlFor={id}>Name</label>;\n}\n";
    let lowered = lower(&[("src/useId.ts", hook), ("routes/index/page.tsx", page)], "routes/index/page.tsx#default").unwrap();
    assert!(lowered.iter().any(|(m, _)| m == "routes/index/page.tsx#default"));
  }

  #[test]
  fn a_module_level_iife_is_named_with_a_hint() {
    let page = "const env = (() => {\n  return \"prod\";\n})();\nexport default function Page() {\n  return <p>{env}</p>;\n}\n";
    let err = lower(&[("routes/index/page.tsx", page)], "routes/index/page.tsx#default").unwrap_err();
    let LowerError::Residue(residue) = err else { panic!("{err}") };
    assert_eq!(residue.message, "a function called where it is written");
    assert!(residue.hint.as_deref().is_some_and(|h| h.contains("named function")), "{residue:?}");
  }

  #[test]
  fn a_default_export_of_a_package_component_is_residue_naming_the_package() {
    let layout = "import { BaseLayout } from \"shell-lib/layouts\";\nexport default BaseLayout;\n";
    let err = lower(&[("routes/layout.tsx", layout)], "routes/layout.tsx#default").unwrap_err();
    let LowerError::Residue(residue) = err else { panic!("{err}") };
    assert_eq!(residue.message, "`BaseLayout` comes from `shell-lib/layouts`, which the build cannot follow");
  }

  #[test]
  fn residue_names_the_line_and_the_construct() {
    let page = "export default function Page() {\n  const params = new WeakMap();\n  return <a href={String(params)}>x</a>;\n}\n";
    let err = lower(&[("routes/index/page.tsx", page)], "routes/index/page.tsx#default").unwrap_err();
    assert_eq!(err.to_string(), "routes/index/page.tsx:2:18: `new WeakMap`");
    let page = "import { Chart } from \"chart-lib\";\nexport default function Page() {\n  return <Chart />;\n}\n";
    let err = lower(&[("routes/index/page.tsx", page)], "routes/index/page.tsx#default").unwrap_err();
    assert!(err.to_string().contains("`Chart` comes from `chart-lib`, which the build cannot follow"), "{err}");
    let page = "import { lazy } from \"react\";\nconst Chart = lazy(() => import(\"chart-lib\"));\nexport default function Page() {\n  return <Chart />;\n}\n";
    let err = lower(&[("routes/index/page.tsx", page)], "routes/index/page.tsx#default").unwrap_err();
    assert!(err.to_string().contains("`Chart` comes from `chart-lib`, which the build cannot follow"), "a lazy import names its specifier: {err}");
  }

  #[test]
  fn jsx_text_follows_the_whitespace_rule_and_decodes_entities() {
    assert_eq!(jsx_text("\n    Hello\n    world\n  "), "Hello world");
    assert_eq!(jsx_text("a  b"), "a  b");
    assert_eq!(jsx_text("&minus; &amp; &#8230; &#x2192; &bogus;"), "\u{2212} & \u{2026} \u{2192} &bogus;");
  }
  #[test]
  fn a_rest_in_a_destructuring_is_the_object_without_the_named_keys() {
    let page = r#"
export default function Page({ title, kind = "note", ...rest }: { title: string; kind?: string; id: number; hidden: boolean }) {
  const { id, ...attrs } = rest;
  return <section data-id={id} {...attrs}>{title}: {kind}</section>;
}
"#;
    let lowered = lower(&[("routes/index/page.tsx", page)], "routes/index/page.tsx#default").unwrap();
    let component = &lowered[0].1;
    let props = Expr::var("$props");
    assert_eq!(component.body, vec![Stmt::Let { name: "$d1".to_owned(), expr: Expr::Builtin { name: Builtin::Omit, args: vec![props.clone(), Expr::lit_str("title"), Expr::lit_str("kind")] } }]);
    let Tmpl::Element { attrs, .. } = &component.render else { panic!("{:?}", component.render) };
    assert_eq!(attrs[0], Entry::Field("data-id".to_owned(), Expr::var("$d1").field("id")));
    assert_eq!(attrs[1], Entry::Spread(Expr::Builtin { name: Builtin::Omit, args: vec![Expr::var("$d1"), Expr::lit_str("id")] }));
  }

  #[test]
  fn a_member_expression_tag_names_an_export_of_a_namespace_import() {
    let files = [
      ("routes/index/page.tsx", "import * as Ui from \"../../src/ui\";\nimport * as React from \"react\";\nexport default function Page() {\n  return <React.Fragment><Ui.Card title=\"a\" /></React.Fragment>;\n}\n"),
      ("src/ui/index.tsx", "export function Card({ title }: { title: string }) {\n  return <div className=\"card\">{title}</div>;\n}\n"),
    ];
    let lowered = lower(&files, "routes/index/page.tsx#default").unwrap();
    let modules: Vec<&str> = lowered.iter().map(|(m, _)| m.as_str()).collect();
    assert_eq!(modules, ["src/ui/index.tsx#Card", "routes/index/page.tsx#default"]);
    let Tmpl::Fragment(children) = &lowered[1].1.render else { panic!("{:?}", lowered[1].1.render) };
    assert!(matches!(&children[0], Tmpl::Component { module, .. } if module == "src/ui/index.tsx#Card"));

    let page = "import { ui } from \"../../src/ui\";\nexport default function Page() {\n  return <ui.Card title=\"a\" />;\n}\n";
    let err = lower(&[("routes/index/page.tsx", page), ("src/ui/index.tsx", "export const ui = {};\n")], "routes/index/page.tsx#default").unwrap_err();
    assert!(err.to_string().contains("`ui` is not a namespace import"), "{err}");
  }

  #[test]
  fn the_island_wrapper_and_the_island_alias_place_a_component_as_an_island() {
    let files = [
      (
        "routes/order/page.tsx",
        r#"
import { Island, island } from "@snapfire/fsr/client/react";
import { Help } from "../../src/ui/Help";
import { Chart } from "../../src/ui/Chart";
const LazyChart = island(Chart, { when: "idle" });
export default function Order({ id }: { id: number }) {
  return (
    <main>
      <Island when="visible">
        <Help id={id} />
      </Island>
      <LazyChart series={[id]} />
      <Island><Help id={0} /></Island>
    </main>
  );
}
"#,
      ),
      ("src/ui/Help.tsx", "export function Help({ id }: { id: number }) {\n  return <p>help {id}</p>;\n}\n"),
      ("src/ui/Chart.tsx", "export function Chart({ series }: { series: number[] }) {\n  return <svg>{series.length}</svg>;\n}\n"),
    ];
    let lowered = lower(&files, "routes/order/page.tsx#default").unwrap();
    let page = &lowered.iter().find(|(m, _)| m == "routes/order/page.tsx#default").unwrap().1;
    let Tmpl::Element { children, .. } = &page.render else { panic!("{:?}", page.render) };
    assert!(matches!(&children[0], Tmpl::Island { module, props, when, .. } if module == "src/ui/Help.tsx#Help" && props.len() == 1 && when.as_deref() == Some("visible")), "{:?}", children[0]);
    assert!(matches!(&children[1], Tmpl::Island { module, props, when, .. } if module == "src/ui/Chart.tsx#Chart" && matches!(&props[0], Entry::Field(name, _) if name == "series") && when.as_deref() == Some("idle")), "{:?}", children[1]);
    assert!(matches!(&children[2], Tmpl::Island { when: None, .. }), "no timing means the registry's: {:?}", children[2]);
    assert!(lowered.iter().any(|(m, _)| m == "src/ui/Help.tsx#Help") && lowered.iter().any(|(m, _)| m == "src/ui/Chart.tsx#Chart"), "an island's component is lowered like any other");
  }

  #[test]
  fn a_layout_places_a_named_slot_by_element_and_by_prop() {
    let files = [
      (
        "routes/layout.tsx",
        "import { Slot } from \"@snapfire/fsr/client/react\";\nexport default function Layout({ children, feed, title }: { children: unknown; feed: unknown; title: string }) {\n  return <div><h1>{title}</h1>{children}<aside>{feed}</aside><Slot name=\"modal\" /></div>;\n}\n",
      ),
    ];
    let mut set = ComponentSet::new(&app(&files));
    set.layouts.push("routes/layout.tsx#default".to_owned());
    set.slots.push(("routes/layout.tsx#default".to_owned(), vec!["feed".to_owned()]));
    set.lower("routes/layout.tsx#default").unwrap();
    let layout = &set.components[0].1;
    let Tmpl::Element { children, .. } = &layout.render else { panic!("{:?}", layout.render) };
    assert_eq!(children[1], Tmpl::Element { tag: "sf-s".to_owned(), attrs: Vec::new(), children: vec![Tmpl::Slot("content".to_owned())] });
    let Tmpl::Element { children: aside, .. } = &children[2] else { panic!("{:?}", children[2]) };
    assert_eq!(aside[0], Tmpl::Element { tag: "sf-s".to_owned(), attrs: vec![Entry::Field("data-sf-name".to_owned(), Expr::lit_str("feed"))], children: vec![Tmpl::Slot("feed".to_owned())] }, "a prop named after a slots/ directory is that slot");
    assert_eq!(children[3], Tmpl::Element { tag: "sf-s".to_owned(), attrs: vec![Entry::Field("data-sf-name".to_owned(), Expr::lit_str("modal"))], children: vec![Tmpl::Slot("modal".to_owned())] });
  }

  #[test]
  fn a_slot_outside_a_layout_is_residue() {
    let page = [("routes/a/page.tsx", "import { Slot } from \"@snapfire/fsr/client/react\";\nexport default function A() {\n  return <Slot name=\"x\" />;\n}\n")];
    let err = lower(&page, "routes/a/page.tsx#default").unwrap_err().to_string();
    assert!(err.contains("outside a layout"), "{err}");
  }

  #[test]
  fn a_slot_placement_carries_its_fallback_by_element_and_by_prop() {
    let files = [(
      "routes/layout.tsx",
      "import { Slot } from \"@snapfire/fsr/client/react\";\nexport default function L({ children, feed }: { children: unknown; feed: unknown }) {\n  return <div>{children}{feed ?? <p>no feed</p>}<Slot name=\"modal\"><p>closed</p></Slot></div>;\n}\n",
    )];
    let mut set = ComponentSet::new(&app(&files));
    set.layouts.push("routes/layout.tsx#default".to_owned());
    set.slots.push(("routes/layout.tsx#default".to_owned(), vec!["feed".to_owned()]));
    set.lower("routes/layout.tsx#default").unwrap();
    let Tmpl::Element { children, .. } = &set.components[0].1.render else { panic!() };
    for (child, name, text) in [(&children[1], "feed", "no feed"), (&children[2], "modal", "closed")] {
      let Tmpl::Element { tag, attrs, children: inner } = child else { panic!("{child:?}") };
      assert_eq!(tag, "sf-s");
      assert_eq!(attrs, &vec![Entry::Field("data-sf-name".to_owned(), Expr::lit_str(name))]);
      let Tmpl::If { cond, then, r#else } = &inner[0] else { panic!("{:?}", inner[0]) };
      assert!(matches!(cond, Expr::Builtin { name: Builtin::Includes, args } if args[1] == Expr::lit_str(name)), "{cond:?}");
      assert_eq!(**then, Tmpl::Slot(name.to_owned()));
      let Some(fallback) = r#else else { panic!() };
      let Tmpl::Fragment(items) = &**fallback else { panic!("{fallback:?}") };
      let Tmpl::Element { children: p, .. } = &items[0] else { panic!("{:?}", items[0]) };
      assert_eq!(p, &vec![Tmpl::Text(text.to_owned())]);
    }
  }

  #[test]
  fn a_store_read_lowers_to_the_key_with_its_initial_value() {
    let files = [
      (
        "routes/index/page.tsx",
        "import { useStore } from \"@snapfire/fsr/client/react\";\nimport { cartCount } from \"../../src/store\";\nexport default function P() {\n  const [items, setItems] = useStore(cartCount, 0);\n  const [name] = useStore(\"user/name\", \"guest\");\n  return <p onClick={() => setItems(items + 1)}>{name}{items}</p>;\n}\n",
      ),
      ("src/store.ts", "import { key } from \"@snapfire/fsr/client/store\";\nexport const cartCount = key<number>(\"cart/count\");\n"),
    ];
    let lowered = lower(&files, "routes/index/page.tsx#default").unwrap();
    let component = &lowered[0].1;
    assert_eq!(
      component.body,
      vec![
        Stmt::Let { name: "items".to_owned(), expr: Expr::Coalesce(Box::new(Expr::Store("cart/count".to_owned())), Box::new(Expr::Lit(Lit::Float(0.0)))) },
        Stmt::Let { name: "name".to_owned(), expr: Expr::Coalesce(Box::new(Expr::Store("user/name".to_owned())), Box::new(Expr::lit_str("guest"))) },
      ],
      "a key() through an import and a literal both lower to the key"
    );
    assert_eq!(component.stores, [("items".to_owned(), "cart/count".to_owned()), ("name".to_owned(), "user/name".to_owned())], "each binding is recorded with the key it reads");
    let Tmpl::Element { attrs, .. } = &component.render else { panic!("{:?}", component.render) };
    assert!(plain(attrs).is_empty(), "the setter is a handler: {attrs:?}");
  }

  #[test]
  fn a_store_key_the_build_cannot_read_is_residue() {
    let files = [(
      "routes/index/page.tsx",
      "import { useStore } from \"@snapfire/fsr/client/react\";\nexport default function P({ id }: { id: string }) {\n  const [n] = useStore(id, 0);\n  return <p>{n}</p>;\n}\n",
    )];
    let err = lower(&files, "routes/index/page.tsx#default").unwrap_err().to_string();
    assert!(err.contains("`useStore` key"), "{err}");
  }

  #[test]
  fn use_locale_lowers_to_the_locale_read() {
    let files = [(
      "routes/help/page.tsx",
      "import { useLocale } from \"@snapfire/fsr/client/react\";\nexport default function Help() {\n  const locale = useLocale();\n  return <p lang={locale}>{locale === \"fr_FR\" ? \"Bonjour\" : \"Hello\"}</p>;\n}\n",
    )];
    let lowered = lower(&files, "routes/help/page.tsx#default").unwrap();
    let component = &lowered[0].1;
    assert_eq!(component.body, vec![Stmt::Let { name: "locale".to_owned(), expr: Expr::Locale }]);
    let Tmpl::Element { attrs, .. } = &component.render else { panic!("{:?}", component.render) };
    assert_eq!(plain(attrs), [Entry::Field("lang".to_owned(), Expr::var("locale"))]);
  }

  #[test]
  fn a_link_lowers_to_an_anchor_the_navigator_reads() {
    let page = [("routes/a/page.tsx", "import { Link } from \"@snapfire/fsr/client/react\";\nexport default function A({ id }: { id: number }) {\n  return <p><Link href={`/photo/${id}`} className=\"x\" full>full</Link><Link href=\"/photo/1\" into=\"modal\" prefetch=\"none\">quick</Link></p>;\n}\n")];
    let lowered = lower(&page, "routes/a/page.tsx#default").unwrap();
    let Tmpl::Element { children, .. } = &lowered[0].1.render else { panic!() };
    let Tmpl::Element { tag, attrs, children: text } = &children[0] else { panic!("{:?}", children[0]) };
    assert_eq!(tag, "a");
    assert!(matches!(&attrs[0], Entry::Field(name, _) if name == "href"));
    assert_eq!(attrs[1], Entry::Field("class".to_owned(), Expr::lit_str("x")));
    assert_eq!(attrs[2], Entry::Field("data-sf-full".to_owned(), Expr::Lit(Lit::Bool(true))));
    assert_eq!(text, &vec![Tmpl::Text("full".to_owned())]);
    let Tmpl::Element { attrs, .. } = &children[1] else { panic!("{:?}", children[1]) };
    assert_eq!(attrs[1], Entry::Field("data-sf-into".to_owned(), Expr::lit_str("modal")));
    assert_eq!(attrs[2], Entry::Field("data-sf-prefetch".to_owned(), Expr::lit_str("none")));
  }

  #[test]
  fn a_link_writes_its_keep_out_since_a_false_attribute_is_dropped() {
    let page = [("routes/a/page.tsx", "import { Link } from \"@snapfire/fsr/client/react\";\nexport default function A({ on }: { on: boolean }) {\n  return <p><Link href=\"?at=1\" keep={false}>a</Link><Link href=\"?at=2\" keep>b</Link><Link href=\"?at=3\" keep={on}>c</Link></p>;\n}\n")];
    let lowered = lower(&page, "routes/a/page.tsx#default").unwrap();
    let Tmpl::Element { children, .. } = &lowered[0].1.render else { panic!() };
    let keep = |i: usize| match &children[i] {
      Tmpl::Element { attrs, .. } => attrs[1].clone(),
      other => panic!("{other:?}"),
    };
    assert_eq!(keep(0), Entry::Field("data-sf-keep".to_owned(), Expr::lit_str("false")));
    assert_eq!(keep(1), Entry::Field("data-sf-keep".to_owned(), Expr::lit_str("true")));
    assert!(matches!(keep(2), Entry::Field(name, Expr::Ternary(..)) if name == "data-sf-keep"), "a computed keep is spelled when it renders: {:?}", keep(2));
  }

  #[test]
  fn a_link_carries_the_rule_that_marks_it_current_and_the_mark_itself() {
    let page = [("routes/a/page.tsx", "import { Link } from \"@snapfire/fsr/client/react\";\nexport default function A() {\n  return <p><Link href=\"/billing\">a</Link><Link href=\"/billing\" match=\"prefix\">b</Link><Link href=\"/billing\" match=\"none\">c</Link><Link href=\"/billing\" aria-current=\"page\">d</Link></p>;\n}\n")];
    let lowered = lower(&page, "routes/a/page.tsx#default").unwrap();
    let Tmpl::Element { children, .. } = &lowered[0].1.render else { panic!() };
    let attrs = |i: usize| match &children[i] {
      Tmpl::Element { attrs, .. } => attrs.clone(),
      other => panic!("{other:?}"),
    };
    let exact = attrs(0);
    assert_eq!(exact[1], Entry::Field("data-sf-link".to_owned(), Expr::lit_str("exact")));
    let hit = Expr::Compare(CompareOp::Eq, Box::new(Expr::Path), Box::new(Expr::lit_str("/billing")));
    assert_eq!(exact[2], Entry::Field("aria-current".to_owned(), Expr::Ternary(Box::new(hit), Box::new(Expr::lit_str("page")), Box::new(Expr::Lit(Lit::Null)))));
    let prefix = attrs(1);
    assert_eq!(prefix[1], Entry::Field("data-sf-link".to_owned(), Expr::lit_str("prefix")));
    assert!(matches!(&prefix[2], Entry::Field(name, Expr::Ternary(cond, hit, _)) if name == "aria-current" && matches!(**cond, Expr::Logic(LogicOp::Or, ..)) && **hit == Expr::lit_str("true")), "{:?}", prefix[2]);
    assert_eq!(attrs(2).len(), 1, "`match=\"none\"` leaves the anchor alone: {:?}", attrs(2));
    let written = attrs(3);
    assert_eq!(written.len(), 2, "an `aria-current` the author wrote stands: {written:?}");
    assert_eq!(written[1], Entry::Field("aria-current".to_owned(), Expr::lit_str("page")));
  }

  #[test]
  fn a_link_marked_by_the_document_compares_the_documents_path_and_says_so() {
    let page = [("routes/a/page.tsx", "import { Link } from \"@snapfire/fsr/client/react\";\nexport default function A() {\n  return <p><Link href=\"/agents\" match=\"prefix\" current=\"document\">a</Link><Link href=\"/help\" current=\"url\">b</Link><Link href=\"/help\" current=\"page\">c</Link></p>;\n}\n")];
    let err = lower(&page, "routes/a/page.tsx#default").unwrap_err().to_string();
    assert!(err.contains("`current` is \"url\" or \"document\""), "{err}");
    let page = [("routes/a/page.tsx", "import { Link } from \"@snapfire/fsr/client/react\";\nexport default function A() {\n  return <p><Link href=\"/agents\" match=\"prefix\" current=\"document\">a</Link><Link href=\"/help\" current=\"url\">b</Link></p>;\n}\n")];
    let lowered = lower(&page, "routes/a/page.tsx#default").unwrap();
    let Tmpl::Element { children, .. } = &lowered[0].1.render else { panic!() };
    let Tmpl::Element { attrs, .. } = &children[0] else { panic!("{:?}", children[0]) };
    assert_eq!(attrs[1], Entry::Field("data-sf-current".to_owned(), Expr::lit_str("document")));
    assert_eq!(attrs[2], Entry::Field("data-sf-link".to_owned(), Expr::lit_str("prefix")));
    let Entry::Field(name, Expr::Ternary(cond, ..)) = &attrs[3] else { panic!("{:?}", attrs[3]) };
    assert_eq!(name, "aria-current");
    let mut by_document = false;
    cond.visit(&mut |e| by_document |= *e == Expr::Document);
    assert!(by_document, "{cond:?}");
    let Tmpl::Element { attrs, .. } = &children[1] else { panic!("{:?}", children[1]) };
    assert_eq!(attrs[1], Entry::Field("data-sf-link".to_owned(), Expr::lit_str("exact")), "`url` is the default and writes nothing of its own: {attrs:?}");
    let Entry::Field(_, Expr::Ternary(cond, ..)) = &attrs[2] else { panic!("{:?}", attrs[2]) };
    assert_eq!(**cond, Expr::Compare(CompareOp::Eq, Box::new(Expr::Path), Box::new(Expr::lit_str("/help"))));
  }

  #[test]
  fn a_link_with_a_computed_match_is_residue() {
    let page = [("routes/a/page.tsx", "import { Link } from \"@snapfire/fsr/client/react\";\nexport default function A({ on }: { on: boolean }) {\n  return <Link href=\"/x\" match={on ? \"prefix\" : \"none\"}>a</Link>;\n}\n")];
    let err = lower(&page, "routes/a/page.tsx#default").unwrap_err().to_string();
    assert!(err.contains("written out"), "{err}");
  }

  #[test]
  fn an_early_return_lowers_to_the_conditional_a_ternary_lowers_to() {
    let early = "export default function P({ live, steps }: { live: boolean; steps: number }) {\n  const count = steps + 1;\n  if (steps === 0) return null;\n  if (live) {\n    return <button>open</button>;\n  }\n  return <p>{count}</p>;\n}\n";
    let ternary = "export default function P({ live, steps }: { live: boolean; steps: number }) {\n  const count = steps + 1;\n  return steps === 0 ? null : live ? <button>open</button> : <p>{count}</p>;\n}\n";
    let early = lower(&[("routes/index/page.tsx", early)], "routes/index/page.tsx#default").unwrap();
    let ternary = lower(&[("routes/index/page.tsx", ternary)], "routes/index/page.tsx#default").unwrap();
    assert_eq!(early[0].1.body, ternary[0].1.body);
    assert_eq!(early[0].1.render, ternary[0].1.render);
    assert!(matches!(&early[0].1.render, Tmpl::If { then, r#else: Some(_), .. } if **then == Tmpl::Fragment(Vec::new())), "{:?}", early[0].1.render);
  }

  #[test]
  fn a_hook_after_an_early_return_is_residue_and_returns_in_branches_lower() {
    let after = "import { useState } from \"react\";\nexport default function P({ live }: { live: boolean }) {\n  if (live) return <b>live</b>;\n  const [n] = useState(0);\n  return <p>{n}</p>;\n}\n";
    let err = lower(&[("routes/index/page.tsx", after)], "routes/index/page.tsx#default").unwrap_err().to_string();
    assert!(err.starts_with("routes/index/page.tsx:4:9: `useState` after an early `return`"), "{err}");
    let otherwise = "export default function P({ live }: { live: boolean }) {\n  if (live) return <b>live</b>;\n  else return <p>still</p>;\n}\n";
    let lowered = lower(&[("routes/index/page.tsx", otherwise)], "routes/index/page.tsx#default").unwrap();
    assert!(matches!(&lowered[0].1.render, Tmpl::If { r#else: Some(_), .. }), "{:?}", lowered[0].1.render);
  }

  #[test]
  fn an_island_around_an_element_or_with_a_computed_timing_is_residue() {
    let element = [("routes/a/page.tsx", "import { Island } from \"@snapfire/fsr/client/react\";\nexport default function A() {\n  return <Island when=\"visible\"><p>x</p></Island>;\n}\n")];
    let err = lower(&element, "routes/a/page.tsx#default").unwrap_err().to_string();
    assert!(err.contains("an island must be a component"), "{err}");
    let timing = [("routes/b/page.tsx", "import { Island } from \"@snapfire/fsr/client/react\";\nimport { Help } from \"../../src/ui/Help\";\nexport default function B({ n }: { n: number }) {\n  return <Island when={n > 0 ? \"visible\" : \"load\"}><Help /></Island>;\n}\n"), ("src/ui/Help.tsx", "export function Help() {\n  return <p>help</p>;\n}\n")];
    let err = lower(&timing, "routes/b/page.tsx#default").unwrap_err().to_string();
    assert!(err.contains("written out"), "{err}");
  }

  #[test]
  fn a_provider_tag_binds_its_value_around_its_children_and_the_component_hydrates() {
    let files = [
      ("src/theme.ts", "import { createContext } from \"react\";\nexport const Theme = createContext(\"light\");\n"),
      (
        "routes/layout.tsx",
        "import { Theme } from \"../src/theme\";\nexport default function Layout({ children, mode }: { children: unknown; mode: string }) {\n  return <Theme.Provider value={mode}><div class=\"shell\">{children}</div></Theme.Provider>;\n}\n",
      ),
    ];
    let mut set = ComponentSet::new(&app(&files));
    set.layouts.push("routes/layout.tsx#default".to_owned());
    set.lower("routes/layout.tsx#default").unwrap();
    let layout = &set.components[0].1;
    let Tmpl::Let { name, expr, then } = &layout.render else { panic!("{:?}", layout.render) };
    assert_eq!(name, "$ctx:src/theme.ts#Theme", "the context is named where `createContext` declares it");
    assert_eq!(expr, &Expr::var("$props").field("mode"));
    let Tmpl::Fragment(items) = &**then else { panic!("{then:?}") };
    let Tmpl::Element { tag, children, .. } = &items[0] else { panic!("{:?}", items[0]) };
    assert_eq!(tag, "div");
    assert_eq!(children[0], Tmpl::Element { tag: "sf-s".to_owned(), attrs: Vec::new(), children: vec![Tmpl::Slot("content".to_owned())] });
    assert_eq!(layout.owner, Owner::React, "a provider only exists in the browser, so the layout is a root");
  }

  #[test]
  fn a_custom_hook_inlines_with_state_of_its_own_per_call_and_its_returned_setter_is_a_handler() {
    let files = [
      ("src/hooks.ts", "import { useState } from \"react\";\nexport function useCount(start: number) {\n  const [n, setN] = useState(start);\n  return { n, twice: n * 2, setN };\n}\n"),
      (
        "routes/a/page.tsx",
        "import { useCount } from \"../../src/hooks\";\nexport default function A() {\n  const one = useCount(1);\n  const { n, twice, setN } = useCount(5);\n  return <button onClick={() => setN(n + 1)}>{one.n} {n} {twice}</button>;\n}\n",
      ),
    ];
    let lowered = lower(&files, "routes/a/page.tsx#default").unwrap();
    let page = &lowered.iter().find(|(m, _)| m == "routes/a/page.tsx#default").unwrap().1;
    assert_eq!(page.state, vec!["useCount$1$n".to_owned(), "useCount$2$n".to_owned()], "each call holds its own state: {:?}", page.body);
    assert_eq!(page.handlers.len(), 1, "the returned setter is a handler the button's arrow calls: {:?}", page.render);
    let Tmpl::Element { attrs, .. } = &page.render else { panic!("{:?}", page.render) };
    assert!(attrs.iter().any(|a| matches!(a, Entry::Field(n, _) if n == "$on:click")), "{attrs:?}");
  }

  const NAME_HOOK: &str = "import { useState } from \"react\";\nexport function useName() {\n  const [name, setName] = useState(\"\");\n  const update = (next: string) => {\n    setName(next);\n  };\n  return [name, update];\n}\n";

  const PREFIX_HOOK: &str = "import { useState } from \"react\";\nexport function useName(prefix: string) {\n  const [name, setName] = useState(\"\");\n  const update = (next: string) => {\n    setName(prefix + next);\n  };\n  return [name, update];\n}\n";

  fn sets(body: &Body) -> Vec<(String, Expr)> {
    let Some(Stmt::Return(Expr::Object(fields))) = body.last() else { panic!("{body:?}") };
    fields.iter().map(|f| match f { Entry::Field(n, e) => (n.clone(), e.clone()), other => panic!("{other:?}") }).collect()
  }

  #[test]
  fn a_function_a_custom_hook_returns_calls_the_hook_s_own_setter_when_the_caller_binds_it_to_the_setter_s_name() {
    let files = [
      ("src/hooks.ts", NAME_HOOK),
      ("routes/a/page.tsx", "import { useName } from \"../../src/hooks\";\nexport default function A() {\n  const [name, setName] = useName();\n  return <button onClick={() => setName(\"b\")}>{name}</button>;\n}\n"),
    ];
    let lowered = lower(&files, "routes/a/page.tsx#default").unwrap();
    let page = &lowered.iter().find(|(m, _)| m == "routes/a/page.tsx#default").unwrap().1;
    assert_eq!(page.state, vec!["useName$1$name".to_owned()], "{:?}", page.body);
    assert_eq!(page.handlers.len(), 1, "{:?} {:?}", page.body, page.render);
    let written = sets(&page.handlers[0].body);
    assert_eq!(written.iter().map(|(n, _)| n.as_str()).collect::<Vec<_>>(), vec!["useName$1$name"], "{written:?}");
    assert!(format!("{:?}", written[0].1).contains("\"b\""), "{written:?}");
  }

  #[test]
  fn a_function_a_custom_hook_returns_passed_to_a_child_s_prop_calls_the_hook_s_own_setter() {
    let files = [
      ("src/hooks.ts", NAME_HOOK),
      ("src/Field.tsx", "export function Field({ onName }: { onName: (name: string) => void }) {\n  return <form onSubmit={(e) => {\n    e.preventDefault();\n    onName(\"b\");\n  }}><button>Set</button></form>;\n}\n"),
      ("routes/a/page.tsx", "import { useName } from \"../../src/hooks\";\nimport { Field } from \"../../src/Field\";\nexport default function A() {\n  const [name, setName] = useName();\n  return <main><p>{name}</p><Field onName={setName} /></main>;\n}\n"),
    ];
    let lowered = lower(&files, "routes/a/page.tsx#default").unwrap();
    let page = &lowered.iter().find(|(m, _)| m == "routes/a/page.tsx#default").unwrap().1;
    assert_eq!(page.state, vec!["useName$1$name".to_owned()], "{:?}", page.body);
    let written: Vec<_> = page.handlers.iter().flat_map(|h| sets(&h.body)).map(|(n, _)| n).collect();
    assert_eq!(written, vec!["useName$1$name".to_owned()], "{:?}", page.handlers);
  }

  #[test]
  fn a_function_a_custom_hook_returns_reads_the_hook_s_parameters_as_the_call_passed_them() {
    let files = [
      ("src/hooks.ts", PREFIX_HOOK),
      ("routes/a/page.tsx", "import { useName } from \"../../src/hooks\";\nexport default function A() {\n  const prefix = \"page\";\n  const [name, update] = useName(\"x-\");\n  return <button onClick={() => update(prefix)}>{name}</button>;\n}\n"),
    ];
    let lowered = lower(&files, "routes/a/page.tsx#default").unwrap();
    let page = &lowered.iter().find(|(m, _)| m == "routes/a/page.tsx#default").unwrap().1;
    assert_eq!(page.handlers.len(), 1, "{:?} {:?}", page.body, page.render);
    let written = sets(&page.handlers[0].body);
    assert_eq!(written.len(), 1, "{written:?}");
    assert_eq!(written[0].0, "useName$1$name");
    assert_eq!(written[0].1, Expr::Arith(ArithOp::Add, Box::new(Expr::lit_str("x-")), Box::new(Expr::Var("prefix".to_owned()))), "the hook's `prefix` is the call's \"x-\" and the argument is the page's own `prefix`");
    assert!(page.body.contains(&Stmt::Let { name: "prefix".to_owned(), expr: Expr::lit_str("page") }), "{:?}", page.body);
  }

  #[test]
  fn use_reducer_holds_its_initial_state_and_use_context_reads_the_nearest_provider() {
    let files = [
      ("src/theme.ts", "import { createContext } from \"react\";\nexport const Theme = createContext(\"light\");\n"),
      ("src/Label.tsx", "import { useContext } from \"react\";\nimport { Theme } from \"./theme\";\nexport function Label() {\n  const theme = useContext(Theme);\n  return <i>{theme}</i>;\n}\n"),
      (
        "routes/a/page.tsx",
        "import { useReducer } from \"react\";\nimport { Theme } from \"../../src/theme\";\nimport { Label } from \"../../src/Label\";\nexport default function A() {\n  const [n, dispatch] = useReducer((s: number, a: number) => s + a, 2);\n  return <Theme.Provider value=\"dark\"><b onClick={() => dispatch(1)}>{n}</b><Label /></Theme.Provider>;\n}\n",
      ),
    ];
    let lowered = lower(&files, "routes/a/page.tsx#default").unwrap();
    let page = &lowered.iter().find(|(m, _)| m == "routes/a/page.tsx#default").unwrap().1;
    assert_eq!(page.state, vec!["n".to_owned()]);
    assert_eq!(page.body, vec![Stmt::Let { name: "n".to_owned(), expr: Expr::Lit(Lit::Float(2.0)) }]);
    let label = &lowered.iter().find(|(m, _)| m == "src/Label.tsx#Label").unwrap().1;
    assert_eq!(label.body, vec![Stmt::Let { name: "theme".to_owned(), expr: Expr::Coalesce(Box::new(Expr::Context("src/theme.ts#Theme".to_owned())), Box::new(Expr::lit_str("light"))) }]);
  }

  #[test]
  fn use_and_use_context_read_a_context_wherever_an_expression_stands() {
    let files = [
      ("src/theme.ts", "import { createContext } from \"react\";\nexport const Theme = createContext(\"light\");\n"),
      (
        "src/Tag.tsx",
        "import * as React from \"react\";\nimport { use, useContext } from \"react\";\nimport { Theme } from \"./theme\";\nexport function Tag({ compact }: { compact: boolean }) {\n  const a = use(Theme);\n  const b = compact ? use(Theme) : \"plain\";\n  const c = React.use(Theme);\n  return <i className={useContext(Theme)} title={React.useContext(Theme)}>{a} {b} {c} {use(Theme)}</i>;\n}\n",
      ),
      ("routes/a/page.tsx", "import { Theme } from \"../../src/theme\";\nimport { Tag } from \"../../src/Tag\";\nexport default function A() {\n  return <Theme.Provider value=\"dark\"><Tag compact={true} /></Theme.Provider>;\n}\n"),
    ];
    let lowered = lower(&files, "routes/a/page.tsx#default").unwrap();
    let tag = &lowered.iter().find(|(m, _)| m == "src/Tag.tsx#Tag").unwrap().1;
    let read = Expr::Coalesce(Box::new(Expr::Context("src/theme.ts#Theme".to_owned())), Box::new(Expr::lit_str("light")));
    assert_eq!(tag.body[0], Stmt::Let { name: "a".to_owned(), expr: read.clone() });
    assert_eq!(tag.body[2], Stmt::Let { name: "c".to_owned(), expr: read });
    let reads = format!("{:?} {:?}", tag.body, tag.render).matches("Context(\"src/theme.ts#Theme\")").count();
    assert_eq!(reads, 6, "{:?} {:?}", tag.body, tag.render);
  }

  #[test]
  fn a_template_context_lowers_like_reacts_and_leaves_the_page_to_the_server() {
    let files = [
      ("src/reserved.ts", "import { createContext } from \"@snapfire/fsr/template\";\nexport const Reserved = createContext<string[]>([]);\n"),
      ("src/Row.tsx", "import { use } from \"@snapfire/fsr/template\";\nimport { Reserved } from \"./reserved\";\nexport function Row({ id }: { id: string }) {\n  return <li>{use(Reserved).includes(id) ? \"kept\" : \"free\"}</li>;\n}\n"),
      ("routes/a/page.tsx", "import { Reserved } from \"../../src/reserved\";\nimport { Row } from \"../../src/Row\";\nexport default function A({ reserved }: { reserved: string[] }) {\n  return <Reserved.Provider value={reserved}><ul><Row id=\"a\" /></ul></Reserved.Provider>;\n}\n"),
    ];
    let lowered = lower(&files, "routes/a/page.tsx#default").unwrap();
    let page = &lowered.iter().find(|(m, _)| m == "routes/a/page.tsx#default").unwrap().1;
    let Tmpl::Let { name, .. } = &page.render else { panic!("{:?}", page.render) };
    assert_eq!(name, "$ctx:src/reserved.ts#Reserved");
    assert_eq!(page.owner, Owner::Fsr, "the server resolves a template context, so nothing mounts the page");
    let row = &lowered.iter().find(|(m, _)| m == "src/Row.tsx#Row").unwrap().1;
    assert!(format!("{:?}", row.render).contains("Context(\"src/reserved.ts#Reserved\")"), "{:?}", row.render);
  }

  #[test]
  fn the_template_use_of_something_other_than_a_context_is_residue() {
    let files = [("routes/a/page.tsx", "import { use } from \"@snapfire/fsr/template\";\nexport default function A({ theme }: { theme: string }) {\n  return <p>{use(theme)}</p>;\n}\n")];
    let err = lower(&files, "routes/a/page.tsx#default").unwrap_err().to_string();
    assert!(err.contains("`use` of something other than a `createContext`"), "{err}");
  }

  #[test]
  fn use_of_something_other_than_a_context_stays_residue() {
    let files = [("routes/a/page.tsx", "import { use } from \"react\";\nexport default function A({ pending }: { pending: Promise<string> }) {\n  const name = use(pending);\n  return <p>{name}</p>;\n}\n")];
    let err = lower(&files, "routes/a/page.tsx#default").unwrap_err().to_string();
    assert!(err.contains("`use` of something other than a `createContext`"), "{err}");
    let inline = [("routes/a/page.tsx", "import { use } from \"react\";\nexport default function A({ pending }: { pending: Promise<string> }) {\n  return <p>{use(pending)}</p>;\n}\n")];
    let err = lower(&inline, "routes/a/page.tsx#default").unwrap_err().to_string();
    assert!(err.contains("`use` of something other than a `createContext`"), "{err}");
  }

  #[test]
  fn use_id_is_a_let_of_its_numbered_id() {
    let id = [("routes/a/page.tsx", "import { useId } from \"react\";\nexport default function A() {\n  const id = useId();\n  return <label htmlFor={id}>x</label>;\n}\n")];
    let lowered = lower(&id, "routes/a/page.tsx#default").unwrap();
    let (_, component) = lowered.iter().find(|(m, _)| m == "routes/a/page.tsx#default").unwrap();
    assert_eq!(component.body, vec![Stmt::Let { name: "id".to_owned(), expr: Expr::HoistId(0) }]);
  }

  #[test]
  fn a_hook_the_build_cannot_follow_stays_residue() {
    let unknown = [("routes/a/page.tsx", "import { useThing } from \"thing-lib\";\nexport default function A() {\n  const t = useThing();\n  return <p>{t}</p>;\n}\n")];
    let err = lower(&unknown, "routes/a/page.tsx#default").unwrap_err().to_string();
    assert!(err.contains("`useThing`, a hook the build cannot follow"), "{err}");
  }

  #[test]
  fn a_provider_whose_object_is_not_a_context_is_residue() {
    let files = [("routes/a/page.tsx", "const Theme = { Provider: (p: { children: unknown }) => p.children };\nexport default function A() {\n  return <Theme.Provider><p>x</p></Theme.Provider>;\n}\n")];
    let err = lower(&files, "routes/a/page.tsx#default").unwrap_err().to_string();
    assert!(err.contains("`Theme` is not a `createContext` value"), "{err}");
  }

  #[test]
  fn tree_is_refused_with_what_to_write_instead() {
    let layout = "import type { ReactNode } from \"react\";\nimport { tree } from \"@snapfire/fsr/client/react\";\nfunction Layout({ children }: { children: ReactNode }) {\n  return <main>{children}</main>;\n}\nexport default tree(Layout);\n";
    let mut set = ComponentSet::new(&app(&[("routes/layout.tsx", layout)]));
    set.layouts.push("routes/layout.tsx#default".to_owned());
    let err = set.lower("routes/layout.tsx#default").unwrap_err();
    assert!(matches!(err, LowerError::Retired(_)), "a hard error, not residue that sends the layout to the browser: {err}");
    assert!(err.to_string().contains("`tree(...)` is gone") && err.to_string().contains("export default"), "{err}");
  }



  #[test]
  fn a_module_that_does_not_lower_is_answered_from_memory_the_second_time() {
    let files = [
      ("routes/a/page.tsx", "import { Broken } from \"@src/Broken\";\nexport default function A() {\n  return <Broken />;\n}\n"),
      ("routes/b/page.tsx", "import { Broken } from \"@src/Broken\";\nexport default function B() {\n  return <Broken />;\n}\n"),
      ("src/Broken.tsx", "export function Broken() {\n  return <p>{;\n}\n"),
    ];
    let dir = app(&files);
    let mut set = ComponentSet::new(&dir);
    let first = set.lower("routes/a/page.tsx#default").unwrap_err();
    assert!(first.to_string().contains("src/Broken.tsx: 2:14"), "{first}");
    std::fs::write(dir.join("src/Broken.tsx"), "export function Broken() {\n  return <p>fixed</p>;\n}\n").unwrap();
    assert_eq!(set.lower("routes/a/page.tsx#default").unwrap_err(), first, "the page answers the same error without reading again");
    let other = set.lower("routes/b/page.tsx#default").unwrap_err();
    assert!(other.to_string().starts_with("routes/b/page.tsx:3:10: src/Broken.tsx: 2:14"), "a second page reaching the module gets the failure it gave the first: {other}");
    std::fs::remove_dir_all(&dir).unwrap();
  }
}

/// Where copying a constant into every body that reads it starts to cost more
/// than a table entry and an indirection.
const CONST_WEIGHT: usize = 32;

/// How many nodes an expression is.
fn weight(expr: &Expr) -> usize {
  let mut n = 0;
  expr.visit(&mut |_| n += 1);
  n
}

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

fn lower_derive(parsed: &Parsed, defaults: &SessionDefaults, resolved: &Resolved, call: &js::CallExpression, nested: bool) -> Result<LoweredDerive, DeriveFailure> {
  let mut lowerer = Lowerer::new(parsed, defaults).resolved(resolved);
  let refused = |lowerer: &mut Lowerer<'_>, residue: Residue, compute: Option<(String, Vec<String>)>| DeriveFailure { residue, unbound: lowerer.unbound.take(), compute };
  let ([k, s, c], false) = (&call.arguments[..], call.arguments.iter().any(|a| crate::syntax::is_spread(a))) else {
    return Err(refused(&mut lowerer, parsed.residue(call.span, "`derive` takes a key, its sources and a compute"), None));
  };
  let key = match lowerer.expr(crate::syntax::arg_expr(k)) {
    Ok(Expr::Lit(Lit::Str(key))) => key,
    Ok(_) => return Err(refused(&mut lowerer, parsed.residue(crate::syntax::arg_expr(k).span(), "a derived key the build cannot read; name it with `key()` or a string"), None)),
    Err(residue) => return Err(refused(&mut lowerer, residue, None)),
  };
  let js::Expression::ArrayExpression(list) = crate::syntax::arg_expr(s) else {
    return Err(refused(&mut lowerer, parsed.residue(crate::syntax::arg_expr(s).span(), "a derive's sources the build cannot read; list them in the call"), None));
  };
  let mut sources = Vec::new();
  for element in &list.elements {
    let Some(element) = element.as_expression() else {
      return Err(refused(&mut lowerer, parsed.residue(list.span, "a derive's sources the build cannot read; list them in the call"), None));
    };
    match lowerer.expr(element) {
      Ok(Expr::Lit(Lit::Str(source))) => sources.push(source),
      Ok(_) => return Err(refused(&mut lowerer, parsed.residue(element.span(), "a derive source the build cannot read; name it with `key()` or a string"), None)),
      Err(residue) => return Err(refused(&mut lowerer, residue, None)),
    }
  }
  let known = Some((key.clone(), sources.clone()));
  if nested {
    return Ok(LoweredDerive { key, sources, body: None, at: None, why: Some("written inside a function, so it runs only when the browser calls that function".to_owned()) });
  }
  let (params, stmts): (Vec<crate::Param>, FunctionBody<'_>) = match crate::syntax::arg_expr(c) {
    js::Expression::ArrowFunctionExpression(arrow) if !arrow.r#async => (crate::params_of(&arrow.params), arrow_body(arrow)),
    js::Expression::FunctionExpression(f) if !f.r#async && !f.generator && f.body.is_some() => (crate::params_of(&f.params), FunctionBody::Block(&f.body.as_ref().unwrap().statements)),
    other => return Err(refused(&mut lowerer, parsed.residue(other.span(), "a compute that is not a function written in the call"), known)),
  };
  lowerer.render_path = true;
  match &params[..] {
    [] => {}
    [param] if param.ident().is_some() => {
      if let crate::Ctxt::Local(read) = parsed.binding_ctxt(param.ident().expect("checked")) {
        lowerer.reader = Some(read);
      }
    }
    _ => return Err(refused(&mut lowerer, parsed.residue(crate::syntax::arg_expr(&c).span(), "a compute whose parameters are not one `read`"), known)),
  }
  let body = match stmts {
    FunctionBody::Expr(e) => lowerer.expr(e),
    FunctionBody::Block(b) => block_to_expr(&mut lowerer, b),
  };
  match body {
    Ok(body) => Ok(LoweredDerive { key, sources, body: Some(body), at: None, why: None }),
    Err(residue) => Err(refused(&mut lowerer, residue, known)),
  }
}

struct DeriveCalls<'m, 'b> {
  meanings: &'m crate::meaning::Meanings,
  build: crate::syntax::Build<'b>,
  calls: Vec<js::CallExpression<'b>>,
}

impl<'a> oxc_ast_visit::Visit<'a> for DeriveCalls<'_, '_> {
  fn visit_call_expression(&mut self, call: &js::CallExpression<'a>) {
    if self.meanings.of_expr(&call.callee) == Some(crate::meaning::Meaning::Library(crate::meaning::Construct::Derive)) {
      self.calls.push(self.build.copy(call));
    }
    oxc_ast_visit::walk::walk_call_expression(self, call);
  }
}

struct StoreCalls<'m, 'b> {
  meanings: &'m crate::meaning::Meanings,
  build: crate::syntax::Build<'b>,
  args: Vec<js::Expression<'b>>,
}

impl<'a> oxc_ast_visit::Visit<'a> for StoreCalls<'_, '_> {
  fn visit_call_expression(&mut self, call: &js::CallExpression<'a>) {
    if self.meanings.of_expr(&call.callee).and_then(|meant| meant.hook()) == Some("useStore") {
      if let Some(first) = call.arguments.first() {
        self.args.push(self.build.copy(crate::syntax::arg_expr(first)));
      }
    }
    oxc_ast_visit::walk::walk_call_expression(self, call);
  }
}

/// A layout's or a page's `export const theme`.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct ThemeExport {
  pub name: String,
  pub force: bool,
}

impl ThemeExport {
  pub fn new(name: String, force: bool) -> Self {
    Self { name, force }
  }
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

fn unwrap_literal<'e>(expr: &'e js::Expression<'e>) -> &'e js::Expression<'e> {
  match expr {
    js::Expression::ParenthesizedExpression(inner) => unwrap_literal(&inner.expression),
    js::Expression::TSAsExpression(inner) => unwrap_literal(&inner.expression),
    js::Expression::TSSatisfiesExpression(inner) => unwrap_literal(&inner.expression),
    other => other,
  }
}

/// Markup written where a value goes, moved into the tree. A local holding
/// markup is substituted where it renders, a placement's markup prop becomes
/// a `$slot:<prop>` fill among its children and every `{$props.name}` becomes
/// a `$slotRead:<name>` that takes such a fill before the value.
mod fills {
  use std::collections::{HashMap, HashSet};

  use snapfire_fsr_ir::ast::{Builtin, Component, Entry, Expr, Lit, LogicOp, Stmt};
  use snapfire_fsr_ir::render::{ARRAY_ITEMS, CONTEXT_PREFIX, SLOT_CONTENT_PREFIX, SLOT_READ_PREFIX};
  use snapfire_fsr_ir::Tmpl;

  /// The tag of an element whose tag a local chose, `<Heading>` over `const { Heading } = table[key]`; its `$tag` attribute holds the choice until [`place_tags`] writes one element per tag it can take. One whose tags it cannot list stays, and the renderer reads the choice.
  pub(super) use snapfire_fsr_ir::render::CHOSEN_TAG;

  /// Each element whose tag a local chose as a branch over the tags it can take, listed from the literals the choice reads through the body's `let`s and the set's named constants; `Err` when the list is not complete or holds something other than a lowercase tag name.
  pub(super) fn place_tags(component: &mut Component, consts: &snapfire_fsr_ir::ast::Consts) -> Result<(), &'static str> {
    let lets: HashMap<String, Expr> = component.body.iter().filter_map(|stmt| match stmt {
      Stmt::Let { name, expr } => Some((name.clone(), expr.clone())),
      _ => None,
    }).collect();
    let mut failed = None;
    choose_tags(&mut component.render, &lets, consts, &mut failed);
    match failed {
      Some(why) => Err(why),
      None => Ok(()),
    }
  }

  fn choose_tags(t: &mut Tmpl, lets: &HashMap<String, Expr>, consts: &snapfire_fsr_ir::ast::Consts, failed: &mut Option<&'static str>) {
    match t {
      Tmpl::Element { tag, attrs, children } if tag == CHOSEN_TAG => {
        children.iter_mut().for_each(|c| choose_tags(c, lets, consts, failed));
        let Some(Entry::Field(_, choice)) = attrs.first().cloned() else { return };
        let rest: Vec<Entry> = attrs[1..].to_vec();
        let mut tags = Vec::new();
        if !strings_of(&choice, lets, consts, &mut tags, 0) || tags.is_empty() {
          return;
        }
        if !tags.iter().all(|tag| tag.starts_with(|c: char| c.is_ascii_lowercase()) && tag.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')) {
          failed.get_or_insert("a tag chosen at render that is not a lowercase element name");
          return;
        }
        let children = std::mem::take(children);
        let element = |tag: &str| Tmpl::Element { tag: tag.to_owned(), attrs: rest.clone(), children: children.clone() };
        let (last, first) = tags.split_last().expect("not empty");
        let mut out = element(last);
        for tag in first.iter().rev() {
          out = Tmpl::If { cond: Expr::Compare(snapfire_fsr_ir::ast::CompareOp::Eq, Box::new(choice.clone()), Box::new(Expr::Lit(Lit::Str(tag.clone())))), then: Box::new(element(tag)), r#else: Some(Box::new(out)) };
        }
        *t = out;
      }
      Tmpl::Element { children, .. } | Tmpl::Fragment(children) | Tmpl::Place { children, .. } | Tmpl::Island { children, .. } | Tmpl::Baked { children, .. } | Tmpl::Component { children, .. } => children.iter_mut().for_each(|c| choose_tags(c, lets, consts, failed)),
      Tmpl::If { then, r#else, .. } => {
        choose_tags(then, lets, consts, failed);
        if let Some(e) = r#else {
          choose_tags(e, lets, consts, failed);
        }
      }
      Tmpl::For { body, .. } | Tmpl::Let { then: body, .. } => choose_tags(body, lets, consts, failed),
      Tmpl::Text(_) | Tmpl::Marker(_) | Tmpl::Outlet { .. } | Tmpl::Html(_) | Tmpl::Expr(_) | Tmpl::Slot(_) => {}
    }
  }

  /// Every string `e` can be, into `out`; false when one of its values is not a literal the build can see.
  fn strings_of(e: &Expr, lets: &HashMap<String, Expr>, consts: &snapfire_fsr_ir::ast::Consts, out: &mut Vec<String>, depth: usize) -> bool {
    if depth > 16 {
      return false;
    }
    match e {
      Expr::Lit(Lit::Str(s)) => {
        if !out.contains(s) {
          out.push(s.clone());
        }
        true
      }
      Expr::Var(n) => lets.get(n).is_some_and(|v| strings_of(v, lets, consts, out, depth + 1)),
      Expr::Const(key) => consts.get(key).is_some_and(|v| strings_of(v, lets, consts, out, depth + 1)),
      Expr::Ternary(_, a, b) => strings_of(a, lets, consts, out, depth + 1) && strings_of(b, lets, consts, out, depth + 1),
      Expr::Field(base, field) => {
        let mut objects = Vec::new();
        objects_of(base, lets, consts, &mut objects, depth + 1) && !objects.is_empty() && objects.iter().all(|entries| {
          entries.iter().find_map(|entry| match entry {
            Entry::Field(k, v) if k == field => Some(v),
            _ => None,
          }).is_some_and(|v| strings_of(v, lets, consts, out, depth + 1))
        })
      }
      _ => false,
    }
  }

  /// The object literals `e` can be: itself, what a name holds, an entry of one an index reads, either side of a branch.
  fn objects_of(e: &Expr, lets: &HashMap<String, Expr>, consts: &snapfire_fsr_ir::ast::Consts, out: &mut Vec<Vec<Entry>>, depth: usize) -> bool {
    if depth > 16 {
      return false;
    }
    match e {
      Expr::Object(entries) if entries.iter().all(|entry| matches!(entry, Entry::Field(..))) => {
        out.push(entries.clone());
        true
      }
      Expr::Var(n) => lets.get(n).is_some_and(|v| objects_of(v, lets, consts, out, depth + 1)),
      Expr::Const(key) => consts.get(key).is_some_and(|v| objects_of(v, lets, consts, out, depth + 1)),
      Expr::Ternary(_, a, b) => objects_of(a, lets, consts, out, depth + 1) && objects_of(b, lets, consts, out, depth + 1),
      Expr::Index(base, _) => {
        let mut tables = Vec::new();
        objects_of(base, lets, consts, &mut tables, depth + 1) && tables.iter().flatten().all(|entry| match entry {
          Entry::Field(_, v) => objects_of(v, lets, consts, out, depth + 1),
          _ => false,
        })
      }
      _ => false,
    }
  }

  /// Places every `Expr::Markup` the component holds where the tree can take it; the rest stay values, rendered where they are interpolated. `Err` says why one cannot be either.
  pub(super) fn place_markup(component: &mut Component) -> Result<(), &'static str> {
    if crate::heap_pass() {
      return Ok(());
    }
    let original = component.body.clone();
    let mut held: HashMap<String, Tmpl> = HashMap::new();
    let mut kept = Vec::new();
    for stmt in std::mem::take(&mut component.body) {
      match &stmt {
        Stmt::Let { name, expr } => match tree_of(expr, &held) {
          Some(t) => {
            held.insert(name.clone(), t);
          }
          None => kept.push(stmt),
        },
        _ => kept.push(stmt),
      }
    }
    component.body = kept;
    place(&mut component.render, &held);
    let mut place_inside = |e: &mut Expr| {
      if let Expr::Markup(t) = e {
        let t = std::sync::Arc::make_mut(t);
        place(t, &held);
        plain_reads(t);
      }
    };
    for stmt in &mut component.body {
      stmt_exprs_mut(stmt, &mut place_inside);
    }
    component.render.visit_mut(&mut place_inside);
    let mut restored: HashMap<String, Stmt> = HashMap::new();
    loop {
      let mut read = HashSet::new();
      let mut reads = |e: &Expr| {
        if let Expr::Var(n) = e {
          if held.contains_key(n) && !restored.contains_key(n) {
            read.insert(n.clone());
          }
        }
      };
      component.body.iter().chain(restored.values()).for_each(|stmt| stmt_exprs(stmt, &mut reads));
      component.render.visit(&mut reads);
      if read.is_empty() {
        break;
      }
      for stmt in &original {
        if let Stmt::Let { name, .. } = stmt {
          if read.contains(name) {
            let mut stmt = stmt.clone();
            stmt_exprs_mut(&mut stmt, &mut place_inside);
            restored.insert(name.clone(), stmt);
          }
        }
      }
    }
    if !restored.is_empty() {
      let mut kept = std::mem::take(&mut component.body).into_iter();
      for stmt in &original {
        match stmt {
          Stmt::Let { name, .. } if held.contains_key(name) => {
            if let Some(back) = restored.remove(name) {
              component.body.push(back);
            }
          }
          _ => component.body.extend(kept.next()),
        }
      }
      component.body.extend(kept);
    }
    let mut unready = false;
    let mut check = |e: &Expr| {
      if let Expr::Markup(t) = e {
        unready |= reads_slots(t);
      }
    };
    component.body.iter().for_each(|stmt| stmt_exprs(stmt, &mut check));
    component.render.visit(&mut check);
    match unready {
      true => Err("markup held as a value that places the component's children, which the build renders only in place"),
      false => Ok(()),
    }
  }

  fn stmt_exprs(stmt: &Stmt, f: &mut dyn FnMut(&Expr)) {
    if let Stmt::Let { expr, .. } | Stmt::Set { expr, .. } | Stmt::Return(expr) | Stmt::Expr(expr) = stmt {
      expr.visit(f);
    }
  }

  fn stmt_exprs_mut(stmt: &mut Stmt, f: &mut dyn FnMut(&mut Expr)) {
    if let Stmt::Let { expr, .. } | Stmt::Set { expr, .. } | Stmt::Return(expr) | Stmt::Expr(expr) = stmt {
      expr.visit_mut(f);
    }
  }

  /// `t` with each read of a prop a placement may fill made a plain read: a markup value reads the prop, which holds the markup a placement filled it with.
  fn plain_reads(t: &mut Tmpl) {
    if let Tmpl::Let { name, expr, .. } = t {
      if name.starts_with(SLOT_READ_PREFIX) {
        *t = Tmpl::Expr(expr.clone());
        return;
      }
    }
    match t {
      Tmpl::Element { children, .. } | Tmpl::Fragment(children) | Tmpl::Place { children, .. } | Tmpl::Island { children, .. } | Tmpl::Baked { children, .. } | Tmpl::Component { children, .. } => children.iter_mut().for_each(plain_reads),
      Tmpl::If { then, r#else, .. } => {
        plain_reads(then);
        if let Some(e) = r#else {
          plain_reads(e);
        }
      }
      Tmpl::For { body, .. } | Tmpl::Let { then: body, .. } => plain_reads(body),
      Tmpl::Text(_) | Tmpl::Marker(_) | Tmpl::Outlet { .. } | Tmpl::Html(_) | Tmpl::Expr(_) | Tmpl::Slot(_) => {}
    }
  }

  /// Whether `t` places a slot, which needs the frame of the placement around it.
  fn reads_slots(t: &Tmpl) -> bool {
    match t {
      Tmpl::Slot(_) => true,
      Tmpl::Text(_) | Tmpl::Marker(_) | Tmpl::Outlet { .. } | Tmpl::Html(_) | Tmpl::Expr(_) => false,
      Tmpl::Element { children, .. } | Tmpl::Fragment(children) | Tmpl::Place { children, .. } | Tmpl::Baked { children, .. } | Tmpl::Component { children, .. } | Tmpl::Island { children, .. } => children.iter().any(reads_slots),
      Tmpl::If { then, r#else, .. } => reads_slots(then) || r#else.as_deref().is_some_and(reads_slots),
      Tmpl::For { body, .. } => reads_slots(body),
      Tmpl::Let { then, .. } => reads_slots(then),
    }
  }

  /// The tree `expr` stands for when it is markup or chooses between markup.
  fn tree_of(expr: &Expr, held: &HashMap<String, Tmpl>) -> Option<Tmpl> {
    match expr {
      Expr::Markup(t) => Some((**t).clone()),
      Expr::Var(n) => held.get(n).cloned(),
      Expr::Ternary(c, a, b) => {
        let (ta, tb) = (tree_of(a, held), tree_of(b, held));
        if ta.is_none() && tb.is_none() {
          return None;
        }
        Some(Tmpl::If { cond: (**c).clone(), then: Box::new(ta.unwrap_or_else(|| value(a))), r#else: Some(Box::new(tb.unwrap_or_else(|| value(b)))) })
      }
      Expr::Logic(LogicOp::And, c, m) => tree_of(m, held).map(|t| Tmpl::If { cond: (**c).clone(), then: Box::new(t), r#else: Some(Box::new(Tmpl::Expr((**c).clone()))) }),
      Expr::Array(entries) => {
        let trees: Vec<Option<Tmpl>> = entries.iter().map(|entry| match entry {
          Entry::Item(e) | Entry::Spread(e) => tree_of(e, held),
          _ => None,
        }).collect();
        if trees.iter().all(Option::is_none) {
          return None;
        }
        let mut items = Vec::new();
        for (entry, tree) in entries.iter().zip(trees) {
          match (entry, tree) {
            (_, Some(t)) => items.push(t),
            (Entry::Item(e) | Entry::Spread(e), None) => items.push(value(e)),
            _ => return None,
          }
        }
        Some(self::items(items))
      }
      Expr::Filter(over, keep) if truthiness(keep) => match tree_of(over, held)? {
        Tmpl::Let { name, then, .. } if name == ARRAY_ITEMS && matches!(*then, Tmpl::Fragment(_)) => {
          let Tmpl::Fragment(kept) = *then else { unreachable!() };
          Some(self::items(kept.into_iter().map(|item| match item {
            Tmpl::Expr(e) => Tmpl::If { cond: e.clone(), then: Box::new(Tmpl::Expr(e)), r#else: None },
            other => other,
          }).collect()))
        }
        other => Some(other),
      },
      Expr::Reduce(over, init, step) => fold_tree(over, init, step, None, held),
      Expr::Field(base, name) => match &**base {
        Expr::Reduce(over, init, step) => fold_tree(over, init, step, Some(name), held),
        _ => None,
      },
      Expr::Builtin { name: Builtin::Concat, args } => {
        let trees: Vec<Option<Tmpl>> = args.iter().map(|e| tree_of(e, held)).collect();
        if trees.iter().all(Option::is_none) {
          return None;
        }
        Some(self::items(args.iter().zip(trees).map(|(e, tree)| tree.unwrap_or_else(|| value(e))).collect()))
      }
      _ => None,
    }
  }

  /// A fold whose step only appends markup to its accumulator, `field` of it when the loop wrote several names: what it starts from, then a `Tmpl::For` over the items writing what each one appends.
  fn fold_tree(over: &Expr, init: &Expr, step: &Expr, field: Option<&String>, held: &HashMap<String, Tmpl>) -> Option<Tmpl> {
    let Expr::Lambda { params, body } = step else { return None };
    let [acc, item] = params.as_slice() else { return None };
    let me = match field {
      None => Expr::Var(acc.clone()),
      Some(f) => Expr::Field(Box::new(Expr::Var(acc.clone())), f.clone()),
    };
    let field_of = |e: &Expr| -> Option<Expr> {
      match field {
        None => Some(e.clone()),
        Some(f) => match e {
          Expr::Object(entries) => entries.iter().find_map(|entry| match entry {
            Entry::Field(k, v) if k == f => Some(v.clone()),
            _ => None,
          }),
          _ => None,
        },
      }
    };
    let (init, body) = (field_of(init)?, field_of(body)?);
    let mut found = false;
    let each = appended(&body, &me, held, &mut found)?;
    let mut reads_acc = false;
    each.visit(&mut |e| {
      let mut free = Vec::new();
      e.free_vars(&mut free);
      reads_acc |= free.contains(acc);
    });
    if !found || reads_acc {
      return None;
    }
    let start = tree_of(&init, held).unwrap_or_else(|| value(&init));
    Some(self::items(vec![start, Tmpl::For { over: over.clone(), params: vec![item.clone()], body: Box::new(each) }]))
  }

  /// What one step of a fold appends to `me`: nothing when it is `me`, the items after a leading `...me`, a branch over either.
  fn appended(e: &Expr, me: &Expr, held: &HashMap<String, Tmpl>, found: &mut bool) -> Option<Tmpl> {
    if e == me {
      return Some(Tmpl::Fragment(Vec::new()));
    }
    match e {
      Expr::Array(entries) if entries.first() == Some(&Entry::Spread(me.clone())) => {
        let mut items = Vec::new();
        for entry in &entries[1..] {
          match entry {
            Entry::Item(x) | Entry::Spread(x) => items.push(match tree_of(x, held) {
              Some(t) => {
                *found = true;
                t
              }
              None => value(x),
            }),
            _ => return None,
          }
        }
        Some(self::items(items))
      }
      Expr::Ternary(c, a, b) => Some(Tmpl::If { cond: (**c).clone(), then: Box::new(appended(a, me, held, found)?), r#else: Some(Box::new(appended(b, me, held, found)?)) }),
      _ => None,
    }
  }

  /// An array's items as markup: they render in order and `Children.count` counts each of them, where `<>…</>` counts as one.
  pub(super) fn items(children: Vec<Tmpl>) -> Tmpl {
    Tmpl::Let { name: ARRAY_ITEMS.to_owned(), expr: Expr::Lit(Lit::Null), then: Box::new(Tmpl::Fragment(children)) }
  }

  /// `(x) => x` or `(x) => !!x`, which keeps an item when it is truthy.
  fn truthiness(keep: &Expr) -> bool {
    let Expr::Lambda { params, body } = keep else { return false };
    let [param] = params.as_slice() else { return false };
    match &**body {
      Expr::Var(v) => v == param,
      Expr::Not(inner) => matches!(&**inner, Expr::Not(v) if matches!(&**v, Expr::Var(v) if v == param)),
      _ => false,
    }
  }

  fn value(e: &Expr) -> Tmpl {
    match e {
      Expr::Lit(Lit::Null) => Tmpl::Fragment(Vec::new()),
      other => Tmpl::Expr(other.clone()),
    }
  }

  /// A `let` the renderer reads by its name: a slot's fill, a slot read, a
  /// Vue outlet and a context provider.
  fn reserved(name: &str) -> bool {
    name.starts_with("$slot") || name.starts_with(CONTEXT_PREFIX)
  }

  fn place(t: &mut Tmpl, held: &HashMap<String, Tmpl>) {
    match t {
      Tmpl::Expr(e) => {
        if let Some(sub) = tree_of(e, held) {
          *t = sub;
          place(t, held);
        } else if let Expr::Field(base, name) = e {
          if matches!(&**base, Expr::Var(v) if v == "$props") {
            *t = Tmpl::Let { name: format!("{SLOT_READ_PREFIX}{name}"), expr: e.clone(), then: Box::new(Tmpl::Fragment(Vec::new())) };
          }
        }
      }
      Tmpl::Component { props, children, .. } => {
        let mut moved = Vec::new();
        for entry in props.iter_mut() {
          if let Entry::Field(k, e) = entry {
            if let Some(sub) = tree_of(e, held) {
              moved.push((k.clone(), sub.clone()));
              *e = Expr::Markup(std::sync::Arc::new(sub));
            }
          }
        }
        for child in children.iter_mut() {
          place(child, held);
        }
        for (k, mut sub) in moved {
          place(&mut sub, held);
          children.push(Tmpl::Let { name: format!("{SLOT_CONTENT_PREFIX}{k}"), expr: Expr::Lit(Lit::Bool(true)), then: Box::new(sub) });
        }
      }
      Tmpl::Let { name, expr, then } if !reserved(name) && tree_of(expr, held).is_some() => {
        let sub = tree_of(expr, held).expect("checked by the guard");
        let mut inner = held.clone();
        inner.insert(name.clone(), sub);
        let mut body = std::mem::replace(&mut **then, Tmpl::Fragment(Vec::new()));
        place(&mut body, &inner);
        *t = body;
      }
      Tmpl::Element { children, .. } | Tmpl::Fragment(children) | Tmpl::Place { children, .. } | Tmpl::Island { children, .. } | Tmpl::Baked { children, .. } => children.iter_mut().for_each(|c| place(c, held)),
      Tmpl::If { then, r#else, .. } => {
        place(then, held);
        if let Some(e) = r#else {
          place(e, held);
        }
      }
      Tmpl::For { params, body, .. } if params.iter().any(|p| held.contains_key(p)) => place(body, &unheld(held, params.iter())),
      Tmpl::Let { name, then, .. } if held.contains_key(name.as_str()) => place(then, &unheld(held, std::iter::once(&*name))),
      Tmpl::For { body, .. } | Tmpl::Let { then: body, .. } => place(body, held),
      Tmpl::Text(_) | Tmpl::Marker(_) | Tmpl::Outlet { .. } | Tmpl::Html(_) | Tmpl::Slot(_) => {}
    }
  }

  /// `held` without the names a binding inside it shadows.
  fn unheld<'n>(held: &HashMap<String, Tmpl>, names: impl Iterator<Item = &'n String>) -> HashMap<String, Tmpl> {
    let mut inner = held.clone();
    for name in names {
      inner.remove(name);
    }
    inner
  }

  /// Every (module, prop) a placement in `t` passes markup for.
  pub(super) fn collect(t: &Tmpl, out: &mut HashSet<(String, String)>) {
    match t {
      Tmpl::Component { module, children, .. } => {
        for child in children {
          if let Tmpl::Let { name, .. } = child {
            if let Some(prop) = name.strip_prefix(SLOT_CONTENT_PREFIX) {
              out.insert((module.clone(), prop.to_owned()));
            }
          }
          collect(child, out);
        }
      }
      Tmpl::Element { children, .. } | Tmpl::Fragment(children) | Tmpl::Place { children, .. } | Tmpl::Island { children, .. } | Tmpl::Baked { children, .. } => children.iter().for_each(|c| collect(c, out)),
      Tmpl::If { then, r#else, .. } => {
        collect(then, out);
        if let Some(e) = r#else {
          collect(e, out);
        }
      }
      Tmpl::For { body, .. } | Tmpl::Let { then: body, .. } => collect(body, out),
      Tmpl::Text(_) | Tmpl::Marker(_) | Tmpl::Outlet { .. } | Tmpl::Html(_) | Tmpl::Expr(_) | Tmpl::Slot(_) => {}
    }
  }

  /// `t` with each read of a prop of `module` that nothing fills made plain.
  pub(super) fn settle(t: &mut Tmpl, module: &str, filled: &HashSet<(String, String)>) {
    if let Tmpl::Let { name, expr, .. } = t {
      if let Some(prop) = name.strip_prefix(SLOT_READ_PREFIX) {
        if !filled.contains(&(module.to_owned(), prop.to_owned())) {
          *t = Tmpl::Expr(expr.clone());
        }
        return;
      }
    }
    match t {
      Tmpl::Element { children, .. } | Tmpl::Fragment(children) | Tmpl::Place { children, .. } | Tmpl::Island { children, .. } | Tmpl::Baked { children, .. } | Tmpl::Component { children, .. } => children.iter_mut().for_each(|c| settle(c, module, filled)),
      Tmpl::If { then, r#else, .. } => {
        settle(then, module, filled);
        if let Some(e) = r#else {
          settle(e, module, filled);
        }
      }
      Tmpl::For { body, .. } | Tmpl::Let { then: body, .. } => settle(body, module, filled),
      Tmpl::Text(_) | Tmpl::Marker(_) | Tmpl::Outlet { .. } | Tmpl::Html(_) | Tmpl::Expr(_) | Tmpl::Slot(_) => {}
    }
  }

  /// Whether `e` can evaluate to a function value: a lambda anywhere in it other than the callback a builtin or a call applies.
  pub(super) fn holds_function(e: &Expr) -> bool {
    let mut callbacks: Vec<*const Expr> = Vec::new();
    e.visit(&mut |x| match x {
      Expr::Map(_, f) | Expr::Filter(_, f) | Expr::Find(_, f) | Expr::FindIndex(_, f) | Expr::Some(_, f) | Expr::Every(_, f) | Expr::Sort(_, f) | Expr::FlatMap(_, f) | Expr::Reduce(_, _, f) | Expr::ReplaceWith(_, _, f) | Expr::Apply { f, .. } | Expr::ApplyOf(f, _) => callbacks.push(&**f as *const Expr),
      _ => {}
    });
    let mut found = false;
    e.visit(&mut |x| found |= matches!(x, Expr::Lambda { .. } | Expr::Fix { .. }) && !callbacks.contains(&(x as *const Expr)));
    found
  }

  /// Gives every placement in `t` of a module `renders_in_browser` holds that passes a function value as a prop its `FUNCTIONS_PROP`, the module id the browser imports each such prop from, taken from `refs`; the indexes of the refs taken go to `used`. A function prop with no ref is the error, as is one whose ref says why the browser cannot reach it: the placement, the prop and the reason.
  pub(super) fn reference_functions(t: &mut Tmpl, renders_in_browser: &dyn Fn(&str) -> bool, refs: &[super::FnRef], used: &mut Vec<usize>) -> Result<(), (String, String, Option<String>)> {
    let island = matches!(t, Tmpl::Island { .. });
    match t {
      Tmpl::Component { module, props, children, .. } | Tmpl::Island { module, props, children, .. } => {
        if island || renders_in_browser(module) {
          let mut ids: Vec<Entry> = Vec::new();
          for entry in props.iter() {
            let Entry::Field(name, e) = entry else { continue };
            match refs.iter().position(|r| r.module == *module && r.prop == *name && r.value == *e) {
              Some(index) => match &refs[index].id {
                Ok(id) => {
                  ids.push(Entry::Field(name.clone(), id.clone()));
                  used.push(index);
                }
                Err(reason) => return Err((module.clone(), name.clone(), Some(reason.clone()))),
              },
              None if holds_function(e) => return Err((module.clone(), name.clone(), None)),
              None => {}
            }
          }
          if !ids.is_empty() {
            for why in used.iter().filter_map(|i| refs[*i].unlowered.as_deref()) {
              props.retain(|e| !matches!(e, Entry::Field(n, Expr::Lit(snapfire_fsr_ir::ast::Lit::Str(s))) if n == snapfire_fsr_ir::render::UNLOWERED_ATTR && s == why));
            }
            props.push(Entry::Field(snapfire_fsr_ir::render::FUNCTIONS_PROP.to_owned(), Expr::Object(ids)));
          }
        }
        children.iter_mut().try_for_each(|c| reference_functions(c, renders_in_browser, refs, used))
      }
      Tmpl::Element { children, .. } | Tmpl::Fragment(children) | Tmpl::Place { children, .. } | Tmpl::Baked { children, .. } => children.iter_mut().try_for_each(|c| reference_functions(c, renders_in_browser, refs, used)),
      Tmpl::If { then, r#else, .. } => {
        reference_functions(then, renders_in_browser, refs, used)?;
        match r#else.as_deref_mut() {
          Some(e) => reference_functions(e, renders_in_browser, refs, used),
          None => Ok(()),
        }
      }
      Tmpl::For { body, .. } | Tmpl::Let { then: body, .. } => reference_functions(body, renders_in_browser, refs, used),
      Tmpl::Text(_) | Tmpl::Marker(_) | Tmpl::Outlet { .. } | Tmpl::Html(_) | Tmpl::Expr(_) | Tmpl::Slot(_) => Ok(()),
    }
  }

  /// Each placement in `t` of a module `renders_in_browser` holds with the markup it fills a prop with given back as the prop, a markup value the island receives as a region of its own.
  pub(super) fn unfill(t: &mut Tmpl, renders_in_browser: &dyn Fn(&str) -> bool) {
    match t {
      Tmpl::Component { module, props, children, .. } => {
        if renders_in_browser(module) {
          let mut kept = Vec::with_capacity(children.len());
          for child in std::mem::take(children) {
            match child {
              Tmpl::Let { name, then, .. } if name.starts_with(SLOT_CONTENT_PREFIX) && name != format!("{SLOT_CONTENT_PREFIX}default") => {
                let prop = &name[SLOT_CONTENT_PREFIX.len()..];
                if !props.iter().any(|entry| matches!(entry, Entry::Field(k, _) if k == prop)) {
                  props.push(Entry::Field(prop.to_owned(), Expr::Markup(std::sync::Arc::new(*then))));
                }
              }
              other => kept.push(other),
            }
          }
          *children = kept;
        }
        children.iter_mut().for_each(|c| unfill(c, renders_in_browser));
      }
      Tmpl::Element { children, .. } | Tmpl::Fragment(children) | Tmpl::Place { children, .. } | Tmpl::Island { children, .. } | Tmpl::Baked { children, .. } => children.iter_mut().for_each(|c| unfill(c, renders_in_browser)),
      Tmpl::If { then, r#else, .. } => {
        unfill(then, renders_in_browser);
        if let Some(e) = r#else {
          unfill(e, renders_in_browser);
        }
      }
      Tmpl::For { body, .. } | Tmpl::Let { then: body, .. } => unfill(body, renders_in_browser),
      Tmpl::Text(_) | Tmpl::Marker(_) | Tmpl::Outlet { .. } | Tmpl::Html(_) | Tmpl::Expr(_) | Tmpl::Slot(_) => {}
    }
  }

}

/// `sub.stdin.write(text)` or `sub.stdin.end()`: the process's name, the method and the call.
fn stdin_of<'e>(e: &'e js::Expression<'e>) -> Option<(String, &'static str, &'e js::CallExpression<'e>)> {
  let js::Expression::CallExpression(call) = crate::meaning::bare(e) else { return None };
  let callee = &call.callee;
  let Some(method) = (crate::meaning::bare(callee)).as_member_expression() else { return None };
  let Some(stream) = (crate::meaning::bare(method.object())).as_member_expression() else { return None };
  let (js::Expression::Identifier(process), Some("stdin")) = (crate::meaning::bare(stream.object()), prop_name_of(stream).as_deref()) else { return None };
  let method = match prop_name_of(method).as_deref() {
    Some("write") => "write",
    Some("end") => "end",
    _ => return None,
  };
  Some((process.name.to_string(), method, call))
}
