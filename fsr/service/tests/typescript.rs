use snapfire_fsr_service::typescript::{declarations, type_declarations, type_name, type_name_for, Flavour};
use snapfire_fsr_service::{Contract, Field, Method, Service, Type, Variant};

const SHOPPING: &str = include_str!("../../examples/shopping_react_ts/app/clients/shopping.openapi.json");

#[test]
fn the_shopping_document_prints_its_types_and_services() {
  let imported = snapfire_fsr_service::import(SHOPPING, "shopping").unwrap();
  let ts = declarations(&imported.contract);

  assert!(ts.contains("export interface Product {"), "{ts}");
  assert!(ts.contains("  id: bigint;"), "int64 is bigint on the server side: {ts}");
  assert!(ts.contains("  price_cents: bigint;"), "{ts}");
  assert!(ts.contains("  tags: string[];"), "{ts}");
  assert!(ts.contains("export interface Services {"), "{ts}");
  assert!(ts.contains("  shopping: {"), "{ts}");
  assert!(ts.contains("listProducts(args?: { q?: string | null; category?: string | null; tag?: string | null; }): Promise<Product[]>;"), "{ts}");
  assert!(ts.contains("  list_price_cents?: bigint | null;"), "an optional property: {ts}");
  assert!(ts.contains("  rating: number;"), "{ts}");
  assert!(ts.contains("getProduct(args: { id: bigint; }): Promise<Product>;"), "{ts}");
  assert!(ts.contains("placeOrder(args: { lines: OrderLine[]; }): Promise<Order>;") || ts.contains("placeOrder(args: { lines: "), "{ts}");
}

#[test]
fn every_type_has_one_spelling() {
  assert_eq!(type_name(&Type::I32), "bigint");
  assert_eq!(type_name(&Type::U128), "bigint");
  assert_eq!(type_name(&Type::F32), "number");
  assert_eq!(type_name(&Type::Bytes), "Uint8Array");
  assert_eq!(type_name(&Type::Array(snapfire_fsr_service::ScalarKind::I64)), "BigInt64Array");
  assert_eq!(type_name(&Type::optional(Type::Str)), "string | null");
  assert_eq!(type_name(&Type::list(Type::optional(Type::Str))), "(string | null)[]");
  assert_eq!(type_name(&Type::map(Type::named("Line"))), "Record<string, Line>");
}

#[test]
fn unions_print_as_tagged_arms_and_odd_names_are_quoted() {
  let contract = Contract::new()
    .union("Status", vec![Variant::unit("open"), Variant::with("closed", Type::named("Reason"))])
    .record("Reason", vec![Field::new("why", Type::Str), Field::new("at-time", Type::optional(Type::I64))])
    .service("billing", Service::new().method("void-invoice", Method::new(vec![], Type::Null)));
  let ts = declarations(&contract);

  assert!(ts.contains("export type Status =\n  | { tag: \"open\" }\n  | { tag: \"closed\"; payload: Reason };"), "{ts}");
  assert!(ts.contains("  \"at-time\"?: bigint | null;"), "{ts}");
  assert!(ts.contains("    \"void-invoice\"(): Promise<null>;"), "{ts}");
}

#[test]
fn the_client_flavour_widens_integers_and_omits_services() {
  let imported = snapfire_fsr_service::import(SHOPPING, "shopping").unwrap();
  let ts = type_declarations(&imported.contract, Flavour::Client);
  assert!(ts.contains("  id: bigint | number;"), "{ts}");
  assert!(!ts.contains("Services"), "{ts}");
  assert_eq!(type_name_for(&Type::list(Type::I64), Flavour::Client), "(bigint | number)[]");
  assert_eq!(type_name_for(&Type::list(Type::Str), Flavour::Client), "string[]");
}
