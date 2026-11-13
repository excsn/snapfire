use std::sync::Arc;

use futures_util::future::BoxFuture;
use parking_lot::Mutex;
use snapfire_fsr_core::{Value, ValueMap};
use snapfire_fsr_runtime::{FailureKind, ServiceError};
use snapfire_fsr_service::{
  import, Call, Contract, Field, Freshness, Interceptor, Method, MockTransport, Next, Recording, Service, Services, Type,
};

fn repo_document(closed: bool) -> String {
  let mut schema = serde_json::json!({
    "type": "object",
    "required": ["stargazers_count"],
    "properties": { "stargazers_count": { "type": "integer" } }
  });
  if closed {
    schema["additionalProperties"] = serde_json::Value::Bool(false);
  }
  serde_json::json!({
    "openapi": "3.0.0",
    "info": { "title": "github", "version": "1" },
    "paths": { "/repos/{owner}": { "get": {
      "operationId": "getRepo",
      "parameters": [{ "name": "owner", "in": "path", "required": true, "schema": { "type": "string" } }],
      "responses": { "200": { "description": "ok", "content": { "application/json": { "schema": schema } } } }
    } } }
  })
  .to_string()
}

fn github_answer() -> Value {
  let mut map = ValueMap::default();
  map.insert("stargazers_count".to_owned(), Value::Int(7));
  map.insert("id".to_owned(), Value::Int(1051881563));
  map.insert("name".to_owned(), Value::str("snapfire"));
  Value::Map(map)
}

fn owner() -> ValueMap {
  let mut args = ValueMap::default();
  args.insert("owner".to_owned(), Value::str("excsn"));
  args
}

#[tokio::test]
async fn an_answer_for_an_openapi_object_keeps_the_fields_it_declares_and_drops_the_rest() {
  let imported = import(&repo_document(false), "github").unwrap();
  let services = Services::builder()
    .contract(imported.contract)
    .default_transport(Arc::new(MockTransport::new().returns("github.getRepo", github_answer())))
    .build();
  let got = services.bind_anonymous().call("github", "getRepo", owner()).await.unwrap();
  let mut expected = ValueMap::default();
  expected.insert("stargazers_count".to_owned(), Value::Int(7));
  assert_eq!(got, Value::Map(expected));
}

#[tokio::test]
async fn an_openapi_object_with_additional_properties_false_refuses_a_field_it_does_not_declare() {
  let imported = import(&repo_document(true), "github").unwrap();
  let services = Services::builder()
    .contract(imported.contract)
    .default_transport(Arc::new(MockTransport::new().returns("github.getRepo", github_answer())))
    .build();
  let failed = services.bind_anonymous().call("github", "getRepo", owner()).await.unwrap_err();
  assert_eq!(failed.kind, FailureKind::Internal);
  assert!(failed.message.contains("unknown field `"), "{}", failed.message);
}

#[cfg(feature = "grpc")]
#[tokio::test]
async fn an_answer_for_a_proto_message_keeps_the_fields_it_declares_and_drops_the_rest() {
  let proto = "syntax = \"proto3\";\npackage gh;\nservice Github { rpc GetRepo (RepoRequest) returns (Repo); }\nmessage RepoRequest { string owner = 1; }\nmessage Repo { int64 stargazers_count = 1; }\n";
  let imported = snapfire_fsr_service::import_proto_source("github.proto", proto, "github").unwrap();
  let services = Services::builder()
    .contract(imported.contract)
    .default_transport(Arc::new(MockTransport::new().returns("github.getRepo", github_answer())))
    .build();
  let got = services.bind_anonymous().call("github", "getRepo", owner()).await.unwrap();
  let Value::Map(map) = got else { panic!("{got:?}") };
  assert_eq!(map.keys().map(|k| k.to_string()).collect::<Vec<_>>(), vec!["stargazers_count"]);
}

#[tokio::test]
async fn a_record_a_contract_declares_in_code_refuses_a_field_it_does_not_declare() {
  let contract = Contract::new()
    .record("Repo", vec![Field::new("stargazers_count", Type::I64)])
    .service("github", Service::new().method("getRepo", Method::new(vec![Field::new("owner", Type::Str)], Type::named("Repo"))));
  let services = Services::builder()
    .contract(contract)
    .default_transport(Arc::new(MockTransport::new().returns("github.getRepo", github_answer())))
    .build();
  let failed = services.bind_anonymous().call("github", "getRepo", owner()).await.unwrap_err();
  assert_eq!(failed.kind, FailureKind::Internal);
}

/// Records whether each call it wraps succeeded, standing where the trace interceptor stands: outside the data cache.
#[derive(Default)]
struct Outcomes(Arc<Mutex<Vec<bool>>>);

impl Interceptor for Outcomes {
  fn call(&self, call: Call, next: Next) -> BoxFuture<'static, Result<Value, ServiceError>> {
    let running = next.run(call);
    let seen = self.0.clone();
    Box::pin(async move {
      let result = running.await;
      seen.lock().push(result.is_ok());
      result
    })
  }
}

#[tokio::test]
async fn an_answer_that_fails_the_contract_fails_the_call_for_every_interceptor_and_the_data_cache_keeps_none() {
  let contract = Contract::new()
    .record("Repo", vec![Field::new("stargazers_count", Type::I64)])
    .service(
      "github",
      Service::new().method("getRepo", Method::new(vec![Field::new("owner", Type::Str)], Type::named("Repo")).cached(Freshness::ttl("1m").shared())),
    );
  let transport = Arc::new(Recording::over(MockTransport::new().returns("github.getRepo", github_answer())));
  let outcomes = Arc::new(Outcomes::default());
  let services = Services::builder().contract(contract).intercept(outcomes.clone()).default_transport(transport.clone()).data_cache(10).build();

  for _ in 0..2 {
    let failed = services.bind_anonymous().call("github", "getRepo", owner()).await.unwrap_err();
    assert_eq!(failed.kind, FailureKind::Internal);
  }
  assert_eq!(*outcomes.0.lock(), vec![false, false], "the interceptor outside the cache sees each failure");
  assert_eq!(transport.calls().len(), 2, "a failed answer is never kept, so the second call reaches the transport again");
}
