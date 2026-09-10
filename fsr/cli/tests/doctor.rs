//! `fsr doctor` over applications built to a temporary directory. Each check
//! is exercised both ways: the condition it reports, and the shape that must
//! stay quiet, since a check that fires on a healthy application is worse than
//! no check at all.

use std::path::{Path, PathBuf};

use snapfire_fsr_cli::doctor;

static NEXT: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);

/// A minimal application: an `app.toml`, a plan and whatever else a case adds.
fn app(toml: &str, files: &[(&str, &str)]) -> PathBuf {
  let nanos = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
  let n = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
  let dir = std::env::temp_dir().join(format!("fsr-doctor-{}-{n}-{nanos}", std::process::id()));
  std::fs::create_dir_all(dir.join("app/generated")).unwrap();
  std::fs::create_dir_all(dir.join("app/routes")).unwrap();
  // a case that opens `[document]` itself supplies the title, since the table
  // may appear only once
  let document = if toml.contains("[document]") { "" } else { "[document]\ntitle = \"t\"\n" };
  std::fs::write(dir.join("app.toml"), format!("[app]\ndir = \"app\"\n[session]\nkey = \"k\"\n{document}{toml}")).unwrap();
  std::fs::write(dir.join("app/generated/plan.sexp"), PLAN).unwrap();
  for (name, source) in files {
    let path = dir.join(name);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, source).unwrap();
  }
  dir
}

const PLAN: &str = "(plan 2)\n(route / (node 0 shell#document))\n";

/// A plan whose loader reads `ctx.host`.
const HOST_PLAN: &str = "(plan 2)\n(route / (node 0 shell#document (source index)))\n(source index lowered (module routes/page.loader.ts) (body (ret (obj (h (host))))))\n";

fn findings(dir: &Path) -> Vec<String> {
  doctor::run(dir).expect("doctor runs").findings.into_iter().map(|f| f.check.to_owned()).collect()
}

fn report(dir: &Path) -> String {
  doctor::run(dir).expect("doctor runs").to_string()
}

#[test]
fn a_healthy_application_reports_nothing() {
  let dir = app("", &[]);
  let out = doctor::run(&dir).expect("runs");
  assert!(out.is_clean(), "{out}");
  assert_eq!(out.clean.len(), 12, "{out}");
  assert!(out.to_string().contains("nothing to report"), "{out}");
}

#[test]
fn an_origin_is_wanted_once_the_deployment_names_its_hosts() {
  let dir = app("[server]\nhosts = [\"example.com\"]\n", &[]);
  assert_eq!(findings(&dir), vec!["canonical"]);
  assert!(report(&dir).contains("example.com"), "{}", report(&dir));

  let with = app("[server]\nhosts = [\"example.com\"]\n[document]\ntitle = \"t\"\norigin = \"https://example.com\"\n", &[]);
  assert!(doctor::run(&with).unwrap().is_clean());
}

#[test]
fn an_origin_is_wanted_once_the_application_prerenders() {
  let dir = app("[server]\nprerender = \"dist\"\n", &[]);
  assert_eq!(findings(&dir), vec!["canonical"]);
  assert!(report(&dir).contains("prerenders"), "{}", report(&dir));
}

#[test]
fn a_body_reading_the_host_against_an_empty_list_is_reported() {
  let dir = app("", &[("app/generated/plan.sexp", HOST_PLAN)]);
  assert_eq!(findings(&dir), vec!["ctx.host"]);
  assert!(report(&dir).contains("always answers null"), "{}", report(&dir));

  let listed = app("[server]\nhosts = [\"example.com\"]\n[document]\ntitle = \"t\"\norigin = \"https://example.com\"\n", &[("app/generated/plan.sexp", HOST_PLAN)]);
  assert!(doctor::run(&listed).unwrap().is_clean(), "{}", report(&listed));
}

#[test]
fn a_locale_with_no_catalog_is_reported() {
  let dir = app("[locales]\nsupported = [\"en\", \"fr\"]\n", &[("app/locales/en.toml", "[a]\nb = \"c\"\n")]);
  assert_eq!(findings(&dir), vec!["locales"]);
  let out = report(&dir);
  assert!(out.contains("fr") && !out.contains("names en"), "{out}");

  let both = app("[locales]\nsupported = [\"en\", \"fr\"]\n", &[("app/locales/en.toml", "[a]\nb = \"c\"\n"), ("app/locales/fr.toml", "[a]\nb = \"c\"\n")]);
  assert!(doctor::run(&both).unwrap().is_clean(), "{}", report(&both));
}

