use std::sync::Arc;
use std::time::Duration;

use snapfire_fsr_service::{LocalTransport, Transport};

/// The field's three systems, in process and slow on purpose. `arrivals` is
/// the board itself and answers at once; the weather takes `pause` and the
/// gate system, which is the oldest thing on the field, takes twice that.
/// Their data is the same file `[clients.board]` mocks for the dev loop.
pub fn board(pause: Duration) -> Arc<dyn Transport> {
  let answers = answers();
  let arrivals = answers.0;
  let weather = answers.1;
  let gates = answers.2;
  let transport = LocalTransport::new()
    .method("board.listArrivals", move |_| {
      let value = arrivals.clone();
      async move { Ok(value) }
    })
    .method("board.getWeather", move |_| {
      let value = weather.clone();
      async move {
        tokio::time::sleep(pause).await;
        Ok(value)
      }
    })
    .method("board.listGateChanges", move |_| {
      let value = gates.clone();
      async move {
        tokio::time::sleep(pause * 2).await;
        Ok(value)
      }
    });
  Arc::new(transport)
}

/// The mock file, read once: one source for the dev loop, the specs and the
/// running board.
fn answers() -> (snapfire_fsr_core::Value, snapfire_fsr_core::Value, snapfire_fsr_core::Value) {
  const MOCK: &str = include_str!("../app/clients/board.mock.json");
  let json: serde_json::Value = serde_json::from_str(MOCK).expect("board.mock.json");
  let of = |name: &str| snapfire_fsr_payload::json_to_value(&json[name]).expect("board.mock.json");
  (of("listArrivals"), of("getWeather"), of("listGateChanges"))
}
