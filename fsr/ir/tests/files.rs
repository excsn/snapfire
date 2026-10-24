//! `file` in a host that names its roots: a relative path is the application's and nothing outside the roots is reached. Without roots, as in a terminal program, a path is the working directory's.

use std::sync::Arc;

use snapfire_fsr_core::ext::FileRoots;
use snapfire_fsr_core::Value;
use snapfire_fsr_ir::{Ambient, Extensions};
use snapfire_fsr_test_internal::Scratch;

fn call(ambient: &Ambient, name: &str, args: &[Value]) -> Result<Value, String> {
  Extensions::standard().call(name, ambient, args).map_err(|e| e.message)
}

fn app() -> (Scratch, Ambient) {
  let root = Scratch::new("files");
  root.write("app/content/a.md", "# A\n");
  root.write("app/secret.toml", "key = 1\n");
  let roots = FileRoots::new(root.join("app"), vec!["content".into(), "data".into()]);
  (root, Ambient::default().files(Some(Arc::new(roots))))
}

#[test]
fn a_relative_path_is_the_applications_and_a_root_is_read_and_written() {
  let (root, ambient) = app();
  assert_eq!(call(&ambient, "file.read", &[Value::str("content/a.md")]).unwrap(), Value::str("# A\n"));
  call(&ambient, "file.makeDir", &[Value::str("data/notes"), Value::Map([("recursive".to_owned(), Value::Bool(true))].into_iter().collect())]).unwrap();
  call(&ambient, "file.write", &[Value::str("data/notes/n.txt"), Value::str("hi")]).unwrap();
  assert_eq!(std::fs::read_to_string(root.join("app/data/notes/n.txt")).unwrap(), "hi");
  assert_eq!(call(&ambient, "file.list", &[Value::str("content")]).unwrap(), Value::seq(vec![Value::str("a.md")]));
  assert_eq!(call(&ambient, "file.exists", &[Value::str("content/missing.md")]).unwrap(), Value::Bool(false));
}

#[test]
fn a_path_outside_the_roots_is_refused_with_the_key_named() {
  let (root, ambient) = app();
  for path in ["secret.toml", "content/../secret.toml", "../app/secret.toml"] {
    let why = call(&ambient, "file.read", &[Value::str(path)]).unwrap_err();
    assert!(why.contains("[files] roots") && why.contains("content"), "{path}: {why}");
  }
  let absolute = root.join("app/secret.toml").to_string_lossy().into_owned();
  assert!(call(&ambient, "file.read", &[Value::str(&absolute)]).unwrap_err().contains("[files] roots"));
  assert!(call(&ambient, "file.write", &[Value::str("elsewhere.txt"), Value::str("x")]).unwrap_err().contains("[files] roots"));
  assert!(!root.join("app/elsewhere.txt").exists());
}

#[test]
fn a_link_inside_a_root_pointing_out_of_it_is_refused() {
  let (root, ambient) = app();
  std::os::unix::fs::symlink(root.join("app/secret.toml"), root.join("app/content/link.toml")).unwrap();
  assert!(call(&ambient, "file.read", &[Value::str("content/link.toml")]).unwrap_err().contains("[files] roots"));
}

#[test]
fn a_host_naming_no_roots_reads_nothing_and_says_which_key_to_set() {
  let root = Scratch::new("files-none");
  root.write("app/content/a.md", "# A\n");
  let ambient = Ambient::default().files(Some(Arc::new(FileRoots::new(root.join("app"), vec![]))));
  let why = call(&ambient, "file.read", &[Value::str("content/a.md")]).unwrap_err();
  assert!(why.contains("no `[files] roots`"), "{why}");
}
