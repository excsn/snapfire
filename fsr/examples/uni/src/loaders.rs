use snapfire_fsr_core::{Value, ValueMap};
use snapfire_fsr_host::HostBuilder;

use crate::state::{self, Tape};

/// What the masthead island is rendered with: the symbol the session is
/// watching and the price beside it, so the React island renders on the
/// server with the value its store will hold.
fn watch_props(symbol: &str) -> Value {
  let mut map = ValueMap::default();
  map.insert("symbol".to_owned(), Value::str(symbol));
  let price = state::holding(symbol).map(|h| h.price).unwrap_or_default();
  map.insert("price".to_owned(), Value::F64(price));
  Value::Map(map)
}

fn watched(ctx: &snapfire_fsr_runtime::RequestCtx) -> String {
  match ctx.session.get("watched") {
    Some(Value::Str(symbol)) => symbol.to_string(),
    _ => state::HOLDINGS[0].symbol.to_owned(),
  }
}

pub fn register(builder: HostBuilder, tape: Tape) -> HostBuilder {
  builder
    .source("layout_loader", move |ctx| async move {
      let mut data = ValueMap::default();
      data.insert("watch".to_owned(), watch_props(&watched(&ctx)));
      Ok(data)
    })
    .source("board_loader", move |ctx| async move {
      let mut desk = ValueMap::default();
      desk.insert("holdings".to_owned(), state::holdings_value());
      desk.insert("watched".to_owned(), Value::str(watched(&ctx)));
      let mut data = ValueMap::default();
      data.insert("desk".to_owned(), Value::Map(desk));
      Ok(data)
    })
    .source("news_loader", move |_ctx| async move {
      let mut feed = ValueMap::default();
      feed.insert("headlines".to_owned(), state::headlines_value(0));
      let mut data = ValueMap::default();
      data.insert("feed".to_owned(), Value::Map(feed));
      Ok(data)
    })
    .source("tape_loader", move |_ctx| {
      let tape = tape.clone();
      async move {
        let mut data = ValueMap::default();
        data.insert("headlines".to_owned(), state::headlines_value(tape.next()));
        Ok(data)
      }
    })
}
