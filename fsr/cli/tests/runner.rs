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
  let summary = test::run(&dir, &Options::beside(&dir), &snapfire_fsr_cli::typecheck::Typecheck::default(), None).unwrap();
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
  let summary = test::run(&dir, &Options::beside(&dir), &snapfire_fsr_cli::typecheck::Typecheck::default(), None).unwrap();
  assert_eq!(summary.failed, 0, "{summary}");
  assert_eq!(summary.passed, 1, "{summary}");
  std::fs::remove_dir_all(dir.parent().unwrap()).unwrap();
}

#[test]
fn a_redirecting_action_shows_its_target_in_the_trace_and_answers_a_spec_post_as_the_host_does() {
  let dir = app(&[
    ("routes/page.tsx", "export default function Page() {\n  return <p>page</p>;\n}\n"),
    ("routes/actions.ts", "import { action, redirect } from \"@snapfire/fsr\";\n\nexport const save = action(async ({ session }) => {\n  session.saved = true;\n  redirect(\"/saved\");\n});\n"),
    ("tests/save.test.ts", "import { save } from \"@routes/actions\";\nimport { ctx, expect, test } from \"@snapfire/fsr/testing\";\n\ntest(\"the trace names where the action sent its caller\", async () => {\n  const c = ctx({});\n  await save(c);\n  expect(c.trace.redirect).toEqual(\"/saved\");\n  expect(c.session.saved).toEqual(true);\n});\n"),
    ("tests/save.spec.ts", "import { ctx, expect, load, test } from \"@snapfire/fsr-client/testing\";\n\ntest(\"a form post goes where the action sent it and a json caller is told\", async () => {\n  await load(\"/\", { ctx: ctx({}) });\n  const posted = await fetch(\"/_sf/action/$root.save?__fragment\", { method: \"POST\", headers: { \"content-type\": \"application/x-www-form-urlencoded\", referer: \"/\" }, body: \"_csrf=t\" });\n  expect(posted.status).toEqual(303);\n  expect(posted.headers.get(\"location\")).toEqual(\"/saved?__fragment\");\n  const called = await fetch(\"/_sf/action/$root.save\", { method: \"POST\", headers: { \"content-type\": \"application/json\" }, body: \"null\" });\n  expect(called.status).toEqual(200);\n  expect(called.headers.get(\"x-sf-redirect\")).toEqual(\"/saved\");\n});\n"),
  ]);
  let summary = test::run(&dir, &Options::beside(&dir), &snapfire_fsr_cli::typecheck::Typecheck::default(), None).unwrap();
  assert_eq!(summary.failed, 0, "{summary}");
  assert_eq!(summary.passed, 2, "{summary}");
  std::fs::remove_dir_all(dir.parent().unwrap()).unwrap();
}

#[test]
fn a_spec_fetch_reads_an_empty_fragment_or_payload_value_like_the_bare_key() {
  let dir = app(&[
    ("routes/layout.tsx", "export default function Layout({ children }: { children: unknown }) {\n  return <main class=\"frame\">{children}</main>;\n}\n"),
    ("routes/page.tsx", "export default function Page() {\n  return <p>page</p>;\n}\n"),
    ("tests/query.spec.ts", "import { ctx, expect, load, test } from \"@snapfire/fsr-client/testing\";\n\ntest(\"an empty value is the bare key\", async () => {\n  await load(\"/\", { ctx: ctx({}) });\n  for (const query of [\"__fragment\", \"__fragment=\", \"__fragment=&by=1\", \"%5F%5Ffragment\"]) {\n    const response = await fetch(`/?${query}`);\n    expect(response.status, query).toEqual(200);\n    const html = await response.text();\n    expect(html.includes(\"<p>page</p>\") && !html.includes(\"frame\"), `${query}: ${html}`).toBeTruthy();\n  }\n  for (const query of [\"__payload\", \"__payload=\", \"by=1&__payload=\"]) {\n    const text = await (await fetch(`/?${query}`)).text();\n    expect(text.startsWith(\"V {\\\"fmt\\\"\"), `${query}: ${text.slice(0, 40)}`).toBeTruthy();\n  }\n});\n"),
  ]);
  let summary = test::run(&dir, &Options::beside(&dir), &snapfire_fsr_cli::typecheck::Typecheck::default(), None).unwrap();
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
  std::thread::spawn(move || done.send(test::run(&spec, &Options::beside(&spec), &snapfire_fsr_cli::typecheck::Typecheck::default(), None).map(|s| (s.failed, s.passed, s.to_string()))));
  let (failed, passed, summary) = finished
    .recv_timeout(std::time::Duration::from_secs(60))
    .expect("the spec answers within a minute")
    .unwrap();
  assert_eq!((failed, passed), (0, 1), "{summary}");
  std::fs::remove_dir_all(dir.parent().unwrap()).unwrap();
}

