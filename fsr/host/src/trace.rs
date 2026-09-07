//! Installing the trace collector, and serving what it kept.
//!
//! Every application wants the same three lines, so they live here rather than
//! in each `main`. `install` is for a host that logs through `tracing`'s
//! default; `install_with` composes beside a layer that already owns the
//! events, which is what an application using `fibre_logging` needs.

use tracing_subscriber::layer::SubscriberExt;

pub use fibre_tracing::{Span, Trace, Traces};

/// The collector, set as the global subscriber. Returns the handle to read it
/// through. Answers `None` when a subscriber is already set, which is not an
/// error: it means something else owns the dispatcher and no trace is kept.
pub fn install() -> Option<Traces> {
  let (layer, traces) = fibre_tracing::layer();
  match tracing::subscriber::set_global_default(tracing_subscriber::registry().with(layer)) {
    Ok(()) => Some(traces),
    Err(_) => None,
  }
}

/// `install` beside a layer that is already handling the events. Only the
/// subscriber is set, never the `log` bridge, since whoever built that layer
/// has usually taken it.
pub fn install_with<L>(other: L) -> Option<Traces>
where
  L: tracing_subscriber::Layer<tracing_subscriber::Registry> + Send + Sync + 'static,
{
  let (layer, traces) = fibre_tracing::layer();
  let composed = tracing_subscriber::registry().with(other).with(layer);
  match tracing::subscriber::set_global_default(composed) {
    Ok(()) => Some(traces),
    Err(_) => None,
  }
}

/// The pair every application wants: `fibre_logging` taking the events out to
/// its appenders and `fibre_tracing` keeping the spans, composed on one
/// registry. The guard has to be held, since its `Drop` flushes the appenders.
///
/// A missing or unreadable configuration is not fatal. The collector is
/// installed alone and the reason is returned, so an example still traces.
pub fn observe(config: &std::path::Path) -> (Option<Traces>, Option<fibre_logging::InitResult>, Option<String>) {
  match fibre_logging::init::layer_from_file(config) {
    Ok((logging, guard)) => (install_with(logging), Some(guard), None),
    Err(e) => (install(), None, Some(e.to_string())),
  }
}

/// One trace as the value a route answers with: the spans in the order they
/// opened, each with where it sits, when it started and how long it was open.
pub fn to_value(traces: &[Trace]) -> snapfire_fsr_core::Value {
  use snapfire_fsr_core::{Value, ValueMap};
  let ms = |d: std::time::Duration| Value::F64((d.as_secs_f64() * 1000.0 * 1000.0).round() / 1000.0);
  Value::Seq(
    traces
      .iter()
      .map(|trace| {
        let mut out = ValueMap::new();
        out.insert("id".to_owned(), Value::Int(trace.id as i128));
        out.insert("ms".to_owned(), ms(trace.duration));
        out.insert(
          "spans".to_owned(),
          Value::Seq(
            trace
              .spans
              .iter()
              .map(|span| {
                let mut row = ValueMap::new();
                row.insert("name".to_owned(), Value::Str(span.name.to_owned()));
                row.insert("depth".to_owned(), Value::Int(span.depth as i128));
                row.insert("at".to_owned(), ms(span.at));
                row.insert("ms".to_owned(), ms(span.duration));
                if let Some(outcome) = span.outcome() {
                  row.insert("outcome".to_owned(), Value::Str(outcome.to_owned()));
                }
                let mut fields = ValueMap::new();
                for (key, value) in &span.fields {
                  if !key.starts_with("fibre.") {
                    fields.insert(key.clone(), Value::Str(value.to_string()));
                  }
                }
                row.insert("fields".to_owned(), Value::Map(fields));
                Value::Map(row)
              })
              .collect(),
          ),
        );
        Value::Map(out)
      })
      .collect(),
  )
}

/// `Server-Timing` for one trace: an entry per span, so per-step cost shows in
/// devtools with no collector running. Under `fsr dev` only; nothing about a
/// source should reach a client in production.
pub fn server_timing(trace: &Trace) -> String {
  trace
    .spans
    .iter()
    .skip(1)
    .enumerate()
    .map(|(i, span)| {
      let name = format!("{}{}", span.name, i);
      match span.fields.get("id").or_else(|| span.fields.get("module")).or_else(|| span.fields.get("method")) {
        Some(what) => format!("{name};dur={:.1};desc=\"{}\"", span.duration.as_secs_f64() * 1000.0, what),
        None => format!("{name};dur={:.1}", span.duration.as_secs_f64() * 1000.0),
      }
    })
    .collect::<Vec<_>>()
    .join(", ")
}
