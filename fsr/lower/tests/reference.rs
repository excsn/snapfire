//! The standard library reference the guides carry between `<!-- fsr:reference -->` and `<!-- /fsr:reference -->`, written from the table by `meaning::reference`. `FSR_WRITE_REFERENCE=1` rewrites it in each document.

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
