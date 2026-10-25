//! An FSR application written against `@snapfire/fsr-client`, `@snapfire/fsr-authoring` and `@snapfire/fsr`'s `Ctx`, rewritten onto `@snapfire/fsr/client`, `@snapfire/fsr/host`, `@snapfire/fsr/terminal`, `@snapfire/fsr/template` and `@snapfire/fsr/components`.
//!
//! A standard module goes to `@snapfire/fsr/client` when every member the file reads of it is one the browser has, else to `@snapfire/fsr/host`. A body file (a loader, an action, a route handler, middleware, a theme, a sitemap or a body test) and every file of a terminal application take the host side whole.

use std::collections::{BTreeMap, BTreeSet};

use swc_core::ecma::ast as js;
use swc_core::ecma::visit::{Visit, VisitWith};

use crate::edit::Edits;
use crate::syntax::{is_script, parse, specifiers, Parsed};
use crate::{Report, Tree};

const OLD_CLIENT: &str = "@snapfire/fsr-client";
const OLD_STD: &str = "@snapfire/fsr-client/std";
const OLD_AUTHORING: &str = "@snapfire/fsr-authoring";
const OLD_TEMPLATE: &str = "@snapfire/fsr-authoring/template";
const ROOT: &str = "@snapfire/fsr";
const CLIENT: &str = "@snapfire/fsr/client";
const HOST: &str = "@snapfire/fsr/host";
const TERMINAL: &str = "@snapfire/fsr/terminal";
const TEMPLATE: &str = "@snapfire/fsr/template";
const COMPONENTS: &str = "@snapfire/fsr/components";

/// The members the browser has of each standard module, as `@snapfire/fsr/client` lists them.
const CLIENT_MEMBERS: &[(&str, &[&str])] = &[
  ("intl", &["number", "currency", "date", "plural", "list", "compare", "localeTag"]),
  ("text", &["slug", "truncate"]),
  ("time", &["format", "add", "diff", "parse"]),
  ("crypto", &["hash", "verify", "digest"]),
  ("i18n", &["t"]),
  ("log", &["trace", "debug", "info", "warn", "error", "fatal"]),
  ("path", &["join", "normalize", "dirname", "basename", "extname", "isAbsolute"]),
];

/// The types the old standard module exported, by the module whose path declares them now.
const TYPE_HOMES: &[(&str, &str)] = &[
  ("NumberOptions", "@snapfire/fsr/client/intl"),
  ("CurrencyOptions", "@snapfire/fsr/client/intl"),
  ("DateOptions", "@snapfire/fsr/client/intl"),
  ("ListOptions", "@snapfire/fsr/client/intl"),
  ("DateStyle", "@snapfire/fsr/client/intl"),
  ("TextWidth", "@snapfire/fsr/client/intl"),
  ("FrontMatterOptions", "@snapfire/fsr/host/text"),
  ("FileStat", "@snapfire/fsr/host/file"),
  ("SpawnOptions", "@snapfire/fsr/terminal/system"),
  ("Spawned", "@snapfire/fsr/terminal/system"),
  ("SpawnedInput", "@snapfire/fsr/terminal/system"),
  ("SpawnedOutput", "@snapfire/fsr/terminal/system"),
];

/// Whether `path` is a body FSR runs in a host only.
fn is_body(path: &str) -> bool {
  let name = path.rsplit('/').next().unwrap_or(path);
  name.ends_with(".loader.ts") || name.ends_with(".test.ts") || ["route.ts", "actions.ts", "middleware.ts", "theme.ts", "sitemap.ts"].contains(&name)
}

/// Whether any file of `tree` is a terminal page, which a host renders.
fn is_terminal(tree: &Tree) -> bool {
  tree.iter().filter(|(p, _)| p.starts_with("app/")).any(|(_, bytes)| std::str::from_utf8(bytes).is_ok_and(|text| text.contains("\"@snapfire/fsr/terminal")))
}

/// Whether `tree` names an old specifier anywhere under `app/`.
pub fn is_old_layout(tree: &Tree) -> bool {
  tree.iter().filter(|(p, _)| p.starts_with("app/")).any(|(_, bytes)| {
    std::str::from_utf8(bytes).is_ok_and(|text| text.contains(OLD_CLIENT) || text.contains(OLD_AUTHORING) || text.contains("from \"@snapfire/fsr\""))
  })
}

