//! The fleet card: an island whose markup is a Tera template and whose one
//! handler is this file. Nothing about it is compiled for the browser.

use snapfire_fsr_core::{Value, ValueMap};
use snapfire_fsr_host::HostBuilder;
use snapfire_fsr_runtime::{ActionError, FailureKind, IslandEvent, RequestCtx};

use snapfire_fsr_service::DeclaredService;

use crate::state::Fleet;

pub const MODULE: &str = "fleet.tera#default";

/// The filters the card offers. A step naming anything else is refused the
/// way an action refuses an input it does not know.
const FILTERS: [&str; 3] = ["all", "busy", "quiet"];

fn load_of(server: &Value) -> f64 {
  match server {
    Value::Map(map) => match map.get("load") {
      Some(Value::F64(load)) => *load,
      Some(Value::Int(load)) => *load as f64,
      _ => 0.0,
    },
    _ => 0.0,
  }
}

/// The fleet as the service holds it now, not as the browser last saw it: a
/// step is a render on the server, so it reads what a page render would.
async fn servers(ctx: &RequestCtx) -> Result<Vec<Value>, ActionError> {
  let mut args = ValueMap::default();
  args.insert("section".to_owned(), Value::str(""));
  match ctx.services.call(Fleet::NAME, "list", args).await {
    Ok(Value::Seq(rows)) => Ok(rows.to_vec()),
    Ok(_) => Ok(Vec::new()),
    Err(e) => Err(ActionError::new(FailureKind::Unavailable, e.message)),
  }
}

pub fn register(builder: HostBuilder) -> HostBuilder {
  builder.island_handler(MODULE, "filter", |ctx, event: IslandEvent| async move {
    let chosen = match &event.event {
      Value::Map(map) => match map.get("target") {
        Some(Value::Map(target)) => match target.get("value") {
          Some(Value::Str(value)) => value.to_string(),
          _ => String::new(),
        },
        _ => String::new(),
      },
      _ => String::new(),
    };
    if !FILTERS.contains(&chosen.as_str()) {
      return Err(ActionError::new(FailureKind::Invalid, format!("`{chosen}` is not one of the fleet's filters")));
    }
    let held = servers(&ctx).await?;
    let shown: Vec<Value> = held
      .into_iter()
      .filter(|server| match chosen.as_str() {
        "busy" => load_of(server) >= 0.5,
        "quiet" => load_of(server) < 0.5,
        _ => true,
      })
      .collect();
    let mut state = ValueMap::default();
    state.insert("filter".to_owned(), Value::str(chosen));
    state.insert("servers".to_owned(), Value::seq(shown));
    Ok(Value::Map(state))
  })
}
