use std::path::{Path, PathBuf};

use snapfire_fsr_host::config::Config;
use snapfire_fsr_sites::layout::{self, Source};

fn dir(name: &str) -> PathBuf {
  let nanos = std::time::SystemTime::now()
    .duration_since(std::time::UNIX_EPOCH)
    .unwrap()
    .as_nanos();
  let at = std::env::temp_dir().join(format!("fsr-layout-{name}-{}-{nanos}", std::process::id()));
  std::fs::create_dir_all(&at).unwrap();
  at
}

fn write(path: &Path, text: &str) {
  std::fs::create_dir_all(path.parent().unwrap()).unwrap();
  std::fs::write(path, text).unwrap();
}

/// A project at `at` whose configuration is `extra` on top of the least the
/// host accepts.
fn project(name: &str, extra: &str) -> PathBuf {
  let at = dir(name);
  write(&at.join("app/generated/plan.sexp"), "(plan 2)");
  write(
    &at.join("config/app.toml"),
    &format!("[app]\ndir = \"app\"\n\n[session]\nkey = \"k\"\n\n{extra}"),
  );
  at
}

fn laid(at: &Path) -> (Config, layout::Layout) {
  let config = Config::load(at).unwrap();
  let laid = layout::layout(at, &config).unwrap();
  (config, laid)
}

fn layer(laid: &layout::Layout) -> String {
  laid
    .places
    .iter()
    .find_map(|p| match &p.from {
      Source::Text(text) if p.to.ends_with(layout::LAYER) => Some(text.clone()),
      _ => None,
    })
    .expect("a project that carries no layer is given one")
}

