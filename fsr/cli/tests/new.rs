use std::path::PathBuf;

use snapfire_fsr_cli::new::{create, NewOptions, SiteScaffold};
use snapfire_fsr_cli::{build, Options};

static NEXT: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);

fn root(tag: &str) -> PathBuf {
  let n = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
  let dir = std::env::temp_dir().join(format!("fsr-cli-new-{}-{n}-{tag}", std::process::id()));
  let _ = std::fs::remove_dir_all(&dir);
  dir
}

fn offline() -> NewOptions {
  NewOptions { fetch: false, ..NewOptions::default() }
}

#[test]
fn a_scaffolded_project_builds_with_every_module_lowered() {
  let root = root("builds");
  let created = create(&root, offline()).unwrap();
  assert!(created.written.iter().any(|p| p.ends_with("config/app.toml")), "{:?}", created.written);
  assert!(created.written.iter().any(|p| p.ends_with("app/src/main.ts")), "{:?}", created.written);

  let built = build(&root.join("app"), &Options::beside(&root.join("app"))).unwrap();
  assert_eq!(built.report.routes.len(), 1, "{}", built.report);
  assert_eq!(built.report.sources, vec![("$root".to_owned(), "routes/page.loader.ts".to_owned())], "{}", built.report);
  let residue: Vec<&(String, String, String)> = built.report.components.iter().filter(|(_, how, _)| how != "lowered").collect();
  assert!(residue.is_empty(), "every module lowers, but {residue:?}\n{}", built.report);
}

#[test]
fn the_scaffold_names_the_react_it_did_not_fetch() {
  let created = create(&root("offline"), offline()).unwrap();
  assert!(created.vendored.is_empty());
  assert!(created.next.iter().any(|s| s.contains("fsr add") && s.contains("react-dom@18.3.1/client")), "{:?}", created.next);
  assert!(created.next.last().unwrap().starts_with("fsr dev"), "{:?}", created.next);
}

#[test]
fn a_second_run_refuses_rather_than_overwriting() {
  let root = root("twice");
  create(&root, offline()).unwrap();
  let again = create(&root, offline()).unwrap_err().to_string();
  assert!(again.contains("already exists"), "{again}");
}

#[test]
fn a_body_calls_the_head_helpers_before_anything_is_generated() {
  let root = root("head");
  create(&root, offline()).unwrap();
  let app = root.join("app");
  std::fs::write(
    app.join("routes/page.loader.ts"),
    "import type { Ctx } from \"@snapfire/fsr\";\nimport { canonical, og } from \"@snapfire/fsr/head\";\n\nexport async function load(_ctx: Ctx<\"/\">) {\n  return { greeting: \"hi\" };\n}\n\nexport const meta = ({ data }: { data: { greeting: string } }) => ({\n  title: data.greeting,\n  head: [og(\"title\", data.greeting), canonical(\"/\")],\n});\n",
  )
  .unwrap();
  assert!(!app.join("generated").exists(), "nothing is generated yet");

  let built = build(&app, &Options::beside(&app)).unwrap();
  assert_eq!(built.report.sources, vec![("$root".to_owned(), "routes/page.loader.ts".to_owned())], "{}", built.report);
}

#[test]
fn a_shell_is_scaffolded_with_a_table_to_mount_into() {
  let root = root("shell");
  create(&root, NewOptions { shell: true, ..offline() }).unwrap();
  let toml = std::fs::read_to_string(root.join("config/app.toml")).unwrap();
  assert!(toml.contains("[sites]"), "{toml}");
  assert!(!toml.contains("[site]\n"), "a shell is not a site: {toml}");
}

#[test]
fn a_site_is_scaffolded_at_the_path_it_is_given() {
  let root = root("site");
  create(&root, NewOptions { site: Some(SiteScaffold { at: "/docs".to_owned(), name: None, into: None }), ..offline() }).unwrap();
  let toml = std::fs::read_to_string(root.join("config/app.toml")).unwrap();
  assert!(toml.contains("[site]"), "{toml}");
  assert!(toml.contains("at = \"/docs\""), "{toml}");
  assert!(!toml.contains("shell = "), "no shell was named: {toml}");
}

#[test]
fn into_writes_both_halves_and_the_site_names_the_shell() {
  let base = root("into");
  let shell = base.join("portal");
  let site = base.join("docs");
  create(&shell, NewOptions { shell: true, ..offline() }).unwrap();
  let created = create(&site, NewOptions { site: Some(SiteScaffold { at: "/docs".to_owned(), name: None, into: Some(shell.clone()) }), ..offline() }).unwrap();

  let linked = created.linked.expect("the project was linked into the shell");
  assert_eq!(linked.name, "docs");
  assert_eq!(linked.artifact, "../docs");
  assert!(std::fs::read_to_string(shell.join("config/app.toml")).unwrap().contains("[sites.docs]"));
  let toml = std::fs::read_to_string(site.join("config/app.toml")).unwrap();
  assert!(toml.contains("shell = \"../portal/app/generated/shell.json\""), "{toml}");
  assert_eq!(toml.matches("[site]").count(), 1, "the section is written once: {toml}");
}

#[test]
fn a_scaffold_refused_by_the_host_s_rules_leaves_no_project() {
  for (at, name) in [("docs", None), ("/docs/", None), ("/ok", Some("BadName".to_owned()))] {
    let root = root("refused");
    let options = NewOptions { site: Some(SiteScaffold { at: at.to_owned(), name, into: None }), ..offline() };
    assert!(create(&root, options).is_err(), "{at} was accepted");
    assert!(!root.join("config").exists(), "{at} left a project behind");
  }
}

#[test]
fn a_shell_that_is_also_a_site_is_refused() {
  let root = root("both");
  let options = NewOptions { shell: true, site: Some(SiteScaffold { at: "/x".to_owned(), name: None, into: None }), ..offline() };
  let refused = create(&root, options).unwrap_err().to_string();
  assert!(refused.contains("a site cannot mount sites"), "{refused}");
}
