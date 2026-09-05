use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;

use futures_util::future::BoxFuture;
use parking_lot::Mutex;
use snapfire_fsr_core::{Value, ValueMap};
use snapfire_fsr_runtime::{FailureKind, Identity, ServiceError};
use snapfire_fsr_service::{Call, Contract, Field, Freshness, Method, MockTransport, NoCredentials, Service, Services, Transport, Type};

fn contract() -> Contract {
  Contract::new()
    .service(
      "catalog",
      Service::new()
        .method("list", Method::new(vec![Field::new("tag", Type::optional(Type::Str))], Type::list(Type::Str)).cached(Freshness::ttl("30s").tags(["catalog"]).shared()))
        .method("mine", Method::new(vec![], Type::list(Type::Str)).cached(Freshness::ttl("30s").per_subject()))
        .method("secret", Method::new(vec![], Type::Str).cached(Freshness::ttl("30s")))
        .method("add", Method::new(vec![Field::new("name", Type::Str)], Type::Null).writes(["catalog"]))
        .method("plain", Method::new(vec![], Type::Str)),
    )
    .service("feed", Service::new().method("latest", Method::new(vec![], Type::Str).cached(Freshness::ttl("1s").stale("5s").shared())))
}

fn mock() -> Arc<MockTransport> {
  Arc::new(
    MockTransport::new()
      .returns("catalog.list", Value::Seq(vec![Value::str("socks")]))
      .returns("catalog.mine", Value::Seq(vec![Value::str("hat")]))
      .returns("catalog.secret", Value::str("s"))
      .returns("catalog.add", Value::Null)
      .returns("catalog.plain", Value::str("p"))
      .returns("feed.latest", Value::str("now")),
  )
}

fn services(transport: Arc<dyn Transport>) -> Arc<Services> {
  Services::builder().contract(contract()).default_transport(transport).data_cache(100).build()
}

fn alice() -> Option<Identity> {
  Some(Identity { subject: "alice".to_owned(), claims: ValueMap::new() })
}

fn bob() -> Option<Identity> {
  Some(Identity { subject: "bob".to_owned(), claims: ValueMap::new() })
}

fn count(transport: &MockTransport, path: &str) -> usize {
  transport.calls().iter().filter(|(p, _, _)| p == path).count()
}

fn args(tag: Option<&str>) -> ValueMap {
  let mut map = ValueMap::new();
  if let Some(tag) = tag {
    map.insert("tag".to_owned(), Value::str(tag));
  }
  map
}

#[tokio::test]
async fn a_shared_method_is_answered_once_per_distinct_arguments() {
  let transport = mock();
  let services = services(transport.clone());
  let anon = services.bind_anonymous();
  anon.call("catalog", "list", args(None)).await.unwrap();
  anon.call("catalog", "list", args(None)).await.unwrap();
  services.bind(alice(), Arc::new(NoCredentials)).call("catalog", "list", args(None)).await.unwrap();
  assert_eq!(count(&transport, "catalog.list"), 1, "shared: everyone reads the one entry");
  anon.call("catalog", "list", args(Some("wool"))).await.unwrap();
  assert_eq!(count(&transport, "catalog.list"), 2, "other arguments are another entry");
  assert_eq!(services.data_cache().unwrap().hits(), 2);
  anon.call("catalog", "plain", ValueMap::new()).await.unwrap();
  anon.call("catalog", "plain", ValueMap::new()).await.unwrap();
  assert_eq!(count(&transport, "catalog.plain"), 2, "a method without a policy is never cached");
}

#[tokio::test]
async fn a_private_method_bypasses_the_cache_for_an_identified_call() {
  let transport = mock();
  let services = services(transport.clone());
  let anon = services.bind_anonymous();
  anon.call("catalog", "secret", ValueMap::new()).await.unwrap();
  anon.call("catalog", "secret", ValueMap::new()).await.unwrap();
  assert_eq!(count(&transport, "catalog.secret"), 1, "anonymous calls share an entry");
  let signed = services.bind(alice(), Arc::new(NoCredentials));
  signed.call("catalog", "secret", ValueMap::new()).await.unwrap();
  signed.call("catalog", "secret", ValueMap::new()).await.unwrap();
  assert_eq!(count(&transport, "catalog.secret"), 3, "an identified call never reads or writes the cache");
}

#[tokio::test]
async fn a_subject_method_keeps_one_entry_per_subject() {
  let transport = mock();
  let services = services(transport.clone());
  let a = services.bind(alice(), Arc::new(NoCredentials));
  let b = services.bind(bob(), Arc::new(NoCredentials));
  a.call("catalog", "mine", ValueMap::new()).await.unwrap();
  a.call("catalog", "mine", ValueMap::new()).await.unwrap();
  b.call("catalog", "mine", ValueMap::new()).await.unwrap();
  b.call("catalog", "mine", ValueMap::new()).await.unwrap();
  services.bind_anonymous().call("catalog", "mine", ValueMap::new()).await.unwrap();
  assert_eq!(count(&transport, "catalog.mine"), 3, "alice, bob and nobody");
}

