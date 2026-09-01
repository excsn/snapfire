use snapfire_fsr_service::openapi::{import, ImportError};
use snapfire_fsr_service::{Type, TypeDef};

const SHOPPING: &str = include_str!("../../examples/shopping_react_ts/openapi.json");

fn doc(paths: &str, schemas: &str) -> String {
  format!(
    r##"{{"openapi":"3.0.3","info":{{"title":"t","version":"1"}},"paths":{{{paths}}},"components":{{"schemas":{{{schemas}}}}}}}"##
  )
}

#[test]
fn a_real_document_lowers_to_a_contract_and_its_routes() {
  let imported = import(SHOPPING, "shopping").unwrap();
  let contract = &imported.contract;

  let list = contract.method("shopping", "listProducts").unwrap();
  assert_eq!(list.params.len(), 1);
  assert_eq!(list.params[0].name, "tag");
  assert_eq!(list.params[0].ty, Type::optional(Type::Str), "an optional query param");
  assert_eq!(list.returns, Type::list(Type::named("Product")));

  let get = contract.method("shopping", "getProduct").unwrap();
  assert_eq!(get.params[0].ty, Type::I64, "a path param is always required");
  assert_eq!(get.returns, Type::named("Product"));

  let place = contract.method("shopping", "placeOrder").unwrap();
  assert_eq!(place.params.len(), 1, "the body object spreads into named arguments");
  assert_eq!(place.params[0].name, "lines");
  assert_eq!(place.params[0].ty, Type::list(Type::named("OrderLine")));
  assert_eq!(place.returns, Type::named("Order"));

  let Some(TypeDef::Record { fields }) = contract.types.get("Product") else { panic!("Product") };
  assert_eq!(fields[0].ty, Type::I64, "int64 keeps its width");
  assert_eq!(fields[3].ty, Type::I32, "int32 keeps its width");
  assert_eq!(fields[4].ty, Type::list(Type::Str));

  let routes: Vec<(String, String, String)> = imported
    .routes
    .iter()
    .map(|(k, r)| (k.clone(), r.method.clone(), r.path.clone()))
    .collect();
  assert!(routes.contains(&("shopping.listProducts".into(), "GET".into(), "/products".into())));
  assert!(routes.contains(&("shopping.getProduct".into(), "GET".into(), "/products/{id}".into())));
  assert!(routes.contains(&("shopping.placeOrder".into(), "POST".into(), "/orders".into())));
}

#[test]
fn scalar_formats_map_onto_the_value_model_widths() {
  let d = doc(
    r##""/x":{"get":{"operationId":"x","responses":{"200":{"content":{"application/json":{"schema":{"$ref":"#/components/schemas/S"}}}}}}}"##,
    r##""S":{"type":"object","required":["a","b","c","d","e","f","g","h"],"properties":{
      "a":{"type":"integer","format":"int32"},
      "b":{"type":"integer","format":"int64"},
      "c":{"type":"integer"},
      "d":{"type":"number","format":"float"},
      "e":{"type":"number"},
      "f":{"type":"string","format":"byte"},
      "g":{"type":"boolean"},
      "h":{"type":"string"}}}"##,
  );
  let contract = import(&d, "s").unwrap().contract;
  let Some(TypeDef::Record { fields }) = contract.types.get("S") else { panic!() };
  let types: Vec<Type> = fields.iter().map(|f| f.ty.clone()).collect();
  assert_eq!(
    types,
    vec![Type::I32, Type::I64, Type::I64, Type::F32, Type::F64, Type::Bytes, Type::Bool, Type::Str]
  );
}

