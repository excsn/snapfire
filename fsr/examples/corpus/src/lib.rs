//! The lowerer corpus: see `tests/corpus.rs`.

use std::path::{Path, PathBuf};

pub fn copy(from: &Path, to: &Path) {
  std::fs::create_dir_all(to).unwrap();
  for entry in std::fs::read_dir(from).unwrap().flatten() {
    let target = to.join(entry.file_name());
    if entry.file_type().unwrap().is_dir() {
      copy(&entry.path(), &target);
    } else {
      std::fs::copy(entry.path(), target).unwrap();
    }
  }
}

/// `example` laid out as an application whose only page renders its `App`.
pub fn app_of(root: &Path, example: &Path, scratch: &Path) -> PathBuf {
  let dir = scratch.join(example.file_name().unwrap());
  let _ = std::fs::remove_dir_all(&dir);
  copy(&root.join("template/app"), &dir.join("app"));
  std::fs::create_dir_all(dir.join("config")).unwrap();
  std::fs::copy(root.join("template/app.toml"), dir.join("config/app.toml")).unwrap();
  copy(example, &dir.join("app/src"));
  // react.dev's TypeScript examples carry an `App.js` shim, `export default App = AppTSX;`, that throws in a module.
  if dir.join("app/src/App.tsx").exists() {
    let _ = std::fs::remove_file(dir.join("app/src/App.js"));
  }
  let named = std::fs::read_to_string(dir.join("app/src/App.js")).is_ok_and(|s| !s.contains("export default") && s.contains("export function App"));
  let import = if named { "{ App }" } else { "App" };
  std::fs::create_dir_all(dir.join("app/routes")).unwrap();
  std::fs::write(dir.join("app/routes/page.tsx"), format!("import {import} from \"@src/App.js\";\n\nexport default function Page() {{\n  return <App />;\n}}\n")).unwrap();
  dir.join("app")
}
