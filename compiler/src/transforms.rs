use std::cell::RefCell;
use std::path::{Component, Path, PathBuf};
use std::rc::Rc;

use crate::assets::{self, Emitted};
use snapfire_compiler_wire::driven::{AssetMap, MappedAsset};
use crate::config::Aliases;

use swc_core::common::DUMMY_SP;
use swc_core::ecma::ast::{
  BindingIdent, CallExpr, Callee, Decl, Expr, ExprOrSpread, ExprStmt, ExportSpecifier, Ident, IdentName,
  ImportSpecifier, KeyValueProp, Lit, MemberExpr, MemberProp, MetaPropExpr, MetaPropKind, ModuleDecl,
  ModuleExportName, ModuleItem, NewExpr, Number, ObjectLit, Pat, Prop, PropName, PropOrSpread, Stmt, Str, VarDecl,
  VarDeclKind, VarDeclarator,
};
use swc_core::ecma::visit::{Fold, FoldWith};

/// Renamed to `.js`, never suffixed: these are what the compiler emits `.js` for.
const COMPILED_TO_JS: [&str; 3] = ["ts", "tsx", "jsx"];

const BROWSER_READY: [&str; 3] = ["js", "mjs", "cjs"];

/// One edge out of a module, named as the specifier actually emitted.
///
/// A dynamic import is deliberately deferred by the author, so it is an edge for dependency
/// purposes but never something to preload.
#[derive(Clone)]
pub struct Import {
  pub specifier: String,
  pub dynamic: bool,
  /// Names taken from the target, as the target spells them. A namespace or
  /// dynamic import takes none, since neither names anything at the import.
  pub names: Vec<String>,
}

/// How an emitted asset is named from a module: by the public path when the
/// build has one, else relative to the module through `import.meta.url`. With
/// a map, by the row the driver wrote for it, with nothing emitted.
#[derive(Clone, Default)]
pub struct AssetUrls {
  pub root_dir: PathBuf,
  pub out_dir: PathBuf,
  pub public_path: Option<String>,
  pub map: Option<std::sync::Arc<AssetMap>>,
}

/// What a reference to an asset resolves to under a map.
pub enum Mapped<'a> {
  /// The driver's row.
  Row(&'a MappedAsset),
  /// A map was given and has no row for the file: the driver has to define it.
  Missing,
  /// No map: the compiler hashes and emits the file itself.
  Unmapped,
}

impl AssetUrls {
  /// The key a file is looked up under in the map: its path under the root
  /// with forward slashes.
  pub fn key(&self, source: &Path) -> Option<String> {
    source.strip_prefix(&self.root_dir).ok().map(crate::graph::slashed)
  }

  pub fn mapped(&self, source: &Path) -> Mapped<'_> {
    let Some(map) = &self.map else { return Mapped::Unmapped };
    match self.key(source).and_then(|key| map.assets.get(&key)) {
      Some(row) => Mapped::Row(row),
      None => Mapped::Missing,
    }
  }

  /// The URL of `asset` as a page serving this build reaches it, when the
  /// public path makes that knowable.
  pub fn public(&self, asset: &Emitted) -> Option<String> {
    let prefix = self.public_path.as_deref()?.trim_end_matches('/');
    let relative = asset.source.strip_prefix(&self.root_dir).ok()?;
    let dir = relative.parent().filter(|p| !p.as_os_str().is_empty());
    Some(match dir {
      Some(dir) => format!("{prefix}/{}/{}", crate::graph::slashed(dir), asset.name),
      None => format!("{prefix}/{}", asset.name),
    })
  }
}

/// What an import became under `asset_binding`.
enum Bound {
  Asset(ModuleItem),
  Kept,
  Other,
}

pub struct ImportRewriter {
  dir: PathBuf,
  aliases: Aliases,
  referenced: Rc<RefCell<Vec<PathBuf>>>,
  externals: Rc<RefCell<Vec<String>>>,
  imports: Rc<RefCell<Vec<Import>>>,
  assets: Rc<RefCell<Vec<Emitted>>>,
  /// Assets the map was asked about and does not name.
  misses: Rc<RefCell<Vec<PathBuf>>>,
  /// The imports of those, held out of the fold so they are neither rewritten nor recorded as references.
  kept: Rc<RefCell<Vec<ModuleItem>>>,
  urls: AssetUrls,
  /// Points every specifier at the `.min` graph, so a minified module never pulls in an
  /// unminified dependency.
  minified: bool,
}