#[test]
fn a_plan_older_than_its_sources_is_reported() {
  let dir = app("", &[]);
  // touching a route after the plan is what a build leaves behind when it is
  // not re-run, so the file's own time is the check
  std::thread::sleep(std::time::Duration::from_millis(20));
  std::fs::write(dir.join("app/routes/page.tsx"), "export default function P() { return <p/>; }\n").unwrap();
  assert_eq!(findings(&dir), vec!["stale"]);
  assert!(report(&dir).contains("`routes/`"), "{}", report(&dir));
}

#[test]
fn a_missing_plan_is_reported_rather_than_failing_every_check() {
  let dir = app("", &[]);
  std::fs::remove_file(dir.join("app/generated/plan.sexp")).unwrap();
  let out = doctor::run(&dir).expect("doctor still runs without a plan");
  assert_eq!(out.findings.iter().map(|f| f.check).collect::<Vec<_>>(), vec!["stale"]);
  assert!(out.to_string().contains("does not exist"), "{out}");
}

#[test]
fn an_import_map_naming_an_unvendored_package_is_reported() {
  let dir = app(
    "[document]\ntitle = \"t\"\nimport_map = \"importmap.json\"\n",
    &[("app/importmap.json", r#"{"imports":{"react":"/static/js/vendor/react/react.js"}}"#)],
  );
  assert_eq!(findings(&dir), vec!["vendor"]);
  assert!(report(&dir).contains("react"), "{}", report(&dir));

  let vendored = app(
    "[document]\ntitle = \"t\"\nimport_map = \"importmap.json\"\n",
    &[
      ("app/importmap.json", r#"{"imports":{"react":"/static/js/vendor/react/react.js"}}"#),
      ("app/vendor/react/react.js", "export default 1;\n"),
    ],
  );
  assert!(doctor::run(&vendored).unwrap().is_clean(), "{}", report(&vendored));
}

#[test]
fn an_import_map_that_is_absent_or_malformed_is_reported() {
  let missing = app("[document]\ntitle = \"t\"\nimport_map = \"importmap.json\"\n", &[]);
  assert_eq!(findings(&missing), vec!["vendor"]);
  assert!(report(&missing).contains("does not exist"), "{}", report(&missing));

  let broken = app("[document]\ntitle = \"t\"\nimport_map = \"importmap.json\"\n", &[("app/importmap.json", "{ not json")]);
  assert_eq!(findings(&broken), vec!["vendor"]);
  assert!(report(&broken).contains("is not JSON"), "{}", report(&broken));
}

#[test]
fn the_islands_render_mode_without_an_island_is_reported() {
  let dir = app("[server]\nrender = \"islands\"\n", &[]);
  assert_eq!(findings(&dir), vec!["render"]);
  assert!(report(&dir).contains("carries no island"), "{}", report(&dir));

  let island = "(plan 2)\n(route / (node 0 shell#document))\n(component routes/page.tsx#default (render (el div () (island src/C.tsx#C 0 nil nil ()))))\n";
  let with = app("[server]\nrender = \"islands\"\n", &[("app/generated/plan.sexp", island)]);
  assert!(doctor::run(&with).unwrap().is_clean(), "{}", report(&with));
}

#[test]
fn several_findings_are_all_reported_and_counted() {
  let dir = app(
    "[server]\nhosts = [\"example.com\"]\nrender = \"islands\"\n[locales]\nsupported = [\"fr\"]\n",
    &[],
  );
  let out = doctor::run(&dir).expect("runs");
  assert_eq!(out.findings.iter().map(|f| f.check).collect::<Vec<_>>(), vec!["canonical", "locales", "render"]);
  assert!(out.to_string().contains("3 of 12 checks"), "{out}");
  assert!(!out.is_clean());
}

/// Every finding names what to do, since a report that only says what is wrong
/// is a report nobody acts on.
#[test]
fn every_finding_carries_a_remedy() {
  let dir = app(
    "[server]\nhosts = [\"example.com\"]\nrender = \"islands\"\n[document]\ntitle = \"t\"\nimport_map = \"importmap.json\"\n[locales]\nsupported = [\"fr\"]\n",
    &[],
  );
  let out = doctor::run(&dir).expect("runs");
  assert!(!out.findings.is_empty());
  for finding in &out.findings {
    assert!(!finding.remedy.trim().is_empty(), "{} has no remedy", finding.check);
    assert!(!finding.what.trim().is_empty(), "{} says nothing", finding.check);
  }
}

#[test]
fn a_directory_with_no_configuration_is_an_error_rather_than_a_finding() {
  let dir = std::env::temp_dir().join(format!("fsr-doctor-none-{}", std::process::id()));
  std::fs::create_dir_all(&dir).unwrap();
  assert!(doctor::run(&dir).is_err());
}

/// A shell with one mounted site, and the site's own artifact beside it.
fn shell(sites_toml: &str, site_toml: &str, site_files: &[(&str, &str)]) -> PathBuf {
  let nanos = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
  let n = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
  let root = std::env::temp_dir().join(format!("fsr-doctor-shell-{}-{n}-{nanos}", std::process::id()));
  std::fs::create_dir_all(root.join("app/generated")).unwrap();
  std::fs::create_dir_all(root.join("app/routes")).unwrap();
  std::fs::write(root.join("app.toml"), format!("[app]\ndir = \"app\"\n[document]\ntitle = \"t\"\n[session]\nkey = \"k\"\n[sites]\nroot = \"sites\"\n{sites_toml}")).unwrap();
  std::fs::write(root.join("app/generated/plan.sexp"), PLAN).unwrap();

  let site = root.join("sites/billing/1.0.0");
  std::fs::create_dir_all(site.join("app/generated")).unwrap();
  std::fs::create_dir_all(site.join("app/routes")).unwrap();
  std::fs::write(site.join("app.toml"), format!("[app]\ndir = \"app\"\n[document]\ntitle = \"b\"\n[session]\nkey = \"k\"\n[site]\nname = \"billing\"\nat = \"/billing\"\n{site_toml}")).unwrap();
  std::fs::write(site.join("app/generated/plan.sexp"), PLAN).unwrap();
  for (name, source) in site_files {
    let path = site.join(name);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, source).unwrap();
  }
  root
}

const MOUNT: &str = "[sites.billing]\nartifact = \"billing@1.0.0\"\n";

#[test]
fn a_mount_that_pins_no_hash_is_reported() {
  let dir = shell(MOUNT, "", &[]);
  let out = doctor::run(&dir).expect("runs");
  let text = out.to_string();
  assert!(text.contains("pins no hash"), "{text}");
  assert!(text.contains("fsr sites hash"), "{text}");
}

/// A mount naming a path rather than a version is a linked working tree, which
/// changes on every build. Asking that to be pinned would be asking for a pin
/// that is stale by the next one.
#[test]
fn a_linked_working_tree_is_not_asked_to_pin() {
  let dir = shell("[sites.billing]\nartifact = \"sites/billing/1.0.0\"\n", "", &[]);
  let text = doctor::run(&dir).expect("runs").to_string();
  assert!(!text.contains("pins no hash"), "{text}");
}

#[test]
fn a_site_whose_plan_is_older_than_its_routes_is_reported() {
  let dir = shell(MOUNT, "", &[]);
  std::thread::sleep(std::time::Duration::from_millis(20));
  std::fs::write(dir.join("sites/billing/1.0.0/app/routes/page.tsx"), "export default function P() { return <p/>; }\n").unwrap();
  let text = doctor::run(&dir).expect("runs").to_string();
  assert!(text.contains("the site `billing` has a plan older than `routes/`"), "{text}");
}

#[test]
fn a_site_with_no_plan_is_reported() {
  let dir = shell(MOUNT, "", &[]);
  std::fs::remove_file(dir.join("sites/billing/1.0.0/app/generated/plan.sexp")).unwrap();
  let text = doctor::run(&dir).expect("runs").to_string();
  assert!(text.contains("the site `billing` has no plan"), "{text}");
}

/// An artifact under the root that the table never names is what an install
/// leaves behind.
#[test]
fn an_unmounted_artifact_under_the_root_is_reported() {
  let dir = shell(MOUNT, "", &[]);
  std::fs::create_dir_all(dir.join("sites/billing/0.9.0/app")).unwrap();
  std::fs::create_dir_all(dir.join("sites/invoices/2.0.0/app")).unwrap();
  let text = doctor::run(&dir).expect("runs").to_string();
  assert!(text.contains("billing@0.9.0") && text.contains("invoices@2.0.0"), "{text}");
  assert!(text.contains("2 sits under the sites root"), "{text}");
}

/// A shell whose mount points at nothing is a host that will not start, and
/// saying so before the deploy is the point.
#[test]
fn a_mount_pointing_at_nothing_is_reported_as_a_refusal_to_start() {
  let dir = shell("[sites.missing]\nartifact = \"missing@1.0.0\"\n", "", &[]);
  let text = doctor::run(&dir).expect("runs").to_string();
  assert!(text.contains("will refuse to start"), "{text}");
}

#[test]
fn a_shell_with_no_sites_table_reports_nothing_about_sites() {
  let dir = app("", &[]);
  let out = doctor::run(&dir).expect("runs");
  assert!(out.clean.contains(&"sites"), "{out}");
}

#[test]
fn a_static_root_with_no_directory_is_reported() {
  let dir = app("[[static]]\nroute = \"/assets\"\ndir = \"public\"\n", &[]);
  assert_eq!(findings(&dir), vec!["statics"]);
  assert!(report(&dir).contains("/assets"), "{}", report(&dir));

  let there = app("[[static]]\nroute = \"/assets\"\ndir = \"public\"\n", &[("app/public/x.css", "a{}")]);
  assert!(doctor::run(&there).unwrap().is_clean(), "{}", report(&there));
}

/// A pin the artifact has moved out from under is the common sites failure,
/// and it gets the answer for that rather than the generic one.
#[test]
fn a_pin_the_artifact_no_longer_matches_is_reported_with_how_to_repin() {
  let dir = shell("[sites.billing]\nartifact = \"billing@1.0.0\"\nhash = \"0000000000000000\"\n", "", &[]);
  let out = doctor::run(&dir).expect("runs");
  let text = out.to_string();
  assert!(text.contains("will refuse to start"), "{text}");
  assert!(text.contains("pinned 0000000000000000"), "{text}");
  assert!(text.contains("moved under its pin"), "{text}");
  assert!(text.contains("fsr sites pin"), "{text}");
}

/// A mount pointing at nothing is a different fault, so it gets different advice.
#[test]
fn a_mount_pointing_at_nothing_is_not_told_to_repin() {
  let dir = shell("[sites.missing]\nartifact = \"missing@1.0.0\"\n", "", &[]);
  let text = doctor::run(&dir).expect("runs").to_string();
  assert!(text.contains("will refuse to start"), "{text}");
  assert!(!text.contains("moved under its pin"), "{text}");
  assert!(text.contains("fsr sites unlink"), "{text}");
}

/// A client's document is imported at boot and no other check names it, so a
/// tree that would not carry it is the one thing this check is for.
#[test]
fn a_client_document_that_would_not_ship_is_reported() {
  let dir = app("[clients.billing]\nbase_url = \"https://b\"\n", &[]);
  assert_eq!(findings(&dir), vec!["tree"]);
  let out = report(&dir);
  assert!(out.contains("app/clients/billing.openapi.json"), "{out}");

  let dir = app(
    "[clients.billing]\nbase_url = \"https://b\"\n",
    &[("app/clients/billing.openapi.json", "{}")],
  );
  std::fs::write(dir.join("app/generated/plan.sexp"), PLAN).unwrap();
  assert!(findings(&dir).is_empty(), "{}", report(&dir));
}

/// A route that would place a file outside the tree is refused by the layout,
/// and the doctor says so before `fsr bundle` gets there.
#[test]
fn a_route_that_leaves_the_tree_is_reported() {
  let dir = app("[[static]]\nroute = \"/../outside\"\ndir = \"public\"\n", &[("app/public/a.css", "a{}")]);
  assert_eq!(findings(&dir), vec!["tree"]);
  let out = report(&dir);
  assert!(out.contains("cannot be laid out") && out.contains("outside the tree"), "{out}");
}

/// A static root returns rather than falling through, so a route under one can
/// never run. The boot refuses a route two plans both claim and is silent
/// about this pair.
#[test]
fn a_static_root_swallowing_a_route_is_reported() {
  const CART: &str = "(plan 2)\n(route / (node 0 shell#document))\n(route /cart (node 1 shell#document))\n";
  let dir = app(
    "[[static]]\nroute = \"/cart\"\ndir = \"public\"\n",
    &[("app/public/a.css", "a{}"), ("app/generated/plan.sexp", CART)],
  );
  assert_eq!(findings(&dir), vec!["shadow"]);
  let out = report(&dir);
  assert!(out.contains("/cart") && out.contains("never runs"), "{out}");

  // A static root on a prefix of its own takes nothing from the plan.
  let dir = app(
    "[[static]]\nroute = \"/assets\"\ndir = \"public\"\n",
    &[("app/public/a.css", "a{}"), ("app/generated/plan.sexp", CART)],
  );
  assert!(findings(&dir).is_empty(), "{}", report(&dir));
}

/// Only an `[auth]` provider writes a token into custody, so a client asking
/// for one without a provider sends nothing.
#[test]
fn a_bearer_client_with_no_auth_provider_is_reported() {
  let bearer = |toml: &str, files: &[(&str, &str)]| {
    let dir = app(toml, files);
    std::fs::write(dir.join("app/generated/plan.sexp"), PLAN).unwrap();
    (findings(&dir), report(&dir))
  };

  let (found, out) = bearer(
    "[clients.ledger]\nbase_url = \"https://l\"\nbearer = true\n",
    &[("app/clients/ledger.openapi.json", OPENAPI)],
  );
  assert_eq!(found, vec!["bearer"], "{out}");
  assert!(out.contains("ledger") && out.contains("in custody"), "{out}");

  let (found, out) = bearer(
    "[auth]\nprovider = \"file\"\nusers = \"users.toml\"\n[clients.ledger]\nbase_url = \"https://l\"\nbearer = true\n",
    &[("app/clients/ledger.openapi.json", OPENAPI), ("users.toml", "[alice]\npassword = \"x\"\n")],
  );
  assert!(!found.contains(&"bearer".to_owned()), "{out}");

  // A client that asks for no token is not asked about a provider.
  let (found, out) = bearer(
    "[clients.ledger]\nbase_url = \"https://l\"\n",
    &[("app/clients/ledger.openapi.json", OPENAPI)],
  );
  assert!(!found.contains(&"bearer".to_owned()), "{out}");
}

/// A cache tag is a string two sides spell, and a mismatch is a stale page
/// rather than an error.
#[test]
fn a_cache_tag_only_one_side_names_is_reported() {
  let contract = |cached: &str, writes: &str| {
    format!(
      r#"{{"services":{{"ledger":{{"methods":{{"list":{{"params":[],"returns":{{"named":"Row"}}{cached}}},"pay":{{"params":[],"returns":{{"named":"Row"}}{writes}}}}}}}}}}}"#
    )
  };
  let of = |cached: &str, writes: &str| {
    let dir = app("", &[("app/generated/contracts/ledger.json", &contract(cached, writes))]);
    std::fs::write(dir.join("app/generated/plan.sexp"), PLAN).unwrap();
    (findings(&dir), report(&dir))
  };

  // A typo either way leaves a written tag nothing caches.
  let (found, out) = of(r#","cache":{"ttl":"30s","tags":["invoices"]}"#, r#","writes":["invoice"]"#);
  assert_eq!(found, vec!["cache.tags"], "{out}");
  assert!(out.contains("invoice is dropped by a call and cached by none"), "{out}");

  let (found, out) = of(r#","cache":{"ttl":"30s","tags":["invoice"]}"#, r#","writes":["invoices"]"#);
  assert_eq!(found, vec!["cache.tags"], "{out}");
  assert!(out.contains("invoices is dropped by a call and cached by none"), "{out}");

  // Spelled the same on both sides, nothing to say.
  let (found, out) = of(r#","cache":{"ttl":"30s","tags":["invoices"]}"#, r#","writes":["invoices"]"#);
  assert!(found.is_empty(), "{out}");

  // A read-only service expires by ttl and writes nothing, which is a design
  // rather than a defect.
  let (found, out) = of(r#","cache":{"ttl":"30s","tags":["invoices"]}"#, "");
  assert!(found.is_empty(), "{out}");
}

const OPENAPI: &str = r#"{"openapi":"3.0.0","info":{"title":"ledger","version":"1"},"paths":{}}"#;