#[test]
fn a_spec_file_that_fails_to_load_fails_alone_and_the_next_file_runs() {
  let dir = app(&[
    ("routes/page.tsx", "export default function Page() {\n  return <p>page</p>;\n}\n"),
    ("tests/a.spec.ts", "import { test } from \"@snapfire/fsr-client/testing\";\n\nthrow new Error(\"boom at load\");\ntest(\"never registered\", () => {});\n"),
    ("tests/b.spec.ts", "import { expect, test } from \"@snapfire/fsr-client/testing\";\n\ntest(\"runs\", () => {\n  expect(1).toEqual(1);\n});\n"),
  ]);
  let summary = test::run(&dir, &Options::beside(&dir), &snapfire_fsr_cli::typecheck::Typecheck::default(), None).unwrap();
  let text = summary.to_string();
  assert_eq!((summary.failed, summary.passed), (1, 1), "{text}");
  assert!(text.contains("tests/a.spec.ts: loading the file") && text.contains("boom at load"), "{text}");
  std::fs::remove_dir_all(dir.parent().unwrap()).unwrap();
}

#[test]
fn a_timer_a_finished_test_left_behind_does_not_fail_the_next_one() {
  let dir = app(&[
    ("routes/page.tsx", "export default function Page() {\n  return <p>page</p>;\n}\n"),
    ("tests/b.spec.ts", "import { advance, expect, test } from \"@snapfire/fsr-client/testing\";\n\ntest(\"leaves a timer behind\", () => {\n  setTimeout(() => {\n    throw new Error(\"left behind\");\n  }, 5);\n});\n\ntest(\"runs after it\", async () => {\n  await advance(10);\n  expect(1).toEqual(1);\n});\n"),
  ]);
  let summary = test::run(&dir, &Options::beside(&dir), &snapfire_fsr_cli::typecheck::Typecheck::default(), None).unwrap();
  assert_eq!((summary.failed, summary.passed), (0, 2), "{summary}");
  std::fs::remove_dir_all(dir.parent().unwrap()).unwrap();
}

#[test]
fn a_node_getter_read_off_node_prototype_answers_for_every_node_as_in_a_browser() {
  let spec = r#"import { expect, test } from "@snapfire/fsr-client/testing";

test("the prototype's getters agree with the node's own", () => {
  const template = document.createElement("template");
  template.innerHTML = "<p>a</p><p>b</p>";
  const fragment = template.content;
  const first = fragment.firstChild!;
  const text = first.firstChild!;
  for (const name of ["firstChild", "lastChild", "nextSibling", "previousSibling"]) {
    const get = Object.getOwnPropertyDescriptor(Node.prototype, name)!.get!;
    for (const [what, node] of [["fragment", fragment], ["element", first], ["text", text]] as const) {
      expect(get.call(node), `${name} of the ${what}`).toBe((node as unknown as Record<string, unknown>)[name] ?? null);
    }
  }
  expect(Object.getOwnPropertyDescriptor(Node.prototype, "firstChild")!.get!.call(fragment)).toBe(first);
});
"#;
  let dir = app(&[("routes/page.tsx", "export default function Page() {\n  return <p>page</p>;\n}\n"), ("tests/node.spec.ts", spec)]);
  let summary = test::run(&dir, &Options::beside(&dir), &snapfire_fsr_cli::typecheck::Typecheck::default(), None).unwrap();
  assert_eq!(summary.failed, 0, "{summary}");
  assert_eq!(summary.passed, 1, "{summary}");
  std::fs::remove_dir_all(dir.parent().unwrap()).unwrap();
}

