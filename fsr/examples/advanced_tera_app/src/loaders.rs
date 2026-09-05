use std::sync::Arc;
use std::time::Duration;

use futures_util::future::BoxFuture;
use snapfire_fsr_core::{Data, TypedArray, Value, ValueMap};
use snapfire_fsr_host::HostBuilder;
use snapfire_fsr_runtime::{LoadError, Meta, Metadata, RequestCtx};

use crate::services::fleet;
use crate::state::Renders;

fn series(points: Vec<f64>) -> Value {
  let mut map = ValueMap::new();
  map.insert("series".to_owned(), Value::TypedArray(TypedArray::F64(points)));
  Value::Map(map)
}

async fn fetch_servers(ctx: &RequestCtx) -> Result<Value, snapfire_fsr_runtime::ServiceError> {
  let mut args = ValueMap::new();
  args.insert("section".to_owned(), Value::Str(ctx.params.get("section").cloned().unwrap_or_default()));
  ctx.services.call(fleet::NAME, fleet::LIST, args).await
}

/// The document's title for a section route: the section and the fleet's
/// size, asked of a capability the failing list never touches. A route with
/// no section keeps the configured title.
struct SectionTitle;

impl Metadata for SectionTitle {
  fn describe(&self, ctx: &RequestCtx, _data: &Data) -> BoxFuture<'static, Result<Meta, LoadError>> {
    let section = ctx.params.get("section").cloned();
    let services = ctx.services.clone();
    Box::pin(async move {
      let Some(section) = section else { return Ok(Meta::default()) };
      let count = match services.call(fleet::NAME, fleet::COUNT, ValueMap::new()).await {
        Ok(Value::Int(count)) => count,
        _ => 0,
      };
      Ok(Meta { title: Some(format!("{section} ({count} servers) - SnapFire FSR")), description: None })
    })
  }
}

pub fn register(builder: HostBuilder, chart_delay: Duration, renders: Renders) -> HostBuilder {
  let chrome_renders = renders.clone();
  let layout_renders = renders.clone();
  let hydrate_renders = renders.clone();
  let page_renders = renders;
  builder
    .source("chrome_loader", move |_ctx| {
      let renders = chrome_renders.clone();
      async move {
        let mut data = ValueMap::new();
        data.insert("renders".to_owned(), Value::int(renders.get() as i64));
        Ok(data)
      }
    })
    .source("layout_loader", move |ctx| {
      let renders = layout_renders.clone();
      async move {
        let visits = match ctx.session.get("visits") {
          Some(Value::Int(n)) => n,
          _ => 0,
        };
        let mut data = ValueMap::new();
        data.insert("nav_label".to_owned(), Value::str("SnapFire FSR"));
        data.insert("visits".to_owned(), Value::Int(visits));
        data.insert("renders".to_owned(), Value::int(renders.get() as i64));
        Ok(data)
      }
    })
    .meta("layout_loader", Arc::new(SectionTitle))
    .source("hydrate_loader", move |_ctx| {
      let renders = hydrate_renders.clone();
      async move {
        let mut data = ValueMap::new();
        data.insert("renders".to_owned(), Value::int(renders.get() as i64));
        for when in ["load", "visible", "idle"] {
          let mut stamp = ValueMap::new();
          stamp.insert("when".to_owned(), Value::str(when));
          data.insert(format!("stamp_{when}"), Value::Map(stamp));
        }
        Ok(data)
      }
    })
    .source("servers_loader", move |ctx| {
      let renders = page_renders.clone();
      async move {
        let servers = fetch_servers(&ctx).await.map_err(|e| LoadError { source_id: "servers_loader".into(), message: e.message })?;
        let mut data = ValueMap::new();
        data.insert("servers".to_owned(), servers);
        data.insert("chart".to_owned(), series(vec![12.0, 15.5, 9.25]));
        data.insert("renders".to_owned(), Value::int(renders.get() as i64));
        Ok(data)
      }
    })
    .source("slow_chart_loader", move |_ctx| async move {
      if !chart_delay.is_zero() {
        tokio::time::sleep(chart_delay).await;
      }
      let mut data = ValueMap::new();
      data.insert("latency".to_owned(), series(vec![4.25, 7.5, 3.75, 6.5]));
      Ok(data)
    })
}
