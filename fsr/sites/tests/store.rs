use std::path::{Path, PathBuf};

use snapfire_fsr_host::config::Config;
use snapfire_fsr_host::Host;
use snapfire_fsr_sites::{fetch_missing, pack, Cache};

fn dir(name: &str) -> PathBuf {
  let nanos = std::time::SystemTime::now()
    .duration_since(std::time::UNIX_EPOCH)
    .unwrap()
    .as_nanos();
  let dir = std::env::temp_dir().join(format!("fsr-store-{name}-{}-{nanos}", std::process::id()));
  std::fs::create_dir_all(&dir).unwrap();
  dir
}

const PLAN: &str = r#"{"version":2,"routes":[{"pattern":"/","plan":{"id":0,"module":"shell#document","children":[{"slot":"content","node":{"id":1,"module":"routes/index/page.tsx#default"}}]}}],"sources":[],"actions":[],"handlers":[],"components":[]}"#;

fn shell(root: &Path, sites: &str) {
  std::fs::create_dir_all(root.join("app/generated")).unwrap();
  let manifest = snapfire_fsr_plan::Manifest::from_json(PLAN).unwrap();
  std::fs::write(root.join("app/generated/plan.sexp"), manifest.to_sexpr()).unwrap();
  std::fs::write(
    root.join("app.toml"),
    format!("[app]\ndir = \"app\"\n[document]\ntitle = \"t\"\n[session]\nkey = \"k\"\n[sites]\nroot = \"sites\"\n{sites}"),
  )
  .unwrap();
}

/// A billing site packed as `billing-<version>.tar.gz` into `store`.
fn packed(store: &Path, version: &str) {
  let at = dir("site");
  std::fs::create_dir_all(at.join("app/generated")).unwrap();
  let manifest = snapfire_fsr_plan::Manifest::from_json(PLAN)
    .unwrap()
    .namespaced("billing", "/billing", "shell#document");
  std::fs::write(at.join("app/generated/plan.sexp"), manifest.to_sexpr()).unwrap();
  std::fs::write(
    at.join("app.toml"),
    "[app]\ndir = \"app\"\n[document]\ntitle = \"t\"\n[session]\nkey = \"k\"\n[site]\nname = \"billing\"\nat = \"/billing\"\n",
  )
  .unwrap();
  std::fs::create_dir_all(store).unwrap();
  pack(&at, version, &store.join(format!("billing-{version}.tar.gz"))).unwrap();
}

fn host(root: &Path) -> Result<Host, String> {
  let config = Config::load(root).map_err(|e| e.to_string())?;
  let builder = snapfire_fsr_sites::mountable(snapfire_fsr_sites::mount_all(Host::from_config(config).unwrap()).map_err(|e| e.to_string())?);
  builder.build().map_err(|e| e.to_string())
}

#[test]
fn a_shell_fetches_a_version_its_cache_lacks_from_the_store_it_names() {
  let root = dir("fetch");
  packed(&root.join("archives"), "1.0.0");
  shell(&root, "store = \"archives\"\n[sites.billing]\nartifact = \"billing@1.0.0\"\n");
  assert!(!root.join("sites/billing/1.0.0").exists());

  let host = host(&root).unwrap();
  assert!(root.join("sites/billing/1.0.0/config").is_dir(), "installed under the root");
  assert_eq!(host.report().sites[0].version, "1.0.0");
  assert!(host.report().app.routes.iter().any(|r| r.0 == "/billing"));

  std::fs::remove_dir_all(root.join("archives")).unwrap();
  host.reload_sites().unwrap();
  assert_eq!(host.report().sites[0].version, "1.0.0", "a held version is not fetched again");
}

#[test]
fn a_version_the_store_lacks_refuses_the_mount_and_names_the_row() {
  let root = dir("absent");
  packed(&root.join("archives"), "1.0.0");
  shell(&root, "store = \"archives\"\n[sites.billing]\nartifact = \"billing@2.0.0\"\n");
  let e = host(&root).err().expect("refused");
  assert!(e.contains("sites.billing") && e.contains("2.0.0"), "{e}");
  assert!(!root.join("sites/billing/2.0.0").exists(), "nothing half-installed");
}

#[test]
fn a_fetched_version_is_still_held_to_its_pin() {
  let root = dir("pinned");
  packed(&root.join("archives"), "1.0.0");
  shell(&root, "store = \"archives\"\n[sites.billing]\nartifact = \"billing@1.0.0\"\nhash = \"0000000000000000\"\n");
  let e = host(&root).err().expect("refused");
  assert!(e.contains("pinned 0000000000000000"), "{e}");
}

#[test]
fn a_store_without_a_root_refuses_to_load() {
  let root = dir("rootless");
  std::fs::create_dir_all(root.join("app/generated")).unwrap();
  std::fs::write(
    root.join("app.toml"),
    "[app]\ndir = \"app\"\n[document]\ntitle = \"t\"\n[session]\nkey = \"k\"\n[sites]\nstore = \"archives\"\n",
  )
  .unwrap();
  let e = Config::load(&root).unwrap_err().to_string();
  assert!(e.contains("sites.store") && e.contains("sites.root"), "{e}");
}

