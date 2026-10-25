//! The standard library by side: `@snapfire/fsr/client` for what a server render and the browser both run, `@snapfire/fsr/host` for what every host body runs, each as a root that holds its modules and as one path per module.

use std::path::PathBuf;

use snapfire_fsr_ir::{Expr, Stmt, Tmpl};
use snapfire_fsr_lower::component::ComponentSet;

fn app(tag: &str, files: &[(&str, &str)]) -> PathBuf {
  let dir = std::env::temp_dir().join(format!("fsr_import_layout_{}_{tag}", std::process::id()));
  let _ = std::fs::remove_dir_all(&dir);
  for (name, source) in files {
    let path = dir.join(name);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, source).unwrap();
  }
  dir
}

fn body_exts(body: &[Stmt]) -> Vec<String> {
  let Some(Stmt::Return(expr)) = body.last() else { panic!("{body:?}") };
  let mut out = Vec::new();
  expr.visit(&mut |e| {
    if let Expr::Ext { module, name, args } = e {
      out.push(format!("{module}.{name}/{}", args.len()));
    }
  });
  out
}

fn tree_exts(tmpl: &Tmpl) -> Vec<String> {
  let mut out = Vec::new();
  tmpl.visit(&mut |e| {
    if let Expr::Ext { module, name, args } = e {
      out.push(format!("{module}.{name}/{}", args.len()));
    }
  });
  out
}

fn residue(set: &mut ComponentSet, module: &str) -> String {
  match set.lower(module) {
    Err(e) => e.to_string(),
    Ok(()) => set.browser_only.iter().find(|(m, _)| m == module).map(|(_, r)| r.to_string()).unwrap_or_else(|| panic!("{module} lowered")),
  }
}

#[test]
fn a_loader_reads_the_host_root_and_each_host_module_path() {
  let dir = app(
    "host",
    &[(
      "routes/a/page.loader.ts",
      "import { text, id } from \"@snapfire/fsr/host\";\nimport { localOffset } from \"@snapfire/fsr/host/time\";\nimport * as crypto from \"@snapfire/fsr/host/crypto\";\nexport async function load() {\n  return { label: text.slug(\"A b\"), id: id.new(), offset: localOffset(0), key: crypto.random(8) };\n}\n",
    )],
  );
  let mut set = ComponentSet::new(&dir);
  let body = set.lower_loader("routes/a/page.loader.ts").unwrap();
  assert_eq!(body_exts(&body), vec!["text.slug/1", "id.new/0", "time.localOffset/1", "crypto.random/1"]);
}

#[test]
fn a_component_reads_the_client_root_and_each_client_module_path() {
  let dir = app(
    "client",
    &[(
      "routes/a/page.tsx",
      "import { crypto, t } from \"@snapfire/fsr/client\";\nimport { slug } from \"@snapfire/fsr/client/text\";\nimport * as time from \"@snapfire/fsr/client/time\";\nexport default function A({ name, at }: { name: string; at: number }) {\n  return <p title={slug(name)} data-at={time.format(at, \"yyyy\")}>{crypto.hash(name)} {t(\"hi\")}</p>;\n}\n",
    )],
  );
  let mut set = ComponentSet::new(&dir);
  set.lower("routes/a/page.tsx#default").unwrap();
  let (_, component) = &set.components[0];
  assert_eq!(tree_exts(&component.render), vec!["text.slug/1", "time.format/2", "crypto.hash/1", "i18n.t/1"]);
}

#[test]
fn a_member_the_client_side_does_not_have_is_refused_with_the_loader_named() {
  let dir = app(
    "client_lacks",
    &[("routes/a/page.tsx", "import { time } from \"@snapfire/fsr/client\";\nexport default function A() {\n  return <p>{time.localOffset(0)}</p>;\n}\n")],
  );
  let mut set = ComponentSet::new(&dir);
  let why = residue(&mut set, "routes/a/page.tsx#default");
  assert!(why.contains("`time.localOffset` is not in `@snapfire/fsr/client`") && why.contains("@snapfire/fsr/host") && why.contains("page.loader.ts"), "{why}");
}

