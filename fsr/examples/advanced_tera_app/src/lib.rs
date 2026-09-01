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
use snapfire_fsr_session::{MemorySessionStore, SessionConfig, Sessions};
use snapfire_fsr_tera::TeraEvaluator;

pub use render::{call_action, negotiate_encoding, render, respond, respond_with, AppError, RenderMode};

pub struct AppCore {
  pub(crate) matcher: MatchitMatcher,
  pub(crate) resolver: TableResolver,
  pub(crate) runtime: Arc<Runtime>,
  pub(crate) actions: ActionRegistry,
  pub(crate) sessions: Sessions,
}

impl AppCore {
  pub fn sessions(&self) -> &Sessions {
    &self.sessions
  }
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
      ("error_section.tera", include_str!("../templates/error_section.tera")),
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

  let sessions = Sessions::new(
    Arc::new(MemorySessionStore::new(4096, Duration::from_secs(8 * 3600))),
    b"tera-app-dev-signing-key-not-a-secret",
    SessionConfig::default(),
  );

  AppCore {
    matcher: routes::matcher(),
    resolver: routes::resolver(),
    runtime: Runtime::builder()
      .sources(sources)
      .evaluators(evaluators)
      .cache(Arc::new(FibreCache::bounded(1024, Duration::from_secs(300))))
      .build(),
    actions: action_registry,
    sessions,
  }
}
