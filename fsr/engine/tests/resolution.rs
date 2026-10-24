//! A bare specifier resolves through the import map as a browser resolves it: an exact key first, else the longest key ending in `/` that starts it.

use std::collections::HashMap;
use std::path::PathBuf;

use snapfire_fsr_engine::Resolution;

#[test]
fn a_prefix_key_resolves_what_follows_it_and_an_exact_key_wins() {
  let map: HashMap<String, String> = [
    ("@snapfire/fsr/client", "/static/js/fsr/index.js"),
    ("@snapfire/fsr/client/", "/static/js/fsr/"),
    ("@snapfire/fsr/client/react", "/static/js/fsr/react-override.js"),
    ("lib/", "/static/js/lib/"),
    ("lib/deep/", "/static/js/deep/"),
  ]
  .into_iter()
  .map(|(k, v)| (k.to_owned(), v.to_owned()))
  .collect();
  let resolution = Resolution::new(map, vec![("/static/js".to_owned(), PathBuf::from("/root"))], HashMap::new());
  assert_eq!(resolution.resolve("@snapfire/fsr/client"), Some(PathBuf::from("/root/fsr/index.js")));
  assert_eq!(resolution.resolve("@snapfire/fsr/client/intl.js"), Some(PathBuf::from("/root/fsr/intl.js")));
  assert_eq!(resolution.resolve("@snapfire/fsr/client/react"), Some(PathBuf::from("/root/fsr/react-override.js")));
  assert_eq!(resolution.resolve("lib/deep/x.js"), Some(PathBuf::from("/root/deep/x.js")), "the longest prefix");
  assert_eq!(resolution.resolve("lib/x.js"), Some(PathBuf::from("/root/lib/x.js")));
  assert_eq!(resolution.resolve("other"), None);
}
