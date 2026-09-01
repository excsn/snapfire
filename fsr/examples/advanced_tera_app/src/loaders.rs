use std::time::Duration;

use snapfire_fsr_core::{TypedArray, Value, ValueMap};
use snapfire_fsr_runtime::{DataSources, LoadError};

use crate::services::fleet;
use crate::state::Renders;

fn series(points: Vec<f64>) -> Value {
  let mut map = ValueMap::new();
  map.insert("series".to_owned(), Value::TypedArray(TypedArray::F64(points)));
  Value::Map(map)
}

async fn fetch_servers(
  ctx: &snapfire_fsr_runtime::RequestCtx,
) -> Result<Value, snapfire_fsr_runtime::ServiceError> {
  let mut args = ValueMap::new();
  args.insert(
    "section".to_owned(),
    Value::Str(ctx.params.get("section").cloned().unwrap_or_default()),
  );
  ctx.services.call(fleet::NAME, fleet::LIST, args).await
}

pub fn register(sources: &mut DataSources, chart_delay: Duration, renders: Renders) {
  let chrome_renders = renders.clone();
  sources.insert_fn("chrome_loader", move |_ctx| {
    let renders = chrome_renders.clone();
    async move {
      let mut data = ValueMap::new();
      data.insert("renders".to_owned(), Value::int(renders.get() as i64));
      Ok(data)
    }
  });

  sources.insert_fn("meta_loader", move |ctx| async move {
    let section = ctx.params.get("section").cloned().unwrap_or_default();
    let count = match ctx.services.call(fleet::NAME, fleet::COUNT, ValueMap::new()).await {
      Ok(Value::Int(count)) => count,
      _ => 0,
    };
    let mut data = ValueMap::new();
    data.insert("title".to_owned(), Value::Str(format!("{section} ({count} servers) - Snapfire FSR")));
    Ok(data)
  });

  let layout_renders = renders.clone();
  sources.insert_fn("layout_loader", move |ctx| {
    let renders = layout_renders.clone();
    async move {
    let visits = match ctx.session.get("visits") {
      Some(Value::Int(n)) => n,
      _ => 0,
    };
    let mut data = ValueMap::new();
    data.insert("nav_label".to_owned(), Value::str("Snapfire FSR"));
    data.insert("visits".to_owned(), Value::Int(visits));
    data.insert("renders".to_owned(), Value::int(renders.get() as i64));
    Ok(data)
    }
  });

  let page_renders = renders;
  sources.insert_fn("servers_loader", move |ctx| {
    let renders = page_renders.clone();
    async move {
    let servers = fetch_servers(&ctx).await.map_err(|e| LoadError {
      source_id: "servers_loader".into(),
      message: e.message,
    })?;
    let mut data = ValueMap::new();
    data.insert("servers".to_owned(), servers);
    data.insert("chart".to_owned(), series(vec![12.0, 15.5, 9.25]));
    data.insert("renders".to_owned(), Value::int(renders.get() as i64));
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
