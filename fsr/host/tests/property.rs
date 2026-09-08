//! The request path under generated input. A path, a `Cookie` header and an
//! `Accept-Language` header are all whatever the client sent, so nothing here
//! may panic: a panic in this layer is a request that kills the worker.

use proptest::prelude::*;
use snapfire_fsr_host::locale::{Locales, LocalesSection};

fn locales() -> impl Strategy<Value = Locales> {
  (
    prop::collection::vec(prop_oneof![
      Just("en".to_owned()),
      Just("fr".to_owned()),
      Just("pt-BR".to_owned()),
      Just("zh-Hans".to_owned()),
    ], 1..4),
    prop::collection::vec(prop_oneof![
      Just("prefix".to_owned()),
      Just("cookie".to_owned()),
      Just("header".to_owned()),
    ], 0..3),
    any::<bool>(),
  )
    .prop_map(|(mut supported, order, remember)| {
      supported.dedup();
      let section = LocalesSection {
        default: Some(supported[0].clone()),
        supported,
        order,
        remember,
        cookie: "sf_locale".to_owned(),
      };
      Locales::from_section(&section).expect("a valid section")
    })
}

/// Paths as they arrive: encoded, empty, unicode, and shaped like a prefix.
fn paths() -> impl Strategy<Value = String> {
  prop_oneof![
    3 => "/[a-zA-Z0-9/_.%-]{0,20}",
    2 => "(?s).{0,16}",
    2 => prop_oneof![
      Just(String::new()),
      Just("/".to_owned()),
      Just("//".to_owned()),
      Just("/en".to_owned()),
      Just("/en/".to_owned()),
      Just("/enx".to_owned()),
      Just("/é/x".to_owned()),
      Just("/🌍".to_owned()),
      Just("/pt-BR/a".to_owned()),
      Just("/../..".to_owned()),
    ],
  ]
}

fn headers() -> impl Strategy<Value = String> {
  prop_oneof![
    2 => "[a-zA-Z0-9,;=. *-]{0,30}",
    2 => "(?s).{0,20}",
    2 => prop_oneof![
      Just(String::new()),
      Just("*".to_owned()),
      Just(";".to_owned()),
      Just("q=".to_owned()),
      Just("en;q=".to_owned()),
      Just("en;q=NaN".to_owned()),
      Just("en;q=inf".to_owned()),
      Just("en;q=-1".to_owned()),
      Just("fr,en;q=0.8".to_owned()),
      Just("sf_locale=fr".to_owned()),
      Just("sf_locale=🌍".to_owned()),
      Just("=".to_owned()),
      Just("a=b;;c".to_owned()),
    ],
  ]
}

proptest! {
  #![proptest_config(ProptestConfig { cases: 512, ..ProptestConfig::default() })]

  /// Nothing a client can send makes resolving a locale panic.
  #[test]
  fn resolving_a_locale_never_panics(
    l in locales(),
    path in paths(),
    cookie in prop::option::of(headers()),
    accept in prop::option::of(headers()),
  ) {
    let _ = l.resolve(&path, cookie.as_deref(), accept.as_deref());
  }

  /// Each header reader on its own, since `resolve` may not reach every branch.
  #[test]
  fn reading_a_header_never_panics(l in locales(), header in headers()) {
    let _ = l.from_accept_language(&header);
    let _ = l.from_cookie(&header);
    let _ = l.find(&header);
    let _ = l.nearest(&header);
  }

  /// A locale a reader answers with is one the application supports; nothing
  /// a client sends can smuggle another value into the request.
  #[test]
  fn a_chosen_locale_is_always_supported(
    l in locales(),
    path in paths(),
    cookie in prop::option::of(headers()),
    accept in prop::option::of(headers()),
  ) {
    let chosen = l.resolve(&path, cookie.as_deref(), accept.as_deref());
    prop_assert!(
      l.supported.iter().any(|s| s == &chosen.locale.tag),
      "{} is not supported: {:?}", chosen.locale.tag, l.supported
    );
    // `set_cookie` is the whole header, so it names the locale rather than being it
    if let Some(set) = &chosen.set_cookie {
      prop_assert!(
        l.supported.iter().any(|s| set.contains(s.as_str())),
        "cookie would set an unsupported locale: {set}"
      );
    }
  }

  /// Stripping a prefix leaves a path, never a fragment of one: whatever comes
  /// back is routed, so it has to start at the root.
  #[test]
  fn a_stripped_path_is_still_a_path(l in locales(), path in paths()) {
    let out = l.resolve(&path, None, None);
    if out.prefixed {
      prop_assert!(out.path.starts_with('/'), "{:?} -> {:?}", path, out.path);
      prop_assert!(path.ends_with(&out.path) || out.path == "/", "{:?} -> {:?}", path, out.path);
    } else {
      prop_assert_eq!(&out.path, &path);
    }
  }
}