#[test]
fn fetch_missing_skips_a_path_row_and_a_held_version() {
  let root = dir("skip");
  packed(&root.join("archives"), "1.0.0");
  shell(&root, "[sites.billing]\nartifact = \"billing@1.0.0\"\n[sites.local]\nartifact = \"../elsewhere\"\n");
  let config = Config::load(&root).unwrap();
  let store = snapfire_fsr_sites::TarStore::new(root.join("archives"));
  assert_eq!(fetch_missing(&config, &store).unwrap().len(), 1);
  assert!(fetch_missing(&config, &store).unwrap().is_empty());
  assert!(Cache::new(root.join("sites")).holds("billing", "1.0.0"));
}

#[cfg(feature = "http")]
mod http {
  use std::io::{BufRead, BufReader, Write};
  use std::net::TcpListener;
  use std::path::PathBuf;

  use super::*;

  /// Serves `dir` over HTTP, answering 401 to a request without
  /// `x-token: t` and 404 to a name it does not hold.
  fn serve(dir: PathBuf) -> String {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    std::thread::spawn(move || {
      for stream in listener.incoming().flatten() {
        let mut reader = BufReader::new(stream.try_clone().unwrap());
        let mut first = String::new();
        reader.read_line(&mut first).unwrap();
        let path = first.split_whitespace().nth(1).unwrap_or("/").to_owned();
        let mut token = false;
        loop {
          let mut line = String::new();
          if reader.read_line(&mut line).unwrap() == 0 || line == "\r\n" {
            break;
          }
          token |= line.to_ascii_lowercase().trim() == "x-token: t";
        }
        let mut stream = stream;
        let file = dir.join(path.trim_start_matches('/'));
        let (status, body) = match (token, std::fs::read(&file)) {
          (false, _) => ("401 Unauthorized", Vec::new()),
          (true, Ok(bytes)) => ("200 OK", bytes),
          (true, Err(_)) => ("404 Not Found", Vec::new()),
        };
        write!(stream, "HTTP/1.1 {status}\r\ncontent-length: {}\r\nconnection: close\r\n\r\n", body.len()).unwrap();
        stream.write_all(&body).unwrap();
      }
    });
    base
  }

  #[test]
  fn an_http_store_fetches_with_its_header_and_verifies_what_lands() {
    let root = dir("http");
    packed(&root.join("served"), "1.0.0");
    let base = serve(root.join("served"));
    shell(&root, &format!("store = \"{base}\"\n[sites.billing]\nartifact = \"billing@1.0.0\"\n"));
    let config = Config::load(&root).unwrap();

    let bare = snapfire_fsr_sites::HttpStore::new(&base);
    let e = fetch_missing(&config, &bare).unwrap_err().to_string();
    assert!(e.contains("401"), "{e}");

    let store = snapfire_fsr_sites::HttpStore::new(&base).header("x-token", "t");
    let fetched = fetch_missing(&config, &store).unwrap();
    assert_eq!(fetched[0].version, "1.0.0");
    let host = snapfire_fsr_sites::mount_all_with(Host::from_config(config).unwrap(), &store).unwrap().build().unwrap();
    assert!(host.report().app.routes.iter().any(|r| r.0 == "/billing"));
  }

  #[test]
  fn an_http_store_answering_404_is_a_version_it_does_not_hold() {
    let root = dir("http404");
    std::fs::create_dir_all(root.join("served")).unwrap();
    let base = serve(root.join("served"));
    shell(&root, &format!("store = \"{base}\"\n[sites.billing]\nartifact = \"billing@9.9.9\"\n"));
    let config = Config::load(&root).unwrap();
    let store = snapfire_fsr_sites::HttpStore::new(&base).header("x-token", "t");
    let e = fetch_missing(&config, &store).unwrap_err().to_string();
    assert!(e.contains("holds no billing at 9.9.9"), "{e}");
  }
}

#[test]
fn a_reload_after_the_row_moves_fetches_the_new_version() {
  let root = dir("moves");
  packed(&root.join("archives"), "1.0.0");
  shell(&root, "store = \"archives\"\n[sites.billing]\nartifact = \"billing@1.0.0\"\n");
  let builder = snapfire_fsr_sites::mountable(snapfire_fsr_sites::mount_all(Host::from(&root).unwrap()).unwrap());
  let host = builder.build().unwrap();
  assert_eq!(host.report().sites[0].version, "1.0.0");

  packed(&root.join("archives"), "1.1.0");
  shell(&root, "store = \"archives\"\n[sites.billing]\nartifact = \"billing@1.1.0\"\n");
  host.reload().unwrap();
  assert_eq!(host.report().sites[0].version, "1.1.0");
  assert!(root.join("sites/billing/1.1.0").is_dir());
}
