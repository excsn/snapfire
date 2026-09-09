//! `fsr bundle` and the check it runs first. A bundle is a thing about to be
//! shipped, so a finding stops it before anything is written.

use std::path::PathBuf;

use snapfire_fsr_cli::{bundle, BuildError};

static NEXT: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);

/// An application `fsr bundle` can read, with `toml` deciding whether doctor
/// finds anything in it.
fn app(toml: &str) -> PathBuf {
  let nanos = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
  let n = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
  let dir = std::env::temp_dir().join(format!("fsr-bundle-{}-{n}-{nanos}", std::process::id()));
  std::fs::create_dir_all(dir.join("app/generated/contracts")).unwrap();
  std::fs::create_dir_all(dir.join("app/public")).unwrap();
  std::fs::write(dir.join("app/public/app.css"), "a{}\n").unwrap();
  std::fs::write(
    dir.join("app.toml"),
    // a case that opens `[document]` supplies the title, since the table may
    // appear only once
    {
      let document = if toml.contains("[document]") { "" } else { "[document]\ntitle = \"t\"\n" };
      format!("[app]\ndir = \"app\"\n[session]\nkey = \"k\"\n{document}[[static]]\nroute = \"/static\"\ndir = \"public\"\n{toml}")
    },
  )
  .unwrap();
  std::fs::write(dir.join("app/generated/plan.sexp"), "(plan 2)\n").unwrap();
  dir
}

#[test]
fn a_finding_stops_the_bundle_before_anything_is_written() {
  // an origin is wanted once the deployment names its hosts
  let dir = app("[server]\nhosts = [\"example.com\"]\n");
  let out = dir.join("dist");
  match bundle::run(&dir, &out) {
    Err(BuildError::Doctor(report)) => {
      assert!(!report.is_clean());
      assert!(report.to_string().contains("canonical"), "{report}");
    }
    Err(other) => panic!("{other}"),
    Ok(_) => panic!("bundled over a finding"),
  }
  assert!(!out.exists(), "the bundle was written over a finding");
}

/// The error carries the findings rather than a summary, so the command prints
/// what to do rather than that something is wrong.
#[test]
fn the_refusal_carries_the_remedy() {
  let dir = app("[server]\nhosts = [\"example.com\"]\n");
  let Err(e) = bundle::run(&dir, &dir.join("dist")) else { panic!("bundled over a finding") };
  let text = e.to_string();
  assert!(text.contains("`[document] origin`"), "{text}");
  assert!(text.contains("set `[document] origin`"), "{text}");
}

/// `--no-doctor` is the caller that means to bundle anyway.
#[test]
fn the_check_can_be_skipped() {
  let dir = app("[server]\nhosts = [\"example.com\"]\n");
  let out = dir.join("dist");
  bundle::run_checked(&dir, &out, false).expect("bundles with the check off");
  assert!(out.join("serve/static/app.css").is_file(), "the static root did not land");
}

/// An application with nothing to report bundles without being asked twice.
#[test]
fn a_clean_application_bundles() {
  let dir = app("[server]\nhosts = [\"example.com\"]\n[document]\ntitle = \"t\"\norigin = \"https://example.com\"\n");
  let out = dir.join("dist");
  let bundled = bundle::run(&dir, &out).expect("bundles");
  assert_eq!(bundled.out, out);
  assert!(out.join("serve/static/app.css").is_file());
}

/// The defect the layout exists to make impossible. A static root outside the
/// project used to be joined onto `--out`, where its `..` segments resolved
/// outside it; on this machine that reached the filesystem root.
#[test]
fn a_static_root_outside_the_project_is_written_inside_the_output() {
  let dir = app("[[static]]\nroute = \"/static/js\"\ndir = \"../../shared/client/dist\"\n");
  // The project moves one level down so the root it points outside of is a
  // directory the test owns, which is where a monorepo's shared client sits.
  let base = dir.with_extension("base");
  let project = base.join("project");
  std::fs::create_dir_all(&base).unwrap();
  std::fs::rename(&dir, &project).unwrap();
  let dir = project;
  let escapes = base.join("shared/client/dist");
  std::fs::create_dir_all(&escapes).unwrap();
  std::fs::write(escapes.join("main.js"), "export {}\n").unwrap();

  let out = dir.join("dist");
  let bundled = bundle::run_checked(&dir, &out, false).expect("bundles");
  assert!(out.join("serve/static/js/main.js").is_file());
  assert!(bundled.served.iter().any(|(route, at)| route == "/static/js" && at == "serve/static/js"));

  // Every file written is under the output directory and the directory the
  // configuration pointed outside the project holds only what it started with.
  for entry in walk(&out) {
    assert!(entry.starts_with(&out), "{} is outside {}", entry.display(), out.display());
  }
  assert_eq!(walk(&escapes).len(), 1);

  // The tree names the moved path back, so the host resolves it from the
  // application directory the tree carries.
  let layer = std::fs::read_to_string(out.join("config/bundle.toml")).unwrap();
  assert!(layer.contains(r#"dir = "../serve/static/js""#), "{layer}");
}

fn walk(dir: &std::path::Path) -> Vec<PathBuf> {
  let mut out = Vec::new();
  let Ok(entries) = std::fs::read_dir(dir) else { return out };
  for entry in entries.flatten() {
    let path = entry.path();
    if path.is_dir() {
      out.extend(walk(&path));
    } else {
      out.push(path);
    }
  }
  out
}
