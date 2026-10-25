//! The client's standard modules (`intl.ts` and the rest beside `std.ts`, with `standard.ts` holding them as namespaces), `host/` and `terminal/` modules are written from the standard library's table: each side lists the members `meaning::side_modules` gives it, with the doc `std.ts` gives the member and where it runs. `FSR_WRITE_FACADES=1` rewrites them.

use std::path::{Path, PathBuf};

use snapfire_fsr_ir::{standard_reach, Reach};
use snapfire_fsr_lower::meaning::{side_modules, Side};

fn client_src() -> PathBuf {
  Path::new(env!("CARGO_MANIFEST_DIR")).join("../client/src")
}

/// The types a module's members are declared with, exported beside them.
fn types_of(module: &str) -> &'static [&'static str] {
  match module {
    "intl" => &["NumberOptions", "CurrencyOptions", "DateOptions", "ListOptions", "DateStyle", "TextWidth"],
    "text" => &["FrontMatterOptions"],
    "file" => &["FileStat"],
    "system" => &["SpawnOptions", "Spawned", "SpawnedInput", "SpawnedOutput"],
    _ => &[],
  }
}

/// The `/** ... */` above `name` in `std.ts`: a member of `export const <module> = {` or a top-level function. `None` for a member `std.ts` does not declare, one the lowerer reaches only from the platform's own spelling (`toLocaleTimeString` for `intl.localTime`).
fn doc(std_ts: &str, module: &str, name: &str) -> Option<String> {
  let block = match std_ts.find(&format!("export const {module} = {{")) {
    Some(at) => &std_ts[at..at + std_ts[at..].find("\n};").expect("the module object ends")],
    None => std_ts,
  };
  let lines: Vec<&str> = block.lines().collect();
  let opens = |line: &str| {
    let t = line.trim_start();
    t.starts_with(&format!("{name}(")) || t.starts_with(&format!("{name}<")) || t.starts_with(&format!("export function {name}(")) || t.starts_with(&format!("export function {name}<"))
  };
  let at = lines.iter().position(|line| opens(line))?;
  let above = lines[at - 1].trim();
  assert!(above.starts_with("/**") && above.ends_with("*/"), "`{module}.{name}` in std.ts has no one-line doc above it: {above}");
  Some(above.trim_start_matches("/**").trim_end_matches("*/").trim().to_owned())
}

fn runs(side: Side, module: &str, name: &str) -> &'static str {
  if side == Side::Terminal {
    return "Runs in a terminal program only.";
  }
  match standard_reach(module, name) {
    Some(Reach::Render) => "Runs in a server render and in the browser.",
    _ => "Runs in every host body: loaders, actions, route handlers, middleware and terminal programs.",
  }
}

/// Member names JavaScript reserves, which a module exports under the name but cannot declare: `id.new`.
const RESERVED: &[&str] = &["new", "delete", "default", "function", "class", "import", "export", "in", "of", "var", "let", "const", "return", "this", "typeof", "void", "with", "yield"];

const HEADER: &str = "// Written from the standard library's table by `FSR_WRITE_FACADES=1 cargo nextest run -p snapfire_fsr_lower --test facades`. Do not edit.\n";

/// One side's module. A client module sits beside `std.ts`, where the browser loads it under the prefix the import map gives every client module; a host or terminal module sits in a directory of its own.
fn facade(std_ts: &str, side: Side, module: &str, names: &[&str], beside: bool) -> String {
  let std_js = if beside { "./std.js" } else { "../std.js" };
  let mut out = String::from(HEADER);
  let object = if module == "i18n" { "t as translate" } else { module };
  out.push_str(&format!("import {{ {object} }} from \"{std_js}\";\n"));
  let types = types_of(module);
  if !types.is_empty() {
    out.push_str(&format!("export type {{ {} }} from \"{std_js}\";\n", types.join(", ")));
  }
  if side == Side::Terminal && module == "system" {
    out.push_str(&format!("export {{ Scope }} from \"{std_js}\";\n"));
  }
  for name in names {
    let Some(doc) = doc(std_ts, module, name) else { continue };
    let source = if module == "i18n" { "translate".to_owned() } else { format!("{module}.{name}") };
    let ty = if module == "i18n" { "typeof translate".to_owned() } else { format!("typeof {module}.{name}") };
    match RESERVED.contains(name) {
      true => out.push_str(&format!("\n/** {doc} {} */\nconst {name}_: {ty} = {source};\nexport {{ {name}_ as {name} }};\n", runs(side, module, name))),
      false => out.push_str(&format!("\n/** {doc} {} */\nexport const {name}: {ty} = {source};\n", runs(side, module, name))),
    }
  }
  if side == Side::Client && module == "intl" {
    out.push_str(&format!("\n/** {} */\nexport {{ localeTag }} from \"{std_js}\";\n", doc(std_ts, "localeTag", "localeTag").expect("std.ts declares localeTag")));
  }
  out
}

/// What a side's `index.ts` re-exports: each module as a namespace and `t` beside them.
fn index(side: Side, modules: &[&str]) -> String {
  let mut out = String::from(HEADER);
  for module in modules {
    out.push_str(&format!("export * as {module} from \"./{module}.js\";\n"));
  }
  if modules.contains(&"i18n") {
    out.push_str("export { t } from \"./i18n.js\";\n");
  }
  if side == Side::Terminal {
    out.push_str("export { Scope } from \"./system.js\";\n");
  }
  out
}

fn expected() -> Vec<(PathBuf, String)> {
  let std_ts = std::fs::read_to_string(client_src().join("std.ts")).unwrap();
  let mut out = Vec::new();
  for (side, dir, index_file) in [(Side::Client, "", "standard.ts"), (Side::Host, "host", "index.ts"), (Side::Terminal, "terminal", "index.ts")] {
    let modules = side_modules(side);
    for (module, names) in &modules {
      out.push((client_src().join(dir).join(format!("{module}.ts")), facade(&std_ts, side, module, names, dir.is_empty())));
    }
    let names: Vec<&str> = modules.iter().map(|(m, _)| *m).collect();
    out.push((client_src().join(dir).join(index_file), index(side, &names)));
  }
  out
}

#[test]
fn each_sides_modules_are_written_from_the_standard_librarys_table() {
  let write = std::env::var_os("FSR_WRITE_FACADES").is_some_and(|v| v == "1");
  let mut stale = Vec::new();
  for (path, text) in expected() {
    if write {
      std::fs::create_dir_all(path.parent().unwrap()).unwrap();
      std::fs::write(&path, &text).unwrap();
    } else if std::fs::read_to_string(&path).ok().as_deref() != Some(text.as_str()) {
      stale.push(path.display().to_string());
    }
  }
  assert!(stale.is_empty(), "these differ from the table; FSR_WRITE_FACADES=1 rewrites them:\n{}", stale.join("\n"));
}
