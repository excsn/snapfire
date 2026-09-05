//! The service this console does not own, reached only over HTTP and described
//! only by the document it publishes.

use actix_web::web::{Data, Path};
use actix_web::{web, App, HttpResponse, HttpServer};
use parking_lot::RwLock;
use serde::Serialize;
use std::sync::Arc;

/// Included from `app/clients/` rather than copied, so the document the build
/// imports and the document this server publishes cannot disagree.
pub const DOCUMENT: &str = include_str!("../../app/clients/fleet.openapi.json");

#[derive(Clone, Serialize)]
pub struct Agent {
  pub id: u64,
  pub name: String,
  pub region: String,
  pub status: String,
  pub queue_depth: u64,
  pub cpu: f64,
}

#[derive(Clone, Serialize)]
pub struct Job {
  pub id: u64,
  pub name: String,
  pub seconds: u64,
}

#[derive(Clone, Serialize)]
pub struct Alert {
  pub id: u64,
  pub agent_id: u64,
  pub level: String,
  pub text: String,
}

pub struct Fleet {
  agents: Vec<Agent>,
  jobs: Vec<(u64, Job)>,
  alerts: RwLock<Vec<Alert>>,
}

fn agent(id: u64, name: &str, region: &str, status: &str, queue_depth: u64, cpu: f64) -> Agent {
  Agent { id, name: name.to_owned(), region: region.to_owned(), status: status.to_owned(), queue_depth, cpu }
}

fn job(id: u64, name: &str, seconds: u64) -> Job {
  Job { id, name: name.to_owned(), seconds }
}

fn alert(id: u64, agent_id: u64, level: &str, text: &str) -> Alert {
  Alert { id, agent_id, level: level.to_owned(), text: text.to_owned() }
}

impl Fleet {
  pub fn seed() -> Arc<Self> {
    Arc::new(Self {
      agents: vec![
        agent(1, "builder-eu-1", "eu", "up", 3, 61.5),
        agent(2, "builder-eu-2", "eu", "up", 0, 12.0),
        agent(3, "builder-us-1", "us", "down", 7, 0.0),
        agent(4, "builder-us-2", "us", "up", 1, 44.25),
        agent(5, "builder-ap-1", "ap", "up", 0, 8.75),
      ],
      jobs: vec![
        (1, job(11, "compile snapfire_fsr_ir", 92)),
        (1, job(12, "run storefront specs", 41)),
        (3, job(13, "compile rocksolid", 210)),
        (4, job(14, "publish docs", 17)),
      ],
      alerts: RwLock::new(vec![
        alert(21, 3, "page", "builder-us-1 stopped answering"),
        alert(22, 1, "warn", "builder-eu-1 queue over 3"),
      ]),
    })
  }

  fn list(&self, region: Option<&str>) -> Vec<Agent> {
    self.agents.iter().filter(|a| region.is_none_or(|r| a.region == r)).cloned().collect()
  }

  fn get(&self, id: u64) -> Option<Agent> {
    self.agents.iter().find(|a| a.id == id).cloned()
  }

  fn jobs_of(&self, id: u64) -> Vec<Job> {
    self.jobs.iter().filter(|(agent, _)| *agent == id).map(|(_, j)| j.clone()).collect()
  }

  fn alerts(&self) -> Vec<Alert> {
    self.alerts.read().clone()
  }

  fn acknowledge(&self, id: u64) -> Vec<Alert> {
    let mut alerts = self.alerts.write();
    alerts.retain(|a| a.id != id);
    alerts.clone()
  }
}

#[derive(serde::Deserialize)]
struct ListQuery {
  region: Option<String>,
  /// The failure switch, so one segment can degrade while the rest renders.
  fail: Option<u8>,
}

async fn list_agents(fleet: Data<Arc<Fleet>>, query: web::Query<ListQuery>) -> HttpResponse {
  if query.fail.is_some_and(|f| f != 0) {
    return HttpResponse::ServiceUnavailable().body("fleet is unreachable");
  }
  let region = query.region.as_deref().filter(|r| *r != "all");
  HttpResponse::Ok().json(fleet.list(region))
}

async fn get_agent(fleet: Data<Arc<Fleet>>, id: Path<u64>) -> HttpResponse {
  match fleet.get(*id) {
    Some(a) => HttpResponse::Ok().json(a),
    None => HttpResponse::NotFound().body(format!("no agent {}", *id)),
  }
}

async fn list_jobs(fleet: Data<Arc<Fleet>>, id: Path<u64>) -> HttpResponse {
  HttpResponse::Ok().json(fleet.jobs_of(*id))
}

async fn list_alerts(fleet: Data<Arc<Fleet>>) -> HttpResponse {
  HttpResponse::Ok().json(fleet.alerts())
}

async fn acknowledge(fleet: Data<Arc<Fleet>>, id: Path<u64>) -> HttpResponse {
  HttpResponse::Ok().json(fleet.acknowledge(*id))
}

async fn document() -> HttpResponse {
  HttpResponse::Ok().content_type("application/json").body(DOCUMENT)
}

pub async fn serve(fleet: Arc<Fleet>, addr: (&'static str, u16)) -> std::io::Result<()> {
  HttpServer::new(move || {
    App::new()
      .app_data(Data::new(fleet.clone()))
      .route("/openapi.json", web::get().to(document))
      .route("/agents", web::get().to(list_agents))
      .route("/agents/{id}", web::get().to(get_agent))
      .route("/agents/{id}/jobs", web::get().to(list_jobs))
      .route("/alerts", web::get().to(list_alerts))
      .route("/alerts/{id}/ack", web::post().to(acknowledge))
  })
  .bind(addr)?
  .run()
  .await
}
