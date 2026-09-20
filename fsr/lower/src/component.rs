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

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::rc::Rc;

use snapfire_compiler_wire::Described;
use snapfire_fsr_ir::ast::{Builtin, CompareOp, Component, Consts, Entry, Expr, Handler, Lit, LogicOp, Stmt, Tmpl};
use snapfire_fsr_ir::render::{html_attr_name, HANDLER_ATTR, KEY_ATTR, RAW_ATTR, SERVER_MODE, UNLOWERED_ATTR};
use snapfire_fsr_ir::Reach;
use swc_core::common::{Span, Spanned};
use swc_core::ecma::ast as js;

use crate::hoist::{self, Candidates, Hook, Rewrite};
use crate::{lower_actions_in, lower_handlers_in, lower_loader_in, lower_middleware_in, lower_of_data_in, lower_paths_in, parse_with, prop_name, Lowered, LowerError, Lowerer, LoweredAction, LoweredHandler, Parsed, Placement, Resolved, Residue, SessionDefaults, Unresolved, EXT_DIR, STD_SPECIFIER};
use snapfire_fsr_ir::Body;

/// The cursor over one application: parsed files, finished components and the
/// resolution stack that turns recursion into a diagnostic.
pub struct ComponentSet {
  app: PathBuf,
  parsed: HashMap<String, Rc<Parsed>>,
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
  /// Foreign components the plugin described, by file, which the set lowers
  /// through the framework's own front end instead of leaving foreign.
  described: HashMap<String, Described>,
  /// Described components that did not lower, with why: each stays foreign
  /// and the report says so.
  pub foreign_residue: Vec<(String, Residue)>,
  /// Files the plugin refused to describe, with what it said, so a placement
  /// of one is foreign for that reason.
  undescribed: HashMap<String, String>,
  /// Set when a component was met while it was still being lowered. Every
  /// hydrate and purity verdict read across that cycle is provisional until
  /// [`Self::settle`].
  cyclic: bool,
  /// Per module with a purity verdict, whether it holds no browser state: the
  /// half of the verdict that does not depend on the components it renders.
  stateless: HashMap<String, bool>,
  /// Per lowered module, whether it keys a hoist, a chunk or a region
  /// anywhere below it, which makes a placement of it keyed. A module met in
  /// a cycle before its verdict is in counts as keyed, which both halves
  /// agree on since the flag is decided once.
  keys: HashMap<String, bool>,
  /// Custom element tags with a shadow template, to the template's module. A
  /// placement of one is marked, so the server writes the template inside it.
  elements: Rc<HashMap<String, String>>,
}

