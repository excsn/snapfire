//! `uni`: one page holding a React island, a Vue island and an htmx region,
//! under a Tera layout, from one payload.

use std::path::Path;
use std::sync::Arc;

use snapfire_fsr_core::ModuleId;
use snapfire_fsr_host::{Config, Host, HostBuilder, HostError};
use snapfire_fsr_tera::TeraEvaluator;

mod actions;
mod loaders;
mod routes;
mod session;
pub mod state;

/// The plan `build.rs` writes: no routes, since every route here is registered
/// in Rust, and one lowered component, the server-mode island the host renders
/// itself. The two tiers meet in this file.
const PLAN: &str = include_str!(concat!(env!("OUT_DIR"), "/plan.sexp"));

fn templates() -> tera::Tera {
  let mut tera = tera::Tera::new();
  snapfire_fsr_tera::register_markers(&mut tera);
  tera
    .add_raw_templates(vec![
      ("document.tera", include_str!("../templates/document.tera")),
      ("layout.tera", include_str!("../templates/layout.tera")),
      ("board.tera", include_str!("../templates/board.tera")),
      ("news.tera", include_str!("../templates/news.tera")),
      ("tape.tera", include_str!("../templates/tape.tera")),
    ])
    .expect("the templates parse");
  tera
}

pub fn builder(ticks: state::Ticks, tape: state::Tape) -> Result<HostBuilder, HostError> {
  let config = Config::load(Path::new(env!("CARGO_MANIFEST_DIR")).join("config"))?;
  let builder = Host::from_config_with(config, PLAN.to_owned(), None)?
    .evaluator(|m: &ModuleId| m.path.ends_with(".tera"), Arc::new(TeraEvaluator::new(templates())))
    .route("/board", routes::board_plan())
    .route("/news", routes::news_plan())
    .route("/", routes::board_plan());
  Ok(actions::register(loaders::register(builder, tape, ticks)))
}

pub fn build() -> Result<Host, HostError> {
  builder(state::Ticks::default(), state::Tape::default())?.build()
}
