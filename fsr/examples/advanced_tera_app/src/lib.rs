pub mod actions;
pub mod http;
pub mod loaders;
pub mod render;
pub mod routes;
pub mod services;
pub mod state;

use std::sync::Arc;
use std::time::Duration;

use snapfire_fsr_auth::{Auth, DevProvider};
use snapfire_fsr_core::ModuleId;
use snapfire_fsr_runtime::{
  ActionRegistry, DataSources, Evaluators, FibreCache, MatchitMatcher, Runtime, TableResolver,
};
use snapfire_fsr_service::Services;
use snapfire_fsr_session::{MemorySessionStore, SessionConfig, Sessions};
use snapfire_fsr_tera::TeraEvaluator;

pub use render::{call_action, negotiate_encoding, render, respond, respond_with, AppError, Incoming, RenderMode};

pub struct AppCore {
  pub(crate) matcher: MatchitMatcher,
  pub(crate) resolver: TableResolver,
  pub(crate) runtime: Arc<Runtime>,
  pub(crate) actions: ActionRegistry,
  pub(crate) sessions: Sessions,
  pub(crate) auth: Auth,
  pub(crate) services: Arc<Services>,
}

impl AppCore {
  pub fn sessions(&self) -> &Sessions {
    &self.sessions
  }

  pub fn auth(&self) -> &Auth {
    &self.auth
  }

  pub fn services(&self) -> &Arc<Services> {
    &self.services
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
      ("login.tera", include_str!("../templates/login.tera")),
    ])
    .expect("templates parse");
  tera
}

pub fn build_app(chart_delay: Duration) -> AppCore {
  let fleet = state::Fleet::seed();

  let services = services::build(fleet.clone());

  let mut sources = DataSources::new();
  loaders::register(&mut sources, chart_delay);

  let mut evaluators = Evaluators::new();
  evaluators.register(
    |m: &ModuleId| m.path.ends_with(".tera"),
    Arc::new(TeraEvaluator::new(templates())),
  );

  let mut action_registry = ActionRegistry::new();
  actions::register(&mut action_registry);

  let sessions = Sessions::new(
    Arc::new(MemorySessionStore::new(4096, Duration::from_secs(8 * 3600))),
    b"tera-app-dev-signing-key-not-a-secret",
    SessionConfig::default(),
  );

  let auth = Auth::new(Arc::new(
    DevProvider::new("/login").user("alice", "wonder").user("bob", "builder"),
  ));

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
    auth,
    services,
  }
}