#[test]
fn a_listener_is_called_with_the_node_it_was_added_to_as_in_a_browser() {
  let spec = r#"import { expect, fireEvent, test } from "@snapfire/fsr-client/testing";

test("this is the current target, capture runs first and a removed listener stays removed", async () => {
  const root = document.createElement("div");
  const button = document.createElement("button");
  root.append(button);
  document.body.append(root);
  const seen: unknown[] = [];
  function onRoot(this: unknown) { seen.push(this === root ? "root" : this); }
  function onCapture(this: unknown) { seen.push(this === root ? "root capture" : this); }
  root.addEventListener("click", onRoot);
  root.addEventListener("click", onCapture, { capture: true });
  await fireEvent.click(button);
  expect(seen, "the capture listener first, as in a browser").toEqual(["root capture", "root"]);
  root.removeEventListener("click", onRoot);
  root.removeEventListener("click", onCapture, true);
  await fireEvent.click(button);
  expect(seen.length, "both listeners were removed").toEqual(2);
});
"#;
  let dir = app(&[("routes/page.tsx", "export default function Page() {\n  return <p>page</p>;\n}\n"), ("tests/listener.spec.ts", spec)]);
  let summary = test::run(&dir, &Options::beside(&dir), &snapfire_fsr_cli::typecheck::Typecheck::default(), None).unwrap();
  assert_eq!(summary.failed, 0, "{summary}");
  assert_eq!(summary.passed, 1, "{summary}");
  std::fs::remove_dir_all(dir.parent().unwrap()).unwrap();
}

#[test]
fn a_mocked_session_that_is_not_written_out_is_refused_naming_the_part_and_what_was_found() {
  let dir = app(&[
    ("routes/page.tsx", "export default function Page() {\n  return <p>page</p>;\n}\n"),
    ("routes/page.loader.ts", "import type { Ctx } from \"@snapfire/fsr\";\n\nexport async function load({ session }: Ctx) {\n  return { n: session.n };\n}\n"),
    ("tests/page.test.ts", "import { load } from \"@routes/page.loader\";\nimport { ctx, expect, test } from \"@snapfire/fsr/testing\";\n\nconst fresh = () => ({ n: 1 });\n\ntest(\"a helper\", async () => {\n  const c = ctx({ session: fresh() });\n  expect((await load(c)).n).toEqual(1);\n});\n"),
  ]);
  let refused = test::run(&dir, &Options::beside(&dir), &snapfire_fsr_cli::typecheck::Typecheck::default(), None).map(|summary| summary.to_string()).unwrap_or_else(|e| e.to_string());
  assert!(refused.contains("tests/page.test.ts:7:"), "{refused}");
  assert!(refused.contains("`session` in a mocked context is an object literal written in the test; a call is not read"), "{refused}");
  std::fs::remove_dir_all(dir.parent().unwrap()).unwrap();
}

#[test]
fn an_awaited_run_inside_expect_reads_as_its_resolved_value() {
  let dir = app(&[
    ("routes/page.tsx", "export default function Page() {\n  return <p>page</p>;\n}\n"),
    ("routes/page.loader.ts", "export async function load() {\n  return { n: 2 };\n}\n"),
    ("routes/actions.ts", "import { action } from \"@snapfire/fsr\";\n\nexport const bump = action(async () => ({ n: 3 }));\n"),
    ("tests/page.test.ts", "import { load } from \"@routes/page.loader\";\nimport { bump } from \"@routes/actions\";\nimport { ctx, expect, test } from \"@snapfire/fsr/testing\";\n\ntest(\"awaited in place\", async () => {\n  const c = ctx({});\n  expect(await load(c)).toEqual({ n: 2 });\n  expect(await bump(c)).toEqual({ n: 3 });\n  expect(await bump(c)).not.toEqual({ n: 4 });\n});\n"),
  ]);
  let summary = test::run(&dir, &Options::beside(&dir), &snapfire_fsr_cli::typecheck::Typecheck::default(), None).unwrap();
  assert_eq!(summary.failed, 0, "{summary}");
  assert_eq!(summary.passed, 1, "{summary}");
  std::fs::remove_dir_all(dir.parent().unwrap()).unwrap();
}