#[tokio::test]
async fn a_write_drops_the_tags_it_names_and_so_does_invalidate_tags() {
  let transport = mock();
  let services = services(transport.clone());
  let anon = services.bind_anonymous();
  anon.call("catalog", "list", args(None)).await.unwrap();
  let mut add = ValueMap::new();
  add.insert("name".to_owned(), Value::str("scarf"));
  anon.call("catalog", "add", add).await.unwrap();
  anon.call("catalog", "list", args(None)).await.unwrap();
  assert_eq!(count(&transport, "catalog.list"), 2, "the write moved the tag on");
  anon.call("catalog", "list", args(None)).await.unwrap();
  assert_eq!(count(&transport, "catalog.list"), 2);
  services.invalidate_tags(["catalog"]);
  anon.call("catalog", "list", args(None)).await.unwrap();
  assert_eq!(count(&transport, "catalog.list"), 3, "an out-of-band drop");
  services.invalidate_tags(["other"]);
  anon.call("catalog", "list", args(None)).await.unwrap();
  assert_eq!(count(&transport, "catalog.list"), 3, "a tag the method does not carry changes nothing");
}

/// Fails once, then answers, counting every call.
struct Flaky {
  calls: AtomicU64,
  failed: Mutex<bool>,
}

impl Transport for Flaky {
  fn call(&self, call: Call) -> BoxFuture<'static, Result<Value, ServiceError>> {
    self.calls.fetch_add(1, Ordering::SeqCst);
    let mut failed = self.failed.lock();
    let result = if *failed {
      Ok(Value::Seq(vec![Value::str("socks")]))
    } else {
      *failed = true;
      Err(ServiceError::new(FailureKind::Unavailable, call.service, call.method, "down"))
    };
    Box::pin(async move { result })
  }
}

#[tokio::test]
async fn a_failure_is_never_cached() {
  let flaky = Arc::new(Flaky { calls: AtomicU64::new(0), failed: Mutex::new(false) });
  let services = services(flaky.clone());
  let anon = services.bind_anonymous();
  assert!(anon.call("catalog", "list", args(None)).await.is_err());
  anon.call("catalog", "list", args(None)).await.unwrap();
  anon.call("catalog", "list", args(None)).await.unwrap();
  assert_eq!(flaky.calls.load(Ordering::SeqCst), 2, "the failure was asked again, the answer was not");
}

#[tokio::test]
async fn a_stale_window_serves_the_last_answer_and_refreshes_behind_it() {
  let transport = mock();
  let services = services(transport.clone());
  let anon = services.bind_anonymous();
  anon.call("feed", "latest", ValueMap::new()).await.unwrap();
  tokio::time::sleep(Duration::from_millis(1300)).await;
  let served = anon.call("feed", "latest", ValueMap::new()).await.unwrap();
  assert_eq!(served, Value::str("now"), "past the ttl the stale answer is served");
  tokio::time::sleep(Duration::from_millis(400)).await;
  assert_eq!(count(&transport, "feed.latest"), 2, "and a refresh ran behind it");
  anon.call("feed", "latest", ValueMap::new()).await.unwrap();
  assert_eq!(count(&transport, "feed.latest"), 2, "the refreshed entry is fresh again");
}

#[test]
fn the_contract_refuses_a_stale_window_off_shared_scope_and_a_bad_duration() {
  let bad_scope = Contract::new().service("x", Service::new().method("m", Method::new(vec![], Type::Str).cached(Freshness::ttl("30s").stale("1m"))));
  let error = bad_scope.validate().unwrap_err().to_string();
  assert!(error.contains("x.m") && error.contains("shared"), "{error}");
  let bad_ttl = Contract::new().service("x", Service::new().method("m", Method::new(vec![], Type::Str).cached(Freshness::ttl("soon"))));
  let error = bad_ttl.validate().unwrap_err().to_string();
  assert!(error.contains("soon"), "{error}");
  assert!(contract().validate().is_ok());
  let error = Services::builder().contract(bad_scope).default_transport(mock()).data_cache(10).try_build().err().unwrap().to_string();
  assert!(error.contains("shared"), "{error}");
}

#[test]
fn the_policy_survives_the_artifact_and_an_openapi_document() {
  let json = serde_json::to_string(&contract()).unwrap();
  let back: Contract = serde_json::from_str(&json).unwrap();
  assert_eq!(back, contract());
  assert!(json.contains("\"cache\":{\"ttl\":\"30s\",\"tags\":[\"catalog\"],\"scope\":\"shared\"}"), "{json}");
  assert!(json.contains("\"writes\":[\"catalog\"]"), "{json}");

  let document = r##"{
    "openapi": "3.0.3", "info": { "title": "Shop", "version": "1" },
    "paths": {
      "/items": { "get": { "operationId": "list", "x-sf-cache": { "ttl": "5m", "tags": ["items"], "scope": "shared", "stale": "10m" },
        "responses": { "200": { "description": "items", "content": { "application/json": { "schema": { "type": "array", "items": { "type": "string" } } } } } } } },
      "/items/{name}": { "put": { "operationId": "add", "x-sf-writes": ["items"],
        "parameters": [{ "name": "name", "in": "path", "required": true, "schema": { "type": "string" } }],
        "responses": { "204": { "description": "added" } } } }
    }
  }"##;
  let imported = snapfire_fsr_service::import(document, "shop").unwrap();
  let list = imported.contract.method("shop", "list").unwrap();
  assert_eq!(list.cache, Some(Freshness::ttl("5m").tags(["items"]).shared().stale("10m")));
  assert_eq!(imported.contract.method("shop", "add").unwrap().writes, vec!["items".to_owned()]);
}
