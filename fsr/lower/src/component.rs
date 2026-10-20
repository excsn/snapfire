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

use std::borrow::Cow;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::rc::Rc;

use snapfire_compiler_wire::Described;
use snapfire_fsr_ir::ast::{ArithOp, Builtin, CompareOp, Component, Consts, Entry, Expr, Handler, Lit, LogicOp, Owner, Stmt, Tmpl};
use snapfire_fsr_ir::render::{html_attr_name, CONTEXT_PREFIX, HANDLER_ATTR, KEY_ATTR, RAW_ATTR, SERVER_MODE, TYPES_PROP, UNLOWERED_ATTR};
use snapfire_fsr_ir::Reach;
use swc_core::common::{Span, Spanned};
use swc_core::ecma::ast as js;
use swc_core::ecma::visit::{Visit, VisitWith};

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
#[non_exhaustive]
pub struct ComponentSet {
  app: PathBuf,
  parsed: HashMap<String, Rc<Parsed>>,
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
  /// Each arrow passed as a prop to a component that renders in the browser: its file, its byte range and the name the rewrite exports it under.
  fn_exports: Vec<(String, std::ops::Range<usize>, String)>,
  /// Each function a file declares without exporting that was passed as a prop to a component that renders in the browser: file and name, which the rewrite exports.
  fn_names: Vec<(String, String)>,
  /// Every file the browser imports a function prop from.
  fn_files: Vec<String>,
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
    Self { app: app.to_path_buf(), parsed: HashMap::new(), assets: Rc::new(NoAssets), rewrite_images: true, heads: HashMap::new(), provided: HashMap::new(), consts: Consts::new(), defaults: SessionDefaults::new(), components: Vec::new(), failed: HashMap::new(), resolving: Vec::new(), layouts: Vec::new(), slots: Vec::new(), rewrites: Vec::new(), pure: HashMap::new(), natives: Vec::new(), remaining: Vec::new(), foreign: Vec::new(), mounted: Vec::new(), described: HashMap::new(), meanings: HashMap::new(), meaning_in_progress: Vec::new(), foreign_residue: Vec::new(), browser_only: Vec::new(), undescribed: HashMap::new(), scripts_only: HashMap::new(), cyclic: false, stateless: HashMap::new(), moving_props: HashSet::new(), keys: HashMap::new(), elements: Rc::default(), transform: None, variants: HashMap::new(), placement_kinds: HashMap::new(), prop_kinds: HashMap::new(), relied: HashMap::new(), browser_texts: Vec::new(), lowering_component: false, module_lets: Vec::new(), fn_exports: Vec::new(), fn_names: Vec::new(), fn_files: Vec::new() }
  }

  /// Where an imported image or font is looked up, and whether a plain
  /// `<img>` of one is rewritten.
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
      let mut calls = StoreCalls { meanings: &meanings, args: Vec::new() };
      parsed.module.visit_with(&mut calls);
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
    let parsed = self.parsed[file].clone();
    let module_level: Vec<Span> = parsed
      .module
      .body
      .iter()
      .filter_map(|item| match item {
        js::ModuleItem::Stmt(js::Stmt::Expr(stmt)) => match &*stmt.expr {
          js::Expr::Call(call) => Some(call.span),
          _ => None,
        },
        _ => None,
      })
      .collect();
    let mut calls = DeriveCalls { meanings: &meanings, calls: Vec::new() };
    parsed.module.visit_with(&mut calls);
    let mut resolved = Resolved { globals: Vec::new(), global_kinds: Vec::new(), global_formatters: Vec::new(), natives: self.natives.clone() };
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
    if self.parsed.contains_key(file) {
      return Ok(());
    }
    if self.parsed.contains_key(file) {
      return Ok(());
    }
    let (source, line) = match &described.script {
      Some(script) => (script.content.clone(), script.line as usize),
      None => (String::new(), 1),
    };
    let padded = format!("{}{source}", "\n".repeat(line.saturating_sub(1)));
    let mut parsed = parse_with(file, &padded, false)?;
    if described.script.as_ref().is_some_and(|script| script.plain && !script.setup) && file.ends_with(".vue") {
      crate::vue::name_this(&mut parsed.module);
    }
    self.parsed.insert(file.to_owned(), Rc::new(parsed));
    Ok(())
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
    let parsed = self.parsed[file].clone();
    for item in &parsed.module.body {
      let js::ModuleItem::ModuleDecl(js::ModuleDecl::ExportDecl(export)) = item else { continue };
      let js::Decl::Var(var) = &export.decl else { continue };
      for decl in &var.decls {
        let (js::Pat::Ident(name), Some(init)) = (&decl.name, decl.init.as_deref()) else { continue };
        if name.id.sym.as_ref() != "theme" {
          continue;
        }
        let init = unwrap_literal(init);
        let refused = || LowerError::from(parsed.residue(init.span(), "`theme` is a theme's name or `{ name: \"<name>\", force: true }`, written as a literal"));
        let text = |expr: &js::Expr| match unwrap_literal(expr) {
          js::Expr::Lit(js::Lit::Str(s)) => Some(s.value.to_string_lossy().into_owned()),
          js::Expr::Tpl(tpl) if tpl.exprs.is_empty() && tpl.quasis.len() == 1 => tpl.quasis[0].cooked.as_ref().map(|c| c.to_string_lossy().into_owned()),
          _ => None,
        };
        if let Some(name) = text(init) {
          return Ok(Some(ThemeExport { name, force: false }));
        }
        let js::Expr::Object(object) = init else { return Err(refused()) };
        let mut out = ThemeExport { name: String::new(), force: false };
        for prop in &object.props {
          let js::PropOrSpread::Prop(prop) = prop else { return Err(refused()) };
          let js::Prop::KeyValue(kv) = &**prop else { return Err(refused()) };
          let key = match &kv.key {
            js::PropName::Ident(id) => id.sym.to_string(),
            js::PropName::Str(s) => s.value.to_string_lossy().into_owned(),
            _ => return Err(refused()),
          };
          match (key.as_str(), unwrap_literal(&kv.value)) {
            ("name", value) => out.name = text(value).ok_or_else(refused)?,
            ("force", js::Expr::Lit(js::Lit::Bool(b))) => out.force = b.value,
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
  fn resolving_loop<T>(&mut self, file: &str, lower: impl Fn(&Parsed, &SessionDefaults, &Resolved) -> Result<T, Unresolved>) -> Result<T, LowerError> {
    self.load(file)?;
    self.meanings(file);
    let mut resolved = Resolved { globals: Vec::new(), global_kinds: Vec::new(), global_formatters: Vec::new(), natives: self.natives.clone() };
    loop {
      let parsed = self.parsed[file].clone();
      let defaults = self.defaults.clone();
      match lower(&parsed, &defaults, &resolved) {
        Ok(done) => return Ok(done),
        Err((error, Some(name))) if !resolved.globals.iter().any(|(n, _)| *n == name) => match self.global(file, &name)? {
          Some((expr, key)) => {
            if let Some(kind) = self.global_kind(file, &name)? {
              resolved.global_kinds.push((name.clone(), kind));
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
    let parsed = self.parsed[file].clone();
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
    use swc_core::ecma::visit::{Visit, VisitWith};
    struct Found(bool);
    impl Visit for Found {
      fn visit_jsx_element(&mut self, _: &js::JSXElement) {
        self.0 = true;
      }
      fn visit_jsx_fragment(&mut self, _: &js::JSXFragment) {
        self.0 = true;
      }
    }
    if self.load(file).is_err() {
      return false;
    }
    let mut found = Found(false);
    self.parsed[file].module.visit_with(&mut found);
    found.0
  }

  /// Whether `module`'s file imports a value from `source`. A `file#export`
  /// module is asked about its file.
  pub fn imports_value_from(&self, module: &str, source: &str) -> bool {
    let file = module.split_once('#').map(|(file, _)| file).unwrap_or(module);
    let Some(parsed) = self.parsed.get(file) else { return false };
    parsed.module.body.iter().any(|item| match item {
      js::ModuleItem::ModuleDecl(js::ModuleDecl::Import(import)) => !import.type_only && import.src.value.to_atom_lossy().as_ref() == source && import.specifiers.iter().any(|spec| !matches!(spec, js::ImportSpecifier::Named(named) if named.is_type_only)),
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

  /// The local script files and described foreign components `file`
  /// imports a value from or re-exports, in source order: a described foreign
  /// component's from its script. None when it does not parse. An imported asset is not a
  /// script and is left out.
  pub fn local_imports(&mut self, file: &str) -> Vec<String> {
    let parsed = match self.described.get(file).and_then(|described| described.script.as_ref()).or_else(|| self.scripts_only.get(file)) {
      Some(script) => match parse_with(file, &script.content, false) {
        Ok(parsed) => Rc::new(parsed),
        Err(_) => return Vec::new(),
      },
      None => {
        if self.load(file).is_err() {
          return Vec::new();
        }
        self.parsed[file].clone()
      }
    };
    let sources: Vec<String> = value_sources(&parsed.module).map(|src| src.value.to_atom_lossy().to_string()).chain(dynamic_sources(&parsed.module)).collect();
    sources
      .iter()
      .filter_map(|src| self.resolve_import(file, src))
      .filter(|target| [".tsx", ".ts", ".jsx", ".js"].iter().any(|ext| target.ends_with(ext)) || is_transformed(target) || self.described.contains_key(target))
      .collect()
  }

  /// The value imports in `file` of components a plugin's language holds, a
  /// `.vue` file among them; none when it does not parse. A namespace import
  /// is left out, since it names no component.
  pub fn foreign_imports(&mut self, file: &str) -> Vec<ForeignImport> {
    if self.load(file).is_err() {
      return Vec::new();
    }
    let parsed = self.parsed[file].clone();
    let Some(source) = parsed.cm.files().first().map(|f| f.src.clone()) else { return Vec::new() };
    let mut out = Vec::new();
    for item in &parsed.module.body {
      let js::ModuleItem::ModuleDecl(js::ModuleDecl::Import(import)) = item else { continue };
      if import.type_only || import.specifiers.iter().any(|spec| matches!(spec, js::ImportSpecifier::Namespace(_))) {
        continue;
      }
      let Some(target) = self.resolve_import(file, import.src.value.to_atom_lossy().as_ref()) else { continue };
      let ext = Path::new(&target).extension().and_then(|e| e.to_str()).unwrap_or("").to_ascii_lowercase();
      if snapfire_compiler_wire::claimed(&ext).is_none() {
        continue;
      }
      let bindings = import
        .specifiers
        .iter()
        .filter_map(|spec| match spec {
          js::ImportSpecifier::Default(default) => Some((default.local.sym.to_string(), format!("{target}#default"))),
          js::ImportSpecifier::Named(named) if !named.is_type_only => {
            let imported = match &named.imported {
              Some(js::ModuleExportName::Ident(id)) => id.sym.to_string(),
              Some(js::ModuleExportName::Str(name)) => name.value.to_atom_lossy().to_string(),
              None => named.local.sym.to_string(),
            };
            Some((named.local.sym.to_string(), format!("{target}#{imported}")))
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
    let parsed = self.parsed[&file].clone();
    let export = module.split_once('#').map(|(_, export)| export).unwrap_or("default");
    if async_function_span(&parsed, export).is_some() && !find_function(&parsed, export).is_some_and(|f| awaits_only_imports(&f)) {
      return Ok(None);
    }
    let slots = self.slots.iter().find(|(m, _)| m == module).map(|(_, names)| names.clone()).unwrap_or_default();
    match crate::extract::extract(&parsed, &file, &slots, None) {
      Ok(Some(extraction)) => {
        self.forget(&file);
        self.provided.insert(file.clone(), extraction.page.clone());
        self.provided.insert(extraction.island_file.clone(), extraction.island.clone());
        Ok(Some(Extracted { island: format!("{}#default", extraction.island_file), file: extraction.island_file, source: extraction.island, page: (file, extraction.page), holds: extraction.holds }))
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
    self.parsed.remove(file);
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
    let Some(parsed) = self.parsed.get(&file).cloned() else { return Err(error) };
    let Some(offset) = parsed.offset(residue.line, residue.column).and_then(|offset| crate::extract::page_offset(&parsed, offset)) else { return Err(error) };
    let slots = self.slots.iter().find(|(m, _)| m == module).map(|(_, names)| names.clone()).unwrap_or_default();
    let Ok(Some(extraction)) = crate::extract::extract(&parsed, &file, &slots, Some(offset)) else { return Err(error) };
    let forget = |set: &mut Self| {
      set.failed.remove(module);
      set.forget(&file);
    };
    forget(self);
    let before = self.provided.insert(file.clone(), extraction.page.clone());
    let island_before = self.provided.insert(extraction.island_file.clone(), extraction.island.clone());
    if self.lower(module).is_err() {
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
      return Err(error);
    }
    Ok(Some(Extracted { island: format!("{}#default", extraction.island_file), file: extraction.island_file, source: extraction.island, page: (file, extraction.page), holds: extraction.holds }))
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
        let parsed = self.parsed.get(file)?;
        let source = parsed.cm.files().first().map(|f| f.src.to_string())?;
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
    let parsed = self.parsed[file].clone();
    let mut out = ImageExports::default();
    for item in &parsed.module.body {
      let js::ModuleItem::ModuleDecl(js::ModuleDecl::ExportDecl(export)) = item else { continue };
      let js::Decl::Var(var) = &export.decl else { continue };
      for decl in &var.decls {
        let (js::Pat::Ident(name), Some(init)) = (&decl.name, decl.init.as_deref()) else { continue };
        let init = unwrap_literal(init);
        match name.id.sym.as_ref() {
          "size" => {
            let refused = || LowerError::from(parsed.residue(init.span(), "`size` is `{ width: <number>, height: <number> }` with literal numbers"));
            let js::Expr::Object(object) = init else { return Err(refused()) };
            for prop in &object.props {
              let js::PropOrSpread::Prop(prop) = prop else { return Err(refused()) };
              let js::Prop::KeyValue(kv) = &**prop else { return Err(refused()) };
              let key = match &kv.key {
                js::PropName::Ident(id) => id.sym.to_string(),
                js::PropName::Str(s) => s.value.to_string_lossy().into_owned(),
                _ => return Err(refused()),
              };
              let js::Expr::Lit(js::Lit::Num(n)) = unwrap_literal(&kv.value) else { return Err(refused()) };
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
            let which = name.id.sym.to_string();
            let text = match init {
              js::Expr::Lit(js::Lit::Str(s)) => s.value.to_string_lossy().into_owned(),
              js::Expr::Tpl(tpl) if tpl.exprs.is_empty() && tpl.quasis.len() == 1 => tpl.quasis[0].cooked.as_ref().map(|c| c.to_string_lossy().into_owned()).unwrap_or_default(),
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
    if self.parsed.contains_key(file) {
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
      return match source.and_then(|source| self.transformed(&base, &source, &overrides)) {
        Ok(parsed) => {
          self.parsed.insert(file.to_owned(), Rc::new(parsed));
          Ok(())
        }
        Err(error) => {
          self.failed.insert(file.to_owned(), error.clone());
          Err(error)
        }
      };
    }
    let parsed = match self.provided.get(file) {
      Some(source) => Ok(source.clone()),
      None => std::fs::read_to_string(self.app.join(file)).map_err(|e| LowerError::Parse { file: file.to_owned(), message: e.to_string() }),
    }
    .and_then(|source: String| match is_transformed(file) {
      true => self.transformed(file, &source, &[]),
      false => parse_with(file, &source, crate::jsx_parses(file)),
    });
    match parsed {
      Ok(parsed) => {
        self.parsed.insert(file.to_owned(), Rc::new(parsed));
        Ok(())
      }
      Err(error) => {
        self.failed.insert(file.to_owned(), error.clone());
        Err(error)
      }
    }
  }

  /// Every `import(`prefix${x}suffix`)` in `file`: by where its call starts,
  /// the files under the app it can load, each with the value of `x` that
  /// names it, and as globals every value each of those files exports, keyed
  /// `file#export`, so a destructured `meta` picks among them.
  fn template_imports(&mut self, file: &str) -> Result<(HashMap<u32, Vec<(String, String)>>, Vec<(String, Expr)>), LowerError> {
    let parsed = self.parsed[file].clone();
    let mut imports = HashMap::new();
    let mut globals = Vec::new();
    for (lo, prefix, suffix) in template_import_calls(&parsed.module) {
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
  fn transformed(&self, file: &str, source: &str, overrides: &[Override]) -> Result<Parsed, LowerError> {
    let Some(transform) = &self.transform else {
      return Err(LowerError::Parse { file: file.to_owned(), message: "an MDX or markdown module, which this fsr reads only when built with its `mdx` feature".to_owned() });
    };
    let written = transform(file, source, overrides).map_err(|message| LowerError::Parse { file: file.to_owned(), message })?;
    crate::parse_from(file, &written.source, true, written.origin.map(Rc::new))
  }

  /// The file an import names relative to the app, through an alias or a
  /// relative path; `None` for a bare or generated specifier the build does
  /// not follow.
  fn resolve_import(&self, from: &str, source: &str) -> Option<String> {
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
    let result = self.lower_loaded_in(file, export);
    self.lowering_component = outer;
    result
  }

  fn lower_loaded_in(&mut self, file: &str, export: &str) -> Result<Component, LowerError> {
    let mut globals: Vec<(String, Expr)> = Vec::new();
    let mut global_kinds = Vec::new();
    let mut global_formatters: Vec<(String, js::NewExpr)> = Vec::new();
    let mut module_bindings: Vec<(String, String, Expr)> = Vec::new();
    let mut unfollowed: Vec<(String, Option<LowerError>)> = Vec::new();
    let module = format!("{file}#{export}");
    // An element template has no browser half, so nothing would read what
    // hoisting records or rewrites.
    let element_template = self.elements.values().any(|m| *m == module);
    let meanings = self.meanings(file);
    if let Some(span) = tree_call(&self.parsed[file].clone(), &meanings).filter(|_| export == "default") {
      return Err(LowerError::Retired(self.parsed[file].residue(span, "`tree(...)` is gone: a layout is composition and its page renders in a region of its own, so `export default` the layout itself and share state with the page through the store")));
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
    let ((component, refs, providers, own_heads), hoisting) = loop {
      let meanings = self.meanings(file);
      let hook_sources: Vec<(String, _, String)> = hook_files.iter().map(|(name, at, export)| (name.clone(), self.parsed[at].clone(), export.clone())).collect();
      let (result, unbound, hoisting) = {
        let parsed = self.parsed[file].clone();
        let defaults = self.defaults.clone();
        let (function, class) = match find_function(&parsed, export) {
          Some(function) => (function, None),
          None => match find_class(&parsed, export) {
            Some((class, render)) => (render, Some(class)),
            None => {
              if let Some(span) = class_component_span(&parsed, export) {
                return Err(Lowerer::new(&parsed, &defaults).residue(span, CLASS_COMPONENT).into());
              }
              return Err(LowerError::MissingExport { file: file.to_owned(), export: export.to_owned() });
            }
          },
        };
        if let Some(span) = async_function_span(&parsed, export).filter(|_| !awaits_only_imports(&function)) {
          return Err(Lowerer::new(&parsed, &defaults).residue(span, ASYNC_COMPONENT).into());
        }
        let mut lowerer = Lowerer::new(&parsed, &defaults);
        lowerer.globals = globals.clone();
        lowerer.global_kinds = global_kinds.clone();
        lowerer.global_formatters = global_formatters.clone();
        lowerer.unfollowed = unfollowed.iter().map(|(name, _)| name.clone()).collect();
        lowerer.natives = self.natives.clone();
        // The browser half reads hoisted values through a hook, which a class's `render()` may not call.
        lowerer.hoisting = (!element_template && class.is_none()).then(Candidates::default);
        lowerer.meanings = Some(meanings);
        lowerer.prop_kinds = self.placement_kinds.get(&module).cloned().unwrap_or_default();
        let layout_root = self.layouts.iter().any(|m| *m == module);
        let slot_names = self.slots.iter().find(|(m, _)| *m == module).map(|(_, names)| names.clone()).unwrap_or_default();
        let mut cl = ComponentLowerer { lowerer, file, handlers: Vec::new(), refs: Vec::new(), select_value: None, props_name: None, children_name: None, forwards_ref: forwards_ref(&parsed, export), class, layout_root, slot_names, slot_props: Vec::new(), state: Vec::new(), hook: None, state_bindings: Vec::new(), store_bindings: Vec::new(), store_lets: Vec::new(), setters: Vec::new(), handler_fns: HashMap::new(), lowered_handlers: Vec::new(), elements: self.elements.clone(), providers: Vec::new(), assets: self.assets.clone(), rewrite_images: self.rewrite_images, heads: Vec::new(), hooks: hook_sources.iter().map(|(name, parsed, export)| (name.clone(), &**parsed, export.clone())).collect(), wanted_hook: None, pending_lets: Vec::new(), hook_calls: 0, unbound_in: None, inlining: Vec::new(), checking: std::cell::RefCell::new(Vec::new()), counts_children: Vec::new(), imports: imports.clone(), chosen: HashMap::new(), variants: Vec::new(), placed_kinds: Vec::new(), browser_texts: Vec::new(), module_lets: module_bindings.clone(), fn_refs: Vec::new() };
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
              if self.load(&declared).is_ok() && find_function(&self.parsed[&declared].clone(), &declared_name).is_some() {
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
          if let Some(made) = self.global_formatter(file, &name)? {
            global_formatters.push((name.clone(), made));
          }
          if let Some((_, declared_file, declared_name)) = self.module_lets.iter().find(|(k, _, _)| *k == key).cloned() {
            let seed = self.module_init(&declared_file, &declared_name)?;
            module_bindings.push((name.clone(), key.clone(), seed));
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
            if !self.browser_only.iter().any(|(m, _)| *m == module) {
              self.browser_only.push((module.clone(), residue.placed_at(Placement { file: file.to_owned(), line, column, tag: name.clone() })));
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
      let pure = state.is_empty() && !leaves_to_browser && hoist::static_tree(&component.render, &self.pure);
      let chunks = hoist::chunks(&mut component, &taint, &self.pure);
      let covered = hoist::cover(&mut component, &chunks);
      kept.retain(|id| !covered.contains(id));
      for placed in hoist::placed_with_state(&component, &taint) {
        self.unhoist_props(&placed);
      }
      self.pure.insert(module.clone(), pure);
      self.stateless.insert(module.clone(), state.is_empty() && !leaves_to_browser);
      let parsed = self.parsed[file].clone();
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
        let parsed = self.parsed[file].clone();
        let defaults = self.defaults.clone();
        let mut lowerer = Lowerer::new(&parsed, &defaults);
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
      if let Some(parsed) = self.parsed.get(&child_file) {
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
        let inherits = self.parsed.get(&child_file).is_none_or(|parsed| crate::vue::inherits_attrs(parsed));
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
    let parsed = self.parsed[&declared].clone();
    let Some(Found::Declared(_, stmts, _)) = find_function(&parsed, &declared_name) else { return false };
    let lifecycle = |callee: &js::Callee| match callee {
      js::Callee::Expr(callee) => match &**callee {
        js::Expr::Ident(id) if id.ctxt == parsed.top => find_import(&parsed, id.sym.as_ref()).is_some_and(|(source, imported)| source == "svelte" && matches!(imported.as_str(), "onMount" | "onDestroy" | "beforeUpdate" | "afterUpdate")),
        _ => false,
      },
      _ => false,
    };
    let timer = |expr: &js::Expr| match expr {
      js::Expr::Call(js::CallExpr { callee: js::Callee::Expr(callee), .. }) => matches!(&**callee, js::Expr::Ident(id) if id.ctxt == parsed.unresolved && matches!(id.sym.as_ref(), "setInterval" | "setTimeout" | "requestAnimationFrame")),
      _ => false,
    };
    !stmts.is_empty()
      && stmts.iter().all(|stmt| match stmt {
        js::Stmt::Expr(e) => matches!(&*e.expr, js::Expr::Call(call) if lifecycle(&call.callee)) || timer(&e.expr),
        js::Stmt::Decl(js::Decl::Var(var)) => var.decls.iter().all(|decl| matches!(&decl.name, js::Pat::Ident(_)) && decl.init.as_deref().is_some_and(timer)),
        js::Stmt::Empty(_) => true,
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
        let parsed = self.parsed[file].clone();
        let defaults = self.defaults.clone();
        let mut lowerer = Lowerer::new(&parsed, &defaults);
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
      Tmpl::Text(_) | Tmpl::Marker(_) | Tmpl::Html(_) | Tmpl::Expr(_) | Tmpl::Slot(_) => false,
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
        let parsed = self.parsed[file].clone();
        let lazy = || match find_value(&parsed, looked) {
          Some(Global::Const(js::Expr::Call(call))) => lazy_import_source(call),
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
    let parsed = self.parsed[file].clone();
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
    let parsed = self.parsed[file].clone();
    let mut meanings = Meanings { top: parsed.top, unresolved: parsed.unresolved, ..Meanings::default() };
    for item in &parsed.module.body {
      let js::ModuleItem::ModuleDecl(js::ModuleDecl::Import(import)) = item else { continue };
      if import.type_only {
        continue;
      }
      let source = import.src.value.to_atom_lossy().to_string();
      for spec in &import.specifiers {
        let (local, meant) = match spec {
          js::ImportSpecifier::Named(named) if !named.is_type_only => {
            let imported = match &named.imported {
              Some(js::ModuleExportName::Ident(id)) => id.sym.to_string(),
              Some(js::ModuleExportName::Str(s)) => s.value.to_atom_lossy().to_string(),
              None => named.local.sym.to_string(),
            };
            (&named.local, self.imported_meaning(file, &source, &imported))
          }
          js::ImportSpecifier::Named(_) => continue,
          js::ImportSpecifier::Default(d) if meaning::is_library(&source) => match meaning::library(&source, "default") {
            Some(construct) => (&d.local, Meaning::Library(construct)),
            None => (&d.local, Meaning::Namespace(meaning::library_namespace(&source))),
          },
          js::ImportSpecifier::Default(d) if assets::kind(&source).is_some() => (&d.local, Meaning::Asset { source: source.clone() }),
          js::ImportSpecifier::Default(d) => (&d.local, self.imported_meaning(file, &source, "default")),
          js::ImportSpecifier::Namespace(ns) => (&ns.local, self.namespace_meaning(file, &source)),
        };
        meanings.bindings.insert(local.sym.to_string(), meant);
      }
    }
    for item in &parsed.module.body {
      if let js::ModuleItem::ModuleDecl(js::ModuleDecl::ExportDefaultDecl(d)) = item {
        let named = match &d.decl {
          js::DefaultDecl::Fn(f) => f.ident.as_ref(),
          js::DefaultDecl::Class(c) => c.ident.as_ref(),
          _ => None,
        };
        if let Some(id) = named {
          meanings.bindings.insert(id.sym.to_string(), Meaning::Own { file: file.to_owned(), name: "default".to_owned() });
        }
        continue;
      }
      let var = match item {
        js::ModuleItem::Stmt(js::Stmt::Decl(js::Decl::Var(var))) | js::ModuleItem::ModuleDecl(js::ModuleDecl::ExportDecl(js::ExportDecl { decl: js::Decl::Var(var), .. })) => var,
        js::ModuleItem::Stmt(js::Stmt::Decl(decl)) | js::ModuleItem::ModuleDecl(js::ModuleDecl::ExportDecl(js::ExportDecl { decl, .. })) => {
          let name = match decl {
            js::Decl::Fn(f) => Some(f.ident.sym.to_string()),
            js::Decl::Class(c) => Some(c.ident.sym.to_string()),
            _ => None,
          };
          if let Some(name) = name {
            meanings.bindings.insert(name.clone(), Meaning::Own { file: file.to_owned(), name });
          }
          continue;
        }
        _ => continue,
      };
      for decl in &var.decls {
        let js::Pat::Ident(name) = &decl.name else { continue };
        let meant = match decl.init.as_deref().map(crate::meaning::bare) {
          Some(js::Expr::Call(call)) => match &call.callee {
            js::Callee::Expr(callee) => match meanings.of_expr(callee) {
              Some(Meaning::Library(meaning::Construct::CreateContext { react })) => {
                let defaults = self.defaults.clone();
                let default = match call.args.first() {
                  Some(arg) => Lowerer::new(&parsed, &defaults).expr(&arg.expr).unwrap_or(Expr::Lit(Lit::Null)),
                  None => Expr::Lit(Lit::Null),
                };
                Meaning::Context { id: format!("{file}#{}", name.id.sym), default, react }
              }
              Some(Meaning::Library(meaning::Construct::AsyncComponent)) => match lazy_import_source(call) {
                Some(source) => self.imported_meaning(file, &source, "default"),
                None => Meaning::Own { file: file.to_owned(), name: name.id.sym.to_string() },
              },
              _ => Meaning::Own { file: file.to_owned(), name: name.id.sym.to_string() },
            },
            _ => Meaning::Own { file: file.to_owned(), name: name.id.sym.to_string() },
          },
          Some(init @ (js::Expr::Ident(_) | js::Expr::Member(_))) => meanings.of_expr(init).unwrap_or_else(|| Meaning::Own { file: file.to_owned(), name: name.id.sym.to_string() }),
          _ => Meaning::Own { file: file.to_owned(), name: name.id.sym.to_string() },
        };
        meanings.bindings.insert(name.id.sym.to_string(), meant);
      }
    }
    for item in &parsed.module.body {
      let js::ModuleItem::ModuleDecl(decl) = item else { continue };
      match decl {
        js::ModuleDecl::ExportDecl(export) => {
          let names: Vec<String> = match &export.decl {
            js::Decl::Fn(f) => vec![f.ident.sym.to_string()],
            js::Decl::Class(c) => vec![c.ident.sym.to_string()],
            js::Decl::Var(var) => var.decls.iter().filter_map(|d| if let js::Pat::Ident(id) = &d.name { Some(id.id.sym.to_string()) } else { None }).collect(),
            _ => Vec::new(),
          };
          for name in names {
            let meant = meanings.bindings.get(&name).cloned().unwrap_or_else(|| Meaning::Own { file: file.to_owned(), name: name.clone() });
            meanings.exports.insert(name, meant);
          }
        }
        js::ModuleDecl::ExportNamed(named) if !named.type_only => {
          let source = named.src.as_ref().map(|s| s.value.to_atom_lossy().to_string());
          for spec in &named.specifiers {
            let export_name = |n: &js::ModuleExportName| match n {
              js::ModuleExportName::Ident(id) => id.sym.to_string(),
              js::ModuleExportName::Str(s) => s.value.to_atom_lossy().to_string(),
            };
            match spec {
              js::ExportSpecifier::Named(n) if !n.is_type_only => {
                let orig = export_name(&n.orig);
                let exported = n.exported.as_ref().map(export_name).unwrap_or_else(|| orig.clone());
                let meant = match &source {
                  Some(source) => self.imported_meaning(file, source, &orig),
                  None => meanings.bindings.get(&orig).cloned().unwrap_or_else(|| Meaning::Own { file: file.to_owned(), name: orig.clone() }),
                };
                meanings.exports.insert(exported, meant);
              }
              js::ExportSpecifier::Namespace(ns) => {
                if let Some(source) = &source {
                  let meant = self.namespace_meaning(file, source);
                  meanings.exports.insert(export_name(&ns.name), meant);
                }
              }
              js::ExportSpecifier::Default(d) => {
                let meant = meanings.bindings.get(d.exported.sym.as_ref()).cloned().unwrap_or_else(|| Meaning::Own { file: file.to_owned(), name: "default".to_owned() });
                meanings.exports.insert("default".to_owned(), meant);
              }
              _ => {}
            }
          }
        }
        js::ModuleDecl::ExportAll(all) if !all.type_only => {
          let source = all.src.value.to_atom_lossy().to_string();
          if let Meaning::Namespace(members) = self.namespace_meaning(file, &source) {
            for (name, meant) in members.iter() {
              meanings.exports.entry(name.clone()).or_insert_with(|| meant.clone());
            }
          }
        }
        js::ModuleDecl::ExportDefaultDecl(_) => {
          meanings.exports.insert("default".to_owned(), Meaning::Own { file: file.to_owned(), name: "default".to_owned() });
        }
        js::ModuleDecl::ExportDefaultExpr(e) => {
          let meant = match meanings.of_expr(&e.expr) {
            Some(Meaning::Own { file: declared, .. }) if declared == file => Meaning::Own { file: file.to_owned(), name: "default".to_owned() },
            Some(meant) => meant,
            None => Meaning::Own { file: file.to_owned(), name: "default".to_owned() },
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
      return Err(LowerError::Parse { file: file.to_owned(), message: format!("`{name}` is recursive, which the build cannot unroll") });
    }
    let meanings = self.meanings(file);
    let parsed = self.parsed[file].clone();
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
      return Ok(Some((Expr::Var(name.to_owned()), key)));
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
      if matches!(crate::strip_parens(init), js::Expr::New(n) if crate::intl_formatter(n).is_some()) {
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
    self.resolving.push(key.clone());
    let result = self.lower_global(file, item);
    self.resolving.pop();
    match result {
      Err(LowerError::Residue(residue)) if file.starts_with(&format!("{EXT_DIR}/")) => Err(LowerError::Extension(residue)),
      // A constant of a module whose `let`s its functions write is what the module's initialisation leaves it at, so it is read by replaying that initialisation.
      Ok(_) if replays => self.module_init(file, name).map(|expr| Some((expr, key))),
      other => other.map(|expr| Some((expr, key))),
    }
  }

  /// The module's initialisation up to `upto` as a block: each top-level `let` and `const` bound in order, with every call a `const` makes applied, then `upto`'s value returned. A `let` a function writes is advanced by those calls, so what comes out is the value the browser's copy of the module starts with.
  fn module_init(&mut self, file: &str, upto: &str) -> Result<Expr, LowerError> {
    let parsed = self.parsed[file].clone();
    let mut body = Vec::new();
    for (name, init, written) in module_lets_of(&parsed) {
      let expr = self.lower_global(file, Global::Const(init))?;
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
    let parsed = self.parsed[file].clone();
    Ok(match find_value(&parsed, name) {
      Some(Global::Const(init)) => crate::made_kind(init),
      _ => None,
    })
  }

  /// The `Intl` formatter a module-level `name` holds when it is declared as `new Intl.NumberFormat(...)` or `new Intl.DateTimeFormat(...)`.
  fn global_formatter(&mut self, file: &str, name: &str) -> Result<Option<js::NewExpr>, LowerError> {
    if let Some(crate::meaning::Meaning::Own { file: declared, name: declared_name }) = self.meanings(file).bindings.get(name).cloned() {
      if declared != file {
        self.load(&declared)?;
        return self.global_formatter(&declared, &declared_name);
      }
    }
    let parsed = self.parsed[file].clone();
    Ok(match find_value(&parsed, name) {
      Some(Global::Const(init)) => match crate::strip_parens(init) {
        js::Expr::New(n) if crate::intl_formatter(n).is_some() => Some(n.clone()),
        _ => None,
      },
      _ => None,
    })
  }

  /// A constant the plan names once or the expression itself when naming it
  /// would cost more than copying it. `CONST_WEIGHT` is where a copy per
  /// reference starts to dominate the plan.
  fn name_if_large(&mut self, key: String, expr: Expr) -> Expr {
    if weight(&expr) < CONST_WEIGHT || matches!(expr, Expr::Lambda { .. } | Expr::Ext { .. }) || is_asset_record(&expr) {
      return expr;
    }
    self.consts.entry(key.clone()).or_insert(expr);
    Expr::Const(key)
  }



  fn lower_global(&mut self, file: &str, item: Global<'_>) -> Result<Expr, LowerError> {
    let mut globals: Vec<(String, Expr)> = Vec::new();
    let mut global_kinds = Vec::new();
    loop {
      let parsed = self.parsed[file].clone();
      let defaults = self.defaults.clone();
      let meanings = self.meanings(file);
      let mut lowerer = Lowerer::new(&parsed, &defaults);
      lowerer.meanings = Some(meanings);
      lowerer.globals = globals.clone();
      lowerer.global_kinds = global_kinds.clone();
      lowerer.natives = self.natives.clone();
      let result = match &item {
        Global::Const(init) => lowerer.expr(init),
        Global::Function(params, body) => function_to_lambda(&mut lowerer, params, *body),
      };
      match result {
        Ok(expr) => return Ok(expr),
        Err(residue) => {
          let Some(name) = lowerer.unbound.take() else { return Err(residue.into()) };
          if globals.iter().any(|(n, _)| *n == name) {
            return Err(residue.into());
          }
          let Some((expr, key)) = self.global(file, &name)? else { return Err(residue.into()) };
          if let Some(kind) = self.global_kind(file, &name)? {
            global_kinds.push((name.clone(), kind));
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
    Tmpl::Text(_) | Tmpl::Marker(_) | Tmpl::Html(_) | Tmpl::Expr(_) | Tmpl::Slot(_) => {}
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
    Tmpl::Text(_) | Tmpl::Marker(_) | Tmpl::Html(_) | Tmpl::Expr(_) | Tmpl::Slot(_) => {}
  }
}

/// The modules `Island`, `island`, `Slot` and `Link` are read from: the
/// client's React module, which a React application also mounts with or the
/// dialect's own declarations, which an application without React types
/// against.
pub const TEMPLATE_SOURCES: &[&str] = &["@snapfire/fsr-authoring/template", "@snapfire/fsr-client/react", "@snapfire/fsr-client/vue", "@snapfire/fsr-client/jsx-runtime"];

/// The client library's root, where `action` comes from.
pub const CLIENT_SOURCE: &str = "@snapfire/fsr-client";

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
    Tmpl::Text(_) | Tmpl::Marker(_) | Tmpl::Html(_) | Tmpl::Expr(_) | Tmpl::Slot(_) => {}
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
    Tmpl::Text(_) | Tmpl::Marker(_) | Tmpl::Html(_) | Tmpl::Expr(_) | Tmpl::Slot(_) | Tmpl::Baked { .. } => false,
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
    Tmpl::Text(_) | Tmpl::Marker(_) | Tmpl::Html(_) | Tmpl::Expr(_) | Tmpl::Slot(_) => None,
  }
}

/// The tag and position that placed `module`, for a message about it.
fn refs_by_module(modules: &HashMap<String, String>, module: &str, refs: &[(String, (usize, usize))]) -> Option<(String, (usize, usize))> {
  let (placed, _) = modules.iter().find(|(_, m)| m.as_str() == module)?;
  let name = placed.split_once('#').map(|(_, n)| n).unwrap_or(placed);
  refs.iter().find(|(n, _)| n == name).cloned()
}

fn rewrite_modules(tmpl: Tmpl, modules: &HashMap<String, String>, islands: &HashMap<String, IslandTiming>) -> Tmpl {
  let walk = |children: Vec<Tmpl>| children.into_iter().map(|c| rewrite_modules(c, modules, islands)).collect();
  match tmpl {
    Tmpl::Component { module, props, children, id, keyed } => match islands.get(&module) {
      Some((when, mode)) => Tmpl::Island { module: modules.get(&module).cloned().unwrap_or(module), props, children: walk(children), when: when.clone(), mode: mode.clone(), id, define: false },
      None => Tmpl::Component { module: modules.get(&module).cloned().unwrap_or(module), props, children: walk(children), id, keyed },
    },
    Tmpl::Island { module, props, children, when, mode, id, define } => Tmpl::Island { module: modules.get(&module).cloned().unwrap_or(module), props, children: walk(children), when, mode, id, define },
    Tmpl::Element { tag, attrs, children } => Tmpl::Element { tag, attrs, children: walk(children) },
    Tmpl::Fragment(children) => Tmpl::Fragment(walk(children)),
    Tmpl::If { cond, then, r#else } => Tmpl::If { cond, then: Box::new(rewrite_modules(*then, modules, islands)), r#else: r#else.map(|e| Box::new(rewrite_modules(*e, modules, islands))) },
    Tmpl::For { over, params, body } => Tmpl::For { over, params, body: Box::new(rewrite_modules(*body, modules, islands)) },
    Tmpl::Let { name, expr, then } => Tmpl::Let { name, expr, then: Box::new(rewrite_modules(*then, modules, islands)) },
    other => other,
  }
}

#[derive(Clone, Copy)]
pub(crate) enum FunctionBody<'a> {
  Block(&'a [js::Stmt]),
  Expr(&'a js::Expr),
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
  let Some(Global::Const(js::Expr::Call(call))) = find_value(parsed, name) else { return Ok(None) };
  let js::Callee::Expr(callee) = &call.callee else { return Ok(None) };
  if meanings.of_expr(callee).and_then(|meant| meant.placement()) != Some("island") {
    return Ok(None);
  }
  let Some(js::Expr::Ident(target)) = call.args.first().map(|a| &*a.expr) else {
    return Err((call.span, "`island(...)` takes a component name first".to_owned()));
  };
  let mut when = None;
  let mut mode = None;
  if let Some(options) = call.args.get(1) {
    let js::Expr::Object(obj) = &*options.expr else { return Err((options.span(), "`island(...)` options must be an object literal".to_owned())) };
    for prop in &obj.props {
      let js::PropOrSpread::Prop(prop) = prop else { return Err((options.span(), "a spread in `island(...)` options".to_owned())) };
      let js::Prop::KeyValue(kv) = &**prop else { return Err((options.span(), "`island(...)` options are `when` and `mode`".to_owned())) };
      let value = match &*kv.value {
        js::Expr::Lit(js::Lit::Str(s)) => Some(s.value.to_atom_lossy().to_string()),
        _ => None,
      };
      match prop_name(&kv.key).as_deref() {
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
  Ok(Some(IslandAlias { target: target.sym.to_string(), when, mode }))
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

pub(crate) fn generated_action_of(parsed: &Parsed, callee: &js::Expr) -> Option<String> {
  let mut path: Vec<String> = Vec::new();
  let mut cursor = callee;
  loop {
    match cursor {
      js::Expr::Member(member) => {
        let js::MemberProp::Ident(name) = &member.prop else { return None };
        path.push(name.sym.to_string());
        cursor = &member.obj;
      }
      js::Expr::Ident(root) => {
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
  let Some(Global::Const(js::Expr::Call(call))) = find_value(parsed, name) else { return None };
  let js::Callee::Expr(callee) = &call.callee else { return None };
  if parsed.meanings.get().and_then(|meanings| meanings.of_expr(callee)) != Some(crate::meaning::Meaning::Library(crate::meaning::Construct::ClientAction)) {
    return None;
  }
  match call.args.first().map(|a| &*a.expr) {
    Some(js::Expr::Lit(js::Lit::Str(id))) => Some(id.value.to_atom_lossy().to_string()),
    _ => None,
  }
}

#[derive(Clone)]
enum Global<'a> {
  Const(&'a js::Expr),
  Function(Vec<js::Pat>, FunctionBody<'a>),
}

pub(crate) fn patterns(function: &js::Function) -> Vec<js::Pat> {
  function.params.iter().map(|p| p.pat.clone()).collect()
}

pub(crate) fn arrow_body(arrow: &js::ArrowExpr) -> FunctionBody<'_> {
  match &*arrow.body {
    js::ArrowFunctionBody::FunctionBody(b) => FunctionBody::Block(&b.stmts),
    js::ArrowFunctionBody::Expr(e) => FunctionBody::Expr(e),
  }
}

/// The exported or declared function named `export`; `default` is the
/// default export.
pub(crate) fn find_function<'a>(parsed: &'a Parsed, export: &str) -> Option<Found<'a>> {
  for item in &parsed.module.body {
    match item {
      js::ModuleItem::ModuleDecl(js::ModuleDecl::ExportDefaultDecl(d)) if export == "default" => {
        if let js::DefaultDecl::Fn(f) = &d.decl {
          let body = f.function.body.as_ref()?;
          return Some(Found::Declared(patterns(&f.function), &body.stmts, body.span));
        }
      }
      js::ModuleItem::ModuleDecl(js::ModuleDecl::ExportDefaultExpr(e)) if export == "default" => match &*e.expr {
        js::Expr::Arrow(arrow) => return Some(Found::Arrow(arrow)),
        js::Expr::Ident(id) => return find_function(parsed, id.sym.as_ref()),
        js::Expr::Call(call) => return unwrapped(parsed, call),
        _ => {}
      },
      js::ModuleItem::ModuleDecl(js::ModuleDecl::ExportDecl(export_decl)) => {
        if let Some(found) = decl_function(parsed, &export_decl.decl, export) {
          return Some(found);
        }
      }
      js::ModuleItem::Stmt(js::Stmt::Decl(decl)) => {
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
fn react_export(parsed: &Parsed, expr: &js::Expr) -> Option<String> {
  let imports = parsed.module.body.iter().filter_map(|item| match item {
    js::ModuleItem::ModuleDecl(js::ModuleDecl::Import(import)) if import.src.value.to_atom_lossy().as_ref() == "react" => Some(import),
    _ => None,
  });
  match crate::meaning::bare(expr) {
    js::Expr::Ident(id) => imports.flat_map(|import| import.specifiers.iter()).find_map(|spec| match spec {
      js::ImportSpecifier::Named(named) if named.local.sym == id.sym => Some(match &named.imported {
        Some(js::ModuleExportName::Ident(imported)) => imported.sym.to_string(),
        Some(js::ModuleExportName::Str(s)) => s.value.to_atom_lossy().to_string(),
        None => named.local.sym.to_string(),
      }),
      _ => None,
    }),
    js::Expr::Member(member) => {
      let (js::Expr::Ident(object), js::MemberProp::Ident(prop)) = (crate::meaning::bare(&member.obj), &member.prop) else { return None };
      let mut imports = imports;
      imports
        .any(|import| import.specifiers.iter().any(|spec| matches!(spec, js::ImportSpecifier::Default(d) if d.local.sym == object.sym) || matches!(spec, js::ImportSpecifier::Namespace(n) if n.local.sym == object.sym)))
        .then(|| prop.sym.to_string())
    }
    _ => None,
  }
}

/// The function `memo(f)` or `forwardRef(f)` wraps, through any nesting of the two.
fn unwrapped<'a>(parsed: &'a Parsed, call: &'a js::CallExpr) -> Option<Found<'a>> {
  let js::Callee::Expr(callee) = &call.callee else { return None };
  if !matches!(react_export(parsed, callee).as_deref(), Some("memo" | "forwardRef")) {
    return None;
  }
  match crate::meaning::bare(&call.args.first()?.expr) {
    js::Expr::Arrow(arrow) => Some(Found::Arrow(arrow)),
    js::Expr::Fn(f) => {
      let body = f.function.body.as_ref()?;
      Some(Found::Declared(patterns(&f.function), &body.stmts, body.span))
    }
    js::Expr::Call(inner) => unwrapped(parsed, inner),
    js::Expr::Ident(id) => find_function(parsed, id.sym.as_ref()),
    _ => None,
  }
}

/// Whether `export` is wrapped in `forwardRef`, whose function takes the ref as its second parameter.
fn forwards_ref(parsed: &Parsed, export: &str) -> bool {
  fn wraps(parsed: &Parsed, expr: &js::Expr) -> bool {
    let js::Expr::Call(call) = crate::meaning::bare(expr) else { return false };
    let js::Callee::Expr(callee) = &call.callee else { return false };
    match react_export(parsed, callee).as_deref() {
      Some("forwardRef") => true,
      Some("memo") => call.args.first().is_some_and(|arg| wraps(parsed, &arg.expr)),
      _ => false,
    }
  }
  parsed.module.body.iter().any(|item| {
    let decl = match item {
      js::ModuleItem::ModuleDecl(js::ModuleDecl::ExportDefaultExpr(e)) if export == "default" => {
        return match crate::meaning::bare(&e.expr) {
          js::Expr::Ident(id) => forwards_ref(parsed, id.sym.as_ref()),
          other => wraps(parsed, other),
        };
      }
      js::ModuleItem::ModuleDecl(js::ModuleDecl::ExportDecl(export_decl)) => &export_decl.decl,
      js::ModuleItem::Stmt(js::Stmt::Decl(decl)) => decl,
      _ => return false,
    };
    let js::Decl::Var(var) = decl else { return false };
    var.decls.iter().any(|d| matches!(&d.name, js::Pat::Ident(id) if id.id.sym.as_ref() == export) && d.init.as_deref().is_some_and(|init| wraps(parsed, init)))
  })
}

/// What a class component is told: the build lowers functions, so the class renders in the browser.
pub(crate) const CLASS_COMPONENT: &str = "a class component, which the build does not lower";

/// `export` when it is a class extending React's `Component` or `PureComponent` with a `render()` method, and that method's body.
fn find_class<'a>(parsed: &'a Parsed, export: &str) -> Option<(&'a js::Class, Found<'a>)> {
  let react_base = |class: &js::Class| class.super_class.as_deref().is_some_and(|base| matches!(react_export(parsed, base).as_deref(), Some("Component" | "PureComponent")));
  let render = |class: &'a js::Class| {
    class.body.iter().find_map(|member| match member {
      js::ClassMember::Method(m) if !m.is_static && prop_name(&m.key).as_deref() == Some("render") => {
        let body = m.function.body.as_ref()?;
        Some((class, Found::Declared(Vec::new(), &body.stmts, body.span)))
      }
      _ => None,
    })
  };
  for item in &parsed.module.body {
    let decl = match item {
      js::ModuleItem::ModuleDecl(js::ModuleDecl::ExportDefaultDecl(d)) if export == "default" => match &d.decl {
        js::DefaultDecl::Class(c) if react_base(&c.class) => return render(&c.class),
        _ => continue,
      },
      js::ModuleItem::ModuleDecl(js::ModuleDecl::ExportDefaultExpr(e)) if export == "default" => match crate::meaning::bare(&e.expr) {
        js::Expr::Ident(id) => return find_class(parsed, id.sym.as_ref()),
        js::Expr::Class(c) if react_base(&c.class) => return render(&c.class),
        _ => continue,
      },
      js::ModuleItem::ModuleDecl(js::ModuleDecl::ExportDecl(export_decl)) => &export_decl.decl,
      js::ModuleItem::Stmt(js::Stmt::Decl(decl)) => decl,
      _ => continue,
    };
    if let js::Decl::Class(c) = decl {
      if c.ident.sym.as_ref() == export && react_base(&c.class) {
        return render(&c.class);
      }
    }
  }
  None
}

/// The span of `export` when it is a class extending React's `Component` or `PureComponent`.
fn class_component_span(parsed: &Parsed, export: &str) -> Option<Span> {
  let react_base = |class: &js::Class| class.super_class.as_deref().is_some_and(|base| matches!(react_export(parsed, base).as_deref(), Some("Component" | "PureComponent")));
  for item in &parsed.module.body {
    let decl = match item {
      js::ModuleItem::ModuleDecl(js::ModuleDecl::ExportDefaultDecl(d)) if export == "default" => match &d.decl {
        js::DefaultDecl::Class(c) if react_base(&c.class) => return Some(c.class.span),
        _ => continue,
      },
      js::ModuleItem::ModuleDecl(js::ModuleDecl::ExportDefaultExpr(e)) if export == "default" => match crate::meaning::bare(&e.expr) {
        js::Expr::Ident(id) => return class_component_span(parsed, id.sym.as_ref()),
        js::Expr::Class(c) if react_base(&c.class) => return Some(c.class.span),
        _ => continue,
      },
      js::ModuleItem::ModuleDecl(js::ModuleDecl::ExportDecl(export_decl)) => &export_decl.decl,
      js::ModuleItem::Stmt(js::Stmt::Decl(decl)) => decl,
      _ => continue,
    };
    if let js::Decl::Class(c) = decl {
      if c.ident.sym.as_ref() == export && react_base(&c.class) {
        return Some(c.class.span);
      }
    }
  }
  None
}

pub(crate) enum Found<'a> {
  Declared(Vec<js::Pat>, &'a [js::Stmt], Span),
  Arrow(&'a js::ArrowExpr),
}

/// What an `async` component is told: the IR has no await in a render and the route's loader runs before it.
pub(crate) const ASYNC_COMPONENT: &str = "an `async` component; what it awaits belongs in the route's loader, `page.loader.ts`, which runs before the page renders";

/// The span of `export`'s function when it is `async`, found the way [`find_function`] finds it.
pub(crate) fn async_function_span(parsed: &Parsed, export: &str) -> Option<Span> {
  for item in &parsed.module.body {
    match item {
      js::ModuleItem::ModuleDecl(js::ModuleDecl::ExportDefaultDecl(d)) if export == "default" => {
        if let js::DefaultDecl::Fn(f) = &d.decl {
          return f.function.is_async.then_some(f.function.span);
        }
      }
      js::ModuleItem::ModuleDecl(js::ModuleDecl::ExportDefaultExpr(e)) if export == "default" => match &*e.expr {
        js::Expr::Arrow(arrow) => return arrow.is_async.then_some(arrow.span),
        js::Expr::Ident(id) => return async_function_span(parsed, id.sym.as_ref()),
        _ => {}
      },
      js::ModuleItem::ModuleDecl(js::ModuleDecl::ExportDecl(export_decl)) => {
        if let Some(span) = decl_async_span(&export_decl.decl, export) {
          return Some(span);
        }
      }
      js::ModuleItem::Stmt(js::Stmt::Decl(decl)) => {
        if let Some(span) = decl_async_span(decl, export) {
          return Some(span);
        }
      }
      _ => {}
    }
  }
  None
}

fn decl_async_span(decl: &js::Decl, name: &str) -> Option<Span> {
  match decl {
    js::Decl::Fn(f) if f.ident.sym.as_ref() == name => f.function.is_async.then_some(f.function.span),
    js::Decl::Var(var) => {
      let decl = var.decls.iter().find(|d| matches!(&d.name, js::Pat::Ident(id) if id.id.sym.as_ref() == name))?;
      match decl.init.as_deref()? {
        js::Expr::Arrow(arrow) => arrow.is_async.then_some(arrow.span),
        _ => None,
      }
    }
    _ => None,
  }
}

/// The specifier a `lazy(() => import("x"))` or `defineAsyncComponent(() => import("x"))` loads, from an arrow whose body is the import or returns it.
fn lazy_import_source(call: &js::CallExpr) -> Option<String> {
  let js::Expr::Arrow(arrow) = &*call.args.first()?.expr else { return None };
  let expr: &js::Expr = match &*arrow.body {
    js::ArrowFunctionBody::Expr(e) => e,
    js::ArrowFunctionBody::FunctionBody(body) => match body.stmts.as_slice() {
      [js::Stmt::Return(ret)] => ret.arg.as_deref()?,
      _ => return None,
    },
  };
  let js::Expr::Call(import) = crate::meaning::bare(expr) else { return None };
  if !matches!(import.callee, js::Callee::Import(_)) {
    return None;
  }
  match &*import.args.first()?.expr {
    js::Expr::Lit(js::Lit::Str(s)) => Some(s.value.to_atom_lossy().to_string()),
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
}

/// The span of `export default tree(...)` with `tree` from FSR's React
/// module or its dialect, a spelling the build refuses.
fn tree_call(parsed: &Parsed, meanings: &crate::meaning::Meanings) -> Option<Span> {
  parsed.module.body.iter().find_map(|item| {
    let js::ModuleItem::ModuleDecl(js::ModuleDecl::ExportDefaultExpr(e)) = item else { return None };
    let js::Expr::Call(call) = &*e.expr else { return None };
    let js::Callee::Expr(callee) = &call.callee else { return None };
    (meanings.of_expr(callee).and_then(|meant| meant.placement()) == Some("tree")).then_some(call.span)
  })
}

fn decl_function<'a>(parsed: &'a Parsed, decl: &'a js::Decl, name: &str) -> Option<Found<'a>> {
  match decl {
    js::Decl::Fn(f) if f.ident.sym.as_ref() == name => {
      let body = f.function.body.as_ref()?;
      Some(Found::Declared(patterns(&f.function), &body.stmts, body.span))
    }
    js::Decl::Var(var) => {
      let decl = var.decls.iter().find(|d| matches!(&d.name, js::Pat::Ident(id) if id.id.sym.as_ref() == name))?;
      match decl.init.as_deref()? {
        js::Expr::Arrow(arrow) => Some(Found::Arrow(arrow)),
        js::Expr::Call(call) => unwrapped(parsed, call),
        _ => None,
      }
    }
    _ => None,
  }
}

/// A module-level value: a `const` with an initialiser or a function.
fn find_value<'a>(parsed: &'a Parsed, name: &str) -> Option<Global<'a>> {
  for item in &parsed.module.body {
    let decl = match item {
      js::ModuleItem::ModuleDecl(js::ModuleDecl::ExportDecl(export_decl)) => &export_decl.decl,
      js::ModuleItem::Stmt(js::Stmt::Decl(decl)) => decl,
      js::ModuleItem::ModuleDecl(js::ModuleDecl::ExportDefaultDecl(d)) => {
        if let js::DefaultDecl::Fn(f) = &d.decl {
          if name == "default" || f.ident.as_ref().is_some_and(|id| id.sym.as_ref() == name) {
            let body = f.function.body.as_ref()?;
            return Some(Global::Function(patterns(&f.function), FunctionBody::Block(&body.stmts)));
          }
        }
        continue;
      }
      js::ModuleItem::ModuleDecl(js::ModuleDecl::ExportDefaultExpr(e)) if name == "default" => {
        return match crate::meaning::bare(&e.expr) {
          js::Expr::Ident(id) => find_value(parsed, id.sym.as_ref()),
          js::Expr::Arrow(arrow) => Some(Global::Function(arrow.params.clone(), arrow_body(arrow))),
          value => Some(Global::Const(value)),
        };
      }
      _ => continue,
    };
    match decl {
      js::Decl::Fn(f) if f.ident.sym.as_ref() == name => {
        let body = f.function.body.as_ref()?;
        return Some(Global::Function(patterns(&f.function), FunctionBody::Block(&body.stmts)));
      }
      js::Decl::Var(var) => {
        for d in &var.decls {
          if matches!(&d.name, js::Pat::Ident(id) if id.id.sym.as_ref() == name) {
            return match d.init.as_deref()? {
              js::Expr::Arrow(arrow) => Some(Global::Function(arrow.params.clone(), arrow_body(arrow))),
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
fn module_lets_of(parsed: &Parsed) -> Vec<(String, &js::Expr, bool)> {
  let writes = module_writes(&parsed.module);
  let mut out = Vec::new();
  for item in &parsed.module.body {
    let var = match item {
      js::ModuleItem::Stmt(js::Stmt::Decl(js::Decl::Var(var))) => var,
      js::ModuleItem::ModuleDecl(js::ModuleDecl::ExportDecl(export)) => match &export.decl {
        js::Decl::Var(var) => var,
        _ => continue,
      },
      _ => continue,
    };
    for decl in &var.decls {
      let (js::Pat::Ident(id), Some(init)) = (&decl.name, decl.init.as_deref()) else { continue };
      if matches!(init, js::Expr::Arrow(_) | js::Expr::Fn(_)) {
        continue;
      }
      let name = id.id.sym.to_string();
      let written = var.kind != js::VarDeclKind::Const && writes.contains(&name);
      out.push((name, init, written));
    }
  }
  out
}

/// The declaration span of `name` when it is a module-level `let` or `var` that a function in the module writes.
fn written_module_let(parsed: &Parsed, name: &str) -> Option<Span> {
  if !module_lets_of(parsed).iter().any(|(n, _, written)| n == name && *written) {
    return None;
  }
  for item in &parsed.module.body {
    let var = match item {
      js::ModuleItem::Stmt(js::Stmt::Decl(js::Decl::Var(var))) => var,
      js::ModuleItem::ModuleDecl(js::ModuleDecl::ExportDecl(export)) => match &export.decl {
        js::Decl::Var(var) => var,
        _ => continue,
      },
      _ => continue,
    };
    if let Some(decl) = var.decls.iter().find(|d| matches!(&d.name, js::Pat::Ident(id) if id.id.sym.as_ref() == name)) {
      return Some(decl.span);
    }
  }
  None
}

/// The specifier string of every value import, `export * from` and
/// `export { .. } from` in `module`.
fn value_sources(module: &js::Module) -> impl Iterator<Item = &js::Str> {
  module.body.iter().filter_map(|item| {
    let js::ModuleItem::ModuleDecl(decl) = item else { return None };
    match decl {
      js::ModuleDecl::Import(import) if !import.type_only && (import.specifiers.is_empty() || import.specifiers.iter().any(|spec| !matches!(spec, js::ImportSpecifier::Named(named) if named.is_type_only))) => Some(&*import.src),
      js::ModuleDecl::ExportAll(export) if !export.type_only => Some(&*export.src),
      js::ModuleDecl::ExportNamed(js::NamedExport { src: Some(src), type_only: false, .. }) => Some(&**src),
      _ => None,
    }
  })
}

/// Every `import(`prefix${x}suffix`)` in `module`: where the call starts and
/// the literal text on either side of its one substitution.
fn template_import_calls(module: &js::Module) -> Vec<(u32, String, String)> {
  struct Found(Vec<(u32, String, String)>);
  impl Visit for Found {
    fn visit_call_expr(&mut self, call: &js::CallExpr) {
      if let (js::Callee::Import(_), Some(js::Expr::Tpl(tpl))) = (&call.callee, call.args.first().map(|a| &*a.expr)) {
        if let ([before, after], [_]) = (tpl.quasis.as_slice(), tpl.exprs.as_slice()) {
          let text = |q: &js::TplElement| q.cooked.as_ref().map(|c| c.to_atom_lossy().to_string()).unwrap_or_else(|| q.raw.to_string());
          self.0.push((call.span.lo.0, text(before), text(after)));
        }
      }
      call.visit_children_with(self);
    }
  }
  let mut found = Found(Vec::new());
  module.visit_with(&mut found);
  found.0
}

/// Whether every `await` in a component's body awaits an `import(...)`, which
/// the build resolves itself, so the component may be `async`.
fn awaits_only_imports(found: &Found<'_>) -> bool {
  struct Awaits(bool);
  impl Visit for Awaits {
    fn visit_await_expr(&mut self, awaited: &js::AwaitExpr) {
      let mut arg = &*awaited.arg;
      while let js::Expr::Paren(inner) = arg {
        arg = &inner.expr;
      }
      if !matches!(arg, js::Expr::Call(call) if matches!(call.callee, js::Callee::Import(_))) {
        self.0 = false;
      }
      awaited.visit_children_with(self);
    }
    fn visit_for_of_stmt(&mut self, stmt: &js::ForOfStmt) {
      if stmt.is_await {
        self.0 = false;
      }
      stmt.visit_children_with(self);
    }
  }
  let mut awaits = Awaits(true);
  match found {
    Found::Declared(_, stmts, _) => stmts.iter().for_each(|s| s.visit_with(&mut awaits)),
    Found::Arrow(arrow) => arrow.body.visit_with(&mut awaits),
  }
  awaits.0
}

/// The call an `await import(...)` awaits, through parentheses.
fn awaited_import(init: &js::Expr) -> Option<&js::CallExpr> {
  let mut expr = init;
  while let js::Expr::Paren(inner) = expr {
    expr = &inner.expr;
  }
  let js::Expr::Await(awaited) = expr else { return None };
  let mut arg = &*awaited.arg;
  while let js::Expr::Paren(inner) = arg {
    arg = &inner.expr;
  }
  match arg {
    js::Expr::Call(call) if matches!(call.callee, js::Callee::Import(_)) => Some(call),
    _ => None,
  }
}

/// Every `import("literal")` in `module`, wherever it sits: a module the browser loads on demand, which the build compiles like one it imports statically and never preloads.
fn dynamic_sources(module: &js::Module) -> Vec<String> {
  struct Dynamic(Vec<String>);
  impl Visit for Dynamic {
    fn visit_call_expr(&mut self, call: &js::CallExpr) {
      if matches!(call.callee, js::Callee::Import(_)) {
        if let Some(js::Expr::Lit(js::Lit::Str(s))) = call.args.first().map(|a| &*a.expr) {
          self.0.push(s.value.to_atom_lossy().to_string());
        }
      }
      call.visit_children_with(self);
    }
  }
  let mut found = Dynamic(Vec::new());
  module.visit_with(&mut found);
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
fn native_declaration(parsed: &Parsed, init: &js::Expr) -> Result<Option<(String, String, Reach)>, LowerError> {
  let js::Expr::Call(call) = init else { return Ok(None) };
  let js::Callee::Expr(callee) = &call.callee else { return Ok(None) };
  if parsed.meanings.get().and_then(|meanings| meanings.of_expr(callee)).and_then(|meant| meant.std()) != Some("native") {
    return Ok(None);
  }
  let name = match call.args.first().map(|a| &*a.expr) {
    Some(js::Expr::Lit(js::Lit::Str(s))) => s.value.to_atom_lossy().to_string(),
    Some(other) => return Err(LowerError::Extension(parsed.residue(other.span(), "the name of a native pair must be a string literal"))),
    None => return Err(LowerError::Extension(parsed.residue(call.span, "`native` takes the pair's name, `module.member`"))),
  };
  let Some((module, member)) = name.split_once('.') else {
    return Err(LowerError::Extension(parsed.residue(call.span, format!("`{name}` is not a pair name; write `module.member`"))));
  };
  if module.is_empty() || member.is_empty() || module.contains('.') || member.contains('.') {
    return Err(LowerError::Extension(parsed.residue(call.span, format!("`{name}` is not a pair name; write `module.member`"))));
  }
  let reach = if call.args.len() >= 2 { Reach::Render } else { Reach::Body };
  Ok(Some((module.to_owned(), member.to_owned(), reach)))
}

/// `(source, imported name)` for a value import binding `local`.
/// A JSX tag's name as written, `Ns.Theme.Provider` for a member chain.
fn jsx_tag_name(name: &js::JSXElementName) -> String {
  match name {
    js::JSXElementName::Ident(id) => id.sym.to_string(),
    js::JSXElementName::JSXMemberExpr(m) => format!("{}.{}", jsx_object_name(&m.obj), m.prop.sym),
    js::JSXElementName::JSXNamespacedName(n) => format!("{}:{}", n.ns.sym, n.name.sym),
  }
}

fn jsx_object_name(obj: &js::JSXObject) -> String {
  match obj {
    js::JSXObject::Ident(id) => id.sym.to_string(),
    js::JSXObject::JSXMemberExpr(m) => format!("{}.{}", jsx_object_name(&m.obj), m.prop.sym),
  }
}

pub(crate) fn find_import(parsed: &Parsed, local: &str) -> Option<(String, String)> {
  for item in &parsed.module.body {
    let js::ModuleItem::ModuleDecl(js::ModuleDecl::Import(import)) = item else { continue };
    if import.type_only {
      continue;
    }
    for spec in &import.specifiers {
      match spec {
        js::ImportSpecifier::Named(named) if !named.is_type_only && named.local.sym.as_ref() == local => {
          let imported = match &named.imported {
            Some(js::ModuleExportName::Ident(id)) => id.sym.to_string(),
            Some(js::ModuleExportName::Str(s)) => s.value.to_atom_lossy().to_string(),
            None => local.to_owned(),
          };
          return Some((import.src.value.to_atom_lossy().to_string(), imported));
        }
        js::ImportSpecifier::Default(d) if d.local.sym.as_ref() == local => {
          return Some((import.src.value.to_atom_lossy().to_string(), "default".to_owned()));
        }
        _ => {}
      }
    }
  }
  None
}

/// A module-level function as a lambda: parameters bind as in `Lowerer::lambda`;
/// `const`s inline; `if (c) return a;` chains become ternaries.
fn function_to_lambda(lowerer: &mut Lowerer<'_>, params: &[js::Pat], body: FunctionBody<'_>) -> Lowered<Expr> {
  let depth = lowerer.scope.len();
  let names = bind_params(lowerer, params)?;
  let result = match body {
    FunctionBody::Expr(e) => lowerer.expr(e),
    FunctionBody::Block(stmts) => {
      // A body that writes a name it neither takes nor declares, a module-level `let`, keeps its statements: a block lambda can write what is around it where an expression lambda cannot.
      let declared = declared_names(stmts);
      let writes_outer = written_names(stmts, false).into_iter().any(|name| !names.contains(&name) && !declared.contains(&name));
      if writes_outer {
        lowerer.block(stmts).map(Expr::Block)
      } else {
        // A parameter is the function's own local, so the body may write it.
        let refs: Vec<&js::Stmt> = stmts.iter().collect();
        block_to_expr_inner(lowerer, &refs, depth)
      }
    }
  };
  lowerer.scope.truncate(depth);
  Ok(Expr::Lambda { params: names, body: Box::new(result?) })
}

/// The bare names `stmts` write through `=`, a compound assignment or `++`, in source order; nested functions and arrows are walked when `through_functions` says so.
pub(crate) fn written_names(stmts: &[js::Stmt], through_functions: bool) -> Vec<String> {
  let mut writes = BareWrites { names: Vec::new(), through_functions };
  for stmt in stmts {
    stmt.visit_with(&mut writes);
  }
  writes.names
}

/// The bare names a module writes anywhere, its functions included.
pub(crate) fn module_writes(module: &js::Module) -> Vec<String> {
  let mut writes = BareWrites { names: Vec::new(), through_functions: true };
  module.visit_with(&mut writes);
  writes.names
}

struct BareWrites {
  names: Vec<String>,
  through_functions: bool,
}

impl Visit for BareWrites {
  fn visit_assign_expr(&mut self, assign: &js::AssignExpr) {
    if let js::AssignTarget::Simple(js::SimpleAssignTarget::Ident(id)) = &assign.left {
      let name = id.id.sym.to_string();
      if !self.names.contains(&name) {
        self.names.push(name);
      }
    }
    assign.visit_children_with(self);
  }
  fn visit_update_expr(&mut self, update: &js::UpdateExpr) {
    if let js::Expr::Ident(id) = &*update.arg {
      let name = id.sym.to_string();
      if !self.names.contains(&name) {
        self.names.push(name);
      }
    }
    update.visit_children_with(self);
  }
  fn visit_arrow_expr(&mut self, arrow: &js::ArrowExpr) {
    if self.through_functions {
      arrow.visit_children_with(self);
    }
  }
  fn visit_function(&mut self, function: &js::Function) {
    if self.through_functions {
      function.visit_children_with(self);
    }
  }
}

/// Reads and declarations of a function, by name and syntax context.
#[derive(Default)]
pub(crate) struct Free {
  reads: Vec<(String, swc_core::common::SyntaxContext)>,
  declared: Vec<(String, swc_core::common::SyntaxContext)>,
}

impl Visit for Free {
  fn visit_ident(&mut self, id: &js::Ident) {
    self.reads.push((id.sym.to_string(), id.ctxt));
  }
  fn visit_binding_ident(&mut self, id: &js::BindingIdent) {
    self.declared.push((id.id.sym.to_string(), id.id.ctxt));
    id.visit_children_with(self);
  }
  fn visit_fn_decl(&mut self, f: &js::FnDecl) {
    self.declared.push((f.ident.sym.to_string(), f.ident.ctxt));
    f.visit_children_with(self);
  }
  fn visit_fn_expr(&mut self, f: &js::FnExpr) {
    if let Some(id) = &f.ident {
      self.declared.push((id.sym.to_string(), id.ctxt));
    }
    f.visit_children_with(self);
  }
}

/// The first name `node` reads that is neither its own, a module-level binding nor a global: a name of the component it sits in. A function reading one cannot be imported from its module.
pub(crate) fn closes_over<N: VisitWith<Free>>(parsed: &Parsed, node: &N) -> Option<String> {
  let mut free = Free::default();
  node.visit_with(&mut free);
  free.reads.into_iter().find(|(name, ctxt)| *ctxt != parsed.top && *ctxt != parsed.unresolved && !free.declared.contains(&(name.clone(), *ctxt))).map(|(name, _)| name)
}

/// Whether `e` is an object literal with at least one field, every field a name holding an arrow or a `function`.
fn functions_only(e: &js::Expr) -> bool {
  let js::Expr::Object(obj) = e else { return false };
  !obj.props.is_empty()
    && obj.props.iter().all(|prop| match prop {
      js::PropOrSpread::Prop(p) => matches!(&**p, js::Prop::KeyValue(kv) if matches!(crate::meaning::bare(&kv.value), js::Expr::Arrow(_) | js::Expr::Fn(_))),
      js::PropOrSpread::Spread(_) => false,
    })
}

/// The names `stmts` declare at any depth outside nested functions.
pub(crate) fn declared_names(stmts: &[js::Stmt]) -> Vec<String> {
  struct Declared(Vec<String>);
  impl Visit for Declared {
    fn visit_var_declarator(&mut self, decl: &js::VarDeclarator) {
      if let js::Pat::Ident(id) = &decl.name {
        self.0.push(id.id.sym.to_string());
      }
      decl.visit_children_with(self);
    }
    fn visit_arrow_expr(&mut self, _: &js::ArrowExpr) {}
    fn visit_function(&mut self, _: &js::Function) {}
  }
  let mut declared = Declared(Vec::new());
  for stmt in stmts {
    stmt.visit_with(&mut declared);
  }
  declared.0
}

/// Each `x++`, `x--`, `++x` or `--x` on a bare name that `stmt` reaches unconditionally, in source order: its span, the name, whether it is a prefix and whether it adds. One under a condition, a short-circuit or inside a function is left out, since lifting it would run it too soon.
pub(crate) fn updates_in(stmt: &js::Stmt) -> Vec<(Span, String, bool, bool)> {
  struct Updates {
    found: Vec<(Span, String, bool, bool)>,
    guarded: usize,
  }
  impl Visit for Updates {
    fn visit_update_expr(&mut self, update: &js::UpdateExpr) {
      if self.guarded == 0 {
        if let js::Expr::Ident(id) = &*update.arg {
          self.found.push((update.span, id.sym.to_string(), update.prefix, update.op == js::UpdateOp::PlusPlus));
        }
      }
      update.visit_children_with(self);
    }
    fn visit_cond_expr(&mut self, cond: &js::CondExpr) {
      cond.test.visit_with(self);
      self.guarded += 1;
      cond.cons.visit_with(self);
      cond.alt.visit_with(self);
      self.guarded -= 1;
    }
    fn visit_bin_expr(&mut self, bin: &js::BinExpr) {
      bin.left.visit_with(self);
      let short = matches!(bin.op, js::BinaryOp::LogicalAnd | js::BinaryOp::LogicalOr | js::BinaryOp::NullishCoalescing);
      if short {
        self.guarded += 1;
      }
      bin.right.visit_with(self);
      if short {
        self.guarded -= 1;
      }
    }
    fn visit_arrow_expr(&mut self, _: &js::ArrowExpr) {}
    fn visit_function(&mut self, _: &js::Function) {}
    fn visit_if_stmt(&mut self, _: &js::IfStmt) {}
    fn visit_for_of_stmt(&mut self, _: &js::ForOfStmt) {}
    fn visit_for_stmt(&mut self, _: &js::ForStmt) {}
    fn visit_while_stmt(&mut self, _: &js::WhileStmt) {}
    fn visit_switch_stmt(&mut self, _: &js::SwitchStmt) {}
  }
  let mut updates = Updates { found: Vec::new(), guarded: 0 };
  stmt.visit_with(&mut updates);
  updates.found
}

pub(crate) fn bind_params(lowerer: &mut Lowerer<'_>, params: &[js::Pat]) -> Lowered<Vec<String>> {
  let mut names = Vec::new();
  for (i, pat) in params.iter().enumerate() {
    match pat {
      js::Pat::Ident(id) => {
        let name = id.id.sym.to_string();
        lowerer.scope.push((name.clone(), Expr::Var(name.clone())));
        names.push(name);
      }
      js::Pat::Assign(assign) if matches!(&*assign.left, js::Pat::Ident(_)) => {
        let js::Pat::Ident(id) = &*assign.left else { unreachable!() };
        let name = id.id.sym.to_string();
        let default = lowerer.expr(&assign.right)?;
        lowerer.scope.push((name.clone(), Expr::Coalesce(Box::new(Expr::Var(name.clone())), Box::new(default))));
        names.push(name);
      }
      js::Pat::Rest(rest) => return Err(lowerer.residue(rest.span, "a rest parameter")),
      pattern => {
        let positional = format!("${i}");
        bind_pattern(lowerer, pattern, Expr::Var(positional.clone()))?;
        names.push(positional);
      }
    }
  }
  Ok(names)
}

/// Binds each name `pat` declares to the part of `target` it reads: a field, an item, the rest of an object or of an array. A default applies when the part is null or absent.
pub(crate) fn bind_pattern(lowerer: &mut Lowerer<'_>, pat: &js::Pat, target: Expr) -> Lowered<()> {
  match pat {
    js::Pat::Ident(id) => {
      lowerer.scope.push((id.id.sym.to_string(), target));
      Ok(())
    }
    js::Pat::Assign(assign) => {
      let default = lowerer.expr(&assign.right)?;
      bind_pattern(lowerer, &assign.left, Expr::Coalesce(Box::new(target), Box::new(default)))
    }
    js::Pat::Object(obj) => bind_object(lowerer, obj, target),
    js::Pat::Array(arr) => {
      for (j, elem) in arr.elems.iter().enumerate() {
        match elem {
          None => {}
          Some(js::Pat::Rest(rest)) => {
            let tail = Expr::Builtin { name: Builtin::Slice, args: vec![target.clone(), Expr::Lit(Lit::Float(j as f64))] };
            bind_pattern(lowerer, &rest.arg, tail)?;
          }
          Some(pat) => bind_pattern(lowerer, pat, target.clone().index(Expr::Lit(Lit::Float(j as f64))))?,
        }
      }
      Ok(())
    }
    other => Err(lowerer.residue(other.span(), "a pattern the build does not read")),
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
fn prop_kinds_of(parsed: &Parsed, pat: &js::Pat) -> Vec<(String, crate::Kind)> {
  let ann = match pat {
    js::Pat::Ident(id) => id.type_ann.as_ref(),
    js::Pat::Object(obj) => obj.type_ann.as_ref(),
    _ => None,
  };
  ann.map(|ann| member_kinds(parsed, &ann.type_ann, 0)).unwrap_or_default()
}

fn member_kinds(parsed: &Parsed, ty: &js::TsType, depth: usize) -> Vec<(String, crate::Kind)> {
  if depth > 8 {
    return Vec::new();
  }
  let of_members = |members: &[js::TsTypeElement]| -> Vec<(String, crate::Kind)> {
    members
      .iter()
      .filter_map(|member| {
        let js::TsTypeElement::TsPropertySignature(prop) = member else { return None };
        let js::Expr::Ident(key) = &*prop.key else { return None };
        crate::Kind::of_type(&prop.type_ann.as_ref()?.type_ann).map(|kind| (key.sym.to_string(), kind))
      })
      .collect()
  };
  match ty {
    js::TsType::TsParenthesizedType(p) => member_kinds(parsed, &p.type_ann, depth + 1),
    js::TsType::TsTypeLit(lit) => of_members(&lit.members),
    js::TsType::TsUnionOrIntersectionType(js::TsUnionOrIntersectionType::TsIntersectionType(i)) => i.types.iter().flat_map(|t| member_kinds(parsed, t, depth + 1)).collect(),
    js::TsType::TsTypeRef(r) => {
      let js::TsEntityName::Ident(id) = &r.type_name else { return Vec::new() };
      let name = id.sym.as_ref();
      for item in &parsed.module.body {
        let decl = match item {
          js::ModuleItem::ModuleDecl(js::ModuleDecl::ExportDecl(export)) => &export.decl,
          js::ModuleItem::Stmt(js::Stmt::Decl(decl)) => decl,
          _ => continue,
        };
        match decl {
          js::Decl::TsTypeAlias(alias) if alias.id.sym.as_ref() == name => return member_kinds(parsed, &alias.type_ann, depth + 1),
          js::Decl::TsInterface(interface) if interface.id.sym.as_ref() == name => return of_members(&interface.body.body),
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
    Tmpl::Text(_) | Tmpl::Marker(_) | Tmpl::Html(_) | Tmpl::Expr(_) | Tmpl::Slot(_) => {}
  }
}

pub(crate) fn bind_object(lowerer: &mut Lowerer<'_>, obj: &js::ObjectPat, target: Expr) -> Lowered<()> {
  let mut taken: Vec<String> = Vec::new();
  for prop in &obj.props {
    match prop {
      js::ObjectPatProp::Assign(a) => {
        let name = a.key.id.sym.to_string();
        taken.push(name.clone());
        let read = target.clone().field(name.clone());
        let bound = match &a.value {
          Some(default) => Expr::Coalesce(Box::new(read), Box::new(lowerer.expr(default)?)),
          None => read,
        };
        lowerer.scope.push((name, bound));
      }
      js::ObjectPatProp::KeyValue(kv) => {
        let key = prop_name(&kv.key).ok_or_else(|| lowerer.residue(kv.key.span(), "a computed field in a pattern"))?;
        taken.push(key.clone());
        bind_pattern(lowerer, &kv.value, target.clone().field(key))?;
      }
      js::ObjectPatProp::Rest(r) => {
        let mut args = vec![target.clone()];
        args.extend(taken.iter().map(|key| Expr::lit_str(key.clone())));
        bind_pattern(lowerer, &r.arg, Expr::Builtin { name: Builtin::Omit, args })?;
      }
    }
  }
  Ok(())
}

/// A `switch` as its arms: the tests that select each body in order and the `default` body. A body ends with a `break` (dropped), a `return` or the end of the last arm; cases with no body share the next one's.
pub(crate) type SwitchArms<'s> = (Vec<(Vec<&'s js::Expr>, &'s [js::Stmt])>, Option<&'s [js::Stmt]>);

pub(crate) fn switch_arms<'s>(lowerer: &Lowerer<'_>, sw: &'s js::SwitchStmt) -> Lowered<SwitchArms<'s>> {
  let mut arms = Vec::new();
  let mut default = None;
  let mut tests: Vec<&js::Expr> = Vec::new();
  let last = sw.cases.len().saturating_sub(1);
  for (i, case) in sw.cases.iter().enumerate() {
    let is_default = case.test.is_none();
    if is_default && i != last {
      return Err(lowerer.residue(case.span, "a `default` before the last `case`"));
    }
    if let Some(test) = &case.test {
      tests.push(test);
    }
    if case.cons.is_empty() {
      continue;
    }
    let body: &[js::Stmt] = match case.cons.last() {
      Some(js::Stmt::Break(b)) if b.label.is_none() => &case.cons[..case.cons.len() - 1],
      Some(js::Stmt::Return(_) | js::Stmt::Throw(_)) => &case.cons,
      _ if i == last => &case.cons,
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
fn throw_expr(lowerer: &mut Lowerer<'_>, thrown: &js::ThrowStmt) -> Lowered<Expr> {
  let error_message = |callee: &js::Expr, args: &[js::ExprOrSpread]| match crate::meaning::bare(callee) {
    js::Expr::Ident(id) if id.sym.as_ref() == "Error" && id.ctxt == lowerer.meanings.as_ref().map(|m| m.unresolved).unwrap_or(id.ctxt) => Some(args.first().map(|a| (*a.expr).clone())),
    _ => None,
  };
  let message = match crate::meaning::bare(&thrown.arg) {
    js::Expr::Call(call) => match &call.callee {
      js::Callee::Expr(callee) => error_message(callee, &call.args),
      _ => None,
    },
    js::Expr::New(new) => error_message(&new.callee, new.args.as_deref().unwrap_or_default()),
    _ => None,
  };
  let message = match message {
    Some(Some(arg)) => lowerer.expr(&arg)?,
    Some(None) => Expr::lit_str(""),
    None => lowerer.expr(&thrown.arg)?,
  };
  Ok(Expr::Builtin { name: Builtin::Throw, args: vec![message] })
}

fn holds_break(stmt: &js::Stmt) -> bool {
  match stmt {
    js::Stmt::Break(_) => true,
    js::Stmt::Block(b) => b.stmts.iter().any(holds_break),
    js::Stmt::If(i) => holds_break(&i.cons) || i.alt.as_deref().is_some_and(holds_break),
    _ => false,
  }
}

/// Whether a statement returns or throws somewhere inside it, either of which ends the path it is on.
pub(crate) fn holds_return(stmt: &js::Stmt) -> bool {
  match stmt {
    js::Stmt::Return(_) | js::Stmt::Throw(_) => true,
    js::Stmt::Block(b) => b.stmts.iter().any(holds_return),
    js::Stmt::If(i) => holds_return(&i.cons) || i.alt.as_deref().is_some_and(holds_return),
    js::Stmt::Switch(sw) => sw.cases.iter().any(|c| c.cons.iter().any(holds_return)),
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
fn setter_called(call: &js::CallExpr, writable: &Writable<'_>) -> Option<String> {
  let js::Callee::Expr(callee) = &call.callee else { return None };
  let js::Expr::Ident(id) = &**callee else { return None };
  writable.setters.iter().find(|(setter, _)| *setter == id.sym.as_ref()).map(|(_, state)| state.clone())
}

/// A setter called while the component renders: `RERUN_PREFIX` and the state hold `[value]`, the next pass's state. An updater applies to what an earlier call in the pass asked for, else to the state this pass read, as React applies queued updates in order.
fn rebind_setter(lowerer: &mut Lowerer<'_>, call: &js::CallExpr, state: &str, assigned: &mut Vec<String>) -> Lowered<()> {
  let [arg] = call.args.as_slice() else { return Err(lowerer.residue(call.span, "a setter called while rendering with other than one argument")) };
  let key = format!("{}{state}", snapfire_fsr_ir::render::RERUN_PREFIX);
  let asked = current(lowerer, &key);
  let value = match &*arg.expr {
    js::Expr::Arrow(_) | js::Expr::Fn(_) => {
      let base = Expr::Ternary(Box::new(asked.clone()), Box::new(Expr::Index(Box::new(asked), Box::new(Expr::Lit(Lit::Int(0))))), Box::new(current(lowerer, state)));
      let f = match &*arg.expr {
        js::Expr::Arrow(arrow) => lowerer.lambda(arrow)?,
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

/// Records `const name = array[index]` on a writable local array as an alias:
/// the array's name, the index as it was then and where the array was bound,
/// so a write through `name` reaches the element JavaScript shares.
fn note_alias(lowerer: &mut Lowerer<'_>, name: &str, init: &js::Expr, writable: &Writable<'_>) -> Lowered<()> {
  let js::Expr::Member(member) = init else { return Ok(()) };
  let js::MemberProp::Computed(prop) = &member.prop else { return Ok(()) };
  let js::Expr::Ident(array) = &*member.obj else { return Ok(()) };
  let array = array.sym.to_string();
  if !writable.local(lowerer, &array) {
    return Ok(());
  }
  let Some(at) = lowerer.scope.iter().rposition(|(n, _)| *n == array) else { return Ok(()) };
  let index = lowerer.expr(&prop.expr)?;
  lowerer.scope.push((alias_key(name), Expr::Array(vec![Entry::Item(Expr::lit_str(array)), Entry::Item(index), Entry::Item(Expr::Lit(Lit::Int(at as i128)))])));
  Ok(())
}

enum Alias {
  None,
  Of { array: String, index: Expr },
  Stale(String),
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
  let [Entry::Item(Expr::Lit(Lit::Str(array))), Entry::Item(index), Entry::Item(Expr::Lit(Lit::Int(bound)))] = entries.as_slice() else { return Alias::None };
  match lowerer.scope.iter().rposition(|(n, _)| n == array) {
    Some(now) if now as i128 == *bound => Alias::Of { array: array.clone(), index: index.clone() },
    _ => Alias::Stale(array.clone()),
  }
}

/// Rebinds the local `name` to `value` after a write to one of its fields,
/// and the array element it aliases with it.
fn write_field(lowerer: &mut Lowerer<'_>, span: Span, name: &str, value: Expr, assigned: &mut Vec<String>) -> Lowered<()> {
  let alias = alias_of(lowerer, name);
  if let Alias::Stale(array) = &alias {
    return Err(lowerer.residue(span, format!("a write through `{name}`, an element of `{array}`, after `{array}` was written")));
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
pub(crate) fn rebind_stmt(lowerer: &mut Lowerer<'_>, stmt: &js::Stmt, writable: &Writable<'_>, assigned: &mut Vec<String>) -> Lowered<()> {
  let note = |assigned: &mut Vec<String>, name: &str| {
    if !assigned.iter().any(|a| a == name) {
      assigned.push(name.to_owned());
    }
  };
  match stmt {
    js::Stmt::Decl(js::Decl::Var(var)) => {
      for decl in &var.decls {
        let expr = match decl.init.as_deref() {
          Some(init) => lowerer.expr(init)?,
          None => Expr::Lit(Lit::Null),
        };
        bind_pattern(lowerer, &decl.name, expr)?;
        if let (js::Pat::Ident(id), Some(init)) = (&decl.name, decl.init.as_deref()) {
          lowerer.note_kind(id.id.sym.as_ref(), init);
          note_alias(lowerer, id.id.sym.as_ref(), init, writable)?;
        }
      }
      Ok(())
    }
    js::Stmt::Expr(e) => match &*e.expr {
      js::Expr::Assign(assign) => {
        let (name, read) = match &assign.left {
          js::AssignTarget::Simple(js::SimpleAssignTarget::Ident(id)) => (id.id.sym.to_string(), js::Expr::Ident(id.id.clone())),
          js::AssignTarget::Simple(js::SimpleAssignTarget::Member(m)) => match &*m.obj {
            js::Expr::Ident(id) => (id.sym.to_string(), js::Expr::Member(m.clone())),
            _ => return Err(lowerer.residue(assign.span, "a write to something other than a local")),
          },
          _ => return Err(lowerer.residue(assign.span, "a write to something other than a local")),
        };
        if !writable.local(lowerer, &name) {
          return Err(lowerer.residue_with(assign.span, format!("a write to `{name}`, which this body did not declare"), "declare a local with `let` and write that; state changes through its setter"));
        }
        let value = match assign.op.to_update() {
          None => lowerer.expr(&assign.right)?,
          Some(op) => lowerer.expr(&js::Expr::Bin(js::BinExpr { span: assign.span, op, left: Box::new(read.clone()), right: assign.right.clone() }))?,
        };
        match &read {
          js::Expr::Member(member) => {
            let field = prop_name_of(member).ok_or_else(|| lowerer.residue(member.span, "a write at a computed index of a local"))?;
            let value = Expr::Object(vec![Entry::Spread(current(lowerer, &name)), Entry::Field(field, value)]);
            write_field(lowerer, assign.span, &name, value, assigned)
          }
          _ => {
            lowerer.scope.push((name.clone(), value));
            note(assigned, &name);
            Ok(())
          }
        }
      }
      js::Expr::Update(update) => {
        let js::Expr::Ident(id) = &*update.arg else { return Err(lowerer.residue(update.span, "an update of something other than a local")) };
        let name = id.sym.to_string();
        if !writable.local(lowerer, &name) {
          return Err(lowerer.residue(update.span, format!("an update of `{name}`, which this body did not declare")));
        }
        let op = match update.op {
          js::UpdateOp::PlusPlus => js::BinaryOp::Add,
          js::UpdateOp::MinusMinus => js::BinaryOp::Sub,
        };
        let one = js::Expr::Lit(js::Lit::Num(js::Number { span: update.span, value: 1.0, raw: None }));
        let value = lowerer.expr(&js::Expr::Bin(js::BinExpr { span: update.span, op, left: update.arg.clone(), right: Box::new(one) }))?;
        lowerer.scope.push((name.clone(), value));
        note(assigned, &name);
        Ok(())
      }
      js::Expr::Call(call) => {
        if let Some(logged) = console_call(lowerer, call)? {
          let name = format!("$log{}", call.span.lo.0);
          lowerer.scope.push((name.clone(), logged));
          note(assigned, &name);
          return Ok(());
        }
        if let Some(looped) = for_each_loop(call, lowerer.meanings.as_deref()) {
          return rebind_stmt(lowerer, &looped, writable, assigned);
        }
        if let Some(state) = setter_called(call, writable) {
          return rebind_setter(lowerer, call, &state, assigned);
        }
        if let js::Callee::Expr(callee) = &call.callee {
          if let js::Expr::Member(m) = &**callee {
            if let (js::Expr::Ident(id), Some(method @ ("reverse" | "sort" | "pop" | "shift"))) = (&*m.obj, prop_name_of(m).as_deref()) {
              let name = id.sym.to_string();
              if writable.local(lowerer, &name) {
                let held = current(lowerer, &name);
                let value = match method {
                  "reverse" => Expr::Builtin { name: Builtin::Reverse, args: vec![held] },
                  "sort" => {
                    let by = match call.args.first().map(|a| &*a.expr) {
                      None => Expr::Lit(Lit::Null),
                      Some(js::Expr::Arrow(arrow)) => lowerer.lambda(arrow)?,
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
        let target = match &call.callee {
          js::Callee::Expr(callee) => match &**callee {
            js::Expr::Member(m) => match (&*m.obj, prop_name_of(m).as_deref()) {
              (js::Expr::Ident(id), Some(method @ ("push" | "unshift"))) if writable.local(lowerer, id.sym.as_ref()) => Some((id.sym.to_string(), None, method == "push")),
              (js::Expr::Member(field), Some(method @ ("push" | "unshift"))) => match (&*field.obj, prop_name_of(field)) {
                (js::Expr::Ident(id), Some(name)) if writable.local(lowerer, id.sym.as_ref()) => Some((id.sym.to_string(), Some((name, &*m.obj)), method == "push")),
                _ => None,
              },
              _ => None,
            },
            _ => None,
          },
          _ => None,
        };
        let Some((name, field, at_end)) = target else { return Err(lowerer.residue(call.span, "a call for its effect, which a straight-line body cannot hold")) };
        let mut items = Vec::with_capacity(call.args.len() + 1);
        for arg in &call.args {
          let value = lowerer.expr(&arg.expr)?;
          items.push(if arg.spread.is_some() { Entry::Spread(value) } else { Entry::Item(value) });
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
    js::Stmt::If(branch) => {
      let cond = lowerer.expr(&branch.test)?;
      let arms = [Some(&*branch.cons), branch.alt.as_deref()];
      rebind_arms(lowerer, vec![(Some(cond), arms[0].map(std::slice::from_ref).unwrap_or(&[]))], arms[1].map(std::slice::from_ref).unwrap_or(&[]), writable, assigned)
    }
    js::Stmt::Switch(sw) => {
      let disc = lowerer.expr(&sw.discriminant)?;
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
    js::Stmt::Block(block) => {
      let depth = lowerer.scope.len();
      let mut inner = Vec::new();
      for stmt in &block.stmts {
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
    js::Stmt::ForOf(for_of) => rebind_for_of(lowerer, for_of, writable, assigned),
    js::Stmt::For(counted) => rebind_counted(lowerer, counted, writable, assigned),
    js::Stmt::Empty(_) => Ok(()),
    other => Err(lowerer.residue(other.span(), "a statement a straight-line body cannot hold; it holds `const`s, `let`s, writes to them, `if`, `switch`, `for…of` and `for (let i = a; i < b; i++)`")),
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

impl Visit for Writes {
  fn visit_assign_expr(&mut self, assign: &js::AssignExpr) {
    match &assign.left {
      js::AssignTarget::Simple(js::SimpleAssignTarget::Ident(id)) => self.wrote(id.id.sym.as_ref()),
      js::AssignTarget::Simple(js::SimpleAssignTarget::Member(m)) => {
        if let js::Expr::Ident(id) = &*m.obj {
          self.wrote(id.sym.as_ref());
        }
      }
      _ => {}
    }
    assign.right.visit_with(self);
  }
  fn visit_update_expr(&mut self, update: &js::UpdateExpr) {
    if let js::Expr::Ident(id) = &*update.arg {
      self.wrote(id.sym.as_ref());
    }
    update.visit_children_with(self);
  }
  fn visit_call_expr(&mut self, call: &js::CallExpr) {
    if let js::Callee::Expr(callee) = &call.callee {
      if let js::Expr::Member(m) = &**callee {
        if let (js::Expr::Ident(id), Some("push" | "unshift")) = (&*m.obj, prop_name_of(m).as_deref()) {
          self.wrote(id.sym.as_ref());
        }
      }
    }
    call.visit_children_with(self);
  }
  fn visit_binding_ident(&mut self, id: &js::BindingIdent) {
    self.declared.push(id.id.sym.to_string());
  }
  fn visit_function(&mut self, _: &js::Function) {}
  fn visit_arrow_expr(&mut self, _: &js::ArrowExpr) {}
}

impl LoopNames {
  pub(crate) fn of(for_of: &js::ForOfStmt) -> Self {
    Self::over(for_of.span, &for_of.left, &for_of.body)
  }

  pub(crate) fn over<H: VisitWith<Writes>>(span: Span, head: &H, body: &js::Stmt) -> Self {
    let mut writes = Writes::default();
    head.visit_with(&mut writes);
    body.visit_with(&mut writes);
    let Writes { written, declared } = writes;
    let at = span.lo.0;
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
  pub(crate) fn head(for_of: &js::ForOfStmt) -> Option<&js::Pat> {
    match &for_of.left {
      js::ForHead::VarDecl(var) => match var.decls.as_slice() {
        [decl] if decl.init.is_none() => Some(&decl.name),
        _ => None,
      },
      _ => None,
    }
  }
}

/// A `for…of` as a fold: each outer local its body writes is the `reduce` over the items of the body's writes, starting from its value before the loop.
fn rebind_for_of(lowerer: &mut Lowerer<'_>, for_of: &js::ForOfStmt, writable: &Writable<'_>, assigned: &mut Vec<String>) -> Lowered<()> {
  if for_of.is_await {
    return Err(lowerer.residue(for_of.span, "`for await`, which a render cannot wait on"));
  }
  let Some(head) = LoopNames::head(for_of) else { return Err(lowerer.residue(for_of.span, "a `for…of` whose head is not one `const` or `let`")) };
  let names = LoopNames::of(for_of);
  let over = lowerer.expr(&for_of.right)?;
  let item = Expr::Var(names.item.clone());
  fold(lowerer, for_of.span, &names, over, &for_of.body, |lowerer| bind_pattern(lowerer, head, item), writable, assigned)
}

/// `for (let i = a; i < b; i++)` (also `<=`, `++i` and `i += 1`) whose body does not write `i` and whose bound reads nothing the body writes.
pub(crate) struct Counted {
  pub(crate) name: String,
  from: js::Expr,
  to: js::Expr,
  inclusive: bool,
}

impl Counted {
  pub(crate) fn of(lowerer: &Lowerer<'_>, f: &js::ForStmt) -> Lowered<Self> {
    let shape = |why: &str| lowerer.residue_with(f.span, format!("a `for` loop {why}"), "a loop the build reads is `for (const x of items)` or `for (let i = a; i < b; i++)`");
    let Some(js::VarDeclOrExpr::VarDecl(var)) = f.init.as_ref() else { return Err(shape("that does not declare its counter")) };
    let [decl] = var.decls.as_slice() else { return Err(shape("that declares more than one counter")) };
    let (js::Pat::Ident(id), Some(from)) = (&decl.name, decl.init.as_deref()) else { return Err(shape("whose counter has no starting value")) };
    let name = id.id.sym.to_string();
    let is_counter = |e: &js::Expr| matches!(e, js::Expr::Ident(i) if i.sym.as_ref() == name);
    let (to, inclusive) = match f.test.as_deref() {
      Some(js::Expr::Bin(b)) if is_counter(&b.left) && b.op == js::BinaryOp::Lt => (&*b.right, false),
      Some(js::Expr::Bin(b)) if is_counter(&b.left) && b.op == js::BinaryOp::LtEq => (&*b.right, true),
      _ => return Err(shape("whose test is not `i < b` or `i <= b`")),
    };
    let steps = match f.update.as_deref() {
      Some(js::Expr::Update(u)) => u.op == js::UpdateOp::PlusPlus && is_counter(&u.arg),
      Some(js::Expr::Assign(a)) => a.op == js::AssignOp::AddAssign && matches!(&a.left, js::AssignTarget::Simple(js::SimpleAssignTarget::Ident(i)) if i.id.sym.as_ref() == name) && matches!(&*a.right, js::Expr::Lit(js::Lit::Num(n)) if n.value == 1.0),
      _ => false,
    };
    if !steps {
      return Err(shape("that does not step by one with `i++`"));
    }
    let mut writes = Writes::default();
    f.body.visit_with(&mut writes);
    if writes.written.contains(&name) {
      return Err(shape(&format!("whose body writes `{name}`")));
    }
    struct Reads(Vec<String>);
    impl Visit for Reads {
      fn visit_ident(&mut self, id: &js::Ident) {
        self.0.push(id.sym.to_string());
      }
    }
    let mut reads = Reads(Vec::new());
    to.visit_with(&mut reads);
    if let Some(read) = reads.0.iter().find(|r| writes.written.contains(r)) {
      return Err(shape(&format!("whose bound reads `{read}`, which its body writes")));
    }
    Ok(Self { name, from: from.clone(), to: to.clone(), inclusive })
  }

  /// The items the loop runs over (`0..count`) with what the counter is for the item `k`.
  pub(crate) fn lowered(&self, lowerer: &mut Lowerer<'_>, item: &str) -> Lowered<(Expr, Expr)> {
    let from = lowerer.expr(&self.from)?;
    let to = lowerer.expr(&self.to)?;
    let mut count = Expr::Arith(snapfire_fsr_ir::ast::ArithOp::Sub, Box::new(to), Box::new(from.clone()));
    if self.inclusive {
      count = Expr::Arith(snapfire_fsr_ir::ast::ArithOp::Add, Box::new(count), Box::new(Expr::Lit(Lit::Float(1.0))));
    }
    let over = Expr::Builtin { name: Builtin::Range, args: vec![Expr::Builtin { name: Builtin::Ceil, args: vec![count] }] };
    let counter = Expr::Arith(snapfire_fsr_ir::ast::ArithOp::Add, Box::new(from), Box::new(Expr::Var(item.to_owned())));
    Ok((over, counter))
  }
}

fn rebind_counted(lowerer: &mut Lowerer<'_>, f: &js::ForStmt, writable: &Writable<'_>, assigned: &mut Vec<String>) -> Lowered<()> {
  let counted = Counted::of(lowerer, f)?;
  let names = LoopNames::over(f.span, &f.init, &f.body);
  let (over, counter) = counted.lowered(lowerer, &names.item)?;
  let name = counted.name.clone();
  fold(lowerer, f.span, &names, over, &f.body, |lowerer| {
    lowerer.scope.push((name, counter));
    Ok(())
  }, writable, assigned)
}

/// A loop as a fold: each outer local the body writes is the `reduce` over `over` of the body's writes, `bind` binding the item, starting from its value before the loop.
#[allow(clippy::too_many_arguments)]
fn fold(lowerer: &mut Lowerer<'_>, span: Span, names: &LoopNames, over: Expr, body: &js::Stmt, bind: impl FnOnce(&mut Lowerer<'_>) -> Lowered<()>, writable: &Writable<'_>, assigned: &mut Vec<String>) -> Lowered<()> {
  for name in &names.written {
    if !writable.local(lowerer, name) {
      return Err(lowerer.residue_with(span, format!("a write to `{name}` inside a loop, which this body did not declare"), "declare a local with `let` before the loop and write that"));
    }
  }
  let body = match body {
    js::Stmt::Block(block) => block.stmts.clone(),
    other => vec![other.clone()],
  };
  let body = iteration(lowerer, &body)?;
  if names.written.is_empty() {
    return Ok(());
  }
  let before: Vec<Expr> = names.written.iter().map(|n| current(lowerer, n)).collect();
  let depth = lowerer.scope.len();
  for name in &names.written {
    lowerer.scope.push((name.clone(), names.read(name)));
  }
  let mut inner = Vec::new();
  let run = bind(lowerer).and_then(|_| body.iter().try_for_each(|s| rebind_stmt(lowerer, s, writable, &mut inner)));
  let after: Vec<Expr> = names.written.iter().map(|n| current(lowerer, n)).collect();
  lowerer.scope.truncate(depth);
  run?;
  let single = names.written.len() == 1;
  let gather = |values: Vec<Expr>| match single {
    true => values.into_iter().next().expect("one name"),
    false => Expr::Object(names.written.iter().cloned().zip(values).map(|(n, v)| Entry::Field(n, v)).collect()),
  };
  let step = Expr::Lambda { params: vec![names.acc.clone(), names.item.clone()], body: Box::new(gather(after)) };
  let folded = Expr::Reduce(Box::new(over), Box::new(gather(before)), Box::new(step));
  for name in &names.written {
    let value = match single {
      true => folded.clone(),
      false => Expr::Field(Box::new(folded.clone()), name.clone()),
    };
    lowerer.scope.push((name.clone(), value));
    if !assigned.contains(name) {
      assigned.push(name.clone());
    }
  }
  Ok(())
}

/// One pass of a loop body with each `if (...) continue;` turned into a branch around the rest; a `break`, a `return` or a `continue` anywhere else is residue.
fn iteration(lowerer: &Lowerer<'_>, stmts: &[js::Stmt]) -> Lowered<Vec<js::Stmt>> {
  let mut out = Vec::new();
  for (at, stmt) in stmts.iter().enumerate() {
    match stmt {
      js::Stmt::Continue(c) if c.label.is_none() => return Ok(out),
      js::Stmt::If(branch) if branch.alt.is_none() && is_continue(&branch.cons) => {
        let rest = iteration(lowerer, &stmts[at + 1..])?;
        let test = js::Expr::Unary(js::UnaryExpr { span: branch.test.span(), op: js::UnaryOp::Bang, arg: branch.test.clone() });
        out.push(js::Stmt::If(js::IfStmt { span: branch.span, test: Box::new(test), cons: Box::new(js::Stmt::Block(js::BlockStmt { span: branch.span, ctxt: Default::default(), stmts: rest })), alt: None }));
        return Ok(out);
      }
      other => {
        if let Some((span, what)) = leaves_loop(other) {
          return Err(lowerer.residue_with(span, format!("{what} inside a loop"), "a loop the build reads runs over every item; filter the items first or put `if (...) continue;` at the top of the loop"));
        }
        out.push(other.clone());
      }
    }
  }
  Ok(out)
}

fn is_continue(stmt: &js::Stmt) -> bool {
  match stmt {
    js::Stmt::Continue(c) => c.label.is_none(),
    js::Stmt::Block(block) => matches!(block.stmts.as_slice(), [js::Stmt::Continue(c)] if c.label.is_none()),
    _ => false,
  }
}

/// The first `break`, `continue` or `return` in `stmt` that leaves the enclosing loop's iteration, outside a nested function, loop or (for `break`) `switch`.
fn leaves_loop(stmt: &js::Stmt) -> Option<(Span, &'static str)> {
  struct Leaves {
    found: Option<(Span, &'static str)>,
    switches: usize,
  }
  impl Visit for Leaves {
    fn visit_break_stmt(&mut self, b: &js::BreakStmt) {
      if self.switches == 0 || b.label.is_some() {
        self.found.get_or_insert((b.span, "`break`"));
      }
    }
    fn visit_continue_stmt(&mut self, c: &js::ContinueStmt) {
      self.found.get_or_insert((c.span, "`continue`"));
    }
    fn visit_return_stmt(&mut self, r: &js::ReturnStmt) {
      self.found.get_or_insert((r.span, "`return`"));
    }
    fn visit_switch_stmt(&mut self, sw: &js::SwitchStmt) {
      self.switches += 1;
      sw.visit_children_with(self);
      self.switches -= 1;
    }
    fn visit_for_of_stmt(&mut self, _: &js::ForOfStmt) {}
    fn visit_function(&mut self, _: &js::Function) {}
    fn visit_arrow_expr(&mut self, _: &js::ArrowExpr) {}
  }
  let mut leaves = Leaves { found: None, switches: 0 };
  stmt.visit_with(&mut leaves);
  leaves.found
}

/// Arms tried in order, each a condition and its statements, then the statements when none holds: each name any arm wrote is rebound to the value of the arm that ran.
fn rebind_arms(lowerer: &mut Lowerer<'_>, arms: Vec<(Option<Expr>, &[js::Stmt])>, otherwise: &[js::Stmt], writable: &Writable<'_>, assigned: &mut Vec<String>) -> Lowered<()> {
  let run = |lowerer: &mut Lowerer<'_>, stmts: &[js::Stmt]| -> Lowered<Vec<(String, Expr)>> {
    let depth = lowerer.scope.len();
    let mut inner = Vec::new();
    let result = stmts.iter().try_for_each(|s| rebind_stmt(lowerer, s, writable, &mut inner));
    let values = inner.iter().map(|n| (n.clone(), current(lowerer, n))).collect();
    lowerer.scope.truncate(depth);
    result.map(|_| values)
  };
  let mut written: Vec<Vec<(String, Expr)>> = Vec::with_capacity(arms.len());
  let mut conds = Vec::with_capacity(arms.len());
  for (cond, stmts) in &arms {
    written.push(run(lowerer, stmts)?);
    conds.push(cond.clone().unwrap_or(Expr::Lit(Lit::Bool(false))));
  }
  let fallback = run(lowerer, otherwise)?;
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

fn prop_name_of(member: &js::MemberExpr) -> Option<String> {
  match &member.prop {
    js::MemberProp::Ident(i) => Some(i.sym.to_string()),
    js::MemberProp::Computed(c) => match &*c.expr {
      js::Expr::Lit(js::Lit::Str(s)) => Some(s.value.to_atom_lossy().to_string()),
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
}

impl HandlerWalk {
  pub(crate) fn set(&mut self, state: String, value: Expr) {
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

pub(crate) fn holds_act(stmts: &[Stmt]) -> bool {
  stmts.iter().any(|stmt| match stmt {
    Stmt::Act { .. } => true,
    Stmt::If { then, r#else, .. } => holds_act(then) || holds_act(r#else),
    _ => false,
  })
}

pub(crate) fn block_to_expr(lowerer: &mut Lowerer<'_>, stmts: &[js::Stmt]) -> Lowered<Expr> {
  let depth = lowerer.scope.len();
  block_to_expr_from(lowerer, stmts, depth)
}

/// `block_to_expr` with the names bound at or above `floor` writable, a function's parameters among them.
pub(crate) fn block_to_expr_from(lowerer: &mut Lowerer<'_>, stmts: &[js::Stmt], floor: usize) -> Lowered<Expr> {
  let depth = lowerer.scope.len();
  let refs: Vec<&js::Stmt> = stmts.iter().collect();
  let result = block_to_expr_inner(lowerer, &refs, floor);
  lowerer.scope.truncate(depth);
  result
}

/// Statements as the value each path returns: a branch with a `return` in it is a ternary over the paths through it and what follows, a write to a local rebinds it, a path that ends without a `return` is `null`.
fn block_to_expr_inner(lowerer: &mut Lowerer<'_>, stmts: &[&js::Stmt], floor: usize) -> Lowered<Expr> {
  let Some((first, rest)) = stmts.split_first() else {
    return Ok(Expr::Lit(Lit::Null));
  };
  let joined = |head: &js::Stmt, rest: &[&js::Stmt]| -> Vec<js::Stmt> {
    let mut out: Vec<js::Stmt> = match head {
      js::Stmt::Block(b) => b.stmts.clone(),
      other => vec![other.clone()],
    };
    out.extend(rest.iter().map(|s| (*s).clone()));
    out
  };
  let path = |lowerer: &mut Lowerer<'_>, stmts: Vec<js::Stmt>| -> Lowered<Expr> {
    let depth = lowerer.scope.len();
    let refs: Vec<&js::Stmt> = stmts.iter().collect();
    let value = block_to_expr_inner(lowerer, &refs, floor);
    lowerer.scope.truncate(depth);
    value
  };
  match first {
    js::Stmt::Decl(js::Decl::Var(var)) => {
      for decl in &var.decls {
        let expr = match decl.init.as_deref() {
          Some(init) => lowerer.expr(init)?,
          None => Expr::Lit(Lit::Null),
        };
        bind_pattern(lowerer, &decl.name, expr)?;
        if let (js::Pat::Ident(id), Some(init)) = (&decl.name, decl.init.as_deref()) {
          lowerer.note_kind(id.id.sym.as_ref(), init);
        }
      }
      block_to_expr_inner(lowerer, rest, floor)
    }
    js::Stmt::Return(ret) => match &ret.arg {
      Some(arg) => lowerer.expr(arg),
      None => Ok(Expr::Lit(Lit::Null)),
    },
    js::Stmt::Throw(thrown) => throw_expr(lowerer, thrown),
    js::Stmt::Empty(_) => block_to_expr_inner(lowerer, rest, floor),
    js::Stmt::Block(_) if holds_return(first) => path(lowerer, joined(first, rest)),
    js::Stmt::If(if_stmt) if holds_return(first) => {
      let cond = lowerer.expr(&if_stmt.test)?;
      let then = path(lowerer, joined(&if_stmt.cons, rest))?;
      let otherwise = match &if_stmt.alt {
        Some(alt) => path(lowerer, joined(alt, rest))?,
        None => path(lowerer, rest.iter().map(|s| (*s).clone()).collect())?,
      };
      Ok(Expr::Ternary(Box::new(cond), Box::new(then), Box::new(otherwise)))
    }
    js::Stmt::Switch(sw) if holds_return(first) => {
      let disc = lowerer.expr(&sw.discriminant)?;
      let (arms, default) = switch_arms(lowerer, sw)?;
      let mut fallback: Vec<js::Stmt> = default.map(|d| d.to_vec()).unwrap_or_default();
      fallback.extend(rest.iter().map(|s| (*s).clone()));
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
        let mut stmts = body.to_vec();
        stmts.extend(rest.iter().map(|s| (*s).clone()));
        let then = path(lowerer, stmts)?;
        value = Expr::Ternary(Box::new(cond.unwrap_or(Expr::Lit(Lit::Bool(false)))), Box::new(then), Box::new(value));
      }
      Ok(value)
    }
    js::Stmt::For(_) | js::Stmt::ForOf(_) if returns_from_loop(first) => path(lowerer, loop_return(first, rest)),
    straight => {
      let writable = Writable { floor, frozen: &[], setters: &[] };
      rebind_stmt(lowerer, straight, &writable, &mut Vec::new())?;
      block_to_expr_inner(lowerer, rest, floor)
    }
  }
}

/// `console.log(...)`, `info`, `debug`, `warn` or `error` on the global `console` as the log it writes on the server, as a server renderer running the component writes it, or as nothing when an argument is one only the browser can compute.
pub(crate) fn console_call(lowerer: &mut Lowerer<'_>, call: &js::CallExpr) -> Lowered<Option<Expr>> {
  let js::Callee::Expr(callee) = &call.callee else { return Ok(None) };
  let js::Expr::Member(m) = crate::meaning::bare(callee) else { return Ok(None) };
  let js::Expr::Ident(console) = crate::meaning::bare(&m.obj) else { return Ok(None) };
  let global = lowerer.meanings.as_ref().is_none_or(|meanings| console.ctxt == meanings.unresolved);
  let method = prop_name_of(m).unwrap_or_default();
  if !global || console.sym.as_ref() != "console" || !matches!(method.as_str(), "log" | "info" | "debug" | "warn" | "error") {
    return Ok(None);
  }
  let mut args = vec![Expr::lit_str(method)];
  for arg in &call.args {
    if arg.spread.is_some() {
      return Err(lowerer.residue(arg.expr.span(), "a spread into `console`"));
    }
    match lowerer.expr(&arg.expr) {
      Ok(value) => args.push(value),
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
  Ok(Some(Expr::Builtin { name: Builtin::Log, args }))
}

/// `xs.forEach((x, i) => body)` as `for (let i = 0; i < xs.length; i++) { const x = xs[i]; body }`, a bare `return` in the callback a `continue`; `xs` a name or a chain of fields read off one, so reading it each pass reads the same array.
fn for_each_loop(call: &js::CallExpr, meanings: Option<&crate::meaning::Meanings>) -> Option<js::Stmt> {
  let js::Callee::Expr(callee) = &call.callee else { return None };
  let js::Expr::Member(m) = crate::meaning::bare(callee) else { return None };
  if prop_name_of(m).as_deref() != Some("forEach") {
    return None;
  }
  fn steady(expr: &js::Expr) -> bool {
    match crate::meaning::bare(expr) {
      js::Expr::Ident(_) => true,
      js::Expr::Member(m) => matches!(m.prop, js::MemberProp::Ident(_)) && steady(&m.obj),
      _ => false,
    }
  }
  // `Children.forEach(children, f)` walks the values the placement passed, which `ComponentLowerer::premake_for_each` binds.
  let children_of = meanings.and_then(|meanings| meanings.of_expr(&m.obj)) == Some(crate::meaning::Meaning::Library(crate::meaning::Construct::Children)) && call.args.len() == 2;
  let (over, callback): (Box<js::Expr>, &js::ExprOrSpread) = if children_of {
    (Box::new(js::Expr::Ident(js::Ident::new_no_ctxt(snapfire_fsr_ir::render::CHILD_VALUES.into(), call.span))), &call.args[1])
  } else if call.args.len() == 1 && steady(&m.obj) {
    (m.obj.clone(), &call.args[0])
  } else {
    return None;
  };
  let (params, stmts): (Vec<js::Pat>, Vec<js::Stmt>) = match crate::meaning::bare(&callback.expr) {
    js::Expr::Arrow(arrow) => (
      arrow.params.clone(),
      match &*arrow.body {
        js::ArrowFunctionBody::FunctionBody(b) => b.stmts.clone(),
        js::ArrowFunctionBody::Expr(e) => vec![js::Stmt::Expr(js::ExprStmt { span: e.span(), expr: e.clone() })],
      },
    ),
    js::Expr::Fn(f) => (patterns(&f.function), f.function.body.as_ref()?.stmts.clone()),
    _ => return None,
  };
  let span = call.span;
  let ident = |name: &str| js::Ident::new_no_ctxt(name.into(), span);
  let counter = match params.get(1) {
    Some(js::Pat::Ident(id)) => id.id.sym.to_string(),
    None => format!("$each{}", span.lo.0),
    Some(_) => return None,
  };
  fn continued(stmts: Vec<js::Stmt>) -> Vec<js::Stmt> {
    stmts
      .into_iter()
      .map(|stmt| match stmt {
        js::Stmt::Return(ret) if ret.arg.is_none() => js::Stmt::Continue(js::ContinueStmt { span: ret.span, label: None }),
        js::Stmt::If(branch) => {
          let arm = |s: Box<js::Stmt>| -> Box<js::Stmt> {
            match *s {
              js::Stmt::Block(b) => Box::new(js::Stmt::Block(js::BlockStmt { stmts: continued(b.stmts), ..b })),
              other => Box::new(continued(vec![other]).remove(0)),
            }
          };
          js::Stmt::If(js::IfStmt { cons: arm(branch.cons), alt: branch.alt.map(arm), ..branch })
        }
        js::Stmt::Block(b) => js::Stmt::Block(js::BlockStmt { stmts: continued(b.stmts), ..b }),
        other => other,
      })
      .collect()
  }
  let mut body = Vec::new();
  if let Some(item) = params.first() {
    let read = js::Expr::Member(js::MemberExpr { span, obj: over.clone(), prop: js::MemberProp::Computed(js::ComputedPropName { span, expr: Box::new(js::Expr::Ident(ident(&counter))) }) });
    body.push(js::Stmt::Decl(js::Decl::Var(Box::new(js::VarDecl { span, ctxt: Default::default(), kind: js::VarDeclKind::Const, declare: false, decls: vec![js::VarDeclarator { span, name: item.clone(), init: Some(Box::new(read)), definite: false }] }))));
  }
  body.extend(continued(stmts));
  let length = js::Expr::Member(js::MemberExpr { span, obj: over.clone(), prop: js::MemberProp::Ident(js::IdentName::new("length".into(), span)) });
  Some(js::Stmt::For(js::ForStmt {
    span,
    init: Some(js::VarDeclOrExpr::VarDecl(Box::new(js::VarDecl { span, ctxt: Default::default(), kind: js::VarDeclKind::Let, declare: false, decls: vec![js::VarDeclarator { span, name: js::Pat::Ident(ident(&counter).into()), init: Some(Box::new(js::Expr::Lit(js::Lit::Num(js::Number { span, value: 0.0, raw: None })))), definite: false }] }))),
    test: Some(Box::new(js::Expr::Bin(js::BinExpr { span, op: js::BinaryOp::Lt, left: Box::new(js::Expr::Ident(ident(&counter))), right: Box::new(length) }))),
    update: Some(Box::new(js::Expr::Update(js::UpdateExpr { span, op: js::UpdateOp::PlusPlus, prefix: false, arg: Box::new(js::Expr::Ident(ident(&counter))) }))),
    body: Box::new(js::Stmt::Block(js::BlockStmt { span, ctxt: Default::default(), stmts: body })),
  }))
}

/// Whether a loop's body returns from the function, outside a nested loop or function.
fn returns_from_loop(stmt: &js::Stmt) -> bool {
  let body = match stmt {
    js::Stmt::For(f) => &f.body,
    js::Stmt::ForOf(f) => &f.body,
    _ => return false,
  };
  match &**body {
    js::Stmt::Block(block) => block.stmts.iter().any(holds_return),
    other => holds_return(other),
  }
}

/// A loop that returns as one that does not: `returned` and `value` declared before it, each `return e` in its body a write of both, the rest of an iteration run only while nothing returned, then `if (returned) return value;` ahead of `rest`.
fn loop_return(looped: &js::Stmt, rest: &[&js::Stmt]) -> Vec<js::Stmt> {
  let span = looped.span();
  let returned = format!("$returned{}", span.lo.0);
  let value = format!("$value{}", span.lo.0);
  let ident = |name: &str| js::Ident::new_no_ctxt(name.into(), span);
  let read = |name: &str| Box::new(js::Expr::Ident(ident(name)));
  let declare = |name: &str, init: js::Expr| {
    js::Stmt::Decl(js::Decl::Var(Box::new(js::VarDecl {
      span,
      ctxt: Default::default(),
      kind: js::VarDeclKind::Let,
      declare: false,
      decls: vec![js::VarDeclarator { span, name: js::Pat::Ident(ident(name).into()), init: Some(Box::new(init)), definite: false }],
    })))
  };
  let set = |name: &str, to: Box<js::Expr>| {
    js::Stmt::Expr(js::ExprStmt { span, expr: Box::new(js::Expr::Assign(js::AssignExpr { span, op: js::AssignOp::Assign, left: js::AssignTarget::Simple(js::SimpleAssignTarget::Ident(ident(name).into())), right: to })) })
  };
  let flag = |on: bool| Box::new(js::Expr::Lit(js::Lit::Bool(js::Bool { span, value: on })));
  let block = |stmts: Vec<js::Stmt>| js::Stmt::Block(js::BlockStmt { span, ctxt: Default::default(), stmts });
  fn rewrite(stmts: &[js::Stmt], on_return: &dyn Fn(Option<Box<js::Expr>>) -> Vec<js::Stmt>, unless: &dyn Fn(Vec<js::Stmt>) -> js::Stmt) -> Vec<js::Stmt> {
    let mut out = Vec::new();
    for (at, stmt) in stmts.iter().enumerate() {
      match stmt {
        js::Stmt::Return(ret) => {
          out.extend(on_return(ret.arg.clone()));
          return out;
        }
        js::Stmt::If(branch) if holds_return(stmt) => {
          let arm = |s: &js::Stmt| -> Box<js::Stmt> {
            let stmts = match s {
              js::Stmt::Block(b) => b.stmts.clone(),
              other => vec![other.clone()],
            };
            Box::new(js::Stmt::Block(js::BlockStmt { span: s.span(), ctxt: Default::default(), stmts: rewrite(&stmts, on_return, unless) }))
          };
          out.push(js::Stmt::If(js::IfStmt { span: branch.span, test: branch.test.clone(), cons: arm(&branch.cons), alt: branch.alt.as_deref().map(arm) }));
        }
        js::Stmt::Block(b) if holds_return(stmt) => out.push(js::Stmt::Block(js::BlockStmt { span: b.span, ctxt: b.ctxt, stmts: rewrite(&b.stmts, on_return, unless) })),
        other => {
          out.push(other.clone());
          continue;
        }
      }
      let after = rewrite(&stmts[at + 1..], on_return, unless);
      if !after.is_empty() {
        out.push(unless(after));
      }
      return out;
    }
    out
  }
  let on_return = |arg: Option<Box<js::Expr>>| vec![set(&value, arg.unwrap_or_else(|| read("undefined"))), set(&returned, flag(true))];
  let unless = |stmts: Vec<js::Stmt>| js::Stmt::If(js::IfStmt { span, test: Box::new(js::Expr::Unary(js::UnaryExpr { span, op: js::UnaryOp::Bang, arg: read(&returned) })), cons: Box::new(block(stmts)), alt: None });
  let guard = |body: &js::Stmt| -> Box<js::Stmt> {
    let stmts = match body {
      js::Stmt::Block(b) => b.stmts.clone(),
      other => vec![other.clone()],
    };
    Box::new(block(vec![unless(rewrite(&stmts, &on_return, &unless))]))
  };
  let looped = match looped {
    js::Stmt::For(f) => js::Stmt::For(js::ForStmt { body: guard(&f.body), ..f.clone() }),
    js::Stmt::ForOf(f) => js::Stmt::ForOf(js::ForOfStmt { body: guard(&f.body), ..f.clone() }),
    other => other.clone(),
  };
  let mut out = vec![declare(&returned, *flag(false)), declare(&value, *read("undefined")), looped];
  out.push(js::Stmt::If(js::IfStmt { span, test: read(&returned), cons: Box::new(js::Stmt::Return(js::ReturnStmt { span, arg: Some(read(&value)) })), alt: None }));
  out.extend(rest.iter().map(|s| (*s).clone()));
  out
}

/// The `return` an `if` with no `else` holds as its only statement, braced or bare.
fn early_return(branch: &js::IfStmt) -> Option<&js::ReturnStmt> {
  if branch.alt.is_some() {
    return None;
  }
  match &*branch.cons {
    js::Stmt::Return(ret) => Some(ret),
    js::Stmt::Block(block) => match block.stmts.as_slice() {
      [js::Stmt::Return(ret)] => Some(ret),
      _ => None,
    },
    _ => None,
  }
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
  fn custom_hook(&self, name: &str) -> bool {
    use crate::meaning::{Construct, Meaning, LIBRARY};
    match self.meanings().bindings.get(name) {
      Some(Meaning::Own { .. } | Meaning::Foreign) => true,
      Some(_) => false,
      None => !LIBRARY.iter().any(|(_, _, construct)| matches!(construct, Construct::Hook(hook) if *hook == name)),
    }
  }

  /// A custom hook's call inlined: its body binds under a prefix of its own so two calls hold two sets of state. What it returns binds at the call site. A returned setter or function is a handler there, run by the browser.
  fn inline_hook(&mut self, hook: &str, call: &'p js::CallExpr, pattern: Option<&'p js::Pat>) -> Lowered<Option<Stmt>> {
    let Some((_, parsed, export)) = self.hooks.iter().find(|(n, _, _)| n == hook).cloned() else {
      let origin = match &call.callee {
        js::Callee::Expr(callee) => self.meanings().of_expr(callee).and_then(|meant| meant.origin().map(|(f, n)| (f.to_owned(), n.to_owned()))),
        _ => None,
      };
      self.wanted_hook = Some((hook.to_owned(), origin));
      return Err(self.lowerer.residue(call.span, format!("`{hook}`, a hook the build cannot follow")));
    };
    let Some(found) = find_function(parsed, &export) else { return Err(self.lowerer.residue(call.span, format!("`{hook}` is not a function the build can read"))) };
    let (params, body): (Vec<js::Pat>, FunctionBody<'p>) = match found {
      Found::Declared(params, stmts, _) => (params, FunctionBody::Block(stmts)),
      Found::Arrow(arrow) => (arrow.params.clone(), arrow_body(arrow)),
    };
    let mut args = Vec::with_capacity(call.args.len());
    for arg in &call.args {
      match self.lowerer.expr(&arg.expr) {
        Ok(value) => args.push(value),
        // A callback that writes state or calls out is one only the browser runs, as an effect or a listener does.
        Err(_) if matches!(crate::meaning::bare(&arg.expr), js::Expr::Arrow(_) | js::Expr::Fn(_)) && !self.unbound_is_followable() => {
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
    self.lowerer.parsed = parsed;
    let outer_meanings = std::mem::replace(&mut self.lowerer.meanings, parsed.meanings.get().cloned());
    let returned = self.hook_body(&prefix, &params, body, args);
    self.lowerer.parsed = outer;
    self.lowerer.meanings = outer_meanings;
    self.lowerer.hoisting = hoisting;
    self.lowerer.scope.truncate(depth);
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

  fn hook_body(&mut self, prefix: &str, params: &[js::Pat], body: FunctionBody<'p>, args: Vec<Expr>) -> Lowered<Returned> {
    for (i, param) in params.iter().enumerate() {
      let arg = args.get(i).cloned().unwrap_or(Expr::Lit(Lit::Null));
      bind_pattern(&mut self.lowerer, param, arg)?;
    }
    let stmts = match body {
      FunctionBody::Expr(e) => return self.returned(e, &[]),
      FunctionBody::Block(stmts) => stmts,
    };
    let floor = self.lowerer.scope.len();
    let mut renames: Vec<(String, String)> = Vec::new();
    for stmt in stmts {
      match stmt {
        js::Stmt::Decl(js::Decl::Var(var)) => {
          for decl in &var.decls {
            self.hook_let(decl, prefix, &mut renames)?;
          }
        }
        js::Stmt::Decl(js::Decl::Fn(f)) => {
          let bound = format!("{prefix}{}", f.ident.sym);
          if let Some(body) = &f.function.body {
            self.handler_fns.insert(bound.clone(), (patterns(&f.function), FunctionBody::Block(&body.stmts)));
          }
          self.lowerer.browser_functions.push(f.ident.sym.to_string());
          self.browser_function(bound.clone());
          renames.push((f.ident.sym.to_string(), bound));
        }
        js::Stmt::Expr(e) if hook_call(&self.meanings(), &e.expr).is_some_and(|(name, _)| EFFECT_HOOKS.contains(&name)) => {}
        js::Stmt::Expr(e) if hook_call(&self.meanings(), &e.expr).is_some_and(|(name, _)| self.custom_hook(name)) => {
          let (hook, call) = hook_call(&self.meanings(), &e.expr).expect("checked");
          self.inline_hook(hook, call, None)?;
        }
        js::Stmt::Return(ret) => {
          return match ret.arg.as_deref() {
            Some(arg) => self.returned(arg, &renames),
            None => Ok(Returned::Value(Expr::Lit(Lit::Null))),
          };
        }
        js::Stmt::If(_) | js::Stmt::Switch(_) | js::Stmt::Block(_) if holds_return(stmt) => {
          return Err(self.lowerer.residue(stmt.span(), "a `return` inside a branch of a custom hook"));
        }
        js::Stmt::Expr(_) | js::Stmt::If(_) | js::Stmt::Switch(_) | js::Stmt::Block(_) => {
          let mut held = std::mem::take(&mut self.pending_lets);
          let result = self.rebound(stmt, floor, &[], &mut held, &|_, expr| expr);
          self.pending_lets = held;
          result?;
        }
        other => return Err(self.lowerer.residue(other.span(), "a statement a custom hook cannot hold; it holds hooks, `const`s, `let`s, functions and a `return`")),
      }
    }
    Ok(Returned::Value(Expr::Lit(Lit::Null)))
  }

  /// One declaration inside a hook: lowered as the component's are, then every name it bound renamed under the call's prefix.
  fn hook_let(&mut self, decl: &'p js::VarDeclarator, prefix: &str, renames: &mut Vec<(String, String)>) -> Lowered<()> {
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
  fn returned(&mut self, e: &'p js::Expr, renames: &[(String, String)]) -> Lowered<Returned> {
    let handler = |this: &Self, e: &js::Expr| -> Option<String> {
      let js::Expr::Ident(id) = e else { return None };
      let bound = renames.iter().rev().find(|(from, _)| from == id.sym.as_ref()).map(|(_, to)| to.clone())?;
      this.handlers.contains(&bound).then_some(bound)
    };
    let part = |this: &mut Self, e: &'p js::Expr| -> Lowered<Part> {
      if let js::Expr::Arrow(arrow) = e {
        this.hook_calls += 1;
        let bound = format!("$hook{}", this.hook_calls);
        this.handler_fns.insert(bound.clone(), (arrow.params.clone(), arrow_body(arrow)));
        this.browser_function(bound.clone());
        return Ok(Part::Handler(bound));
      }
      Ok(match handler(this, e) {
        Some(bound) => Part::Handler(bound),
        None => Part::Value(this.lowerer.expr(e)?),
      })
    };
    match e {
      js::Expr::Paren(p) => self.returned(&p.expr, renames),
      js::Expr::TsAs(a) => self.returned(&a.expr, renames),
      js::Expr::TsConstAssertion(a) => self.returned(&a.expr, renames),
      js::Expr::TsSatisfies(a) => self.returned(&a.expr, renames),
      js::Expr::Object(obj) => {
        let mut parts = Vec::with_capacity(obj.props.len());
        for prop in &obj.props {
          let js::PropOrSpread::Prop(prop) = prop else { return Err(self.lowerer.residue(obj.span, "a spread in what a custom hook returns")) };
          match &**prop {
            js::Prop::Shorthand(id) => {
              let ident = js::Expr::Ident(id.clone());
              let p = match handler(self, &ident) {
                Some(bound) => Part::Handler(bound),
                None => Part::Value(self.lowerer.expr(&ident)?),
              };
              parts.push((id.sym.to_string(), p));
            }
            js::Prop::KeyValue(kv) => {
              let key = prop_name(&kv.key).ok_or_else(|| self.lowerer.residue(kv.key.span(), "a computed key in what a custom hook returns"))?;
              parts.push((key, part(self, &kv.value)?));
            }
            other => return Err(self.lowerer.residue(other.span(), "a method in what a custom hook returns")),
          }
        }
        Ok(Returned::Object(parts))
      }
      js::Expr::Array(arr) => {
        let mut parts = Vec::with_capacity(arr.elems.len());
        for elem in &arr.elems {
          match elem {
            None => parts.push(None),
            Some(e) if e.spread.is_some() => return Err(self.lowerer.residue(e.expr.span(), "a spread in what a custom hook returns")),
            Some(e) => parts.push(Some(part(self, &e.expr)?)),
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

  fn bind_value(&mut self, pattern: &js::Pat, value: Expr) -> Lowered<()> {
    let name = self.lowerer.temp();
    self.pending_lets.push(Stmt::Let { name: name.clone(), expr: value });
    bind_pattern(&mut self.lowerer, pattern, Expr::Var(name))
  }

  fn bind_returned(&mut self, pattern: &'p js::Pat, returned: Returned, prefix: &str) -> Lowered<()> {
    let part_into = |this: &mut Self, pattern: &js::Pat, part: Option<Part>| -> Lowered<()> {
      match part {
        Some(Part::Handler(bound)) => match pattern {
          js::Pat::Ident(id) => {
            this.alias_handler(id.id.sym.as_ref(), &bound);
            Ok(())
          }
          other => Err(this.lowerer.residue(other.span(), "a function a custom hook returns bound to a pattern")),
        },
        Some(Part::Value(value)) => this.bind_value(pattern, value),
        None => this.bind_value(pattern, Expr::Lit(Lit::Null)),
      }
    };
    match (pattern, returned) {
      (js::Pat::Ident(id), Returned::Handler(bound)) => {
        self.alias_handler(id.id.sym.as_ref(), &bound);
        Ok(())
      }
      (js::Pat::Ident(id), Returned::Value(value)) => {
        let name = format!("{prefix}{}", id.id.sym);
        self.pending_lets.push(Stmt::Let { name: name.clone(), expr: value });
        self.lowerer.scope.push((id.id.sym.to_string(), Expr::Var(name)));
        Ok(())
      }
      (js::Pat::Object(obj), Returned::Object(mut parts)) => {
        for prop in &obj.props {
          match prop {
            js::ObjectPatProp::Assign(a) => {
              let key = a.key.id.sym.to_string();
              let part = parts.iter().position(|(k, _)| *k == key).map(|at| parts.remove(at).1);
              let target = js::Pat::Ident(a.key.clone());
              match (part, &a.value) {
                (Some(Part::Value(value)), Some(default)) => {
                  let default = self.lowerer.expr(default)?;
                  self.bind_value(&target, Expr::Coalesce(Box::new(value), Box::new(default)))?;
                }
                (part, _) => part_into(self, &target, part)?,
              }
            }
            js::ObjectPatProp::KeyValue(kv) => {
              let key = prop_name(&kv.key).ok_or_else(|| self.lowerer.residue(kv.key.span(), "a computed field in a pattern"))?;
              let part = parts.iter().position(|(k, _)| *k == key).map(|at| parts.remove(at).1);
              part_into(self, &kv.value, part)?;
            }
            js::ObjectPatProp::Rest(r) => return Err(self.lowerer.residue(r.span, "a rest of what a custom hook returns")),
          }
        }
        Ok(())
      }
      (js::Pat::Array(arr), Returned::Array(mut parts)) => {
        for (i, elem) in arr.elems.iter().enumerate() {
          let part = if i < parts.len() { parts[i].take() } else { None };
          match elem {
            None => {}
            Some(js::Pat::Rest(r)) => return Err(self.lowerer.residue(r.span, "a rest of what a custom hook returns")),
            Some(pattern) => part_into(self, pattern, part)?,
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
  /// The local name `children` was destructured to.
  children_name: Option<String>,
  /// Whether the component is wrapped in `forwardRef`, so its second parameter is the `ref` prop.
  forwards_ref: bool,
  /// The class whose `render()` this is, which binds `this`.
  class: Option<&'p js::Class>,
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
  handler_fns: HashMap<String, (Vec<js::Pat>, FunctionBody<'p>)>,
  /// The handlers lowered so far; an element's `$on:` marker holds an index into it.
  lowered_handlers: Vec<Handler>,
  /// Custom element tags with a shadow template, to the template's module.
  elements: Rc<HashMap<String, String>>,
  /// Custom hooks resolved so far: the name a call uses, the module that declares it and its export there.
  hooks: Vec<(String, &'p Parsed, String)>,
  /// A custom hook the body called that the set has not read yet, with where its meaning says it is declared.
  wanted_hook: Option<(String, Option<(String, String)>)>,
  /// Lets an inlined custom hook bound, which the body places before the declaration that called it.
  pending_lets: Vec<Stmt>,
  /// How many custom hook calls have been inlined, which names each call's bindings apart.
  hook_calls: usize,
  /// The file a name the lowerer could not bind was met in, when that is a hook's rather than the component's.
  unbound_in: Option<String>,
  /// The functions returning markup being inlined, innermost last, so one that calls itself is refused rather than unrolled for ever.
  inlining: Vec<String>,
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

  /// The slot an expression stands for: `children` or `props.children` is
  /// `content`; in a layout, a prop named after one of its slots is that slot.
  fn slot_of_expr(&self, expr: &js::Expr) -> Option<String> {
    match expr {
      js::Expr::Ident(id) => {
        let name = id.sym.as_ref();
        if self.children_name.as_deref() == Some(name) {
          return Some("content".to_owned());
        }
        self.slot_props.iter().find(|(local, _)| local == name).map(|(_, slot)| slot.clone())
      }
      js::Expr::Member(m) => {
        let js::Expr::Ident(obj) = &*m.obj else { return None };
        let js::MemberProp::Ident(prop) = &m.prop else { return None };
        if self.props_name.as_deref() != Some(obj.sym.as_ref()) {
          return None;
        }
        let prop = prop.sym.as_ref();
        if prop == "children" {
          return Some("content".to_owned());
        }
        (self.layout_root && self.slot_names.iter().any(|n| n == prop)).then(|| prop.to_owned())
      }
      _ => None,
    }
  }
}

const EFFECT_HOOKS: &[&str] = &["useEffect", "useLayoutEffect", "useInsertionEffect", "useDebugValue", "useImperativeHandle"];

/// The hook a call names when its callee is a bare identifier.
/// The hook `expr` calls: a library's by the library's own name, however the
/// file reaches it, and an application's by the name the file calls it. `None`
/// for a call that is no hook, a local that shadows a hook's name among them.
fn hook_call<'a>(meanings: &crate::meaning::Meanings, expr: &'a js::Expr) -> Option<(&'a str, &'a js::CallExpr)> {
  let js::Expr::Call(call) = expr else { return None };
  let js::Callee::Expr(callee) = &call.callee else { return None };
  let meant = meanings.of_expr(callee);
  if let Some(hook) = meant.as_ref().and_then(|meant| meant.hook()) {
    return Some((hook, call));
  }
  let declared_hook = meant.as_ref().and_then(|meant| meant.origin()).is_some_and(|(_, name)| name.starts_with("use"));
  let name = match &**callee {
    js::Expr::Ident(id) if id.ctxt == meanings.top || id.ctxt == meanings.unresolved => id.sym.as_ref(),
    js::Expr::Member(member) => {
      let js::Expr::Ident(ns) = &*member.obj else { return None };
      let js::MemberProp::Ident(name) = &member.prop else { return None };
      if !matches!(meanings.of_ident(ns), Some(crate::meaning::Meaning::Namespace(_))) {
        return None;
      }
      name.sym.as_ref()
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
  fn premake_expr(&mut self, e: &'p js::Expr) -> Lowered<()> {
    if let Some(kind) = self.counts_own_children(e) {
      let span = e.span();
      self.lowerer.premade.insert((span.lo.0, span.hi.0), Expr::Var(kind.to_owned()));
      if !self.counts_children.contains(&kind) {
        self.counts_children.push(kind);
      }
      return Ok(());
    }
    self.premake_children(e)?;
    if let js::Expr::Array(a) = e {
      for elem in a.elems.iter().flatten() {
        self.premake_expr(&elem.expr)?;
      }
      return Ok(());
    }
    if self.holds_markup(e) {
      let tmpl = self.child_expr(e)?;
      let span = e.span();
      self.lowerer.premade.insert((span.lo.0, span.hi.0), Expr::Markup(std::sync::Arc::new(tmpl)));
      return Ok(());
    }
    match e {
      js::Expr::Paren(p) => self.premake_expr(&p.expr),
      js::Expr::Object(o) => {
        for prop in &o.props {
          if let js::PropOrSpread::Prop(prop) = prop {
            if let js::Prop::KeyValue(kv) = &**prop {
              self.premake_expr(&kv.value)?;
            }
          }
        }
        Ok(())
      }
      js::Expr::Array(a) => {
        for elem in a.elems.iter().flatten() {
          self.premake_expr(&elem.expr)?;
        }
        Ok(())
      }
      js::Expr::Cond(c) => {
        self.premake_expr(&c.cons)?;
        self.premake_expr(&c.alt)
      }
      js::Expr::Bin(b) => {
        self.premake_expr(&b.left)?;
        self.premake_expr(&b.right)
      }
      js::Expr::Assign(a) => self.premake_expr(&a.right),
      js::Expr::Arrow(arrow) => {
        let depth = self.lowerer.scope.len();
        let result = bind_params(&mut self.lowerer, &arrow.params).and_then(|_| match &*arrow.body {
          js::ArrowFunctionBody::Expr(e) => self.premake_expr(e),
          js::ArrowFunctionBody::FunctionBody(b) => self.premake_returning(&b.stmts),
        });
        self.lowerer.scope.truncate(depth);
        result
      }
      js::Expr::Call(call) => {
        if let js::Callee::Expr(callee) = &call.callee {
          if let js::Expr::Member(member) = &**callee {
            self.premake_expr(&member.obj)?;
          }
        }
        for arg in &call.args {
          self.premake_expr(&arg.expr)?;
        }
        Ok(())
      }
      _ => Ok(()),
    }
  }

  /// [`Self::premake_expr`] over a statement whose expressions reach
  /// `Lowerer::expr`: a declaration, a write or a branch holding them.
  /// Marks the loop at `span` open for the hoist candidates its body holds, with the `push` calls its body makes onto `written`, so the browser's copy keys them by iteration; whether there was a candidate set to mark.
  fn open_statement_loop(&mut self, span: Span, body: &js::Stmt, written: &[String]) -> bool {
    let range = self.lowerer.parsed.range(span);
    let body_range = self.lowerer.parsed.range(body.span());
    struct Pushes<'w> {
      written: &'w [String],
      args: Vec<Span>,
    }
    impl Visit for Pushes<'_> {
      fn visit_call_expr(&mut self, call: &js::CallExpr) {
        if let js::Callee::Expr(callee) = &call.callee {
          if let js::Expr::Member(m) = &**callee {
            if let (js::Expr::Ident(id), Some("push")) = (&*m.obj, prop_name_of(m).as_deref()) {
              if self.written.iter().any(|w| w == id.sym.as_ref()) {
                self.args.extend(call.args.iter().filter(|a| a.spread.is_none()).map(|a| a.expr.span()));
              }
            }
          }
        }
        call.visit_children_with(self);
      }
      fn visit_function(&mut self, _: &js::Function) {}
      fn visit_arrow_expr(&mut self, _: &js::ArrowExpr) {}
      fn visit_for_of_stmt(&mut self, _: &js::ForOfStmt) {}
      fn visit_for_stmt(&mut self, _: &js::ForStmt) {}
    }
    let mut pushes = Pushes { written, args: Vec::new() };
    body.visit_with(&mut pushes);
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
  fn premake_for_each(&mut self, call: &'p js::CallExpr) -> Lowered<()> {
    let Some(js::Stmt::For(f)) = for_each_loop(call, self.lowerer.meanings.as_deref()) else { return Ok(()) };
    let over_children = matches!(&call.callee, js::Callee::Expr(c) if matches!(&**c, js::Expr::Member(m) if self.meanings().of_expr(&m.obj) == Some(crate::meaning::Meaning::Library(crate::meaning::Construct::Children))));
    if over_children {
      if self.slot_of_expr(&call.args[0].expr).as_deref() != Some("content") {
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
    let callback = if over_children { &call.args[1] } else { &call.args[0] };
    let (params, body): (Vec<js::Pat>, FunctionBody<'p>) = match crate::meaning::bare(&callback.expr) {
      js::Expr::Arrow(arrow) => (arrow.params.clone(), arrow_body(arrow)),
      js::Expr::Fn(func) => match &func.function.body {
        Some(b) => (patterns(&func.function), FunctionBody::Block(&b.stmts)),
        None => return Ok(()),
      },
      _ => return Ok(()),
    };
    let js::Stmt::Block(looped) = &*f.body else { return Ok(()) };
    let item = looped.stmts.first().and_then(|s| match s {
      js::Stmt::Decl(js::Decl::Var(var)) if params.first().is_some() => var.decls.first().and_then(|d| d.init.clone()),
      _ => None,
    });
    let depth = self.lowerer.scope.len();
    let result = counted.lowered(&mut self.lowerer, &names.item).and_then(|(_, counter)| {
      for name in &names.written {
        self.lowerer.scope.push((name.clone(), names.read(name)));
      }
      self.lowerer.scope.push((counted.name.clone(), counter));
      if let (Some(head), Some(item)) = (params.first(), item) {
        let value = self.lowerer.expr(&item)?;
        bind_pattern(&mut self.lowerer, head, value)?;
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

  fn premake_block(&mut self, stmts: &'p [js::Stmt]) -> Lowered<()> {
    let depth = self.lowerer.scope.len();
    let result = stmts.iter().try_for_each(|stmt| {
      self.premake_stmt(stmt)?;
      if let js::Stmt::Decl(js::Decl::Var(var)) = stmt {
        for decl in &var.decls {
          let value = match decl.init.as_deref() {
            Some(init) => self.lowerer.expr(init)?,
            None => Expr::Lit(Lit::Null),
          };
          bind_pattern(&mut self.lowerer, &decl.name, value)?;
        }
      }
      Ok(())
    });
    self.lowerer.scope.truncate(depth);
    result
  }

  /// [`Self::premake_block`] over a function's body, whose `return`s hold values the way its declarations do.
  fn premake_returning(&mut self, stmts: &'p [js::Stmt]) -> Lowered<()> {
    let depth = self.lowerer.scope.len();
    let result = stmts.iter().try_for_each(|stmt| {
      match stmt {
        js::Stmt::Return(r) => {
          if let Some(arg) = r.arg.as_deref() {
            self.premake_expr(arg)?;
          }
        }
        other => self.premake_stmt(other)?,
      }
      if let js::Stmt::Decl(js::Decl::Var(var)) = stmt {
        for decl in &var.decls {
          let value = match decl.init.as_deref() {
            Some(init) => self.lowerer.expr(init)?,
            None => Expr::Lit(Lit::Null),
          };
          bind_pattern(&mut self.lowerer, &decl.name, value)?;
        }
      }
      Ok(())
    });
    self.lowerer.scope.truncate(depth);
    result
  }

  fn premake_stmt(&mut self, stmt: &'p js::Stmt) -> Lowered<()> {
    match stmt {
      js::Stmt::Decl(js::Decl::Var(var)) => {
        for decl in &var.decls {
          if let Some(init) = decl.init.as_deref() {
            self.premake_expr(init)?;
          }
        }
        Ok(())
      }
      js::Stmt::Expr(e) => match &*e.expr {
        js::Expr::Call(call) if for_each_loop(call, self.lowerer.meanings.as_deref()).is_some() => self.premake_for_each(call),
        other => self.premake_expr(other),
      },
      js::Stmt::If(branch) => {
        self.premake_stmt(&branch.cons)?;
        match branch.alt.as_deref() {
          Some(alt) => self.premake_stmt(alt),
          None => Ok(()),
        }
      }
      js::Stmt::Block(b) => self.premake_block(&b.stmts),
      js::Stmt::Switch(sw) => {
        for case in &sw.cases {
          self.premake_block(&case.cons)?;
        }
        Ok(())
      }
      js::Stmt::ForOf(for_of) => {
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
      js::Stmt::For(f) => {
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
    let (params, body): (Vec<js::Pat>, FunctionBody<'p>) = match found {
      Found::Declared(params, stmts, block) => {
        self.hook = Some(Hook::Block { after: self.lowerer.parsed.range(*block).start + 1 });
        (params.clone(), FunctionBody::Block(stmts))
      }
      Found::Arrow(arrow) => {
        self.hook = Some(match &*arrow.body {
          js::ArrowFunctionBody::FunctionBody(b) => Hook::Block { after: self.lowerer.parsed.range(b.span).start + 1 },
          js::ArrowFunctionBody::Expr(e) => Hook::Expression(self.lowerer.parsed.range(e.span())),
        });
        (arrow.params.clone(), arrow_body(arrow))
      }
    };
    let floor = self.lowerer.scope.len();
    self.bind_props(&params)?;
    if self.forwards_ref {
      if let Some(js::Pat::Ident(id)) = params.get(1) {
        self.lowerer.scope.push((id.id.sym.to_string(), Expr::Field(Box::new(Expr::Var("$props".to_owned())), "ref".to_owned())));
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
        for (at, stmt) in stmts.iter().enumerate() {
          match stmt {
            js::Stmt::Return(ret) => {
              render = Some(match ret.arg.as_deref() {
                Some(arg) => self.child_expr(arg)?,
                None => Tmpl::Fragment(Vec::new()),
              });
              break;
            }
            js::Stmt::If(branch) if holds_return(stmt) && early_return(branch).is_none() => {
              let rest: Vec<&'p js::Stmt> = stmts[at..].iter().collect();
              render = Some(self.tree_of(&rest, floor)?);
              break;
            }
            js::Stmt::Switch(_) | js::Stmt::Block(_) if holds_return(stmt) => {
              let rest: Vec<&'p js::Stmt> = stmts[at..].iter().collect();
              render = Some(self.tree_of(&rest, floor)?);
              break;
            }
            js::Stmt::If(branch) if early_return(branch).is_some() => {
              let ret = early_return(branch).expect("checked");
              let cond = self.lowerer.expr(&branch.test)?;
              let then = match ret.arg.as_deref() {
                Some(arg) => self.child_expr(arg)?,
                None => Tmpl::Fragment(Vec::new()),
              };
              early.push((cond, then));
            }
            js::Stmt::Decl(js::Decl::Var(var)) if !early.is_empty() => {
              self.premake_stmt(stmt)?;
              for decl in &var.decls {
                if let Some(init) = decl.init.as_deref() {
                  if let Some((hook, _)) = hook_call(&self.meanings(), init) {
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
            js::Stmt::Expr(e) if hook_call(&self.meanings(), &e.expr).is_some_and(|(name, _)| self.custom_hook(name)) => {
              let (hook, call) = hook_call(&self.meanings(), &e.expr).expect("checked");
              self.inline_hook(hook, call, None)?;
              lets.append(&mut self.pending_lets);
            }
            js::Stmt::Expr(e) if !hook_call(&self.meanings(), &e.expr).is_some_and(|(name, _)| EFFECT_HOOKS.contains(&name)) => {
              self.premake_stmt(stmt)?;
              self.rebound(stmt, floor, &early, &mut lets, &guarded)?
            }
            js::Stmt::If(_) | js::Stmt::Switch(_) | js::Stmt::Block(_) | js::Stmt::ForOf(_) | js::Stmt::For(_) => {
              self.premake_stmt(stmt)?;
              self.rebound(stmt, floor, &early, &mut lets, &guarded)?
            }
            js::Stmt::Decl(js::Decl::Fn(f)) => {
              self.browser_function(f.ident.sym.to_string());
              if let Some(body) = &f.function.body {
                self.handler_fns.insert(f.ident.sym.to_string(), (patterns(&f.function), FunctionBody::Block(&body.stmts)));
              }
            }
            js::Stmt::Expr(e) if hook_call(&self.meanings(), &e.expr).is_some_and(|(name, _)| EFFECT_HOOKS.contains(&name)) => {}
            js::Stmt::Decl(js::Decl::Var(var)) => {
              self.premake_stmt(stmt)?;
              for decl in &var.decls {
                let stmt = self.let_stmt(decl)?;
                self.note_alias_of(decl, floor)?;
                lets.append(&mut self.pending_lets);
                if let Some(stmt) = stmt {
                  lets.push(stmt);
                }
              }
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
      self.state.push(name.clone());
      self.state_bindings.push(name.clone());
      self.store_bindings.push((name.clone(), key.clone()));
      bound.push(Stmt::Let { name, expr: Expr::Coalesce(Box::new(Expr::Store(key)), Box::new(seed)) });
    }
    bound.extend(lets);
    let mut lets = bound;
    lets.extend(std::mem::take(&mut self.store_lets));
    Ok((Component { body: lets, render, state: std::mem::take(&mut self.state_bindings), stores: std::mem::take(&mut self.store_bindings), handlers: std::mem::take(&mut self.lowered_handlers), owner: Owner::React, shadow: None, scope: None }, std::mem::take(&mut self.refs)))
  }

  /// Records `decl` as an alias when it binds a name to an element of a local array (`note_alias`).
  fn note_alias_of(&mut self, decl: &js::VarDeclarator, floor: usize) -> Lowered<()> {
    let (js::Pat::Ident(id), Some(init)) = (&decl.name, decl.init.as_deref()) else { return Ok(()) };
    let frozen = self.state.clone();
    note_alias(&mut self.lowerer, id.id.sym.as_ref(), init, &Writable { floor, frozen: &frozen, setters: &[] })
  }

  /// The statements from a branching `return` on, as the tree each path returns: a branch is a `Tmpl::If`, a declaration a `Tmpl::Let` over what follows it, a write to a local a `Tmpl::Let` of its new value. A path that ends without a `return` renders nothing.
  fn tree_of(&mut self, stmts: &[&'p js::Stmt], floor: usize) -> Lowered<Tmpl> {
    let Some((first, rest)) = stmts.split_first() else { return Ok(Tmpl::Fragment(Vec::new())) };
    let joined = |head: &'p js::Stmt, rest: &[&'p js::Stmt]| -> Vec<&'p js::Stmt> {
      let mut out: Vec<&'p js::Stmt> = match head {
        js::Stmt::Block(b) => b.stmts.iter().collect(),
        other => vec![other],
      };
      out.extend_from_slice(rest);
      out
    };
    match first {
      js::Stmt::Return(ret) => match ret.arg.as_deref() {
        Some(arg) => self.child_expr(arg),
        None => Ok(Tmpl::Fragment(Vec::new())),
      },
      js::Stmt::Throw(thrown) => Ok(Tmpl::Expr(throw_expr(&mut self.lowerer, thrown)?)),
      js::Stmt::Empty(_) => self.tree_of(rest, floor),
      js::Stmt::Block(_) if holds_return(first) => {
        let depth = self.lowerer.scope.len();
        let tree = self.tree_of(&joined(first, rest), floor);
        self.lowerer.scope.truncate(depth);
        tree
      }
      js::Stmt::If(branch) if holds_return(first) => {
        let cond = self.lowerer.expr(&branch.test)?;
        let depth = self.lowerer.scope.len();
        let then = self.tree_of(&joined(&branch.cons, rest), floor);
        self.lowerer.scope.truncate(depth);
        let r#else = match branch.alt.as_deref() {
          Some(alt) => self.tree_of(&joined(alt, rest), floor),
          None => self.tree_of(rest, floor),
        };
        self.lowerer.scope.truncate(depth);
        Ok(Tmpl::If { cond, then: Box::new(then?), r#else: Some(Box::new(r#else?)) })
      }
      js::Stmt::Switch(sw) if holds_return(first) => {
        let disc = self.lowerer.expr(&sw.discriminant)?;
        let (arms, default) = switch_arms(&self.lowerer, sw)?;
        let depth = self.lowerer.scope.len();
        let mut fallback: Vec<&'p js::Stmt> = default.map(|d| d.iter().collect()).unwrap_or_default();
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
          let mut path: Vec<&'p js::Stmt> = body.iter().collect();
          path.extend_from_slice(rest);
          let then = self.tree_of(&path, floor);
          self.lowerer.scope.truncate(depth);
          let cond = cond.unwrap_or(Expr::Lit(Lit::Bool(false)));
          tree = Tmpl::If { cond, then: Box::new(then?), r#else: Some(Box::new(tree)) };
        }
        Ok(tree)
      }
      js::Stmt::Decl(js::Decl::Var(var)) => {
        self.premake_stmt(first)?;
        let depth = self.lowerer.scope.len();
        let mut lets = Vec::new();
        for decl in &var.decls {
          if let Some((hook, _)) = decl.init.as_deref().and_then(|init| hook_call(&self.meanings(), init)).filter(|(hook, _)| !matches!(*hook, "use" | "useContext")) {
            return Err(self.lowerer.residue(decl.span, format!("`{hook}` after a `return`, which React refuses since the hooks would change between renders")));
          }
          let expr = match decl.init.as_deref() {
            Some(init) => self.lowerer.expr(init)?,
            None => Expr::Lit(Lit::Null),
          };
          let name = match &decl.name {
            js::Pat::Ident(id) => {
              let name = id.id.sym.to_string();
              self.lowerer.scope.push((name.clone(), Expr::Var(name.clone())));
              if let Some(init) = decl.init.as_deref() {
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
      js::Stmt::Decl(js::Decl::Fn(f)) => Err(self.lowerer.residue(f.function.span, "a function declared after a branching `return`; declare it before")),
      straight => {
        self.premake_stmt(straight)?;
        let depth = self.lowerer.scope.len();
        let frozen = self.state.clone();
        let writable = Writable { floor, frozen: &frozen, setters: &[] };
        let mut assigned = Vec::new();
        rebind_stmt(&mut self.lowerer, straight, &writable, &mut assigned)?;
        let mut lets = Vec::new();
        for name in assigned {
          let value = current(&self.lowerer, &name);
          let fresh = self.lowerer.temp();
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

  /// A statement that writes locals, run over the scope: each name it wrote is bound to its final value by a `let` of its own.
  fn rebound(&mut self, stmt: &js::Stmt, floor: usize, early: &[(Expr, Tmpl)], lets: &mut Vec<Stmt>, guarded: &dyn Fn(&[(Expr, Tmpl)], Expr) -> Expr) -> Lowered<()> {
    let frozen = self.state.clone();
    let setters = self.setters.clone();
    let writable = Writable { floor, frozen: &frozen, setters: &setters };
    let mut assigned = Vec::new();
    rebind_stmt(&mut self.lowerer, stmt, &writable, &mut assigned)?;
    for name in assigned {
      let value = current(&self.lowerer, &name);
      let fresh = self.lowerer.temp();
      lets.push(Stmt::Let { name: fresh.clone(), expr: guarded(early, value) });
      self.lowerer.scope.push((name, Expr::Var(fresh)));
    }
    Ok(())
  }

  /// `components={{ h2: Heading }}` on a placed MDX file: each element and the
  /// component this file's name for it resolves to.
  fn component_overrides(&mut self, attr: &'p js::JSXAttr) -> Lowered<Vec<Override>> {
    let object = match &attr.value {
      Some(js::JSXAttrValue::JSXExprContainer(js::JSXExprContainer { expr: js::JSXExpr::Expr(e), .. })) => match &**e {
        js::Expr::Object(object) => object,
        other => return Err(self.lowerer.residue(other.span(), "`components` that is not an object written out, as `{{ h2: Heading }}`")),
      },
      _ => return Err(self.lowerer.residue(attr.span, "`components` that is not an object written out, as `{{ h2: Heading }}`")),
    };
    let mut out = Vec::new();
    for prop in &object.props {
      let (key, local) = match prop {
        js::PropOrSpread::Prop(p) => match &**p {
          js::Prop::KeyValue(kv) => match (prop_name(&kv.key), &*kv.value) {
            (Some(key), js::Expr::Ident(local)) => (key, local.sym.to_string()),
            _ => return Err(self.lowerer.residue(prop.span(), "a `components` entry that is not a component's name")),
          },
          js::Prop::Shorthand(id) => (id.sym.to_string(), id.sym.to_string()),
          _ => return Err(self.lowerer.residue(prop.span(), "a `components` entry that is not a component's name")),
        },
        js::PropOrSpread::Spread(spread) => return Err(self.lowerer.residue(spread.expr.span(), "a spread in `components`")),
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
  fn template_import(&mut self, decl: &'p js::VarDeclarator, call: &'p js::CallExpr) -> Lowered<()> {
    let Some(choices) = self.imports.get(&call.span.lo.0).cloned() else {
      return Err(self.lowerer.residue(call.span, "an `import()` of something other than a template with one substitution, such as `../content/${slug}.mdx`"));
    };
    let Some(js::Expr::Tpl(tpl)) = call.args.first().map(|a| &*a.expr) else { unreachable!("a template import") };
    let key = self.lowerer.expr(&tpl.exprs[0])?;
    let js::Pat::Object(pattern) = &decl.name else {
      return Err(self.lowerer.residue(decl.span, "an imported module bound whole; destructure it, as `const { default: Post, meta } = await import(...)`"));
    };
    if choices.is_empty() {
      return Err(self.lowerer.residue(call.span, "an `import()` no file under the app matches"));
    }
    let picks = |value: &str| Expr::Compare(CompareOp::Eq, Box::new(Expr::Str(Box::new(key.clone()))), Box::new(Expr::lit_str(value)));
    for prop in &pattern.props {
      let (export, local) = match prop {
        js::ObjectPatProp::KeyValue(kv) => match (prop_name(&kv.key), &*kv.value) {
          (Some(export), js::Pat::Ident(local)) => (export, local.id.sym.to_string()),
          _ => return Err(self.lowerer.residue(prop.span(), "a nested pattern in an imported module's destructuring")),
        },
        js::ObjectPatProp::Assign(a) if a.value.is_none() => (a.key.id.sym.to_string(), a.key.id.sym.to_string()),
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

  /// `this` in a class's `render()`: `this.props` the props, `this.state` the initial state as component state, each method and arrow field a function the browser runs, `createRef()` a ref holding null and each other field its value.
  fn bind_this(&mut self, class: &'p js::Class) -> Lowered<Option<Stmt>> {
    let mut entries = vec![Entry::Field("props".to_owned(), Expr::Var("$props".to_owned()))];
    let mut state: Option<Expr> = None;
    let browser = || Expr::Handler(snapfire_fsr_ir::render::BROWSER_HANDLER);
    for member in &class.body {
      match member {
        js::ClassMember::ClassProp(p) if !p.is_static => {
          let Some(key) = prop_name(&p.key) else { return Err(self.lowerer.residue(p.span, "a computed class field")) };
          let value = match p.value.as_deref().map(crate::meaning::bare) {
            None => Expr::Lit(Lit::Null),
            Some(js::Expr::Arrow(_) | js::Expr::Fn(_)) => browser(),
            Some(js::Expr::Call(call)) if matches!(&call.callee, js::Callee::Expr(c) if react_export(self.lowerer.parsed, c).as_deref() == Some("createRef")) => Expr::Object(vec![Entry::Field("current".to_owned(), Expr::Lit(Lit::Null))]),
            Some(value) => self.lowerer.expr(value)?,
          };
          if key == "state" {
            state = Some(value);
          } else {
            entries.push(Entry::Field(key, value));
          }
        }
        js::ClassMember::Method(m) if !m.is_static => {
          if let Some(key) = prop_name(&m.key).filter(|k| k != "render") {
            entries.push(Entry::Field(key, browser()));
          }
        }
        js::ClassMember::Constructor(c) => {
          let depth = self.lowerer.scope.len();
          if let Some(js::ParamOrTsParamProp::Param(param)) = c.params.first() {
            if let js::Pat::Ident(id) = &param.pat {
              self.lowerer.scope.push((id.id.sym.to_string(), Expr::Var("$props".to_owned())));
            }
          }
          for stmt in c.body.iter().flat_map(|b| &b.stmts) {
            let js::Stmt::Expr(e) = stmt else { return Err(self.lowerer.residue(stmt.span(), "a constructor statement other than `super(props)` or a write to `this`")) };
            match &*e.expr {
              js::Expr::Call(call) if matches!(call.callee, js::Callee::Super(_)) => {}
              js::Expr::Assign(a) if a.op == js::AssignOp::Assign => {
                let field = match &a.left {
                  js::AssignTarget::Simple(js::SimpleAssignTarget::Member(m)) if matches!(&*m.obj, js::Expr::This(_)) => prop_name_of(m),
                  _ => None,
                };
                let Some(field) = field else { return Err(self.lowerer.residue(a.span, "a constructor write to something other than `this`")) };
                if field == "state" {
                  state = Some(self.lowerer.expr(&a.right)?);
                } else if !entries.iter().any(|e| matches!(e, Entry::Field(n, _) if *n == field)) {
                  let value = match crate::meaning::bare(&a.right) {
                    js::Expr::Arrow(_) | js::Expr::Fn(_) | js::Expr::Call(_) => browser(),
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

  fn bind_props(&mut self, params: &[js::Pat]) -> Lowered<()> {
    let Some(first) = params.first() else { return Ok(()) };
    for (key, kind) in prop_kinds_of(self.lowerer.parsed, first) {
      match self.lowerer.prop_kinds.iter_mut().find(|(k, _)| *k == key) {
        Some(entry) => entry.1 = kind,
        None => self.lowerer.prop_kinds.push((key, kind)),
      }
    }
    match first {
      js::Pat::Ident(id) => {
        self.props_name = Some(id.id.sym.to_string());
        self.lowerer.scope.push((id.id.sym.to_string(), Expr::Var("$props".to_owned())));
        Ok(())
      }
      js::Pat::Object(obj) => {
        self.children_name = obj.props.iter().find_map(|prop| match prop {
          js::ObjectPatProp::Assign(a) if a.key.id.sym.as_ref() == "children" => Some("children".to_owned()),
          js::ObjectPatProp::KeyValue(kv) if prop_name(&kv.key).as_deref() == Some("children") => match &*kv.value {
            js::Pat::Ident(local) => Some(local.id.sym.to_string()),
            _ => None,
          },
          _ => None,
        });
        if self.layout_root {
          for prop in &obj.props {
            let (local, key) = match prop {
              js::ObjectPatProp::Assign(a) => (a.key.id.sym.to_string(), a.key.id.sym.to_string()),
              js::ObjectPatProp::KeyValue(kv) => match (prop_name(&kv.key), &*kv.value) {
                (Some(key), js::Pat::Ident(local)) => (local.id.sym.to_string(), key),
                _ => continue,
              },
              js::ObjectPatProp::Rest(_) => continue,
            };
            if self.slot_names.contains(&key) {
              self.slot_props.push((local, key));
            }
          }
        }
        bind_object(&mut self.lowerer, obj, Expr::Var("$props".to_owned()))?;
        for prop in &obj.props {
          let (local, key) = match prop {
            js::ObjectPatProp::Assign(a) => (a.key.id.sym.to_string(), a.key.id.sym.to_string()),
            js::ObjectPatProp::KeyValue(kv) => match (prop_name(&kv.key), &*kv.value) {
              (Some(key), js::Pat::Ident(local)) => (local.id.sym.to_string(), key),
              _ => continue,
            },
            js::ObjectPatProp::Rest(_) => continue,
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
  fn let_stmt(&mut self, decl: &'p js::VarDeclarator) -> Lowered<Option<Stmt>> {
    let Some(init) = decl.init.as_deref() else {
      let js::Pat::Ident(name) = &decl.name else { return Err(self.lowerer.residue(decl.span, "a destructuring without a value")) };
      let local = name.id.sym.to_string();
      self.lowerer.scope.push((local.clone(), Expr::Var(local.clone())));
      return Ok(Some(Stmt::Let { name: local, expr: Expr::Lit(Lit::Null) }));
    };
    if let Some(call) = awaited_import(init) {
      self.template_import(decl, call)?;
      return Ok(None);
    }
    match &decl.name {
      js::Pat::Ident(name) => {
        let local = name.id.sym.to_string();
        if self.lowerer.bind_formatter(&local, init) {
          return Ok(None);
        }
        let expr = match hook_call(&self.meanings(), init) {
          Some(("useCallback", call)) => {
            if let Some(js::Expr::Arrow(arrow)) = call.args.first().map(|a| &*a.expr) {
              self.handler_fns.insert(local.clone(), (arrow.params.clone(), arrow_body(arrow)));
            }
            self.browser_function(local);
            return Ok(None);
          }
          Some(("useMemo", call)) => {
            let Some(js::Expr::Arrow(arrow)) = call.args.first().map(|a| &*a.expr) else {
              return Err(self.lowerer.residue(decl.span, "`useMemo` of something other than an arrow"));
            };
            match &*arrow.body {
              js::ArrowFunctionBody::Expr(e) => self.lowerer.expr(e)?,
              js::ArrowFunctionBody::FunctionBody(b) => block_to_expr(&mut self.lowerer, &b.stmts)?,
            }
          }
          Some(("useRef", call)) => {
            let current = match call.args.first() {
              Some(a) => self.lowerer.expr(&a.expr)?,
              None => Expr::Lit(Lit::Null),
            };
            self.state.push(local.clone());
            Expr::Object(vec![Entry::Field("current".to_owned(), current)])
          }
          Some(("useStore", _)) => {
            return Err(self.lowerer.residue(decl.span, "`useStore` bound to one name; it is a pair, as `const [x, setX] = useStore(key, initial)`"))
          }
          Some(("useLocale", _)) => Expr::Locale,
          Some(("useEffectEvent", call)) => {
            if let Some(js::Expr::Arrow(arrow)) = call.args.first().map(|a| &*a.expr) {
              self.handler_fns.insert(local.clone(), (arrow.params.clone(), arrow_body(arrow)));
            }
            self.browser_function(local);
            return Ok(None);
          }
          Some((hook @ ("useDeferredValue" | "useFormStatus" | "useSyncExternalStore"), call)) => self.lowerer.server_hook_value(hook, call)?,
          Some(("useId", call)) if !matches!(self.meanings().bindings.get("useId"), Some(crate::meaning::Meaning::Own { .. })) => {
            let range = self.lowerer.parsed.range(call.span);
            match &mut self.lowerer.hoisting {
              Some(candidates) => candidates.id(range),
              None => return Err(self.lowerer.residue_with(decl.span, "`useId` inside a custom hook", "the id is keyed by the component that calls the hook; call `useId` there and pass the id in")),
            }
          }
          Some((hook, call)) if self.custom_hook(hook) => return self.inline_hook(hook, call, Some(&decl.name)),
          Some((hook, _)) if !matches!(hook, "useState" | "use" | "useContext") => return Err(self.lowerer.residue(decl.span, format!("`{hook}`"))),
          _ if matches!(init, js::Expr::Arrow(_) | js::Expr::Fn(_)) => {
            match init {
              js::Expr::Arrow(arrow) => {
                self.handler_fns.insert(local.clone(), (arrow.params.clone(), arrow_body(arrow)));
              }
              js::Expr::Fn(f) => {
                if let Some(body) = &f.function.body {
                  self.handler_fns.insert(local.clone(), (patterns(&f.function), FunctionBody::Block(&body.stmts)));
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
              let js::Expr::Object(obj) = init else { unreachable!("checked") };
              let mut entries = Vec::new();
              for prop in &obj.props {
                let js::PropOrSpread::Prop(p) = prop else { unreachable!("checked") };
                let js::Prop::KeyValue(kv) = &**p else { unreachable!("checked") };
                let key = prop_name(&kv.key).ok_or_else(|| self.lowerer.residue(kv.key.span(), "a numeric property name"))?;
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
                  let (line, column) = self.lowerer.parsed.loc(init.span().lo);
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
      js::Pat::Array(arr) => {
        let called = hook_call(&self.meanings(), init).map(|(name, _)| name).unwrap_or_default();
        let js::Expr::Call(call) = init else { return self.pattern_stmt(&decl.name, init) };
        if called == "useStore" {
          return self.store_stmt(decl, arr, call);
        }
        if called == "useReducer" {
          return self.reducer_stmt(arr, call);
        }
        if !called.is_empty() && called != "useState" && self.custom_hook(called) {
          return self.inline_hook(called, call, Some(&decl.name));
        }
        if matches!(called, "useTransition" | "useActionState" | "useOptimistic") {
          return self.pending_hook_stmt(called, arr, call);
        }
        if called != "useState" {
          if !called.is_empty() {
            return Err(self.lowerer.residue(decl.span, format!("`{called}`")));
          }
          return self.pattern_stmt(&decl.name, init);
        }
        self.lowerer.browser_value = None;
        let initial = match call.args.first() {
          Some(a) => match &*a.expr {
            js::Expr::Arrow(arrow) => match &*arrow.body {
              js::ArrowFunctionBody::Expr(e) => self.lowerer.expr(e),
              js::ArrowFunctionBody::FunctionBody(b) => block_to_expr(&mut self.lowerer, &b.stmts),
            },
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
        let mut names = arr.elems.iter().map(|e| match e {
          Some(js::Pat::Ident(id)) => Some(id.id.sym.to_string()),
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
        } else if let Some(initial) = call.args.first() {
          self.lowerer.note_kind(&name, &initial.expr);
        }
        Ok(Some(Stmt::Let { name, expr }))
      }
      pattern => self.pattern_stmt(pattern, init),
    }
  }

  /// `useTransition()` as `[false, startTransition]`, `useActionState(action, initial)` as `[initial, formAction, false]` and `useOptimistic(value, update)` as `[value, addOptimistic]`, which is what React's server renderer returns; each function is the browser's.
  fn pending_hook_stmt(&mut self, hook: &str, arr: &js::ArrayPat, call: &js::CallExpr) -> Lowered<Option<Stmt>> {
    let arg = |this: &mut Self, i: usize| -> Lowered<Expr> {
      match call.args.get(i) {
        Some(a) => this.lowerer.expr(&a.expr),
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
    for (at, (elem, part)) in arr.elems.iter().zip(parts).enumerate() {
      let Some(js::Pat::Ident(id)) = elem else { continue };
      let name = id.id.sym.to_string();
      let state = hook == "useActionState" && at == 0;
      match part {
        None => self.browser_function(name),
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
  fn reducer_stmt(&mut self, arr: &js::ArrayPat, call: &js::CallExpr) -> Lowered<Option<Stmt>> {
    let initial = match call.args.get(1) {
      Some(a) => self.lowerer.expr(&a.expr)?,
      None => return Err(self.lowerer.residue(call.span, "`useReducer` without an initial state")),
    };
    let expr = match call.args.get(2).map(|a| &*a.expr) {
      None => initial,
      Some(js::Expr::Arrow(arrow)) => Expr::Apply { f: Box::new(self.lowerer.lambda(arrow)?), args: vec![initial] },
      Some(init) => Expr::Apply { f: Box::new(self.lowerer.expr(init)?), args: vec![initial] },
    };
    let mut names = arr.elems.iter().map(|e| match e {
      Some(js::Pat::Ident(id)) => Some(id.id.sym.to_string()),
      _ => None,
    });
    let state = names.next().flatten();
    if let Some(dispatch) = names.next().flatten() {
      self.browser_function(dispatch);
    }
    let Some(name) = state else { return Ok(None) };
    self.state.push(name.clone());
    self.state_bindings.push(name.clone());
    self.lowerer.scope.push((name.clone(), Expr::Var(name.clone())));
    Ok(Some(Stmt::Let { name, expr }))
  }

  /// A destructuring in a component body: the value under a temporary each name reads a part of.
  fn pattern_stmt(&mut self, pattern: &'p js::Pat, init: &'p js::Expr) -> Lowered<Option<Stmt>> {
    let expr = match hook_call(&self.meanings(), init) {
      Some((hook, call)) if self.custom_hook(hook) => return self.inline_hook(hook, call, Some(pattern)),
      Some(("useFormStatus", call)) => {
        // Each field is a constant on the server, bound as one so a read of `data` is known to be null.
        let Expr::Object(fields) = self.lowerer.server_hook_value("useFormStatus", call)? else { unreachable!("useFormStatus is an object") };
        let js::Pat::Object(obj) = pattern else { return Err(self.lowerer.residue(pattern.span(), "`useFormStatus` destructured as other than an object")) };
        for prop in &obj.props {
          let (key, local) = match prop {
            js::ObjectPatProp::Assign(a) => (a.key.id.sym.to_string(), a.key.id.sym.to_string()),
            js::ObjectPatProp::KeyValue(kv) => match (prop_name(&kv.key), &*kv.value) {
              (Some(key), js::Pat::Ident(id)) => (key, id.id.sym.to_string()),
              _ => return Err(self.lowerer.residue(prop.span(), "a `useFormStatus` field bound to other than a name")),
            },
            js::ObjectPatProp::Rest(r) => return Err(self.lowerer.residue(r.span, "a rest of `useFormStatus`")),
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
  fn store_stmt(&mut self, decl: &js::VarDeclarator, arr: &js::ArrayPat, call: &js::CallExpr) -> Lowered<Option<Stmt>> {
    let Some(first) = call.args.first() else {
      return Err(self.lowerer.residue(decl.span, "`useStore` without a key"));
    };
    let key = match self.lowerer.expr(&first.expr)? {
      Expr::Lit(Lit::Str(key)) => key,
      _ => return Err(self.lowerer.residue(first.expr.span(), "a `useStore` key that is not a string the build can read")),
    };
    let initial = match call.args.get(1) {
      Some(a) => self.lowerer.expr(&a.expr)?,
      None => Expr::Lit(Lit::Null),
    };
    let mut names = arr.elems.iter().map(|e| match e {
      Some(js::Pat::Ident(id)) => Some(id.id.sym.to_string()),
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
  fn child_expr(&mut self, expr: &'p js::Expr) -> Lowered<Tmpl> {
    self.premake_children(expr)?;
    if let Some(name) = self.slot_of_expr(expr) {
      return Ok(self.slot(&name));
    }
    match expr {
      js::Expr::Paren(p) => self.child_expr(&p.expr),
      js::Expr::JSXElement(el) => self.element(el, false),
      js::Expr::JSXFragment(frag) => Ok(Tmpl::Fragment(self.children(&frag.children)?)),
      js::Expr::Lit(js::Lit::Str(s)) => Ok(Tmpl::Text(s.value.to_atom_lossy().to_string())),
      js::Expr::Cond(c) if self.holds_markup(&c.cons) || self.holds_markup(&c.alt) => {
        let cond = self.lowerer.expr(&c.test)?;
        let then = Box::new(self.child_expr(&c.cons)?);
        let r#else = Some(Box::new(self.child_expr(&c.alt)?));
        Ok(Tmpl::If { cond, then, r#else })
      }
      js::Expr::Bin(bin) if bin.op == js::BinaryOp::NullishCoalescing && self.layout_root && self.slot_of_expr(&bin.left).is_some() => {
        let name = self.slot_of_expr(&bin.left).expect("checked by the guard");
        let fallback = self.child_expr(&bin.right)?;
        Ok(self.slot_with_fallback(&name, vec![fallback]))
      }
      js::Expr::Bin(bin) if bin.op == js::BinaryOp::LogicalAnd && self.holds_markup(&bin.right) => {
        let cond = self.lowerer.expr(&bin.left)?;
        let then = Box::new(self.child_expr(&bin.right)?);
        // `c && <b />` is `c` when `c` is falsy, so `0` and `NaN` print as React prints them and `false`, `null` and `""` print nothing.
        let r#else = Some(Box::new(Tmpl::Expr(cond.clone())));
        Ok(Tmpl::If { cond, then, r#else })
      }
      js::Expr::Call(call) if map_call(call).is_some_and(|(_, arrow)| self.arrow_markup(arrow)) => {
        let (over, arrow) = map_call(call).expect("checked by the guard");
        self.map_tree(over, arrow)
      }
      js::Expr::OptChain(_) if optional_map(expr).is_some_and(|(_, arrow)| self.arrow_markup(arrow)) => {
        let (over, arrow) = optional_map(expr).expect("checked by the guard");
        let missing = Expr::Compare(CompareOp::Eq, Box::new(self.lowerer.expr(over)?), Box::new(Expr::Lit(Lit::Null)));
        let each = self.map_tree(over, arrow)?;
        Ok(Tmpl::If { cond: missing, then: Box::new(Tmpl::Fragment(Vec::new())), r#else: Some(Box::new(each)) })
      }
      js::Expr::Call(_) if self.markup_call(expr).is_some() => {
        let (call, name, params, body) = self.markup_call(expr).expect("checked by the guard");
        self.inline_markup_fn(call, name, &params, body)
      }
      js::Expr::Array(array) if self.holds_markup(expr) => {
        let mut items = Vec::new();
        for item in array.elems.iter().flatten() {
          items.push(self.child_expr(&item.expr)?);
        }
        Ok(fills::items(items))
      }
      js::Expr::Ident(id) if id.sym.as_ref() == "null" || id.sym.as_ref() == "undefined" => Ok(Tmpl::Fragment(Vec::new())),
      js::Expr::Lit(js::Lit::Null(_)) => Ok(Tmpl::Fragment(Vec::new())),
      other => {
        self.premake_expr(other)?;
        Ok(Tmpl::Expr(self.lowerer.expr(other)?))
      }
    }
  }

  /// `over.map(arrow)` where `arrow` returns markup: a `Tmpl::For` over the items.
  fn map_tree(&mut self, over: &'p js::Expr, arrow: &'p js::ArrowExpr) -> Lowered<Tmpl> {
    let over = self.lowerer.expr(over)?;
    let depth = self.lowerer.scope.len();
    let params = bind_params(&mut self.lowerer, &arrow.params)?;
    let range = self.lowerer.parsed.range(arrow.span);
    if let Some(candidates) = &mut self.lowerer.hoisting {
      candidates.open_loops.push(range);
    }
    let body = match &*arrow.body {
      js::ArrowFunctionBody::Expr(e) => self.child_expr(e),
      js::ArrowFunctionBody::FunctionBody(b) => self.tree_of(&b.stmts.iter().collect::<Vec<_>>(), depth),
    };
    if let Some(candidates) = &mut self.lowerer.hoisting {
      candidates.open_loops.pop();
    }
    self.lowerer.scope.truncate(depth);
    Ok(Tmpl::For { over, params, body: Box::new(body?) })
  }

  /// Whether a child expression is markup: JSX or a call to a function the component declares that returns some, alone or inside a branch or `&&`.
  fn holds_markup(&self, expr: &'p js::Expr) -> bool {
    match expr {
      js::Expr::Paren(p) => self.holds_markup(&p.expr),
      js::Expr::Cond(c) => self.holds_markup(&c.cons) || self.holds_markup(&c.alt),
      js::Expr::Bin(b) if b.op == js::BinaryOp::LogicalAnd => self.holds_markup(&b.right),
      js::Expr::Array(array) => array.elems.iter().flatten().any(|item| self.holds_markup(&item.expr)),
      js::Expr::JSXElement(_) | js::Expr::JSXFragment(_) => true,
      js::Expr::Call(call) if map_call(call).is_some_and(|(_, arrow)| self.arrow_markup(arrow)) => true,
      js::Expr::OptChain(_) if optional_map(expr).is_some_and(|(_, arrow)| self.arrow_markup(arrow)) => true,
      other => self.markup_call(other).is_some(),
    }
  }

  /// Each `Children.toArray(children)`, `Children.map(children, f)` and `Children.only(children)` under `e` over the component's own children, made ahead as a read of `render::CHILD_VALUES`, which the placement passes.
  fn premake_children(&mut self, e: &'p js::Expr) -> Lowered<()> {
    match e {
      js::Expr::Call(call) if matches!(&call.callee, js::Callee::Expr(callee) if self.slot_of_expr(callee).as_deref() == Some("content")) => {
        let mut args = Vec::with_capacity(call.args.len());
        for arg in &call.args {
          self.premake_children(&arg.expr)?;
          args.push(self.lowerer.expr(&arg.expr)?);
        }
        let first = Expr::Builtin { name: Builtin::At, args: vec![Expr::Var(snapfire_fsr_ir::render::CHILD_VALUES.to_owned()), Expr::Lit(Lit::Int(0))] };
        let span = call.span;
        self.lowerer.premade.insert((span.lo.0, span.hi.0), Expr::Apply { f: Box::new(first), args });
        if !self.counts_children.contains(&snapfire_fsr_ir::render::CHILD_VALUES) {
          self.counts_children.push(snapfire_fsr_ir::render::CHILD_VALUES);
        }
        Ok(())
      }
      js::Expr::Call(call) => {
        if let Some((method, rest)) = self.own_children_call(call) {
          let values = Expr::Var(snapfire_fsr_ir::render::CHILD_VALUES.to_owned());
          let made = match (method, rest) {
            ("toArray", []) => Some(values),
            ("only", []) => Some(Expr::Builtin { name: Builtin::At, args: vec![values, Expr::Lit(Lit::Int(0))] }),
            ("map", [f]) => {
              self.premake_expr(&f.expr)?;
              Some(Expr::Map(Box::new(values), Box::new(self.lowerer.expr(&f.expr)?)))
            }
            _ => None,
          };
          if let Some(made) = made {
            let span = call.span;
            self.lowerer.premade.insert((span.lo.0, span.hi.0), made);
            if !self.counts_children.contains(&snapfire_fsr_ir::render::CHILD_VALUES) {
              self.counts_children.push(snapfire_fsr_ir::render::CHILD_VALUES);
            }
            return Ok(());
          }
        }
        if let js::Callee::Expr(callee) = &call.callee {
          if let js::Expr::Member(member) = &**callee {
            self.premake_children(&member.obj)?;
          }
        }
        call.args.iter().try_for_each(|arg| self.premake_children(&arg.expr))
      }
      js::Expr::Member(member) => self.premake_children(&member.obj),
      js::Expr::Paren(p) => self.premake_children(&p.expr),
      js::Expr::TsAs(a) => self.premake_children(&a.expr),
      js::Expr::Cond(c) => {
        self.premake_children(&c.test)?;
        self.premake_children(&c.cons)?;
        self.premake_children(&c.alt)
      }
      js::Expr::Bin(b) => {
        self.premake_children(&b.left)?;
        self.premake_children(&b.right)
      }
      _ => Ok(()),
    }
  }

  /// `Children.<method>(children, ...rest)` over the component's own children.
  fn own_children_call<'c>(&self, call: &'c js::CallExpr) -> Option<(&'static str, &'c [js::ExprOrSpread])> {
    let js::Callee::Expr(callee) = &call.callee else { return None };
    let js::Expr::Member(member) = &**callee else { return None };
    if self.meanings().of_expr(&member.obj) != Some(crate::meaning::Meaning::Library(crate::meaning::Construct::Children)) {
      return None;
    }
    let (first, rest) = call.args.split_first()?;
    if first.spread.is_some() || self.slot_of_expr(&first.expr).as_deref() != Some("content") {
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
  fn counts_own_children(&self, e: &js::Expr) -> Option<&'static str> {
    let children_call = |call: &js::CallExpr, method: &str| -> bool {
      let js::Callee::Expr(callee) = &call.callee else { return false };
      let js::Expr::Member(member) = &**callee else { return false };
      prop_name_of(member).as_deref() == Some(method)
        && self.meanings().of_expr(&member.obj) == Some(crate::meaning::Meaning::Library(crate::meaning::Construct::Children))
        && matches!(call.args.as_slice(), [arg] if arg.spread.is_none() && self.slot_of_expr(&arg.expr).as_deref() == Some("content"))
    };
    match unwrap_literal(e) {
      js::Expr::Call(call) if children_call(call, "count") => Some(snapfire_fsr_ir::render::CHILD_COUNT),
      js::Expr::Member(member) if prop_name_of(member).as_deref() == Some("length") && matches!(unwrap_literal(&member.obj), js::Expr::Call(call) if children_call(call, "toArray")) => Some(snapfire_fsr_ir::render::CHILD_ITEMS),
      _ => None,
    }
  }

  /// Whether an arrow returns markup: its expression holds some or its block returns some.
  fn arrow_markup(&self, arrow: &'p js::ArrowExpr) -> bool {
    match &*arrow.body {
      js::ArrowFunctionBody::Expr(e) => self.holds_markup(e),
      js::ArrowFunctionBody::FunctionBody(b) => self.returns_markup(&b.stmts),
    }
  }

  /// Whether a function body returns markup on some path: a `return` of markup or of a local a declaration in the body made markup, inside a branch, a block or a `switch` arm included.
  fn returns_markup(&self, stmts: &'p [js::Stmt]) -> bool {
    let mut locals = Vec::new();
    self.stmts_return_markup(stmts, &mut locals)
  }

  fn stmts_return_markup(&self, stmts: &'p [js::Stmt], locals: &mut Vec<String>) -> bool {
    stmts.iter().any(|stmt| self.stmt_returns_markup(stmt, locals))
  }

  fn stmt_returns_markup(&self, stmt: &'p js::Stmt, locals: &mut Vec<String>) -> bool {
    match stmt {
      js::Stmt::Decl(js::Decl::Var(var)) => {
        for decl in &var.decls {
          if let (js::Pat::Ident(id), Some(init)) = (&decl.name, decl.init.as_deref()) {
            if self.holds_markup(init) {
              locals.push(id.id.sym.to_string());
            }
          }
        }
        false
      }
      js::Stmt::Return(r) => r.arg.as_deref().is_some_and(|arg| self.holds_markup(arg) || matches!(unwrap_literal(arg), js::Expr::Ident(id) if locals.iter().any(|l| l == id.sym.as_ref()))),
      js::Stmt::Block(b) => self.stmts_return_markup(&b.stmts, locals),
      js::Stmt::If(branch) => self.stmt_returns_markup(&branch.cons, locals) || branch.alt.as_deref().is_some_and(|alt| self.stmt_returns_markup(alt, locals)),
      js::Stmt::Switch(sw) => sw.cases.iter().any(|case| self.stmts_return_markup(&case.cons, locals)),
      _ => false,
    }
  }

  /// A call by name to a function the component or its module declares, an arrow `const` or a `function`, whose body returns markup on some path.
  fn markup_call(&self, expr: &'p js::Expr) -> Option<(&'p js::CallExpr, String, Vec<js::Pat>, FunctionBody<'p>)> {
    let js::Expr::Call(call) = expr else { return None };
    let js::Callee::Expr(callee) = &call.callee else { return None };
    let js::Expr::Ident(id) = &**callee else { return None };
    let name = id.sym.to_string();
    let (params, body) = match self.handler_fns.get(&name) {
      Some(declared) => declared.clone(),
      None if !self.lowerer.scope.iter().any(|(n, _)| *n == name) => match find_function(self.lowerer.parsed, &name)? {
        Found::Declared(params, stmts, _) => (params, FunctionBody::Block(stmts)),
        Found::Arrow(arrow) => (arrow.params.clone(), arrow_body(arrow)),
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

  /// A function returning markup, inlined where it is called: each argument is lowered in the caller's scope and bound to its parameter by a `Tmpl::Let` around the function's tree.
  fn inline_markup_fn(&mut self, call: &'p js::CallExpr, name: String, params: &[js::Pat], body: FunctionBody<'p>) -> Lowered<Tmpl> {
    if self.inlining.contains(&name) {
      return Err(self.lowerer.residue(call.span, format!("`{name}` returns markup by calling itself, which the build cannot unroll")));
    }
    if let Some(spread) = call.args.iter().find(|a| a.spread.is_some()) {
      return Err(self.lowerer.residue(spread.expr.span(), format!("a spread argument to `{name}`, which returns markup")));
    }
    let mut args = Vec::with_capacity(params.len());
    for i in 0..params.len() {
      args.push(match call.args.get(i) {
        Some(arg) => self.lowerer.expr(&arg.expr)?,
        None => Expr::Lit(Lit::Null),
      });
    }
    let depth = self.lowerer.scope.len();
    let mut lets = Vec::with_capacity(params.len());
    for (param, value) in params.iter().zip(args) {
      let held = self.lowerer.temp();
      if let Err(residue) = bind_pattern(&mut self.lowerer, param, Expr::Var(held.clone())) {
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
  fn element(&mut self, el: &'p js::JSXElement, as_child: bool) -> Lowered<Tmpl> {
    let tagged = self.lowerer.meanings.as_ref().and_then(|meanings| meanings.of_tag(&el.opening.name));
    let provider = tagged.as_ref().and_then(|meant| meant.as_provider().map(|(id, react)| (id.to_owned(), react)));
    let named_provider = match &el.opening.name {
      js::JSXElementName::JSXMemberExpr(m) if m.prop.sym.as_ref() == "Provider" => Some(jsx_object_name(&m.obj)),
      _ => None,
    };
    if provider.is_some() || named_provider.is_some() {
      let loc = self.lowerer.parsed.loc(el.span.lo);
      let shown = named_provider.unwrap_or_else(|| jsx_tag_name(&el.opening.name));
      self.providers.push((shown, loc, provider.as_ref().map(|(_, react)| *react)));
      let children = Tmpl::Fragment(self.children(&el.children)?);
      let Some((id, _)) = provider else { return Ok(children) };
      let mut value = Expr::Lit(Lit::Null);
      for attr in &el.opening.attrs {
        if let js::JSXAttrOrSpread::JSXAttr(attr) = attr {
          if attr_name(&attr.name) == "value" {
            value = self.attr_value(attr)?;
          }
        }
      }
      return Ok(Tmpl::Let { name: format!("{CONTEXT_PREFIX}{id}"), expr: value, then: Box::new(children) });
    }
    let (name, member) = match &el.opening.name {
      js::JSXElementName::Ident(id) => (id.sym.to_string(), false),
      js::JSXElementName::JSXMemberExpr(m) => match &m.obj {
        js::JSXObject::Ident(obj) => (format!("{}.{}", obj.sym, m.prop.sym), true),
        js::JSXObject::JSXMemberExpr(_) => return Err(self.lowerer.residue(m.span, "a member expression as a tag more than one level deep")),
      },
      js::JSXElementName::JSXNamespacedName(n) => return Err(self.lowerer.residue(n.span, "a namespaced tag")),
    };
    let chosen_tag = match &el.opening.name {
      js::JSXElementName::Ident(id) if !member && self.chosen.get(&name).is_none() => {
        let meanings = self.meanings();
        let local = id.ctxt != meanings.top && id.ctxt != meanings.unresolved;
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
    for attr in &el.opening.attrs {
      let attr = match attr {
        js::JSXAttrOrSpread::JSXAttr(attr) => attr,
        js::JSXAttrOrSpread::SpreadElement(spread) => {
          bound = true;
          attrs.push(Entry::Spread(self.lowerer.expr(&spread.expr)?));
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
          let (line, column) = self.lowerer.parsed.loc(attr.span.lo);
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
        "value" if name == "option" => {
          attrs.push(Entry::Field("value".to_owned(), value.clone()));
          if let Some(selected) = &self.select_value {
            attrs.push(Entry::Field("selected".to_owned(), Expr::Compare(snapfire_fsr_ir::CompareOp::Eq, Box::new(Expr::Str(Box::new(value))), Box::new(Expr::Str(Box::new(selected.clone()))))));
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
    let children = children?;
    if bound {
      attrs.push(Entry::Field(hoist::BOUND_ATTR.to_owned(), Expr::Lit(Lit::Bool(true))));
    }
    if let Some(candidates) = &mut self.lowerer.hoisting {
      if !children.is_empty() && !el.opening.self_closing {
        let range = self.lowerer.parsed.range(el.span);
        let open = self.lowerer.parsed.range(el.opening.span);
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
    self.meanings().of_tag(&el.opening.name).and_then(|meant| meant.placement())
  }

  /// `<Mount module=… props=… />`: the empty region the adapter's `Mount`
  /// renders and then fills in the browser, since the island it places is
  /// named at run time and the server renders none of it.
  fn mount_element(&mut self, el: &'p js::JSXElement) -> Lowered<Tmpl> {
    let attrs = self.jsx_attrs(el)?;
    placements::mount(self, &attrs, At::Span(el.span))
  }

  /// `<Island when="visible"><Chart … /></Island>`: the one component child
  /// as an island with that timing.
  fn island_element(&mut self, el: &'p js::JSXElement) -> Lowered<Tmpl> {
    let mut when = None;
    let mut mode = None;
    let mut define = None;
    for attr in &el.opening.attrs {
      let js::JSXAttrOrSpread::JSXAttr(attr) = attr else { return Err(self.lowerer.residue(el.span, "a spread on `<Island>`")) };
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
      js::JSXElementChild::JSXText(text) => !jsx_text(&text.value.to_atom_lossy()).is_empty(),
      _ => true,
    });
    let (Some(js::JSXElementChild::JSXElement(child)), None) = (elements.next(), elements.next()) else {
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
      let at = self.lowerer.parsed.range(el.opening.name.span()).end;
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
  fn function_prop(&mut self, attr: &'p js::JSXAttr) -> Lowered<Option<Expr>> {
    let Some(js::JSXAttrValue::JSXExprContainer(c)) = &attr.value else { return Ok(None) };
    let js::JSXExpr::Expr(e) = &c.expr else { return Ok(None) };
    let mut inner: &'p js::Expr = e;
    while let js::Expr::Paren(p) = inner {
      inner = &p.expr;
    }
    if !matches!(inner, js::Expr::Arrow(_)) {
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
  fn note_function(&mut self, module: &str, prop: &str, value: &Expr, attr: &'p js::JSXAttr) {
    let Some(js::JSXAttrValue::JSXExprContainer(c)) = &attr.value else { return };
    let js::JSXExpr::Expr(e) = &c.expr else { return };
    let mut needs = Vec::new();
    if let Some(id) = self.function_reference(e, &mut needs) {
      self.fn_refs.push(FnRef { module: module.to_owned(), prop: prop.to_owned(), value: value.clone(), id, needs, unlowered: None });
    }
  }

  /// A function this file holds that the server does not lower but the browser imports by name, as a handler value with the `UNLOWERED_ATTR` text the placement carries for it: a component that calls it only in an effect or a handler never calls it on the server. `None` when the browser cannot reach it either.
  fn unlowered_function(&mut self, module: &str, prop: &str, attr: &'p js::JSXAttr, residue: &Residue) -> Option<(Expr, String)> {
    let Some(js::JSXAttrValue::JSXExprContainer(c)) = &attr.value else { return None };
    let js::JSXExpr::Expr(e) = &c.expr else { return None };
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
  fn names_function(&self, e: &js::Expr) -> bool {
    match crate::meaning::bare(e) {
      js::Expr::Arrow(_) | js::Expr::Fn(_) => true,
      js::Expr::Ident(id) => find_function(self.lowerer.parsed, id.sym.as_ref()).is_some(),
      js::Expr::Cond(c) => self.names_function(&c.cons) && self.names_function(&c.alt),
      _ => false,
    }
  }

  /// The module id `file#export` the browser imports the function `e` from, as an expression, else why the browser cannot reach it. An arrow or a `function` that closes over nothing of this component is hoisted into an export of this file; a function a module declares is named by its file and export; a conditional chooses between two. `None` for anything else, a function held in a local among them.
  fn function_reference(&mut self, e: &'p js::Expr, needs: &mut Vec<Need>) -> Option<Result<Expr, String>> {
    match crate::meaning::bare(e) {
      js::Expr::Arrow(arrow) => Some(self.hoisted_function(arrow.span, closes_over(self.lowerer.parsed, arrow), needs)),
      js::Expr::Fn(f) => Some(self.hoisted_function(f.function.span, closes_over(self.lowerer.parsed, &f.function), needs)),
      js::Expr::Ident(id) => {
        let name = id.sym.as_ref();
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
      js::Expr::Cond(c) => {
        let test = match self.lowerer.expr(&c.test) {
          Ok(test) => test,
          Err(residue) => return Some(Err(format!("and the choice between them reads {}", residue.message))),
        };
        let cons = self.function_reference(&c.cons, needs)?;
        let alt = self.function_reference(&c.alt, needs)?;
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
  fn passes_function(&self, attr: &js::JSXAttr) -> bool {
    let Some(js::JSXAttrValue::JSXExprContainer(c)) = &attr.value else { return false };
    let js::JSXExpr::Expr(e) = &c.expr else { return false };
    match crate::meaning::bare(e) {
      js::Expr::Arrow(_) | js::Expr::Fn(_) => true,
      js::Expr::Ident(id) => self.handler_fns.contains_key(id.sym.as_ref()) || self.browser_function_named(attr).is_some(),
      _ => false,
    }
  }

  /// The setter, dispatcher or inner function an attribute passes by name.
  fn browser_function_named(&self, attr: &js::JSXAttr) -> Option<String> {
    let Some(js::JSXAttrValue::JSXExprContainer(c)) = &attr.value else { return None };
    let js::JSXExpr::Expr(e) = &c.expr else { return None };
    let js::Expr::Ident(id) = crate::meaning::bare(e) else { return None };
    let name = id.sym.as_ref();
    (self.handlers.iter().any(|h| h == name) && !self.lowerer.scope.iter().any(|(bound, _)| bound == name)).then(|| name.to_owned())
  }

  fn handler_prop(&mut self, attr: &'p js::JSXAttr, name: &str) -> Option<Lowered<usize>> {
    let Some(js::JSXAttrValue::JSXExprContainer(c)) = &attr.value else { return None };
    let js::JSXExpr::Expr(e) = &c.expr else { return None };
    let mut inner: &'p js::Expr = e;
    while let js::Expr::Paren(p) = inner {
      inner = &p.expr;
    }
    let function = match inner {
      js::Expr::Arrow(_) | js::Expr::Fn(_) => true,
      js::Expr::Ident(id) => self.handler_fns.contains_key(id.sym.as_ref()),
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
  fn handler_reference(&mut self, attr: &'p js::JSXAttr) -> Option<Expr> {
    let Some(js::JSXAttrValue::JSXExprContainer(c)) = &attr.value else { return None };
    let js::JSXExpr::Expr(e) = &c.expr else { return None };
    let mut root: &js::Expr = e;
    loop {
      match root {
        js::Expr::Paren(p) => root = &p.expr,
        js::Expr::Member(m) => root = &m.obj,
        _ => break,
      }
    }
    let js::Expr::Ident(id) = root else { return None };
    let name = id.sym.as_ref();
    if self.handler_fns.contains_key(name) || !self.lowerer.scope.iter().any(|(bound, _)| bound == name) {
      return None;
    }
    self.lowerer.expr(e).ok()
  }

  /// An `on*` attribute's handler as a lowered body or why it is not one.
  /// A handler is `const`s, calls to state setters and calls to actions, `e.preventDefault()`
  /// aside; the body returns the state it set.
  fn handler_attr(&mut self, attr: &'p js::JSXAttr) -> Lowered<usize> {
    let Some(js::JSXAttrValue::JSXExprContainer(c)) = &attr.value else { return Err(self.lowerer.residue(attr.span, "a handler that is not an expression")) };
    let js::JSXExpr::Expr(e) = &c.expr else { return Err(self.lowerer.residue(attr.span, "an empty handler")) };
    let body = self.handler_body(e)?;
    self.lowered_handlers.push(Handler { event: attr_name(&attr.name)[2..].to_ascii_lowercase(), body });
    Ok(self.lowered_handlers.len() - 1)
  }

  fn handler_body(&mut self, e: &'p js::Expr) -> Lowered<Vec<Stmt>> {
    match e {
      js::Expr::Paren(p) => self.handler_body(&p.expr),
      js::Expr::Arrow(arrow) => {
        let params = arrow.params.clone();
        let body = arrow_body(arrow);
        self.handler_fn(&params, body, arrow.span)
      }
      js::Expr::Ident(id) => {
        let name = id.sym.to_string();
        let Some((params, body)) = self.handler_fns.get(&name).cloned() else {
          return Err(self.lowerer.residue(id.span, format!("`{name}` is not a handler this component declares")));
        };
        self.handler_fn(&params, body, id.span)
      }
      other => Err(self.lowerer.residue(other.span(), "a handler that is not an arrow or a name")),
    }
  }

  fn handler_fn(&mut self, params: &[js::Pat], body: FunctionBody<'p>, span: Span) -> Lowered<Vec<Stmt>> {
    let hoisting = self.lowerer.hoisting.take();
    let was_handler = std::mem::replace(&mut self.lowerer.in_handler, true);
    let result = self.handler_fn_in(params, body, span);
    self.lowerer.hoisting = hoisting;
    self.lowerer.in_handler = was_handler;
    result
  }

  fn handler_fn_in(&mut self, params: &[js::Pat], body: FunctionBody<'p>, span: Span) -> Lowered<Vec<Stmt>> {
    let mut walk = HandlerWalk::default();
    self.handler_fn_into(params, body, &mut walk)?;
    if walk.patch.is_empty() && !holds_act(&walk.out) {
      return Err(self.lowerer.residue(span, "a handler that sets no state and calls no action"));
    }
    walk.out.push(Stmt::Return(Expr::Object(walk.patch)));
    Ok(walk.out)
  }

  /// A handler's statements into `walk`, under the reach it holds: an inlined handler runs under the branch that called it.
  fn handler_fn_into(&mut self, params: &[js::Pat], body: FunctionBody<'p>, walk: &mut HandlerWalk) -> Lowered<()> {
    let depth = self.lowerer.scope.len();
    if let Some(first) = params.first() {
      let js::Pat::Ident(id) = first else { return Err(self.lowerer.residue(first.span(), "a handler's event parameter must be a name")) };
      self.lowerer.scope.push((id.id.sym.to_string(), Expr::Var("$event".to_owned())));
    }
    let result = match body {
      FunctionBody::Expr(e) => self.handler_stmt(e, walk),
      FunctionBody::Block(stmts) => self.handler_block(stmts, walk).map(|_| ()),
    };
    self.lowerer.scope.truncate(depth);
    result
  }

  /// Statements of a handler or of one of its branches. True when the block ended in a bare `return`, which makes what follows the branch reachable only when the branch was not taken.
  fn handler_block(&mut self, stmts: &'p [js::Stmt], walk: &mut HandlerWalk) -> Lowered<bool> {
    for stmt in stmts {
      match stmt {
        js::Stmt::Expr(e) => self.handler_stmt(&e.expr, walk)?,
        js::Stmt::Decl(js::Decl::Var(var)) => {
          for decl in &var.decls {
            let js::Pat::Ident(name) = &decl.name else { return Err(self.lowerer.residue(decl.span, "a destructuring in a handler")) };
            let expr = match decl.init.as_deref() {
              Some(init) => self.lowerer.expr(init)?,
              None => Expr::Lit(Lit::Null),
            };
            let local = name.id.sym.to_string();
            let (name, expr) = match &walk.reach {
              None => (local.clone(), expr),
              Some(reach) => {
                walk.locals += 1;
                (format!("{local}${}", walk.locals), Expr::Ternary(Box::new(reach.clone()), Box::new(expr), Box::new(Expr::Lit(Lit::Null))))
              }
            };
            self.lowerer.scope.push((local, Expr::Var(name.clone())));
            walk.out.push(Stmt::Let { name, expr });
          }
        }
        js::Stmt::Return(r) if r.arg.is_none() => return Ok(true),
        js::Stmt::If(branch) => self.handler_if(branch, walk)?,
        js::Stmt::Try(t) => {
          if t.finalizer.is_some() {
            return Err(self.lowerer.residue(t.span, "a `finally` in a handler"));
          }
          if self.handler_block(&t.block.stmts, walk)? {
            return Ok(true);
          }
        }
        other => return Err(self.lowerer.residue(other.span(), "a statement a handler cannot hold; a handler is `const`s, branches, calls to state setters, writes to the store and calls to actions")),
      }
    }
    Ok(false)
  }

  /// `if`, `else` and `else if`: each branch lowers under its condition joined to the reach around it and the names it declares go out of scope with it. A branch that returns leaves the rest of the handler reachable only when the other one ran.
  fn handler_if(&mut self, branch: &'p js::IfStmt, walk: &mut HandlerWalk) -> Lowered<()> {
    let cond = self.lowerer.expr(&branch.test)?;
    let outer = walk.reach.clone();
    let depth = self.lowerer.scope.len();
    walk.reach = Some(reach_under(&outer, cond.clone()));
    let then_returned = self.handler_branch(&branch.cons, walk)?;
    self.lowerer.scope.truncate(depth);
    let mut else_returned = false;
    if let Some(alt) = &branch.alt {
      walk.reach = Some(reach_under(&outer, Expr::Not(Box::new(cond.clone()))));
      else_returned = self.handler_branch(alt, walk)?;
      self.lowerer.scope.truncate(depth);
    }
    walk.reach = match (then_returned, else_returned) {
      (false, false) => outer,
      (true, false) => Some(reach_under(&outer, Expr::Not(Box::new(cond)))),
      (false, true) => Some(reach_under(&outer, cond)),
      (true, true) => Some(Expr::Lit(Lit::Bool(false))),
    };
    Ok(())
  }

  fn handler_branch(&mut self, stmt: &'p js::Stmt, walk: &mut HandlerWalk) -> Lowered<bool> {
    match stmt {
      js::Stmt::Block(block) => self.handler_block(&block.stmts, walk),
      other => self.handler_block(std::slice::from_ref(other), walk),
    }
  }

  /// `setX(expr)` or `setX((prev) => expr)` adds to the patch; `e.preventDefault()`
  /// and `e.stopPropagation()` are the browser's; `void f()` or `f()` naming a
  /// declared handler inlines it. Under a branch, a key's value is the branch's
  /// where it ran and what the patch held before, or the state as it stands,
  /// where it did not; an action call runs inside an `if` of the same reach.
  fn handler_stmt(&mut self, e: &'p js::Expr, walk: &mut HandlerWalk) -> Lowered<()> {
    match e {
      js::Expr::Paren(p) => self.handler_stmt(&p.expr, walk),
      js::Expr::Unary(u) if u.op == js::UnaryOp::Void => self.handler_stmt(&u.arg, walk),
      js::Expr::Await(a) => self.handler_stmt(&a.arg, walk),
      js::Expr::Call(call) => {
        let js::Callee::Expr(callee) = &call.callee else { return Err(self.lowerer.residue(call.span, "a call a handler cannot make")) };
        match &**callee {
          js::Expr::Ident(id) => {
            let name = id.sym.to_string();
            if let Some((_, state)) = self.setters.iter().find(|(s, _)| *s == name).cloned() {
              let arg = call.args.first().ok_or_else(|| self.lowerer.residue(call.span, format!("`{name}` without a value")))?;
              let value = match &*arg.expr {
                js::Expr::Arrow(arrow) => {
                  let depth = self.lowerer.scope.len();
                  if let Some(js::Pat::Ident(prev)) = arrow.params.first() {
                    self.lowerer.scope.push((prev.id.sym.to_string(), Expr::Var(state.clone())));
                  }
                  let value = match &*arrow.body {
                    js::ArrowFunctionBody::Expr(e) => self.lowerer.expr(e),
                    js::ArrowFunctionBody::FunctionBody(b) => block_to_expr(&mut self.lowerer, &b.stmts),
                  };
                  self.lowerer.scope.truncate(depth);
                  value?
                }
                other => self.lowerer.expr(other)?,
              };
              walk.set(state, value);
              return Ok(());
            }
            if let Some((params, body)) = self.handler_fns.get(&name).cloned() {
              let before = (walk.out.len(), walk.patch.clone());
              let hoisting = self.lowerer.hoisting.take();
              let result = self.handler_fn_into(&params, body, walk);
              self.lowerer.hoisting = hoisting;
              result?;
              if walk.out.len() == before.0 && walk.patch == before.1 {
                return Err(self.lowerer.residue(call.span, "a handler that sets no state and calls no action"));
              }
              return Ok(());
            }
            if let Some(action) = action_alias_of(self.lowerer.parsed, &name) {
              let input = match call.args.first() {
                Some(arg) => self.lowerer.expr(&arg.expr)?,
                None => Expr::Object(Vec::new()),
              };
              walk.act(action, input);
              return Ok(());
            }
            if let Some(store_call) = self.meanings().of_expr(callee).and_then(|meant| meant.store_call()) {
              return self.store_call(store_call, call, walk);
            }
            Err(self.lowerer.residue(id.span, format!("a call to `{name}`, which is not a state setter, a write to the store, an action or a handler this component declares")))
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
              walk.act(action, input);
              return Ok(());
            }
            Err(self.lowerer.residue(call.span, format!("`.{method}()` in a handler; a handler is `const`s, branches, calls to state setters and calls to actions")))
          }
          other => Err(self.lowerer.residue(other.span(), "a call a handler cannot make")),
        }
      }
      other => Err(self.lowerer.residue(other.span(), "a statement a handler cannot hold; a handler is `const`s, branches, calls to state setters and calls to actions")),
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
  fn store_call(&mut self, name: &'static str, call: &'p js::CallExpr, walk: &mut HandlerWalk) -> Lowered<()> {
    match name {
      "set" | "clear" => {
        let key = self.store_key_arg(call)?;
        let value = match (name, call.args.get(1)) {
          ("set", Some(arg)) => self.lowerer.expr(&arg.expr)?,
          ("set", None) => return Err(self.lowerer.residue(call.span, "`set` without a value")),
          _ => Expr::Lit(Lit::Null),
        };
        let state = self.store_state(&key);
        walk.set(state, value);
        Ok(())
      }
      "transaction" => {
        let Some(arg) = call.args.first() else { return Err(self.lowerer.residue(call.span, "`transaction` without a function")) };
        match &*arg.expr {
          js::Expr::Arrow(arrow) => {
            let params = arrow.params.clone();
            self.handler_fn_into(&params, arrow_body(arrow), walk)
          }
          other => Err(self.lowerer.residue(other.span(), "a `transaction` whose work is not an arrow")),
        }
      }
      _ => {
        let key = self.store_key_arg(call)?;
        let Some(guess) = call.args.get(1) else { return Err(self.lowerer.residue(call.span, "`optimistic` without a guess")) };
        let guess = self.lowerer.expr(&guess.expr)?;
        let state = self.store_state(&key);
        walk.set(state, guess);
        let Some(remote) = call.args.get(2) else { return Err(self.lowerer.residue(call.span, "`optimistic` without the action it runs")) };
        match &*remote.expr {
          js::Expr::Arrow(arrow) => {
            let params = arrow.params.clone();
            self.handler_fn_into(&params, arrow_body(arrow), walk)
          }
          other => Err(self.lowerer.residue(other.span(), "an `optimistic` whose remote is not an arrow calling an action")),
        }
      }
    }
  }

  /// The key a store call names, a string the build can read.
  fn store_key_arg(&mut self, call: &'p js::CallExpr) -> Lowered<String> {
    let Some(first) = call.args.first() else { return Err(self.lowerer.residue(call.span, "a store call without a key")) };
    match self.lowerer.expr(&first.expr)? {
      Expr::Lit(Lit::Str(key)) => Ok(key),
      _ => Err(self.lowerer.residue(first.expr.span(), "a store key that is not a string the build can read")),
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
  fn slot_element(&mut self, el: &'p js::JSXElement) -> Lowered<Tmpl> {
    if !self.layout_root {
      return Err(self.lowerer.residue(el.span, "`<Slot>` outside a layout"));
    }
    let mut name = None;
    for attr in &el.opening.attrs {
      let js::JSXAttrOrSpread::JSXAttr(attr) = attr else { return Err(self.lowerer.residue(el.span, "a spread on `<Slot>`")) };
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
  fn link_element(&mut self, el: &'p js::JSXElement) -> Lowered<Tmpl> {
    let attrs = self.jsx_attrs(el)?;
    let children = self.children(&el.children)?;
    placements::link(self, &attrs, children)
  }

  /// Whether an `<img>`'s `src` is `<asset>.src` of an imported image, which
  /// the rewrite lowers as a `Picture`, and not escaped with `data-sf-raw`.
  fn img_names_an_asset(&self, el: &'p js::JSXElement) -> bool {
    let escaped = el.opening.attrs.iter().any(|attr| matches!(attr, js::JSXAttrOrSpread::JSXAttr(attr) if attr_name(&attr.name) == RAW_ESCAPE));
    !escaped && self.img_asset_path(el).is_some()
  }

  /// The imported image an `<img src={x.src}>` names, by its app-relative path.
  fn img_asset_path(&self, el: &'p js::JSXElement) -> Option<String> {
    for attr in &el.opening.attrs {
      let js::JSXAttrOrSpread::JSXAttr(attr) = attr else { continue };
      if attr_name(&attr.name) != "src" {
        continue;
      }
      let Some(js::JSXAttrValue::JSXExprContainer(c)) = &attr.value else { return None };
      let js::JSXExpr::Expr(e) = &c.expr else { return None };
      let js::Expr::Member(m) = &**e else { return None };
      let (js::Expr::Ident(obj), js::MemberProp::Ident(prop)) = (&*m.obj, &m.prop) else { return None };
      if prop.sym.as_ref() != "src" {
        return None;
      }
      return placements::asset_import(&self.lowerer, obj.sym.as_ref());
    }
    None
  }

  /// `<Picture src={hero} alt="…" sizes="…" priority widths={[…]} quality={…} source="cms">`,
  /// or an `<img>` of an imported asset under the rewrite: a `<picture>`
  /// with a `<source>` per format and the hashed original as its `<img>`,
  /// an `<img>` alone for an image served as it is, and for a string `src`
  /// an `<img>` whose `srcset` a named source's template writes.
  fn picture_element(&mut self, el: &'p js::JSXElement, from_img: bool) -> Lowered<Tmpl> {
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
  fn boundary(&mut self, which: &str, el: &'p js::JSXElement) -> Lowered<Tmpl> {
    let children = Tmpl::Fragment(self.children(&el.children)?);
    let kind = if which == "Suspense" { "$" } else { "&" };
    let boundary = Tmpl::Element { tag: snapfire_fsr_ir::render::BOUNDARY_TAG.to_owned(), attrs: vec![Entry::Field("kind".to_owned(), Expr::lit_str(kind))], children: vec![children] };
    if which == "Suspense" {
      return Ok(boundary);
    }
    let mode = el.opening.attrs.iter().find_map(|attr| match attr {
      js::JSXAttrOrSpread::JSXAttr(attr) if attr_name(&attr.name) == "mode" => Some(attr),
      _ => None,
    });
    let Some(mode) = mode else { return Ok(boundary) };
    let mode = self.attr_value(mode)?;
    Ok(Tmpl::If { cond: Expr::Compare(CompareOp::Ne, Box::new(mode), Box::new(Expr::lit_str("hidden"))), then: Box::new(boundary), r#else: None })
  }

  /// `as_child` says the element sits among JSX children, where a keyed
  /// placement's wrap must be braced.
  fn component_ref(&mut self, name: &str, el: &'p js::JSXElement, as_child: bool) -> Lowered<Tmpl> {
    if name == "Fragment" || name == "React.Fragment" {
      return Ok(Tmpl::Fragment(self.children(&el.children)?));
    }
    if let Some(crate::meaning::Meaning::Library(crate::meaning::Construct::Boundary(which))) = self.meanings().of_tag(&el.opening.name) {
      return self.boundary(which, el);
    }
    match self.placement(el) {
      Some("Island") => return self.island_element(el),
      Some("Slot") => return self.slot_element(el),
      Some("Link") => return self.link_element(el),
      Some("Picture") => return self.picture_element(el, false),
      Some("Mount") => return self.mount_element(el),
      Some("Embed") => {
        let attrs = self.jsx_attrs(el)?;
        return placements::embed(self, &attrs, At::Span(el.span));
      }
      _ => {}
    }
    let chosen = self.chosen.get(name).cloned();
    if let (js::JSXElementName::Ident(id), None) = (&el.opening.name, &chosen) {
      let meanings = self.meanings();
      if id.ctxt != meanings.top && id.ctxt != meanings.unresolved {
        return Err(self.lowerer.residue(el.opening.name.span(), format!("`{name}` is not a component this file declares or imports")));
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
    for attr in &el.opening.attrs {
      let attr = match attr {
        js::JSXAttrOrSpread::JSXAttr(attr) => attr,
        js::JSXAttrOrSpread::SpreadElement(spread) => {
          props.push(Entry::Spread(self.lowerer.expr(&spread.expr)?));
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
        continue;
      }
      if let Some(name) = self.browser_function_named(attr) {
        if !props.iter().any(|e| matches!(e, Entry::Field(n, _) if n == UNLOWERED_ATTR)) {
          let (line, column) = self.lowerer.parsed.loc(attr.span.lo);
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
            let (line, column) = self.lowerer.parsed.loc(attr.span.lo);
            let wrap = match &attr.value {
              Some(js::JSXAttrValue::JSXExprContainer(js::JSXExprContainer { expr: js::JSXExpr::Expr(e), .. })) => Some(self.lowerer.parsed.range(e.span())),
              _ => None,
            };
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
    for attr in &el.opening.attrs {
      let js::JSXAttrOrSpread::JSXAttr(attr) = attr else { continue };
      let Some(js::JSXAttrValue::JSXExprContainer(js::JSXExprContainer { expr: js::JSXExpr::Expr(e), .. })) = &attr.value else { continue };
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
    let loc = self.lowerer.parsed.loc(el.span.lo);
    let at = self.lowerer.parsed.range(el.opening.name.span()).end;
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

  fn attr_value(&mut self, attr: &'p js::JSXAttr) -> Lowered<Expr> {
    match &attr.value {
      None => Ok(Expr::Lit(Lit::Bool(true))),
      Some(js::JSXAttrValue::Str(s)) => Ok(Expr::Lit(Lit::Str(decode_entities(&s.value.to_atom_lossy())))),
      Some(js::JSXAttrValue::JSXExprContainer(c)) => match &c.expr {
        js::JSXExpr::Expr(e) if self.holds_markup(e) => Ok(Expr::Markup(std::sync::Arc::new(self.child_expr(e)?))),
        js::JSXExpr::Expr(e) => {
          self.premake_expr(e)?;
          self.lowerer.expr(e)
        }
        js::JSXExpr::JSXEmptyExpr(e) => Err(self.lowerer.residue(e.span, "an empty attribute expression")),
      },
      Some(other) => Err(self.lowerer.residue(other.span(), "an element as an attribute value")),
    }
  }

  /// `style={{ a: x, bTwo: y }}` as an object keyed by CSS name, which the
  /// renderer serialises the way React does.
  fn style(&mut self, attr: &'p js::JSXAttr) -> Lowered<Expr> {
    let attr = self.jsx_attr(attr)?;
    placements::style(self, &attr)
  }

  /// An attribute as the shared placements read it.
  fn jsx_attr(&self, attr: &'p js::JSXAttr) -> Lowered<Attr<'p>> {
    let value = match &attr.value {
      None => Value::Present,
      Some(js::JSXAttrValue::Str(s)) => Value::Text(decode_entities(&s.value.to_atom_lossy())),
      Some(js::JSXAttrValue::JSXExprContainer(c)) => match &c.expr {
        js::JSXExpr::Expr(e) => Value::Script(Cow::Borrowed(&**e)),
        js::JSXExpr::JSXEmptyExpr(e) => return Err(self.lowerer.residue(e.span, "an empty attribute expression")),
      },
      Some(other) => return Err(self.lowerer.residue(other.span(), "an element as an attribute value")),
    };
    Ok(Attr { name: attr_name(&attr.name), value, at: At::Span(attr.span) })
  }

  /// An element's attributes as the shared placements read them, handlers
  /// left out: they belong to the browser.
  fn jsx_attrs(&self, el: &'p js::JSXElement) -> Lowered<Vec<Attr<'p>>> {
    let mut out = Vec::new();
    for attr in &el.opening.attrs {
      match attr {
        js::JSXAttrOrSpread::JSXAttr(attr) if is_handler_name(&attr_name(&attr.name)) => {}
        js::JSXAttrOrSpread::JSXAttr(attr) => out.push(self.jsx_attr(attr)?),
        js::JSXAttrOrSpread::SpreadElement(spread) => out.push(Attr { name: String::new(), value: Value::Spread(Cow::Borrowed(&*spread.expr)), at: At::Span(spread.expr.span()) }),
      }
    }
    Ok(out)
  }

  fn children(&mut self, children: &'p [js::JSXElementChild]) -> Lowered<Vec<Tmpl>> {
    let mut out = Vec::new();
    for child in children {
      match child {
        js::JSXElementChild::JSXText(text) => {
          let cleaned = jsx_text(&text.value.to_atom_lossy());
          if !cleaned.is_empty() {
            out.push(Tmpl::Text(cleaned));
          }
        }
        js::JSXElementChild::JSXExprContainer(c) => match &c.expr {
          js::JSXExpr::JSXEmptyExpr(_) => {}
          js::JSXExpr::Expr(e) => {
            self.lowerer.browser_value = None;
            match self.child_expr(e) {
              Ok(child) => out.push(child),
              // Text only the browser knows: the server writes nothing and the browser copy computes it once mounted, so its first render matches. Markup the value would choose is structure, which stays a residue.
              Err(residue) => match self.lowerer.browser_value.take() {
                Some(reason) if !self.holds_markup(e) => {
                  let (line, column) = self.lowerer.parsed.loc(e.span().lo);
                  self.browser_texts.push(BrowserText { wrap: Some(self.lowerer.parsed.range(e.span())), line, column, reason });
                  out.push(Tmpl::Expr(Expr::Var(BROWSER_VAR.to_owned())));
                }
                _ => return Err(residue),
              },
            }
          }
        },
        js::JSXElementChild::JSXElement(el) => out.push(self.element(el, true)?),
        js::JSXElementChild::JSXFragment(frag) => out.push(Tmpl::Fragment(self.children(&frag.children)?)),
        js::JSXElementChild::JSXSpreadChild(s) => return Err(self.lowerer.residue(s.span, "a spread child")),
      }
    }
    Ok(out)
  }
}

fn attr_name(name: &js::JSXAttrName) -> String {
  match name {
    js::JSXAttrName::Ident(id) => id.sym.to_string(),
    js::JSXAttrName::JSXNamespacedName(n) => format!("{}:{}", n.ns.sym, n.name.sym),
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
fn optional_map(expr: &js::Expr) -> Option<(&js::Expr, &js::ArrowExpr)> {
  let js::Expr::OptChain(chain) = expr else { return None };
  let js::OptChainBase::Call(call) = &*chain.base else { return None };
  let js::Expr::OptChain(callee) = &*call.callee else { return None };
  let js::OptChainBase::Member(member) = &*callee.base else { return None };
  if !callee.optional || !matches!(&member.prop, js::MemberProp::Ident(id) if id.sym.as_ref() == "map") {
    return None;
  }
  let js::Expr::Arrow(arrow) = &*call.args.first()?.expr else { return None };
  Some((&*member.obj, arrow))
}

/// `xs.map(f)` with `f` an arrow written in place: the array and the callback.
fn map_call(call: &js::CallExpr) -> Option<(&js::Expr, &js::ArrowExpr)> {
  let js::Callee::Expr(callee) = &call.callee else { return None };
  let js::Expr::Member(member) = &**callee else { return None };
  if !matches!(&member.prop, js::MemberProp::Ident(id) if id.sym.as_ref() == "map") {
    return None;
  }
  let js::Expr::Arrow(arrow) = &*call.args.first()?.expr else { return None };
  Some((&*member.obj, arrow))
}

#[cfg(test)]
mod tests {
  use super::*;
  use snapfire_fsr_ir::ast::CompareOp;

  fn app(files: &[(&str, &str)]) -> PathBuf {
    static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let dir = std::env::temp_dir().join(format!("fsr_component_{}_{}", std::process::id(), NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)));
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
      Tmpl::Text(_) | Tmpl::Marker(_) | Tmpl::Html(_) | Tmpl::Expr(_) | Tmpl::Slot(_) => {}
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
import { Island } from "@snapfire/fsr-client/react";
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
import { useStore } from "@snapfire/fsr-client/react";
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
      ("src/ui/Shell.tsx", "import Card from \"./Card\";\nclass Clock {}\nexport default function Shell({ total }: { total: number }) {\n  const clock = new Clock();\n  return <section data-clock={String(clock)}><Card total={total} /></section>;\n}\n"),
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
import { Island } from "@snapfire/fsr-client/react";
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
import { action } from "@snapfire/fsr-client";
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
import { Island } from "@snapfire/fsr-client/react";
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
import { action } from "@snapfire/fsr-client";
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
import { Island } from "@snapfire/fsr-client/react";
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
import { key } from "@snapfire/fsr-client/store";
export const size = key<string>("order/size");
export const price = key<number>("order/price");
export const qty = key<number>("order/qty");
"#,
      ),
      (
        "src/Sizes.tsx",
        r#"
import { useStore } from "@snapfire/fsr-client/react";
import { set, transaction } from "@snapfire/fsr-client/store";
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
import { useStore } from "@snapfire/fsr-client/react";
import { optimistic } from "@snapfire/fsr-client/store";
import { action } from "@snapfire/fsr-client";
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
    assert_eq!(stepper.handlers[1].body, vec![Stmt::Act { action: "order.setQty".to_owned(), input: Expr::Object(vec![Entry::Field("qty".to_owned(), down.clone())]) }, Stmt::Return(Expr::Object(vec![Entry::Field("count".to_owned(), down)]))], "the try's catch is the browser's; the server refuses the patch when the action fails");
  }

  #[test]
  fn handlers_lower_to_state_patches_and_an_island_takes_a_mode() {
    let files = [
      (
        "routes/index/page.tsx",
        r#"
import { Island, island } from "@snapfire/fsr-client/react";
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
import { Island } from "@snapfire/fsr-client/react";
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
  const PANEL_ON_STATE: &str = "import { useStore } from \"@snapfire/fsr-client/react\";\nimport { Panel } from \"@src/Panel\";\nexport default function Live() {\n  const [n] = useStore(\"demo/n\", 0);\n  return <main><Panel n={n} /></main>;\n}\n";

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
import { Island, island } from "@snapfire/fsr-client/react";
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
        "import { Slot } from \"@snapfire/fsr-client/react\";\nexport default function Layout({ children, feed, title }: { children: unknown; feed: unknown; title: string }) {\n  return <div><h1>{title}</h1>{children}<aside>{feed}</aside><Slot name=\"modal\" /></div>;\n}\n",
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
    let page = [("routes/a/page.tsx", "import { Slot } from \"@snapfire/fsr-client/react\";\nexport default function A() {\n  return <Slot name=\"x\" />;\n}\n")];
    let err = lower(&page, "routes/a/page.tsx#default").unwrap_err().to_string();
    assert!(err.contains("outside a layout"), "{err}");
  }

  #[test]
  fn a_slot_placement_carries_its_fallback_by_element_and_by_prop() {
    let files = [(
      "routes/layout.tsx",
      "import { Slot } from \"@snapfire/fsr-client/react\";\nexport default function L({ children, feed }: { children: unknown; feed: unknown }) {\n  return <div>{children}{feed ?? <p>no feed</p>}<Slot name=\"modal\"><p>closed</p></Slot></div>;\n}\n",
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
        "import { useStore } from \"@snapfire/fsr-client/react\";\nimport { cartCount } from \"../../src/store\";\nexport default function P() {\n  const [items, setItems] = useStore(cartCount, 0);\n  const [name] = useStore(\"user/name\", \"guest\");\n  return <p onClick={() => setItems(items + 1)}>{name}{items}</p>;\n}\n",
      ),
      ("src/store.ts", "import { key } from \"@snapfire/fsr-client/store\";\nexport const cartCount = key<number>(\"cart/count\");\n"),
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
      "import { useStore } from \"@snapfire/fsr-client/react\";\nexport default function P({ id }: { id: string }) {\n  const [n] = useStore(id, 0);\n  return <p>{n}</p>;\n}\n",
    )];
    let err = lower(&files, "routes/index/page.tsx#default").unwrap_err().to_string();
    assert!(err.contains("`useStore` key"), "{err}");
  }

  #[test]
  fn use_locale_lowers_to_the_locale_read() {
    let files = [(
      "routes/help/page.tsx",
      "import { useLocale } from \"@snapfire/fsr-client/react\";\nexport default function Help() {\n  const locale = useLocale();\n  return <p lang={locale}>{locale === \"fr_FR\" ? \"Bonjour\" : \"Hello\"}</p>;\n}\n",
    )];
    let lowered = lower(&files, "routes/help/page.tsx#default").unwrap();
    let component = &lowered[0].1;
    assert_eq!(component.body, vec![Stmt::Let { name: "locale".to_owned(), expr: Expr::Locale }]);
    let Tmpl::Element { attrs, .. } = &component.render else { panic!("{:?}", component.render) };
    assert_eq!(plain(attrs), [Entry::Field("lang".to_owned(), Expr::var("locale"))]);
  }

  #[test]
  fn a_link_lowers_to_an_anchor_the_navigator_reads() {
    let page = [("routes/a/page.tsx", "import { Link } from \"@snapfire/fsr-client/react\";\nexport default function A({ id }: { id: number }) {\n  return <p><Link href={`/photo/${id}`} className=\"x\" full>full</Link><Link href=\"/photo/1\" into=\"modal\" prefetch=\"none\">quick</Link></p>;\n}\n")];
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
    let page = [("routes/a/page.tsx", "import { Link } from \"@snapfire/fsr-client/react\";\nexport default function A({ on }: { on: boolean }) {\n  return <p><Link href=\"?at=1\" keep={false}>a</Link><Link href=\"?at=2\" keep>b</Link><Link href=\"?at=3\" keep={on}>c</Link></p>;\n}\n")];
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
    let page = [("routes/a/page.tsx", "import { Link } from \"@snapfire/fsr-client/react\";\nexport default function A() {\n  return <p><Link href=\"/billing\">a</Link><Link href=\"/billing\" match=\"prefix\">b</Link><Link href=\"/billing\" match=\"none\">c</Link><Link href=\"/billing\" aria-current=\"page\">d</Link></p>;\n}\n")];
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
    let page = [("routes/a/page.tsx", "import { Link } from \"@snapfire/fsr-client/react\";\nexport default function A() {\n  return <p><Link href=\"/agents\" match=\"prefix\" current=\"document\">a</Link><Link href=\"/help\" current=\"url\">b</Link><Link href=\"/help\" current=\"page\">c</Link></p>;\n}\n")];
    let err = lower(&page, "routes/a/page.tsx#default").unwrap_err().to_string();
    assert!(err.contains("`current` is \"url\" or \"document\""), "{err}");
    let page = [("routes/a/page.tsx", "import { Link } from \"@snapfire/fsr-client/react\";\nexport default function A() {\n  return <p><Link href=\"/agents\" match=\"prefix\" current=\"document\">a</Link><Link href=\"/help\" current=\"url\">b</Link></p>;\n}\n")];
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
    let page = [("routes/a/page.tsx", "import { Link } from \"@snapfire/fsr-client/react\";\nexport default function A({ on }: { on: boolean }) {\n  return <Link href=\"/x\" match={on ? \"prefix\" : \"none\"}>a</Link>;\n}\n")];
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
    let element = [("routes/a/page.tsx", "import { Island } from \"@snapfire/fsr-client/react\";\nexport default function A() {\n  return <Island when=\"visible\"><p>x</p></Island>;\n}\n")];
    let err = lower(&element, "routes/a/page.tsx#default").unwrap_err().to_string();
    assert!(err.contains("an island must be a component"), "{err}");
    let timing = [("routes/b/page.tsx", "import { Island } from \"@snapfire/fsr-client/react\";\nimport { Help } from \"../../src/ui/Help\";\nexport default function B({ n }: { n: number }) {\n  return <Island when={n > 0 ? \"visible\" : \"load\"}><Help /></Island>;\n}\n"), ("src/ui/Help.tsx", "export function Help() {\n  return <p>help</p>;\n}\n")];
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
      ("src/reserved.ts", "import { createContext } from \"@snapfire/fsr-authoring/template\";\nexport const Reserved = createContext<string[]>([]);\n"),
      ("src/Row.tsx", "import { use } from \"@snapfire/fsr-authoring/template\";\nimport { Reserved } from \"./reserved\";\nexport function Row({ id }: { id: string }) {\n  return <li>{use(Reserved).includes(id) ? \"kept\" : \"free\"}</li>;\n}\n"),
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
    let files = [("routes/a/page.tsx", "import { use } from \"@snapfire/fsr-authoring/template\";\nexport default function A({ theme }: { theme: string }) {\n  return <p>{use(theme)}</p>;\n}\n")];
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
    let layout = "import type { ReactNode } from \"react\";\nimport { tree } from \"@snapfire/fsr-client/react\";\nfunction Layout({ children }: { children: ReactNode }) {\n  return <main>{children}</main>;\n}\nexport default tree(Layout);\n";
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

fn lower_derive(parsed: &Parsed, defaults: &SessionDefaults, resolved: &Resolved, call: &js::CallExpr, nested: bool) -> Result<LoweredDerive, DeriveFailure> {
  let mut lowerer = Lowerer::new(parsed, defaults).resolved(resolved);
  let refused = |lowerer: &mut Lowerer<'_>, residue: Residue, compute: Option<(String, Vec<String>)>| DeriveFailure { residue, unbound: lowerer.unbound.take(), compute };
  let ([k, s, c], false) = (&call.args[..], call.args.iter().any(|a| a.spread.is_some())) else {
    return Err(refused(&mut lowerer, parsed.residue(call.span, "`derive` takes a key, its sources and a compute"), None));
  };
  let key = match lowerer.expr(&k.expr) {
    Ok(Expr::Lit(Lit::Str(key))) => key,
    Ok(_) => return Err(refused(&mut lowerer, parsed.residue(k.expr.span(), "a derived key the build cannot read; name it with `key()` or a string"), None)),
    Err(residue) => return Err(refused(&mut lowerer, residue, None)),
  };
  let js::Expr::Array(list) = &*s.expr else {
    return Err(refused(&mut lowerer, parsed.residue(s.expr.span(), "a derive's sources the build cannot read; list them in the call"), None));
  };
  let mut sources = Vec::new();
  for element in &list.elems {
    let Some(element) = element.as_ref().filter(|e| e.spread.is_none()) else {
      return Err(refused(&mut lowerer, parsed.residue(list.span, "a derive's sources the build cannot read; list them in the call"), None));
    };
    match lowerer.expr(&element.expr) {
      Ok(Expr::Lit(Lit::Str(source))) => sources.push(source),
      Ok(_) => return Err(refused(&mut lowerer, parsed.residue(element.expr.span(), "a derive source the build cannot read; name it with `key()` or a string"), None)),
      Err(residue) => return Err(refused(&mut lowerer, residue, None)),
    }
  }
  let known = Some((key.clone(), sources.clone()));
  if nested {
    return Ok(LoweredDerive { key, sources, body: None, at: None, why: Some("written inside a function, so it runs only when the browser calls that function".to_owned()) });
  }
  let (params, stmts): (Vec<&js::Pat>, FunctionBody<'_>) = match &*c.expr {
    js::Expr::Arrow(arrow) if !arrow.is_async && !arrow.is_generator => (arrow.params.iter().collect(), arrow_body(arrow)),
    js::Expr::Fn(f) if !f.function.is_async && !f.function.is_generator && f.function.body.is_some() => (f.function.params.iter().map(|p| &p.pat).collect(), FunctionBody::Block(&f.function.body.as_ref().unwrap().stmts)),
    other => return Err(refused(&mut lowerer, parsed.residue(other.span(), "a compute that is not a function written in the call"), known)),
  };
  lowerer.render_path = true;
  match &params[..] {
    [] => {}
    [js::Pat::Ident(read)] => lowerer.reader = Some(read.id.to_id()),
    _ => return Err(refused(&mut lowerer, parsed.residue(c.expr.span(), "a compute whose parameters are not one `read`"), known)),
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

struct DeriveCalls<'a> {
  meanings: &'a crate::meaning::Meanings,
  calls: Vec<js::CallExpr>,
}

impl swc_core::ecma::visit::Visit for DeriveCalls<'_> {
  fn visit_call_expr(&mut self, call: &js::CallExpr) {
    if let js::Callee::Expr(callee) = &call.callee {
      if self.meanings.of_expr(callee) == Some(crate::meaning::Meaning::Library(crate::meaning::Construct::Derive)) {
        self.calls.push(call.clone());
      }
    }
    call.visit_children_with(self);
  }
}

struct StoreCalls<'a> {
  meanings: &'a crate::meaning::Meanings,
  args: Vec<js::Expr>,
}

impl swc_core::ecma::visit::Visit for StoreCalls<'_> {
  fn visit_call_expr(&mut self, call: &js::CallExpr) {
    if let js::Callee::Expr(callee) = &call.callee {
      if self.meanings.of_expr(callee).and_then(|meant| meant.hook()) == Some("useStore") {
        if let Some(first) = call.args.first() {
          self.args.push((*first.expr).clone());
        }
      }
    }
    call.visit_children_with(self);
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

fn unwrap_literal(expr: &js::Expr) -> &js::Expr {
  match expr {
    js::Expr::Paren(inner) => unwrap_literal(&inner.expr),
    js::Expr::TsAs(inner) => unwrap_literal(&inner.expr),
    js::Expr::TsSatisfies(inner) => unwrap_literal(&inner.expr),
    js::Expr::TsConstAssertion(inner) => unwrap_literal(&inner.expr),
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
      Tmpl::Text(_) | Tmpl::Marker(_) | Tmpl::Html(_) | Tmpl::Expr(_) | Tmpl::Slot(_) => {}
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
      Tmpl::Text(_) | Tmpl::Marker(_) | Tmpl::Html(_) | Tmpl::Expr(_) | Tmpl::Slot(_) => {}
    }
  }

  /// Whether `t` places a slot, which needs the frame of the placement around it.
  fn reads_slots(t: &Tmpl) -> bool {
    match t {
      Tmpl::Slot(_) => true,
      Tmpl::Text(_) | Tmpl::Marker(_) | Tmpl::Html(_) | Tmpl::Expr(_) => false,
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
      Tmpl::For { body, .. } | Tmpl::Let { then: body, .. } => place(body, held),
      Tmpl::Text(_) | Tmpl::Marker(_) | Tmpl::Html(_) | Tmpl::Slot(_) => {}
    }
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
      Tmpl::Text(_) | Tmpl::Marker(_) | Tmpl::Html(_) | Tmpl::Expr(_) | Tmpl::Slot(_) => {}
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
      Tmpl::Text(_) | Tmpl::Marker(_) | Tmpl::Html(_) | Tmpl::Expr(_) | Tmpl::Slot(_) => {}
    }
  }

  /// Whether `e` can evaluate to a function value: a lambda anywhere in it other than the callback a builtin or a call applies.
  pub(super) fn holds_function(e: &Expr) -> bool {
    let mut callbacks: Vec<*const Expr> = Vec::new();
    e.visit(&mut |x| match x {
      Expr::Map(_, f) | Expr::Filter(_, f) | Expr::Find(_, f) | Expr::FindIndex(_, f) | Expr::Some(_, f) | Expr::Every(_, f) | Expr::Sort(_, f) | Expr::FlatMap(_, f) | Expr::Reduce(_, _, f) | Expr::ReplaceWith(_, _, f) | Expr::Apply { f, .. } => callbacks.push(&**f as *const Expr),
      _ => {}
    });
    let mut found = false;
    e.visit(&mut |x| found |= matches!(x, Expr::Lambda { .. }) && !callbacks.contains(&(x as *const Expr)));
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
      Tmpl::Text(_) | Tmpl::Marker(_) | Tmpl::Html(_) | Tmpl::Expr(_) | Tmpl::Slot(_) => Ok(()),
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
      Tmpl::Text(_) | Tmpl::Marker(_) | Tmpl::Html(_) | Tmpl::Expr(_) | Tmpl::Slot(_) => {}
    }
  }

}
