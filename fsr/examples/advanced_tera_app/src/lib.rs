pub mod actions;
pub mod loaders;
pub mod routes;
pub mod services;
pub mod state;

use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use snapfire_fsr_core::{ModuleId, Value};
use snapfire_fsr_host::{Config, Host, HostBuilder, HostError};
use snapfire_fsr_tera::TeraEvaluator;

/// No plan file: every route, source and action is bound in Rust.
const EMPTY_PLAN: &str = r#"{"version":2,"routes":[]}"#;

fn templates() -> tera::Tera {
  let mut tera = tera::Tera::new();
  snapfire_fsr_tera::register_markers(&mut tera);
  tera
    .add_raw_templates([
      ("layout.tera", include_str!("../templates/layout.tera")),
      ("page.tera", include_str!("../templates/page.tera")),
      ("stream_page.tera", include_str!("../templates/stream_page.tera")),
      ("chart_section.tera", include_str!("../templates/chart_section.tera")),
      ("chart_loading.tera", include_str!("../templates/chart_loading.tera")),
      ("error_section.tera", include_str!("../templates/error_section.tera")),
      ("login.tera", include_str!("../templates/login.tera")),
      ("index.tera", include_str!("../templates/index.tera")),
    ])
    .expect("templates parse");
  tera
}

/// The stock host over `config/`, with the fleet, the loaders, the action and
/// the Tera evaluator bound in Rust. `chart_delay` is how long the deferred
/// chart takes, so a test can make it instant.
pub fn builder(chart_delay: Duration) -> Result<HostBuilder, HostError> {
  let fleet = state::Fleet::seed();
  let renders = state::Renders::default();
  let config = Config::load(Path::new(env!("CARGO_MANIFEST_DIR")).join("config"))?;
  let counting = renders.clone();
  let builder = Host::from_config_with(config, EMPTY_PLAN.to_owned(), None)?
    .services(services::build(fleet))
    .evaluator(|m: &ModuleId| m.path.ends_with(".tera"), Arc::new(TeraEvaluator::new(templates())))
    .route("/dash/{section}", routes::dash_plan())
    .route("/slow/{section}", routes::slow_plan())
    .route("/login", routes::login_plan())
    .route("/", routes::index_plan())
    .middleware(move |ctx, request| {
      let renders = counting.clone();
      async move {
        let (page, payload) = match &request {
          Value::Map(line) => (
            !matches!(line.get("path"), Some(Value::Str(path)) if path.starts_with("/_sf/")) && line.get("method") == Some(&Value::str("GET")),
            line.get("payload") == Some(&Value::Bool(true)),
          ),
          _ => (false, false),
        };
        if page {
          renders.next();
          if !payload {
            let visits = match ctx.session.get("visits") {
              Some(Value::Int(n)) => n + 1,
              _ => 1,
            };
            ctx.session.insert("visits", Value::Int(visits));
          }
        }
        Ok(Value::Null)
      }
    });
  let builder = loaders::register(builder, chart_delay, renders);
  Ok(actions::register(builder))
}

pub fn build(chart_delay: Duration) -> Result<Host, HostError> {
  builder(chart_delay)?.build()
}
