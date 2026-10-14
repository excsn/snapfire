//! An fsr built without its `mdx` feature refuses MDX by name rather than
//! leaving a page out or reading a file as TypeScript.
#![cfg(not(feature = "mdx"))]

use std::path::PathBuf;

use snapfire_fsr_cli::{build, BuildError, Options};

static NEXT: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);

fn project(config: &str, files: &[(&str, &str)]) -> PathBuf {
  let n = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
  let root = std::env::temp_dir().join(format!("fsr-cli-mdx-unbuilt-{}-{n}", std::process::id()));
  let _ = std::fs::remove_dir_all(&root);
  let app = root.join("app");
  std::fs::create_dir_all(app.join("routes")).unwrap();
  std::fs::create_dir_all(root.join("config")).unwrap();
  std::fs::write(root.join("config/app.toml"), format!("[app]\ndir = \"app\"\n\n{config}")).unwrap();
  for (name, source) in files {
    let path = app.join(name);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, source).unwrap();
  }
  app
}

fn refusal(config: &str, files: &[(&str, &str)]) -> BuildError {
  let app = project(config, files);
  let error = build(&app, &Options::default()).err().expect("refused");
  let _ = std::fs::remove_dir_all(app.parent().unwrap());
  error
}

#[test]
fn a_page_mdx_names_the_feature() {
  match refusal("", &[("routes/page.mdx", "# x\n")]) {
    BuildError::MdxFeature { what, feature } => {
      assert!(what.ends_with("routes/page.mdx"), "{what}");
      assert_eq!(feature, "mdx");
    }
    other => panic!("{other}"),
  }
}

#[test]
fn collections_and_frontmatter_name_their_features() {
  assert!(matches!(refusal("[mdx.collections]\nposts = \"content/posts\"\n", &[("routes/page.tsx", "export default function P() {\n  return <p />;\n}\n")]), BuildError::MdxFeature { feature: "mdx", .. }));
  assert!(matches!(refusal("[mdx]\nfrontmatter = true\n", &[("routes/page.tsx", "export default function P() {\n  return <p />;\n}\n")]), BuildError::MdxFeature { feature: "mdx-frontmatter", .. }));
}

#[test]
fn an_imported_mdx_file_names_the_feature() {
  let page = "import Post from \"../content/post.mdx\";\nexport default function P() {\n  return <Post />;\n}\n";
  let error = refusal("", &[("routes/page.tsx", page), ("content/post.mdx", "# x\n")]).to_string();
  assert!(error.contains("`mdx` feature"), "{error}");
}
