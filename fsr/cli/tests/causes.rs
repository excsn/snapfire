use std::path::{Path, PathBuf};

use snapfire_fsr_cli::new::{create, NewOptions};
use snapfire_fsr_cli::{build, Built, Options};

static NEXT: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);

fn root(tag: &str) -> PathBuf {
  let n = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
  let dir = std::env::temp_dir().join(format!("fsr-cli-causes-{}-{n}-{tag}", std::process::id()));
  let _ = std::fs::remove_dir_all(&dir);
  dir
}

fn write(app: &Path, name: &str, source: &str) {
  let path = app.join(name);
  std::fs::create_dir_all(path.parent().unwrap()).unwrap();
  std::fs::write(path, source).unwrap();
}

/// A scaffold with `files` written over it, built.
fn built(tag: &str, files: &[(&str, &str)]) -> Built {
  let root = root(tag);
  create(&root, NewOptions { fetch: false, with: vec!["react".to_owned()], ..NewOptions::default() }).unwrap();
  let app = root.join("app");
  write(&app, "importmap.json", r#"{"imports":{"@snapfire/fsr-client/react":"/r","react":"/r","react-dom/client":"/d"}}"#);
  write(&app, "vendor/.fsr-vendor.json", r#"{"packages":{"react":{"version":"18.3.1"},"react-dom":{"version":"18.3.1"}}}"#);
  for (name, source) in files {
    write(&app, name, source);
  }
  build(&app, &Options::beside(&app)).unwrap()
}

/// `.slice()` is outside the lowered subset, so it stands in for whatever the
/// subset does not hold yet.
const STARS: &str = "export function Stars({ label }: { label: string }) {\n  return <span>{label.slice(0, 3)}</span>;\n}\n";
const HEADER: &str = "import { Stars } from \"@src/ui/Stars\";\n\nexport function Header() {\n  return (\n    <header>\n      <Stars label=\"aaaa\" />\n    </header>\n  );\n}\n";

fn page(title: &str) -> String {
  format!("import {{ Header }} from \"@src/ui/Header\";\n\nexport default function {title}() {{\n  return (\n    <section>\n      <Header />\n    </section>\n  );\n}}\n")
}

#[test]
fn a_cause_names_the_leaf_and_where_it_is_placed_and_the_pages_lower_around_it() {
  let built = built(
    "chain",
    &[("src/ui/Stars.tsx", STARS), ("src/ui/Header.tsx", HEADER), ("routes/page.tsx", &page("Index"))],
  );
  let report = &built.report;

  assert_eq!(report.causes.len(), 1, "one leaf, one cause\n{report}");
  let cause = &report.causes[0];
  assert_eq!(cause.at, "src/ui/Stars.tsx:2:17", "the cause is the leaf's line and column\n{report}");
  assert!(cause.message.contains("`.slice()`"), "{}", cause.message);
  assert!(cause.hint.as_ref().is_some_and(|h| h.contains("the builtins are")), "the hint survives into the report\n{report}");

  assert_eq!(cause.pages.len(), 1, "{report}");
  let (module, chain) = &cause.pages[0];
  assert_eq!(module, "src/ui/Stars.tsx#Stars", "the leaf is what renders in the browser");
  assert!(chain.contains("<Stars> src/ui/Header.tsx:"), "the leaf's placement is named: {chain}");
  for lowered in ["routes/page.tsx#default", "src/ui/Header.tsx#Header"] {
    assert!(report.components.iter().any(|(module, how, _)| module == lowered && how == "lowered"), "{lowered} lowers around the leaf\n{report}");
  }

  let client: Vec<&(String, String, String)> = report.components.iter().filter(|(_, how, _)| how == "client").collect();
  assert_eq!(client.len(), 1, "{report}");
  assert_eq!(client[0].2, cause.at, "the row points at the cause rather than repeating it\n{report}");
}

#[test]
fn two_pages_over_one_leaf_are_one_cause() {
  let built = built(
    "fanout",
    &[
      ("src/ui/Stars.tsx", STARS),
      ("src/ui/Header.tsx", HEADER),
      ("routes/page.tsx", &page("Index")),
      ("routes/other/page.tsx", &page("Other")),
      ("routes/other/page.loader.ts", "import type { Ctx } from \"@snapfire/fsr\";\n\nexport async function load(_ctx: Ctx<\"/other\">) {\n  return {};\n}\n"),
    ],
  );
  let report = &built.report;

  assert_eq!(report.causes.len(), 1, "one leaf is one cause however many pages reach it\n{report}");
  let modules: Vec<&String> = report.causes[0].pages.iter().map(|(module, _)| module).collect();
  assert_eq!(modules, vec!["src/ui/Stars.tsx#Stars"], "the leaf renders in the browser once for every page that reaches it\n{report}");

  let printed = report.to_string();
  assert!(printed.contains("1 module renders in the browser for it"), "{printed}");
  assert_eq!(printed.matches("`.slice()`").count(), 1, "the message is printed once, not once per page\n{printed}");
}
