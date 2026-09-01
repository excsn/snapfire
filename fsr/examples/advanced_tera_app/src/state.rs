use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use parking_lot::Mutex;

/// The application's one piece of mutable state: the server fleet the pages
/// render and the actions mutate.
#[derive(Clone)]
pub struct Fleet {
  servers: Arc<Mutex<Vec<(String, f64)>>>,
}

impl Fleet {
  pub fn seed() -> Self {
    Self {
      servers: Arc::new(Mutex::new(vec![("web-1".to_owned(), 0.73), ("web-2".to_owned(), 0.41)])),
    }
  }

  pub fn list(&self) -> Vec<(String, f64)> {
    self.servers.lock().clone()
  }

  /// Errs when the name is taken; returns the new fleet size otherwise.
  pub fn add(&self, name: String, load: f64) -> Result<usize, ()> {
    let mut servers = self.servers.lock();
    if servers.iter().any(|(existing, _)| *existing == name) {
      return Err(());
    }
    servers.push((name, load));
    Ok(servers.len())
  }
}

/// Server renders since boot, so a page can show what a navigation cost. The
/// edge bumps it once per assembly, before the eager wave, so every loader in
/// one render reads the same number.
#[derive(Clone, Default)]
pub struct Renders(Arc<AtomicU64>);

impl Renders {
  pub fn next(&self) -> u64 {
    self.0.fetch_add(1, Ordering::Relaxed) + 1
  }

  pub fn get(&self) -> u64 {
    self.0.load(Ordering::Relaxed)
  }
}