#[test]
fn nullable_becomes_optional_in_both_dialects() {
  let d = doc(
    r##""/x":{"get":{"operationId":"x","responses":{"200":{"content":{"application/json":{"schema":{"$ref":"#/components/schemas/S"}}}}}}}"##,
    r##""S":{"type":"object","required":["a","b"],"properties":{
      "a":{"type":"string","nullable":true},
      "b":{"type":["integer","null"],"format":"int32"}}}"##,
  );
  let contract = import(&d, "s").unwrap().contract;
  let Some(TypeDef::Record { fields }) = contract.types.get("S") else { panic!() };
  assert_eq!(fields[0].ty, Type::optional(Type::Str), "3.0 nullable");
  assert_eq!(fields[1].ty, Type::optional(Type::I32), "3.1 type array");
}

#[test]
fn an_enum_becomes_a_union_of_unit_variants() {
  let d = doc(
    r##""/x":{"get":{"operationId":"x","responses":{"200":{"content":{"application/json":{"schema":{"$ref":"#/components/schemas/Status"}}}}}}}"##,
    r##""Status":{"type":"string","enum":["open","closed"]}"##,
  );
  let contract = import(&d, "s").unwrap().contract;
  let Some(TypeDef::Union { variants }) = contract.types.get("Status") else { panic!("Status is a union") };
  assert_eq!(variants.len(), 2);
  assert_eq!(variants[0].tag, "open");
  assert_eq!(variants[0].payload, None);
}

#[test]
fn one_of_becomes_a_union_tagged_by_the_referenced_names() {
  let d = doc(
    r##""/x":{"get":{"operationId":"x","responses":{"200":{"content":{"application/json":{"schema":{"$ref":"#/components/schemas/Pet"}}}}}}}"##,
    r##""Dog":{"type":"object","required":["bark"],"properties":{"bark":{"type":"boolean"}}},
      "Cat":{"type":"object","required":["purr"],"properties":{"purr":{"type":"boolean"}}},
      "Pet":{"oneOf":[{"$ref":"#/components/schemas/Dog"},{"$ref":"#/components/schemas/Cat"}]}"##,
  );
  let contract = import(&d, "s").unwrap().contract;
  let Some(TypeDef::Union { variants }) = contract.types.get("Pet") else { panic!("Pet is a union") };
  assert_eq!(variants[0].tag, "Dog");
  assert_eq!(variants[0].payload, Some(Type::named("Dog")));
  assert_eq!(variants[1].tag, "Cat");
}

#[test]
fn all_of_merges_its_branches_into_one_record() {
  let d = doc(
    r##""/x":{"get":{"operationId":"x","responses":{"200":{"content":{"application/json":{"schema":{"$ref":"#/components/schemas/Employee"}}}}}}}"##,
    r##""Person":{"type":"object","required":["name"],"properties":{"name":{"type":"string"}}},
      "Employee":{"allOf":[{"$ref":"#/components/schemas/Person"},{"type":"object","required":["desk"],"properties":{"desk":{"type":"integer","format":"int32"}}}]}"##,
  );
  let contract = import(&d, "s").unwrap().contract;
  let Some(TypeDef::Record { fields }) = contract.types.get("Employee") else { panic!() };
  let names: Vec<&str> = fields.iter().map(|f| f.name.as_str()).collect();
  assert_eq!(names, vec!["name", "desk"]);
}

#[test]
fn a_self_referential_schema_resolves() {
  let d = doc(
    r##""/x":{"get":{"operationId":"x","responses":{"200":{"content":{"application/json":{"schema":{"$ref":"#/components/schemas/Node"}}}}}}}"##,
    r##""Node":{"type":"object","required":["name"],"properties":{"name":{"type":"string"},"children":{"type":"array","items":{"$ref":"#/components/schemas/Node"}}}}"##,
  );
  let contract = import(&d, "s").unwrap().contract;
  let Some(TypeDef::Record { fields }) = contract.types.get("Node") else { panic!() };
  assert_eq!(fields[1].ty, Type::optional(Type::list(Type::named("Node"))));
  contract.validate().unwrap();
}