impl ImportRewriter {
  #[allow(clippy::too_many_arguments)]
  pub fn new(
    source: &Path,
    aliases: Aliases,
    referenced: Rc<RefCell<Vec<PathBuf>>>,
    externals: Rc<RefCell<Vec<String>>>,
    imports: Rc<RefCell<Vec<Import>>>,
    assets: Rc<RefCell<Vec<Emitted>>>,
    misses: Rc<RefCell<Vec<PathBuf>>>,
    urls: AssetUrls,
    minified: bool,
  ) -> Self {
    Self {
      dir: source.parent().unwrap_or_else(|| Path::new(".")).to_path_buf(),
      aliases,
      referenced,
      externals,
      imports,
      assets,
      misses,
      kept: Rc::new(RefCell::new(Vec::new())),
      urls,
      minified,
    }
  }

  /// The binding an image or font import becomes: a `const` holding the URL
  /// the file is served at, plus the dimensions for an image. Under a map the
  /// row says both; otherwise the file is emitted here and has to sit under
  /// the root or there is nowhere to emit it. `Bound::Kept` is an import of
  /// an asset the map does not define, left as written and not a reference
  /// to copy; `Bound::Other` is not an asset import at all.
  fn asset_binding(&self, local: &Ident, specifier: &str) -> Bound {
    let (path, _) = assets::split_reference(specifier);
    let source = crate::graph::normalise(&self.dir.join(path));
    let Some(kind) = assets::kind(&source) else { return Bound::Other };
    if source.strip_prefix(&self.urls.root_dir).is_err() {
      return Bound::Other;
    }

    let (url, width, height): (Expr, Option<u32>, Option<u32>) = match self.urls.mapped(&source) {
      Mapped::Row(row) => (string(row.url.clone()), row.width, row.height),
      Mapped::Missing => {
        self.misses.borrow_mut().push(source);
        return Bound::Kept;
      }
      Mapped::Unmapped => {
        if !source.is_file() {
          return Bound::Other;
        }
        let Ok(emitted) = assets::emit(&source) else { return Bound::Other };
        let url = match self.urls.public(&emitted) {
          Some(url) => string(url),
          None => {
            let relative = relative_specifier(&self.dir, &source.with_file_name(&emitted.name));
            module_relative_url(relative)
          }
        };
        let (width, height) = (emitted.width, emitted.height);
        self.assets.borrow_mut().push(emitted);
        (url, width, height)
      }
    };

    let init = match kind {
      assets::Kind::Image => {
        let mut props = vec![property("src", url)];
        if let (Some(width), Some(height)) = (width, height) {
          props.push(property("width", number(width)));
          props.push(property("height", number(height)));
        }
        Expr::Object(ObjectLit { span: DUMMY_SP, props })
      }
      assets::Kind::Font => url,
    };

    Bound::Asset(ModuleItem::Stmt(Stmt::Decl(Decl::Var(Box::new(VarDecl {
      span: DUMMY_SP,
      ctxt: Default::default(),
      kind: VarDeclKind::Const,
      declare: false,
      decls: vec![VarDeclarator {
        span: DUMMY_SP,
        name: Pat::Ident(BindingIdent { id: local.clone(), type_ann: None }),
        init: Some(Box::new(init)),
        definite: false,
      }],
    })))))
  }

  fn rewrite(&self, src: &mut Str, dynamic: bool, names: Vec<String>) {
    let written = src.value.to_string_lossy();

    let specifier: String = if written.starts_with('.') {
      written.into_owned()
    } else {
      match self.aliases.expand(&written) {
        Some(target) => relative_specifier(&self.dir, &target),
        None => {
          if is_bare(&written) {
            self.externals.borrow_mut().push(written.into_owned());
          }
          return;
        }
      }
    };

    self.referenced.borrow_mut().push(self.dir.join(&specifier));

    let mut resolved = self.resolve(&specifier);

    if self.minified {
      resolved = minified_name(&resolved);
    }

    self.imports.borrow_mut().push(Import {
      specifier: resolved.clone(),
      dynamic,
      names,
    });

    if resolved == src.value.to_string_lossy() {
      return;
    }

    src.value = resolved.into();
    src.raw = None;
  }