#[test]
fn a_run_inside_a_helper_function_or_an_expression_is_refused_saying_where_a_run_goes() {
  let dir = app(&[
    ("routes/page.tsx", "export default function Page() {\n  return <p>page</p>;\n}\n"),
    ("routes/page.loader.ts", "export async function load() {\n  return { n: 2 };\n}\n"),
    ("tests/page.test.ts", "import { load } from \"@routes/page.loader\";\nimport { ctx, expect, test } from \"@snapfire/fsr/testing\";\n\ntest(\"a helper\", async () => {\n  const c = ctx({});\n  const n = async () => (await load(c)).n;\n  expect(await n()).toEqual(2);\n});\n"),
  ]);
  let refused = test::run(&dir, &Options::beside(&dir), &snapfire_fsr_cli::typecheck::Typecheck::default(), None).map(|summary| summary.to_string()).unwrap_or_else(|e| e.to_string());
  assert!(refused.contains("tests/page.test.ts:6:"), "{refused}");
  assert!(refused.contains("a run of `load` inside a larger expression or a function the test declares"), "{refused}");
  assert!(!refused.contains("which the build cannot follow"), "{refused}");
  std::fs::write(dir.join("tests/page.test.ts"), "import { load } from \"@routes/page.loader\";\nimport { ctx, expect, test } from \"@snapfire/fsr/testing\";\n\ntest(\"a field of a run\", async () => {\n  const c = ctx({});\n  expect((await load(c)).n).toEqual(2);\n});\n").unwrap();
  let refused = test::run(&dir, &Options::beside(&dir), &snapfire_fsr_cli::typecheck::Typecheck::default(), None).map(|summary| summary.to_string()).unwrap_or_else(|e| e.to_string());
  assert!(refused.contains("tests/page.test.ts:6:"), "{refused}");
  assert!(refused.contains("a run of `load` inside a larger expression"), "{refused}");
  std::fs::remove_dir_all(dir.parent().unwrap()).unwrap();
}

#[test]
fn a_cleanup_that_throws_fails_its_own_test_and_the_rest_of_the_file_still_reports() {
  let spec = "import { registerIsland, scan } from \"@snapfire/fsr-client\";\nimport { expect, settle, test, waitFor } from \"@snapfire/fsr-client/testing\";\n\ntest(\"leaves an island whose unmount throws\", async () => {\n  registerIsland(\"src/Bad.tsx#default\", { loader: async () => ({}), mount: () => ({}), unmount: () => { throw new Error(\"unmount broke\"); } });\n  document.body.innerHTML = '<sf-i id=\"bad\" data-sf-module=\"src/Bad.tsx#default\"></sf-i>';\n  scan(document);\n  await settle();\n  await waitFor(() => expect(document.querySelector(\"#bad\")?.hasAttribute(\"data-sf-mounted\")).toEqual(true));\n});\n\ntest(\"runs after it\", () => {\n  expect(1 + 1).toEqual(2);\n});\n";
  let dir = app(&[("routes/page.tsx", "export default function Page() {\n  return <p>page</p>;\n}\n"), ("tests/cleanup.spec.ts", spec)]);
  let summary = test::run(&dir, &Options::beside(&dir), &snapfire_fsr_cli::typecheck::Typecheck::default(), None).unwrap();
  let report = summary.to_string();
  assert!(!report.contains("loading the file"), "{report}");
  assert_eq!((summary.passed, summary.failed), (1, 1), "{report}");
  assert!(report.contains("leaves an island whose unmount throws") && report.contains("unmount broke"), "the cleanup's error is the first test's: {report}");
  std::fs::remove_dir_all(dir.parent().unwrap()).unwrap();
}

