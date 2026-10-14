use std::path::PathBuf;

use snapfire_fsr_ir::Tmpl;
use snapfire_fsr_lower::component::ComponentSet;

fn app(tag: &str, files: &[(&str, &str)]) -> PathBuf {
  let dir = std::env::temp_dir().join(format!("fsr_islands_{}_{tag}", std::process::id()));
  let _ = std::fs::remove_dir_all(&dir);
  for (name, source) in files {
    let path = dir.join(name);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, source).unwrap();
  }
  dir
}

const TIPS: &str = r#"import { island } from "@snapfire/fsr-client/react";
export function TipList({ tips }: { tips: string[] }) {
  return <ul>{tips.map((t) => <li key={t}>{t}</li>)}</ul>;
}
export const Tips = island(TipList, { when: "idle" });
"#;

fn island_in(tmpl: &Tmpl) -> Option<(&str, Option<&str>, Option<&str>)> {
  match tmpl {
    Tmpl::Island { module, when, mode, .. } => Some((module, when.as_deref(), mode.as_deref())),
    Tmpl::Element { children, .. } | Tmpl::Fragment(children) => children.iter().find_map(island_in),
    _ => None,
  }
}

#[test]
fn an_island_alias_imported_from_another_file_places_the_island_it_declares() {
  let dir = app(
    "alias",
    &[
      ("src/ui/Tips.tsx", TIPS),
      ("routes/a/page.tsx", "import { Tips } from \"../../src/ui/Tips\";\nexport default function A({ tips }: { tips: string[] }) {\n  return <main><Tips tips={tips} /></main>;\n}\n"),
    ],
  );
  let mut set = ComponentSet::new(&dir);
  set.lower("routes/a/page.tsx#default").unwrap();
  let (_, page) = set.components.iter().find(|(m, _)| m == "routes/a/page.tsx#default").unwrap();
  assert_eq!(island_in(&page.render), Some(("src/ui/Tips.tsx#TipList", Some("idle"), None)), "{:?}", page.render);
  assert!(set.components.iter().any(|(m, _)| m == "src/ui/Tips.tsx#TipList"), "the aliased component lowered as its own module");
}

#[test]
fn an_alias_reached_through_a_namespace_import_places_the_island_too() {
  let dir = app(
    "namespace",
    &[
      ("src/ui/Tips.tsx", TIPS),
      ("routes/a/page.tsx", "import * as Ui from \"../../src/ui/Tips\";\nexport default function A({ tips }: { tips: string[] }) {\n  return <main><Ui.Tips tips={tips} /></main>;\n}\n"),
    ],
  );
  let mut set = ComponentSet::new(&dir);
  set.lower("routes/a/page.tsx#default").unwrap();
  let (_, page) = set.components.iter().find(|(m, _)| m == "routes/a/page.tsx#default").unwrap();
  assert_eq!(island_in(&page.render), Some(("src/ui/Tips.tsx#TipList", Some("idle"), None)), "{:?}", page.render);
}

#[test]
fn a_bad_alias_in_the_imported_file_is_refused_at_the_placement() {
  let dir = app(
    "bad",
    &[
      ("src/ui/Tips.tsx", "import { island } from \"@snapfire/fsr-client/react\";\nexport function TipList() {\n  return <ul />;\n}\nexport const Tips = island(TipList, { when: \"sometimes\" });\n"),
      ("routes/a/page.tsx", "import { Tips } from \"../../src/ui/Tips\";\nexport default function A() {\n  return <main><Tips /></main>;\n}\n"),
    ],
  );
  let mut set = ComponentSet::new(&dir);
  let err = set.lower("routes/a/page.tsx#default").unwrap_err().to_string();
  assert!(err.contains("routes/a/page.tsx:3") && err.contains("`when`"), "{err}");
}

const NOTE: &str = "import type { ReactNode } from \"react\";\nexport function Note({ id, children }: { id?: string; children?: ReactNode }) {\n  return <h2 id={id}><a href={`#${id}`}>#</a> {children}</h2>;\n}\n";
const OPENER: &str = "import { useState } from \"react\";\nimport { Note } from \"./Note\";\nexport function Opener() {\n  const [open, setOpen] = useState(false);\n  return <div><button onClick={() => setOpen(!open)}>open</button>{open ? <Note id=\"later\">Later</Note> : null}</div>;\n}\n";
const SHOWS: &str = "import { Island } from \"@snapfire/fsr-client/react\";\nimport { Note } from \"../src/ui/Note\";\nimport { Opener } from \"../src/ui/Opener\";\nexport default function Page() {\n  return <main><Note id=\"now\">Now</Note><Island><Opener /></Island></main>;\n}\n";

/// A chunk the browser renders without the server's markup when a stateful
/// component opens a branch the server left closed: `Note` is rendered by the
/// page, so its chunk was proven, and the island placing it under `open`
/// found no table for it and threw.
#[test]
fn a_chunk_a_hydrating_component_can_place_keeps_its_fallback() {
  let dir = app("unproven", &[("routes/page.tsx", SHOWS), ("src/ui/Note.tsx", NOTE), ("src/ui/Opener.tsx", OPENER)]);
  let mut set = ComponentSet::new(&dir);
  set.lower("routes/page.tsx#default").unwrap();
  set.settle_proofs();
  let rewrite = set.rewrites.iter().find(|r| r.module == "src/ui/Note.tsx#Note").expect("Note is rewritten");
  assert!(!rewrite.chunks.is_empty() && rewrite.chunks.iter().all(|chunk| !chunk.proven), "{:?}", rewrite.chunks);
  let (_, source) = set.rewritten().into_iter().find(|(file, _)| file == "src/ui/Note.tsx").expect("a rewritten Note");
  assert!(source.contains("__sfh.c(0, (__sfHtml) => <a href={`#${id}`} dangerouslySetInnerHTML={__sfHtml} />, () => "), "the miss renders the original: {source}");
}

#[test]
fn a_chunk_only_pages_place_stays_proven() {
  let page = "import { Note } from \"../src/ui/Note\";\nexport default function Page() {\n  return <main><Note id=\"now\">Now</Note></main>;\n}\n";
  let dir = app("proven", &[("routes/page.tsx", page), ("src/ui/Note.tsx", NOTE)]);
  let mut set = ComponentSet::new(&dir);
  set.lower("routes/page.tsx#default").unwrap();
  set.settle_proofs();
  let rewrite = set.rewrites.iter().find(|r| r.module == "src/ui/Note.tsx#Note").expect("Note is rewritten");
  assert!(rewrite.chunks.iter().all(|chunk| chunk.proven), "{:?}", rewrite.chunks);
}
