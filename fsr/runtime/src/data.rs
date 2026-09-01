use std::future::Future;
use std::sync::Arc;

use futures_util::future::BoxFuture;
use indexmap::IndexMap;
use snapfire_fsr_core::{Data, DataSourceId};

use crate::ctx::RequestCtx;

#[derive(Debug, Clone, thiserror::Error)]
#[error("data source {source_id}: {message}")]
pub struct LoadError {
  pub source_id: String,
  pub message: String,
}

pub trait DataSource: Send + Sync {
  fn load(&self, ctx: &RequestCtx) -> BoxFuture<'static, Result<Data, LoadError>>;
}

struct FnSource<F>(F);

impl<F, Fut> DataSource for FnSource<F>
where
  F: Fn(RequestCtx) -> Fut + Send + Sync,
  Fut: Future<Output = Result<Data, LoadError>> + Send + 'static,
{
  fn load(&self, ctx: &RequestCtx) -> BoxFuture<'static, Result<Data, LoadError>> {
    Box::pin((self.0)(ctx.clone()))
  }
}

#[derive(Default)]
pub struct DataSources {
  sources: IndexMap<String, Arc<dyn DataSource>>,
}

impl DataSources {
  pub fn new() -> Self {
    Self::default()
  }

  pub fn insert(&mut self, id: impl Into<String>, source: Arc<dyn DataSource>) {
    self.sources.insert(id.into(), source);
  }

  pub fn insert_fn<F, Fut>(&mut self, id: impl Into<String>, f: F)
  where
    F: Fn(RequestCtx) -> Fut + Send + Sync + 'static,
    Fut: Future<Output = Result<Data, LoadError>> + Send + 'static,
  {
    self.insert(id, Arc::new(FnSource(f)));
  }

  pub fn get(&self, id: &DataSourceId) -> Option<&Arc<dyn DataSource>> {
    self.sources.get(&id.0)
  }
}
