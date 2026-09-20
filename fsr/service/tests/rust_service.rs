use std::sync::Arc;

use futures::executor::block_on;
use snapfire_fsr_core::{Value, ValueMap};
use snapfire_fsr_macros::{service, Record};
use snapfire_fsr_runtime::{FailureKind, FromNativeValue, IntoNativeValue, ServiceError};
use snapfire_fsr_service::{Call, DeclaredService, NoCredentials, Scope, Services, Transport, Type, TypeDef};

#[derive(Record, Debug, PartialEq)]
pub struct Server {
  pub name: String,
  pub load_avg: f64,
}

#[derive(Record)]
pub struct Added {
  pub count: u32,
  pub first: Option<Server>,
}

#[derive(Clone, Default)]
pub struct ServerFleet;

#[service]
impl ServerFleet {
  #[cache(ttl = "15s", tags = ["servers"], scope = "shared", stale = "2m")]
  pub async fn list_servers(&self, section: String) -> Result<Vec<Server>, ServiceError> {
    match section.as_str() {
      "down" => Err(ServiceError::new(FailureKind::Unavailable, "server_fleet", "listServers", "unreachable")),
      _ => Ok(vec![Server { name: format!("{section}-1"), load_avg: 0.5 }]),
    }
  }

  pub fn count(&self) -> u32 {
    self.tally()
  }

  #[writes("servers")]
  pub async fn add(&self, server: Server, note: Option<String>) -> Added {
    let _ = note;
    Added { count: 1, first: Some(server) }
  }

  pub fn touch(&self) {}

  fn tally(&self) -> u32 {
    2
  }
}

fn call(method: &str, args: ValueMap) -> Call {
  Call {
    service: "server_fleet".to_owned(),
    method: method.to_owned(),
    args,
    identity: None,
    metadata: ValueMap::default(),
    credentials: Arc::new(NoCredentials),
  }
}

#[test]
fn the_contract_is_read_off_the_signatures() {
  assert_eq!(ServerFleet::NAME, "server_fleet");
  let contract = ServerFleet::contract();
  contract.validate().unwrap();
  let service = &contract.services["server_fleet"];
  assert_eq!(service.methods.keys().collect::<Vec<_>>(), vec!["listServers", "count", "add", "touch"]);
  let list = &service.methods["listServers"];
  assert_eq!(list.params.len(), 1);
  assert_eq!((list.params[0].name.as_str(), &list.params[0].ty), ("section", &Type::Str));
  assert_eq!(list.returns, Type::list(Type::named("Server")));
  let cache = list.cache.as_ref().unwrap();
  assert_eq!((cache.ttl.as_str(), cache.tags.as_slice(), cache.scope, cache.stale.as_deref()), ("15s", &["servers".to_owned()][..], Scope::Shared, Some("2m")));
  assert_eq!(service.methods["count"].returns, Type::U32);
  let add = &service.methods["add"];
  assert_eq!(add.params[0].ty, Type::named("Server"));
  assert_eq!(add.params[1].ty, Type::optional(Type::Str));
  assert_eq!(add.writes, vec!["servers".to_owned()]);
  assert_eq!(service.methods["touch"].returns, Type::Null);
  let TypeDef::Record { fields } = &contract.types["Server"] else { panic!("a record") };
  assert_eq!(fields.iter().map(|f| (f.name.as_str(), f.ty.clone())).collect::<Vec<_>>(), vec![("name", Type::Str), ("loadAvg", Type::F64)]);
  let TypeDef::Record { fields } = &contract.types["Added"] else { panic!("a record") };
  assert_eq!(fields[1].ty, Type::optional(Type::named("Server")));
}

#[test]
fn a_call_is_answered_by_the_method_it_names() {
  let fleet = ServerFleet;
  let mut args = ValueMap::default();
  args.insert("section".to_owned(), Value::str("web"));
  let Value::Seq(servers) = block_on(fleet.call(call("listServers", args))).unwrap() else { panic!("a list") };
  assert_eq!(Vec::<Server>::from_native_value(Value::Seq(servers)).unwrap(), vec![Server { name: "web-1".to_owned(), load_avg: 0.5 }]);
  assert_eq!(block_on(fleet.call(call("count", ValueMap::default()))).unwrap(), Value::Int(2));
  assert_eq!(block_on(fleet.call(call("touch", ValueMap::default()))).unwrap(), Value::Null);

  let mut args = ValueMap::default();
  args.insert("server".to_owned(), Server { name: "db-1".to_owned(), load_avg: 0.1 }.into_native_value());
  let Value::Map(added) = block_on(fleet.call(call("add", args))).unwrap() else { panic!("a map") };
  assert_eq!(added.get("count"), Some(&Value::Int(1)));
  assert!(matches!(added.get("first"), Some(Value::Map(first)) if first.get("name") == Some(&Value::str("db-1"))));
}

#[test]
fn a_failure_travels_as_the_calls_error() {
  let fleet = ServerFleet;
  let mut args = ValueMap::default();
  args.insert("section".to_owned(), Value::str("down"));
  let err = block_on(fleet.call(call("listServers", args))).unwrap_err();
  assert_eq!((err.kind, err.message.as_str()), (FailureKind::Unavailable, "unreachable"));

  let err = block_on(fleet.call(call("purge", ValueMap::default()))).unwrap_err();
  assert_eq!((err.kind, err.service.as_str(), err.method.as_str()), (FailureKind::NotFound, "server_fleet", "purge"));

  let mut wrong = ValueMap::default();
  wrong.insert("section".to_owned(), Value::Int(1));
  let err = block_on(fleet.call(call("listServers", wrong))).unwrap_err();
  assert_eq!((err.kind, err.service.as_str(), err.method.as_str()), (FailureKind::Invalid, "server_fleet", "listServers"));
}

#[test]
fn the_registry_checks_a_call_against_the_written_contract() {
  let services = Services::builder()
    .contract(ServerFleet::contract())
    .transport(ServerFleet::NAME, Arc::new(ServerFleet))
    .build();
  let handle = services.bind_anonymous();
  let mut args = ValueMap::default();
  args.insert("section".to_owned(), Value::str("web"));
  assert!(matches!(block_on(handle.call("server_fleet", "listServers", args)).unwrap(), Value::Seq(_)));
  let err = block_on(handle.call("server_fleet", "listServers", ValueMap::default())).unwrap_err();
  assert_eq!(err.kind, FailureKind::Invalid);
}
