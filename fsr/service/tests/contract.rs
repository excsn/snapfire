use snapfire_fsr_core::{TypedArray, Value, ValueMap};
use snapfire_fsr_service::{
  Contract, ContractError, Field, Method, ScalarKind, Service, Type, TypeDef, Variant,
};

fn users() -> Contract {
  Contract::new()
    .record(
      "User",
      vec![
        Field::new("id", Type::U64),
        Field::new("name", Type::Str),
        Field::new("nickname", Type::optional(Type::Str)),
      ],
    )
    .union(
      "Tier",
      vec![Variant::unit("free"), Variant::unit("paid"), Variant::with("trial", Type::U32)],
    )
    .service(
      "users",
      Service::new()
        .method("get", Method::new(vec![Field::new("id", Type::U64)], Type::named("User")))
        .method(
          "list",
          Method::new(vec![Field::new("limit", Type::optional(Type::U32))], Type::list(Type::named("User"))),
        )
        .method("tier", Method::new(vec![], Type::named("Tier"))),
    )
}

fn user_value(id: i128, name: &str) -> Value {
  let mut map = ValueMap::new();
  map.insert("id".to_owned(), Value::Int(id));
  map.insert("name".to_owned(), Value::str(name));
  Value::Map(map)
}

fn args(pairs: Vec<(&str, Value)>) -> ValueMap {
  pairs.into_iter().map(|(k, v)| (k.to_owned(), v)).collect()
}

#[test]
fn the_artifact_round_trips_through_json() {
  let contract = users();
  let back = Contract::from_json(&contract.to_json()).unwrap();
  assert_eq!(back, contract, "the artifact is the truth both front ends produce");
  assert!(contract.to_json().contains("\"u64\""), "widths survive the artifact");
}

#[test]
fn validate_catches_a_dangling_type_reference() {
  let contract = Contract::new().service(
    "users",
    Service::new().method("get", Method::new(vec![], Type::named("Ghost"))),
  );
  assert!(matches!(
    contract.validate().unwrap_err(),
    ContractError::UnknownType { name, .. } if name == "Ghost"
  ));
  assert!(users().validate().is_ok());
}

#[test]
fn a_call_is_checked_against_the_signature() {
  let contract = users();
  assert!(contract.check_call("users", "get", &args(vec![("id", Value::Int(7))])).is_ok());
  assert!(contract.check_call("users", "list", &ValueMap::new()).is_ok(), "an optional param may be omitted");

  assert!(matches!(
    contract.check_call("billing", "get", &ValueMap::new()).unwrap_err(),
    ContractError::UnknownService(name) if name == "billing"
  ));
  assert!(matches!(
    contract.check_call("users", "purge", &ValueMap::new()).unwrap_err(),
    ContractError::UnknownMethod { .. }
  ));
  assert!(matches!(
    contract.check_call("users", "get", &ValueMap::new()).unwrap_err(),
    ContractError::MissingField { field, .. } if field == "id"
  ));
  assert!(matches!(
    contract.check_call("users", "get", &args(vec![("id", Value::Int(7)), ("extra", Value::Null)])).unwrap_err(),
    ContractError::UnknownField { field, .. } if field == "extra"
  ));
  let err = contract
    .check_call("users", "get", &args(vec![("id", Value::str("7"))]))
    .unwrap_err();
  assert_eq!(err.to_string(), "users.get.id: expected u64, found str");
}

#[test]
fn integer_widths_are_enforced_at_the_boundary() {
  let contract = users();
  let too_big = Value::Int(i128::from(u64::MAX) + 1);
  assert!(contract.check_call("users", "get", &args(vec![("id", too_big)])).is_err());
  assert!(contract.check_call("users", "get", &args(vec![("id", Value::Int(-1))])).is_err());
  assert!(contract
    .check_call("users", "get", &args(vec![("id", Value::Int(i128::from(u64::MAX)))]))
    .is_ok(), "the full u64 range survives, which JSON numbers would not");

  assert!(contract.check_value(&Type::U128, &Value::uint(u128::MAX), "x").is_ok());
  assert!(contract.check_value(&Type::I32, &Value::Int(i128::from(i32::MAX) + 1), "x").is_err());
  assert!(contract.check_value(&Type::F64, &Value::Int(1), "x").is_err(), "no silent numeric coercion");
}

#[test]
fn records_are_strict_in_both_directions() {
  let contract = users();
  assert!(contract.check_return("users", "get", &user_value(1, "alice")).is_ok());

  let mut missing = ValueMap::new();
  missing.insert("id".to_owned(), Value::Int(1));
  assert!(matches!(
    contract.check_return("users", "get", &Value::Map(missing)).unwrap_err(),
    ContractError::MissingField { field, .. } if field == "name"
  ));

  let Value::Map(mut extra) = user_value(1, "alice") else { unreachable!() };
  extra.insert("admin".to_owned(), Value::Bool(true));
  assert!(matches!(
    contract.check_return("users", "get", &Value::Map(extra)).unwrap_err(),
    ContractError::UnknownField { field, .. } if field == "admin"
  ));

  let Value::Map(mut with_optional) = user_value(1, "alice") else { unreachable!() };
  with_optional.insert("nickname".to_owned(), Value::Null);
  assert!(contract.check_return("users", "get", &Value::Map(with_optional)).is_ok());
}

