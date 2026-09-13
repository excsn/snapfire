//! `uni`: one page holding a React island, a Vue island and an htmx region,
//! under a Tera layout, from one payload.

use std::path::Path;
use std::sync::Arc;

use snapfire_fsr_core::ModuleId;
use snapfire_fsr_host::{Config, Host, HostBuilder, HostError};
use snapfire_fsr_tera::TeraEvaluator;

mod loaders;
mod routes;
pub mod state;

/// A plan file the host reads as empty: every route here is registered in
/// Rust, which is what the Tera tier is.
const EMPTY_PLAN: &str = r#"{ "version": 2, "routes": [], "sources": [], "actions": [] }"#;

fn templates() -> tera::Tera {
  let mut tera = tera::Tera::new();
  snapfire_fsr_tera::register_markers(&mut tera);
  tera
    .add_raw_templates(vec![
      ("layout.tera", include_str!("../templates/layout.tera")),
      ("board.tera", include_str!("../templates/board.tera")),
      ("news.tera", include_str!("../templates/news.tera")),
      ("tape.tera", include_str!("../templates/tape.tera")),
    ])
    .expect("the templates parse");
  tera
}

pub fn builder() -> Result<HostBuilder, HostError> {
  let config = Config::load(Path::new(env!("CARGO_MANIFEST_DIR")).join("config"))?;
  let builder = Host::from_config_with(config, EMPTY_PLAN.to_owned(), None)?
    .evaluator(|m: &ModuleId| m.path.ends_with(".tera"), Arc::new(TeraEvaluator::new(templates())))
    .route("/board", routes::board_plan())
    .route("/news", routes::news_plan())
    .route("/", routes::board_plan());
  Ok(loaders::register(builder, state::Tape::default()))
}

pub fn build() -> Result<Host, HostError> {
  builder()?.build()
}
