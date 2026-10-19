//! The published-package reader over generated input that knows its own
//! answer, every vendored module in the repository, arbitrary bytes and hostile
//! archives.

use proptest::prelude::*;

use super::{entry, imports_of, matching, normalize, relative, unpack, Published};

fn specifier() -> impl Strategy<Value = String> {
  prop::sample::select(vec!["./a.js", "../b/c.js", "./d", "#alias", "#shared/k", "pkg", "pkg/sub", "@s/p", "@s/p/deep/x.js", "./with space.js", "./ünïcode.js"]).prop_map(str::to_owned)
}

fn quote() -> impl Strategy<Value = char> {
  prop::sample::select(vec!['"', '\''])
}

/// One statement and the specifier it imports, when it imports one: an
/// import form the reader must find or text that only looks like one.
fn statement() -> impl Strategy<Value = (String, Option<String>)> {
  let q = || quote();
  let real = |form: &'static str| (specifier(), quote()).prop_map(move |(s, q)| (form.replace("{q}", &q.to_string()).replace("{s}", &s), Some(s)));
  let fake = |form: &'static str| (specifier(), quote()).prop_map(move |(s, q)| (form.replace("{q}", &q.to_string()).replace("{s}", &s), None));
  let _ = q;
  prop_oneof![
    real("import x from {q}{s}{q};"),
    real("import { a, b as c } from {q}{s}{q};"),
    real("import {\n  a,\n  // b\n  c,\n} from {q}{s}{q};"),
    real("import * as ns from {q}{s}{q};"),
    real("import {q}{s}{q};"),
    real("import d, { e } from {q}{s}{q};"),
    real("import j from {q}{s}{q} with { type: 'json' };"),
    real("export * from {q}{s}{q};"),
    real("export { a } from {q}{s}{q};"),
    real("export { default } from {q}{s}{q};"),
    real("export * as ns from {q}{s}{q};"),
    real("const m = await import({q}{s}{q});"),
    real("f(() => import( {q}{s}{q} ));"),
    real("const u = {q}/*{q}; import z from {q}{s}{q}; // */"),
    real("const half = a / 2; import h from {q}{s}{q}; const back = 2 / 1;"),
    fake("const t = {q}import y from {s}{q};"),
    fake("const v = `import w from '{s}'`;"),
    fake("const v2 = `${a}import w from '{s}'${b}`;"),
    fake("/* import k from '{s}' */"),
    fake("/* a */ /* import k from '{s}' */"),
    fake("// import k from '{s}'"),
    fake("let r = 1; // import('{s}')"),
    fake("/** @import { T } from '{s}' */"),
    fake("const re = /import x from 'y'/; const s1 = '{s}';"),
    fake("const re2 = /\\/*/; const s2 = '{s}';"),
    fake("obj.import('{s}');"),
    fake("const dyn = import(name + '{s}');"),
    fake("const url = new URL('{s}', import.meta.url);"),
    fake("const from = '{s}'; let importer = from;"),
    fake("export default function f() { return 'from \"{s}\"'; }"),
  ]
}

fn module() -> impl Strategy<Value = (String, Vec<String>)> {
  prop::collection::vec(statement(), 0..8).prop_map(|statements| {
    let text = statements.iter().map(|(s, _)| s.as_str()).collect::<Vec<_>>().join("\n") + "\n";
    let expected = statements.into_iter().filter_map(|(_, s)| s).collect();
    (text, expected)
  })
}

/// Every vendored module in the repository: npm sources kept as published and esm.sh bundles.
fn corpus() -> Vec<(std::path::PathBuf, String)> {
  fn walk(dir: &std::path::Path, vendored: bool, out: &mut Vec<(std::path::PathBuf, String)>) {
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    for path in entries.flatten().map(|e| e.path()) {
      let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
      if path.is_dir() {
        if !["target", "node_modules", ".git", ".fsr"].contains(&name.as_str()) {
          walk(&path, vendored || name == "vendor", out);
        }
      } else if vendored && (name.ends_with(".js") || name.ends_with(".mjs")) {
        if let Ok(text) = std::fs::read_to_string(&path) {
          out.push((path, text));
        }
      }
    }
  }
  let examples = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../examples");
  let mut out = Vec::new();
  walk(&examples, false, &mut out);
  out.sort();
  out
}

