//! Packages of the same workspace the application depends on as source: the files the application reaches through them move into the application and every import of them is rewritten to follow, so the passes after this one migrate them with the rest.

use std::collections::{BTreeMap, BTreeSet};

use crate::config::lenient_json;
use crate::relocate::{dirname, find, normalize};
use crate::syntax::{is_script, parse, specifiers};
use crate::{Pass, Report, Tree};

/// Where the caller puts each workspace package of the input, as `.workspace/<name>/<path in the package>`.
pub const WORKSPACE_DIR: &str = ".workspace/";

/// Where the files the application reaches go before the layout moves them under `app/src/`.
const PLACED_DIR: &str = "packages/";

/// A workspace package: its name and its `exports`, each subpath with the file it names under `.workspace/<name>/`, a condition object as the entry it settles on.
struct Package {
  name: String,
  exports: Vec<(String, String)>,
  dependencies: BTreeMap<String, String>,
}

pub(crate) struct Workspace;

impl Pass for Workspace {
  fn name(&self) -> &'static str {
    "workspace"
  }

  fn run(&self, tree: &mut Tree, report: &mut Report) {
    let packages = read_packages(tree, report);
    if packages.is_empty() {
      return;
    }
    if tree.paths().any(|p| p.starts_with(PLACED_DIR)) {
      report.note(PLACED_DIR, 0, "the application has a `packages/` directory of its own, so its workspace packages were left out");
      return;
    }
    let aliased = app_aliases(tree, &packages);
    let mut reached: BTreeSet<String> = BTreeSet::new();
    let mut queue: Vec<String> = tree.paths().filter(|p| !p.starts_with(WORKSPACE_DIR) && is_script(p)).map(str::to_owned).collect();
    while let Some(path) = queue.pop() {
      let Some(text) = tree.text(&path) else { continue };
      let Ok(parsed) = parse(&path, text) else { continue };
      for spec in specifiers(&parsed) {
        let target = if spec.value.starts_with('.') {
          if !path.starts_with(WORKSPACE_DIR) {
            continue;
          }
          normalize(dirname(&path), &spec.value).and_then(|p| find(tree, &p)).map(|(p, _)| p)
        } else {
          matched(&aliased, &spec.value).or_else(|| exported(&packages, &spec.value)).and_then(|p| find(tree, &p)).map(|(p, _)| p)
        };
        let Some(target) = target else { continue };
        if target.starts_with(WORKSPACE_DIR) && reached.insert(target.clone()) && is_script(&target) {
          queue.push(target);
        }
      }
    }
    let sheets: Vec<String> = reached.iter().filter(|p| p.ends_with(".css")).cloned().collect();
    for sheet in sheets {
      let Some(css) = tree.text(&sheet).map(str::to_owned) else { continue };
      for (_, url) in crate::modules::relative_urls(&css) {
        if let Some((target, _)) = normalize(dirname(&sheet), &url).and_then(|p| find(tree, &p)) {
          reached.insert(target);
        }
      }
    }
    let mut paths: Vec<(String, String)> = aliased.iter().map(|(pattern, file)| (pattern.clone(), without_extension(&format!("{PLACED_DIR}{}", file.strip_prefix(WORKSPACE_DIR).unwrap_or(file))))).collect();
    for package in &packages {
      for (subpath, file) in &package.exports {
        let pattern = match subpath.strip_prefix('.') {
          Some(rest) => format!("{}{rest}", package.name),
          None => continue,
        };
        let placed = format!("{PLACED_DIR}{}", file.strip_prefix(WORKSPACE_DIR).unwrap_or(file));
        paths.push((pattern, without_extension(&placed)));
      }
      if reached.iter().any(|p| p.starts_with(&format!("{WORKSPACE_DIR}{}/", package.name))) {
        for (dep, range) in &package.dependencies {
          if !range.starts_with("workspace:") {
            report.packages.entry(dep.clone()).or_insert_with(|| range.clone());
          }
        }
      }
    }
    for path in reached {
      if let Some(bytes) = tree.remove(&path) {
        tree.insert(format!("{PLACED_DIR}{}", &path[WORKSPACE_DIR.len()..]), bytes);
      }
    }
    add_paths(tree, &paths);
  }
}

