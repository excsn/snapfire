use super::*;

impl ComponentSet {
  /// A module-level name as an expression: a `const` inlined, a function as a
  /// lambda, an import resolved in its own file. `None` when the file has no
  /// such name.
  pub(super) fn global(&mut self, file: &str, name: &str) -> Result<Option<(Expr, String)>, LowerError> {
    let key = format!("{file}#{name}");
    if self.resolving.iter().any(|m| *m == key) {
      // The function's own name inside its body: the binding `Expr::Fix` gives it while it runs.
      if !self.recursive.iter().any(|k| *k == key) {
        self.recursive.push(key.clone());
      }
      return Ok(Some((Expr::Var(name.to_owned()), key)));
    }
    let meanings = self.meanings(file);
    let parsed_cached = self.parsed(file);
    let parsed = parsed_cached.get();
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
  pub(super) fn module_init(&mut self, file: &str, upto: &str) -> Result<Expr, LowerError> {
    let parsed_cached = self.parsed(file);
    let parsed = parsed_cached.get();
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
  pub(super) fn global_kind(&mut self, file: &str, name: &str) -> Result<Option<crate::Kind>, LowerError> {
    if let Some(crate::meaning::Meaning::Own { file: declared, name: declared_name }) = self.meanings(file).bindings.get(name).cloned() {
      if declared != file {
        self.load(&declared)?;
        return self.global_kind(&declared, &declared_name);
      }
    }
    let parsed_cached = self.parsed(file);
    let parsed = parsed_cached.get();
    Ok(match find_value(&parsed, name) {
      Some(Global::Const(init)) => crate::made_kind(init),
      _ => None,
    })
  }

  /// The kind a module-level function `name` is declared to return, a type alias read through to what it stands for.
  pub(super) fn global_return_kind(&mut self, file: &str, name: &str) -> Result<Option<crate::Kind>, LowerError> {
    if let Some(crate::meaning::Meaning::Own { file: declared, name: declared_name }) = self.meanings(file).bindings.get(name).cloned() {
      if declared != file {
        self.load(&declared)?;
        return self.global_return_kind(&declared, &declared_name);
      }
    }
    let parsed_cached = self.parsed(file);
    let parsed = parsed_cached.get();
    let Some(returned) = declared_return(&parsed.module(), name) else { return Ok(None) };
    let aliases = self.alias_kinds(file);
    Ok(crate::Kind::of_type_in(returned, &aliases))
  }

  /// Every type alias `file` declares or imports that stands for a kind, by its local name.
  pub(crate) fn alias_kinds(&mut self, file: &str) -> Vec<(String, crate::AliasKind)> {
    let Some(parsed_cached) = self.parsed_if(file) else { return Vec::new() };
    let parsed = parsed_cached.get();
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
  pub(super) fn resolve_alias(&mut self, file: &str, name: &str) -> Option<crate::AliasKind> {
    let key = format!("{file}#{name}");
    if let Some(known) = self.aliases.get(&key) {
      return *known;
    }
    self.aliases.insert(key.clone(), None);
    let found = self.resolve_alias_uncached(file, name);
    self.aliases.insert(key, found);
    found
  }

  pub(super) fn resolve_alias_uncached(&mut self, file: &str, name: &str) -> Option<crate::AliasKind> {
    let parsed_cached = self.parsed_if(file)?;
    let parsed = parsed_cached.get();
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
  pub(super) fn alias_body_kind(&mut self, file: &str, ty: &js::TSType, params: &[String]) -> Option<crate::AliasKind> {
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
  pub(super) fn global_formatter(&mut self, file: &str, name: &str) -> Result<Option<String>, LowerError> {
    if let Some(crate::meaning::Meaning::Own { file: declared, name: declared_name }) = self.meanings(file).bindings.get(name).cloned() {
      if declared != file {
        self.load(&declared)?;
        return self.global_formatter(&declared, &declared_name);
      }
    }
    let parsed_cached = self.parsed(file);
    let parsed = parsed_cached.get();
    Ok(match find_value(&parsed, name) {
      Some(Global::Const(init)) => match crate::strip_parens(init) {
        js::Expression::NewExpression(n) if crate::platform_object(n).is_some() => Some(parsed.text()[parsed.range(n.span)].to_owned()),
        _ => None,
      },
      _ => None,
    })
  }

  /// A constant the plan names once or the expression itself when naming it would cost more than copying it. `CONST_WEIGHT` is where a copy per reference starts to dominate the plan. A program that writes objects names every constant that builds one, so all its readers in a request hold one object.
  pub(super) fn name_if_large(&mut self, key: String, expr: Expr) -> Expr {
    let shared = data_only(&expr) && asset_facts(&expr).is_none() && !is_asset_record(&expr);
    if weight(&expr) < CONST_WEIGHT && !shared || matches!(expr, Expr::Lambda { .. } | Expr::Fix { .. } | Expr::Ext { .. }) || is_asset_record(&expr) {
      return expr;
    }
    let key = if crate::heap_pass() && !shared { format!("{key}$heap") } else { key };
    self.consts.entry(key.clone()).or_insert(expr);
    Expr::Const(key)
  }



  pub(super) fn lower_global(&mut self, file: &str, item: Global<'_>) -> Result<Expr, LowerError> {
    self.load_class_graph(file);
    let mut globals: Vec<(String, Expr)> = Vec::new();
    let mut global_kinds = Vec::new();
    let mut global_formatters = Vec::new();
    loop {
      let parsed_cached = self.parsed(file);
      let parsed = parsed_cached.get();
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
