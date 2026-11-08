//! `Dynamic` in a contract: any value passes, and only that part goes unchecked.

use std::sync::Arc;

use futures::executor::block_on;
use serde::{Deserialize, Serialize};
use snapfire_fsr_core::{Dynamic, Value, ValueMap};
use snapfire_fsr_macros::service;
use snapfire_fsr_runtime::{FromNativeValue, IntoNativeValue, ServiceError};
use snapfire_fsr_service::typescript::{declarations, type_name};
use snapfire_fsr_service::{Call, Contract, DeclaredService, Field, Method, Service, Transport, Type};

#[derive(Clone)]
struct Echo;

#[derive(Debug, PartialEq, Serialize, Deserialize)]
struct Asked {
  name: String,
  count: u64,
}

#[service]
impl Echo {
  pub async fn echo(&self, input: Dynamic, label: String) -> Result<Dynamic, ServiceError> {
    let asked: Asked = input.to_serde().unwrap();
    Ok(Dynamic::from_serde(&(asked.name, asked.count + 1, label)).unwrap())
  }
}

fn map(fields: &[(&str, Value)]) -> Value {
  Value::Map(fields.iter().map(|(k, v)| (k.to_string(), v.clone())).collect())
}

fn with_dynamic(ty: Type) -> Contract {
  Contract::new().service("s", Service::new().method("m", Method::new(vec![Field::new("x", ty)], Type::Null)))
}

fn accepts(contract: &Contract, value: Value) -> bool {
  let mut args = ValueMap::default();
  args.insert("x".to_string(), value);
  contract.check_call("s", "m", &args).is_ok()
}

#[test]
fn a_dynamic_field_takes_any_value_and_its_neighbours_stay_checked() {
  let contract = with_dynamic(Type::Dynamic);
  contract.validate().unwrap();
  assert!(accepts(&contract, Value::Null));
  assert!(accepts(&contract, Value::Int(1)));
  assert!(accepts(&contract, Value::str("x")));
  assert!(accepts(&contract, map(&[("nested", map(&[("deep", Value::Bool(true))]))])));

  let listed = with_dynamic(Type::list(Type::Dynamic));
  assert!(accepts(&listed, Value::Seq(vec![Value::Int(1), Value::str("x")].into_iter().collect())));
  assert!(!accepts(&listed, Value::Int(1)), "the list around it is still checked");

  let contract = Contract::new().service("s", Service::new().method("m", Method::new(vec![Field::new("x", Type::Dynamic), Field::new("y", Type::Str)], Type::Null)));
  let args: ValueMap = [("x".to_string(), Value::Int(1)), ("y".to_string(), Value::Int(1))].into_iter().collect();
  assert!(contract.check_call("s", "m", &args).is_err());
}

#[test]
fn typescript_sees_unknown() {
  assert_eq!(type_name(&Type::Dynamic), "unknown");
  assert_eq!(type_name(&Type::list(Type::Dynamic)), "unknown[]");
  assert!(declarations(&with_dynamic(Type::Dynamic)).contains("x: unknown"));
}

#[test]
fn a_contract_with_dynamic_survives_its_file_form() {
  let contract = with_dynamic(Type::Dynamic);
  let text = serde_json::to_string(&contract).unwrap();
  assert_eq!(Contract::from_json(&text).unwrap(), contract);
  assert_eq!(Type::Dynamic.describe(), "dynamic");
}

#[test]
fn a_rust_service_takes_and_answers_dynamic_with_the_contract_read_off_its_signature() {
  let contract = Echo::contract();
  contract.validate().unwrap();
  let method = &contract.services["echo"].methods["echo"];
  assert_eq!(method.returns, Type::Dynamic);
  assert_eq!(method.params[0].ty, Type::Dynamic);

  let echo: Arc<dyn Transport> = Arc::new(Echo);
  let args: ValueMap = [("input".to_string(), map(&[("name", Value::str("a")), ("count", Value::Int(1))])), ("label".to_string(), Value::str("l"))].into_iter().collect();
  let answer = block_on(echo.call(Call::new("echo", "echo", args))).unwrap();
  assert_eq!(answer, Value::Seq(vec![Value::str("a"), Value::Int(2), Value::str("l")].into_iter().collect()));
}