#[test]
fn an_input_and_a_textarea_answer_their_default_value_as_in_a_browser() {
  let spec = "import { expect, test } from \"@snapfire/fsr-client/testing\";\n\ntest(\"defaultValue is the markup's\", () => {\n  document.body.innerHTML = '<input id=\"i\" value=\"start\"><input id=\"e\"><textarea id=\"t\">notes</textarea>';\n  const input = document.querySelector(\"#i\") as HTMLInputElement;\n  const textarea = document.querySelector(\"#t\") as HTMLTextAreaElement;\n  input.value = \"typed\";\n  expect([input.defaultValue, (document.querySelector(\"#e\") as HTMLInputElement).defaultValue, textarea.defaultValue]).toEqual([\"start\", \"\", \"notes\"]);\n});\n";
  let dir = app(&[("routes/page.tsx", "export default function Page() {\n  return <p>page</p>;\n}\n"), ("tests/default.spec.ts", spec)]);
  let summary = test::run(&dir, &Options::beside(&dir), &snapfire_fsr_cli::typecheck::Typecheck::default(), None).unwrap();
  assert_eq!((summary.passed, summary.failed), (1, 0), "{summary}");
  std::fs::remove_dir_all(dir.parent().unwrap()).unwrap();
}

#[test]
fn a_dialog_and_a_details_answer_open_as_a_boolean_reflecting_the_attribute() {
  let spec = "import { expect, test } from \"@snapfire/fsr-client/testing\";\n\ntest(\"open reflects the attribute\", () => {\n  document.body.innerHTML = '<dialog id=\"d\"></dialog><details id=\"s\" open></details>';\n  const dialog = document.querySelector(\"#d\") as HTMLDialogElement;\n  const details = document.querySelector(\"#s\") as HTMLDetailsElement;\n  expect([dialog.open, details.open]).toEqual([false, true]);\n  dialog.open = true;\n  details.open = false;\n  expect([dialog.getAttribute(\"open\"), details.hasAttribute(\"open\")]).toEqual([\"\", false]);\n});\n";
  let dir = app(&[("routes/page.tsx", "export default function Page() {\n  return <p>page</p>;\n}\n"), ("tests/open.spec.ts", spec)]);
  let summary = test::run(&dir, &Options::beside(&dir), &snapfire_fsr_cli::typecheck::Typecheck::default(), None).unwrap();
  assert_eq!((summary.passed, summary.failed), (1, 0), "{summary}");
  std::fs::remove_dir_all(dir.parent().unwrap()).unwrap();
}

#[test]
fn a_textarea_keeps_what_was_typed_when_its_default_value_changes() {
  let spec = "import { expect, test } from \"@snapfire/fsr-client/testing\";\n\ntest(\"value and defaultValue are apart once typed\", () => {\n  document.body.innerHTML = '<textarea id=\"t\">start</textarea>';\n  const area = document.querySelector(\"#t\") as HTMLTextAreaElement;\n  area.value = \"typed\";\n  area.defaultValue = \"markup\";\n  expect([area.value, area.defaultValue, area.textContent]).toEqual([\"typed\", \"markup\", \"markup\"]);\n});\n";
  let dir = app(&[("routes/page.tsx", "export default function Page() {\n  return <p>page</p>;\n}\n"), ("tests/textarea.spec.ts", spec)]);
  let summary = test::run(&dir, &Options::beside(&dir), &snapfire_fsr_cli::typecheck::Typecheck::default(), None).unwrap();
  assert_eq!((summary.passed, summary.failed), (1, 0), "{summary}");
  std::fs::remove_dir_all(dir.parent().unwrap()).unwrap();
}
