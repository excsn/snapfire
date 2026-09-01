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
