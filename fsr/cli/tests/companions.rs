//! `fsr add` of a framework brings what comes with it at the same version.

use snapfire_fsr_cli::direction::with_companions;
use snapfire_fsr_cli::vendor::Spec;

fn named(specs: &[Spec]) -> Vec<String> {
  specs.iter().map(|spec| format!("{}@{}{}", spec.package, spec.version, spec.subpath.as_deref().map(|s| format!("/{s}")).unwrap_or_default())).collect()
}

#[test]
fn react_brings_its_jsx_runtime_and_react_dom_at_its_version() {
  let (plain, published) = with_companions(vec![Spec::parse("react@19.2.0").unwrap()]);
  assert_eq!(named(&plain), ["react@19.2.0", "react@19.2.0/jsx-runtime", "react-dom@19.2.0/client"]);
  assert!(published.is_empty());

  let (plain, _) = with_companions(vec![Spec::parse("react-dom@18.3.1").unwrap()]);
  assert_eq!(named(&plain), ["react@18.3.1", "react@18.3.1/jsx-runtime", "react-dom@18.3.1/client"], "either half brings the other");
}

#[test]
fn svelte_brings_every_subpath_its_adapter_imports_as_published() {
  let (plain, published) = with_companions(vec![Spec::parse("svelte@5.60.0").unwrap(), Spec::parse("dayjs@1.11.13").unwrap()]);
  assert_eq!(named(&plain), ["dayjs@1.11.13"], "a package of no framework is left as written");
  assert!(published.iter().all(|spec| spec.package == "svelte" && spec.version == "5.60.0"), "{:?}", named(&published));
  assert!(named(&published).contains(&"svelte@5.60.0/internal/client".to_owned()));
  assert_eq!(published.len(), snapfire_fsr_cli::direction::find("svelte").unwrap().vendors.len());
}

#[test]
fn vue_is_one_package() {
  let (plain, published) = with_companions(vec![Spec::parse("vue@3.5.44").unwrap()]);
  assert_eq!(named(&plain), ["vue@3.5.44"]);
  assert!(published.is_empty());
}
