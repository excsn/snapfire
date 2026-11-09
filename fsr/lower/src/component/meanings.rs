use super::*;

impl ComponentSet {
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
    let parsed_cached = self.parsed(file);
    let parsed = parsed_cached.get();
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
  pub(super) fn imported_meaning(&mut self, file: &str, source: &str, export: &str) -> crate::meaning::Meaning {
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
  pub(super) fn namespace_meaning(&mut self, file: &str, source: &str) -> crate::meaning::Meaning {
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
}