#[test]
fn dynamic_crosses_the_native_boundary_unchanged() {
  let value = map(&[("a", Value::Int(1))]);
  assert_eq!(Dynamic::from_native_value(value.clone()).unwrap(), Dynamic(value.clone()));
  assert_eq!(Dynamic(value.clone()).into_native_value(), value);
}

#[cfg(feature = "grpc")]
mod grpc {
  use snapfire_fsr_service::Type;
  use snapfire_fsr_service::grpc::{decode_response, encode_request};
  use snapfire_fsr_service::proto::import_proto_source;

  use super::*;

  const SOURCE: &str = r#"
syntax = "proto3";
package demo;
import "google/protobuf/struct.proto";
import "google/protobuf/any.proto";

message Holder {
  google.protobuf.Value value = 1;
  google.protobuf.Struct object = 2;
  google.protobuf.ListValue list = 3;
}

service Demo {
  rpc Echo (Holder) returns (Holder);
}
"#;

  #[test]
  fn the_protobuf_json_types_import_as_dynamic() {
    let imported = import_proto_source("demo.proto", SOURCE, "demo").unwrap();
    let Some(snapfire_fsr_service::TypeDef::Record { fields, .. }) = imported.contract.types.get("Holder").or_else(|| imported.contract.types.get("demo.Holder")) else { panic!("{:?}", imported.contract.types.keys()) };
    for field in fields {
      assert!(matches!(&field.ty, Type::Dynamic) || matches!(&field.ty, Type::Optional(inner) if **inner == Type::Dynamic), "{field:?}");
    }
  }

  #[test]
  fn protobuf_any_stays_refused_because_it_is_opaque_bytes() {
    let source = SOURCE.replace("google.protobuf.ListValue list = 3;", "google.protobuf.Any list = 3;");
    assert!(import_proto_source("demo.proto", &source, "demo").is_err());
  }

  #[test]
  fn a_dynamic_value_round_trips_through_the_wire() {
    let imported = import_proto_source("demo.proto", SOURCE, "demo").unwrap();
    let holder = imported.pool.get_message_by_name("demo.Holder").unwrap();
    let nested = map(&[("n", Value::F64(1.5)), ("t", Value::str("x")), ("b", Value::Bool(true)), ("nothing", Value::Null), ("list", Value::Seq(vec![Value::F64(2.0), Value::str("y")].into_iter().collect()))]);
    let fields: ValueMap = [("value".to_string(), nested.clone()), ("object".to_string(), nested.clone()), ("list".to_string(), Value::Seq(vec![Value::F64(1.0), nested.clone()].into_iter().collect()))].into_iter().collect();

    let bytes = encode_request(&holder, &fields).unwrap();
    let Value::Map(back) = decode_response(&holder, &bytes).unwrap() else { panic!("a record") };
    assert_eq!(back.get("value"), Some(&nested));
    assert_eq!(back.get("object"), Some(&nested));
    assert_eq!(back.get("list"), Some(&Value::Seq(vec![Value::F64(1.0), nested].into_iter().collect())));
  }

  #[test]
  fn a_small_integer_becomes_a_double_and_one_too_big_to_fit_is_refused() {
    let imported = import_proto_source("demo.proto", SOURCE, "demo").unwrap();
    let holder = imported.pool.get_message_by_name("demo.Holder").unwrap();
    let args = |value: Value| -> ValueMap { [("value".to_string(), value)].into_iter().collect() };

    let bytes = encode_request(&holder, &args(Value::Int(5))).unwrap();
    let Value::Map(back) = decode_response(&holder, &bytes).unwrap() else { panic!("a record") };
    assert_eq!(back.get("value"), Some(&Value::F64(5.0)));

    let error = encode_request(&holder, &args(Value::Int(1 << 60))).unwrap_err();
    assert!(error.contains("double"), "{error}");
    assert!(encode_request(&holder, &args(Value::Bytes(vec![1]))).is_err());
  }
}