impl ComponentSet {
  pub fn new(app: &Path) -> Self {
    Self { app: app.to_path_buf(), parsed: HashMap::new(), provided: HashMap::new(), consts: Consts::new(), defaults: SessionDefaults::new(), components: Vec::new(), failed: HashMap::new(), resolving: Vec::new(), layouts: Vec::new(), slots: Vec::new(), rewrites: Vec::new(), pure: HashMap::new(), natives: Vec::new(), remaining: Vec::new(), foreign: Vec::new(), described: HashMap::new(), foreign_residue: Vec::new(), undescribed: HashMap::new(), cyclic: false, stateless: HashMap::new(), keys: HashMap::new(), elements: Rc::default() }
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

  /// The plugin refused to describe `file` and said `why`: a placement of it
  /// stays foreign for that reason.
  pub fn undescribed(&mut self, file: impl Into<String>, why: impl Into<String>) {
    self.undescribed.insert(file.into(), why.into());
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
    self.resolving_loop(file, |parsed, defaults, resolved| lower_of_data_in(parsed, defaults, resolved, "meta"))
  }

  /// The loader module's `store`, when it exports one.
  pub fn lower_store(&mut self, file: &str) -> Result<Option<Body>, LowerError> {
    self.resolving_loop(file, |parsed, defaults, resolved| lower_of_data_in(parsed, defaults, resolved, "store"))
  }

  /// The page loader module's `paths`, when it exports one.
  pub fn lower_paths(&mut self, file: &str) -> Result<Option<Body>, LowerError> {
    self.resolving_loop(file, |parsed, defaults, resolved| lower_paths_in(parsed, defaults, resolved))
  }

  pub fn lower_actions(&mut self, file: &str) -> Result<Vec<LoweredAction>, LowerError> {
    self.resolving_loop(file, |parsed, defaults, resolved| lower_actions_in(parsed, defaults, resolved))
  }

  pub fn lower_handlers(&mut self, file: &str) -> Result<Vec<LoweredHandler>, LowerError> {
    self.resolving_loop(file, |parsed, defaults, resolved| lower_handlers_in(parsed, defaults, resolved))
  }

  pub fn lower_middleware(&mut self, file: &str) -> Result<Body, LowerError> {
    self.resolving_loop(file, |parsed, defaults, resolved| lower_middleware_in(parsed, defaults, resolved))
  }

  /// Runs `lower` over `file`, binding each module-level name it could not
  /// resolve through this set and trying again, until it lowers or a name
  /// cannot be bound.
  fn resolving_loop<T>(&mut self, file: &str, lower: impl Fn(&Parsed, &SessionDefaults, &Resolved) -> Result<T, Unresolved>) -> Result<T, LowerError> {
    self.load(file)?;
    let mut resolved = Resolved { globals: Vec::new(), natives: self.natives.clone() };
    loop {
      let parsed = self.parsed[file].clone();
      let defaults = self.defaults.clone();
      match lower(&parsed, &defaults, &resolved) {
        Ok(done) => return Ok(done),
        Err((error, Some(name))) if !resolved.globals.iter().any(|(n, _)| *n == name) => match self.global(file, &name)? {
          Some((expr, key)) => {
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

  /// Every file with a rewrite, with its rewritten source.
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

  pub fn rewritten(&self) -> Vec<(String, String)> {
    let mut files: Vec<&str> = Vec::new();
    for rewrite in &self.rewrites {
      if !files.contains(&rewrite.file.as_str()) {
        files.push(&rewrite.file);
      }
    }
    files
      .into_iter()
      .filter_map(|file| {
        let parsed = self.parsed.get(file)?;
        let source = parsed.cm.files().first().map(|f| f.src.to_string())?;
        let rewrites: Vec<&Rewrite> = self.rewrites.iter().filter(|r| r.file == file).collect();
        Some((file.to_owned(), hoist::apply(&source, &rewrites)))
      })
      .collect()
  }

  /// Lowers `module` (a `path#export` under the app) and everything it
  /// renders. A module already lowered is not read again and one that did
  /// not lower answers with the same error. A module still
  /// being lowered is a component that renders itself, directly or through
  /// others: the call is the renderer's, which looks the module up by name.
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
        let result = self.lower_vue(file, export);
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

  /// Brings every hydrate and purity verdict to what the whole graph says.
  /// Lowering read a component still in progress as neither hydrating nor
  /// pure, which is low for anything on a cycle: hydrate rises to the least
  /// verdict that holds, purity falls from `stateless` to the greatest. The
  /// chunks hoisting chose with the provisional verdicts stay, since a
  /// component read as impure only keeps a subtree out of a chunk.
  fn settle(&mut self) {
    loop {
      let rising: Vec<usize> = (0..self.components.len()).filter(|&i| self.components[i].1.hydrated_by.is_none() && self.inline_hydrates(&self.components[i].1.render)).collect();
      if rising.is_empty() {
        break;
      }
      for i in rising {
        self.components[i].1.hydrated_by = Some(snapfire_fsr_ir::HydratedBy::React);
      }
    }
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
    let parsed = match self.provided.get(file) {
      Some(source) => Ok(source.clone()),
      None => std::fs::read_to_string(self.app.join(file)).map_err(|e| LowerError::Parse { file: file.to_owned(), message: e.to_string() }),
    }
    .and_then(|source: String| parse_with(file, &source, file.ends_with(".tsx")));
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

  /// The file an import names relative to the app, through an alias or a
  /// relative path; `None` for a bare or generated specifier the build does
  /// not follow.
  fn resolve_import(&self, from: &str, source: &str) -> Option<String> {
    let joined = crate::resolve_specifier(from, source)?;
    for candidate in [joined.clone(), format!("{joined}.tsx"), format!("{joined}.ts"), format!("{joined}/index.tsx"), format!("{joined}/index.ts")] {
      if candidate.starts_with("generated/") && candidate != crate::HEAD_MODULE {
        continue;
      }
      if self.provided.contains_key(&candidate) || self.app.join(&candidate).is_file() {
        return Some(candidate);
      }
    }
    None
  }

  fn lower_loaded(&mut self, file: &str, export: &str) -> Result<Component, LowerError> {
    let mut globals: Vec<(String, Expr)> = Vec::new();
    let module = format!("{file}#{export}");
    // An element template has no browser half, so nothing would read what
    // hoisting records or rewrites.
    let element_template = self.elements.values().any(|m| *m == module);
    let tree = match tree_target(&self.parsed[file].clone()) {
      Ok(found) => found.filter(|_| export == "default"),
      Err((span, message)) => return Err(self.parsed[file].residue(span, message).into()),
    };
    let ((component, refs, providers), hoisting) = loop {
      let (result, unbound, hoisting) = {
        let parsed = self.parsed[file].clone();
        let function = find_function(&parsed, export).ok_or_else(|| LowerError::MissingExport { file: file.to_owned(), export: export.to_owned() })?;
        let defaults = self.defaults.clone();
        let mut lowerer = Lowerer::new(&parsed, &defaults);
        lowerer.globals = globals.clone();
        lowerer.natives = self.natives.clone();
        lowerer.hoisting = (!element_template).then(Candidates::default);
        let layout_root = self.layouts.iter().any(|m| *m == module);
        let slot_names = self.slots.iter().find(|(m, _)| *m == module).map(|(_, names)| names.clone()).unwrap_or_default();
        let mut cl = ComponentLowerer { lowerer, file, handlers: Vec::new(), refs: Vec::new(), select_value: None, props_name: None, children_name: None, layout_root, slot_names, slot_props: Vec::new(), state: Vec::new(), hook: None, state_bindings: Vec::new(), setters: Vec::new(), handler_fns: HashMap::new(), lowered_handlers: Vec::new(), elements: self.elements.clone(), providers: Vec::new() };
        let result = cl.component(&function);
        let result = result.map(|(component, refs)| (component, refs, std::mem::take(&mut cl.providers)));
        let hoisting = cl.lowerer.hoisting.take().map(|candidates| (candidates, std::mem::take(&mut cl.state), cl.hook.take()));
        let result = match result {
          Err(residue) if cl.lowerer.reach_violation => return Err(LowerError::Reach(residue)),
          other => other,
        };
        (result, cl.lowerer.unbound.take(), hoisting)
      };
      match result {
        Ok(done) => break (done, hoisting),
        Err(residue) => {
          let Some(name) = unbound else { return Err(residue.into()) };
          if globals.iter().any(|(n, _)| *n == name) {
            return Err(residue.into());
          }
          let Some((expr, key)) = self.global(file, &name)? else { return Err(residue.into()) };
          let expr = self.name_if_large(key, expr);
          globals.push((name, expr));
        }
      }
    };
    let mut modules: HashMap<String, String> = HashMap::new();
    let mut islands: HashMap<String, IslandTiming> = HashMap::new();
    let refs_positions: Vec<(String, (usize, usize))> = refs.clone();
    for (name, (line, column)) in refs {
      let (module, island) = self.component_module(file, &name).map_err(|message| Residue { file: file.to_owned(), line, column, message, hint: None, via: Vec::new() })?;
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
        self.lower(&module).map_err(|error| match error {
          LowerError::Residue(residue) => LowerError::Residue(residue.placed_at(Placement { file: file.to_owned(), line, column, tag: name.clone() })),
          other => other,
        })?;
      }
      let placed = format!("{file}#{name}");
      if let Some(timing) = island {
        islands.insert(placed.clone(), timing);
      }
      modules.insert(placed, module);
    }
    // A template with nothing for the browser to change is never mounted, so
    // it has no browser twin and pulls no framework into the page. A component
    // it renders inline is part of its markup, so that component's state and
    // handlers are its own to hydrate; an island's are the island's.
    let provides = !providers.is_empty();
    for (name, (line, column)) in providers {
      if !self.context_binding(file, &name)? {
        return Err(LowerError::Residue(Residue { file: file.to_owned(), line, column, message: format!("`<{name}.Provider>`: `{name}` is not a `createContext` value this file declares or imports"), hint: None, via: Vec::new() }));
      }
    }
    let render = rewrite_modules(component.render, &modules, &islands);
    // A provider is React state the page's islands read, so the component
    // is a root even when nothing else in it needs the browser.
    let hydrates = !component.state.is_empty() || !component.handlers.is_empty() || provides || self.inline_hydrates(&render);
    let hydrated_by = match tree {
      Some((_, span)) if !self.layouts.iter().any(|m| *m == module) => return Err(self.parsed[file].residue(span, "`tree(...)` marks a layout; this module is not one").into()),
      Some(_) => Some(snapfire_fsr_ir::HydratedBy::ReactTree),
      None => hydrates.then_some(snapfire_fsr_ir::HydratedBy::React),
    };
    let mut component = Component { body: component.body, render, state: component.state, handlers: component.handlers, hydrated_by, shadow: component.shadow };
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
      let kept = hoist::decide(&mut component, &state);
      let pure = state.is_empty() && hoist::static_tree(&component.render, &self.pure);
      let chunks = hoist::chunks(&mut component, &state, &self.pure);
      self.pure.insert(module.clone(), pure);
      self.stateless.insert(module.clone(), state.is_empty());
      let parsed = self.parsed[file].clone();
      for range in candidates.remaining(&kept) {
        let (line, column) = parsed.position(range.start);
        self.remaining.push((module.clone(), format!("{file}:{line}:{column}")));
      }
      let mut islands = Vec::new();
      island_ids(&component.render, &mut islands);
      let placed: Vec<u32> = candidates.island_sites.iter().map(|(id, ..)| *id).collect();
      let mut keyed = Vec::new();
      mark_keyed(&mut component.render, &placed, &|callee| self.keys.get(callee).copied().unwrap_or(true), &|callee| !self.pure.get(callee).copied().unwrap_or(false), &mut keyed);
      let rewrite = candidates.rewrite(&kept, &chunks, &islands, &keyed, file, &module, hook);
      self.keys.insert(module.clone(), rewrite.is_some());
      if let Some(rewrite) = rewrite {
        self.rewrites.push(rewrite);
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
    if !self.parsed.contains_key(file) {
      let (source, line) = match &described.script {
        Some(script) => (script.content.clone(), script.line as usize),
        None => (String::new(), 1),
      };
      let padded = format!("{}{source}", "\n".repeat(line.saturating_sub(1)));
      let parsed = parse_with(file, &padded, false)?;
      self.parsed.insert(file.to_owned(), Rc::new(parsed));
    }
    let mut globals: Vec<(String, Expr)> = Vec::new();
    let component = loop {
      let (result, unbound) = {
        let parsed = self.parsed[file].clone();
        let defaults = self.defaults.clone();
        let mut lowerer = Lowerer::new(&parsed, &defaults);
        lowerer.globals = globals.clone();
        lowerer.natives = self.natives.clone();
        lowerer.render_path = true;
        let mut vue = crate::vue::VueLowerer::new(lowerer, &described);
        let result = vue.component();
        let result = match result {
          Err(residue) if vue.lowerer.reach_violation => return Err(LowerError::Reach(residue)),
          other => other,
        };
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
          let expr = self.name_if_large(key, expr);
          globals.push((name, expr));
        }
      }
    };
    self.keys.insert(module.clone(), false);
    self.pure.insert(module.clone(), false);
    self.stateless.insert(module, component.state.is_empty());
    Ok(component)
  }

  /// Whether markup, islands aside, is something the browser mounts: an
  /// element with an event handler (lowered or not) or a component rendered
  /// inline that is. Its children were lowered before it, so their verdicts
  /// are in.
  fn inline_hydrates(&self, tmpl: &Tmpl) -> bool {
    match tmpl {
      Tmpl::Component { module, children, .. } => {
        self.components.iter().any(|(m, c)| m == module && c.hydrated_by.is_some()) || children.iter().any(|c| self.inline_hydrates(c))
      }
      Tmpl::Element { attrs, children, .. } => attrs.iter().any(|a| matches!(a, Entry::Field(n, _) if n.starts_with(HANDLER_ATTR) || n == UNLOWERED_ATTR)) || children.iter().any(|c| self.inline_hydrates(c)),
      Tmpl::Island { children, .. } | Tmpl::Fragment(children) => children.iter().any(|c| self.inline_hydrates(c)),
      Tmpl::Baked { children, .. } => children.iter().any(|c| self.inline_hydrates(c)),
      Tmpl::If { then, r#else, .. } => self.inline_hydrates(then) || r#else.as_ref().is_some_and(|e| self.inline_hydrates(e)),
      Tmpl::For { body, .. } => self.inline_hydrates(body),
      Tmpl::Let { then, .. } => self.inline_hydrates(then),
      Tmpl::Text(_) | Tmpl::Expr(_) | Tmpl::Slot(_) => false,
    }
  }

  /// The module id a capitalised JSX tag names: a function in this file or a
  /// local import, lowered as its own component; `Ns.Name` is `Name` from the
  /// file a namespace import binds as `Ns`.
  fn component_module(&mut self, file: &str, name: &str) -> Result<(String, Option<IslandTiming>), String> {
    let parsed = self.parsed[file].clone();
    if let Some((namespace, member)) = name.split_once('.') {
      let source = find_namespace_import(&parsed, namespace).ok_or_else(|| format!("`{namespace}` is not a namespace import; `<{name}>` needs `import * as {namespace}`"))?;
      let target = self.resolve_import(file, &source).ok_or_else(|| format!("`{namespace}` comes from `{source}`, which the build cannot follow"))?;
      self.load(&target).map_err(|e| e.to_string())?;
      return self.exported_component(&target, member);
    }
    if default_function_name(&parsed) == Some(name) {
      return Ok((format!("{file}#default"), None));
    }
    if find_function(&parsed, name).is_some() {
      return Ok((format!("{file}#{name}"), None));
    }
    if let Some((source, imported)) = find_import(&parsed, name) {
      let target = self.resolve_import(file, &source).ok_or_else(|| format!("`{name}` comes from `{source}`, which the build cannot follow"))?;
      if is_foreign(&target) {
        return Ok((format!("{target}#{imported}"), None));
      }
      self.load(&target).map_err(|e| e.to_string())?;
      return self.exported_component(&target, &imported);
    }
    Err(format!("`{name}` is not a component this file declares or imports"))
  }

  /// `export` of `file` as a component module: the function itself, or, when
  /// the export is `island(Comp, { when, mode })`, the component the alias
  /// names with the timing and mode it set, so a page importing the alias
  /// places the island the way the aliasing module declared it.
  fn exported_component(&mut self, file: &str, export: &str) -> Result<(String, Option<IslandTiming>), String> {
    let parsed = self.parsed[file].clone();
    match island_alias_of(&parsed, export) {
      Ok(Some(alias)) => {
        let (module, _) = self.component_module(file, &alias.target)?;
        Ok((module, Some((alias.when, alias.mode))))
      }
      Ok(None) => Ok((format!("{file}#{export}"), None)),
      Err((_, message)) => Err(message),
    }
  }

  /// A module-level name as an expression: a `const` inlined, a function as a
  /// lambda, an import resolved in its own file. `None` when the file has no
  /// such name.
  /// Whether `name` in `file` is a module-level `createContext(...)` from
  /// `react`, followed through imports.
  fn context_binding(&mut self, file: &str, name: &str) -> Result<bool, LowerError> {
    let parsed = self.parsed[file].clone();
    if let Some((source, imported)) = find_import(&parsed, name) {
      let Some(target) = self.resolve_import(file, &source) else { return Ok(false) };
      self.load(&target)?;
      return self.context_binding(&target, &imported);
    }
    let Some(Global::Const(js::Expr::Call(call))) = find_value(&parsed, name) else { return Ok(false) };
    let js::Callee::Expr(callee) = &call.callee else { return Ok(false) };
    Ok(imported_callee(&parsed, callee).is_some_and(|(source, imported)| source == "react" && imported == "createContext"))
  }

  fn global(&mut self, file: &str, name: &str) -> Result<Option<(Expr, String)>, LowerError> {
    let key = format!("{file}#{name}");
    if self.resolving.iter().any(|m| *m == key) {
      return Err(LowerError::Parse { file: file.to_owned(), message: format!("`{name}` is recursive, which the build cannot unroll") });
    }
    let parsed = self.parsed[file].clone();
    if let Some((source, imported)) = find_import(&parsed, name) {
      let Some(target) = self.resolve_import(file, &source) else { return Ok(None) };
      self.load(&target)?;
      return self.global(&target, &imported);
    }
    let Some(item) = find_value(&parsed, name) else { return Ok(None) };
    if let Global::Const(init) = &item {
      if let Some((module, member, reach)) = native_declaration(&parsed, init)? {
        let native = format!("{module}.{member}");
        if !self.natives.iter().any(|(n, _)| *n == native) {
          self.natives.push((native, reach));
        }
        return Ok(Some((Expr::Ext { module, name: member, args: Vec::new() }, key)));
      }
    }
    self.resolving.push(key.clone());
    let result = self.lower_global(file, item);
    self.resolving.pop();
    match result {
      Err(LowerError::Residue(residue)) if file.starts_with(&format!("{EXT_DIR}/")) => Err(LowerError::Extension(residue)),
      other => other.map(|expr| Some((expr, key))),
    }
  }

  /// A constant the plan names once or the expression itself when naming it
  /// would cost more than copying it. `CONST_WEIGHT` is where a copy per
  /// reference starts to dominate the plan.
  fn name_if_large(&mut self, key: String, expr: Expr) -> Expr {
    if weight(&expr) < CONST_WEIGHT || matches!(expr, Expr::Lambda { .. } | Expr::Ext { .. }) {
      return expr;
    }
    self.consts.entry(key.clone()).or_insert(expr);
    Expr::Const(key)
  }



  fn lower_global(&mut self, file: &str, item: Global<'_>) -> Result<Expr, LowerError> {
    let mut globals: Vec<(String, Expr)> = Vec::new();
    loop {
      let parsed = self.parsed[file].clone();
      let defaults = self.defaults.clone();
      let mut lowerer = Lowerer::new(&parsed, &defaults);
      lowerer.globals = globals.clone();
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
    Tmpl::Component { children, .. } | Tmpl::Element { children, .. } | Tmpl::Fragment(children) => children.iter().for_each(|c| island_ids(c, out)),
    Tmpl::If { then, r#else, .. } => {
      island_ids(then, out);
      if let Some(other) = r#else {
        island_ids(other, out);
      }
    }
    Tmpl::For { body, .. } => island_ids(body, out),
    Tmpl::Let { then, .. } => island_ids(then, out),
    Tmpl::Text(_) | Tmpl::Expr(_) | Tmpl::Slot(_) => {}
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
    Tmpl::Island { children, .. } | Tmpl::Element { children, .. } | Tmpl::Fragment(children) => children.iter_mut().for_each(|c| mark_keyed(c, placed, keys, stateful, out)),
    Tmpl::If { then, r#else, .. } => {
      mark_keyed(then, placed, keys, stateful, out);
      if let Some(other) = r#else {
        mark_keyed(other, placed, keys, stateful, out);
      }
    }
    Tmpl::For { body, .. } => mark_keyed(body, placed, keys, stateful, out),
    Tmpl::Let { then, .. } => mark_keyed(then, placed, keys, stateful, out),
    Tmpl::Text(_) | Tmpl::Expr(_) | Tmpl::Slot(_) => {}
  }
}

/// The modules `Island`, `island`, `Slot` and `Link` are read from: the
/// client's React module, which a React application also mounts with or the
/// dialect's own declarations, which an application without React types
/// against.
pub const TEMPLATE_SOURCES: &[&str] = &["@snapfire/fsr-client/react", "@snapfire/fsr-authoring/template"];

/// The client library's root, where `action` comes from.
pub const CLIENT_SOURCE: &str = "@snapfire/fsr-client";

fn is_template_source(source: &str) -> bool {
  TEMPLATE_SOURCES.contains(&source)
}

/// Whether a file or a `file#export` module, is written in a language the
/// build does not read. Such a component has no server body: a plugin
/// compiles it for the browser and the page places it as an island.
pub fn is_foreign(module: &str) -> bool {
  let file = module.split_once('#').map(|(file, _)| file).unwrap_or(module);
  let ext = std::path::Path::new(file).extension().and_then(|e| e.to_str()).unwrap_or("");
  !matches!(ext, "ts" | "tsx" | "js" | "jsx" | "mts" | "mjs")
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
    Tmpl::Island { children, .. } | Tmpl::Element { children, .. } | Tmpl::Fragment(children) => children.iter().find_map(inline_foreign),
    Tmpl::Baked { children, .. } => children.iter().find_map(inline_foreign),
    Tmpl::If { then, r#else, .. } => inline_foreign(then).or_else(|| r#else.as_ref().and_then(|e| inline_foreign(e))),
    Tmpl::For { body, .. } => inline_foreign(body),
    Tmpl::Let { then, .. } => inline_foreign(then),
    Tmpl::Text(_) | Tmpl::Expr(_) | Tmpl::Slot(_) => None,
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
fn island_alias_of(parsed: &Parsed, name: &str) -> Result<Option<IslandAlias>, (Span, String)> {
  let Some(Global::Const(js::Expr::Call(call))) = find_value(parsed, name) else { return Ok(None) };
  let js::Callee::Expr(callee) = &call.callee else { return Ok(None) };
  if imported_callee(parsed, callee).filter(|(source, _)| is_template_source(source)).map(|(_, name)| name).as_deref() != Some("island") {
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
fn generated_action_of(parsed: &Parsed, callee: &js::Expr) -> Option<String> {
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
        let source = find_import(parsed, root.sym.as_ref())?.0;
        let file = source.rsplit_once('.').map(|(stem, _)| stem).unwrap_or(&source);
        if !file.ends_with("generated/client") {
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
fn action_alias_of(parsed: &Parsed, name: &str) -> Option<String> {
  let Some(Global::Const(js::Expr::Call(call))) = find_value(parsed, name) else { return None };
  let js::Callee::Expr(callee) = &call.callee else { return None };
  if imported_callee(parsed, callee).filter(|(source, _)| source == CLIENT_SOURCE).map(|(_, name)| name).as_deref() != Some("action") {
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

fn patterns(function: &js::Function) -> Vec<js::Pat> {
  function.params.iter().map(|p| p.pat.clone()).collect()
}

fn arrow_body(arrow: &js::ArrowExpr) -> FunctionBody<'_> {
  match &*arrow.body {
    js::ArrowFunctionBody::FunctionBody(b) => FunctionBody::Block(&b.stmts),
    js::ArrowFunctionBody::Expr(e) => FunctionBody::Expr(e),
  }
}

/// The exported or declared function named `export`; `default` is the
/// default export.
fn find_function<'a>(parsed: &'a Parsed, export: &str) -> Option<Found<'a>> {
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
        js::Expr::Call(_) => {
          if let Ok(Some((name, _))) = tree_target(parsed) {
            return find_function(parsed, &name);
          }
        }
        _ => {}
      },
      js::ModuleItem::ModuleDecl(js::ModuleDecl::ExportDecl(export_decl)) => {
        if let Some(found) = decl_function(&export_decl.decl, export) {
          return Some(found);
        }
      }
      js::ModuleItem::Stmt(js::Stmt::Decl(decl)) => {
        if let Some(found) = decl_function(decl, export) {
          return Some(found);
        }
      }
      _ => {}
    }
  }
  None
}

enum Found<'a> {
  Declared(Vec<js::Pat>, &'a [js::Stmt], Span),
  Arrow(&'a js::ArrowExpr),
}

/// The name an `export default function Name` declaration gives itself, which
/// is how the rest of its own file refers to it.
fn default_function_name(parsed: &Parsed) -> Option<&str> {
  parsed.module.body.iter().find_map(|item| match item {
    js::ModuleItem::ModuleDecl(js::ModuleDecl::ExportDefaultDecl(d)) => match &d.decl {
      js::DefaultDecl::Fn(f) => f.ident.as_ref().map(|id| id.sym.as_ref()),
      _ => None,
    },
    _ => None,
  })
}

/// `export default tree(Layout)` with `tree` from the React adapter: the
/// component's name and the call's span. `None` for any other default export.
fn tree_target(parsed: &Parsed) -> Result<Option<(String, Span)>, (Span, String)> {
  for item in &parsed.module.body {
    let js::ModuleItem::ModuleDecl(js::ModuleDecl::ExportDefaultExpr(e)) = item else { continue };
    let js::Expr::Call(call) = &*e.expr else { return Ok(None) };
    let js::Callee::Expr(callee) = &call.callee else { return Ok(None) };
    if imported_callee(parsed, callee).filter(|(source, _)| is_template_source(source)).map(|(_, name)| name).as_deref() != Some("tree") {
      return Ok(None);
    }
    return match (call.args.first().map(|a| &*a.expr), call.args.len()) {
      (Some(js::Expr::Ident(target)), 1) => Ok(Some((target.sym.to_string(), call.span))),
      _ => Err((call.span, "`tree(...)` takes the layout's component name and nothing else".to_owned())),
    };
  }
  Ok(None)
}

fn decl_function<'a>(decl: &'a js::Decl, name: &str) -> Option<Found<'a>> {
  match decl {
    js::Decl::Fn(f) if f.ident.sym.as_ref() == name => {
      let body = f.function.body.as_ref()?;
      Some(Found::Declared(patterns(&f.function), &body.stmts, body.span))
    }
    js::Decl::Var(var) => {
      let decl = var.decls.iter().find(|d| matches!(&d.name, js::Pat::Ident(id) if id.id.sym.as_ref() == name))?;
      match decl.init.as_deref()? {
        js::Expr::Arrow(arrow) => Some(Found::Arrow(arrow)),
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
          if f.ident.as_ref().is_some_and(|id| id.sym.as_ref() == name) {
            let body = f.function.body.as_ref()?;
            return Some(Global::Function(patterns(&f.function), FunctionBody::Block(&body.stmts)));
          }
        }
        continue;
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

/// The source of `import * as local`.
fn find_namespace_import(parsed: &Parsed, local: &str) -> Option<String> {
  for item in &parsed.module.body {
    let js::ModuleItem::ModuleDecl(js::ModuleDecl::Import(import)) = item else { continue };
    if import.type_only {
      continue;
    }
    for spec in &import.specifiers {
      if let js::ImportSpecifier::Namespace(ns) = spec {
        if ns.local.sym.as_ref() == local {
          return Some(import.src.value.to_atom_lossy().to_string());
        }
      }
    }
  }
  None
}

/// `(source, imported name)` for whatever `expr` names, whether it is a local
/// binding of a named import or a member of a namespace import. The imported
/// name is what the build recognises a call by, so `import { action as act }`
/// and `import * as fsr` reach the same place as `import { action }`.
pub(crate) fn imported_callee(parsed: &Parsed, expr: &js::Expr) -> Option<(String, String)> {
  match expr {
    js::Expr::Ident(id) => find_import(parsed, id.sym.as_ref()),
    js::Expr::Member(member) => {
      let js::Expr::Ident(ns) = &*member.obj else { return None };
      let js::MemberProp::Ident(name) = &member.prop else { return None };
      let source = find_namespace_import(parsed, ns.sym.as_ref())?;
      Some((source, name.sym.to_string()))
    }
    _ => None,
  }
}

/// The name `local` was imported under when its source satisfies `source_is`,
/// which is how a call or a tag is recognised however it was spelled here.
pub(crate) fn imported_as(parsed: &Parsed, local: &str, source_is: impl Fn(&str) -> bool) -> Option<String> {
  find_import(parsed, local).filter(|(source, _)| source_is(source)).map(|(_, imported)| imported)
}

/// `native("module.member", f?)` from the standard library: the pair's
/// name and its reach, `render` with a browser half and `body` without.
/// `None` for any other initialiser.
fn native_declaration(parsed: &Parsed, init: &js::Expr) -> Result<Option<(String, String, Reach)>, LowerError> {
  let js::Expr::Call(call) = init else { return Ok(None) };
  let js::Callee::Expr(callee) = &call.callee else { return Ok(None) };
  let Some((source, imported)) = imported_callee(parsed, callee) else { return Ok(None) };
  if source != STD_SPECIFIER || imported != "native" {
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
    FunctionBody::Block(stmts) => block_to_expr(lowerer, stmts),
  };
  lowerer.scope.truncate(depth);
  Ok(Expr::Lambda { params: names, body: Box::new(result?) })
}

fn bind_params(lowerer: &mut Lowerer<'_>, params: &[js::Pat]) -> Lowered<Vec<String>> {
  let mut names = Vec::new();
  for (i, pat) in params.iter().enumerate() {
    let positional = format!("${i}");
    match pat {
      js::Pat::Ident(id) => {
        let name = id.id.sym.to_string();
        lowerer.scope.push((name.clone(), Expr::Var(name.clone())));
        names.push(name);
      }
      js::Pat::Assign(assign) => {
        let js::Pat::Ident(id) = &*assign.left else {
          return Err(lowerer.residue(assign.span, "a default on a destructured parameter"));
        };
        let name = id.id.sym.to_string();
        let default = lowerer.expr(&assign.right)?;
        lowerer.scope.push((name.clone(), Expr::Coalesce(Box::new(Expr::Var(name.clone())), Box::new(default))));
        names.push(name);
      }
      js::Pat::Object(obj) => {
        bind_object(lowerer, obj, Expr::Var(positional.clone()))?;
        names.push(positional);
      }
      js::Pat::Array(arr) => {
        for (j, elem) in arr.elems.iter().enumerate() {
          let Some(js::Pat::Ident(id)) = elem else {
            if elem.is_some() {
              return Err(lowerer.residue(pat.span(), "a nested pattern in a parameter"));
            }
            continue;
          };
          lowerer.scope.push((id.id.sym.to_string(), Expr::Var(positional.clone()).index(Expr::Lit(Lit::Int(j as i128)))));
        }
        names.push(positional);
      }
      other => return Err(lowerer.residue(other.span(), "a parameter pattern")),
    }
  }
  Ok(names)
}

/// `{ a, b = x, c: d }` over `target`: each name reads a field, a default
/// applies when the field is null or absent.
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
        let read = target.clone().field(key);
        match &*kv.value {
          js::Pat::Ident(local) => lowerer.scope.push((local.id.sym.to_string(), read)),
          js::Pat::Assign(assign) => {
            let js::Pat::Ident(local) = &*assign.left else {
              return Err(lowerer.residue(assign.span, "a nested pattern in a parameter"));
            };
            let default = lowerer.expr(&assign.right)?;
            lowerer.scope.push((local.id.sym.to_string(), Expr::Coalesce(Box::new(read), Box::new(default))));
          }
          other => return Err(lowerer.residue(other.span(), "a nested pattern in a parameter")),
        }
      }
      js::ObjectPatProp::Rest(r) => {
        let js::Pat::Ident(id) = &*r.arg else {
          return Err(lowerer.residue(r.span, "a pattern in a rest"));
        };
        let mut args = vec![target.clone()];
        args.extend(taken.iter().map(|key| Expr::lit_str(key.clone())));
        lowerer.scope.push((id.id.sym.to_string(), Expr::Builtin { name: Builtin::Omit, args }));
      }
    }
  }
  Ok(())
}

/// A function body as one expression: `const`s inline into what follows and
/// `if (c) return a;` followed by more becomes `c ? a : rest`.
/// A handler's statements as they accumulate: the statements in order, the
/// patch as one entry per state key and the condition the statement being
/// lowered runs under, `None` at the top of the body. A branch's declarations
/// are hoisted under a name of their own, since the patch is evaluated after
/// every branch has bound.
#[derive(Default)]
struct HandlerWalk {
  out: Vec<Stmt>,
  patch: Vec<Entry>,
  reach: Option<Expr>,
  locals: usize,
}

impl HandlerWalk {
  fn set(&mut self, state: String, value: Expr) {
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

  fn act(&mut self, action: String, input: Expr) {
    let act = Stmt::Act { action, input };
    match &self.reach {
      None => self.out.push(act),
      Some(reach) => self.out.push(Stmt::If { cond: reach.clone(), then: vec![act], r#else: Vec::new() }),
    }
  }
}

fn reach_under(outer: &Option<Expr>, cond: Expr) -> Expr {
  match outer {
    None => cond,
    Some(outer) => Expr::Logic(LogicOp::And, Box::new(outer.clone()), Box::new(cond)),
  }
}

fn holds_act(stmts: &[Stmt]) -> bool {
  stmts.iter().any(|stmt| match stmt {
    Stmt::Act { .. } => true,
    Stmt::If { then, r#else, .. } => holds_act(then) || holds_act(r#else),
    _ => false,
  })
}

pub(crate) fn block_to_expr(lowerer: &mut Lowerer<'_>, stmts: &[js::Stmt]) -> Lowered<Expr> {
  let depth = lowerer.scope.len();
  let result = block_to_expr_inner(lowerer, stmts);
  lowerer.scope.truncate(depth);
  result
}

fn block_to_expr_inner(lowerer: &mut Lowerer<'_>, stmts: &[js::Stmt]) -> Lowered<Expr> {
  let Some((first, rest)) = stmts.split_first() else {
    return Ok(Expr::Lit(Lit::Null));
  };
  match first {
    js::Stmt::Decl(js::Decl::Var(var)) => {
      if var.decls.len() != 1 {
        return Err(lowerer.residue(var.span, "one binding per declaration"));
      }
      let decl = &var.decls[0];
      let init = decl.init.as_deref().ok_or_else(|| lowerer.residue(decl.span, "a declaration without a value"))?;
      let expr = lowerer.expr(init)?;
      match &decl.name {
        js::Pat::Ident(name) => lowerer.scope.push((name.id.sym.to_string(), expr)),
        js::Pat::Object(obj) => bind_object(lowerer, obj, expr)?,
        other => return Err(lowerer.residue(other.span(), "a destructuring the build does not read")),
      }
      block_to_expr_inner(lowerer, rest)
    }
    js::Stmt::Return(ret) => match &ret.arg {
      Some(arg) => lowerer.expr(arg),
      None => Ok(Expr::Lit(Lit::Null)),
    },
    js::Stmt::If(if_stmt) => {
      let cond = lowerer.expr(&if_stmt.test)?;
      let then = branch_to_expr(lowerer, &if_stmt.cons)?;
      let otherwise = match &if_stmt.alt {
        Some(alt) => branch_to_expr(lowerer, alt)?,
        None => block_to_expr_inner(lowerer, rest)?,
      };
      Ok(Expr::Ternary(Box::new(cond), Box::new(then), Box::new(otherwise)))
    }
    other => Err(lowerer.residue(other.span(), "a statement a helper cannot hold; a helper is `const`s, `if ... return` and a `return`")),
  }
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

fn branch_to_expr(lowerer: &mut Lowerer<'_>, stmt: &js::Stmt) -> Lowered<Expr> {
  match stmt {
    js::Stmt::Block(block) => block_to_expr(lowerer, &block.stmts),
    single => block_to_expr(lowerer, std::slice::from_ref(single)),
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
  /// Each setter and the state it sets.
  setters: Vec<(String, String)>,
  /// Handler functions declared in the component, by name, for `onClick={add}`.
  handler_fns: HashMap<String, (Vec<js::Pat>, FunctionBody<'p>)>,
  /// The handlers lowered so far; an element's `$on:` marker holds an index into it.
  lowered_handlers: Vec<Handler>,
  /// Custom element tags with a shadow template, to the template's module.
  elements: Rc<HashMap<String, String>>,
  /// `<X.Provider>` tags met, by the name of `X` and where, checked by the
  /// set after to be `createContext` values.
  providers: Vec<(String, (usize, usize))>,
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
fn hook_call<'a>(parsed: &Parsed, expr: &'a js::Expr) -> Option<(&'a str, &'a js::CallExpr)> {
  let js::Expr::Call(call) = expr else { return None };
  let js::Callee::Expr(callee) = &call.callee else { return None };
  let name = match &**callee {
    js::Expr::Ident(id) => id.sym.as_ref(),
    js::Expr::Member(member) => {
      let js::Expr::Ident(ns) = &*member.obj else { return None };
      let js::MemberProp::Ident(name) = &member.prop else { return None };
      find_namespace_import(parsed, ns.sym.as_ref())?;
      name.sym.as_ref()
    }
    _ => return None,
  };
  name.starts_with("use").then_some((name, call))
}

impl<'a, 'p> ComponentLowerer<'a, 'p> {
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
    self.bind_props(&params)?;
    let mut lets = Vec::new();
    let render = match body {
      FunctionBody::Expr(e) => self.child_expr(e)?,
      FunctionBody::Block(stmts) => {
        let mut render = None;
        let mut early = Vec::new();
        for stmt in stmts {
          match stmt {
            js::Stmt::Return(ret) => {
              let arg = ret.arg.as_deref().ok_or_else(|| self.lowerer.residue(ret.span, "a component must return its tree"))?;
              render = Some(self.child_expr(arg)?);
              break;
            }
            js::Stmt::If(branch) => {
              let ret = early_return(branch).ok_or_else(|| self.lowerer.residue(branch.span, "an `if` in a component other than `if (...) return` with no `else`"))?;
              let cond = self.lowerer.expr(&branch.test)?;
              let then = match ret.arg.as_deref() {
                Some(arg) => self.child_expr(arg)?,
                None => Tmpl::Fragment(Vec::new()),
              };
              early.push((cond, then));
            }
            other if !early.is_empty() => return Err(self.lowerer.residue(other.span(), "a statement after an early `return`; only another `if (...) return` or the final `return` can follow one")),
            js::Stmt::Decl(js::Decl::Fn(f)) => {
              self.handlers.push(f.ident.sym.to_string());
              if let Some(body) = &f.function.body {
                self.handler_fns.insert(f.ident.sym.to_string(), (patterns(&f.function), FunctionBody::Block(&body.stmts)));
              }
            }
            js::Stmt::Expr(e) if hook_call(self.lowerer.parsed, &e.expr).is_some_and(|(name, _)| EFFECT_HOOKS.contains(&name)) => {}
            js::Stmt::Decl(js::Decl::Var(var)) => {
              for decl in &var.decls {
                if let Some(stmt) = self.let_stmt(decl)? {
                  lets.push(stmt);
                }
              }
            }
            other => return Err(self.lowerer.residue(other.span(), "a statement a component cannot hold before its `return`")),
          }
        }
        let render = render.ok_or_else(|| self.lowerer.residue(Span::default(), "a component must return its tree"))?;
        early.into_iter().rev().fold(render, |r#else, (cond, then)| Tmpl::If { cond, then: Box::new(then), r#else: Some(Box::new(r#else)) })
      }
    };
    self.lowerer.scope.truncate(depth);
    Ok((Component { body: lets, render, state: std::mem::take(&mut self.state_bindings), handlers: std::mem::take(&mut self.lowered_handlers), hydrated_by: Some(snapfire_fsr_ir::HydratedBy::React), shadow: None }, std::mem::take(&mut self.refs)))
  }

  fn bind_props(&mut self, params: &[js::Pat]) -> Lowered<()> {
    let Some(first) = params.first() else { return Ok(()) };
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
        bind_object(&mut self.lowerer, obj, Expr::Var("$props".to_owned()))
      }
      other => Err(self.lowerer.residue(other.span(), "the props parameter must be a name or a destructuring")),
    }
  }

  /// `const x = e` as a `let`; `const [x, setX] = useState(e)` as `let x = e`
  /// with `setX` a handler; `const { a, b } = e` as one `let` plus field reads.
  /// A function value, `useCallback` included, is a handler.
  fn let_stmt(&mut self, decl: &'p js::VarDeclarator) -> Lowered<Option<Stmt>> {
    let init = decl.init.as_deref().ok_or_else(|| self.lowerer.residue(decl.span, "a declaration without a value"))?;
    match &decl.name {
      js::Pat::Ident(name) => {
        let local = name.id.sym.to_string();
        let expr = match hook_call(self.lowerer.parsed, init) {
          Some(("useCallback", call)) => {
            if let Some(js::Expr::Arrow(arrow)) = call.args.first().map(|a| &*a.expr) {
              self.handler_fns.insert(local.clone(), (arrow.params.clone(), arrow_body(arrow)));
            }
            self.handlers.push(local);
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
          Some((hook, _)) if hook != "useState" => return Err(self.lowerer.residue(decl.span, format!("`{hook}`"))),
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
            self.handlers.push(local);
            return Ok(None);
          }
          _ => self.lowerer.expr(init)?,
        };
        self.lowerer.scope.push((local.clone(), Expr::Var(local.clone())));
        Ok(Some(Stmt::Let { name: local, expr }))
      }
      js::Pat::Array(arr) => {
        let js::Expr::Call(call) = init else {
          return Err(self.lowerer.residue(decl.span, "an array destructuring of something other than `useState` or `useStore`"));
        };
        let called = hook_call(self.lowerer.parsed, init).map(|(name, _)| name).unwrap_or_default();
        if called == "useStore" {
          return self.store_stmt(decl, arr, call);
        }
        if called != "useState" {
          return Err(self.lowerer.residue(decl.span, "an array destructuring of something other than `useState` or `useStore`"));
        }
        let expr = match call.args.first() {
          Some(a) => match &*a.expr {
            js::Expr::Arrow(arrow) => match &*arrow.body {
              js::ArrowFunctionBody::Expr(e) => self.lowerer.expr(e)?,
              js::ArrowFunctionBody::FunctionBody(b) => block_to_expr(&mut self.lowerer, &b.stmts)?,
            },
            e => self.lowerer.expr(e)?,
          },
          None => Expr::Lit(Lit::Null),
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
          self.handlers.push(setter);
        }
        self.state.push(name.clone());
        self.state_bindings.push(name.clone());
        self.lowerer.scope.push((name.clone(), Expr::Var(name.clone())));
        Ok(Some(Stmt::Let { name, expr }))
      }
      js::Pat::Object(obj) => {
        let expr = self.lowerer.expr(init)?;
        let name = format!("$let{}", self.lowerer.scope.len());
        bind_object(&mut self.lowerer, obj, Expr::Var(name.clone()))?;
        Ok(Some(Stmt::Let { name, expr }))
      }
      other => Err(self.lowerer.residue(other.span(), "a declaration pattern the build does not read")),
    }
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
      self.handlers.push(setter);
    }
    self.state.push(name.clone());
    self.state_bindings.push(name.clone());
    self.lowerer.scope.push((name.clone(), Expr::Var(name.clone())));
    Ok(Some(Stmt::Let { name, expr: Expr::Coalesce(Box::new(Expr::Store(key)), Box::new(initial)) }))
  }

  /// A JSX child or a component's return: a tree when the expression holds
  /// JSX anywhere a render would reach, text otherwise.
  fn child_expr(&mut self, expr: &'p js::Expr) -> Lowered<Tmpl> {
    if let Some(name) = self.slot_of_expr(expr) {
      return Ok(self.slot(&name));
    }
    match expr {
      js::Expr::Paren(p) => self.child_expr(&p.expr),
      js::Expr::JSXElement(el) => self.element(el, false),
      js::Expr::JSXFragment(frag) => Ok(Tmpl::Fragment(self.children(&frag.children)?)),
      js::Expr::Lit(js::Lit::Str(s)) => Ok(Tmpl::Text(s.value.to_atom_lossy().to_string())),
      js::Expr::Cond(c) if holds_jsx(&c.cons) || holds_jsx(&c.alt) => {
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
      js::Expr::Bin(bin) if bin.op == js::BinaryOp::LogicalAnd && holds_jsx(&bin.right) => {
        let cond = self.lowerer.expr(&bin.left)?;
        let then = Box::new(self.child_expr(&bin.right)?);
        Ok(Tmpl::If { cond, then, r#else: None })
      }
      js::Expr::Call(call) if map_with_jsx(call) => {
        let js::Callee::Expr(callee) = &call.callee else { unreachable!() };
        let js::Expr::Member(member) = &**callee else { unreachable!() };
        let over = self.lowerer.expr(&member.obj)?;
        let js::Expr::Arrow(arrow) = &*call.args[0].expr else { unreachable!() };
        let depth = self.lowerer.scope.len();
        let params = bind_params(&mut self.lowerer, &arrow.params)?;
        let range = self.lowerer.parsed.range(arrow.span);
        if let Some(candidates) = &mut self.lowerer.hoisting {
          candidates.open_loops.push(range);
        }
        let body = match &*arrow.body {
          js::ArrowFunctionBody::Expr(e) => self.child_expr(e),
          js::ArrowFunctionBody::FunctionBody(b) => self.block_tree(&b.stmts),
        };
        if let Some(candidates) = &mut self.lowerer.hoisting {
          candidates.open_loops.pop();
        }
        self.lowerer.scope.truncate(depth);
        Ok(Tmpl::For { over, params, body: Box::new(body?) })
      }
      js::Expr::Ident(id) if id.sym.as_ref() == "null" || id.sym.as_ref() == "undefined" => Ok(Tmpl::Fragment(Vec::new())),
      js::Expr::Lit(js::Lit::Null(_)) => Ok(Tmpl::Fragment(Vec::new())),
      other => Ok(Tmpl::Expr(self.lowerer.expr(other)?)),
    }
  }

  /// A `.map` callback with statements: `const`s then a `return` of a tree.
  fn block_tree(&mut self, stmts: &'p [js::Stmt]) -> Lowered<Tmpl> {
    let Some((first, rest)) = stmts.split_first() else {
      return Ok(Tmpl::Fragment(Vec::new()));
    };
    match first {
      js::Stmt::Decl(js::Decl::Var(var)) => {
        if var.decls.len() != 1 {
          return Err(self.lowerer.residue(var.span, "one binding per declaration"));
        }
        let decl = &var.decls[0];
        let js::Pat::Ident(name) = &decl.name else {
          return Err(self.lowerer.residue(decl.name.span(), "a destructuring declaration; bind the whole value and read its fields"));
        };
        let init = decl.init.as_deref().ok_or_else(|| self.lowerer.residue(decl.span, "a declaration without a value"))?;
        let expr = self.lowerer.expr(init)?;
        let name = name.id.sym.to_string();
        self.lowerer.scope.push((name.clone(), Expr::Var(name.clone())));
        let then = self.block_tree(rest)?;
        self.lowerer.scope.pop();
        Ok(Tmpl::Let { name, expr, then: Box::new(then) })
      }
      js::Stmt::Return(ret) => match &ret.arg {
        Some(arg) => self.child_expr(arg),
        None => Ok(Tmpl::Fragment(Vec::new())),
      },
      other => Err(self.lowerer.residue(other.span(), "a statement in a `.map` callback other than `const` and `return`")),
    }
  }

  /// `as_child` says the element sits directly among JSX children, where a
  /// rewrite of it must be braced, rather than in an expression.
  fn element(&mut self, el: &'p js::JSXElement, as_child: bool) -> Lowered<Tmpl> {
    let (name, member) = match &el.opening.name {
      js::JSXElementName::Ident(id) => (id.sym.to_string(), false),
      js::JSXElementName::JSXMemberExpr(m) => match &m.obj {
        js::JSXObject::Ident(obj) => (format!("{}.{}", obj.sym, m.prop.sym), true),
        js::JSXObject::JSXMemberExpr(_) => return Err(self.lowerer.residue(m.span, "a member expression as a tag more than one level deep")),
      },
      js::JSXElementName::JSXNamespacedName(n) => return Err(self.lowerer.residue(n.span, "a namespaced tag")),
    };
    if let js::JSXElementName::JSXMemberExpr(m) = &el.opening.name {
      if let (js::JSXObject::Ident(obj), "Provider") = (&m.obj, m.prop.sym.as_ref()) {
        if find_namespace_import(self.lowerer.parsed, obj.sym.as_ref()).is_none() {
          let loc = self.lowerer.parsed.cm.lookup_char_pos(el.span.lo);
          self.providers.push((obj.sym.to_string(), (loc.line, loc.col_display + 1)));
          return Ok(Tmpl::Fragment(self.children(&el.children)?));
        }
      }
    }
    let is_component = member || name.chars().next().is_some_and(|c| c.is_ascii_uppercase());
    if is_component {
      return self.component_ref(&name, el, as_child);
    }
    let mut attrs = Vec::new();
    let mut select_value = None;
    let mut bound = false;
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
      if is_handler_name(&raw) {
        bound = true;
        let event = raw[2..].to_ascii_lowercase();
        match self.handler_attr(attr) {
          Ok(index) => attrs.push(Entry::Field(format!("{HANDLER_ATTR}{event}"), Expr::Lit(Lit::Int(index as i128)))),
          Err(residue) => {
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
    Ok(Tmpl::Element { tag: name, attrs, children })
  }

  /// True when `name` is imported from the client library's React adapter.
  /// The dialect tag `name` stands for, whatever it was imported as.
  fn template_tag(&self, name: &str) -> Option<String> {
    imported_as(self.lowerer.parsed, name, is_template_source)
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
            let init = decl.init.as_deref().ok_or_else(|| self.lowerer.residue(decl.span, "a declaration without a value"))?;
            let expr = self.lowerer.expr(init)?;
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
        other => return Err(self.lowerer.residue(other.span(), "a statement a handler cannot hold; a handler is `const`s, branches, calls to state setters and calls to actions")),
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
            Err(self.lowerer.residue(id.span, format!("a call to `{name}`, which is not a state setter, an action or a handler this component declares")))
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

  /// `const Lazy = island(Chart, { when: "visible" })` at module scope, when
  /// `name` is such a `Lazy`: the component and the timing.
  fn island_alias(&mut self, name: &str) -> Lowered<Option<(String, Option<String>, Option<String>)>> {
    match island_alias_of(self.lowerer.parsed, name) {
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
    let mut attrs = Vec::new();
    let mut href = None;
    let mut rule = Some(Match::Exact);
    let mut marked = false;
    let mut by_document = false;
    for attr in &el.opening.attrs {
      let attr = match attr {
        js::JSXAttrOrSpread::JSXAttr(attr) => attr,
        js::JSXAttrOrSpread::SpreadElement(spread) => {
          attrs.push(Entry::Spread(self.lowerer.expr(&spread.expr)?));
          continue;
        }
      };
      let raw = attr_name(&attr.name);
      if raw == "key" || raw == "ref" || is_handler_name(&raw) {
        continue;
      }
      if raw == "style" {
        attrs.push(Entry::Field("style".to_owned(), self.style(attr)?));
        continue;
      }
      if raw == "match" {
        rule = self.link_match(attr)?;
        continue;
      }
      if raw == "current" {
        by_document = self.link_current(attr)?;
        continue;
      }
      let value = self.attr_value(attr)?;
      if raw == "href" {
        href = Some(value.clone());
      }
      if raw == "aria-current" {
        marked = true;
      }
      let (name, value) = match raw.as_str() {
        "full" => ("data-sf-full", value),
        "into" => ("data-sf-into", value),
        "prefetch" => ("data-sf-prefetch", value),
        "native" => ("data-sf-native", value),
        "keep" => ("data-sf-keep", keep_value(value)),
        other => (html_attr_name(other), value),
      };
      attrs.push(Entry::Field(name.to_owned(), value));
    }
    if let (Some(rule), Some(href), false) = (rule, href, marked) {
      if by_document {
        attrs.push(Entry::Field("data-sf-current".to_owned(), Expr::lit_str("document")));
      }
      attrs.extend(active_attrs(rule, href, if by_document { Expr::Document } else { Expr::Path }));
    }
    let children = self.children(&el.children)?;
    Ok(Tmpl::Element { tag: "a".to_owned(), attrs, children })
  }

  /// A `Link`'s `current`, which says which path the mark is judged
  /// against: `"url"` is the address and `"document"` the page beneath an
  /// open intercept. True for `"document"`.
  fn link_current(&mut self, attr: &'p js::JSXAttr) -> Lowered<bool> {
    match self.attr_value(attr)? {
      Expr::Lit(Lit::Str(by)) if by == "url" => Ok(false),
      Expr::Lit(Lit::Str(by)) if by == "document" => Ok(true),
      _ => Err(self.lowerer.residue(attr.span, "a `<Link>`'s `current` is \"url\" or \"document\", written out")),
    }
  }

  /// A `Link`'s `match`, which says when the navigator calls it the current
  /// page. `None` is `"none"`: the link is never marked.
  fn link_match(&mut self, attr: &'p js::JSXAttr) -> Lowered<Option<Match>> {
    match self.attr_value(attr)? {
      Expr::Lit(Lit::Str(rule)) if rule == "exact" => Ok(Some(Match::Exact)),
      Expr::Lit(Lit::Str(rule)) if rule == "prefix" => Ok(Some(Match::Prefix)),
      Expr::Lit(Lit::Str(rule)) if rule == "none" => Ok(None),
      _ => Err(self.lowerer.residue(attr.span, "a `<Link>`'s `match` is \"exact\", \"prefix\" or \"none\", written out")),
    }
  }

  fn island_timing(&self, value: Expr, span: Span) -> Lowered<String> {
    match value {
      Expr::Lit(Lit::Str(timing)) if matches!(timing.as_str(), "load" | "visible" | "idle") => Ok(timing),
      _ => Err(self.lowerer.residue(span, "an island's `when` is \"load\", \"visible\" or \"idle\", written out")),
    }
  }

  /// `lowered` as an island placement, keeping the id its placement took, so
  /// the bundle's copy of this component carries the same region key.
  fn island_of(&self, lowered: Tmpl, when: Option<String>, mode: Option<String>, span: Span) -> Lowered<Tmpl> {
    match lowered {
      Tmpl::Component { module, props, children, id, .. } => Ok(Tmpl::Island { module, props, children, when, mode, id, define: false }),
      _ => Err(self.lowerer.residue(span, "an island must be a component, not an element")),
    }
  }

  /// `as_child` says the element sits among JSX children, where a keyed
  /// placement's wrap must be braced.
  fn component_ref(&mut self, name: &str, el: &'p js::JSXElement, as_child: bool) -> Lowered<Tmpl> {
    if name == "Fragment" || name == "React.Fragment" {
      return Ok(Tmpl::Fragment(self.children(&el.children)?));
    }
    match self.template_tag(name).as_deref() {
      Some("Island") => return self.island_element(el),
      Some("Slot") => return self.slot_element(el),
      Some("Link") => return self.link_element(el),
      _ => {}
    }
    if let Some((target, when, mode)) = self.island_alias(name)? {
      let lowered = self.component_ref(&target, el, false)?;
      return self.island_of(lowered, when, mode, el.span);
    }
    let mut props = Vec::new();
    for attr in &el.opening.attrs {
      let attr = match attr {
        js::JSXAttrOrSpread::JSXAttr(attr) => attr,
        js::JSXAttrOrSpread::SpreadElement(spread) => {
          props.push(Entry::Spread(self.lowerer.expr(&spread.expr)?));
          continue;
        }
      };
      let raw = attr_name(&attr.name);
      if raw == "key" || raw == "ref" || is_handler_name(&raw) {
        continue;
      }
      if raw == "children" {
        return Err(self.lowerer.residue(attr.span, "`children` as a prop; pass them between the tags"));
      }
      let value = self.attr_value(attr)?;
      props.push(Entry::Field(raw, value));
    }
    let children = self.children(&el.children)?;
    let loc = self.lowerer.parsed.cm.lookup_char_pos(el.span.lo);
    self.refs.push((name.to_owned(), (loc.line, loc.col_display + 1)));
    let at = self.lowerer.parsed.range(el.opening.name.span()).end;
    let range = self.lowerer.parsed.range(el.span);
    let id = match &mut self.lowerer.hoisting {
      Some(candidates) => candidates.island(at, range, as_child),
      None => 0,
    };
    Ok(Tmpl::Component { module: format!("{}#{name}", self.file), props, children, id, keyed: false })
  }

  fn attr_value(&mut self, attr: &'p js::JSXAttr) -> Lowered<Expr> {
    match &attr.value {
      None => Ok(Expr::Lit(Lit::Bool(true))),
      Some(js::JSXAttrValue::Str(s)) => Ok(Expr::Lit(Lit::Str(decode_entities(&s.value.to_atom_lossy())))),
      Some(js::JSXAttrValue::JSXExprContainer(c)) => match &c.expr {
        js::JSXExpr::Expr(e) => self.lowerer.expr(e),
        js::JSXExpr::JSXEmptyExpr(e) => Err(self.lowerer.residue(e.span, "an empty attribute expression")),
      },
      Some(other) => Err(self.lowerer.residue(other.span(), "an element as an attribute value")),
    }
  }

  /// `style={{ a: x, bTwo: y }}` as an object keyed by CSS name, which the
  /// renderer serialises the way React does.
  fn style(&mut self, attr: &'p js::JSXAttr) -> Lowered<Expr> {
    let Some(js::JSXAttrValue::JSXExprContainer(c)) = &attr.value else {
      return self.attr_value(attr);
    };
    let js::JSXExpr::Expr(e) = &c.expr else {
      return Err(self.lowerer.residue(attr.span, "an empty style"));
    };
    let js::Expr::Object(obj) = &**e else {
      return Err(self.lowerer.residue(e.span(), "a style that is not an object literal"));
    };
    let mut entries = Vec::new();
    for prop in &obj.props {
      let (key, value) = match prop {
        js::PropOrSpread::Spread(spread) => {
          entries.push(Entry::Spread(self.lowerer.expr(&spread.expr)?));
          continue;
        }
        js::PropOrSpread::Prop(p) => match &**p {
          js::Prop::KeyValue(kv) => (prop_name(&kv.key).ok_or_else(|| self.lowerer.residue(kv.key.span(), "a computed style property"))?, self.lowerer.expr(&kv.value)?),
          js::Prop::Shorthand(id) => (id.sym.to_string(), self.lowerer.ident(id)?),
          other => return Err(self.lowerer.residue(other.span(), "a method in a style")),
        },
      };
      entries.push(Entry::Field(css_name(&key), value));
    }
    Ok(Expr::Object(entries))
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
          js::JSXExpr::Expr(e) => out.push(self.child_expr(e)?),
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

fn css_name(key: &str) -> String {
  let mut out = String::with_capacity(key.len() + 4);
  for c in key.chars() {
    if c.is_ascii_uppercase() {
      out.push('-');
      out.push(c.to_ascii_lowercase());
    } else {
      out.push(c);
    }
  }
  out
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

/// Whether a render reaching this expression would meet JSX.
fn holds_jsx(expr: &js::Expr) -> bool {
  match expr {
    js::Expr::JSXElement(_) | js::Expr::JSXFragment(_) => true,
    js::Expr::Paren(p) => holds_jsx(&p.expr),
    js::Expr::Cond(c) => holds_jsx(&c.cons) || holds_jsx(&c.alt),
    js::Expr::Bin(b) if b.op == js::BinaryOp::LogicalAnd => holds_jsx(&b.right),
    js::Expr::Call(call) => map_with_jsx(call),
    _ => false,
  }
}

/// `xs.map((x) => <jsx>)` or with a block whose return is JSX.
fn map_with_jsx(call: &js::CallExpr) -> bool {
  let js::Callee::Expr(callee) = &call.callee else { return false };
  let js::Expr::Member(member) = &**callee else { return false };
  if !matches!(&member.prop, js::MemberProp::Ident(id) if id.sym.as_ref() == "map") {
    return false;
  }
  let Some(first) = call.args.first() else { return false };
  let js::Expr::Arrow(arrow) = &*first.expr else { return false };
  match &*arrow.body {
    js::ArrowFunctionBody::Expr(e) => holds_jsx(e),
    js::ArrowFunctionBody::FunctionBody(b) => b.stmts.iter().any(|s| matches!(s, js::Stmt::Return(r) if r.arg.as_deref().is_some_and(holds_jsx))),
  }
}

#[cfg(test)]
mod tests {
  use super::*;

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
      Tmpl::Fragment(children) | Tmpl::Component { children, .. } | Tmpl::Island { children, .. } => children.iter().for_each(|c| chunk_ids(c, out)),
      Tmpl::If { then, r#else, .. } => {
        chunk_ids(then, out);
        if let Some(e) = r#else {
          chunk_ids(e, out);
        }
      }
      Tmpl::For { body, .. } => chunk_ids(body, out),
      Tmpl::Let { then, .. } => chunk_ids(then, out),
      Tmpl::Text(_) | Tmpl::Expr(_) | Tmpl::Slot(_) => {}
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
    assert_eq!(hoist_ids(component), vec![0, 6, 10], "props only: money(total), money(l.price) and money(1); not qty, tip or grand: {component:?}");
    let mut chunks = Vec::new();
    chunk_ids(&component.render, &mut chunks);
    assert_eq!(chunks, vec![("b".to_owned(), 1)], "the one static element that does work; the others read state, hold a handler or only literals: {component:?}");
    assert_eq!(set.rewrites.len(), 1);
    let rewrite = &set.rewrites[0];
    assert_eq!(rewrite.module, "routes/cart/page.tsx#default");
    assert_eq!(rewrite.sites.len(), 3);
    assert_eq!(rewrite.chunks.len(), 1);
    assert_eq!(rewrite.loops.len(), 1, "the one .map callback holding a survivor");
    let (file, source) = set.rewritten().pop().unwrap();
    assert_eq!(file, "routes/cart/page.tsx");
    assert!(source.starts_with(hoist::IMPORT), "{source}");
    assert!(source.contains("{ const __sfh = __sfUseHoisted(\"routes/cart/page.tsx#default\"); "), "{source}");
    assert!(source.contains("__sfh.c(1, (__sfHtml) => <b dangerouslySetInnerHTML={__sfHtml} />, () => (<b>{__sfh.r(0, () => (money(total)))}</b>))"), "{source}");
    assert!(source.contains("<i>{money(total * qty)}</i>"), "a state read stays a call: {source}");
    assert!(source.contains("<ul>{lines.map(__sfh.l((l) => <li key={l.price}>{__sfh.r(6, () => (money(l.price)))}{money(l.price * qty)}</li>))}</ul>"), "{source}");
    assert!(source.contains("{__sfh.r(10, () => (money(1)))}</button>"), "{source}");
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
    assert!(unlowered.is_some_and(|why| why.contains("not a state setter, an action or a handler")), "a call to something else is still residue: {attrs:?}");
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
    assert!(source.contains("{__sfh.c(2, (__sfHtml) => <ul className=\"list\" dangerouslySetInnerHTML={__sfHtml} />, () => (<ul className=\"list\">"), "{source}");
    assert!(source.contains("      </ul>))}\n"), "a chunk among JSX children is braced: {source}");
    assert!(source.contains("{__sfh.c(3, (__sfHtml) => <p dangerouslySetInnerHTML={__sfHtml} />, () => (<p>{note}</p>))}"), "{source}");
    assert!(source.contains("<button onClick={() => setOpen(!open)}>{note}</button>"), "a bound element is untouched: {source}");
    assert_eq!(source.matches("__sfh.c(").count(), 3, "{source}");
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
    assert_eq!(hoisted, vec![2, 3], "wrap(money(n)) as one hoist, toLocaleString as another; the lambda's money(i) is none: {component:?}");
    let (_, source) = set.rewritten().pop().unwrap();
    assert!(source.contains("=> { const __sfh = __sfUseHoisted(\"routes/index/page.tsx#Page\"); return (("), "an expression body becomes a block: {source}");
    assert!(source.trim_end().ends_with(")); };"), "{source}");
    assert!(source.contains("__sfh.c(4, (__sfHtml) => <p title={items.map((i) => money(i)).join(\", \")} dangerouslySetInnerHTML={__sfHtml} />, () => (<p title={items.map((i) => money(i)).join(\", \")}>"), "the whole paragraph is static, so it is a chunk whose hit keeps the attribute: {source}");
    assert!(source.contains("{__sfh.r(2, () => (wrap(money(n))))}{__sfh.r(3, () => (items.length.toLocaleString()))}</p>))"), "{source}");
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
    assert!(matches!(&children[2], Tmpl::If { r#else: None, .. }));
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
    let Tmpl::Element { children: span, .. } = &children[1] else { panic!() };
    let Tmpl::Expr(Expr::Hoist { id: 0, expr }) = &span[0] else { panic!("{:?}", span[0]) };
    assert!(matches!(&**expr, Expr::Apply { f, args } if matches!(**f, Expr::Lambda { .. }) && args.len() == 1), "{expr:?}");
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
  fn residue_names_the_line_and_the_construct() {
    let page = "export default function Page() {\n  const params = new URLSearchParams();\n  return <a href={params.toString()}>x</a>;\n}\n";
    let err = lower(&[("routes/index/page.tsx", page)], "routes/index/page.tsx#default").unwrap_err();
    assert_eq!(err.to_string(), "routes/index/page.tsx:2:18: `new`");
    let page = "import { Chart } from \"chart-lib\";\nexport default function Page() {\n  return <Chart />;\n}\n";
    let err = lower(&[("routes/index/page.tsx", page)], "routes/index/page.tsx#default").unwrap_err();
    assert!(err.to_string().contains("`Chart` comes from `chart-lib`, which the build cannot follow"), "{err}");
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
    assert_eq!(component.body, vec![Stmt::Let { name: "$let3".to_owned(), expr: Expr::Builtin { name: Builtin::Omit, args: vec![props.clone(), Expr::lit_str("title"), Expr::lit_str("kind")] } }]);
    let Tmpl::Element { attrs, .. } = &component.render else { panic!("{:?}", component.render) };
    assert_eq!(attrs[0], Entry::Field("data-id".to_owned(), Expr::var("$let3").field("id")));
    assert_eq!(attrs[1], Entry::Spread(Expr::Builtin { name: Builtin::Omit, args: vec![Expr::var("$let3"), Expr::lit_str("id")] }));
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
  fn a_statement_after_an_early_return_or_an_if_that_is_not_one_is_residue() {
    let after = "import { useState } from \"react\";\nexport default function P({ live }: { live: boolean }) {\n  if (live) return <b>live</b>;\n  const [n] = useState(0);\n  return <p>{n}</p>;\n}\n";
    let err = lower(&[("routes/index/page.tsx", after)], "routes/index/page.tsx#default").unwrap_err().to_string();
    assert_eq!(err, "routes/index/page.tsx:4:3: a statement after an early `return`; only another `if (...) return` or the final `return` can follow one");
    let otherwise = "export default function P({ live }: { live: boolean }) {\n  if (live) return <b>live</b>;\n  else return <p>still</p>;\n}\n";
    let err = lower(&[("routes/index/page.tsx", otherwise)], "routes/index/page.tsx#default").unwrap_err().to_string();
    assert_eq!(err, "routes/index/page.tsx:2:3: an `if` in a component other than `if (...) return` with no `else`");
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
  fn a_provider_tag_lowers_to_its_children_and_the_component_hydrates() {
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
    let Tmpl::Fragment(items) = &layout.render else { panic!("{:?}", layout.render) };
    let Tmpl::Element { tag, children, .. } = &items[0] else { panic!("{:?}", items[0]) };
    assert_eq!(tag, "div");
    assert_eq!(children[0], Tmpl::Element { tag: "sf-s".to_owned(), attrs: Vec::new(), children: vec![Tmpl::Slot("content".to_owned())] });
    assert_eq!(layout.hydrated_by, Some(snapfire_fsr_ir::HydratedBy::React), "a provider only exists in the browser, so the layout is a root");
  }

  #[test]
  fn a_provider_whose_object_is_not_a_context_is_residue() {
    let files = [("routes/a/page.tsx", "const Theme = { Provider: (p: { children: unknown }) => p.children };\nexport default function A() {\n  return <Theme.Provider><p>x</p></Theme.Provider>;\n}\n")];
    let err = lower(&files, "routes/a/page.tsx#default").unwrap_err().to_string();
    assert!(err.contains("`Theme` is not a `createContext` value"), "{err}");
  }

  #[test]
  fn tree_marks_a_layout_as_one_react_tree_and_is_refused_elsewhere() {
    let layout = "import type { ReactNode } from \"react\";\nimport { tree } from \"@snapfire/fsr-client/react\";\nfunction Layout({ children }: { children: ReactNode }) {\n  return <main>{children}</main>;\n}\nexport default tree(Layout);\n";
    let mut set = ComponentSet::new(&app(&[("routes/layout.tsx", layout)]));
    set.layouts.push("routes/layout.tsx#default".to_owned());
    set.lower("routes/layout.tsx#default").unwrap();
    assert_eq!(set.components[0].1.hydrated_by, Some(snapfire_fsr_ir::HydratedBy::ReactTree));
    let Tmpl::Element { children, .. } = &set.components[0].1.render else { panic!() };
    assert_eq!(children[0], Tmpl::Element { tag: "sf-s".to_owned(), attrs: Vec::new(), children: vec![Tmpl::Slot("content".to_owned())] });

    let page = [("routes/a/page.tsx", "import { tree } from \"@snapfire/fsr-client/react\";\nfunction A() {\n  return <p>x</p>;\n}\nexport default tree(A);\n")];
    let err = lower(&page, "routes/a/page.tsx#default").unwrap_err().to_string();
    assert!(err.contains("marks a layout"), "{err}");

    let odd = [("routes/layout.tsx", "import { tree } from \"@snapfire/fsr-client/react\";\nexport default tree(() => <p>x</p>);\n")];
    let mut set = ComponentSet::new(&app(&odd));
    set.layouts.push("routes/layout.tsx#default".to_owned());
    let err = set.lower("routes/layout.tsx#default").unwrap_err().to_string();
    assert!(err.contains("takes the layout's component name"), "{err}");
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

/// When a `Link` points at the page being shown.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Match {
  /// That path alone.
  Exact,
  /// That path and anything under it.
  Prefix,
}

impl Match {
  fn marker(self) -> &'static str {
    match self {
      Match::Exact => "exact",
      Match::Prefix => "prefix",
    }
  }

  /// The `aria-current` a hit writes. A section link takes `true` rather than
  /// `page` so that a nav marking both the section and the page inside it
  /// still names one page.
  fn current(self) -> &'static str {
    match self {
      Match::Exact => "page",
      Match::Prefix => "true",
    }
  }
}

/// The two attributes an active link carries: the rule, which the navigator
/// reads to keep the mark right after a navigation the layout does not
/// re-render for and `aria-current` for the page `by` names, the request's
/// path or the document's. An `href` carrying a query or a fragment never
/// matches, since the path the request matched holds neither.
fn active_attrs(rule: Match, href: Expr, by: Expr) -> [Entry; 2] {
  let hit = Expr::Compare(CompareOp::Eq, Box::new(by.clone()), Box::new(href.clone()));
  let hit = match rule {
    Match::Exact => hit,
    Match::Prefix => {
      let under = Expr::Builtin { name: Builtin::StartsWith, args: vec![by, Expr::Template(vec![href, Expr::lit_str("/")])] };
      Expr::Logic(LogicOp::Or, Box::new(hit), Box::new(under))
    }
  };
  let current = Expr::Ternary(Box::new(hit), Box::new(Expr::lit_str(rule.current())), Box::new(Expr::Lit(Lit::Null)));
  [Entry::Field("data-sf-link".to_owned(), Expr::lit_str(rule.marker())), Entry::Field("aria-current".to_owned(), current)]
}

/// A `Link`'s `keep` as the navigator reads it: `"true"` or `"false"`, since a
/// false attribute is dropped from the markup and the navigator reads a
/// missing one as leaving it the choice.
fn keep_value(value: Expr) -> Expr {
  match value {
    Expr::Lit(Lit::Bool(keep)) => Expr::lit_str(if keep { "true" } else { "false" }),
    value => Expr::Ternary(Box::new(value), Box::new(Expr::lit_str("true")), Box::new(Expr::lit_str("false"))),
  }


}
