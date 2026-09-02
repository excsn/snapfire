#![cfg(feature = "grpc")]

use snapfire_fsr_core::{Value, ValueMap};
use snapfire_fsr_service::grpc::{decode_response, encode_request};
use snapfire_fsr_service::typescript::declarations;
use snapfire_fsr_service::{import_proto_source, ImportError, Type, TypeDef};

const INVENTORY: &str = r#"
syntax = "proto3";
package shop.inventory;
import "google/protobuf/timestamp.proto";
import "google/protobuf/empty.proto";

enum Status { UNKNOWN = 0; ACTIVE = 1; RETIRED = 2; }

message StockRequest { int64 product_id = 1; }

message StockLevel {
  int64 product_id = 1;
  int32 on_hand = 2;
  uint32 reserved = 3;
  string warehouse = 4;
  Status status = 5;
  repeated string bins = 6;
  map<string, int32> by_bin = 7;
  optional double weight_kg = 8;
  Location location = 9;
  google.protobuf.Timestamp counted_at = 10;
  message Location { string aisle = 1; }
}

service Inventory {
  rpc GetStock (StockRequest) returns (StockLevel);
  rpc Ping (google.protobuf.Empty) returns (google.protobuf.Empty);
}
"#;

#[test]
fn a_proto_lowers_to_a_contract_and_keeps_its_descriptors() {
  let imported = import_proto_source("inventory.proto", INVENTORY, "inventory").unwrap();
  let contract = &imported.contract;

  let get = contract.method("inventory", "getStock").expect("one service takes the client's name and methods go lowerCamel");
  assert_eq!(get.params.len(), 1, "the request message's fields spread into arguments");
  assert_eq!(get.params[0].name, "product_id");
  assert_eq!(get.params[0].ty, Type::I64);
  assert_eq!(get.returns, Type::named("StockLevel"));

  let ping = contract.method("inventory", "ping").unwrap();
  assert!(ping.params.is_empty(), "Empty in is no arguments");
  assert_eq!(ping.returns, Type::Null, "Empty out is null");

  let Some(TypeDef::Record { fields }) = contract.types.get("StockLevel") else { panic!("StockLevel") };
  let field = |name: &str| fields.iter().find(|f| f.name == name).map(|f| f.ty.clone()).unwrap_or_else(|| panic!("{name}"));
  assert_eq!(field("product_id"), Type::I64);
  assert_eq!(field("on_hand"), Type::I32);
  assert_eq!(field("reserved"), Type::U32);
  assert_eq!(field("status"), Type::Str, "an enum is its value name");
  assert_eq!(field("bins"), Type::list(Type::Str));
  assert_eq!(field("by_bin"), Type::map(Type::I32));
  assert_eq!(field("weight_kg"), Type::optional(Type::F64), "proto3 optional has presence");
  assert_eq!(field("location"), Type::optional(Type::named("StockLevelLocation")), "a message field is nullable and a nested name joins");
  assert_eq!(field("counted_at"), Type::optional(Type::Str), "Timestamp is a nullable RFC 3339 string");
  assert!(contract.types.contains_key("StockLevelLocation"));

  assert_eq!(imported.methods[0].0, "inventory.getStock");
  assert_eq!(imported.methods[0].1.path, "/shop.inventory.Inventory/GetStock");
  assert_eq!(imported.methods[0].1.input, "shop.inventory.StockRequest");

  let ts = declarations(contract);
  assert!(ts.contains("getStock(args: { product_id: bigint; }): Promise<StockLevel>;"), "{ts}");
}

#[test]
fn a_streaming_method_and_a_non_string_map_key_are_refused() {
  let streaming = "syntax = \"proto3\"; message A {} service S { rpc Watch (A) returns (stream A); }";
  let err = import_proto_source("s.proto", streaming, "s").unwrap_err();
  assert!(matches!(err, ImportError::Unsupported { .. }), "{err}");
  assert!(err.to_string().contains("streaming"), "{err}");

  let keyed = "syntax = \"proto3\"; message A { map<int32, string> m = 1; } service S { rpc Get (A) returns (A); }";
  let err = import_proto_source("k.proto", keyed, "k").unwrap_err();
  assert!(err.to_string().contains("keyed by int32"), "{err}");
}

#[test]
fn values_round_trip_through_the_messages() {
  let imported = import_proto_source("inventory.proto", INVENTORY, "inventory").unwrap();
  let request = imported.pool.get_message_by_name("shop.inventory.StockRequest").unwrap();
  let mut args = ValueMap::new();
  args.insert("product_id".to_owned(), Value::Int(9_007_199_254_740_993));
  let bytes = encode_request(&request, &args).unwrap();
  assert!(!bytes.is_empty());

  let mut extra = ValueMap::new();
  extra.insert("nope".to_owned(), Value::int(1));
  assert!(encode_request(&request, &extra).is_err(), "an argument the message lacks is refused");

  let level = imported.pool.get_message_by_name("shop.inventory.StockLevel").unwrap();
  let mut fields = ValueMap::new();
  fields.insert("product_id".to_owned(), Value::Int(9_007_199_254_740_993));
  fields.insert("on_hand".to_owned(), Value::int(12));
  fields.insert("warehouse".to_owned(), Value::str("north"));
  fields.insert("status".to_owned(), Value::str("ACTIVE"));
  fields.insert("bins".to_owned(), Value::Seq(vec![Value::str("a1")]));
  fields.insert("counted_at".to_owned(), Value::str("2026-09-02T10:00:00Z"));
  let mut by_bin = ValueMap::new();
  by_bin.insert("a1".to_owned(), Value::int(3));
  fields.insert("by_bin".to_owned(), Value::Map(by_bin));
  let bytes = encode_request(&level, &fields).unwrap();
  let back = decode_response(&level, &bytes).unwrap();
  let Value::Map(map) = back else { panic!("a record") };
  assert_eq!(map.get("product_id"), Some(&Value::Int(9_007_199_254_740_993)), "int64 keeps its width past 2^53");
  assert_eq!(map.get("warehouse"), Some(&Value::str("north")));
  assert_eq!(map.get("status"), Some(&Value::str("ACTIVE")));
  assert_eq!(map.get("reserved"), Some(&Value::int(0)), "an unset scalar is present at its default");
  assert_eq!(map.get("location"), Some(&Value::Null), "an unset message is null");
  assert_eq!(map.get("counted_at"), Some(&Value::str("2026-09-02T10:00:00Z")));
  assert_eq!(map.get("weight_kg"), Some(&Value::Null), "an unset optional scalar is null");
  let Some(Value::Map(by_bin)) = map.get("by_bin") else { panic!("by_bin") };
  assert_eq!(by_bin.get("a1"), Some(&Value::int(3)));
}
