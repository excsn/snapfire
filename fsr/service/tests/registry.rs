use std::sync::Arc;

use futures::executor::block_on;
use snapfire_fsr_core::{Value, ValueMap};
use snapfire_fsr_runtime::{FailureKind, Identity, RequestCtx, ServiceHandle};
use snapfire_fsr_service::{
  Contract, CredentialInterceptor, Field, IdentityInterceptor, LocalTransport, Method,
  MockTransport, Services, TraceInterceptor, Type,
};
use snapfire_fsr_session::TokenCell;

fn contract() -> Contract {
  Contract::new()
    .record("Server", vec![Field::new("name", Type::Str), Field::new("load", Type::F64)])
    .service(
      "fleet",
      snapfire_fsr_service::Service::new()
        .method("get", Method::new(vec![Field::new("name", Type::Str)], Type::named("Server")))
        .method("count", Method::new(vec![], Type::U32)),
    )
}

fn server(name: &str, load: f64) -> Value {
  let mut map = ValueMap::new();
  map.insert("name".to_owned(), Value::str(name));
  map.insert("load".to_owned(), Value::F64(load));
  Value::Map(map)
}

fn args(pairs: Vec<(&str, Value)>) -> ValueMap {
  pairs.into_iter().map(|(k, v)| (k.to_owned(), v)).collect()
}

#[test]
fn a_loader_calls_through_ctx_services_and_never_names_a_transport() {
  let services = Services::builder()
    .contract(contract())
    .default_transport(Arc::new(MockTransport::new().returns("fleet.get", server("web-1", 0.5))))
    .build();

  let ctx = RequestCtx { services: services.bind_anonymous(), ..Default::default() };
  let got = block_on(ctx.services.call("fleet", "get", args(vec![("name", Value::str("web-1"))]))).unwrap();
  assert_eq!(got, server("web-1", 0.5));
}

#[test]
fn an_unbound_handle_fails_rather_than_pretending() {
  let ctx = RequestCtx::anonymous(Default::default());
  assert!(!ctx.services.is_bound());
  let err = block_on(ctx.services.call("fleet", "get", ValueMap::new())).unwrap_err();
  assert_eq!(err.kind, FailureKind::Unavailable);
}

#[test]
fn the_contract_rejects_a_bad_call_before_the_transport_sees_it() {
  let transport = Arc::new(MockTransport::new().returns("fleet.get", server("web-1", 0.5)));
  let services = Services::builder()
    .contract(contract())
    .default_transport(transport.clone())
    .build();
  let handle = services.bind_anonymous();

  let err = block_on(handle.call("fleet", "get", args(vec![("name", Value::Int(1))]))).unwrap_err();
  assert_eq!(err.kind, FailureKind::Invalid);
  assert!(err.to_string().contains("expected str"), "{err}");

  let err = block_on(handle.call("fleet", "purge", ValueMap::new())).unwrap_err();
  assert_eq!(err.kind, FailureKind::NotFound);

  assert!(transport.calls().is_empty(), "nothing reached the wire");
}

#[test]
fn a_backend_that_breaks_its_contract_is_an_internal_failure() {
  let services = Services::builder()
    .contract(contract())
    .default_transport(Arc::new(MockTransport::new().returns("fleet.get", Value::str("web-1"))))
    .build();

  let err = block_on(services.bind_anonymous().call("fleet", "get", args(vec![("name", Value::str("web-1"))])))
    .unwrap_err();
  assert_eq!(err.kind, FailureKind::Internal);
  assert!(err.to_string().contains("expected Server"), "{err}");
}

#[test]
fn interceptors_attach_identity_and_the_token_without_application_code() {
  let transport = Arc::new(MockTransport::new().returns("fleet.count", Value::Int(3)));
  let services = Services::builder()
    .contract(contract())
    .intercept(Arc::new(TraceInterceptor::new()))
    .intercept(Arc::new(IdentityInterceptor::new()))
    .intercept(Arc::new(CredentialInterceptor::bearer("access_token")))
    .default_transport(transport.clone())
    .build();

  let tokens = TokenCell::default();
  tokens.set("access_token", Value::str("secret-abc"));
  let identity = Identity { subject: "alice".into(), claims: Default::default() };
  let handle = services.bind(Some(identity), Arc::new(tokens));

  assert_eq!(block_on(handle.call("fleet", "count", ValueMap::new())).unwrap(), Value::Int(3));
  assert_eq!(transport.last_metadata("x-sf-subject").as_deref(), Some("alice"));
  assert_eq!(transport.last_metadata("authorization").as_deref(), Some("Bearer secret-abc"));
  assert!(transport.last_metadata("x-sf-request-id").is_some());
}

