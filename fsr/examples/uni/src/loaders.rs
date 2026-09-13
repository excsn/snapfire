use snapfire_fsr_core::{Value, ValueMap};
use snapfire_fsr_host::HostBuilder;

use crate::session::{bought, lot, watched};
use crate::state::{self, Tape, Ticks};

/// What the masthead island is rendered with. Every price rides along, so the
/// masthead shows whichever symbol the store moves to without asking the
/// server again; `symbols` carries the desk's own order for the arrows to
/// step, which a map has none of.
fn watch_props(symbol: &str, tick: usize) -> Value {
  let mut quotes = ValueMap::default();
  for holding in state::HOLDINGS {
    let mut quote = ValueMap::default();
    quote.insert("price".to_owned(), Value::F64(state::price_at(holding, tick)));
    quote.insert("change".to_owned(), Value::F64(holding.change));
    quote.insert("trail".to_owned(), Value::seq(state::trail(holding).into_iter().map(Value::F64).collect::<Vec<_>>()));
    quotes.insert(holding.symbol.to_owned(), Value::Map(quote));
  }
  let mut map = ValueMap::default();
  map.insert("symbol".to_owned(), Value::str(symbol));
  map.insert("symbols".to_owned(), Value::seq(state::HOLDINGS.iter().map(|h| Value::str(h.symbol)).collect::<Vec<_>>()));
  map.insert("quotes".to_owned(), Value::Map(quotes));
  Value::Map(map)
}

pub fn register(builder: HostBuilder, tape: Tape, ticks: Ticks) -> HostBuilder {
  builder
    .source("layout_loader", {
      let ticks = ticks.clone();
      move |ctx| {
        let ticks = ticks.clone();
        async move {
      let symbol = watched(&ctx);
      let size = lot(&ctx);
      let Value::Map(mut watch) = watch_props(&symbol, ticks.now()) else { unreachable!("watch props are a map") };
      watch.insert("owned".to_owned(), Value::Int(bought(&ctx, &symbol) as i128));
      watch.insert("lot".to_owned(), Value::Int(size as i128));
      let mut stepper = ValueMap::default();
      stepper.insert("size".to_owned(), Value::Int(size as i128));
      let mut data = ValueMap::default();
      data.insert("watch".to_owned(), Value::Map(watch));
      data.insert("lot".to_owned(), Value::Map(stepper));
      Ok(data)
        }
      }
    })
    .source("board_loader", {
      let ticks = ticks.clone();
      move |ctx| {
        let ticks = ticks.clone();
        async move {
      let tick = ticks.now();
      let rows = state::HOLDINGS
        .iter()
        .map(|holding| {
          let mut row = match state::as_value(holding) {
            Value::Map(map) => map,
            _ => ValueMap::default(),
          };
          row.insert("shares".to_owned(), Value::Int((holding.shares + bought(&ctx, holding.symbol)) as i128));
          row.insert("price".to_owned(), Value::F64(state::price_at(holding, tick)));
          Value::Map(row)
        })
        .collect::<Vec<_>>();
      let mut desk = ValueMap::default();
      desk.insert("holdings".to_owned(), Value::seq(rows));
      desk.insert("watched".to_owned(), Value::str(watched(&ctx)));
      let mut data = ValueMap::default();
      data.insert("desk".to_owned(), Value::Map(desk));
      Ok(data)
        }
      }
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
        data.insert("headlines".to_owned(), state::headlines_value(tape.at()));
        Ok(data)
      }
    })
}
