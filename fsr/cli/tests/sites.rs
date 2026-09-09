use std::path::{Path, PathBuf};

use snapfire_fsr_cli::new::{create, NewOptions};
use snapfire_fsr_cli::sites;

static NEXT: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);

fn root(tag: &str) -> PathBuf {
  let n = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
  let dir = std::env::temp_dir().join(format!("fsr-cli-sites-{}-{n}-{tag}", std::process::id()));
  let _ = std::fs::remove_dir_all(&dir);
  dir
}

/// A shell and a site scaffolded side by side, so the relative paths a link
/// writes have somewhere to point.
fn pair(tag: &str) -> (PathBuf, PathBuf) {
  let base = root(tag);
  let shell = base.join("shell");
  let site = base.join("handbook");
  create(&shell, NewOptions { fetch: false, ..NewOptions::default() }).unwrap();
  create(&site, NewOptions { fetch: false, ..NewOptions::default() }).unwrap();
  (shell, site)
}

fn config(project: &Path) -> String {
  std::fs::read_to_string(project.join("config/app.toml")).unwrap()
}

#[test]
fn a_link_writes_both_halves_and_the_paths_point_at_each_other() {
  let (shell, site) = pair("link");
  let linked = sites::link(&shell, &site, "/examples/handbook", None).unwrap();

  assert_eq!(linked.name, "handbook");
  assert_eq!(linked.artifact, "../handbook");
  assert_eq!(linked.shell_json, "../shell/app/generated/shell.json");
  assert!(!linked.site_kept);

  let site_toml = config(&site);
  assert!(site_toml.contains("[site]"), "{site_toml}");
  assert!(site_toml.contains("name = \"handbook\""), "{site_toml}");
  assert!(site_toml.contains("at = \"/examples/handbook\""), "{site_toml}");
  assert!(site_toml.contains("shell = \"../shell/app/generated/shell.json\""), "{site_toml}");
  assert!(config(&shell).contains("[sites.handbook]"), "{}", config(&shell));

  let rows = sites::list(&shell).unwrap();
  assert_eq!(rows.len(), 1);
  assert_eq!(rows[0].name, "handbook");
  assert_eq!(rows[0].at.as_deref(), Some("/examples/handbook"));
  assert_eq!(rows[0].note.as_deref(), None, "the row resolves");

  assert!(linked.next.iter().any(|c| c.starts_with("fsr build") && c.contains("shell")), "{:?}", linked.next);
}

/// Every line is back except the blank ones at the end of the file, which the
/// removal collapses.
#[test]
fn an_unlink_takes_both_halves_back_out() {
  let (shell, site) = pair("unlink");
  let before_shell = config(&shell);
  let before_site = config(&site);

  sites::link(&shell, &site, "/handbook", None).unwrap();
  let unlinked = sites::unlink(&shell, "handbook", false).unwrap();

  assert_eq!(unlinked.name, "handbook");
  assert!(unlinked.site_config.is_some(), "the site's own [site] came out too");
  assert_eq!(config(&shell).trim_end(), before_shell.trim_end(), "the shell's configuration is back as it was");
  assert_eq!(config(&site).trim_end(), before_site.trim_end(), "the site's configuration is back as it was");
  assert!(sites::list(&shell).unwrap().is_empty());
}

#[test]
fn keep_site_leaves_the_site_a_site() {
  let (shell, site) = pair("keep");
  sites::link(&shell, &site, "/handbook", None).unwrap();
  let unlinked = sites::unlink(&shell, "handbook", true).unwrap();

  assert!(unlinked.site_config.is_none());
  assert!(config(&site).contains("[site]"), "{}", config(&site));
  assert!(!config(&shell).contains("[sites.handbook]"), "{}", config(&shell));
}

