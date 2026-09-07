use std::sync::Arc;

use futures::executor::block_on;
use parking_lot::Mutex;
use snapfire_fsr_core::{Value, ValueMap};
use snapfire_fsr_macros::native;
use snapfire_fsr_runtime::{Native, NativeHandle, Natives};

/// A module the application defines, holding another and calling it as
/// ordinary Rust: `Users` never enters the surface a body can reach.
#[derive(Clone)]
struct Users {
  names: Arc<Mutex<Vec<String>>>,
}

impl Users {
  fn find(&self, id: &str) -> Option<String> {
    self.names.lock().iter().find(|n| n.as_str() == id).cloned()
  }
}

#[derive(Clone)]
struct Rooms {
  users: Users,
  seen: Arc<Mutex<u64>>,
}

#[native]
impl Rooms {
  pub fn tally(&self) -> u64 {
    *self.seen.lock()
  }

  pub async fn greet(&self, who: String, loud: bool) -> String {
    *self.seen.lock() += 1;
    let found = self.users.find(&who).unwrap_or_else(|| "stranger".to_owned());
    match loud {
      true => found.to_uppercase(),
      false => found,
    }
  }

  fn private(&self) -> u64 {
    99
  }
}

fn args(pairs: &[(&str, Value)]) -> ValueMap {
  pairs.iter().map(|(k, v)| ((*k).to_owned(), v.clone())).collect()
}

#[test]
fn a_native_module_answers_by_the_name_a_body_calls_it() {
  let rooms = Rooms {
    users: Users { names: Arc::new(Mutex::new(vec!["alice".to_owned()])) },
    seen: Arc::new(Mutex::new(0)),
  };
  let mut natives = Natives::new();
  natives.register("rooms", Arc::new(rooms) as Arc<dyn Native>);
  let handle = NativeHandle::new(Arc::new(natives));

  let answered = block_on(handle.call("rooms", "greet", args(&[("who", Value::str("alice")), ("loud", Value::Bool(true))]))).unwrap();
  assert_eq!(answered, Value::Str("ALICE".to_owned()), "an async method composes with the module it holds");

  let unknown = block_on(handle.call("rooms", "greet", args(&[("who", Value::str("bob")), ("loud", Value::Bool(false))]))).unwrap();
  assert_eq!(unknown, Value::Str("stranger".to_owned()));

  let tallied = handle.call_sync("rooms", "tally", ValueMap::new()).expect("a method the build read as `fn` answers without a future");
  assert_eq!(tallied.unwrap(), Value::Int(2), "and it sees what the async ones did");
  assert!(handle.call_sync("rooms", "greet", ValueMap::new()).is_none(), "an async method has no sync half");
  assert!(handle.call_sync("rooms", "private", ValueMap::new()).is_none(), "a method that is not `pub` never crosses");

  let missing = block_on(handle.call("rooms", "nothing", ValueMap::new())).unwrap_err();
  assert_eq!(missing.kind, snapfire_fsr_runtime::FailureKind::NotFound);
  let unbound = block_on(NativeHandle::default().call("rooms", "greet", ValueMap::new())).unwrap_err();
  assert_eq!(unbound.kind, snapfire_fsr_runtime::FailureKind::Unavailable);
}

#[test]
fn a_wrong_argument_is_the_callers_error() {
  let rooms = Rooms {
    users: Users { names: Arc::new(Mutex::new(Vec::new())) },
    seen: Arc::new(Mutex::new(0)),
  };
  let mut natives = Natives::new();
  natives.register("rooms", Arc::new(rooms) as Arc<dyn Native>);
  let handle = NativeHandle::new(Arc::new(natives));

  let wrong = block_on(handle.call("rooms", "greet", args(&[("who", Value::Int(7)), ("loud", Value::Bool(false))]))).unwrap_err();
  assert_eq!(wrong.kind, snapfire_fsr_runtime::FailureKind::Invalid, "{wrong}");
}
