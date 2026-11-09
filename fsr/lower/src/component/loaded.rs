use super::*;

impl ComponentSet {
  /// Marks `module` as placed with props the browser can change and, when it
  /// is already lowered, unwraps every hoist and chunk of it that reads its
  /// props, trims its rewrite to what stays and does the same for what it
  /// places with those props. One not lowered yet reads the mark when it is.
  pub(super) fn unhoist_props(&mut self, module: &str) {
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
  pub(super) fn settle(&mut self) {
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

  pub(super) fn lower_loaded(&mut self, file: &str, export: &str) -> Result<Component, LowerError> {
    let outer = std::mem::replace(&mut self.lowering_component, true);
    let (rewrites, continuations) = (self.rewrites.len(), self.continuations.len());
    let result = crate::with_heap_fresh(|| { self.rewrites.truncate(rewrites); self.continuations.truncate(continuations); self.lower_loaded_in(file, export) }, |component: &mut Component| component.body.insert(0, crate::heap_marker()));
    self.lowering_component = outer;
    result
  }

  pub(super) fn lower_loaded_in(&mut self, file: &str, export: &str) -> Result<Component, LowerError> {
    self.load_class_graph(file);
    let mut globals: Vec<(String, Expr)> = Vec::new();
    let mut global_kinds = Vec::new();
    let mut global_formatters: Vec<(String, String)> = Vec::new();
    let continued = self.continuations.len();
    let mut module_bindings: Vec<(String, String, Expr)> = Vec::new();
    if self.reaches(Reach::Later) {
      let parsed_cached = self.parsed(file);
      let parsed = parsed_cached.get();
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
    let parsed_cached = self.parsed(file);
    let parsed = parsed_cached.get();
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
      let hook_sources: Vec<(String, Rc<Cached>, String)> = hook_files.iter().map(|(name, at, export)| (name.clone(), self.parsed(at), export.clone())).collect();
      let (result, unbound, hoisting) = {
        let parsed_cached = self.parsed(file);
        let parsed = parsed_cached.get();
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
        let mut cl = ComponentLowerer { lowerer, file, handlers: Vec::new(), refs: Vec::new(), select_value: None, props_name: None, props_rest: None, children_name: None, forwards_ref: forwards_ref(&parsed, export), class, layout_root, slot_names, slot_props: Vec::new(), state: Vec::new(), hook: None, state_bindings: Vec::new(), store_bindings: Vec::new(), store_lets: Vec::new(), setters: Vec::new(), handler_fns: HashMap::new(), lowered_handlers: Vec::new(), event_binds: None, writers: Vec::new(), animations: 0, elements: self.elements.clone(), providers: Vec::new(), assets: self.assets.clone(), rewrite_images: self.rewrite_images, heads: Vec::new(), hooks: hook_sources.iter().map(|(name, parsed, export)| (name.clone(), parsed.get(), export.clone())).collect(), wanted_hook: None, pending_lets: Vec::new(), hook_calls: 0, unbound_in: None, inlining: Vec::new(), checking: std::cell::RefCell::new(Vec::new()), counts_children: Vec::new(), imports: imports.clone(), chosen: HashMap::new(), variants: Vec::new(), placed_kinds: Vec::new(), browser_texts: Vec::new(), module_lets: module_bindings.clone(), fn_refs: Vec::new(), held_refs: Vec::new(), dispatchers: Vec::new(), transitions: Vec::new(), effect_locals: Vec::new(), effect_fns: Vec::new(), stable_callbacks: Vec::new(), spawns: Vec::new(), hook_ids: 0, cleanup_for: None, hook_renames: Vec::new(), named_listeners: Vec::new(), positional: self.positional.contains(&module), positional_refs: Vec::new() };
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
              if self.load(&declared).is_ok() && find_function(self.parsed(&declared).get(), &declared_name).is_some() {
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
            let parsed_cached = self.parsed(&declared_file);
            let parsed = parsed_cached.get();
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
      let parsed_cached = self.parsed(file);
      let parsed = parsed_cached.get();
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
  pub(super) fn lower_vue(&mut self, file: &str, export: &str) -> Result<Component, LowerError> {
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
        let parsed_cached = self.parsed(file);
        let parsed = parsed_cached.get();
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
      let child_parsed = self.parsed_if(&child_file);
      if let Some(parsed) = child_parsed.as_ref().map(|c| c.get()) {
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
        let inherits = child_parsed.as_ref().is_none_or(|c| crate::vue::inherits_attrs(c.get()));
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
  pub(super) fn runs_in_browser_only(&mut self, file: &str, name: &str) -> bool {
    let Some((declared, declared_name)) = self.meanings(file).bindings.get(name).and_then(|meant| meant.origin()).map(|(f, n)| (f.to_owned(), n.to_owned())) else { return false };
    if self.load(&declared).is_err() {
      return false;
    }
    let parsed_cached = self.parsed(&declared);
    let parsed = parsed_cached.get();
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
  pub(super) fn lower_svelte(&mut self, file: &str, export: &str) -> Result<Component, LowerError> {
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
        let parsed_cached = self.parsed(file);
        let parsed = parsed_cached.get();
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
  pub(super) fn inline_hydrates(&self, tmpl: &Tmpl) -> bool {
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
  pub(super) fn component_module(&mut self, file: &str, name: &str) -> Result<(String, Option<IslandTiming>), String> {
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
        let parsed_cached = self.parsed(file);
        let parsed = parsed_cached.get();
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
  pub(super) fn exported_component(&mut self, file: &str, export: &str) -> Result<(String, Option<IslandTiming>), String> {
    let parsed_cached = self.parsed(file);
    let parsed = parsed_cached.get();
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
}
