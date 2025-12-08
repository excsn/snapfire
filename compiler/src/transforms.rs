use swc_core::ecma::visit::{Fold, FoldWith};
use swc_core::{
  common::Spanned,
  ecma::ast::{Callee, EmptyStmt, Expr, ExprStmt, MemberExpr, MemberProp, Stmt, Str},
};

pub struct ImportRewriter;

impl Fold for ImportRewriter {
  fn fold_import_decl(&mut self, mut n: swc_core::ecma::ast::ImportDecl) -> swc_core::ecma::ast::ImportDecl {
    rewrite(&mut n.src);
    n
  }
  fn fold_named_export(&mut self, mut n: swc_core::ecma::ast::NamedExport) -> swc_core::ecma::ast::NamedExport {
    if let Some(src) = &mut n.src {
      rewrite(src);
    }
    n
  }
  fn fold_export_all(&mut self, mut n: swc_core::ecma::ast::ExportAll) -> swc_core::ecma::ast::ExportAll {
    rewrite(&mut n.src);
    n
  }
}

fn rewrite(src: &mut Str) {
  if src.value.starts_with(".") {
    let path_str = src.value.to_string_lossy();
    if !path_str.ends_with(".js") {
      let new_val = format!("{}.js", path_str);
      src.value = new_val.into();
      src.raw = None;
    }
  }
}

// --- CORRECTED StripConsole Transformer ---

pub struct StripConsole {
  pub strip_log: bool,
  pub strip_debug: bool,
}

impl Fold for StripConsole {
  // We are interested in transforming statements.
  fn fold_stmts(&mut self, stmts: Vec<Stmt>) -> Vec<Stmt> {
    let stmts = stmts.fold_children_with(self);

    // Filter out the statements we want to remove.
    stmts
      .into_iter()
      .filter(|stmt| {
        if let Stmt::Expr(ExprStmt { expr, .. }) = stmt {
          if let Expr::Call(call) = &**expr {
            if let Callee::Expr(callee_expr) = &call.callee {
              if let Expr::Member(MemberExpr { obj, prop, .. }) = &**callee_expr {
                if let Expr::Ident(ident) = &**obj {
                  if ident.sym != "console" {
                    return true; // Not a console call, keep it.
                  }
                }

                if let MemberProp::Ident(prop_ident) = prop {
                  let should_strip_log = self.strip_log && prop_ident.sym == "log";
                  let should_strip_debug = self.strip_debug && prop_ident.sym == "debug";

                  if should_strip_log || should_strip_debug {
                    return false; // This is a console call to strip, so filter it out.
                  }
                }
              }
            }
          }
        }
        true // Keep all other statements.
      })
      .collect()
  }
}
