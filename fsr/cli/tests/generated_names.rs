//! The names `generated/client.ts` derives from route directories are names
//! TypeScript accepts, whatever characters the directory starts with.

use std::path::PathBuf;

use snapfire_fsr_cli::new::{create, NewOptions};
use snapfire_fsr_cli::{build, Options};

static NEXT: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);

const PAGE: &str = "export default function Page({ title }: { title: string }) {\n  return <main>{title}</main>;\n}\n";

const LOADER: &str = "export async function load() {\n  return { title: \"intro\" };\n}\n";

const ACTIONS: &str = "import { action } from \"@snapfire/fsr/host\";\nimport type { ActionCtx } from \"@snapfire/fsr/host\";\n\nexport const join = action(async (_ctx: ActionCtx) => {\n  return { ok: true };\n});\n";

fn client(tag: &str, files: &[(&str, &str)]) -> String {
  let n = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
  let root = std::env::temp_dir().join(format!("fsr-cli-names-{}-{n}-{tag}", std::process::id()));
  let _ = std::fs::remove_dir_all(&root);
  create(&root, NewOptions::default().fetch(false)).unwrap();
  let app = root.join("app");
  for (path, text) in files {
    let file = app.join(path);
    std::fs::create_dir_all(file.parent().unwrap()).unwrap();
    std::fs::write(file, text).unwrap();
  }
  let built = build(&app, &Options::beside(&app)).unwrap_or_else(|e| panic!("builds: {e}"));
  let client = built.files.iter().find(|(name, _)| name == "generated/client.ts").map(|(_, text)| text.clone()).unwrap();
  std::fs::remove_dir_all(&root).unwrap();
  client
}

#[test]
fn a_route_whose_directory_starts_with_a_digit_gets_a_props_type_that_starts_with_a_letter() {
  let client = client("digit", &[("routes/030-intro/page.tsx", PAGE), ("routes/030-intro/page.loader.ts", LOADER)]);
  assert!(client.contains("export type Route030IntroProps = Wire<Awaited<ReturnType<typeof import(\"../routes/030-intro/page.loader\")[\"load\"]>>> & HostProps;"), "{client}");
  let bad: Vec<&str> = client.lines().filter(|line| line.strip_prefix("export type ").is_some_and(|rest| rest.starts_with(|c: char| c.is_ascii_digit()))).collect();
  assert!(bad.is_empty(), "{bad:#?}");
}

#[test]
fn an_action_group_that_is_not_an_identifier_is_quoted() {
  let client = client("actions", &[
    ("routes/cart/page.tsx", PAGE),
    ("routes/cart/actions.ts", ACTIONS),
    ("routes/sign-up/page.tsx", PAGE),
    ("routes/sign-up/actions.ts", ACTIONS),
    ("routes/030-intro/page.tsx", PAGE),
    ("routes/030-intro/actions.ts", ACTIONS),
  ]);
  assert!(client.contains("\n  cart: {\n"), "an identifier stays bare: {client}");
  assert!(client.contains("\n  \"sign-up\": {\n"), "{client}");
  assert!(client.contains("\n  \"030-intro\": {\n"), "{client}");
}
