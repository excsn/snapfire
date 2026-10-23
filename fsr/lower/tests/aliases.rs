//! A local bound to an element of a local array or to a field of a local is
//! that element or field: a write through it reaches the array or the root,
//! as it does in JavaScript.

use std::path::PathBuf;
use std::sync::Arc;

use snapfire_fsr_core::ValueMap;
use snapfire_fsr_ir::render::{prepare, Components};
use snapfire_fsr_ir::Interpreter;
use snapfire_fsr_lower::component::ComponentSet;

fn app(tag: &str, files: &[(&str, &str)]) -> PathBuf {
  let dir = std::env::temp_dir().join(format!("fsr_aliases_{}_{tag}", std::process::id()));
  let _ = std::fs::remove_dir_all(&dir);
  for (name, text) in files {
    let path = dir.join(name);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, text).unwrap();
  }
  dir
}

fn html(tag: &str, page: &str) -> Result<String, String> {
  let dir = app(tag, &[("routes/page.tsx", page)]);
  let mut set = ComponentSet::new(&dir);
  let result = set.lower("routes/page.tsx#default").map_err(|e| e.to_string());
  std::fs::remove_dir_all(&dir).unwrap();
  result?;
  set.settle_fills();
  if let Some((module, residue)) = set.browser_only.first() {
    return Err(format!("{module}: {}", residue.message));
  }
  let library: Components = set.components.iter().map(|(module, component)| (module.clone(), Arc::new(prepare(component)))).collect();
  let module = "routes/page.tsx#default";
  Interpreter::default().with_consts(Some(Arc::new(set.consts.clone()))).render_module(module, &library[module], &ValueMap::default(), &library).map(|r| r.html).map_err(|e| format!("{e:?}"))
}

const GROUPED: &str = r#"const ROWS = [{ group: "a", n: 1 }, { group: "a", n: 2 }, { group: "b", n: 3 }];

export default function Page() {
  return <main>{grouped(ROWS).map((g) => <p key={g.group}>{g.group}:{g.items.map((t) => t.n).join(",")}</p>)}</main>;
}

function grouped<T extends { group: string }>(rows: T[]): { group: string; items: T[] }[] {
  const groups: { group: string; items: T[] }[] = [];
  for (const t of rows) {
    const last = groups[groups.length - 1];
    if (last && last.group === t.group) {
      last.items.push(t);
    } else {
      groups.push({ group: t.group, items: [t] });
    }
  }
  return groups;
}
"#;

#[test]
fn a_push_through_the_last_element_of_a_local_array_reaches_the_array() {
  let html = html("grouped", GROUPED).unwrap();
  assert!(html.contains("<p>a:1,2</p><p>b:3</p>"), "{html}");
}

#[test]
fn a_field_written_through_an_element_is_written_in_the_array() {
  let page = r#"export default function Page() {
  const counts = [{ n: 0 }, { n: 0 }];
  const second = counts[1];
  second.n = second.n + 5;
  return <p>{counts.map((c) => c.n).join(",")} {second.n}</p>;
}
"#;
  let html = html("field", page).unwrap();
  assert!(html.contains("<p>0,5 5</p>"), "{html}");
}

#[test]
fn a_write_through_an_element_after_its_array_was_written_is_residue() {
  let page = r#"export default function Page() {
  const rows = [{ n: 1 }];
  const first = rows[0];
  rows.unshift({ n: 0 });
  first.n = 9;
  return <p>{rows.length}</p>;
}
"#;
  let err = html("stale", page).unwrap_err();
  assert!(err.contains("a write through `first`, an element of `rows`, after `rows` was written"), "{err}");
}

#[test]
fn an_index_that_is_not_a_position_reads_as_undefined() {
  let page = r#"export default function Page() {
  const seen: string[] = [];
  for (const word of ["x", "y"]) {
    const before = seen[seen.length - 1];
    seen.push(before ? `${before}${word}` : word);
  }
  return <p>{seen.join(",")}</p>;
}
"#;
  let html = html("negative", page).unwrap();
  assert!(html.contains("<p>x,xy</p>"), "{html}");
}

#[test]
fn a_write_through_a_field_alias_reaches_the_object_it_came_from() {
  let page = r#"const normalize = (config: { settings: { verbs: { punctuation?: string; format?: string }; theme: string } }) => {
  const verbs = config.settings.verbs;
  if (verbs.punctuation) {
    verbs.format = "{}" + verbs.punctuation;
    delete verbs.punctuation;
  }
  return config;
};

export default function Page() {
  const config = normalize({ settings: { verbs: { punctuation: "..." }, theme: "dark" } });
  return <p>{config.settings.verbs.format ?? "none"} {"punctuation" in config.settings.verbs ? "kept" : "dropped"} {config.settings.theme}</p>;
}
"#;
  let html = html("field_alias", page).unwrap();
  assert!(html.contains("<p>{}... dropped dark</p>"), "{html}");
}

#[test]
fn a_write_through_a_field_alias_after_its_root_was_written_is_residue() {
  let page = r#"export default function Page() {
  const config = { settings: { theme: "dark" } };
  const settings = config.settings;
  config.settings = { theme: "light" };
  settings.theme = "blue";
  return <p>{config.settings.theme}</p>;
}
"#;
  let err = html("field_stale", page).unwrap_err();
  assert!(err.contains("a write through `settings`, a field of `config`, after `config` was written"), "{err}");
}

#[test]
fn a_write_through_a_field_alias_of_a_parameter_at_the_top_of_a_body_reaches_it() {
  let page = r#"const normalize = (config: { settings: { verbs: { format?: string } } }) => {
  const verbs = config.settings.verbs;
  verbs.format = "{}";
  return config;
};

export default function Page() {
  return <p>{normalize({ settings: { verbs: {} } }).settings.verbs.format ?? "none"}</p>;
}
"#;
  let html = html("field_alias_top", page).unwrap();
  assert!(html.contains("<p>{}</p>"), "{html}");
}

#[test]
fn a_write_through_a_field_alias_of_a_local_reaches_it() {
  let page = r#"export default function Page() {
  const config = { settings: { verbs: { format: "none" } } };
  const verbs = config.settings.verbs;
  verbs.format = "{}";
  return <p>{config.settings.verbs.format}</p>;
}
"#;
  let html = html("field_alias_local", page).unwrap();
  assert!(html.contains("<p>{}</p>"), "{html}");
}
