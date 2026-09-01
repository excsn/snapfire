use std::time::Duration;

use snapfire_fsr_core::{TypedArray, Value, ValueMap};
use snapfire_fsr_runtime::DataSources;

use crate::state::Fleet;

fn server(name: &str, load: f64) -> Value {
  let mut map = ValueMap::new();
  map.insert("name".to_owned(), Value::str(name));
  map.insert("load".to_owned(), Value::F64(load));
  Value::Map(map)
}

fn series(points: Vec<f64>) -> Value {
  let mut map = ValueMap::new();
  map.insert("series".to_owned(), Value::TypedArray(TypedArray::F64(points)));
  Value::Map(map)
}

pub fn register(sources: &mut DataSources, fleet: Fleet, chart_delay: Duration) {
  let meta_fleet = fleet.clone();
  sources.insert_fn("meta_loader", move |ctx| {
    let fleet = meta_fleet.clone();
    async move {
      let section = ctx.params.get("section").cloned().unwrap_or_default();
      let mut data = ValueMap::new();
      data.insert(
        "title".to_owned(),
        Value::Str(format!("{section} ({} servers) - Snapfire FSR", fleet.list().len())),
      );
      Ok(data)
    }
  });

  sources.insert_fn("layout_loader", |_ctx| async {
    let mut data = ValueMap::new();
    data.insert("nav_label".to_owned(), Value::str("Snapfire FSR"));
    Ok(data)
  });

  sources.insert_fn("servers_loader", move |ctx| {
    let fleet = fleet.clone();
    async move {
      if ctx.params.get("section").map(String::as_str) == Some("down") {
        return Err(snapfire_fsr_runtime::LoadError {
          source_id: "servers_loader".into(),
          message: "the servers backend is unreachable".into(),
        });
      }
      let mut data = ValueMap::new();
      let servers = fleet.list().iter().map(|(name, load)| server(name, *load)).collect();
      data.insert("servers".to_owned(), Value::Seq(servers));
      data.insert("chart".to_owned(), series(vec![12.0, 15.5, 9.25]));
      Ok(data)
    }
  });

  sources.insert_fn("slow_chart_loader", move |_ctx| async move {
    if !chart_delay.is_zero() {
      actix_web::rt::time::sleep(chart_delay).await;
    }
    let mut data = ValueMap::new();
    data.insert("latency".to_owned(), series(vec![4.25, 7.5, 3.75, 6.5]));
    Ok(data)
  });
}
