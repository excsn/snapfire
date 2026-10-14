use std::path::{Path, PathBuf};

use snapfire_fsr_host::config::Config;
use snapfire_fsr_sites::{hash_dir, pack, resolve, ArchiveStore, Cache, Listing, Manifest, TarStore};

fn dir(name: &str) -> PathBuf {
  let nanos = std::time::SystemTime::now()
    .duration_since(std::time::UNIX_EPOCH)
    .unwrap()
    .as_nanos();
  let dir = std::env::temp_dir().join(format!("fsr-sites-{name}-{}-{nanos}", std::process::id()));
  std::fs::create_dir_all(&dir).unwrap();
  dir
}

fn shell(sites: &str) -> PathBuf {
  let root = dir("shell");
  std::fs::create_dir_all(root.join("app/generated")).unwrap();
  std::fs::write(root.join("app/generated/plan.sexp"), "(plan 2)").unwrap();
  std::fs::write(
    root.join("app.toml"),
    format!("[app]\ndir = \"app\"\n[session]\nkey = \"k\"\n{sites}"),
  )
  .unwrap();
  root
}

/// A site artifact at `at`: the smallest tree `Config::load` accepts as a site,
/// with one file in each part the listing covers.
fn site(at: &Path, name: &str, prefix: &str) {
  std::fs::create_dir_all(at.join("app/generated")).unwrap();
  std::fs::create_dir_all(at.join("app/dist")).unwrap();
  std::fs::create_dir_all(at.join("app/styles")).unwrap();
  std::fs::write(at.join("app/generated/plan.sexp"), "(plan 2)").unwrap();
  std::fs::write(at.join("app/dist/main.js"), "export {}\n").unwrap();
  // `dist/` is a static root only once the build facts name a public path, so
  // a fixture without one is not a site the host would serve a bundle for.
  std::fs::write(
    at.join("app/dist/.snapfire-build.json"),
    r#"{"version":1,"publicPath":"/billing/static/js/app","entries":["src/main.js"]}"#,
  )
  .unwrap();
  std::fs::write(at.join("app/styles/site.css"), ".a{}\n").unwrap();
  std::fs::write(at.join("app/importmap.json"), r#"{"imports":{}}"#).unwrap();
  std::fs::write(
    at.join("app.toml"),
    format!("[app]\ndir = \"app\"\n[session]\nkey = \"k\"\n[site]\nname = \"{name}\"\nat = \"{prefix}\"\n"),
  )
  .unwrap();
}

#[test]
fn a_version_resolves_under_the_root_and_a_path_stands_alone() {
  let root = shell("[sites]\nroot = \"sites\"\n[sites.billing]\nartifact = \"billing@1.2.0\"\n[sites.reports]\nartifact = \"elsewhere/reports\"\n");
  site(&root.join("sites/billing/1.2.0"), "billing", "/billing");
  site(&root.join("elsewhere/reports"), "reports", "/reports");
  let config = Config::load(&root).unwrap();
  let resolved = resolve(&config).unwrap();
  assert_eq!(resolved.len(), 2);
  assert_eq!(resolved[0].name, "billing");
  assert_eq!(resolved[0].version, "1.2.0");
  assert!(resolved[0].artifact.ends_with("sites/billing/1.2.0"));
  assert_eq!(resolved[0].hash, hash_dir(&root.join("sites/billing/1.2.0")).unwrap());
  assert_eq!(resolved[1].version, "path");
  assert!(resolved[1].artifact.ends_with("elsewhere/reports"));
}

#[test]
fn a_pinned_hash_refuses_an_artifact_that_differs_and_a_version_needs_a_root() {
  let root = shell("[sites.billing]\nartifact = \"./billing\"\nhash = \"0000000000000000\"\n");
  site(&root.join("billing"), "billing", "/billing");
  let config = Config::load(&root).unwrap();
  let e = resolve(&config).unwrap_err().to_string();
  assert!(
    e.contains("sites.billing") && e.contains("pinned 0000000000000000"),
    "{e}"
  );
  let root = shell("[sites.billing]\nartifact = \"billing@1\"\n");
  let e = Config::load(&root).map(|_| ()).unwrap_err().to_string();
  assert!(e.contains("needs sites.root"), "{e}");
}

#[test]
fn the_hash_follows_shipped_content_and_ignores_what_does_not_ship() {
  let at = dir("hash-a");
  site(&at, "billing", "/billing");
  let first = hash_dir(&at).unwrap();

  std::fs::write(at.join(".hidden"), "ignored").unwrap();
  std::fs::create_dir_all(at.join("app/src")).unwrap();
  std::fs::write(at.join("app/src/main.ts"), "a build input").unwrap();
  std::fs::create_dir_all(at.join("content")).unwrap();
  std::fs::write(at.join("content/one.md"), "# one").unwrap();
  assert_eq!(
    hash_dir(&at).unwrap(),
    first,
    "a file outside every part does not move the hash"
  );

  std::fs::write(at.join("app/dist/main.js"), "export const a = 1\n").unwrap();
  assert_ne!(hash_dir(&at).unwrap(), first, "a file inside a part does");
}

#[test]
fn a_dot_file_inside_a_part_counts_toward_the_hash() {
  let at = dir("hash-dot");
  site(&at, "billing", "/billing");
  let first = hash_dir(&at).unwrap();
  std::fs::write(
    at.join("app/dist/.snapfire-build.json"),
    r#"{"version":1,"publicPath":"/billing/static/js/other","entries":["src/main.js"]}"#,
  )
  .unwrap();
  assert_ne!(
    hash_dir(&at).unwrap(),
    first,
    "the build facts decide what the host serves, so they are content"
  );
}

#[test]
fn a_working_tree_and_the_release_copied_out_of_it_hash_the_same() {
  let at = dir("hash-tree");
  site(&at, "billing", "/billing");
  std::fs::create_dir_all(at.join("app/routes")).unwrap();
  std::fs::write(at.join("app/routes/page.tsx"), "export default () => null").unwrap();
  std::fs::create_dir_all(at.join("app/types/react")).unwrap();
  std::fs::write(at.join("app/types/react/index.d.ts"), "declare module 'react'").unwrap();

  let release = dir("hash-release");
  let config = Config::load(&at).unwrap();
  for row in snapfire_fsr_sites::layout(&at, &config).unwrap().rows().unwrap() {
    let to = release.join(&row.path);
    std::fs::create_dir_all(to.parent().unwrap()).unwrap();
    std::fs::write(&to, row.bytes().unwrap()).unwrap();
  }
  assert_eq!(hash_dir(&release).unwrap(), hash_dir(&at).unwrap());

  // The tree lays out as itself, so laying it out again moves nothing.
  let again = dir("hash-again");
  let config = Config::load(&release).unwrap();
  for row in snapfire_fsr_sites::layout(&release, &config).unwrap().rows().unwrap() {
    let to = again.join(&row.path);
    std::fs::create_dir_all(to.parent().unwrap()).unwrap();
    std::fs::write(&to, row.bytes().unwrap()).unwrap();
  }
  assert_eq!(hash_dir(&again).unwrap(), hash_dir(&at).unwrap());
}

#[test]
fn a_pack_round_trips_through_an_install_and_repeats_as_a_hold() {
  let at = dir("pack-site");
  site(&at, "billing", "/billing");
  let out = dir("pack-out").join("billing-1.2.0.tar.gz");
  let manifest = pack(&at, "1.2.0", &out).unwrap();
  assert_eq!(manifest.name, "billing");
  assert_eq!(manifest.at, "/billing");
  assert_eq!(manifest.hash, hash_dir(&at).unwrap());
  assert_eq!(Manifest::read_archive(&out).unwrap(), manifest);

  let cache = Cache::new(dir("pack-cache"));
  let store = ArchiveStore { archive: out.clone() };
  let installed = cache.install(&store, "billing", "billing", "1.2.0", Some(2)).unwrap();
  assert!(!installed.held);
  assert_eq!(installed.hash, manifest.hash);
  assert_eq!(hash_dir(&installed.path).unwrap(), manifest.hash);
  assert_eq!(Manifest::read(&installed.path).unwrap().unwrap(), manifest);

  let again = cache.install(&store, "billing", "billing", "1.2.0", Some(2)).unwrap();
  assert!(again.held);
  assert_eq!(again.hash, manifest.hash);
}

#[test]
fn packing_the_same_tree_twice_writes_the_same_bytes() {
  let at = dir("pack-repeat");
  site(&at, "billing", "/billing");
  let out = dir("pack-repeat-out");
  pack(&at, "1.0.0", &out.join("a.tar.gz")).unwrap();
  pack(&at, "1.0.0", &out.join("b.tar.gz")).unwrap();
  assert_eq!(
    std::fs::read(out.join("a.tar.gz")).unwrap(),
    std::fs::read(out.join("b.tar.gz")).unwrap()
  );
}

#[test]
fn a_staged_tree_whose_bytes_differ_from_its_manifest_is_refused() {
  let at = dir("bad-site");
  site(&at, "billing", "/billing");
  let out = dir("bad-out").join("billing-1.0.0.tar.gz");
  let manifest = pack(&at, "1.0.0", &out).unwrap();

  let staged = dir("bad-staged").join("one");
  snapfire_fsr_sites::unpack(&out, &staged).unwrap();
  assert!(manifest.verify(&staged).is_ok());

  std::fs::write(staged.join("serve/billing/static/js/app/main.js"), "export const tampered = 1\n").unwrap();
  let e = manifest.verify(&staged).unwrap_err().to_string();
  assert!(e.contains("serve/billing/static/js/app/main.js") && e.contains("sha256"), "{e}");

  std::fs::write(staged.join("serve/billing/static/js/app/extra.js"), "export {}\n").unwrap();
  std::fs::write(staged.join("serve/billing/static/js/app/main.js"), "export {}\n").unwrap();
  let e = manifest.verify(&staged).unwrap_err().to_string();
  assert!(e.contains("serve/billing/static/js/app/extra.js") && e.contains("not listed"), "{e}");

  std::fs::remove_file(staged.join("serve/billing/static/js/app/extra.js")).unwrap();
  std::fs::remove_file(staged.join("serve/billing/static/css/site.css")).unwrap();
  let e = manifest.verify(&staged).unwrap_err().to_string();
  assert!(e.contains("serve/billing/static/css/site.css") && e.contains("absent"), "{e}");
}

#[test]
fn a_store_that_holds_no_version_stages_nothing() {
  let cache = Cache::new(dir("absent-cache"));
  let store = TarStore::new(dir("absent-store"));
  let e = cache
    .install(&store, "billing", "billing", "9.9.9", None)
    .unwrap_err()
    .to_string();
  assert!(e.contains("holds no billing at 9.9.9"), "{e}");
  assert_eq!(cache.sweep_staging().unwrap(), 0);
  assert!(cache.versions("billing").is_empty());
}

#[test]
fn a_sweep_keeps_the_newest_and_never_the_active_one() {
  let at = dir("sweep-site");
  site(&at, "billing", "/billing");
  let store_dir = dir("sweep-store");
  let cache = Cache::new(dir("sweep-cache"));
  for version in ["1.0.0", "1.1.0", "1.2.0"] {
    std::fs::write(at.join("app/dist/main.js"), format!("export const v = '{version}'\n")).unwrap();
    pack(&at, version, &store_dir.join(format!("billing-{version}.tar.gz"))).unwrap();
    cache
      .install(&TarStore::new(&store_dir), "billing", "billing", version, None)
      .unwrap();
    std::thread::sleep(std::time::Duration::from_millis(20));
  }
  assert_eq!(cache.versions("billing"), vec!["1.0.0", "1.1.0", "1.2.0"]);
  assert_eq!(cache.sweep("billing", 2, &[]).unwrap(), vec!["1.0.0"]);
  assert_eq!(cache.versions("billing"), vec!["1.1.0", "1.2.0"]);
  assert!(cache.sweep("billing", 1, &["1.1.0".to_owned()]).unwrap().is_empty());
  assert_eq!(cache.versions("billing"), vec!["1.1.0", "1.2.0"]);
}

#[test]
fn a_part_is_where_a_file_lands_rather_than_where_it_came_from() {
  let at = dir("parts");
  site(&at, "billing", "/billing");
  let config = Config::load(&at).unwrap();
  let parts = snapfire_fsr_sites::parts(&at, &config).unwrap();
  let held = |p: &str| parts.contains(&p.to_owned());
  assert!(held("config/app.toml"), "{parts:?}");
  assert!(held("config/bundle.toml"), "{parts:?}");
  assert!(held("app/generated/plan.sexp"), "{parts:?}");
  assert!(held("app/importmap.json"), "{parts:?}");
  // A static root is placed under the route it answers, so `dist/` and
  // `styles/` are named by their prefixes rather than by their directories.
  assert!(held("serve/billing/static/js/app"), "{parts:?}");
  assert!(held("serve/billing/static/css"), "{parts:?}");
  assert!(!held("app/dist"), "{parts:?}");
  assert!(!held("app/styles"), "{parts:?}");
  assert!(!parts.iter().any(|p| p.starts_with("app/types")), "{parts:?}");

  let listing = Listing::of(&at).unwrap();
  assert!(listing.entries.iter().all(|e| parts
    .iter()
    .any(|p| e.path == *p || e.path.starts_with(&format!("{p}/")))));
  assert_eq!(listing.hash(), hash_dir(&at).unwrap());
}

fn copy(from: &Path, to: &Path) {
  if from.is_dir() {
    std::fs::create_dir_all(to).unwrap();
    for entry in std::fs::read_dir(from).unwrap().flatten() {
      copy(&entry.path(), &to.join(entry.file_name()));
    }
  } else {
    std::fs::copy(from, to).unwrap();
  }
}