#[test]
fn a_shell_that_is_a_site_mounts_nothing() {
  let (shell, site) = pair("nested");
  sites::link(&shell, &site, "/handbook", None).unwrap();

  let third = root("nested-third");
  create(&third, NewOptions { fetch: false, ..NewOptions::default() }).unwrap();
  let refused = sites::link(&site, &third, "/deeper", None).unwrap_err().to_string();
  assert!(refused.contains("a site cannot mount sites"), "{refused}");
}

#[test]
fn a_name_the_table_holds_is_refused_before_anything_is_written() {
  let (shell, site) = pair("twice");
  sites::link(&shell, &site, "/handbook", None).unwrap();
  let after_first = config(&shell);

  let refused = sites::link(&shell, &site, "/elsewhere", Some("handbook")).unwrap_err().to_string();
  assert!(refused.contains("already mounted"), "{refused}");
  assert_eq!(config(&shell), after_first, "a refused link writes nothing");
}

#[test]
fn a_site_already_naming_something_else_is_refused() {
  let (shell, site) = pair("conflict");
  sites::link(&shell, &site, "/handbook", None).unwrap();
  sites::unlink(&shell, "handbook", true).unwrap();

  let refused = sites::link(&shell, &site, "/other", Some("other")).unwrap_err().to_string();
  assert!(refused.contains("already names site `handbook`"), "{refused}");
}

#[test]
fn the_host_s_own_rules_on_a_name_and_a_path_are_the_command_s() {
  let (shell, site) = pair("rules");
  for (at, name) in [("handbook", None), ("/handbook/", None), ("/{id}", None), ("/ok", Some("Handbook"))] {
    let refused = sites::link(&shell, &site, at, name).unwrap_err().to_string();
    assert!(refused.contains("must be"), "{at} {name:?}: {refused}");
  }
  assert!(!config(&shell).contains("[sites."), "{}", config(&shell));
}

#[test]
fn unlinking_a_name_the_table_does_not_hold_says_what_it_holds() {
  let (shell, site) = pair("missing");
  sites::link(&shell, &site, "/handbook", None).unwrap();
  let refused = sites::unlink(&shell, "nope", false).unwrap_err().to_string();
  assert!(refused.contains("`nope` is not mounted"), "{refused}");
  assert!(refused.contains("handbook"), "it names what is mounted: {refused}");
}

/// A shell whose table mounts a versioned artifact in its own cache.
fn pinnable() -> PathBuf {
  let nanos = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
  let root = std::env::temp_dir().join(format!("fsr-pin-{}-{nanos}", std::process::id()));
  std::fs::create_dir_all(root.join("app/generated")).unwrap();
  std::fs::write(root.join("app.toml"), "[app]\ndir = \"app\"\n[document]\ntitle = \"t\"\n[session]\nkey = \"k\"\n[sites]\nroot = \"sites\"\n\n[sites.billing]\nartifact = \"billing@1.0.0\"\n").unwrap();
  std::fs::write(root.join("app/generated/plan.sexp"), "(plan 2)\n").unwrap();

  let site = root.join("sites/billing/1.0.0");
  std::fs::create_dir_all(site.join("app/generated")).unwrap();
  std::fs::write(site.join("app.toml"), "[app]\ndir = \"app\"\n[document]\ntitle = \"b\"\n[session]\nkey = \"k\"\n[site]\nname = \"billing\"\nat = \"/billing\"\n").unwrap();
  std::fs::write(site.join("app/generated/plan.sexp"), "(plan 2)\n").unwrap();
  root
}

#[test]
fn pinning_writes_the_hash_into_the_mount() {
  let shell = pinnable();
  let pinned = snapfire_fsr_cli::sites::pin(&shell, None).expect("pins");
  assert_eq!(pinned.len(), 1);
  assert_eq!(pinned[0].name, "billing");
  assert!(pinned[0].was.is_none());
  assert!(pinned[0].moved());

  let toml = std::fs::read_to_string(shell.join("app.toml")).unwrap();
  assert!(toml.contains(&format!("hash = \"{}\"", pinned[0].hash)), "{toml}");
  assert!(toml.contains("[sites.billing]"), "{toml}");
}

