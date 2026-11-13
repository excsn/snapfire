use super::*;

/// `error.not-found.tsx` and its kin beside `error.tsx`, as (kind, module).
pub(crate) fn error_kind_modules(dir: &Path, rel: &str) -> Vec<(String, String)> {
  let mut found = Vec::new();
  for kind in snapfire_fsr_core::ext::FailureKind::ALL {
    let infix = kind.as_str().replace('_', "-");
    for ext in COMPONENT_EXTENSIONS {
      let file = format!("error.{infix}.{ext}");
      if dir.join(&file).is_file() {
        found.push((kind.as_str().to_owned(), format!("{rel}/{file}#default")));
        break;
      }
    }
  }
  found
}

/// `build_with`, or with `skipped` given, a build that records each residue in a loader, an action, a handler, middleware or an extension there and goes on without that part rather than stopping at it.
pub(crate) fn build_in(app: &Path, options: &Options, plugins: &mut plugins::Plugins, skipped: &mut Option<Vec<snapfire_fsr_lower::Residue>>) -> Result<Built, BuildError> {
  macro_rules! lowered {
    ($e:expr) => {
      match $e {
        Ok(value) => value,
        Err(LowerError::Residue(residue)) if skipped.is_some() => {
          if let Some(all) = skipped.as_mut() {
            all.push(residue);
          }
          Default::default()
        }
        Err(e) => return Err(e.into()),
      }
    };
  }
  let routes_dir = app.join("routes");
  if !routes_dir.is_dir() {
    return Err(BuildError::NoRoutes(app.to_path_buf()));
  }

  match snapfire_fsr_host::config::Config::load(serve::project_root(app)) {
    Ok(_) | Err(snapfire_fsr_host::HostError::NoConfig(_)) => {}
    Err(e) => return Err(BuildError::Configuration(e.to_string())),
  }
  let mut report = Report::default();
  if let Some(config) = config_beside(app) {
    report.csp = doctor::csp(&config).into_iter().map(|f| (f.what, f.remedy)).collect();
  }
  let mut contract = Contract::new();
  let mut contracts: Vec<(String, Contract)> = Vec::new();
  let mut session_import: Option<String> = None;
  let mut defaults = SessionDefaults::new();

  for document in sorted_files(&app.join("clients"), ".openapi.json")? {
    let file = document.file_name().unwrap_or_default().to_string_lossy().to_string();
    let name = file.trim_end_matches(".openapi.json").to_owned();
    let text = std::fs::read_to_string(&document).map_err(|e| BuildError::Io(document.clone(), e))?;
    let imported = snapfire_fsr_service::import(&text, &name)
      .map_err(|error| BuildError::Import { document: format!("clients/{file}"), error })?;
    for service in imported.contract.services.keys() {
      report.services.push((service.clone(), format!("clients/{file}")));
    }
    contract.merge(imported.contract.clone(), &format!("clients/{file}"))?;
    contracts.push((format!("{CONTRACTS_DIR}/{name}.json"), imported.contract));
  }
  for document in sorted_files(&app.join("clients"), ".proto")? {
    let file = document.file_name().unwrap_or_default().to_string_lossy().to_string();
    let name = file.trim_end_matches(".proto").to_owned();
    let imported = snapfire_fsr_service::import_proto(&document, &name)
      .map_err(|error| BuildError::Import { document: format!("clients/{file}"), error })?;
    for service in imported.contract.services.keys() {
      report.services.push((service.clone(), format!("clients/{file}")));
    }
    contract.merge(imported.contract.clone(), &format!("clients/{file}"))?;
    contracts.push((format!("{CONTRACTS_DIR}/{name}.json"), imported.contract));
  }
  for document in sorted_files(&app.join("clients"), ".graphql")? {
    let file = document.file_name().unwrap_or_default().to_string_lossy().to_string();
    let name = file.trim_end_matches(".graphql").to_owned();
    let text = std::fs::read_to_string(&document).map_err(|e| BuildError::Io(document.clone(), e))?;
    let imported = snapfire_fsr_service::import_graphql(&text, &name)
      .map_err(|error| BuildError::Import { document: format!("clients/{file}"), error })?;
    for service in imported.contract.services.keys() {
      report.services.push((service.clone(), format!("clients/{file}")));
    }
    contract.merge(imported.contract.clone(), &format!("clients/{file}"))?;
    contracts.push((format!("{CONTRACTS_DIR}/{name}.json"), imported.contract));
  }

  // The application's own Rust sits beside `app/`, so the crate's `src/` is
  // the sibling. Read rather than compiled, which is what lets `build.rs` run
  // this before the crate exists.
  let natives = match app.parent() {
    Some(project) => native::read(&project.join("src"))?,
    None => native::Natives::default(),
  };
  let mut rust = Contract::new();
  for service in &natives.services {
    report.services.push((service.name.clone(), service.file.clone()));
    contract.adopt(service.contract.clone(), &service.file)?;
    rust.adopt(service.contract.clone(), &service.file)?;
  }
  if !rust.services.is_empty() {
    contracts.push((format!("{CONTRACTS_DIR}/{RUST_CONTRACT}"), rust));
  }

  let mut schemas = Contract::new();
  for ty in builtin_types() {
    schemas.types.insert(ty.name, ty.def);
  }
  for schema in sorted_files(&app.join("schemas"), ".ts")? {
    let file = schema.file_name().unwrap_or_default().to_string_lossy().to_string();
    let rel = format!("schemas/{file}");
    let text = std::fs::read_to_string(&schema).map_err(|e| BuildError::Io(schema.clone(), e))?;
    for ty in read_schema(&rel, &text)? {
      if ty.name == UPLOAD {
        return Err(BuildError::DuplicateType { name: ty.name, first: "the host's own".to_owned(), second: rel });
      }
      if let Some((_, first)) = report.schemas.iter().find(|(n, _)| *n == ty.name) {
        return Err(BuildError::DuplicateType { name: ty.name, first: first.clone(), second: rel });
      }
      if ty.name == "Session" {
        session_import = Some(format!("../schemas/{}", file.trim_end_matches(".ts")));
        defaults = read_session_defaults(&rel, &text)?;
      }
      report.schemas.push((ty.name.clone(), rel.clone()));
      schemas.types.insert(ty.name, ty.def);
    }
  }
  contract.merge(schemas.clone(), "schemas/")?;
  contracts.push((format!("{CONTRACTS_DIR}/schemas.json"), schemas));
  contract.validate()?;

  let elements = element_templates(app)?;
  let sections = assets::Sections::of(app);
  let public_path = bundle_base(options.site.as_ref());
  let resolver = std::rc::Rc::new(assets::Resolver::new(app, &public_path, sections.clone()));
  let mut set = ComponentSet::new(app).with_defaults(defaults.clone()).provide(snapfire_fsr_lower::HEAD_MODULE, HEAD_HELPERS).with_elements(elements.iter().cloned().collect()).with_assets(resolver.clone(), sections.images.rewrite);
  let terminal = is_terminal_app(app);
  if terminal {
    set = set.for_terminal();
  } else if app.join(ROOT_PAGE).is_file() && set.draws_in_terminal(ROOT_PAGE) {
    return Err(BuildError::TerminalUndeclared { file: ROOT_PAGE.to_owned() });
  }
  let mdx_settings = mdx::settings(&config_beside(app).map(|config| config.mdx).unwrap_or_default())?;
  #[cfg_attr(not(feature = "mdx"), allow(unused_mut))]
  let mut mdx_files: Vec<(String, String)> = Vec::new();
  #[cfg(feature = "mdx")]
  let mdx_installed = {
    let class_attr = match types::jsx_source(app, &crate::xwpm::Layout::of_site(app, options.site.as_ref())?)? {
      "react" => snapfire_fsr_mdx::ClassAttr::ClassName,
      _ => snapfire_fsr_mdx::ClassAttr::Class,
    };
    let (installed_set, installed) = mdx::install(app, set, &mdx_settings, class_attr)?;
    set = installed_set;
    if let Some(content) = mdx::collections(app, &installed)? {
      set = set.provide(mdx::CONTENT_MODULE, content.clone());
      mdx_files.push((mdx::CONTENT_MODULE.to_owned(), content));
    }
    mdx_files.push((mdx::DECLARATIONS_FILE.to_owned(), mdx::DECLARATIONS.to_owned()));
    installed
  };
  let _ = &mdx_settings;
  describe_foreign(app, &mut set, &mut report, plugins)?;
  for file in sorted_files(&app.join(EXT_DIR), ".ts")? {
    let rel = format!("{EXT_DIR}/{}", file.file_name().unwrap_or_default().to_string_lossy());
    report.extensions.extend(lowered!(set.lower_extensions(&rel)));
  }
  for (_, module) in &elements {
    set.lower(module)?;
    let refused = |reason: String| BuildError::ElementTemplate { module: module.clone(), reason };
    let Some((_, component)) = set.components.iter_mut().find(|(m, _)| m == module) else { continue };
    if component.owner.hydrates() {
      return Err(refused("it holds state or a handler, which belong in the element's class".to_owned()));
    }
    component.shadow = snapfire_fsr_ir::ShadowRoot::take(&mut component.render).map_err(|e| refused(e.to_string()))?;
  }

  let mut routes = Vec::new();
  let mut handler_routes = Vec::new();
  discover(&routes_dir, &routes_dir, &mut routes, &mut handler_routes)?;
  routes.sort_by(|a, b| a.pattern.cmp(&b.pattern));
  handler_routes.sort_by(|a, b| a.pattern.cmp(&b.pattern));

  let root_boundary = Boundary::read(&routes_dir, "routes");

  let not_found_module = component_file(&routes_dir, "not-found").map(|f| format!("routes/{f}#default"));

  let mut entries = Vec::new();
  let mut sources = Vec::new();
  let mut actions = Vec::new();
  let mut islands: Vec<String> = Vec::new();
  let mut templates: Vec<PathBuf> = Vec::new();
  for module in root_boundary.modules().chain(not_found_module.iter()) {
    islands.push(module.clone());
  }

  let mut layouts: Vec<LayoutInfo> = Vec::new();
  let mut layout_ids: Vec<String> = Vec::new();
  for dir in routes.iter().chain(handler_routes.iter()).flat_map(|r| r.dir.ancestors().map(Path::to_path_buf).collect::<Vec<_>>()) {
    if !dir.starts_with(&routes_dir) || layouts.iter().any(|l| l.dir == dir) {
      continue;
    }
    let Some(layout_file) = layout_file(&dir)? else { continue };
    let rel = dir.strip_prefix(app).unwrap_or(&dir).to_string_lossy().replace('\\', "/");
    let (prefix, dir_id) = pattern_of(&routes_dir, &dir)?;
    let id = if dir == routes_dir { "layout".to_owned() } else { format!("{dir_id}.layout") };
    let module = format!("{rel}/{layout_file}#default");
    let loader = dir.join("layout.loader.ts");
    let source = if loader.is_file() {
      let loader_module = format!("{rel}/layout.loader.ts");
      let body = lowered!(set.lower_loader(&loader_module));
      let meta = lowered!(set.lower_meta(&loader_module));
      refuse_inline_scripts(&loader_module, meta.as_ref())?;
      let store = lowered!(set.lower_store(&loader_module));
      let defaults = lowered!(set.lower_defaults(&loader_module));
      if lowered!(set.lower_paths(&loader_module)).is_some() {
        return Err(BuildError::PathsOffPage(loader_module));
      }
      sources.push(SourceEntry::lowered(id.clone(), loader_module.clone(), body).with_meta(meta).with_store(store).with_defaults(defaults));
      report.sources.push((id.clone(), loader_module));
      Some(id.clone())
    } else {
      None
    };
    report.layouts.push((prefix, module.clone()));
    if is_template(&module) {
      report.components.push((module.clone(), "template".to_owned(), String::new()));
      templates.push(dir.join(&layout_file));
    } else {
      islands.push(module.clone());
    }
    layout_ids.push(id.clone());
    let mut slots = Vec::new();
    for slot_dir in sorted_dirs(&dir.join("slots"))? {
      let name = slot_dir.file_name().unwrap_or_default().to_string_lossy().to_string();
      let Some(slot_page) = page_file(&slot_dir)? else {
        return Err(BuildError::SlotWithoutPage(slot_dir));
      };
      for d in sorted_dirs(&slot_dir)? {
        if page_file(&d)?.is_some() || d.join("route.ts").is_file() {
          return Err(BuildError::SlotRoute(slot_dir));
        }
      }
      let slot_rel = format!("{rel}/slots/{name}");
      let slot_id = format!("{id}.{name}");
      let page = format!("{slot_rel}/{slot_page}#default");
      let loader = slot_dir.join("page.loader.ts");
      let source = if loader.is_file() {
        let loader_module = format!("{slot_rel}/page.loader.ts");
        let body = lowered!(set.lower_loader(&loader_module));
        let meta = lowered!(set.lower_meta(&loader_module));
        refuse_inline_scripts(&loader_module, meta.as_ref())?;
        let store = lowered!(set.lower_store(&loader_module));
      let defaults = lowered!(set.lower_defaults(&loader_module));
        if lowered!(set.lower_paths(&loader_module)).is_some() {
          return Err(BuildError::PathsOffPage(loader_module));
        }
        sources.push(SourceEntry::lowered(slot_id.clone(), loader_module.clone(), body).with_meta(meta).with_store(store).with_defaults(defaults));
        report.sources.push((slot_id.clone(), loader_module));
        Some(slot_id.clone())
      } else {
        None
      };
      let slot_actions = slot_dir.join("actions.ts");
      if slot_actions.is_file() {
        let module = format!("{slot_rel}/actions.ts");
        for lowered in lowered!(set.lower_actions(&module)) {
          let action_id = format!("{slot_id}.{}", lowered.export);
          if let Some(name) = &lowered.input {
            if !contract.types.contains_key(name) {
              return Err(BuildError::UnknownInput { action: action_id, name: name.clone() });
            }
          }
          let mut entry = ActionEntry::lowered(action_id.clone(), module.clone(), lowered.body);
          entry.export = Some(lowered.export);
          entry.input = lowered.input;
          actions.push(entry);
          report.actions.push((action_id, module.clone()));
        }
      }

      let loading = component_file(&slot_dir, "loading").map(|f| format!("{slot_rel}/{f}#default"));
      let error = Boundary::read(&slot_dir, &slot_rel);
      for module in [Some(&page), loading.as_ref()].into_iter().flatten().chain(error.modules()) {
        if is_template(module) {
          report.components.push((module.clone(), "template".to_owned(), String::new()));
          templates.push(slot_dir.join(&slot_page));
        } else {
          islands.push(module.clone());
        }
      }
      report.slots.push((slot_id.clone(), page.clone()));
      layout_ids.push(slot_id);
      slots.push(SlotInfo { name, page, source, loading, error });
    }
    layouts.push(LayoutInfo { dir, module, source, slots, placed: Vec::new() });
  }
  layouts.sort_by(|a, b| a.dir.cmp(&b.dir));

  set.layouts = layouts.iter().filter(|l| !is_template(&l.module)).map(|l| l.module.clone()).collect();
  set.slots = layouts.iter().filter(|l| !is_template(&l.module)).map(|l| (l.module.clone(), l.slots.iter().map(|s| s.name.clone()).collect())).collect();
  for layout in &mut layouts {
    if is_template(&layout.module) {
      continue;
    }
    lower_into(&mut set, &layout.module, &mut report)?;
    if let Some((_, component)) = set.components.iter().find(|(m, _)| *m == layout.module) {
      layout.placed = slots_placed(&component.render);
    }
  }
  let mut intercepts = Vec::new();

  for route in &routes {
    let rel = route.dir.strip_prefix(app).unwrap_or(&route.dir);
    let rel = rel.to_string_lossy().replace('\\', "/");
    let page_file = page_file(&route.dir)?.expect("a discovered route has a page file");
    let page = format!("{rel}/{page_file}#default");
    report.routes.push((route.pattern.clone(), rel.clone()));

    let wrapping: Vec<&LayoutInfo> = layouts.iter().filter(|l| route.dir.starts_with(&l.dir)).collect();
    let loader = route.dir.join("page.loader.ts");
    let page_source = format!("{rel}/{page_file}");
    let page_meta = match snapfire_fsr_lower::component::is_transformed(&page_file) {
      true => lowered!(set.lower_meta(&page_source)),
      false => None,
    };
    let source = if loader.is_file() {
      let module = format!("{rel}/page.loader.ts");
      let body = lowered!(set.lower_loader(&module));
      let meta = match (lowered!(set.lower_meta(&module)), page_meta) {
        (Some(_), Some(_)) => return Err(BuildError::MetaTwice { module, page: page_source }),
        (meta, page_meta) => meta.or(page_meta),
      };
      refuse_inline_scripts(&module, meta.as_ref())?;
      let store = lowered!(set.lower_store(&module));
      let defaults = lowered!(set.lower_defaults(&module));
      let paths = lowered!(set.lower_paths(&module));
      if paths.is_some() && !route.pattern.contains('{') {
        return Err(BuildError::PathsWithoutParameter { module, pattern: route.pattern.clone() });
      }
      sources.push(SourceEntry::lowered(route.id.clone(), module.clone(), body).with_meta(meta).with_store(store).with_defaults(defaults).with_paths(paths));
      report.sources.push((route.id.clone(), module));
      Some(route.id.clone())
    } else if let Some(meta) = page_meta {
      refuse_inline_scripts(&page_source, Some(&meta))?;
      let empty = vec![snapfire_fsr_ir::ast::Stmt::Return(snapfire_fsr_ir::ast::Expr::Object(Vec::new()))];
      sources.push(SourceEntry::lowered(route.id.clone(), page_source.clone(), empty).with_meta(Some(meta)));
      report.sources.push((route.id.clone(), page_source));
      Some(route.id.clone())
    } else {
      None
    };

    let actions_file = route.dir.join("actions.ts");
    if actions_file.is_file() {
      let module = format!("{rel}/actions.ts");
      for lowered in lowered!(set.lower_actions(&module)) {
        let id = format!("{}.{}", route.id, lowered.export);
        if let Some(name) = &lowered.input {
          if !contract.types.contains_key(name) {
            return Err(BuildError::UnknownInput { action: id, name: name.clone() });
          }
        }
        let mut entry = ActionEntry::lowered(id.clone(), module.clone(), lowered.body);
        entry.export = Some(lowered.export);
        entry.input = lowered.input;
        actions.push(entry);
        report.actions.push((id, module.clone()));
      }
    }

    let local = Boundary::read(&route.dir, &rel);
    let boundary = local.or(&root_boundary);
    let loading = component_file(&route.dir, "loading").map(|f| format!("{rel}/{f}#default"));

    for module in [Some(&page), loading.as_ref()].into_iter().flatten().chain(local.modules()) {
      if is_template(module) {
        report.components.push((module.clone(), "template".to_owned(), String::new()));
        templates.push(route.dir.join(&page_file));
      } else if !islands.contains(module) {
        islands.push(module.clone());
      }
    }

    let content = Node {
      id: 0,
      module: page.clone(),
      source: source.clone(),
      deferred: loading.is_some(),
      fallback: loading.clone(),
      error: boundary.module.clone(),
      error_kinds: boundary.kinds.clone(),
      cache_key: Some(page),
      children: Vec::new(),
      keep: Vec::new(),
    };
    let content = wrap_in_layouts(content, &wrapping, &root_boundary);
    entries.push(RouteEntry { pattern: route.pattern.clone(), plan: shell_over(options, content) });

    for (file, slot) in variant_files(&route.dir)? {
      let module = format!("{rel}/{file}#default");
      let Some(declaring) = wrapping.iter().rposition(|l| l.declares(&slot)) else {
        return Err(BuildError::SlotUndeclared { path: route.dir.clone(), file, slot });
      };
      islands.push(module.clone());
      let slot_loading = component_file(&route.dir, &format!("loading.{slot}")).map(|f| format!("{rel}/{f}#default"));
      if let Some(loading) = &slot_loading {
        islands.push(loading.clone());
      }
      let variant = Node {
        id: 0,
        module: module.clone(),
        source: source.clone(),
        deferred: slot_loading.is_some(),
        fallback: slot_loading,
        error: boundary.module.clone(),
        error_kinds: boundary.kinds.clone(),
        cache_key: Some(module.clone()),
        children: Vec::new(),
        keep: Vec::new(),
      };
      let plan = intercept_plan(variant, &slot, &wrapping[..=declaring], &root_boundary);
      report.intercepts.push((format!("{} into {slot}", route.pattern), module));
      intercepts.push(RouteEntry { pattern: route.pattern.clone(), plan: shell_over(options, plan) });
    }
  }

  let mut handlers = Vec::new();
  for route in &handler_routes {
    let rel = route.dir.strip_prefix(app).unwrap_or(&route.dir);
    let rel = rel.to_string_lossy().replace('\\', "/");
    let module = format!("{rel}/route.ts");
    for lowered in lowered!(set.lower_handlers(&module)) {
      let id = format!("{}.{}", route.id, lowered.method);
      if let Some(name) = &lowered.input {
        if !contract.types.contains_key(name) {
          return Err(BuildError::UnknownHandlerInput { handler: id, name: name.clone() });
        }
      }
      let mut entry = HandlerEntry::lowered(id, lowered.method.clone(), route.pattern.clone(), module.clone(), lowered.body);
      entry.input = lowered.input;
      report.handlers.push((format!("{} {}", lowered.method, route.pattern), module.clone()));
      handlers.push(entry);
    }
  }

  let middleware_file = app.join("middleware.ts");
  let middleware = if middleware_file.is_file() {
    report.middleware = Some("middleware.ts".to_owned());
    Some(lowered!(set.lower_middleware("middleware.ts")))
  } else {
    None
  };

  let mut not_found = not_found_module.map(|module| {
    let wrapping: Vec<&LayoutInfo> = layouts.iter().filter(|l| l.dir == routes_dir).collect();
    let content = Node { id: 0, module: module.clone(), source: None, deferred: false, fallback: None, error: root_boundary.module.clone(), error_kinds: root_boundary.kinds.clone(), cache_key: Some(module), children: Vec::new(), keep: Vec::new() };
    shell_over(options, wrap_in_layouts(content, &wrapping, &root_boundary))
  });
  for plan in entries.iter_mut().map(|e| &mut e.plan).chain(intercepts.iter_mut().map(|e| &mut e.plan)).chain(not_found.iter_mut()) {
    renumber(plan, &mut 0);
  }

  for file in &templates {
    for placed in template_islands(file)? {
      if !islands.contains(&placed) {
        islands.push(placed);
      }
    }
  }
  // A page or layout holding state, handlers or effects is split before it is
  // lowered: they move with the markup that uses them into an island beside
  // it, which only the bundle overlay holds.
  let mut generated: Vec<(String, String)> = Vec::new();
  for module in islands.iter().filter(|m| !terminal && m.starts_with("routes/") && !m.contains(".tera#")) {
    match set.extract_route(module) {
      Ok(Some(extracted)) => {
        report.extracted.push((module.clone(), extracted.island.clone(), extracted.holds));
        generated.push((extracted.file, extracted.source));
        generated.push(extracted.page);
      }
      Ok(None) => {}
      Err(why) => report.kept.push((module.clone(), why)),
    }
  }
  let mut split_from: std::collections::HashMap<String, snapfire_fsr_lower::Residue> = std::collections::HashMap::new();
  for module in &islands {
    if !terminal && module.starts_with("routes/") && !module.contains(".tera#") {
      match set.lower_route(module) {
        Ok(Some(extracted)) => {
          if let Some(started) = extracted.started {
            split_from.insert(extracted.island.clone(), started);
          }
          report.extracted.push((module.clone(), extracted.island.clone(), extracted.holds));
          generated.retain(|(file, _)| *file != extracted.page.0);
          generated.push((extracted.file, extracted.source));
          generated.push(extracted.page);
          continue;
        }
        Ok(None) => continue,
        // A page or layout is composition and never renders in the browser,
        // so a residue nothing could move into an island stops the build. An
        // island split out of a page is not composition.
        Err(e) if !generated.iter().any(|(file, _)| module.split_once('#').is_some_and(|(f, _)| f == file)) => return Err(e.into()),
        Err(_) => {}
      }
    }
    lower_into(&mut set, module, &mut report)?;
  }
  if let (true, Some((placed, why))) = (terminal, set.browser_only.first()) {
    return Err(BuildError::BrowserOnlyInTerminal { placed: placed.clone(), why: why.to_string() });
  }
  let (generated_images, generated_heads) = generated_images(app, &routes_dir, &routes, &mut set, &mut sources)?;
  for (module, component) in &set.components {
    for placed in server_islands(&component.render) {
      let Some((_, inner)) = set.components.iter().find(|(m, _)| *m == placed) else { continue };
      if let Some(reason) = unlowered_handler(&inner.render) {
        return Err(BuildError::ServerIsland { module: placed.clone(), reason: format!("{module} places it there, but a handler did not lower: {reason}") });
      }
      if let Some((name, index)) = captured_by_handler(inner) {
        let reason = format!(
          "{module} places it there, but handler {index} reads `{name}`, which the markup around it bound: a handler runs with the props, the state and the event, so put the value on the element and read it from `e.target`"
        );
        return Err(BuildError::ServerIsland { module: placed.clone(), reason });
      }
      let mut nested = nested_components(&inner.render);
      let mut i = 0;
      while i < nested.len() {
        if let Some((_, component)) = set.components.iter().find(|(m, _)| *m == nested[i]) {
          for below in nested_components(&component.render) {
            if !nested.contains(&below) {
              nested.push(below);
            }
          }
        }
        i += 1;
      }
      let mut handlers = inner.handlers.len();
      for within in &nested {
        let Some((_, component)) = set.components.iter().find(|(m, _)| m == within) else { continue };
        if let Some(reason) = unlowered_handler(&component.render) {
          return Err(BuildError::ServerIsland { module: placed.clone(), reason: format!("{module} places it there, but a handler of `{within}` inside it did not lower: {reason}") });
        }
        if let Some((name, index)) = captured_by_handler(component) {
          let reason = format!(
            "{module} places it there, but handler {index} of `{within}` inside it reads `{name}`, which the markup around it bound: a handler runs with the props, the state and the event, so put the value on the element and read it from `e.target`"
          );
          return Err(BuildError::ServerIsland { module: placed.clone(), reason });
        }
        handlers += component.handlers.len();
      }
      if let Some(slot) = first_slot(&inner.render) {
        let what = if slot == "content" { "`children`".to_owned() } else { format!("the slot `{slot}`") };
        return Err(BuildError::ServerIsland { module: placed.clone(), reason: format!("{module} places it there, but it renders {what}, which a step would drop: a server island's markup is its own component's and nothing else fills a part of it") });
      }
      let row = (format!("{}{placed}", options.prefix()), handlers);
      if !report.islands.contains(&row) {
        report.islands.push(row);
      }
    }
  }
  report.islands.sort();
  set.settle_proofs();
  set.settle_fills();
  for rewrite in &mut set.rewrites {
    rewrite.module = format!("{}{}", options.prefix(), rewrite.module);
    report.hoisted.push(Hoisted {
      module: rewrite.module.clone(),
      values: rewrite.sites.len(),
      subtrees: rewrite.chunks.len(),
      fallbacks: rewrite.chunks.iter().filter(|c| !c.proven).count(),
      islands: rewrite.islands.len(),
      keyed: rewrite.placements.len(),
      ids: rewrite.ids.len(),
      browser: rewrite.browser.len(),
    });
  }
  let mut rewritten = set.rewritten();
  for (file, source) in &generated {
    if !rewritten.iter().any(|(f, _)| f == file) {
      rewritten.push((file.clone(), source.clone()));
    }
  }
  report.hoisted.sort();
  report.browser = set.remaining.iter().chain(set.browser_texts.iter()).filter(|_| !terminal).map(|(module, site)| (format!("{}{module}", options.prefix()), site.clone())).collect();
  for (module, component) in &set.components {
    let file = module.split('#').next().unwrap_or(module);
    for why in unlowered_handlers(&component.render) {
      report.browser.push((format!("{}{module}", options.prefix()), format!("{file}:{why}")));
    }
  }
  report.browser.sort();
  let mut components = Vec::new();
  // A terminal draws every component itself, so none has a browser twin.
  let mut islands = if terminal { Vec::new() } else { islands };
  // Modules an `<Island define>` names: the browser imports one at its timing and the element upgrades itself.
  let mut defines: Vec<String> = Vec::new();
  // A template nothing mounts has no browser twin: it is left out of the
  // island registry and out of the bundle, so a page of such templates loads
  // no framework at all.
  let mut static_modules: Vec<String> = Vec::new();
  // What renders each lowered module in the browser, which picks its adapter.
  let mut owners: HashMap<String, Owner> = HashMap::new();
  let local_stills = sections.media.embed_stills == snapfire_fsr_host::config::EmbedStills::Local;
  for (module, residue) in std::mem::take(&mut set.foreign_residue) {
    let at = format!("{}:{}:{}", residue.file, residue.line, residue.column);
    if local_stills && places_embed(app, &module) {
      report.browser_embeds.push(module.clone());
    }
    report.components.push((module.clone(), "foreign".to_owned(), at.clone()));
    match report.foreign.iter_mut().find(|c| c.at == at && c.message == residue.message) {
      Some(cause) => cause.pages.push((module, String::new())),
      None => report.foreign.push(Cause { at, message: residue.message, hint: residue.hint, pages: vec![(module, String::new())] }),
    }
  }
  for (module, component) in std::mem::take(&mut set.components) {
    let detail = match component.owner {
      Owner::Vue | Owner::Svelte => component.owner.as_str().to_owned(),
      Owner::React => String::new(),
      Owner::Fsr => "static".to_owned(),
    };
    report.components.push((module.clone(), "lowered".to_owned(), detail));
    if sections.media.embed_stills == snapfire_fsr_host::config::EmbedStills::Local {
      for player in snapfire_fsr_ir::ast::computed_embed_players(&component.render) {
        if let Some(provider) = snapfire_fsr_lower::assets::embed_provider(&player) {
          report.embeds.push((module.clone(), provider.to_owned()));
        }
      }
    }
    if !component.owner.hydrates() {
      static_modules.push(module.clone());
    }
    owners.insert(module.clone(), component.owner);
    for placement in island_modules(&component.render) {
      if placement.define && !defines.contains(&placement.module) {
        defines.push(placement.module.clone());
      }
      if !placement.server && !islands.contains(&placement.module) {
        islands.push(placement.module);
      }
    }
    let head = set.heads.get(&module).map(|rows| rows.iter().map(|row| snapfire_fsr_plan::HeadRow { tag: row.tag.clone(), attrs: row.attrs.clone() }).collect()).unwrap_or_default();
    components.push(ComponentEntry { module, body: component, head });
  }
  // A component that does not lower renders in the browser alone, as a
  // component of the application's JSX framework. FSR is no JSX framework,
  // so an application without one has nothing to run it with.
  let uses_react = types::serves_react(app, &crate::xwpm::Layout::of_site(app, options.site.as_ref())?)? || owners.values().any(|owner| *owner == Owner::React);
  for (module, residue) in std::mem::take(&mut set.browser_only) {
    let at = format!("{}:{}:{}", residue.file, residue.line, residue.column);
    if !uses_react {
      let residue = split_from.remove(&module).unwrap_or(residue);
      let at = format!("{}:{}:{}", residue.file, residue.line, residue.column);
      return Err(BuildError::NoJsxFramework { module, at, reason: residue.message });
    }
    let chain = residue.chain();
    if local_stills && places_embed(app, &module) {
      report.browser_embeds.push(module.clone());
    }
    blame(&mut report, &module, at, residue.message, residue.hint, chain);
    owners.insert(module.clone(), Owner::React);
    if !islands.contains(&module) {
      islands.push(module);
    }
  }
  report.embed_failures = resolver.stills_failed.borrow().clone();
  if options.strict {
    if let Some(refused) = strict_refusal(&report) {
      return Err(BuildError::Strict(refused));
    }
  }
  // A component a page places as an island is mounted because the page asked
  // for it, whatever its own markup would need.
  let placements: Vec<Placement> = components.iter().flat_map(|entry| island_modules(&entry.body.render)).collect();
  if let Some(both) = placements.iter().find(|p| p.server && placements.iter().any(|q| !q.server && q.module == p.module)) {
    return Err(BuildError::ServerIsland { module: both.module.clone(), reason: "it is placed in server mode and also as a browser island, and a module has one owner".to_owned() });
  }
  let mut placed: Vec<String> = placements.into_iter().filter(|p| !p.server).map(|p| p.module).collect();
  // A component a framework hydrates that composition renders inline is
  // placed as an island by the renderer, so it mounts like any other.
  for entry in components.iter().filter(|entry| !terminal && entry.body.owner == Owner::Fsr) {
    for module in inline_components(&entry.body.render) {
      if owners.get(&module).is_some_and(|owner| owner.hydrates()) && !placed.contains(&module) {
        if !islands.contains(&module) {
          islands.push(module.clone());
        }
        placed.push(module);
      }
    }
  }
  for module in std::mem::take(&mut set.mounted).into_iter().filter(|_| !terminal) {
    if !islands.contains(&module) {
      islands.push(module);
    }
  }
  static_modules.retain(|module| !placed.contains(module));
  for (file, text) in dialect_rewrites(app, &mut set, &islands, &static_modules, &defines, &owners, &rewritten)? {
    match rewritten.iter_mut().find(|(f, _)| *f == file) {
      Some((_, held)) => *held = text,
      None => rewritten.push((file, text)),
    }
  }
  if types::jsx_source(app, &crate::xwpm::Layout::of_site(app, options.site.as_ref())?)? == "react" {
    for (file, text) in foreign_rewrites(app, &mut set, &rewritten)? {
      match rewritten.iter_mut().find(|(f, _)| *f == file) {
        Some((_, held)) => *held = text,
        None => rewritten.push((file, text)),
      }
    }
  }
  let react = types::serves_react(app, &crate::xwpm::Layout::of_site(app, options.site.as_ref())?)?;
  let pointed = components_rewrites(app, &mut set, &islands, &owners, react, &rewritten)?;
  let svelte_components = pointed.iter().any(|(file, _)| file.to_ascii_lowercase().ends_with(".svelte"));
  for (file, text) in pointed {
    match rewritten.iter_mut().find(|(f, _)| *f == file) {
      Some((_, held)) => *held = text,
      None => rewritten.push((file, text)),
    }
  }
  let mut exported: Vec<String> = Vec::new();
  for module in &islands {
    let Some((file, name)) = module.split_once('#') else { continue };
    if name == "default" || snapfire_fsr_lower::component::is_foreign(file) || snapfire_fsr_lower::component::is_transformed(file) || generated.iter().any(|(f, _)| f == file) {
      continue;
    }
    let source = match rewritten.iter().find(|(f, _)| f == file) {
      Some((_, text)) => text.clone(),
      None => match std::fs::read_to_string(app.join(file)) {
        Ok(text) => text,
        Err(_) => continue,
      },
    };
    if !snapfire_fsr_lower::declares_unexported(file, &source, name) {
      continue;
    }
    let text = format!("{source}\nexport {{ {name} }};\n");
    match rewritten.iter_mut().find(|(f, _)| f == file) {
      Some((_, held)) => *held = text,
      None => rewritten.push((file.to_owned(), text)),
    }
    exported.push(module.clone());
  }
  for (module, _, detail) in &mut report.components {
    if placed.contains(module) && detail == "static" {
      detail.clear();
    }
  }
  report.components.sort();
  let layout = crate::xwpm::Layout::of_site(app, options.site.as_ref())?;
  let mut vue_declarations: Vec<(String, String)> = set.described().filter_map(|(file, described)| snapfire_fsr_lower::vue::props_declaration(file, described).map(|declaration| (snapfire_fsr_lower::vue::props_declaration_path(file), declaration))).collect();
  vue_declarations.sort();
  let mut shadowed_declarations: Vec<(String, String)> = Vec::new();
  let shadowed: Vec<(String, String)> = types::shadowed_components(app)
    .into_iter()
    .map(|(file, _)| {
      let typed = snapfire_fsr_lower::vue::props_declaration_path(&file);
      match vue_declarations.iter().any(|(path, _)| *path == typed) {
        true => (file, typed),
        false => {
          let path = types::shadowed_declaration_path(&file);
          shadowed_declarations.push((path.clone(), types::shadowed_declaration(&file)));
          (file, path)
        }
      }
    })
    .collect();
  refuse_shadowed_relative_imports(app, &mut set)?;
  let shim = types::foreign_shim(app, &set.foreign);
  let mut browser_route_files: Vec<String> = report
    .components
    .iter()
    .filter(|(module, owner, detail)| module.starts_with("routes/") && (owner == "client" || detail != "static"))
    .filter_map(|(module, _, _)| module.split_once('#').map(|(file, _)| file.to_owned()))
    .collect();
  browser_route_files.sort();
  browser_route_files.dedup();
  let islands_generated: Vec<&String> = report.extracted.iter().filter_map(|(_, island, _)| island.split_once('#').map(|(file, _)| file)).map(|file| generated.iter().find(|(f, _)| f == file).map(|(f, _)| f)).flatten().collect();
  let mut generated_files: Vec<String> = islands_generated.into_iter().cloned().collect();
  browser_route_files.retain(|file| !generated_files.contains(file));
  let mut roots = browser_roots(app, &islands, &static_modules, &browser_route_files, &generated_files, &public_path);
  let function_modules = set.function_modules();
  for file in &function_modules {
    if !roots.contains(file) {
      roots.push(file.clone());
    }
  }
  if !uses_react {
    refuse_unrendered(&mut set, &rewritten, &roots)?;
  }
  let mut browser_files: Vec<String> = Vec::new();
  for root in &roots {
    for file in set.local_reach(root) {
      if !browser_files.contains(&file) {
        browser_files.push(file);
      }
    }
  }
  for file in &browser_files {
    if let Some(line) = set.host_import(file) {
      return Err(BuildError::HostInBrowser { file: file.clone(), line });
    }
  }
  let derives = browser_derives(&mut set, &browser_files, &mut report)?;
  report.causes.sort_by(|a, b| (&a.at, &a.message).cmp(&(&b.at, &b.message)));
  for cause in &mut report.causes {
    cause.pages.sort();
  }

  let session_type = session_import.as_ref().map(|_| "Session");
  let prefix = options.prefix();
  let config = public_of(app);
  let mut image_sources: Vec<String> = generated_images.iter().filter_map(|image| image.source.clone()).filter(|id| !routes.iter().any(|r| r.id == *id) && !layout_ids.contains(id)).collect();
  image_sources.sort();
  image_sources.dedup();
  let client = client_module(&contract, &routes, &layout_ids, &image_sources, &sources, &actions, &prefix, &natives);
  let shell = match options.site.as_ref().and_then(|site| site.shell.as_ref()) {
    Some(path) => Some((path, ShellContract::read(path)?)),
    None => None,
  };
  let mut theme_names = theme_files(&app.join(&sections.dirs.styles).join(THEMES_DIR))?;
  for name in shell.iter().flat_map(|(_, contract)| contract.themes.iter()) {
    if !theme_names.contains(name) {
      theme_names.push(name.clone());
    }
  }
  theme_names.sort();
  report.themes = theme_names.clone();
  let root_tokens = themes::root_tokens(&app.join(&sections.dirs.styles))?;
  let named_themes = themes::named_themes(app, &sections.dirs.styles)?;
  let shell_tokens = shell.as_ref().map(|(_, contract)| contract.tokens.clone());
  let written_themes = themes::write(&sections.dirs.styles, &named_themes, &root_tokens, shell_tokens.as_deref())?;
  report.theme_warnings = written_themes.warnings.clone();
  let mut theme_entries: Vec<snapfire_fsr_plan::ThemeEntry> = Vec::new();
  let mut route_modules: Vec<String> = Vec::new();
  for plan in entries.iter().map(|e| &e.plan).chain(intercepts.iter().map(|e| &e.plan)).chain(not_found.iter()) {
    plan_modules(plan, &mut route_modules);
  }
  for module in route_modules {
    let Some(file) = module.strip_suffix("#default") else { continue };
    let stem = Path::new(file).file_stem().and_then(|s| s.to_str()).unwrap_or("");
    let component = COMPONENT_EXTENSIONS.iter().any(|ext| file.ends_with(&format!(".{ext}")));
    if !file.starts_with("routes/") || !component || !(stem == "layout" || stem.starts_with("page")) {
      continue;
    }
    let Some(theme) = set.theme_export(file)? else { continue };
    if !theme_names.contains(&theme.name) {
      let known = if theme_names.is_empty() { format!("none: add {}/{THEMES_DIR}/{}.css", sections.dirs.styles, theme.name) } else { theme_names.join(", ") };
      return Err(BuildError::UnknownTheme { module, name: theme.name, known });
    }
    report.theme_of.push((module.clone(), if theme.force { format!("{} forced", theme.name) } else { theme.name.clone() }));
    theme_entries.push(snapfire_fsr_plan::ThemeEntry { module, name: theme.name, force: theme.force });
  }
  let theme_body = if app.join("theme.ts").is_file() {
    report.theme_body = Some("theme.ts".to_owned());
    Some(lowered!(set.lower_theme("theme.ts")))
  } else {
    None
  };
  check_vendor_urls(app, &layout)?;
  check_vendor_imports(app, &layout, shell.as_ref().map(|(_, contract)| contract))?;
  check_shell_urls(app, &layout, shell.as_ref().map(|(path, contract)| (path.as_path(), contract)))?;
  let (frameworks, unchecked, unused) = vendored_frameworks(app, &layout, shell.as_ref().map(|(path, contract)| (path.as_path(), contract)))?;
  report.unchecked = unchecked;
  report.shadowed = unused;
  plugin_compilers(app, &layout)?;
  let clients: Vec<snapfire_fsr_plan::ClientEntry> = report
    .components
    .iter()
    .filter(|(_, owner, _)| owner == "client")
    .map(|(module, _, at)| {
      let cause = report.causes.iter().find(|c| &c.at == at && c.pages.iter().any(|(page, _)| page == module));
      snapfire_fsr_plan::ClientEntry {
        module: module.clone(),
        at: at.clone(),
        message: cause.map(|c| c.message.clone()).unwrap_or_default(),
        hint: cause.and_then(|c| c.hint.clone()),
      }
    })
    .collect();
  // What a deferred segment promises a first wave: the keys its `store` is
  // inferred to return.
  let mut sources = sources;
  for source in &mut sources {
    let Some(loader) = &source.body else { continue };
    let mut keys: Option<Vec<String>> = None;
    for body in [&source.store, &source.defaults].into_iter().flatten() {
      match seeded(&contract, session_type, &set.consts, &config, loader, body) {
        Some(fields) => keys.get_or_insert_with(Vec::new).extend(fields.into_iter().map(|(key, _)| key)),
        None => {
          keys = None;
          break;
        }
      }
    }
    if let Some(keys) = &mut keys {
      keys.sort();
      keys.dedup();
    }
    source.store_keys = keys;
  }
  let lowered: std::collections::HashSet<&str> = components.iter().map(|entry| entry.module.as_str()).collect();
  let mut reads = std::collections::BTreeMap::new();
  for module in islands.iter().filter(|module| !lowered.contains(module.as_str())) {
    let file = module.split('#').next().unwrap_or(module);
    let keys = set.store_reads(file);
    if !keys.is_empty() {
      reads.insert(module.clone(), keys);
    }
  }
  let mut social = social_images(app, &routes_dir, &resolver)?;
  social.extend(generated_heads);
  let mut manifest = Manifest::new(entries).with_heads(social).with_images(generated_images).with_sources(sources).with_actions(actions).with_components(components).with_clients(clients).with_not_found(not_found).with_handlers(handlers).with_middleware(middleware).with_intercepts(intercepts).with_consts(set.consts.clone()).with_frameworks(frameworks.clone()).with_theme(theme_body, theme_entries);
  manifest.reads = reads;
  let sync_natives: std::collections::HashSet<(&str, &str)> = natives.modules.iter().flat_map(|m| m.methods.iter().filter(|method| method.is_sync).map(move |method| (m.name.as_str(), method.name.as_str()))).collect();
  manifest.visit_exprs_mut(&mut |e| {
    if let snapfire_fsr_ir::Expr::NativeCall { module, method, sync, .. } = e {
      *sync = sync_natives.contains(&(module.as_str(), method.as_str()));
    }
  });
  let mut composition_files: Vec<(String, Vec<String>)> = Vec::new();
  for component in manifest.components.iter().filter(|c| c.body.owner == snapfire_fsr_ir::Owner::Fsr && c.module.starts_with("routes/")) {
    let Some((file, _)) = component.module.split_once('#') else { continue };
    if composition_files.iter().any(|(f, _)| f == file) {
      continue;
    }
    let reached: Vec<String> = set.local_reach(file).into_iter().filter(|f| f != file && !browser_files.contains(f)).collect();
    composition_files.push((file.to_owned(), reached));
  }
  let mut segments: Vec<String> = Vec::new();
  for node in manifest.routes.iter().map(|route| &route.plan).chain(manifest.not_found.iter()) {
    segment_modules(node, &mut segments);
  }
  let mut sheet_files: Vec<String> = Vec::new();
  for module in segments {
    let file = module.split_once('#').map_or(module.as_str(), |(file, _)| file).to_owned();
    let mut linked: Vec<String> = Vec::new();
    for reached in set.local_reach(&file) {
      for sheet in set.sheets(&reached) {
        if app.join(&sheet).is_file() && !sheet_files.contains(&sheet) {
          sheet_files.push(sheet.clone());
        }
        if !linked.contains(&sheet) {
          linked.push(sheet);
        }
      }
    }
    if !linked.is_empty() {
      manifest.sheets.push(snapfire_fsr_plan::ModuleSheets { module, sheets: linked });
    }
  }
  browser_files.extend(sheet_files);
  manifest.derives = derives;
  debug_assert!(manifest.sources.iter().all(|s| s.owner == RowOwner::Lowered));
  let declarations = typescript::declarations(&contract);
  let mut manifest = manifest;
  if let Some(site) = &options.site {
    manifest.namespace(&site.name, &site.at, &options.shell);
  }
  let (contract, contracts) = match &options.site {
    Some(site) => (
      contract.namespaced(&site.name),
      contracts.into_iter().map(|(rel, c)| (rel, c.namespaced(&site.name))).collect(),
    ),
    None => (contract, contracts),
  };

  resolver.scan(&app.join(&sections.dirs.images), &sections.dirs.images);
  let (fonts, font_lines) = assets::fonts(app, &public_path, &sections)?;
  let assets_manifest = assets::manifest(&resolver, fonts);
  for entry in &assets_manifest.entries {
    let detail = if entry.passthrough {
      format!("{}x{}, served as it is", entry.width, entry.height)
    } else {
      format!("{}x{}, {} variants", entry.width, entry.height, entry.variants.len())
    };
    report.images.push((entry.source.clone(), detail));
  }
  report.fonts = font_lines;
  for (path, why) in resolver.refused.borrow().iter() {
    report.images.push((path.clone(), format!("not read: {why}")));
  }
  let mut tailwind_file = None;
  let mut cards_file = None;
  if tailwind::wanted(&app.join(&sections.dirs.styles)) {
    #[allow(unused_mut)]
    let mut classes = tailwind::collect(&manifest);
    #[cfg(feature = "mdx")]
    classes.listed.retain(|class, _| !mdx_installed.log.borrow().classes.contains(class));
    report.tailwind = Some((classes.listed.len(), classes.unlisted.clone()));
    tailwind_file = Some(classes.file());
    if !manifest.images.is_empty() {
      let table = tailwind::card_table(&app.join(&sections.dirs.styles), &classes)?;
      cards_file = Some(serde_json::to_string_pretty(&table).expect("a card table serializes") + "\n");
    }
  }
  let plan_text = manifest.to_sexpr();
  report.plan = Some(plan_text.len());
  let mut files = vec![(PLAN_FILE.to_owned(), plan_text)];
  files.extend(tailwind_file.map(|text| (tailwind::CLASSES_FILE.to_owned(), text)));
  files.extend(cards_file.map(|text| (tailwind::CARDS_FILE.to_owned(), text)));
  files.extend(contracts.into_iter().map(|(rel, c)| (rel, c.to_json() + "\n")));
  files.extend(rewritten.into_iter().filter(|(file, _)| !snapfire_fsr_lower::component::is_transformed(file)).map(|(file, source)| (format!("{}/{file}", dev::BUNDLE_OVERLAY), source)));
  files.extend(mdx_files);
  files.extend(written_themes.files);
  let svelte_islands = islands.iter().filter(|m| !static_modules.contains(m) && !defines.contains(m)).any(|m| adapter_for(m, &owners).is_ok_and(|adapter| adapter.module == SVELTE.module));
  let svelte_components = svelte_components || svelte_islands;
  if svelte_components {
    generated_files.extend(svelte_components_files().into_iter().map(|(file, _)| file));
  }
  let mut bundle_files = generated_files.clone();
  bundle_files.extend(written_themes.added);
  #[cfg(feature = "mdx")]
  {
    let log = mdx_installed.log.borrow().clone();
    for file in &log.files {
      let copy = mdx::browser_copy(app, file, &mdx_installed)?;
      let at = format!("{}/{file}", dev::BUNDLE_OVERLAY);
      files.retain(|(f, _)| *f != at);
      files.push((at, copy));
    }
    report.mdx = log.warnings.clone();
  }
  match &options.site {
    None => {
      let mut shell_contract = shell_contract(app, &contract, session_type, &manifest.sources, &set.consts, &config, &frameworks)?;
      shell_contract.themes = report.themes.clone();
      shell_contract.tokens = root_tokens.names();
      files.push(("generated/shell.json".to_owned(), serde_json::to_string_pretty(&shell_contract).expect("a shell contract serializes") + "\n"));
    }
    Some(_) => {
      if let Some((path, shell_contract)) = &shell {
        let own: std::collections::BTreeMap<String, String> = std::fs::read_to_string(app.join("importmap.json"))
          .ok()
          .and_then(|text| serde_json::from_str::<serde_json::Value>(&text).ok())
          .and_then(|json| json.get("imports").cloned())
          .and_then(|imports| serde_json::from_value(imports).ok())
          .unwrap_or_default();
        let differ: Vec<String> = own.iter().filter(|(k, v)| shell_contract.imports.get(*k).is_some_and(|theirs| theirs != *v)).map(|(k, _)| k.clone()).collect();
        report.shell = Some((path.display().to_string(), shell_contract.store.len(), shell_contract.imports.len(), differ));
        files.push(("generated/shell.d.ts".to_owned(), shell_contract.declarations()));
      }
    }
  }
  let split: Vec<String> = report.extracted.iter().filter_map(|(_, island, _)| island.split_once('#').map(|(file, _)| file.to_owned())).collect();
  let shadowed_files: Vec<String> = shadowed.iter().map(|(file, _)| file.clone()).collect();
  let registry = islands_module(&islands, &static_modules, &defines, &owners, &split, &exported, &function_modules, &shadowed_files, options)?;
  check_island_imports(app, &layout, shell.as_ref().map(|(_, contract)| contract), &islands, &static_modules, &defines, &owners)?;
  files.extend([
    ("generated/uploads.d.ts".to_owned(), UPLOAD_DECLARATION.to_owned()),
    ("generated/assets.d.ts".to_owned(), ASSET_DECLARATIONS.to_owned()),
    (snapfire_fsr_host::assets::ASSETS_FILE.to_owned(), serde_json::to_string_pretty(&assets_manifest).expect("an assets manifest serializes") + "\n"),
    (assets::MAP_FILE.to_owned(), assets::map_text(&resolver, &assets_manifest.fonts)),
    ("generated/native.d.ts".to_owned(), native::declarations(&natives)),
    ("generated/services.d.ts".to_owned(), declarations),
    ("generated/elements.d.ts".to_owned(), types::element_declarations(app, &layout, &elements)?),
    ("generated/head.ts".to_owned(), HEAD_HELPERS.to_owned()),
    ("generated/config.ts".to_owned(), config_module(&config)),
    ("generated/config.schema.json".to_owned(), serde_json::to_string_pretty(&snapfire_fsr_host::config::config_schema()).expect("a schema serializes") + "\n"),
    ("generated/host.ts".to_owned(), ctx_module(&routes.iter().chain(handler_routes.iter()).cloned().collect::<Vec<_>>(), session_import.as_deref(), &config, &report.themes)),
    ("generated/islands.ts".to_owned(), registry),
    ("generated/client.ts".to_owned(), client),
    ("generated/testing.ts".to_owned(), testing_module()),
    ("tsconfig.json".to_owned(), types::tsconfig(app, true, shim.is_some(), !vue_declarations.is_empty(), &shadowed)?),
    ("tsconfig.build.json".to_owned(), types::tsconfig_build(app, &browser_files, &bundle_files, types::jsx_source(app, &layout)?)),
  ]);
  if let Some(shim) = shim {
    files.push((format!("{}/{}", layout.types.trim_end_matches('/'), types::FOREIGN_SHIM), shim));
  }
  if svelte_components {
    files.extend(svelte_components_files());
  }
  files.extend(vue_declarations);
  files.extend(shadowed_declarations);
  let mut report = report;
  let declarations = types::Declarations::read(app)?;
  report.types = declarations.status();
  let missing_declarations = declarations.missing();
  claimed(&report, &routes, &handler_routes, &layout_ids)?;
  public_claimed(app, routes.iter().chain(&handler_routes).map(|r| r.pattern.as_str()).chain(manifest.images.iter().map(|i| i.pattern.as_str())))?;
  Ok(Built { manifest, contract, report, files, defaults, browser_routes: browser_route_files, browser_files, missing_declarations, added: generated_files, composition_files, assets: assets_manifest, resolver })
}

