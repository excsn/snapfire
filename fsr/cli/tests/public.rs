//! `app/public/`: files served at the site root under their own names.

use std::path::PathBuf;

use snapfire_fsr_cli::new::{create, NewOptions};
use snapfire_fsr_cli::{build, bundle, write, BuildError, Options};

const PAGE: &str = "export default function Page() {\n  return <main>page</main>;\n}\n";

fn project(tag: &str, files: &[(&str, &str)], config: &str) -> PathBuf {
  let root = std::env::temp_dir().join(format!("fsr-cli-public-{}-{tag}", std::process::id()));
  let _ = std::fs::remove_dir_all(&root);
  create(&root, NewOptions { fetch: false, ..NewOptions::default() }).unwrap();
  for (path, text) in files {
    let file = root.join("app").join(path);
    std::fs::create_dir_all(file.parent().unwrap()).unwrap();
    std::fs::write(file, text).unwrap();
  }
  let at = root.join("config/app.toml");
  let text = std::fs::read_to_string(&at).unwrap().replacen("[document]\n", "[document]\norigin = \"https://board.example\"\n", 1);
  std::fs::write(&at, format!("{text}{config}")).unwrap();
  root
}

fn refusal(root: &PathBuf) -> (String, String, String) {
  match build(&root.join("app"), &Options::default()) {
    Err(BuildError::PublicClaimed { file, path, by }) => (file, path, by),
    Err(other) => panic!("{other}"),
    Ok(_) => panic!("built"),
  }
}

#[test]
fn a_public_file_at_a_routes_url_is_refused() {
  let root = project("route", &[("routes/about/page.tsx", PAGE), ("public/about", "x")], "");
  let (file, path, by) = refusal(&root);
  assert_eq!((file.as_str(), path.as_str(), by.as_str()), ("public/about", "/about", "a route"));
  let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn a_public_robots_txt_beside_crawls_own_is_refused() {
  let root = project("robots", &[("public/robots.txt", "User-agent: *\n")], "\n[crawl]\nrobots = \"/robots.txt\"\n");
  let (file, path, by) = refusal(&root);
  assert_eq!((file.as_str(), path.as_str(), by.as_str()), ("public/robots.txt", "/robots.txt", "[crawl] robots"));
  let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn a_public_favicon_beside_one_in_icons_is_refused() {
  let root = project("icon", &[("public/favicon.ico", "a"), ("icons/favicon.ico", "b")], "");
  let (_, path, by) = refusal(&root);
  assert_eq!(path, "/favicon.ico");
  assert!(by.contains("icons"), "{by}");
  let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn a_public_file_under_a_parameter_route_builds_and_ships_at_the_root_of_serve() {
  let root = project("ships", &[("routes/notes/[slug]/page.tsx", PAGE), ("public/notes/plan.pdf", "pdf"), ("public/.well-known/security.txt", "Contact: x\n"), ("public/robots.txt", "User-agent: *\n")], "");
  let app = root.join("app");
  let built = build(&app, &Options::default()).expect("a parameter route does not claim a file under it");
  write(&app, &built).unwrap();
  let out = root.join("dist");
  bundle::run_checked(&root, &out, false).unwrap();
  for (at, text) in [("serve/robots.txt", "User-agent: *\n"), ("serve/notes/plan.pdf", "pdf"), ("serve/.well-known/security.txt", "Contact: x\n")] {
    assert_eq!(std::fs::read_to_string(out.join(at)).unwrap_or_default(), text, "{at}");
  }
  let shipped = snapfire_fsr_host::config::Config::load(&out).unwrap();
  assert!(shipped.statics.iter().any(|s| s.route.trim_end_matches('/').is_empty()), "the tree serves its root: {:?}", shipped.statics);
  let _ = std::fs::remove_dir_all(&root);
}