/// Pinning twice is one pin: the second run has nothing to write.
#[test]
fn pinning_again_holds_when_nothing_moved() {
  let shell = pinnable();
  let first = snapfire_fsr_cli::sites::pin(&shell, None).expect("pins");
  let again = snapfire_fsr_cli::sites::pin(&shell, None).expect("pins");
  assert_eq!(again[0].was.as_deref(), Some(first[0].hash.as_str()));
  assert!(!again[0].moved(), "the hash moved without the artifact moving");
  assert_eq!(std::fs::read_to_string(shell.join("app.toml")).unwrap().matches("hash = ").count(), 1);
}

/// A changed artifact repins to the new content rather than appending a second key.
#[test]
fn repinning_replaces_the_hash_it_had() {
  let shell = pinnable();
  let before = snapfire_fsr_cli::sites::pin(&shell, None).expect("pins")[0].hash.clone();
  std::fs::write(shell.join("sites/billing/1.0.0/app/generated/plan.sexp"), "(plan 2)\n(route / (node 0 shell#document))\n").unwrap();
  let after = snapfire_fsr_cli::sites::pin(&shell, None).expect("pins");
  assert_eq!(after[0].was.as_deref(), Some(before.as_str()));
  assert_ne!(after[0].hash, before);
  let toml = std::fs::read_to_string(shell.join("app.toml")).unwrap();
  assert_eq!(toml.matches("hash = ").count(), 1, "{toml}");
  assert!(toml.contains(&after[0].hash), "{toml}");
}

/// A mount naming a path is a working tree, so it is never pinned.
#[test]
fn a_path_mount_is_not_pinned() {
  let shell = pinnable();
  let toml = std::fs::read_to_string(shell.join("app.toml")).unwrap();
  std::fs::write(shell.join("app.toml"), toml.replace("artifact = \"billing@1.0.0\"", "artifact = \"sites/billing/1.0.0\"")).unwrap();
  assert!(snapfire_fsr_cli::sites::pin(&shell, None).expect("pins").is_empty());
  assert!(!std::fs::read_to_string(shell.join("app.toml")).unwrap().contains("hash = "));
}

#[test]
fn pinning_one_mount_leaves_the_others() {
  let shell = pinnable();
  let toml = std::fs::read_to_string(shell.join("app.toml")).unwrap();
  std::fs::write(shell.join("app.toml"), format!("{toml}\n[sites.other]\nartifact = \"other@2.0.0\"\n")).unwrap();
  let site = shell.join("sites/other/2.0.0");
  std::fs::create_dir_all(site.join("app/generated")).unwrap();
  std::fs::write(site.join("app.toml"), "[app]\ndir = \"app\"\n[document]\ntitle = \"o\"\n[session]\nkey = \"k\"\n[site]\nname = \"other\"\nat = \"/other\"\n").unwrap();
  std::fs::write(site.join("app/generated/plan.sexp"), "(plan 2)\n").unwrap();

  let pinned = snapfire_fsr_cli::sites::pin(&shell, Some("billing")).expect("pins");
  assert_eq!(pinned.len(), 1);
  let toml = std::fs::read_to_string(shell.join("app.toml")).unwrap();
  assert_eq!(toml.matches("hash = ").count(), 1, "{toml}");
}

#[test]
fn pinning_a_name_the_table_does_not_mount_says_so() {
  let shell = pinnable();
  let e = snapfire_fsr_cli::sites::pin(&shell, Some("nope")).unwrap_err().to_string();
  assert!(e.contains("`nope` is not mounted"), "{e}");
}

// --------------------------------------------------- a shell over the wire

use std::io::{BufRead, BufReader, Write};
use std::net::TcpListener;