/// The files the browser loads by name: every island the registry mounts and
/// every module an `<Island define>` names, the route modules it mounts and
/// the islands split out of pages, the document's entry (`[document] entry`
/// under `public_path`, else `src/main`) and each browser half under `ext/`.
/// A server island and a template nothing mounts are not among them.
/// The `derive` calls the browser's files make, as plan rows, each listed in
/// the report. A key derived twice or derived keys that feed each other fail
/// the build.
pub(crate) fn browser_derives(set: &mut ComponentSet, browser_files: &[String], report: &mut Report) -> Result<Vec<snapfire_fsr_plan::DeriveEntry>, BuildError> {
  let mut rows: Vec<(String, snapfire_fsr_plan::DeriveEntry)> = Vec::new();
  for file in browser_files.iter().filter(|file| [".ts", ".tsx", ".js", ".jsx"].iter().any(|ext| file.ends_with(ext))) {
    for lowered in set.lower_derives(file)? {
      if let Some((first, _)) = rows.iter().find(|(_, row)| row.key == lowered.key) {
        return Err(BuildError::DerivedTwice { key: lowered.key, first: first.clone(), second: file.clone() });
      }
      rows.push((file.clone(), snapfire_fsr_plan::DeriveEntry { key: lowered.key, sources: lowered.sources, body: lowered.body, at: lowered.at, why: lowered.why }));
    }
  }
  if let Some(keys) = derive_cycle(rows.iter().map(|(_, row)| row)) {
    return Err(BuildError::DeriveCycle { keys });
  }
  rows.sort_by(|a, b| a.1.key.cmp(&b.1.key));
  for (file, row) in &rows {
    report.derives.push(match (&row.at, &row.why) {
      (Some(at), Some(why)) => (row.key.clone(), "browser".to_owned(), format!("{at}: {why}")),
      _ => (row.key.clone(), "lowered".to_owned(), file.clone()),
    });
  }
  Ok(rows.into_iter().map(|(_, row)| row).collect())
}

