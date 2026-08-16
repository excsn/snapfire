use std::cell::RefCell;
use std::path::{Path, PathBuf};
use std::rc::Rc;

use swc_core::ecma::ast::{
  CallExpr, Callee, Expr, ExprStmt, Lit, MemberExpr, MemberProp, ModuleItem, Stmt, Str,
};
use swc_core::ecma::visit::{Fold, FoldWith};

/// Renamed to `.js`, never suffixed: these are what the compiler emits `.js` for.
const COMPILED_TO_JS: [&str; 3] = ["ts", "tsx", "jsx"];

const BROWSER_READY: [&str; 3] = ["js", "mjs", "cjs"];

/// One edge out of a module, named as the specifier actually emitted.
///
/// A dynamic import is deliberately deferred by the author, so it is an edge for dependency
/// purposes but never something to preload.
pub struct Import {
  pub specifier: String,
  pub dynamic: bool,
}

pub struct ImportRewriter {
  dir: PathBuf,
  referenced: Rc<RefCell<Vec<PathBuf>>>,
  externals: Rc<RefCell<Vec<String>>>,
  imports: Rc<RefCell<Vec<Import>>>,
  /// Points every specifier at the `.min` graph, so a minified module never pulls in an
  /// unminified dependency.
  minified: bool,
}

impl ImportRewriter {
  pub fn new(
    source: &Path,
    referenced: Rc<RefCell<Vec<PathBuf>>>,
    externals: Rc<RefCell<Vec<String>>>,
    imports: Rc<RefCell<Vec<Import>>>,
    minified: bool,
  ) -> Self {
    Self {
      dir: source.parent().unwrap_or_else(|| Path::new(".")).to_path_buf(),
      referenced,
      externals,
      imports,
      minified,
    }
  }

  fn rewrite(&self, src: &mut Str, dynamic: bool) {
    let specifier = src.value.to_string_lossy();

    if !specifier.starts_with('.') {
      if is_bare(&specifier) {
        self.externals.borrow_mut().push(specifier.into_owned());
      }
      return;
    }

    self.referenced.borrow_mut().push(self.dir.join(specifier.as_ref()));

    let mut resolved = self.resolve(&specifier);

    if self.minified {
      resolved = minified_name(&resolved);
    }

    self.imports.borrow_mut().push(Import {
      specifier: resolved.clone(),
      dynamic,
    });

    if resolved == specifier {
      return;
    }

    src.value = resolved.into();
    src.raw = None;
  }

  fn resolve(&self, specifier: &str) -> String {
    let extension = Path::new(specifier)
      .extension()
      .and_then(|e| e.to_str())
      .map(|e| e.to_ascii_lowercase());

    match extension.as_deref() {
      Some(ext) if COMPILED_TO_JS.contains(&ext) => format!("{}.js", &specifier[..specifier.len() - ext.len() - 1]),
      Some(ext) if BROWSER_READY.contains(&ext) => specifier.to_string(),
      Some(_) => specifier.to_string(),
      None => {
        let trimmed = specifier.trim_end_matches('/');
        if self.dir.join(trimmed).is_dir() {
          format!("{trimmed}/index.js")
        } else {
          format!("{specifier}.js")
        }
      }
    }
  }
}

/// A specifier only a package resolver can satisfy, which in a browser means an import map. A URL
/// or a root-relative path resolves natively and is nobody's problem.
fn is_bare(specifier: &str) -> bool {
  !specifier.starts_with('/') && !specifier.contains(':')
}

/// Inserts `.min` before the extension of a specifier that names something the compiler emits a
/// minified variant of. Assets have no `.min` counterpart, so they are delivered once and both
/// graphs point at the same copy.
fn minified_name(specifier: &str) -> String {
  for ext in ["js", "mjs", "css"] {
    if let Some(stem) = specifier.strip_suffix(&format!(".{ext}")) {
      return format!("{stem}.min.{ext}");
    }
  }

  specifier.to_string()
}

impl Fold for ImportRewriter {
  fn fold_import_decl(&mut self, mut n: swc_core::ecma::ast::ImportDecl) -> swc_core::ecma::ast::ImportDecl {
    self.rewrite(&mut n.src, false);
    n
  }

  fn fold_named_export(&mut self, mut n: swc_core::ecma::ast::NamedExport) -> swc_core::ecma::ast::NamedExport {
    if let Some(src) = &mut n.src {
      self.rewrite(src, false);
    }
    n
  }

  fn fold_export_all(&mut self, mut n: swc_core::ecma::ast::ExportAll) -> swc_core::ecma::ast::ExportAll {
    self.rewrite(&mut n.src, false);
    n
  }

  fn fold_call_expr(&mut self, n: CallExpr) -> CallExpr {
    let mut n = n.fold_children_with(self);

    if matches!(n.callee, Callee::Import(_))
      && let Some(arg) = n.args.first_mut()
      && arg.spread.is_none()
      && let Expr::Lit(Lit::Str(src)) = &mut *arg.expr
    {
      self.rewrite(src, true);
    }

    n
  }
}

pub struct StripConsole {
  pub strip_log: bool,
  pub strip_debug: bool,
}

impl StripConsole {
  fn is_stripped(&self, stmt: &Stmt) -> bool {
    let Stmt::Expr(ExprStmt { expr, .. }) = stmt else {
      return false;
    };
    let Expr::Call(call) = &**expr else {
      return false;
    };
    let Callee::Expr(callee) = &call.callee else {
      return false;
    };
    let Expr::Member(MemberExpr { obj, prop, .. }) = &**callee else {
      return false;
    };
    let Expr::Ident(obj) = &**obj else {
      return false;
    };

    if obj.sym != "console" {
      return false;
    }

    let MemberProp::Ident(prop) = prop else {
      return false;
    };

    (self.strip_log && prop.sym == "log") || (self.strip_debug && prop.sym == "debug")
  }
}

impl Fold for StripConsole {
  fn fold_stmts(&mut self, stmts: Vec<Stmt>) -> Vec<Stmt> {
    let stmts = stmts.fold_children_with(self);
    stmts.into_iter().filter(|stmt| !self.is_stripped(stmt)).collect()
  }

  fn fold_module_items(&mut self, items: Vec<ModuleItem>) -> Vec<ModuleItem> {
    let items = items.fold_children_with(self);

    items
      .into_iter()
      .filter(|item| match item {
        ModuleItem::Stmt(stmt) => !self.is_stripped(stmt),
        _ => true,
      })
      .collect()
  }
}
