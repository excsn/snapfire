use snapfire_fsr_lower::{read_schema, LowerError};
use snapfire_fsr_service::{Field, Type, TypeDef, Variant};

#[test]
fn a_session_schema_and_an_input_type_become_contract_records() {
  let types = read_schema("schemas/session.ts", r#"
    export interface Session {
      cart?: Record<string, bigint>;
      theme: Theme | null;
    }
    export interface AddToCart { product_id: bigint; quantity: bigint; note?: string }
    export type Theme = "dark" | "light";
    interface Private { x: number }
  "#).unwrap();

  assert_eq!(types.len(), 3, "only exports are read");
  assert_eq!(types[0].name, "Session");
  assert_eq!(
    types[0].def,
    TypeDef::Record { fields: vec![
      Field::new("cart", Type::optional(Type::map(Type::I64))),
      Field::new("theme", Type::optional(Type::named("Theme"))),
    ]}
  );
  assert_eq!(types[2].def, TypeDef::Union { variants: vec![Variant::unit("dark"), Variant::unit("light")] });
}

#[test]
fn scalars_and_containers_map_onto_the_value_model() {
  let types = read_schema("s.ts", r#"
    export interface All {
      s: string; n: number; b: bigint; f: boolean; z: null;
      l: string[]; a: Array<bigint>; m: Record<string, number>; bytes: Uint8Array; big: BigInt64Array;
      r: Other; o: Other | null; lo: (Other | undefined)[];
    }
  "#).unwrap();
  let TypeDef::Record { fields } = &types[0].def else { panic!() };
  let of = |name: &str| fields.iter().find(|f| f.name == name).unwrap().ty.clone();
  assert_eq!(of("s"), Type::Str);
  assert_eq!(of("n"), Type::F64);
  assert_eq!(of("b"), Type::I64);
  assert_eq!(of("f"), Type::Bool);
  assert_eq!(of("z"), Type::Null);
  assert_eq!(of("l"), Type::list(Type::Str));
  assert_eq!(of("a"), Type::list(Type::I64));
  assert_eq!(of("m"), Type::map(Type::F64));
  assert_eq!(of("bytes"), Type::Bytes);
  assert_eq!(of("big"), Type::Array(snapfire_fsr_service::ScalarKind::I64));
  assert_eq!(of("r"), Type::named("Other"));
  assert_eq!(of("o"), Type::optional(Type::named("Other")));
  assert_eq!(of("lo"), Type::list(Type::optional(Type::named("Other"))));
}

#[test]
fn a_shape_the_contract_cannot_hold_is_named_with_its_line() {
  let err = read_schema("s.ts", "export interface X {\n  nested: { a: string };\n}").unwrap_err();
  let LowerError::Residue(r) = err else { panic!("{err}") };
  assert_eq!(r.line, 2);
  assert!(r.message.contains("inline object"), "{r}");

  let err = read_schema("s.ts", "export interface X { u: string | number }").unwrap_err();
  assert!(err.to_string().contains("union of two types"), "{err}");
}

#[test]
fn session_defaults_fold_into_every_read_of_the_key() {
  use snapfire_fsr_ir::{Expr, Stmt};
  use snapfire_fsr_lower::{lower_loader_with, read_session_defaults};

  let defaults = read_session_defaults("schemas/session.ts", r#"
    export interface Session { cart: Record<string, bigint>; visits: bigint }
    export const defaults: Session = { cart: {}, visits: 0n };
  "#).unwrap();
  assert_eq!(defaults, vec![("cart".to_owned(), Expr::Object(vec![])), ("visits".to_owned(), Expr::lit_int(0))]);

  let body = lower_loader_with("page.loader.ts", "export async function load({ session }) { return { n: session.visits, other: session.theme }; }", &defaults).unwrap();
  let Stmt::Return(Expr::Object(entries)) = &body[0] else { panic!("{body:?}") };
  assert_eq!(entries[0], snapfire_fsr_ir::ast::Entry::Field("n".into(), Expr::Coalesce(Box::new(Expr::Session("visits".into())), Box::new(Expr::lit_int(0)))));
  assert_eq!(entries[1], snapfire_fsr_ir::ast::Entry::Field("other".into(), Expr::Session("theme".into())), "a key with no default reads plain");

  let none = read_session_defaults("s.ts", "export interface Session { cart: Record<string, bigint> }").unwrap();
  assert!(none.is_empty());
}

#[test]
fn query_reads_lower_like_params() {
  use snapfire_fsr_ir::{Expr, Stmt};
  use snapfire_fsr_lower::lower_loader;
  let body = lower_loader("page.loader.ts", "export async function load({ query, services }) { return { p: await services.shop.list({ tag: query.tag }) }; }").unwrap();
  let Stmt::Return(Expr::Object(entries)) = &body[0] else { panic!() };
  let snapfire_fsr_ir::ast::Entry::Field(_, Expr::Call { args, .. }) = &entries[0] else { panic!() };
  assert_eq!(args[0], ("tag".to_owned(), Expr::Query("tag".into())));
}