  fn resolve(&self, specifier: &str) -> String {
    resolve_specifier(&self.dir, specifier)
  }
}

/// What a relative specifier becomes in the emitted graph, resolved against the directory the
/// importing file sits in.
///
/// Declaration emit resolves specifiers with this too, so a `.d.ts` names what its `.js` names and
/// TypeScript reaches the sibling declaration through the same path the browser uses for the
/// module.
pub fn resolve_specifier(dir: &Path, specifier: &str) -> String {
  let extension = Path::new(specifier)
    .extension()
    .and_then(|e| e.to_str())
    .map(|e| e.to_ascii_lowercase());

  match extension.as_deref() {
    Some(ext) if COMPILED_TO_JS.contains(&ext) => format!("{}.js", &specifier[..specifier.len() - ext.len() - 1]),
    // A framework source becomes a module the same way a `.ts` does, so an
    // import of one names the `.js` beside it rather than the source.
    Some(ext) if crate::plugin::claimed(ext).is_some() => format!("{}.js", &specifier[..specifier.len() - ext.len() - 1]),
    Some(ext) if BROWSER_READY.contains(&ext) => specifier.to_string(),
    Some(_) => specifier.to_string(),
    None => {
      let trimmed = specifier.trim_end_matches('/');
      if dir.join(trimmed).is_dir() {
        format!("{trimmed}/index.js")
      } else {
        format!("{specifier}.js")
      }
    }
  }
}

/// `target` as a relative specifier from `dir`, with the `./` a browser needs to
/// tell a path from a package. Pure path arithmetic, so both should be absolute.
pub fn relative_specifier(dir: &Path, target: &Path) -> String {
  let (dir, target) = (crate::graph::normalise(dir), crate::graph::normalise(target));
  let from: Vec<Component> = dir.components().collect();
  let to: Vec<Component> = target.components().collect();
  let shared = from.iter().zip(to.iter()).take_while(|(a, b)| a == b).count();
  let mut parts: Vec<String> = Vec::new();
  for _ in shared..from.len() {
    parts.push("..".to_owned());
  }
  for component in &to[shared..] {
    parts.push(component.as_os_str().to_string_lossy().into_owned());
  }
  if parts.first().is_some_and(|p| p != "..") {
    parts.insert(0, ".".to_owned());
  }
  parts.join("/")
}

