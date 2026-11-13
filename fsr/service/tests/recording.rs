//! `Recording` keeps every call that reaches the transport it wraps, in order, and answers with that transport's answer.

use futures::executor::block_on;
use snapfire_fsr_core::{Value, ValueMap};
use snapfire_fsr_runtime::FailureKind;
use snapfire_fsr_service::{Call, MockTransport, Recording, Transport};

#[test]
fn a_recording_keeps_each_call_in_order_and_passes_the_answer_through() {
  let recording = Recording::over(MockTransport::new().returns("fleet.count", Value::Int(3)).fails("fleet.restart", FailureKind::Unavailable, "down"));
  let mut args = ValueMap::default();
  args.insert("region".to_owned(), Value::str("eu"));
  let mut call = Call::new("fleet", "count", args.clone());
  call.metadata.insert("authorization".to_owned(), Value::str("Bearer t"));
  assert_eq!(block_on(recording.call(call)).unwrap(), Value::Int(3));
  assert!(block_on(recording.call(Call::new("fleet", "restart", ValueMap::default()))).is_err());
  let calls = recording.calls();
  assert_eq!(calls.iter().map(|(path, ..)| path.as_str()).collect::<Vec<_>>(), ["fleet.count", "fleet.restart"]);
  assert_eq!(calls[0].1, args);
  assert_eq!(recording.last_metadata("authorization"), None, "the last call carried none");
  assert_eq!(calls[0].2.get("authorization"), Some(&Value::str("Bearer t")));
}
