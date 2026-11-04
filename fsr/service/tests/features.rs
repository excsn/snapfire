//! A contract whose feature is off is refused at import, naming the feature, so an application that never declares one compiles none of its importer.

#[cfg(not(feature = "grpc"))]
#[test]
fn a_proto_contract_without_the_grpc_feature_names_the_feature() {
  let refused = snapfire_fsr_service::import_proto(std::path::Path::new("clients/shop.proto"), "shop").err().expect("refused");
  assert!(refused.to_string().contains("`grpc`"), "{refused}");
}

#[cfg(not(feature = "graphql"))]
#[test]
fn a_graphql_contract_without_the_graphql_feature_names_the_feature() {
  let refused = snapfire_fsr_service::import_graphql("type Query { a: Int }", "shop").err().expect("refused");
  assert!(refused.to_string().contains("`graphql`"), "{refused}");
}