proptest! {
  #![proptest_config(ProptestConfig::with_cases(512))]

  #[test]
  fn the_reader_finds_exactly_the_imports_a_module_holds((text, expected) in module()) {
    let found = imports_of(&text).expect("the generator writes modules");
    for (range, specifier) in &found {
      prop_assert_eq!(text.get(range.clone()), Some(specifier.as_str()));
    }
    let specifiers: Vec<String> = found.into_iter().map(|(_, s)| s).collect();
    prop_assert_eq!(specifiers, expected, "{}", text);
  }

  #[test]
  fn a_relative_path_resolves_back_to_its_target(
    from in prop::collection::vec("[a-z]{1,3}", 1..5),
    to in prop::collection::vec("[a-z]{1,3}", 1..5),
  ) {
    let from = from.join("/");
    let to = to.join("/");
    let dir = from.rsplit_once('/').map_or("", |(dir, _)| dir);
    let specifier = relative(&from, &to);
    prop_assert!(specifier.starts_with("./") || specifier.starts_with("../"), "{}", specifier);
    prop_assert_eq!(normalize(&format!("{dir}/{specifier}")), to);
  }

  #[test]
  fn normalizing_is_idempotent_and_never_climbs_out(path in "[a-z./]{0,24}") {
    let once = normalize(&path);
    prop_assert_eq!(normalize(&once), once.clone());
    prop_assert!(!once.split('/').any(|part| part == ".." || part == "." || part.is_empty()) || once.is_empty(), "{}", once);
  }
}

/// A tree vendored as published holds every file its modules import by a relative path.
#[test]
fn every_vendored_module_reads_and_each_relative_import_names_a_file_beside_it() {
  let corpus = corpus();
  assert!(corpus.len() > 100, "the corpus is the vendored trees under examples/: {}", corpus.len());
  let mut relative_imports = 0;
  for (path, text) in &corpus {
    let found = imports_of(text).unwrap_or_else(|why| panic!("{}: {why}", path.display()));
    for (range, specifier) in found {
      assert_eq!(text.get(range), Some(specifier.as_str()), "{}", path.display());
      if specifier.starts_with("./") || specifier.starts_with("../") {
        relative_imports += 1;
        let target = path.parent().unwrap().join(&specifier);
        let named = [target.clone(), target.with_extension("js"), target.join("index.js")].iter().any(|candidate| candidate.is_file());
        assert!(named, "{} imports {specifier}, which is not beside it", path.display());
      }
    }
  }
  assert!(relative_imports > 100, "the published trees under examples/ import by relative path: {relative_imports}");
}

#[test]
fn the_reader_never_panics_and_every_range_holds_its_specifier() {
  bolero::check!().for_each(|bytes: &[u8]| {
    let text = String::from_utf8_lossy(bytes);
    for (range, specifier) in imports_of(&text).unwrap_or_default() {
      assert_eq!(text.get(range.clone()), Some(specifier.as_str()));
    }
  });
}

#[test]
fn the_reader_never_panics_on_any_cut_of_a_vendored_module() {
  for (_, text) in corpus().iter().filter(|(_, text)| text.len() < 64 * 1024).take(40) {
    let step = (text.len() / 64).max(1);
    for (cut, _) in text.char_indices().step_by(step) {
      for piece in [&text[..cut], &text[cut..]] {
        for (range, specifier) in imports_of(piece).unwrap_or_default() {
          assert_eq!(piece.get(range), Some(specifier.as_str()));
        }
      }
    }
  }
}

/// A gzipped tarball whose entries carry these names byte for byte, past the
/// checks `tar::Builder` makes on a path.
fn raw_tarball(entries: &[(Vec<u8>, tar::EntryType, &[u8])]) -> Vec<u8> {
  let mut builder = tar::Builder::new(flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::fast()));
  for (name, kind, data) in entries {
    let mut header = tar::Header::new_old();
    let field = &mut header.as_old_mut().name;
    let n = name.len().min(field.len());
    field[..n].copy_from_slice(&name[..n]);
    header.set_entry_type(*kind);
    header.set_size(data.len() as u64);
    header.set_mode(0o644);
    header.set_cksum();
    builder.append(&header, *data).unwrap();
  }
  builder.into_inner().unwrap().finish().unwrap()
}

/// Each name a tarball's reader hands on is a plain relative path, so it stays under whatever directory it is joined to.
fn assert_contained(files: &std::collections::BTreeMap<String, Vec<u8>>) {
  for name in files.keys() {
    assert!(!name.is_empty() && !name.starts_with('/') && !name.contains(['\\', ':']), "{name:?}");
    for part in name.split('/') {
      assert!(!part.is_empty() && part != "." && part != "..", "{name:?}");
    }
    let joined = std::path::Path::new("/vendor/pkg").join(name);
    assert!(joined.components().all(|c| matches!(c, std::path::Component::RootDir | std::path::Component::Normal(_))), "{name:?}");
  }
}