/// `tree` with every old specifier under `app/` rewritten and its import map renamed.
pub fn relayout(tree: &Tree) -> (Tree, Report) {
  let mut report = Report::default();
  let mut out = tree.clone();
  let terminal = is_terminal(tree);
  let paths: Vec<String> = tree.paths().filter(|p| p.starts_with("app/")).map(str::to_owned).collect();
  for path in paths {
    let Some(text) = tree.text(&path) else { continue };
    let rewritten = if path.ends_with("importmap.json") {
      import_map(text)
    } else if is_script(&path) {
      match parse(&path, text) {
        Ok(parsed) => script(&parsed, text, terminal || is_body(&path)),
        Err(e) => {
          report.note(&path, 0, format!("not rewritten, it does not parse: {e}"));
          continue;
        }
      }
    } else if path.ends_with(".vue") || path.ends_with(".svelte") || path.ends_with(".mdx") {
      component_text(text)
    } else {
      continue;
    };
    if rewritten != text {
      out.insert(path, rewritten.into_bytes());
    }
  }
  (out, report)
}

/// A specifier that maps whole, with no member deciding where it goes.
fn renamed(specifier: &str) -> Option<String> {
  match specifier {
    OLD_CLIENT => Some(CLIENT.to_owned()),
    OLD_AUTHORING => Some(HOST.to_owned()),
    OLD_TEMPLATE => Some(TEMPLATE.to_owned()),
    _ => specifier.strip_prefix("@snapfire/fsr-client/").filter(|rest| *rest != "std").map(|rest| format!("{CLIENT}/{rest}")),
  }
}

/// A Vue, Svelte or MDX file: whatever its script imports, the browser runs it, so the standard module is the client's.
fn component_text(text: &str) -> String {
  let mut out = text.replace(&format!("\"{OLD_STD}\""), &format!("\"{CLIENT}\"")).replace(&format!("'{OLD_STD}'"), &format!("'{CLIENT}'"));
  for (old, new) in [(OLD_TEMPLATE, TEMPLATE), (OLD_AUTHORING, HOST)] {
    out = out.replace(&format!("\"{old}\""), &format!("\"{new}\"")).replace(&format!("'{old}'"), &format!("'{new}'"));
  }
  out = out.replace(&format!("\"{OLD_CLIENT}/"), &format!("\"{CLIENT}/")).replace(&format!("'{OLD_CLIENT}/"), &format!("'{CLIENT}/"));
  out.replace(&format!("\"{OLD_CLIENT}\""), &format!("\"{CLIENT}\"")).replace(&format!("'{OLD_CLIENT}'"), &format!("'{CLIENT}'"))
}

/// Each name a file reads off `local`, as `local.name`; `None` when it uses `local` any other way.
fn members_read(parsed: &Parsed, local: &str) -> Option<BTreeSet<String>> {
  struct Reads<'a> {
    local: &'a str,
    names: BTreeSet<String>,
    whole: bool,
  }
  impl Visit for Reads<'_> {
    fn visit_member_expr(&mut self, m: &js::MemberExpr) {
      if let (js::Expr::Ident(id), js::MemberProp::Ident(prop)) = (&*m.obj, &m.prop) {
        if id.sym.as_ref() == self.local {
          self.names.insert(prop.sym.to_string());
          return;
        }
      }
      m.visit_children_with(self);
    }
    fn visit_ident(&mut self, id: &js::Ident) {
      if id.sym.as_ref() == self.local {
        self.whole = true;
      }
    }
    fn visit_import_decl(&mut self, _: &js::ImportDecl) {}
  }
  let mut reads = Reads { local, names: BTreeSet::new(), whole: false };
  parsed.module.visit_with(&mut reads);
  (!reads.whole).then_some(reads.names)
}

/// The path `imported` from the old standard module goes to.
fn std_home(parsed: &Parsed, imported: &str, local: &str, host: bool) -> String {
  if let Some((_, home)) = TYPE_HOMES.iter().find(|(name, _)| *name == imported) {
    return (*home).to_owned();
  }
  match imported {
    "system" | "clipboard" | "Scope" => TERMINAL.to_owned(),
    "native" => ROOT.to_owned(),
    "localeTag" => CLIENT.to_owned(),
    "t" if !host => CLIENT.to_owned(),
    _ if host => HOST.to_owned(),
    module => {
      let browser = CLIENT_MEMBERS.iter().find(|(m, _)| *m == module).map(|(_, members)| *members);
      match (browser, members_read(parsed, local)) {
        (Some(members), Some(read)) if read.iter().all(|name| members.contains(&name.as_str())) => CLIENT.to_owned(),
        _ => HOST.to_owned(),
      }
    }
  }
}

