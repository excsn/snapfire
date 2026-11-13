use std::sync::atomic::{AtomicU32, Ordering};

use snapfire_fsr_ir::Tmpl;
use snapfire_fsr_lower::component::ComponentSet;

static NEXT: AtomicU32 = AtomicU32::new(0);

fn app(files: &[(&str, &str)]) -> std::path::PathBuf {
  let n = NEXT.fetch_add(1, Ordering::Relaxed);
  let dir = snapfire_fsr_test_internal::scratch_root().join(format!("fsr_capacity_{}_{n}", std::process::id()));
  let _ = std::fs::remove_dir_all(&dir);
  for (name, source) in files {
    let path = dir.join(name);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, source).unwrap();
  }
  dir
}

fn exact(t: &Tmpl, path: &str) {
  match t {
    Tmpl::Element { tag, attrs, children } => {
      assert_eq!(attrs.capacity(), attrs.len(), "{path}/{tag} attrs");
      assert_eq!(children.capacity(), children.len(), "{path}/{tag} children");
      children.iter().for_each(|c| exact(c, &format!("{path}/{tag}")));
    }
    Tmpl::Fragment(children) => {
      assert_eq!(children.capacity(), children.len(), "{path}/<> children");
      children.iter().for_each(|c| exact(c, &format!("{path}/<>")));
    }
    _ => {}
  }
}

#[test]
fn a_lowered_template_holds_its_attributes_and_children_in_vectors_no_longer_than_they_are() {
  let page = r#"export default function Page() {
  return <pre className="hl-code" data-language="rust"><code><span data-line=""><span className="hl-k">fn</span>{" "}<span className="hl-n">main</span></span></code></pre>;
}
"#;
  let dir = app(&[("routes/page.tsx", page)]);
  let mut set = ComponentSet::new(&dir);
  set.lower("routes/page.tsx#default").unwrap();
  let (_, component) = set.components.iter().find(|(module, _)| module == "routes/page.tsx#default").expect("the page lowers");
  exact(&component.render, "");
  std::fs::remove_dir_all(&dir).unwrap();
}