/// The workspace packages the caller put under `.workspace/`.
fn read_packages(tree: &Tree, report: &mut Report) -> Vec<Package> {
  let manifests: Vec<String> = tree.paths().filter(|p| p.starts_with(WORKSPACE_DIR) && p.ends_with("/package.json")).map(str::to_owned).collect();
  let mut out = Vec::new();
  for manifest in manifests {
    let Some(json) = tree.text(&manifest).and_then(lenient_json) else { continue };
    let Some(name) = json.get("name").and_then(|n| n.as_str()) else { continue };
    let root = format!("{WORKSPACE_DIR}{name}");
    if manifest != format!("{root}/package.json") {
      continue;
    }
    let mut exports = Vec::new();
    match json.get("exports") {
      Some(serde_json::Value::String(file)) => exports.extend(settled(&root, ".", &serde_json::Value::String(file.clone()), &manifest, report)),
      Some(serde_json::Value::Object(map)) if map.keys().all(|k| k.starts_with('.')) => {
        for (subpath, target) in map {
          exports.extend(settled(&root, subpath, target, &manifest, report));
        }
      }
      Some(target @ serde_json::Value::Object(_)) => exports.extend(settled(&root, ".", target, &manifest, report)),
      _ => {
        if let Some(main) = json.get("module").or_else(|| json.get("main")).and_then(|m| m.as_str()) {
          exports.extend(normalize(&root, main).map(|file| (".".to_owned(), file)));
        }
      }
    }
    let dependencies = json.get("dependencies").and_then(|d| d.as_object()).map(|d| d.iter().map(|(k, v)| (k.clone(), v.as_str().unwrap_or_default().to_owned())).collect()).unwrap_or_default();
    out.push(Package { name: name.to_owned(), exports, dependencies });
  }
  out
}

/// The file an `exports` entry names: a string or the `import`, `default` or `browser` entry of a condition object, in that order; `node` alone is taken with a note, since the component that imports it may render in the browser too.
fn settled(root: &str, subpath: &str, target: &serde_json::Value, manifest: &str, report: &mut Report) -> Option<(String, String)> {
  let file = match target {
    serde_json::Value::String(file) => file.clone(),
    serde_json::Value::Object(conditions) => {
      let found = ["import", "default", "browser"].iter().find_map(|c| conditions.get(*c));
      match found {
        Some(nested @ serde_json::Value::Object(_)) => return settled(root, subpath, nested, manifest, report),
        Some(serde_json::Value::String(file)) => {
          if conditions.contains_key("node") {
            report.note(manifest, 0, format!("`{subpath}` names a file for node and another for the browser; the browser's was taken"));
          }
          file.clone()
        }
        _ => match conditions.get("node").and_then(|n| n.as_str()) {
          Some(file) => {
            report.note(manifest, 0, format!("`{subpath}` names a file only for node, which a component rendering in the browser cannot import"));
            file.to_owned()
          }
          None => return None,
        },
      }
    }
    _ => return None,
  };
  Some((subpath.to_owned(), normalize(root, &file)?))
}

/// The application's own `tsconfig.json` aliases whose target lies in a workspace package, each as the file under `.workspace/` it names. The caller's `.workspace/dirs.json` says where each package sits relative to the application.
fn app_aliases(tree: &Tree, packages: &[Package]) -> Vec<(String, String)> {
  let dirs: BTreeMap<String, String> = tree.text(&format!("{WORKSPACE_DIR}dirs.json")).and_then(|t| serde_json::from_str(t).ok()).unwrap_or_default();
  let Some(config) = tree.text("tsconfig.json").and_then(lenient_json) else { return Vec::new() };
  let options = config.get("compilerOptions");
  let base = options.and_then(|o| o.get("baseUrl")).and_then(|b| b.as_str()).unwrap_or(".");
  let Some(map) = options.and_then(|o| o.get("paths")).and_then(|p| p.as_object()) else { return Vec::new() };
  let mut out = Vec::new();
  for (pattern, targets) in map {
    for target in targets.as_array().into_iter().flatten().filter_map(|t| t.as_str()) {
      let at = lexical(&lexical("", base), target);
      let inside = packages.iter().find_map(|p| {
        let dir = lexical("", dirs.get(&p.name)?);
        at.strip_prefix(&format!("{dir}/")).map(|rest| format!("{WORKSPACE_DIR}{}/{rest}", p.name))
      });
      if let Some(file) = inside {
        out.push((pattern.clone(), file));
        break;
      }
    }
  }
  out
}

/// `rel` joined to `dir` without touching the disk, a `..` past the start kept.
fn lexical(dir: &str, rel: &str) -> String {
  let mut parts: Vec<&str> = Vec::new();
  for part in dir.split('/').chain(rel.split('/')) {
    match part {
      "" | "." => {}
      ".." if parts.last().is_some_and(|p| *p != "..") => {
        parts.pop();
      }
      other => parts.push(other),
    }
  }
  parts.join("/")
}

/// The file an alias pattern, `*` and all, gives `spec`.
fn matched(aliases: &[(String, String)], spec: &str) -> Option<String> {
  aliases.iter().find_map(|(pattern, file)| match pattern.split_once('*') {
    Some((head, tail)) => spec.strip_prefix(head).and_then(|r| r.strip_suffix(tail)).map(|star| file.replacen('*', star, 1)),
    None => (pattern == spec).then(|| file.clone()),
  })
}