#[test]
fn unions_carry_tags_and_payloads() {
  let contract = users();
  assert!(contract
    .check_return("users", "tier", &Value::Variant { tag: "free".into(), payload: None })
    .is_ok());
  assert!(contract
    .check_return(
      "users",
      "tier",
      &Value::Variant { tag: "trial".into(), payload: Some(Box::new(Value::Int(14))) }
    )
    .is_ok());

  let err = contract
    .check_return("users", "tier", &Value::Variant { tag: "enterprise".into(), payload: None })
    .unwrap_err();
  assert!(matches!(err, ContractError::UnknownVariant { .. }));
  assert!(err.to_string().contains("free, paid, trial"));

  assert!(contract
    .check_return("users", "tier", &Value::Variant { tag: "free".into(), payload: Some(Box::new(Value::Null)) })
    .is_err(), "a unit arm carries no payload");
}

#[test]
fn lists_maps_bytes_and_typed_arrays_name_the_failing_position() {
  let contract = users();
  let good = Value::Seq(vec![user_value(1, "alice"), user_value(2, "bob")]);
  assert!(contract.check_return("users", "list", &good).is_ok());

  let bad = Value::Seq(vec![user_value(1, "alice"), Value::str("nope")]);
  let err = contract.check_return("users", "list", &bad).unwrap_err();
  assert!(err.to_string().starts_with("users.list()[1]:"), "{err}");

  assert!(contract.check_value(&Type::map(Type::I64), &Value::Map(args(vec![("a", Value::Int(1))])), "m").is_ok());
  assert!(contract.check_value(&Type::Bytes, &Value::Bytes(vec![1, 2]), "b").is_ok());
  assert!(contract
    .check_value(&Type::Array(ScalarKind::F64), &Value::TypedArray(TypedArray::F64(vec![1.5])), "a")
    .is_ok());
  assert!(contract
    .check_value(&Type::Array(ScalarKind::F64), &Value::TypedArray(TypedArray::F32(vec![1.5])), "a")
    .is_err(), "element width is part of the type");
}

/// The vocabulary has to receive proto3 and OpenAPI without a redesign, since
/// a brownfield shop imports contracts it already maintains.
#[test]
fn the_vocabulary_receives_a_proto3_message() {
  let contract = Contract::new()
    .union("Status", vec![Variant::unit("ACTIVE"), Variant::unit("SUSPENDED")])
    .union(
      "Payment",
      vec![Variant::with("card", Type::Str), Variant::with("invoice", Type::U64)],
    )
    .record(
      "Account",
      vec![
        Field::new("id", Type::U64),
        Field::new("balance_cents", Type::I64),
        Field::new("labels", Type::list(Type::Str)),
        Field::new("annotations", Type::map(Type::Str)),
        Field::new("status", Type::named("Status")),
        Field::new("payment", Type::named("Payment")),
        Field::new("blob", Type::Bytes),
        Field::new("closed_at", Type::optional(Type::I64)),
      ],
    )
    .service(
      "accounts",
      Service::new().method("get", Method::new(vec![Field::new("id", Type::U64)], Type::named("Account"))),
    );
  contract.validate().unwrap();

  let account = Value::Map(args(vec![
    ("id", Value::Int(9)),
    ("balance_cents", Value::Int(-250)),
    ("labels", Value::Seq(vec![Value::str("vip")])),
    ("annotations", Value::Map(args(vec![("region", Value::str("eu"))]))),
    ("status", Value::Variant { tag: "ACTIVE".into(), payload: None }),
    ("payment", Value::Variant { tag: "card".into(), payload: Some(Box::new(Value::str("4242"))) }),
    ("blob", Value::Bytes(vec![0, 1, 2])),
  ]));
  contract.check_return("accounts", "get", &account).unwrap();
}

#[test]
fn a_union_of_records_covers_openapi_oneof() {
  let contract = Contract::new()
    .record("Dog", vec![Field::new("bark", Type::Bool)])
    .record("Cat", vec![Field::new("purr", Type::Bool)])
    .union("Pet", vec![Variant::with("dog", Type::named("Dog")), Variant::with("cat", Type::named("Cat"))])
    .service("pets", Service::new().method("get", Method::new(vec![], Type::named("Pet"))));
  contract.validate().unwrap();

  let pet = Value::Variant {
    tag: "cat".into(),
    payload: Some(Box::new(Value::Map(args(vec![("purr", Value::Bool(true))])))),
  };
  contract.check_return("pets", "get", &pet).unwrap();

  let wrong = Value::Variant {
    tag: "cat".into(),
    payload: Some(Box::new(Value::Map(args(vec![("bark", Value::Bool(true))])))),
  };
  assert!(contract.check_return("pets", "get", &wrong).is_err());
}

#[test]
fn a_typedef_is_data_the_build_can_read_back() {
  let json = users().to_json();
  let back = Contract::from_json(&json).unwrap();
  let Some(TypeDef::Record { fields }) = back.types.get("User") else { panic!("User is a record") };
  assert_eq!(fields[0], Field::new("id", Type::U64));
  assert_eq!(back.method("users", "get").unwrap().returns, Type::named("User"));
}