/// A chain of derived keys that comes back to its first, when one does.
pub(crate) fn derive_cycle<'a>(rows: impl Iterator<Item = &'a snapfire_fsr_plan::DeriveEntry> + Clone) -> Option<Vec<String>> {
  fn walk<'a>(key: &str, rows: &[&'a snapfire_fsr_plan::DeriveEntry], path: &mut Vec<String>, done: &mut std::collections::HashSet<String>) -> Option<Vec<String>> {
    if let Some(at) = path.iter().position(|k| k == key) {
      let mut cycle = path[at..].to_vec();
      cycle.push(key.to_owned());
      return Some(cycle);
    }
    if done.contains(key) {
      return None;
    }
    let row = rows.iter().find(|row| row.key == key)?;
    path.push(key.to_owned());
    for source in &row.sources {
      if let Some(cycle) = walk(source, rows, path, done) {
        return Some(cycle);
      }
    }
    path.pop();
    done.insert(key.to_owned());
    None
  }
  let rows: Vec<&snapfire_fsr_plan::DeriveEntry> = rows.collect();
  let mut done = std::collections::HashSet::new();
  rows.iter().find_map(|row| walk(&row.key, &rows, &mut Vec::new(), &mut done))
}

/// The modules a plan node renders and the nodes below it: each segment's own, its fallback, its error boundaries.
pub(crate) fn segment_modules(node: &snapfire_fsr_plan::Node, out: &mut Vec<String>) {
  let own = std::iter::once(&node.module).chain(node.fallback.iter()).chain(node.error.iter()).chain(node.error_kinds.iter().map(|(_, module)| module));
  for module in own {
    if !out.contains(module) {
      out.push(module.clone());
    }
  }
  for child in &node.children {
    segment_modules(&child.node, out);
  }
}