/// A name as the exporting module spells it, whether written as an identifier or
/// as a string literal.
pub fn export_name(name: &ModuleExportName) -> String {
  match name {
    ModuleExportName::Ident(ident) => ident.sym.to_string(),
    ModuleExportName::Str(literal) => literal.value.to_string_lossy().into_owned(),
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

fn string(value: String) -> Expr {
  Expr::Lit(Lit::Str(Str { span: DUMMY_SP, value: value.into(), raw: None }))
}

fn number(value: u32) -> Expr {
  Expr::Lit(Lit::Num(Number { span: DUMMY_SP, value: f64::from(value), raw: None }))
}

fn property(key: &str, value: Expr) -> PropOrSpread {
  PropOrSpread::Prop(Box::new(Prop::KeyValue(KeyValueProp {
    key: PropName::Ident(IdentName::new(key.into(), DUMMY_SP)),
    value: Box::new(value),
  })))
}

/// `new URL(relative, import.meta.url).href`, which the browser resolves
/// against the module rather than the page, so the build stays mountable
/// anywhere.
fn module_relative_url(relative: String) -> Expr {
  let meta_url = Expr::Member(MemberExpr {
    span: DUMMY_SP,
    obj: Box::new(Expr::MetaProp(MetaPropExpr { span: DUMMY_SP, kind: MetaPropKind::ImportMeta })),
    prop: MemberProp::Ident(IdentName::new("url".into(), DUMMY_SP)),
  });
  let constructed = Expr::New(NewExpr {
    span: DUMMY_SP,
    ctxt: Default::default(),
    callee: Box::new(Expr::Ident(Ident::new_no_ctxt("URL".into(), DUMMY_SP))),
    args: Some(vec![
      ExprOrSpread { spread: None, expr: Box::new(string(relative)) },
      ExprOrSpread { spread: None, expr: Box::new(meta_url) },
    ]),
    type_args: None,
  });
  Expr::Member(MemberExpr {
    span: DUMMY_SP,
    obj: Box::new(constructed),
    prop: MemberProp::Ident(IdentName::new("href".into(), DUMMY_SP)),
  })
}

impl Fold for ImportRewriter {
  /// An image or font imported as a default binding is replaced by a `const`
  /// before the import itself is rewritten, so it never becomes an edge in the
  /// module graph. Any other shape of import is left to resolve as written.
  fn fold_module_items(&mut self, items: Vec<ModuleItem>) -> Vec<ModuleItem> {
    let items: Vec<ModuleItem> = items
      .into_iter()
      .map(|item| {
        let ModuleItem::ModuleDecl(ModuleDecl::Import(import)) = &item else {
          return item;
        };
        let specifier = import.src.value.to_string_lossy();
        if !specifier.starts_with('.') || import.specifiers.len() != 1 {
          return item;
        }
        let ImportSpecifier::Default(default) = &import.specifiers[0] else {
          return item;
        };
        match self.asset_binding(&default.local, &specifier) {
          Bound::Asset(bound) => bound,
          Bound::Kept => {
            self.kept.borrow_mut().push(item);
            ModuleItem::Stmt(Stmt::Empty(swc_core::ecma::ast::EmptyStmt { span: DUMMY_SP }))
          }
          Bound::Other => item,
        }
      })
      .collect();

    items.fold_children_with(self)
  }

  fn fold_import_decl(&mut self, mut n: swc_core::ecma::ast::ImportDecl) -> swc_core::ecma::ast::ImportDecl {
    let names = n
      .specifiers
      .iter()
      .filter_map(|specifier| match specifier {
        ImportSpecifier::Named(named) if !named.is_type_only => Some(match &named.imported {
          Some(imported) => export_name(imported),
          None => named.local.sym.to_string(),
        }),
        ImportSpecifier::Default(_) => Some("default".to_string()),
        // A namespace binding names nothing, so there is nothing to check.
        ImportSpecifier::Namespace(_) | ImportSpecifier::Named(_) => None,
      })
      .collect();

    self.rewrite(&mut n.src, false, names);
    n
  }

  fn fold_named_export(&mut self, mut n: swc_core::ecma::ast::NamedExport) -> swc_core::ecma::ast::NamedExport {
    if let Some(src) = &mut n.src {
      let names = n
        .specifiers
        .iter()
        .filter_map(|specifier| match specifier {
          ExportSpecifier::Named(named) if !named.is_type_only => Some(export_name(&named.orig)),
          ExportSpecifier::Default(default) => Some(default.exported.sym.to_string()),
          ExportSpecifier::Namespace(_) | ExportSpecifier::Named(_) => None,
        })
        .collect();

      self.rewrite(src, false, names);
    }
    n
  }

  fn fold_export_all(&mut self, mut n: swc_core::ecma::ast::ExportAll) -> swc_core::ecma::ast::ExportAll {
    self.rewrite(&mut n.src, false, Vec::new());
    n
  }

  fn fold_call_expr(&mut self, n: CallExpr) -> CallExpr {
    let mut n = n.fold_children_with(self);

    if matches!(n.callee, Callee::Import(_))
      && let Some(arg) = n.args.first_mut()
      && arg.spread.is_none()
      && let Expr::Lit(Lit::Str(src)) = &mut *arg.expr
    {
      self.rewrite(src, true, Vec::new());
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

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn a_relative_specifier_walks_up_then_down_with_a_dot_when_it_stays_below() {
    assert_eq!(relative_specifier(Path::new("/app/routes/index"), Path::new("/app/src/ui/Header")), "../../src/ui/Header");
    assert_eq!(relative_specifier(Path::new("/app/src/ui"), Path::new("/app/src/ui/Stars")), "./Stars");
    assert_eq!(relative_specifier(Path::new("/app/src"), Path::new("/app/src/ui/Stars")), "./ui/Stars");
    assert_eq!(relative_specifier(Path::new("/app/src/ui"), Path::new("/app/generated/client")), "../../generated/client");
  }
}
