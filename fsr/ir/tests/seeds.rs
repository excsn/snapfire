//! What a lowered `store` export says about its keys before any data is: the
//! promise a deferred segment makes to the first wave.

use snapfire_fsr_ir::ast::{Entry, Expr, Lit, Stmt};
use snapfire_fsr_ir::IrStore;
use snapfire_fsr_runtime::Seeds;

fn keys(body: Vec<Stmt>) -> Option<Vec<String>> {
  IrStore::new("page", body).keys()
}

fn field(name: &str) -> Entry {
  Entry::Field(name.to_owned(), Expr::Lit(Lit::Int(1)))
}

#[test]
fn an_object_of_named_fields_names_its_keys() {
  assert_eq!(keys(vec![Stmt::Return(Expr::Object(vec![field("cart/count"), field("owner")]))]), Some(vec!["cart/count".to_owned(), "owner".to_owned()]));
}

#[test]
fn every_branch_contributes_the_keys_it_can_return() {
  let body = vec![
    Stmt::If { cond: Expr::Lit(Lit::Bool(true)), then: vec![Stmt::Return(Expr::Object(vec![field("a")]))], r#else: Vec::new() },
    Stmt::Let { name: "out".to_owned(), expr: Expr::Object(vec![field("b")]) },
    Stmt::Return(Expr::Var("out".to_owned())),
  ];
  assert_eq!(keys(body), Some(vec!["a".to_owned(), "b".to_owned()]));
}

#[test]
fn a_spread_or_a_computed_key_leaves_the_keys_to_the_data() {
  assert_eq!(keys(vec![Stmt::Return(Expr::Object(vec![field("a"), Entry::Spread(Expr::Input)]))]), None);
  assert_eq!(keys(vec![Stmt::Return(Expr::Object(vec![Entry::Computed(Expr::Input, Expr::Input)]))]), None);
  assert_eq!(keys(vec![Stmt::Return(Expr::Input)]), None, "a return that is not an object literal");
}

#[test]
fn the_keys_the_build_inferred_win_over_reading_the_returns() {
  let spread = vec![Stmt::Return(Expr::Object(vec![Entry::Spread(Expr::Input)]))];
  assert_eq!(IrStore::new("page", spread.clone()).keys(), None, "the returns alone do not say");
  assert_eq!(IrStore::new("page", spread).with_keys(Some(vec!["repro/slot".to_owned()])).keys(), Some(vec!["repro/slot".to_owned()]), "the plan's `store-keys` does");
}