pub(crate) fn browser_roots(app: &Path, islands: &[String], static_modules: &[String], route_files: &[String], generated: &[String], public_path: &str) -> Vec<String> {
  let mut roots: Vec<String> = Vec::new();
  let mut add = |file: String| {
    if !roots.contains(&file) {
      roots.push(file);
    }
  };
  for module in islands.iter().filter(|m| !static_modules.contains(m)) {
    if let Some((file, _)) = module.split_once('#') {
      add(file.to_owned());
    }
  }
  for file in route_files.iter().chain(generated) {
    add(file.clone());
  }
  let configured = config_beside(app).and_then(|config| config.document.entry).and_then(|url| url.strip_prefix(public_path.trim_end_matches('/')).map(|rest| rest.trim_start_matches('/').to_owned()));
  let entry = configured.as_deref().unwrap_or("src/main.js");
  let stem = entry.strip_suffix(".js").unwrap_or(entry);
  if let Some(file) = [".ts", ".tsx", ".js", ".jsx"].iter().map(|ext| format!("{stem}{ext}")).find(|file| app.join(file).is_file()) {
    add(file);
  }
  let mut pending = vec![app.join("ext")];
  let mut halves = Vec::new();
  while let Some(dir) = pending.pop() {
    for path in std::fs::read_dir(&dir).into_iter().flatten().flatten().map(|entry| entry.path()) {
      let name = path.to_string_lossy();
      if path.is_dir() {
        pending.push(path);
      } else if [".ts", ".tsx", ".js", ".jsx"].iter().any(|ext| name.ends_with(ext)) && !name.ends_with(".d.ts") {
        halves.push(path.strip_prefix(app).unwrap_or(&path).to_string_lossy().replace('\\', "/"));
      }
    }
  }
  halves.sort();
  for half in halves {
    add(half);
  }
  roots
}

/// In an application with no JSX framework, refuses a file the browser would
/// load that writes JSX or imports the template dialect, which only the
/// server resolves there, naming the root that reaches it.
pub(crate) fn refuse_unrendered(set: &mut ComponentSet, rewritten: &[(String, String)], roots: &[String]) -> Result<(), BuildError> {
  let dialect = format!("\"{TEMPLATE_SPECIFIER}\"");
  for root in roots {
    for file in set.local_reach(root) {
      let reason = if set.writes_jsx(&file) {
        "writes JSX"
      } else if set.imports_value_from(&file, TEMPLATE_SPECIFIER) || rewritten.iter().any(|(f, text)| *f == file && text.contains(&dialect)) {
        "imports the template dialect"
      } else {
        continue;
      };
      return Err(BuildError::Unrendered { file, root: root.clone(), reason });
    }
  }
  Ok(())
}
