use std::path::PathBuf;

use snapfire_fsr_cli::{test, Options};

static NEXT: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);

fn app(files: &[(&str, &str)]) -> PathBuf {
  let n = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
  let root = std::env::temp_dir().join(format!("fsr-cli-runner-{}-{n}", std::process::id()));
  let _ = std::fs::remove_dir_all(&root);
  let dir = root.join("app");
  std::fs::create_dir_all(dir.join("routes")).unwrap();
  std::fs::create_dir_all(root.join("config")).unwrap();
  std::fs::write(root.join("config/app.toml"), "[server]\nlisten = \"127.0.0.1:0\"\n\n[session]\nkey = \"runner-test-key\"\nttl = \"1h\"\n").unwrap();
  std::fs::write(dir.join("importmap.json"), r#"{"imports":{"@snapfire/fsr-client":"/static/js/fsr/index.js"}}"#).unwrap();
  for (name, source) in files {
    let path = dir.join(name);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, source).unwrap();
  }
  dir
}

#[test]
fn a_body_test_runs_a_loader_over_the_module_constant_it_imports() {
  let dir = app(&[
    ("src/items.ts", "export const items: string[] = [\"a\", \"b\"];\n"),
    ("routes/page.tsx", "export default function Page() {\n  return <p>page</p>;\n}\n"),
    ("routes/page.loader.ts", "import { items } from \"@src/items\";\n\nexport async function load() {\n  return { items, count: items.length };\n}\n\nexport const meta = ({ data }: { data: { count: number } }) => ({ title: `${data.count} items` });\n"),
    ("tests/page.test.ts", "import { load, meta } from \"@routes/page.loader\";\nimport { ctx, expect, test } from \"@snapfire/fsr/testing\";\n\ntest(\"the loader reads the constant\", async () => {\n  const c = ctx({});\n  const data = await load(c);\n  expect(data.items).toEqual([\"a\", \"b\"]);\n  const described = meta({ data });\n  expect(described.title).toEqual(\"2 items\");\n});\n"),
  ]);
  let summary = test::run(&dir, &Options::beside(&dir), None).unwrap();
  assert_eq!(summary.failed, 0, "{summary}");
  assert_eq!(summary.passed, 1, "{summary}");
  std::fs::remove_dir_all(dir.parent().unwrap()).unwrap();
}

#[test]
fn a_body_test_sees_an_extension_in_the_session_trace() {
  let dir = app(&[
    ("routes/page.tsx", "export default function Page() {\n  return <p>page</p>;\n}\n"),
    ("routes/actions.ts", "import { action } from \"@snapfire/fsr\";\n\nexport const stay = action(async ({ session }) => {\n  session.extend(7200);\n  return null;\n});\n"),
    ("tests/stay.test.ts", "import { stay } from \"@routes/actions\";\nimport { ctx, expect, test } from \"@snapfire/fsr/testing\";\n\ntest(\"the action extends the session and writes no key\", async () => {\n  const c = ctx({});\n  await stay(c);\n  expect(c.trace.session.extended).toEqual(7200);\n  expect(c.trace.session.written).toEqual([]);\n});\n"),
  ]);
  let summary = test::run(&dir, &Options::beside(&dir), None).unwrap();
  assert_eq!(summary.failed, 0, "{summary}");
  assert_eq!(summary.passed, 1, "{summary}");
  std::fs::remove_dir_all(dir.parent().unwrap()).unwrap();
}

#[test]
fn a_spec_fetch_reads_an_empty_fragment_or_payload_value_like_the_bare_key() {
  let dir = app(&[
    ("routes/layout.tsx", "export default function Layout({ children }: { children: unknown }) {\n  return <main class=\"frame\">{children}</main>;\n}\n"),
    ("routes/page.tsx", "export default function Page() {\n  return <p>page</p>;\n}\n"),
    ("tests/query.spec.ts", "import { ctx, expect, load, test } from \"@snapfire/fsr-client/testing\";\n\ntest(\"an empty value is the bare key\", async () => {\n  await load(\"/\", { ctx: ctx({}) });\n  for (const query of [\"__fragment\", \"__fragment=\", \"__fragment=&by=1\", \"%5F%5Ffragment\"]) {\n    const response = await fetch(`/?${query}`);\n    expect(response.status, query).toEqual(200);\n    const html = await response.text();\n    expect(html.includes(\"<p>page</p>\") && !html.includes(\"frame\"), `${query}: ${html}`).toBeTruthy();\n  }\n  for (const query of [\"__payload\", \"__payload=\", \"by=1&__payload=\"]) {\n    const text = await (await fetch(`/?${query}`)).text();\n    expect(text.startsWith(\"V {\\\"fmt\\\"\"), `${query}: ${text.slice(0, 40)}`).toBeTruthy();\n  }\n});\n"),
  ]);
  let summary = test::run(&dir, &Options::beside(&dir), None).unwrap();
  assert_eq!(summary.failed, 0, "{summary}");
  assert_eq!(summary.passed, 1, "{summary}");
  std::fs::remove_dir_all(dir.parent().unwrap()).unwrap();
}

#[test]
fn a_spec_fetch_of_an_action_decodes_only_a_percent_and_two_hex_digits() {
  let dir = app(&[
    ("routes/page.tsx", "export default function Page() {\n  return <p>page</p>;\n}\n"),
    ("tests/action.spec.ts", "import { ctx, expect, load, test } from \"@snapfire/fsr-client/testing\";\n\ntest(\"a stray percent stays as it is\", async () => {\n  await load(\"/\", { ctx: ctx({}) });\n  for (const [path, id] of [[\"/_sf/action/%a\\u00e9\", \"%a\\u00e9\"], [\"/_sf/action/%+1x\", \"%+1x\"]]) {\n    const response = await fetch(path, { method: \"POST\", headers: { \"content-type\": \"application/json\" }, body: \"{}\" });\n    const body = await response.text();\n    expect(body.includes(`\\`${id}\\``), `${path}: ${body}`).toBeTruthy();\n  }\n});\n"),
  ]);
  let (done, finished) = std::sync::mpsc::channel();
  let spec = dir.clone();
  std::thread::spawn(move || done.send(test::run(&spec, &Options::beside(&spec), None).map(|s| (s.failed, s.passed, s.to_string()))));
  let (failed, passed, summary) = finished
    .recv_timeout(std::time::Duration::from_secs(60))
    .expect("the spec answers within a minute")
    .unwrap();
  assert_eq!((failed, passed), (0, 1), "{summary}");
  std::fs::remove_dir_all(dir.parent().unwrap()).unwrap();
}