/// The file under `.workspace/` a bare specifier names through a package's `exports`.
fn exported(packages: &[Package], spec: &str) -> Option<String> {
  let package = packages.iter().find(|p| spec == p.name || spec.starts_with(&format!("{}/", p.name)))?;
  let subpath = format!(".{}", &spec[package.name.len()..]);
  for (pattern, file) in &package.exports {
    match pattern.split_once('*') {
      Some((head, tail)) => {
        if let Some(star) = subpath.strip_prefix(head).and_then(|r| r.strip_suffix(tail)) {
          return Some(file.replacen('*', star, 1));
        }
      }
      None if *pattern == subpath => return Some(file.clone()),
      None => {}
    }
  }
  None
}

fn without_extension(path: &str) -> String {
  let file = path.rsplit('/').next().unwrap_or(path);
  match file.rfind('.') {
    Some(dot) if is_script(file) => path[..path.len() - file.len() + dot].to_owned(),
    _ => path.to_owned(),
  }
}

/// `tree`'s `tsconfig.json` with `paths` added, so the layout's import rewrite follows each package specifier to where its file now is.
fn add_paths(tree: &mut Tree, paths: &[(String, String)]) {
  let mut config = tree.text("tsconfig.json").and_then(lenient_json).unwrap_or_else(|| serde_json::json!({}));
  let Some(object) = config.as_object_mut() else { return };
  let options = object.entry("compilerOptions").or_insert_with(|| serde_json::json!({}));
  let Some(options) = options.as_object_mut() else { return };
  let base = options.get("baseUrl").and_then(|b| b.as_str()).and_then(|b| normalize("", b)).unwrap_or_default();
  let map = options.entry("paths").or_insert_with(|| serde_json::json!({}));
  let Some(map) = map.as_object_mut() else { return };
  for (pattern, target) in paths {
    let from_base = crate::relocate::relative(&base, target);
    let aliased = map.get(pattern).and_then(|t| t.as_array()).and_then(|t| t.first()).and_then(|t| t.as_str()).is_some_and(|t| normalize(&base, t).is_none());
    if aliased || !map.contains_key(pattern) {
      map.insert(pattern.clone(), serde_json::json!([from_base]));
    }
  }
  tree.insert("tsconfig.json", serde_json::to_string_pretty(&config).unwrap_or_default());
}

#[cfg(test)]
mod tests {
  use crate::{migrate, Tree};

  fn tree(files: &[(&str, &str)]) -> Tree {
    let mut t = Tree::new();
    for (p, s) in files {
      t.insert(*p, *s);
    }
    t
  }

