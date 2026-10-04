//! A page's own state, handlers and effects move into an island beside it.

use std::sync::atomic::{AtomicU32, Ordering};

use snapfire_fsr_lower::component::ComponentSet;

static NEXT: AtomicU32 = AtomicU32::new(0);

fn set(files: &[(&str, &str)]) -> ComponentSet {
  let n = NEXT.fetch_add(1, Ordering::Relaxed);
  let dir = std::env::temp_dir().join(format!("fsr_extract_{}_{n}", std::process::id()));
  let _ = std::fs::remove_dir_all(&dir);
  for (name, source) in files {
    let path = dir.join(name);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, source).unwrap();
  }
  ComponentSet::new(&dir)
}

const TALK: &str = r#"import { useState } from "react";
import { Link } from "@snapfire/fsr-client/react";

export default function Talk({ talk, alongside }: { talk: { title: string }; alongside: { id: string; title: string }[] }) {
  const [open, setOpen] = useState(true);
  return (
    <article>
      <h2>{talk.title}</h2>
      <section className="alongside">
        <button onClick={() => setOpen(!open)}>{open ? "Hide" : "Show"}</button>
        {open ? <ul>{alongside.map((other) => <li key={other.id}><Link href={`/talk/${other.id}`}>{other.title}</Link></li>)}</ul> : null}
      </section>
    </article>
  );
}
"#;

#[test]
fn a_section_holding_state_moves_with_it_and_the_page_stays_composition() {
  let mut set = set(&[("routes/talk/page.tsx", TALK)]);
  let extracted = set.extract_route("routes/talk/page.tsx#default").unwrap().expect("the page holds state");
  assert_eq!(extracted.island, "routes/talk/page.island0.tsx#default");
  assert_eq!(extracted.holds, vec!["open", "setOpen"]);
  assert!(extracted.source.contains("export default function TalkIsland({ alongside }: any)"), "{}", extracted.source);
  assert!(extracted.source.contains("const [open, setOpen] = useState(true);"), "{}", extracted.source);
  assert!(extracted.source.contains("<section className=\"alongside\">"), "{}", extracted.source);
  assert!(!extracted.source.contains("<h2>"), "only the section moves: {}", extracted.source);
  let page = &extracted.page.1;
  assert!(page.contains("import SfIsland from \"./page.island0.tsx\";"), "{page}");
  assert!(page.contains("<SfIsland alongside={alongside} />"), "{page}");
  assert!(!page.contains("useState(true)"), "{page}");
  set.lower("routes/talk/page.tsx#default").unwrap();
  let (_, page) = set.components.iter().find(|(m, _)| m == "routes/talk/page.tsx#default").unwrap();
  assert!(!page.owner.hydrates(), "the page is composition");
  let (_, island) = set.components.iter().find(|(m, _)| m == "routes/talk/page.island0.tsx#default").unwrap();
  assert!(island.owner.hydrates(), "the island is React's");
}

#[test]
fn a_handler_inside_a_loop_takes_the_whole_loop_so_its_parameter_stays_inside() {
  let page = r#"import { useState } from "react";
export default function List({ rows }: { rows: string[] }) {
  const [picked, setPicked] = useState("");
  return (
    <main>
      <h1>rows</h1>
      <ul className="rows">{rows.map((row) => <li key={row}><button onClick={() => setPicked(row)}>{row}</button></li>)}</ul>
    </main>
  );
}
"#;
  let mut set = set(&[("routes/page.tsx", page)]);
  let extracted = set.extract_route("routes/page.tsx#default").unwrap().unwrap();
  assert!(extracted.source.contains("<ul className=\"rows\">"), "the unit is the list, outside the callback: {}", extracted.source);
  assert!(extracted.page.1.contains("<SfIsland rows={rows} />"), "{}", extracted.page.1);
  assert!(extracted.page.1.contains("<h1>rows</h1>"), "{}", extracted.page.1);
}

#[test]
fn a_page_with_nothing_for_the_browser_is_left_alone() {
  let mut set = set(&[("routes/page.tsx", "export default function Plain({ n }: { n: number }) {\n  return <p>{n}</p>;\n}\n")]);
  assert!(set.extract_route("routes/page.tsx#default").unwrap().is_none());
}

#[test]
fn state_that_reaches_a_layout_slot_keeps_the_layout_a_root_and_says_why() {
  let layout = r#"import { useState } from "react";
export default function Shell({ children, drawer }: { children: unknown; drawer: unknown }) {
  const [open, setOpen] = useState(false);
  return <div className={open ? "open" : ""} onClick={() => setOpen(!open)}>{children}{drawer}</div>;
}
"#;
  let mut set = set(&[("routes/layout.tsx", layout)]);
  set.slots.push(("routes/layout.tsx#default".to_owned(), vec!["drawer".to_owned()]));
  let why = set.extract_route("routes/layout.tsx#default").err().expect("kept");
  assert!(why.contains("`drawer`"), "{why}");
}
