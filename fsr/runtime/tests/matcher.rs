use snapfire_fsr_runtime::{EntryId, HandlerMatcher, PathMatcher, RouteConflict};

#[test]
fn a_conflicting_page_pattern_is_a_route_conflict() {
  let mut matcher = PathMatcher::new();
  matcher.insert("/products/{slug}", EntryId(0)).unwrap();
  let conflict: RouteConflict = matcher.insert("/products/{id}", EntryId(1)).unwrap_err();
  assert_eq!(conflict.pattern, "/products/{id}");
  assert!(!conflict.reason.is_empty());
  assert_eq!(conflict.to_string(), conflict.reason);
}

#[test]
fn a_conflicting_handler_pattern_is_a_route_conflict() {
  let mut matcher = HandlerMatcher::new();
  matcher.insert("get", "/api/{id}", "a").unwrap();
  matcher.insert("post", "/api/{other}", "b").unwrap();
  let conflict: RouteConflict = matcher.insert("GET", "/api/{other}", "c").unwrap_err();
  assert_eq!(conflict.pattern, "/api/{other}");
  assert_eq!(conflict.to_string(), conflict.reason);
}
