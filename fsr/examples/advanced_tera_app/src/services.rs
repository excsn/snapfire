use snapfire_fsr_macros::{service, Record};
use snapfire_fsr_runtime::{FailureKind, ServiceError};
use snapfire_fsr_service::DeclaredService;

use crate::state::Fleet;

#[derive(Record)]
pub struct Server {
  pub name: String,
  pub load: f64,
}

#[derive(Record)]
pub struct Added {
  pub count: u32,
}

/// The fleet as a service: `ctx.services.fleet`, its contract read off these
/// signatures and served in process.
#[service]
impl Fleet {
  pub fn list(&self, section: String) -> Result<Vec<Server>, ServiceError> {
    if section == "down" {
      return Err(ServiceError::new(FailureKind::Unavailable, Fleet::NAME, "list", "the servers backend is unreachable"));
    }
    Ok(self.servers().into_iter().map(|(name, load)| Server { name, load }).collect())
  }

  pub fn count(&self) -> u32 {
    self.servers().len() as u32
  }

  pub fn add(&self, name: String, load: f64) -> Result<Added, ServiceError> {
    match self.insert(name.clone(), load) {
      Ok(count) => Ok(Added { count: count as u32 }),
      Err(()) => Err(ServiceError::new(FailureKind::Conflict, Fleet::NAME, "add", format!("server `{name}` already exists"))),
    }
  }
}
