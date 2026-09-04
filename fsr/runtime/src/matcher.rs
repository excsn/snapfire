use snapfire_fsr_core::Params;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct EntryId(pub u32);

#[derive(Debug, Clone, PartialEq)]
pub struct RouteMatch {
  pub entry: EntryId,
  pub params: Params,
}

pub trait Matcher: Send + Sync {
  fn match_path(&self, path: &str) -> Option<RouteMatch>;
}

#[derive(Default)]
pub struct MatchitMatcher {
  router: matchit::Router<EntryId>,
}

impl MatchitMatcher {
  pub fn new() -> Self {
    Self::default()
  }

  pub fn insert(&mut self, pattern: &str, entry: EntryId) -> Result<(), matchit::InsertError> {
    self.router.insert(pattern, entry)
  }
}

/// Handler routes, one router per HTTP method, resolving to a handler id.
#[derive(Default)]
pub struct HandlerMatcher {
  routers: std::collections::HashMap<String, matchit::Router<String>>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct HandlerMatch {
  pub id: String,
  pub params: Params,
}

impl HandlerMatcher {
  pub fn new() -> Self {
    Self::default()
  }

  pub fn insert(&mut self, method: &str, pattern: &str, id: impl Into<String>) -> Result<(), matchit::InsertError> {
    self.routers.entry(method.to_ascii_uppercase()).or_default().insert(pattern, id.into())
  }

  pub fn match_request(&self, method: &str, path: &str) -> Option<HandlerMatch> {
    let found = self.routers.get(&method.to_ascii_uppercase())?.at(path).ok()?;
    let mut params = Params::new();
    for (key, value) in found.params.iter() {
      params.insert(key.to_owned(), value.to_owned());
    }
    Some(HandlerMatch { id: found.value.clone(), params })
  }

  pub fn is_empty(&self) -> bool {
    self.routers.is_empty()
  }
}

impl Matcher for MatchitMatcher {
  fn match_path(&self, path: &str) -> Option<RouteMatch> {
    let found = self.router.at(path).ok()?;
    let mut params = Params::new();
    for (key, value) in found.params.iter() {
      params.insert(key.to_owned(), value.to_owned());
    }
    Some(RouteMatch { entry: *found.value, params })
  }
}
