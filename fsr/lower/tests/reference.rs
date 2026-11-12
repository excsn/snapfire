//! The standard library reference the guides carry between `<!-- fsr:reference -->` and `<!-- /fsr:reference -->`, written from the table by `meaning::reference`. The list of every built-in chapter 901 carries between `<!-- fsr:members -->` and `<!-- /fsr:members -->` is written from `standard_members.tsv`. `FSR_WRITE_REFERENCE=1` rewrites both.

use std::path::{Path, PathBuf};

const OPEN: &str = "<!-- fsr:reference -->\n";
const CLOSE: &str = "<!-- /fsr:reference -->";

fn documents() -> Vec<PathBuf> {
  let fsr = Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
  vec![fsr.join("cli/README.USAGE.md"), fsr.join("docs/guide/102-components-the-server-renders.md")]
}

#[test]
fn each_guide_carries_the_reference_the_table_writes() {
  let write = std::env::var_os("FSR_WRITE_REFERENCE").is_some_and(|v| v == "1");
  let reference = snapfire_fsr_lower::meaning::reference();
  let mut stale = Vec::new();
  for path in documents() {
    let text = std::fs::read_to_string(&path).unwrap();
    let start = text.find(OPEN).unwrap_or_else(|| panic!("{} has no {}", path.display(), OPEN.trim())) + OPEN.len();
    let end = start + text[start..].find(CLOSE).unwrap_or_else(|| panic!("{} has no {CLOSE}", path.display()));
    if text[start..end] == reference {
      continue;
    }
    if write {
      std::fs::write(&path, format!("{}{reference}{}", &text[..start], &text[end..])).unwrap();
    } else {
      stale.push(path.display().to_string());
    }
  }
  assert!(stale.is_empty(), "these differ from the table; FSR_WRITE_REFERENCE=1 rewrites them:\n{}", stale.join("\n"));
}

const ROWS: &str = include_str!("standard_members.tsv");
const MEMBERS_OPEN: &str = "<!-- fsr:members -->\n";
const MEMBERS_CLOSE: &str = "<!-- /fsr:members -->";

/// The rows as the guide lists them: a table of each global's members and what a loader and a component do with each.
fn members_list() -> String {
  let cell = |class: &str| match class {
    "lowered" | "server" => "server",
    "browser" => "browser",
    "refused" => "refused",
    "throws" => "throws, as in node",
    "awaits" => "loader only",
    other => panic!("no word for `{other}`"),
  };
  let mut out = String::new();
  let mut current = "";
  for line in ROWS.lines().filter(|line| !line.starts_with('#')) {
    let fields: Vec<&str> = line.split('\t').collect();
    let (global, member, loader, component) = (fields[0], fields[1], fields[2], fields[3]);
    if global != current {
      out.push_str(&format!("\n### {global}\n\n| Member | Loader | Component |\n| --- | --- | --- |\n"));
      current = global;
    }
    out.push_str(&format!("| `{member}` | {} | {} |\n", cell(loader), cell(component)));
  }
  out.push('\n');
  out
}

#[test]
fn the_guide_lists_every_member_as_the_table_does() {
  let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../docs/guide/901-javascripts-built-ins.md");
  let text = std::fs::read_to_string(&path).unwrap();
  let start = text.find(MEMBERS_OPEN).expect("the chapter marks where its list starts") + MEMBERS_OPEN.len();
  let end = start + text[start..].find(MEMBERS_CLOSE).expect("the chapter marks where its list ends");
  let list = members_list();
  if text[start..end] == list {
    return;
  }
  assert!(std::env::var_os("FSR_WRITE_REFERENCE").is_some_and(|v| v == "1"), "{} lists other members than the table; FSR_WRITE_REFERENCE=1 rewrites it", path.display());
  std::fs::write(&path, format!("{}{list}{}", &text[..start], &text[end..])).unwrap();
}