/// A server that answers each request with the next canned response, so the
/// client is exercised without a host behind it. Returns the address and the
/// requests it saw.
fn serving(answers: Vec<(u16, String)>) -> (String, std::sync::mpsc::Receiver<String>) {
  let listener = TcpListener::bind("127.0.0.1:0").expect("a port");
  let addr = listener.local_addr().expect("an address").to_string();
  let (tx, rx) = std::sync::mpsc::channel();
  std::thread::spawn(move || {
    for (status, body) in answers {
      let Ok((stream, _)) = listener.accept() else { return };
      let mut reader = BufReader::new(&stream);
      let mut line = String::new();
      reader.read_line(&mut line).ok();
      let mut headers = String::new();
      loop {
        let mut header = String::new();
        if reader.read_line(&mut header).unwrap_or(0) <= 2 {
          break;
        }
        headers.push_str(&header);
      }
      tx.send(format!("{}{headers}", line.trim_end())).ok();
      let reason = if status == 200 { "OK" } else { "Conflict" };
      let out = format!(
        "HTTP/1.1 {status} {reason}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
      );
      (&stream).write_all(out.as_bytes()).ok();
    }
  });
  (addr, rx)
}

const SERVING_ONE: &str = r#"{"sites":[{"name":"billing","at":"/billing","version":"1.0.0","hash":"aaaa"}]}"#;

#[test]
fn a_header_is_name_and_value_or_it_is_refused() {
  assert_eq!(snapfire_fsr_cli::sites::header("Authorization: Bearer x").unwrap(), ("Authorization".to_owned(), "Bearer x".to_owned()));
  assert_eq!(snapfire_fsr_cli::sites::header("  X-Key :  v  ").unwrap(), ("X-Key".to_owned(), "v".to_owned()));
  for bad in ["nocolon", ": value", "name:", ""] {
    assert!(snapfire_fsr_cli::sites::header(bad).is_err(), "{bad:?} was taken as a header");
  }
}

#[test]
fn asking_an_instance_reports_what_it_serves() {
  let (addr, seen) = serving(vec![(200, SERVING_ONE.to_owned())]);
  let instance = snapfire_fsr_cli::sites::mounted(&addr, &[]).expect("asks");
  assert_eq!(instance.host, addr);
  assert_eq!(instance.sites.len(), 1);
  assert_eq!(instance.sites[0].name, "billing");
  assert_eq!(instance.sites[0].hash, "aaaa");
  assert!(seen.recv().expect("a request").starts_with("GET /__fsr/sites "));
}

/// Headers are forwarded as given: the command carries a proxy's credentials
/// and creates none of its own.
#[test]
fn headers_are_forwarded_to_the_instance() {
  let (addr, seen) = serving(vec![(200, SERVING_ONE.to_owned())]);
  let headers = vec![("Authorization".to_owned(), "Bearer tok".to_owned())];
  snapfire_fsr_cli::sites::mounted(&addr, &headers).expect("asks");
  let request = seen.recv().expect("a request").to_lowercase();
  assert!(request.contains("authorization: bearer tok"), "{request}");
}