#[test]
fn additional_properties_becomes_a_map() {
  let d = doc(
    r##""/x":{"get":{"operationId":"x","responses":{"200":{"content":{"application/json":{"schema":{"$ref":"#/components/schemas/Labels"}}}}}}}"##,
    r##""Labels":{"type":"object","additionalProperties":{"type":"string"}}"##,
  );
  let imported = import(&d, "s").unwrap();
  assert_eq!(imported.contract.method("s", "x").unwrap().returns, Type::map(Type::Str));
}

#[test]
fn tags_group_operations_into_services() {
  let d = doc(
    r##""/a":{"get":{"operationId":"a","tags":["catalog"],"responses":{"204":{"description":"none"}}}},
      "/b":{"get":{"operationId":"b","responses":{"204":{"description":"none"}}}}"##,
    "",
  );
  let contract = import(&d, "fallback").unwrap().contract;
  assert!(contract.method("catalog", "a").is_some(), "the tag names the service");
  assert!(contract.method("fallback", "b").is_some(), "an untagged operation joins the default");
  assert_eq!(contract.method("catalog", "a").unwrap().returns, Type::Null, "no content is Null");
}

#[test]
fn a_missing_operation_id_is_derived_from_the_verb_and_path() {
  let d = doc(
    r##""/products/{id}/reviews":{"get":{"responses":{"204":{"description":"none"}}}}"##,
    "",
  );
  let contract = import(&d, "s").unwrap().contract;
  assert!(contract.method("s", "getProductsIdReviews").is_some(), "{:?}", contract.services);
}

#[test]
fn unsupported_constructs_name_themselves_and_where_they_are() {
  let header = doc(
    r##""/x":{"get":{"operationId":"x","parameters":[{"name":"h","in":"header","schema":{"type":"string"}}],"responses":{"204":{"description":"none"}}}}"##,
    "",
  );
  let err = import(&header, "s").unwrap_err();
  assert!(matches!(err, ImportError::Unsupported { .. }), "{err}");
  assert!(err.to_string().contains("`header` parameter"), "{err}");
  assert!(err.to_string().contains("#/paths//x/get"), "{err}");

  let external = doc(
    r##""/x":{"get":{"operationId":"x","responses":{"200":{"content":{"application/json":{"schema":{"$ref":"other.json#/S"}}}}}}}"##,
    "",
  );
  assert!(import(&external, "s").unwrap_err().to_string().contains("other.json"));

  let open_map = doc(
    r##""/x":{"get":{"operationId":"x","responses":{"200":{"content":{"application/json":{"schema":{"$ref":"#/components/schemas/M"}}}}}}}"##,
    r##""M":{"type":"object","additionalProperties":true}"##,
  );
  assert!(import(&open_map, "s").unwrap_err().to_string().contains("additionalProperties: true"));

  assert!(import("not json", "s").unwrap_err().to_string().contains("not JSON"));
}

#[test]
fn an_inline_response_object_gets_a_name_from_its_operation() {
  let d = doc(
    r##""/x":{"get":{"operationId":"listThings","responses":{"200":{"content":{"application/json":{"schema":{"type":"object","required":["count"],"properties":{"count":{"type":"integer","format":"int32"}}}}}}}}}"##,
    "",
  );
  let contract = import(&d, "s").unwrap().contract;
  assert_eq!(contract.method("s", "listThings").unwrap().returns, Type::named("ListThingsResult"));
  assert!(matches!(contract.types.get("ListThingsResult"), Some(TypeDef::Record { .. })));
}

#[test]
fn the_server_url_comes_back_for_the_transport() {
  let d = format!(
    r##"{{"openapi":"3.0.3","info":{{"title":"t","version":"1"}},"servers":[{{"url":"http://127.0.0.1:8081"}}],"paths":{{"/x":{{"get":{{"operationId":"x","responses":{{"204":{{"description":"none"}}}}}}}}}}}}"##
  );
  assert_eq!(import(&d, "s").unwrap().base_url.as_deref(), Some("http://127.0.0.1:8081"));
}