/// The defect this layout exists to make impossible: a static root outside the
/// project used to be joined onto the output directory, where its `..`
/// segments resolved outside it.
#[test]
fn a_static_root_outside_the_project_is_placed_inside_the_tree() {
  let at = project("escaping", "[[static]]\nroute = \"/static/js\"\ndir = \"../../shared/client/dist\"\n");
  write(&at.join("../../shared/client/dist/main.js"), "export {}\n");
  let (_, laid) = laid(&at);

  let place = laid.places.iter().find(|p| p.to.starts_with("serve/")).unwrap();
  assert_eq!(place.to, "serve/static/js");
  assert!(
    place.path().unwrap().ends_with("shared/client/dist"),
    "{:?}",
    place.path()
  );
  assert!(layer(&laid).contains(r#"dir = "../serve/static/js""#), "{}", layer(&laid));

  for row in laid.rows().unwrap() {
    assert!(
      Path::new(&row.path).components().all(|c| matches!(c, std::path::Component::Normal(_))),
      "{} leaves the tree",
      row.path
    );
  }
}

#[test]
fn a_route_that_climbs_is_refused_rather_than_written() {
  let at = project("climbing", "[[static]]\nroute = \"/../../etc\"\ndir = \"dist\"\n");
  write(&at.join("app/dist/main.js"), "export {}\n");
  let config = Config::load(&at).unwrap();
  let e = layout::layout(&at, &config).unwrap_err().to_string();
  assert!(e.contains("outside the tree"), "{e}");
}

/// `load_catalogs` reads `locales/` by name, so nothing in the configuration
/// names it and a tree that drops it serves message keys.
#[test]
fn the_message_catalogs_ship() {
  let at = project("locales", "[locales]\nsupported = [\"en\", \"fr\"]\ndefault = \"en\"\n");
  write(&at.join("app/locales/en.toml"), "greeting = \"hello\"\n");
  write(&at.join("app/locales/fr.toml"), "greeting = \"bonjour\"\n");
  let (_, laid) = laid(&at);
  let rows: Vec<String> = laid.rows().unwrap().into_iter().map(|r| r.path).collect();
  assert!(rows.contains(&"app/locales/en.toml".to_owned()), "{rows:?}");
  assert!(rows.contains(&"app/locales/fr.toml".to_owned()), "{rows:?}");
}

/// A client's document is imported at boot, so a tree without it cannot start.
#[test]
fn a_clients_document_ships_under_a_name_the_host_reads_back() {
  let at = project(
    "clients",
    "[clients.billing]\nbase_url = \"https://b\"\n\n[clients.feed]\nbase_url = \"https://f\"\ndocument = \"../schemas/feed.proto\"\n\n[clients.stub]\ntransport = \"mock\"\n",
  );
  write(&at.join("app/clients/billing.openapi.json"), "{}");
  write(&at.join("schemas/feed.proto"), "syntax = \"proto3\";\n");
  write(&at.join("app/clients/stub.openapi.json"), "{}");
  write(&at.join("app/clients/stub.mock.json"), "{}");
  let (_, laid) = laid(&at);
  let rows: Vec<String> = laid.rows().unwrap().into_iter().map(|r| r.path).collect();
  assert!(rows.contains(&"app/clients/billing.openapi.json".to_owned()), "{rows:?}");
  // The host chooses a proto import by the suffix, so that survives the move
  // even though the file came from outside the application directory.
  assert!(rows.contains(&"app/clients/feed.proto".to_owned()), "{rows:?}");
  assert!(rows.contains(&"app/clients/stub.mock.json".to_owned()), "{rows:?}");
  let layer = layer(&laid);
  assert!(layer.contains(r#"document = "clients/feed.proto""#), "{layer}");
  assert!(layer.contains(r#"responses = "clients/stub.mock.json""#), "{layer}");
}

/// The tree has no `icons/` or `styles/` of its own, so what inference read
/// out of the project has to be written down or it is lost.
#[test]
fn the_layer_carries_what_inference_found_rather_than_what_was_written() {
  let at = project("inferred", "");
  write(&at.join("app/styles/site.css"), ".a{}\n");
  write(&at.join("app/icons/favicon.svg"), "<svg/>");
  write(
    &at.join("app/dist/.snapfire-build.json"),
    r#"{"version":1,"publicPath":"/static/js/app/","entries":["src/main.js"]}"#,
  );
  let (config, laid) = laid(&at);
  assert!(!config.inferred.is_empty());
  let layer = layer(&laid);
  assert!(layer.contains(r#"entry = "/static/js/app/src/main.js""#), "{layer}");
  assert!(layer.contains(r#"styles = ["/static/css/site.css"]"#), "{layer}");
  assert!(layer.contains("/static/icons/favicon.svg"), "{layer}");
}

/// A tree is deployed under a `RELEASE_ENV` the bundle did not run under, so
/// it carries every overlay rather than the ones this run happened to load.
#[test]
fn the_configuration_directory_ships_whole() {
  let at = project("overlays", "");
  write(&at.join("config/production.toml"), "[server]\nlisten = \"0.0.0.0:80\"\n");
  let (_, laid) = laid(&at);
  let rows: Vec<String> = laid.rows().unwrap().into_iter().map(|r| r.path).collect();
  assert!(rows.contains(&"config/app.toml".to_owned()), "{rows:?}");
  assert!(rows.contains(&"config/production.toml".to_owned()), "{rows:?}");
  assert!(rows.contains(&"config/bundle.toml".to_owned()), "{rows:?}");
}

/// Laying out a tree yields the tree: its configuration already names tree
/// paths, so an artifact verifies against the bundle that produced it.
#[test]
fn a_tree_lays_out_as_itself() {
  let at = project(
    "idempotent",
    "[[static]]\nroute = \"/static/js\"\ndir = \"../../shared/dist\"\n\n[clients.billing]\nbase_url = \"https://b\"\n",
  );
  write(&at.join("../../shared/dist/main.js"), "export {}\n");
  write(&at.join("app/clients/billing.openapi.json"), "{}");
  write(&at.join("app/locales/en.toml"), "a = \"b\"\n");

  let tree = dir("idempotent-tree");
  let (_, project) = laid(&at);
  let first = project.rows().unwrap();
  for row in &first {
    let to = tree.join(&row.path);
    std::fs::create_dir_all(to.parent().unwrap()).unwrap();
    std::fs::write(&to, row.bytes().unwrap()).unwrap();
  }

  let (_, again) = laid(&tree);
  let second = again.rows().unwrap();
  assert_eq!(
    first.iter().map(|r| &r.path).collect::<Vec<_>>(),
    second.iter().map(|r| &r.path).collect::<Vec<_>>()
  );
  for (a, b) in first.iter().zip(&second) {
    assert_eq!(a.bytes().unwrap(), b.bytes().unwrap(), "{}", a.path);
  }
}

/// Two files cannot be given one destination, because the second would
/// silently replace the first.
#[test]
fn two_static_roots_on_one_route_collide_rather_than_overwrite() {
  let at = project(
    "collide",
    "[[static]]\nroute = \"/assets\"\ndir = \"one\"\n\n[[static]]\nroute = \"/assets\"\ndir = \"two\"\n",
  );
  write(&at.join("app/one/a.js"), "a");
  write(&at.join("app/two/b.js"), "b");
  let config = Config::load(&at).unwrap();
  let e = layout::layout(&at, &config).unwrap_err().to_string();
  assert!(e.contains("placed twice"), "{e}");
}

/// Whatever a route says, the tree keeps it: a destination that is not a plain
/// relative path is an error rather than a write.
#[test]
fn no_route_reaches_outside_the_tree() {
  proptest::proptest!(|(route in "[/.a-zA-Z0-9_-]{0,40}")| {
    let at = project("fuzz-route", &format!("[[static]]\nroute = \"/{route}\"\ndir = \"dist\"\n"));
    write(&at.join("app/dist/a.js"), "a");
    let config = Config::load(&at).unwrap();
    if let Ok(laid) = layout::layout(&at, &config) {
      for place in &laid.places {
        let path = Path::new(&place.to);
        proptest::prop_assert!(
          path.is_relative()
            && path.components().all(|c| matches!(c, std::path::Component::Normal(_))),
          "{} leaves the tree",
          place.to
        );
      }
    }
    std::fs::remove_dir_all(&at).ok();
  });
}