#[test]
fn reloading_reports_what_the_instance_serves_now() {
  let (addr, seen) = serving(vec![(200, r#"{"reloaded":true,"sites":[{"name":"billing","at":"/billing","version":"1.0.0","hash":"aaaa"}]}"#.to_owned())]);
  let answers = snapfire_fsr_cli::sites::reload(&[addr], &[], false).expect("reloads");
  assert_eq!(answers.len(), 1);
  assert!(answers[0].ok());
  assert_eq!(answers[0].sites.as_ref().unwrap()[0].name, "billing");
  assert!(seen.recv().unwrap().starts_with("POST /__fsr/sites/reload "));
}

/// A 409 carries the reason, which is the whole value of the route over a signal.
#[test]
fn a_refusal_carries_the_reason() {
  let (addr, _seen) = serving(vec![(409, r#"{"reloaded":false,"error":"sites.billing: hash bbbb, pinned aaaa"}"#.to_owned())]);
  let answers = snapfire_fsr_cli::sites::reload(&[addr], &[], false).expect("asks");
  assert!(!answers[0].ok());
  assert_eq!(answers[0].refused.as_deref(), Some("sites.billing: hash bbbb, pinned aaaa"));
}

/// A refusal stops the fleet: carrying on ships what was refused to the rest.
#[test]
fn a_refusal_stops_before_the_next_instance() {
  let (first, _a) = serving(vec![(409, r#"{"reloaded":false,"error":"no"}"#.to_owned())]);
  let (second, seen) = serving(vec![(200, r#"{"reloaded":true,"sites":[]}"#.to_owned())]);
  let answers = snapfire_fsr_cli::sites::reload(&[first, second], &[], false).expect("asks");
  assert_eq!(answers.len(), 1, "the second instance was asked after a refusal");
  assert!(seen.recv_timeout(std::time::Duration::from_millis(300)).is_err(), "the second instance saw a request");
}

#[test]
fn all_carries_on_past_a_refusal() {
  let (first, _a) = serving(vec![(409, r#"{"reloaded":false,"error":"no"}"#.to_owned())]);
  let (second, _b) = serving(vec![(200, r#"{"reloaded":true,"sites":[]}"#.to_owned())]);
  let answers = snapfire_fsr_cli::sites::reload(&[first, second], &[], true).expect("asks");
  assert_eq!(answers.len(), 2);
  assert!(!answers[0].ok());
  assert!(answers[1].ok());
}

/// A route the host does not serve says why, since that is a build-time fact
/// rather than a wrong URL.
#[test]
fn a_missing_route_says_the_host_was_not_built_for_it() {
  let (addr, _seen) = serving(vec![(404, "not found".to_owned())]);
  let e = snapfire_fsr_cli::sites::reload(&[addr], &[], false).unwrap_err().to_string();
  assert!(e.contains("sites_reload"), "{e}");
  assert!(e.contains("sites mounter"), "{e}");
}

#[test]
fn the_table_is_compared_against_every_instance() {
  let shell = pinnable();
  let hash = snapfire_fsr_cli::sites::pin(&shell, None).expect("pins")[0].hash.clone();
  let agrees = format!(r#"{{"sites":[{{"name":"billing","at":"/billing","version":"1.0.0","hash":"{hash}"}}]}}"#);
  let lags = r#"{"sites":[{"name":"billing","at":"/billing","version":"0.9.0","hash":"old"}]}"#;
  let (a, _x) = serving(vec![(200, agrees)]);
  let (b, _y) = serving(vec![(200, lags.to_owned())]);
  let rows = snapfire_fsr_cli::sites::compare(&shell, &[a.clone(), b.clone()], &[]).expect("compares");
  assert_eq!(rows.len(), 1);
  assert!(!rows[0].agrees(), "a lagging instance was reported as agreeing");
  assert_eq!(rows[0].against.iter().map(|(h, _)| h.clone()).collect::<Vec<_>>(), vec![a, b]);
}

/// An instance that does not mount the site at all does not agree either.
#[test]
fn an_instance_missing_the_site_does_not_agree() {
  let shell = pinnable();
  snapfire_fsr_cli::sites::pin(&shell, None).expect("pins");
  let (addr, _x) = serving(vec![(200, r#"{"sites":[]}"#.to_owned())]);
  let rows = snapfire_fsr_cli::sites::compare(&shell, &[addr], &[]).expect("compares");
  assert!(!rows[0].agrees());
  assert!(rows[0].against[0].1.is_none());
}

#[test]
fn a_host_comes_from_the_flag_then_the_shells_listen() {
  let shell = pinnable();
  let given = vec!["a.internal:8080".to_owned()];
  assert_eq!(snapfire_fsr_cli::sites::hosts_for(Some(&shell), &given).unwrap(), given);
  assert_eq!(snapfire_fsr_cli::sites::hosts_for(Some(&shell), &[]).unwrap().len(), 1);
  assert!(snapfire_fsr_cli::sites::hosts_for(None, &[]).is_err());
}
