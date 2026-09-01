pub mod actions;
pub mod http;
pub mod loaders;
pub mod render;
pub mod routes;
pub mod state;

use std::sync::Arc;
use std::time::Duration;

use snapfire_fsr_core::ModuleId;
use snapfire_fsr_runtime::{
  ActionRegistry, DataSources, Evaluators, FibreCache, MatchitMatcher, Runtime, TableResolver,
};
use snapfire_fsr_tera::TeraEvaluator;

pub use render::{call_action, negotiate_encoding, render, respond, AppError, RenderMode};

pub struct AppCore {
  pub(crate) matcher: MatchitMatcher,
  pub(crate) resolver: TableResolver,
  pub(crate) runtime: Arc<Runtime>,
  pub(crate) actions: ActionRegistry,
}

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
    ])
    .expect("templates parse");
  tera
}

pub fn build_app(chart_delay: Duration) -> AppCore {
  let fleet = state::Fleet::seed();

  let mut sources = DataSources::new();
  loaders::register(&mut sources, fleet.clone(), chart_delay);

  let mut evaluators = Evaluators::new();
  evaluators.register(
    |m: &ModuleId| m.path.ends_with(".tera"),
    Arc::new(TeraEvaluator::new(templates())),
  );

  let mut action_registry = ActionRegistry::new();
  actions::register(&mut action_registry, fleet);

  AppCore {
    matcher: routes::matcher(),
    resolver: routes::resolver(),
    runtime: Runtime::builder()
      .sources(sources)
      .evaluators(evaluators)
      .cache(Arc::new(FibreCache::bounded(1024, Duration::from_secs(300))))
      .build(),
    actions: action_registry,
  }
}