  #[test]
  fn what_the_application_reaches_in_a_workspace_package_moves_in_and_its_imports_follow() {
    let shell = r#"{ "name": "@shell/web", "exports": { "./i18n/server": "./src/i18n/server.ts", "./env": { "node": "./src/env/server.ts", "default": "./src/env/client.ts" }, "./images/*": "./src/images/*" }, "dependencies": { "intl-lib": "^3", "@shell/core": "workspace:*" } }"#;
    let core = r#"{ "name": "@shell/core", "exports": { ".": "./src/index.ts" } }"#;
    let input = tree(&[
      ("package.json", r#"{ "dependencies": { "next": "15", "@shell/web": "workspace:*" } }"#),
      ("app/page.tsx", "import { getLocale } from \"@shell/web/i18n/server\";\nimport { env } from \"@shell/web/env\";\nimport logo from \"@shell/web/images/logo.svg\";\nexport default function Page() { return <img src={logo.src} alt={getLocale() + env} />; }\n"),
      (".workspace/@shell/web/package.json", shell),
      (".workspace/@shell/web/src/i18n/server.ts", "import { pick } from \"./pick\";\nimport { first } from \"@shell/core\";\nimport { format } from \"intl-lib\";\nexport const getLocale = () => format(pick(first([\"en\"])));\n"),
      (".workspace/@shell/web/src/i18n/pick.ts", "export const pick = (l) => l;\n"),
      (".workspace/@shell/web/src/i18n/unused.ts", "export const unused = 1;\n"),
      (".workspace/@shell/web/src/env/client.ts", "export const env = \"client\";\n"),
      (".workspace/@shell/web/src/env/server.ts", "export const env = \"server\";\n"),
      (".workspace/@shell/web/src/images/logo.svg", "<svg/>"),
      (".workspace/@shell/core/package.json", core),
      (".workspace/@shell/core/src/index.ts", "export const first = (xs) => xs[0];\n"),
    ]);
    let (out, report) = migrate(&input);
    let page = out.text("app/routes/page.tsx").unwrap();
    assert!(page.contains("from \"@src/packages/@shell/web/src/i18n/server\""), "{page}");
    assert!(page.contains("from \"@src/packages/@shell/web/src/env/client\""), "{page}");
    assert!(page.contains("from \"@src/packages/@shell/web/src/images/logo.svg\""), "{page}");
    let server = out.text("app/src/packages/@shell/web/src/i18n/server.ts").unwrap();
    assert!(server.contains("from \"./pick\"") && server.contains("from \"@src/packages/@shell/core/src/index\""), "{server}");
    assert!(out.contains("app/src/packages/@shell/web/src/i18n/pick.ts"));
    assert!(out.contains("app/src/packages/@shell/web/src/images/logo.svg"));
    assert!(!out.paths().any(|p| p.contains("unused") || p.contains("env/server") || p.contains(".workspace") || p.ends_with("package.json")), "{:?}", out.paths().collect::<Vec<_>>());
    assert_eq!(report.packages.keys().collect::<Vec<_>>(), ["intl-lib"]);
    assert!(report.notes.iter().any(|n| n.message.contains("`./env` names a file for node and another for the browser")), "{:?}", report.notes);
  }
  #[test]
  fn a_package_stylesheet_brings_the_files_its_urls_name_and_names_them_from_where_it_lands() {
    let input = tree(&[
      ("package.json", r#"{ "dependencies": { "next": "15", "@shell/web": "workspace:*" } }"#),
      ("app/page.tsx", "import { Button } from \"@shell/web/button\";\nexport default function Page() { return <Button />; }\n"),
      (".workspace/@shell/web/package.json", r#"{ "name": "@shell/web", "exports": { "./button": "./src/ui/Button.tsx" } }"#),
      (".workspace/@shell/web/src/ui/Button.tsx", "import \"./button.css\";\nexport function Button() { return <button className=\"b\">go</button>; }\n"),
      (".workspace/@shell/web/src/ui/button.css", ".b { background: url(\"../images/bg.png\"); }\n@font-face { src: url(./fonts/a.woff2) format(\"woff2\"); }\n.c { mask: url(data:image/svg+xml,x); }\n"),
      (".workspace/@shell/web/src/images/bg.png", "png"),
      (".workspace/@shell/web/src/ui/fonts/a.woff2", "woff"),
    ]);
    let (out, report) = migrate(&input);
    assert_eq!(out.text("app/styles/button.css").unwrap(), ".b { background: url(\"../src/packages/@shell/web/src/images/bg.png\"); }\n@font-face { src: url(../src/packages/@shell/web/src/ui/fonts/a.woff2) format(\"woff2\"); }\n.c { mask: url(data:image/svg+xml,x); }\n");
    assert!(out.contains("app/src/packages/@shell/web/src/images/bg.png"));
    assert!(out.contains("app/src/packages/@shell/web/src/ui/fonts/a.woff2"));
    assert!(!report.notes.iter().any(|n| n.message.contains("now resolve from")), "{:?}", report.notes);
  }

  #[test]
  fn an_application_alias_into_a_workspace_package_wins_over_its_exports() {
    let input = tree(&[
      ("package.json", r#"{ "dependencies": { "next": "15", "@shell/web": "workspace:*", "@shell/scripts": "workspace:*" } }"#),
      ("tsconfig.json", "{\n  \"compilerOptions\": {\n    // the shim stands in for the real module\n    \"paths\": { \"@shell/scripts/react\": [\"../../packages/web/src/shims/react.tsx\"] }\n  }\n}\n"),
      ("app/page.tsx", "import { mount } from \"@shell/scripts/react\";\nexport default function Page() { return <p>{mount()}</p>; }\n"),
      (".workspace/dirs.json", r#"{ "@shell/web": "../../packages/web", "@shell/scripts": "../../packages/scripts" }"#),
      (".workspace/@shell/web/package.json", r#"{ "name": "@shell/web", "exports": {} }"#),
      (".workspace/@shell/web/src/shims/react.tsx", "export const mount = () => \"shim\";\n"),
      (".workspace/@shell/scripts/package.json", r#"{ "name": "@shell/scripts", "exports": { "./react": "./src/react.ts" } }"#),
      (".workspace/@shell/scripts/src/react.ts", "export const mount = () => \"real\";\n"),
    ]);
    let (out, _) = migrate(&input);
    let page = out.text("app/routes/page.tsx").unwrap();
    assert!(page.contains("from \"@src/packages/@shell/web/src/shims/react\""), "{page}");
    assert!(out.contains("app/src/packages/@shell/web/src/shims/react.tsx"));
    assert!(!out.paths().any(|p| p.ends_with("scripts/src/react.ts")), "{:?}", out.paths().collect::<Vec<_>>());
  }
}