#[test]
fn a_component_importing_the_host_side_is_refused_with_the_client_path_named() {
  let dir = app(
    "host_in_component",
    &[("routes/a/page.tsx", "import { text } from \"@snapfire/fsr/host\";\nexport default function A({ name }: { name: string }) {\n  return <p>{text.slug(name)}</p>;\n}\n")],
  );
  let mut set = ComponentSet::new(&dir);
  let why = residue(&mut set, "routes/a/page.tsx#default");
  assert!(why.contains("`@snapfire/fsr/host`") && why.contains("`@snapfire/fsr/client`"), "{why}");
}

#[test]
fn native_comes_from_the_shared_root() {
  let dir = app(
    "native",
    &[
      ("ext/fmt.ts", "import { native } from \"@snapfire/fsr\";\nexport const pretty = native(\"fmt.pretty\", (n: number) => String(n));\n"),
      ("routes/a/page.tsx", "import { pretty } from \"@ext/fmt\";\nexport default function A({ n }: { n: number }) {\n  return <p>{pretty(n)}</p>;\n}\n"),
    ],
  );
  let mut set = ComponentSet::new(&dir);
  set.lower_extensions("ext/fmt.ts").unwrap();
  set.lower("routes/a/page.tsx#default").unwrap();
  let (_, component) = &set.components[0];
  assert_eq!(tree_exts(&component.render), vec!["fmt.pretty/1"]);
}

const ECHO: &str = "import { useEffect, useState } from \"react\";\nimport { Text } from \"@snapfire/fsr/terminal/ink\";\nIMPORT\n\nexport default function Echo() {\n  const [heard, setHeard] = useState(\"\");\n  useEffect(() => {\n    const sub = system.spawn(\"cat\", []);\n    sub.stdout.on(\"data\", (chunk) => setHeard((held) => held + chunk));\n    sub.stdin.write(\"hello\");\n    sub.stdin.end();\n  }, []);\n  return <Text>{heard} {new Date(Date.now()).toLocaleTimeString()}</Text>;\n}\n";

/// The page lowered for a terminal, with the spawn id, the offset of the call, read as `ID`.
fn terminal_page(tag: &str, imports: &str) -> String {
  let dir = app(tag, &[("routes/page.tsx", &ECHO.replace("IMPORT", imports))]);
  let mut set = ComponentSet::new(&dir).for_terminal();
  set.lower("routes/page.tsx#default").unwrap_or_else(|e| panic!("{e}"));
  let text = format!("{:?}", set.components.iter().find(|(m, _)| m == "routes/page.tsx#default").unwrap().1);
  let marker = "Field(\"id\", Lit(Str(\"";
  let at = text.find(marker).expect("a spawn id") + marker.len();
  let id = &text[at..at + text[at..].find('"').unwrap()];
  text.replace(&format!("\"{id}\""), "\"ID\"").replace(&format!("spawn:{id}:"), "spawn:ID:")
}

#[test]
fn a_terminal_page_spawns_through_the_terminal_root_and_reads_the_host_side() {
  let lowered = terminal_page("terminal", "import { system } from \"@snapfire/fsr/terminal\";");
  assert!(lowered.contains("action: \"$spawn\"") && lowered.contains("name: \"localTime\""), "{lowered}");
}


#[test]
fn pure_path_members_are_on_the_client_side_and_the_working_directory_ones_on_the_host_side() {
  let dir = app(
    "path",
    &[
      ("routes/a/page.tsx", "import { path } from \"@snapfire/fsr/client\";\nexport default function A({ file }: { file: string }) {\n  return <p title={path.basename(file, \".md\")}>{path.join(\"docs\", path.dirname(file))}</p>;\n}\n"),
      ("routes/a/page.loader.ts", "import { path, time, text } from \"@snapfire/fsr/host\";\nexport async function load() {\n  return { here: path.resolve(\"a\"), at: new Date(Date.now()).toLocaleTimeString(), zone: time.localOffset(Date.now()), meta: text.frontMatter(\"---\\na: 1\\n---\\n\") };\n}\n"),
      ("routes/b/page.tsx", "import { path } from \"@snapfire/fsr/client\";\nexport default function B() {\n  return <p>{path.resolve(\"a\")}</p>;\n}\n"),
    ],
  );
  let mut set = ComponentSet::new(&dir);
  set.lower("routes/a/page.tsx#default").unwrap_or_else(|e| panic!("{e}"));
  let (_, component) = set.components.iter().find(|(m, _)| m == "routes/a/page.tsx#default").unwrap();
  assert_eq!(tree_exts(&component.render), vec!["path.basename/2", "path.join/2", "path.dirname/1"]);
  let body = set.lower_loader("routes/a/page.loader.ts").unwrap_or_else(|e| panic!("{e}"));
  assert_eq!(body_exts(&body), vec!["path.resolve/1", "intl.localTime/2", "time.now/0", "time.localOffset/1", "time.now/0", "text.frontMatter/1"]);
  let why = residue(&mut set, "routes/b/page.tsx#default");
  assert!(why.contains("`path.resolve` is not in `@snapfire/fsr/client`"), "{why}");
}

