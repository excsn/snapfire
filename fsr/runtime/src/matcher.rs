use snapfire_fsr_core::Params;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct EntryId(pub u32);

#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub struct RouteMatch {
  pub entry: EntryId,
  pub params: Params,
}

impl RouteMatch {
  pub fn new(entry: EntryId, params: Params) -> Self {
    Self { entry, params }
  }
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
#[non_exhaustive]
pub struct HandlerMatch {
  pub id: String,
  pub params: Params,
}

impl HandlerMatcher {
  pub fn new() -> Self {
    Self::default()
  }

  pub fn insert(&mut self, method: &str, pattern: &str, id: impl Into<String>) -> Result<(), matchit::InsertError> {
    self
      .routers
      .entry(method.to_ascii_uppercase())
      .or_default()
      .insert(pattern, id.into())
  }

  /// The handler for `method` at `path`. A HEAD with no handler of its own is answered by the GET handler.
  pub fn match_request(&self, method: &str, path: &str) -> Option<HandlerMatch> {
    let method = method.to_ascii_uppercase();
    let router = match self.routers.get(&method) {
      None if method == "HEAD" => self.routers.get("GET")?,
      router => router?,
    };
    let found = router.at(path).ok()?;
    let mut params = Params::new();
    for (key, value) in found.params.iter() {
      params.insert(key.to_owned(), value.to_owned());
    }
    Some(HandlerMatch {
      id: found.value.clone(),
      params,
    })
  }

  /// The methods some handler answers at `path`, sorted, with HEAD wherever GET is: empty when no handler claims the path.
  pub fn allowed(&self, path: &str) -> Vec<String> {
    let mut methods: Vec<String> = self.routers.iter().filter(|(_, router)| router.at(path).is_ok()).map(|(method, _)| method.clone()).collect();
    if methods.iter().any(|m| m == "GET") && !methods.iter().any(|m| m == "HEAD") {
      methods.push("HEAD".to_owned());
    }
    methods.sort();
    methods
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
    Some(RouteMatch {
      entry: *found.value,
      params,
    })
  }
}