#[test]
fn an_anonymous_request_attaches_neither() {
  let transport = Arc::new(MockTransport::new().returns("fleet.count", Value::Int(3)));
  let services = Services::builder()
    .contract(contract())
    .intercept(Arc::new(IdentityInterceptor::new()))
    .intercept(Arc::new(CredentialInterceptor::bearer("access_token")))
    .default_transport(transport.clone())
    .build();

  block_on(services.bind_anonymous().call("fleet", "count", ValueMap::new())).unwrap();
  assert_eq!(transport.last_metadata("x-sf-subject"), None);
  assert_eq!(transport.last_metadata("authorization"), None);
}

#[test]
fn an_interceptor_can_short_circuit_the_chain() {
  struct Deny;
  impl snapfire_fsr_service::Interceptor for Deny {
    fn call(
      &self,
      call: snapfire_fsr_service::Call,
      _next: snapfire_fsr_service::Next,
    ) -> futures_util::future::BoxFuture<'static, Result<Value, snapfire_fsr_runtime::ServiceError>> {
      let error = snapfire_fsr_runtime::ServiceError::new(
        FailureKind::Unauthorized,
        &call.service,
        &call.method,
        "no identity on the request",
      );
      Box::pin(async move { Err(error) })
    }
  }

  let transport = Arc::new(MockTransport::new().returns("fleet.count", Value::Int(3)));
  let services = Services::builder()
    .contract(contract())
    .intercept(Arc::new(Deny))
    .default_transport(transport.clone())
    .build();

  let err = block_on(services.bind_anonymous().call("fleet", "count", ValueMap::new())).unwrap_err();
  assert_eq!(err.kind, FailureKind::Unauthorized);
  assert!(transport.calls().is_empty(), "a short circuit never reaches the transport");
}

#[test]
fn a_transport_is_per_service_with_a_fallback() {
  let fleet = Arc::new(MockTransport::new().returns("fleet.count", Value::Int(7)));
  let fallback = Arc::new(MockTransport::new().returns("fleet.count", Value::Int(0)));
  let services = Services::builder()
    .contract(contract())
    .transport("fleet", fleet.clone())
    .default_transport(fallback.clone())
    .build();

  assert_eq!(block_on(services.bind_anonymous().call("fleet", "count", ValueMap::new())).unwrap(), Value::Int(7));
  assert!(fallback.calls().is_empty());
}

#[test]
fn a_local_implementation_is_the_same_machinery() {
  let services = Services::builder()
    .contract(contract())
    .default_transport(Arc::new(LocalTransport::new().method("fleet.get", |call| async move {
      let name = match call.args.get("name") {
        Some(Value::Str(name)) => name.clone(),
        _ => unreachable!("the contract checked this"),
      };
      Ok(server(&name, 0.25))
    })))
    .build();

  let got = block_on(services.bind_anonymous().call("fleet", "get", args(vec![("name", Value::str("db-1"))]))).unwrap();
  assert_eq!(got, server("db-1", 0.25));
}

#[test]
fn a_failing_backend_keeps_its_kind_for_the_ui() {
  let services = Services::builder()
    .contract(contract())
    .default_transport(Arc::new(
      MockTransport::new().fails("fleet.count", FailureKind::Unavailable, "connection refused"),
    ))
    .build();

  let err = block_on(services.bind_anonymous().call("fleet", "count", ValueMap::new())).unwrap_err();
  assert_eq!(err.kind, FailureKind::Unavailable);
  assert_eq!(err.kind.http_status(), 503);
  assert_eq!(err.service, "fleet");
}

#[test]
fn the_handle_is_clonable_into_a_request_ctx() {
  let services = Services::builder()
    .contract(contract())
    .default_transport(Arc::new(MockTransport::new().returns("fleet.count", Value::Int(1))))
    .build();
  let handle: ServiceHandle = services.bind_anonymous();
  let ctx = RequestCtx { services: handle.clone(), ..Default::default() };
  let cloned = ctx.clone();
  assert!(cloned.services.is_bound());
  assert_eq!(block_on(cloned.services.call("fleet", "count", ValueMap::new())).unwrap(), Value::Int(1));
}