/// DEFECTS 1.147: the refusal of Node's `fs` names what a web body can use. Using it lowers.
#[test]
fn the_node_fs_refusal_names_the_host_file_module_a_web_action_can_use() {
  let dir = app(
    "node_fs",
    &[
      ("routes/actions.ts", "import { readFile } from \"node:fs/promises\";\nimport { action } from \"@snapfire/fsr/host\";\nexport const note = action(async () => {\n  return { text: await readFile(\"content/a.md\", \"utf8\") };\n});\n"),
      ("routes/fixed/actions.ts", "import { action, file } from \"@snapfire/fsr/host\";\nexport const note = action(async () => {\n  return { text: file.read(\"content/a.md\") };\n});\n"),
    ],
  );
  let mut set = ComponentSet::new(&dir);
  let why = set.lower_actions("routes/actions.ts").err().map(|e| e.to_string()).unwrap_or_default();
  assert!(why.contains("`file`") && why.contains("@snapfire/fsr/host") && why.contains("[files] roots"), "{why}");
  set.lower_actions("routes/fixed/actions.ts").unwrap_or_else(|e| panic!("{e}"));
}

#[test]
fn a_member_the_platform_has_a_form_for_is_refused_with_the_form_to_write() {
  let dir = app(
    "platform_members",
    &[
      ("routes/a/page.loader.ts", "import { intl } from \"@snapfire/fsr/host\";\nexport async function load() {\n  return { label: intl.number(3) };\n}\n"),
      ("routes/b/page.loader.ts", "import { now } from \"@snapfire/fsr/host/time\";\nexport async function load() {\n  return { at: now() };\n}\n"),
      ("routes/c/page.tsx", "import { intl } from \"@snapfire/fsr/client\";\nexport default function Page() {\n  return <p>{intl.compare(\"a\", \"b\")}</p>;\n}\n"),
      ("routes/d/page.loader.ts", "import { encoding } from \"@snapfire/fsr/host\";\nexport async function load() {\n  return { text: encoding.btoa(\"hi\") };\n}\n"),
    ],
  );
  let mut set = ComponentSet::new(&dir);
  let why = set.lower_loader("routes/a/page.loader.ts").err().map(|e| e.to_string()).unwrap_or_default();
  assert!(why.contains("`intl.number` is not imported") && why.contains("`new Intl.NumberFormat(locale, options).format(n)`"), "{why}");
  let why = set.lower_loader("routes/b/page.loader.ts").err().map(|e| e.to_string()).unwrap_or_default();
  assert!(why.contains("`Date.now()`"), "{why}");
  let why = residue(&mut set, "routes/c/page.tsx#default");
  assert!(why.contains("`new Intl.Collator(locale, options).compare(a, b)`"), "{why}");
  let why = set.lower_loader("routes/d/page.loader.ts").err().map(|e| e.to_string()).unwrap_or_default();
  assert!(why.contains("`encoding` is not in `@snapfire/fsr/host`") && why.contains("`btoa(text)`") && why.contains("`new TextDecoder().decode(bytes)`"), "{why}");
}

#[test]
fn a_standard_module_named_as_the_platforms_global_answers_the_platform_spelling() {
  let dir = app("crypto_global", &[("routes/a/page.loader.ts", "import { crypto } from \"@snapfire/fsr/host\";\nexport async function load() {\n  return { id: crypto.randomUUID(), salt: crypto.random(4) };\n}\n")]);
  let body = ComponentSet::new(&dir).lower_loader("routes/a/page.loader.ts").unwrap_or_else(|e| panic!("{e}"));
  assert_eq!(body_exts(&body), vec!["crypto.uuid/0", "crypto.random/1"]);
}