/// The path a name imported from the old `@snapfire/fsr` or `@snapfire/fsr-authoring/template` goes to.
fn split_home(source: &str, imported: &str) -> String {
  match (source, imported) {
    (ROOT, "native") => ROOT.to_owned(),
    (ROOT, _) => HOST.to_owned(),
    (OLD_TEMPLATE, "Island") => COMPONENTS.to_owned(),
    _ => TEMPLATE.to_owned(),
  }
}

/// One import's named specifiers grouped by where each goes, every group written as its own import.
fn regroup(import: &js::ImportDecl, text: &str, parsed: &Parsed, home: impl Fn(&str, &str) -> String) -> String {
  let mut groups: BTreeMap<String, Vec<String>> = BTreeMap::new();
  for spec in &import.specifiers {
    let (imported, local, written) = match spec {
      js::ImportSpecifier::Named(named) => {
        let imported = match &named.imported {
          Some(js::ModuleExportName::Ident(id)) => id.sym.to_string(),
          Some(js::ModuleExportName::Str(s)) => s.value.to_string_lossy().into_owned(),
          None => named.local.sym.to_string(),
        };
        (imported, named.local.sym.to_string(), text[parsed.range(named.span)].to_owned())
      }
      js::ImportSpecifier::Default(d) => ("default".to_owned(), d.local.sym.to_string(), String::new()),
      js::ImportSpecifier::Namespace(ns) => ("*".to_owned(), ns.local.sym.to_string(), String::new()),
    };
    let target = home(&imported, &local);
    match spec {
      js::ImportSpecifier::Named(_) => groups.entry(target).or_default().push(written),
      js::ImportSpecifier::Namespace(_) => return format!("import * as {local} from \"{target}\";"),
      js::ImportSpecifier::Default(_) => return format!("import {local} from \"{target}\";"),
    }
  }
  let keyword = if import.type_only { "import type" } else { "import" };
  groups.into_iter().map(|(target, names)| format!("{keyword} {{ {} }} from \"{target}\";", names.join(", "))).collect::<Vec<_>>().join("\n")
}

fn script(parsed: &Parsed, text: &str, host: bool) -> String {
  let mut edits = Edits::default();
  let mut whole: Vec<std::ops::Range<usize>> = Vec::new();
  for item in &parsed.module.body {
    let js::ModuleItem::ModuleDecl(js::ModuleDecl::Import(import)) = item else { continue };
    let source = import.src.value.to_string_lossy().into_owned();
    let range = parsed.range(import.span);
    let replaced = match source.as_str() {
      OLD_STD => Some(regroup(import, text, parsed, |imported, local| std_home(parsed, imported, local, host))),
      ROOT | OLD_TEMPLATE => Some(regroup(import, text, parsed, |imported, _| split_home(&source, imported))),
      _ => None,
    };
    if let Some(replaced) = replaced {
      edits.replace(range.clone(), replaced);
      whole.push(range);
    }
  }
  for spec in specifiers(parsed) {
    if whole.iter().any(|r| r.start <= spec.range.start && spec.range.end <= r.end) {
      continue;
    }
    let target = match spec.value.as_str() {
      OLD_STD => Some(if host { HOST.to_owned() } else { CLIENT.to_owned() }),
      other => renamed(other),
    };
    if let Some(target) = target {
      edits.replace(spec.range, target);
    }
  }
  if edits.is_empty() {
    return text.to_owned();
  }
  edits.apply(text)
}

/// The import map with each old key renamed; the standard module's one entry becomes one per module the browser loads beside `native`'s.
fn import_map(text: &str) -> String {
  let mut out = String::with_capacity(text.len());
  for line in text.split_inclusive('\n') {
    let trimmed = line.trim_start();
    let indent = &line[..line.len() - trimmed.len()];
    let Some(rest) = trimmed.strip_prefix('"') else {
      out.push_str(line);
      continue;
    };
    let Some((key, after)) = rest.split_once('"') else {
      out.push_str(line);
      continue;
    };
    let comma = trimmed.trim_end().ends_with(',');
    let value = after.split('"').nth(1).unwrap_or_default();
    if key == OLD_STD {
      let dir = value.strip_suffix("std.js").unwrap_or(value);
      let mut entries: Vec<String> = CLIENT_MEMBERS.iter().map(|(module, _)| format!("{indent}\"{CLIENT}/{module}\": \"{dir}{module}.js\"")).collect();
      entries.push(format!("{indent}\"{ROOT}\": \"{dir}native.js\""));
      out.push_str(&entries.join(",\n"));
      out.push_str(if comma { ",\n" } else { "\n" });
      continue;
    }
    match renamed(key) {
      Some(new) => out.push_str(&line.replacen(&format!("\"{key}\""), &format!("\"{new}\""), 1)),
      None => out.push_str(line),
    }
  }
  out
}
