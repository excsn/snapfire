//! `--asset-map`: a driving process defines every image and font and the
//! compiler rewrites each reference from the map, emitting none of them, and
//! reports what the map does not name so the driver can define it.

use std::fs;
use std::io::{BufRead, BufReader, Write};
use std::path::Path;
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};

mod common;
use common::{Fixture, get_snapfirec_cmd, run_snapfirec};

const MAP: &str = "assets.map.json";

fn write_map(root: &Path, rows: &[(&str, &str, Option<(u32, u32)>)]) {
  let assets: Vec<String> = rows
    .iter()
    .map(|(path, url, size)| match size {
      Some((w, h)) => format!(r#""{path}": {{"url": "{url}", "width": {w}, "height": {h}}}"#),
      None => format!(r#""{path}": {{"url": "{url}"}}"#),
    })
    .collect();
  fs::write(root.join(MAP), format!(r#"{{"version": 1, "assets": {{{}}}}}"#, assets.join(", "))).unwrap();
}

const WHOLE: &[(&str, &str, Option<(u32, u32)>)] = &[
  ("img/hero.png", "https://cdn.example.com/a/hero.11111111.png", Some((6, 4))),
  ("img/photo.jpg", "https://cdn.example.com/a/photo.22222222.jpg", Some((160, 320))),
  ("img/absent.png", "https://cdn.example.com/a/absent.33333333.png", Some((1, 1))),
  ("fonts/inter.woff2", "https://cdn.example.com/a/inter.44444444.woff2", None),
];

#[test]
fn a_map_rewrites_every_reference_from_its_row_and_emits_nothing() {
  let fixture = Fixture::new("asset-urls");
  write_map(fixture.root(), WHOLE);
  let mut cmd = get_snapfirec_cmd();
  run_snapfirec(cmd.arg("--root").arg(fixture.root()).args(["--public-path", "/static/js/app", "--asset-map", MAP]));

  let dist = fixture.root().join("dist");
  let module = fs::read_to_string(dist.join("ui/card.js")).unwrap();
  assert!(module.contains("src: \"https://cdn.example.com/a/hero.11111111.png\""), "{module}");
  assert!(module.contains("width: 6") && module.contains("height: 4"), "{module}");
  assert!(module.contains("width: 160") && module.contains("height: 320"), "the row's numbers, not the header's: {module}");
  assert!(module.contains("const inter = \"https://cdn.example.com/a/inter.44444444.woff2\""), "{module}");

  let css = fs::read_to_string(dist.join("theme.css")).unwrap();
  assert!(css.contains("url(\"https://cdn.example.com/a/inter.44444444.woff2#iefix\")"), "{css}");
  assert!(css.contains("https://cdn.example.com/a/hero.11111111.png"), "{css}");
  assert!(css.contains("https://cdn.example.com/a/absent.33333333.png"), "the map is believed over the disk: {css}");
  assert!(css.contains("https://cdn.example.com/x.png") && css.contains("./card.cur"), "{css}");

  assert!(!dist.join("img").exists() && !dist.join("fonts").exists(), "nothing was placed: {:?}", fs::read_dir(&dist).unwrap().map(|e| e.unwrap().file_name()).collect::<Vec<_>>());
  let facts: serde_json::Value = serde_json::from_str(&fs::read_to_string(dist.join(".snapfire-build.json")).unwrap()).unwrap();
  assert_eq!(facts["assets"].as_array().unwrap().len(), 0, "{}", facts["assets"]);
  let outputs: Vec<&str> = facts["outputs"].as_array().unwrap().iter().map(|v| v.as_str().unwrap()).collect();
  assert!(!outputs.iter().any(|o| o.starts_with("img/") || o.starts_with("fonts/")), "{outputs:?}");
}

#[test]
fn a_reference_the_map_does_not_name_fails_the_source_by_path() {
  let fixture = Fixture::new("asset-urls");
  write_map(fixture.root(), &WHOLE[..1]);
  let mut cmd = get_snapfirec_cmd();
  let output = cmd.arg("--root").arg(fixture.root()).args(["--asset-map", MAP]).output().unwrap();
  assert!(!output.status.success());
  let stderr = String::from_utf8_lossy(&output.stderr);
  assert!(stderr.contains(r#""input/ui/card.ts" names an asset the map does not define: "input/img/photo.jpg", "input/fonts/inter.woff2""#), "{stderr}");
  assert!(stderr.contains(r#""input/theme.css" names an asset the map does not define: "input/fonts/inter.woff2", "input/img/absent.png""#), "{stderr}");
  assert!(!fixture.root().join("dist/ui/card.js").exists(), "a source waiting on the map is not written");
}

#[test]
fn a_map_of_another_version_is_refused() {
  let fixture = Fixture::new("asset-urls");
  fs::write(fixture.root().join(MAP), r#"{"version": 9, "assets": {}}"#).unwrap();
  let mut cmd = get_snapfirec_cmd();
  let output = cmd.arg("--root").arg(fixture.root()).args(["--asset-map", MAP]).output().unwrap();
  assert!(!output.status.success());
  let stderr = String::from_utf8_lossy(&output.stderr);
  assert!(stderr.contains("is version 9, which this snapfirec does not read (it reads 1)"), "{stderr}");
}

struct Driven {
  child: Child,
  stdin: Option<ChildStdin>,
  stdout: BufReader<ChildStdout>,
}

impl Drop for Driven {
  fn drop(&mut self) {
    drop(self.stdin.take());
    let _ = self.child.kill();
    let _ = self.child.wait();
  }
}

impl Driven {
  fn start(root: &Path) -> Self {
    let mut cmd: Command = get_snapfirec_cmd();
    let mut child = cmd
      .arg("--root")
      .arg(root)
      .args(["--driven", "--public-path", "/static/js/app", "--asset-map", MAP])
      .stdin(Stdio::piped())
      .stdout(Stdio::piped())
      .stderr(Stdio::null())
      .spawn()
      .expect("failed to start the compiler");
    let stdin = child.stdin.take();
    let stdout = BufReader::new(child.stdout.take().unwrap());
    let mut driven = Driven { child, stdin, stdout };
    assert_eq!(driven.line(), "snapfirec: driven 2", "the hello comes before anything else");
    driven
  }

  fn line(&mut self) -> String {
    let mut line = String::new();
    let read = self.stdout.read_line(&mut line).unwrap();
    assert!(read > 0, "the compiler closed its output");
    line.trim_end().to_owned()
  }

  fn send(&mut self, lines: &[&str]) {
    let stdin = self.stdin.as_mut().unwrap();
    for line in lines {
      writeln!(stdin, "{line}").unwrap();
    }
    stdin.flush().unwrap();
  }

  /// Reads until the batch ends or the compiler asks for the map: the status
  /// line, or the paths it listed.
  fn next(&mut self) -> Answer {
    loop {
      let text = self.line();
      match text.as_str() {
        "snapfirec: rebuilt" => return Answer::Rebuilt,
        "snapfirec: failed" => return Answer::Failed,
        "snapfirec: references" => {
          let mut paths = Vec::new();
          loop {
            let path = self.line();
            if path.is_empty() {
              return Answer::References(paths);
            }
            paths.push(path);
          }
        }
        _ => {}
      }
    }
  }
}

#[derive(Debug, PartialEq)]
enum Answer {
  Rebuilt,
  Failed,
  References(Vec<String>),
}

#[test]
fn driven_reports_what_the_map_does_not_name_and_compiles_it_once_mapped() {
  let fixture = Fixture::new("asset-urls");
  write_map(fixture.root(), &WHOLE[..1]);
  let mut driven = Driven::start(fixture.root());

  assert_eq!(driven.next(), Answer::References(vec!["fonts/inter.woff2".into(), "img/absent.png".into(), "img/photo.jpg".into()]), "under the root directory, as the map keys them");
  assert!(!fixture.root().join("dist/ui/card.js").exists(), "nothing waiting on the map was written");
  write_map(fixture.root(), WHOLE);
  driven.send(&["mapped"]);
  assert_eq!(driven.next(), Answer::Rebuilt);
  let module = fs::read_to_string(fixture.root().join("dist/ui/card.js")).unwrap();
  assert!(module.contains("https://cdn.example.com/a/photo.22222222.jpg"), "{module}");
  let css = fs::read_to_string(fixture.root().join("dist/theme.css")).unwrap();
  assert!(css.contains("https://cdn.example.com/a/inter.44444444.woff2#iefix"), "{css}");
  assert!(!fixture.root().join("dist/img").exists(), "still nothing placed");

  let extra = fixture.root().join("input/img/extra.png");
  fs::copy(fixture.root().join("input/img/hero.png"), &extra).unwrap();
  let card = fixture.root().join("input/ui/card.ts");
  let mut source = fs::read_to_string(&card).unwrap();
  source.push_str("import extra from \"../img/extra.png\";\nexport const more = extra;\n");
  fs::write(&card, source).unwrap();
  driven.send(&["input/ui/card.ts", ""]);
  assert_eq!(driven.next(), Answer::References(vec!["img/extra.png".into()]));
  let mut rows = WHOLE.to_vec();
  rows.push(("img/extra.png", "https://cdn.example.com/a/extra.55555555.png", Some((6, 4))));
  write_map(fixture.root(), &rows);
  driven.send(&["mapped"]);
  assert_eq!(driven.next(), Answer::Rebuilt);
  let module = fs::read_to_string(fixture.root().join("dist/ui/card.js")).unwrap();
  assert!(module.contains("https://cdn.example.com/a/extra.55555555.png"), "{module}");

  fs::write(&card, "import ghost from \"../img/ghost.png\";\nexport const g = ghost;\n").unwrap();
  driven.send(&["input/ui/card.ts", ""]);
  assert_eq!(driven.next(), Answer::References(vec!["img/ghost.png".into()]));
  driven.send(&["mapped"]);
  assert_eq!(driven.next(), Answer::Failed, "the driver answered without defining it");
}
