use std::sync::Arc;

use shopping_react_ts::backend::inventory;
use snapfire_fsr_core::{Value, ValueMap};
use snapfire_fsr_runtime::FailureKind;
use snapfire_fsr_service::{import_proto, Call, GrpcTransport, NoCredentials, Transport};

fn call(method: &str, product_id: i64) -> Call {
  let mut args = ValueMap::new();
  args.insert("product_id".to_owned(), Value::int(product_id));
  Call { service: "inventory".to_owned(), method: method.to_owned(), args, identity: None, metadata: ValueMap::new(), credentials: Arc::new(NoCredentials) }
}

#[tokio::test]
async fn the_proto_reaches_the_warehouse_over_grpc_with_no_generated_client() {
  let (listener, bound) = inventory::bind("127.0.0.1:0".parse().unwrap()).await.unwrap();
  tokio::spawn(inventory::serve_on(listener));

  let proto = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("app/clients/inventory.proto");
  let imported = import_proto(&proto, "inventory").unwrap();
  assert!(imported.contract.method("inventory", "getStock").is_some());
  let transport = GrpcTransport::new(&format!("http://{bound}"), &imported).unwrap();

  let level = transport.call(call("getStock", 4)).await.unwrap();
  let Value::Map(level) = level else { panic!("a record") };
  assert_eq!(level.get("product_id"), Some(&Value::int(4)));
  assert_eq!(level.get("on_hand"), Some(&Value::int(7)));
  assert_eq!(level.get("warehouse"), Some(&Value::str("north")));
  assert_eq!(level.get("bins"), Some(&Value::Seq(vec![Value::str("N-04")])));

  let missing = transport.call(call("getStock", 99)).await.unwrap_err();
  assert_eq!(missing.kind, FailureKind::NotFound, "a gRPC status maps onto a failure kind: {missing:?}");

  let unknown = transport.call(call("nope", 1)).await.unwrap_err();
  assert_eq!(unknown.kind, FailureKind::NotFound);
}