fn entry_name() -> impl Strategy<Value = Vec<u8>> {
  let part = prop_oneof![
    Just(b"package".to_vec()),
    Just(b"..".to_vec()),
    Just(b".".to_vec()),
    Just(Vec::new()),
    Just(b"src".to_vec()),
    Just(b"index.js".to_vec()),
    Just(b"C:".to_vec()),
    Just(b"a\\b".to_vec()),
    "[a-z.]{1,6}".prop_map(String::into_bytes),
  ];
  (prop::bool::ANY, prop::collection::vec(part, 1..6)).prop_map(|(absolute, parts)| {
    let mut name = if absolute { b"/".to_vec() } else { Vec::new() };
    name.extend(parts.join(&b'/'));
    name
  })
}

fn entry_kind() -> impl Strategy<Value = tar::EntryType> {
  prop::sample::select(vec![tar::EntryType::Regular, tar::EntryType::Directory, tar::EntryType::Symlink, tar::EntryType::Link, tar::EntryType::Continuous])
}

proptest! {
  #![proptest_config(ProptestConfig::with_cases(512))]

  #[test]
  fn no_entry_of_a_hostile_tarball_names_a_path_outside_its_package(entries in prop::collection::vec((entry_name(), entry_kind()), 0..8)) {
    let entries: Vec<(Vec<u8>, tar::EntryType, &[u8])> = entries.into_iter().map(|(name, kind)| (name, kind, &b"x"[..])).collect();
    if let Ok(files) = unpack(&raw_tarball(&entries)) {
      assert_contained(&files);
    }
  }
}

#[test]
fn a_tarball_names_its_files_under_the_top_directory_and_drops_one_that_climbs_out() {
  let files = unpack(&raw_tarball(&[
    (b"package/index.js".to_vec(), tar::EntryType::Regular, b"a"),
    (b"package/../../escaped.js".to_vec(), tar::EntryType::Regular, b"b"),
    (b"/package/abs.js".to_vec(), tar::EntryType::Regular, b"c"),
    (b"package/link.js".to_vec(), tar::EntryType::Symlink, b""),
    (b"package/src/./deep.js".to_vec(), tar::EntryType::Regular, b"d"),
    (b"package/..\\..\\windows.js".to_vec(), tar::EntryType::Regular, b"e"),
    (b"package/C:/drive.js".to_vec(), tar::EntryType::Regular, b"f"),
  ]))
  .unwrap();
  assert_eq!(files.keys().map(String::as_str).collect::<Vec<_>>(), ["abs.js", "index.js", "src/deep.js"]);
}

#[test]
fn unpacking_arbitrary_bytes_never_panics_or_names_a_path_outside() {
  bolero::check!().for_each(|bytes: &[u8]| {
    if let Ok(files) = unpack(bytes) {
      assert_contained(&files);
    }
    let gzipped = {
      use std::io::Write;
      let mut encoder = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::fast());
      encoder.write_all(bytes).unwrap();
      encoder.finish().unwrap()
    };
    if let Ok(files) = unpack(&gzipped) {
      assert_contained(&files);
    }
  });
}

fn export_key() -> impl Strategy<Value = String> {
  prop::sample::select(vec![".", "./store", "./*", "./internal/*", "./*.js", "*", "./a/*/b", "#x", "#x/*", "import", "browser", "default", "production", "node", "types", "", "./../*"]).prop_map(str::to_owned)
}

fn export_target() -> impl Strategy<Value = serde_json::Value> {
  let leaf = prop_oneof![
    prop::sample::select(vec!["./index.js", "./src/*.js", "./*", "../outside.js", "/abs.js", "./a/../../b.js", "", "*"]).prop_map(|s| serde_json::Value::String(s.to_owned())),
    Just(serde_json::Value::Null),
    Just(serde_json::Value::Bool(false)),
    any::<i64>().prop_map(|n| serde_json::json!(n)),
  ];
  leaf.prop_recursive(4, 32, 4, |inner| {
    prop_oneof![
      prop::collection::vec(inner.clone(), 0..4).prop_map(serde_json::Value::Array),
      prop::collection::vec((export_key(), inner), 0..5).prop_map(|pairs| serde_json::Value::Object(pairs.into_iter().collect())),
    ]
  })
}

proptest! {
  #![proptest_config(ProptestConfig::with_cases(1024))]

  #[test]
  fn resolving_any_export_map_never_panics(exports in proptest::option::of(export_target()), imports in proptest::option::of(export_target()), subpath in "[a-z/.*#]{0,12}") {
    let doc: Published = serde_json::from_value(serde_json::json!({ "version": "1.0.0", "exports": exports, "imports": imports, "dist": { "tarball": "" } })).unwrap();
    let _ = entry(&doc, &subpath);
    if let Some(serde_json::Value::Object(map)) = &doc.imports {
      let _ = matching(map, &format!("#{subpath}"));
    }
  }
}
