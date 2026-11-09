use super::*;

impl ComponentSet {
  /// The file an import names relative to the app, through an alias or a
  /// relative path; `None` for a bare or generated specifier the build does
  /// not follow.
  /// Adds each class `file` declares to the table, its parent resolved through the import that binds it.
  pub(super) fn register_classes(&mut self, file: &str) {
    let parsed_cached = self.parsed(file);
    let parsed = parsed_cached.get();
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
  pub(super) fn class_id(&mut self, file: &str, name: &str) -> Option<String> {
    let parsed_cached = self.parsed_if(file)?;
    let parsed = parsed_cached.get();
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
  pub(super) fn reexported_class(&mut self, file: &str, name: &str) -> Option<String> {
    let parsed_cached = self.parsed_if(file)?;
    let parsed = parsed_cached.get();
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
  pub(super) fn load_class_graph(&mut self, file: &str) {
    let mut pending = vec![file.to_owned()];
    while let Some(at) = pending.pop() {
      if !self.class_graphs.insert(at.clone()) || self.load(&at).is_err() {
        continue;
      }
      let parsed_cached = self.parsed(&at);
      let parsed = parsed_cached.get();
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

  /// Lowers every member of the class `id`, and of each class it extends, into `consts`. A member that does not lower is a constant that fails with its residue when it runs.
  pub(super) fn lower_class(&mut self, id: &str) -> Result<(), LowerError> {
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
    let parsed_cached = self.parsed(file);
    let parsed = parsed_cached.get();
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
}
