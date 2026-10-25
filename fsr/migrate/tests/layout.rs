//! An FSR application on the old specifiers, rewritten onto `@snapfire/fsr/client`, `/host`, `/terminal`, `/template` and `/components`.

use snapfire_fsr_migrate::{is_old_layout, relayout, Tree};

fn rewritten(files: &[(&str, &str)]) -> Tree {
  let mut tree = Tree::new();
  for (path, text) in files {
    tree.insert(*path, text.as_bytes().to_vec());
  }
  assert!(is_old_layout(&tree));
  relayout(&tree).0
}

fn text<'a>(tree: &'a Tree, path: &str) -> &'a str {
  tree.text(path).unwrap()
}

#[test]
fn a_component_takes_the_client_side_and_a_loader_the_host_side() {
  let tree = rewritten(&[
    ("app/routes/page.tsx", "import { intl, text } from \"@snapfire/fsr-client/std\";\nimport { useStore } from \"@snapfire/fsr-client/react\";\nimport { set } from \"@snapfire/fsr-client\";\nexport default function P({ n }: { n: number }) {\n  return <p>{intl.number(n)} {text.slug(\"A b\")}</p>;\n}\n"),
    ("app/routes/page.loader.ts", "import type { Ctx } from \"@snapfire/fsr\";\nimport { fail } from \"@snapfire/fsr\";\nimport { intl, time } from \"@snapfire/fsr-client/std\";\nexport async function load(ctx: Ctx<\"/\">) {\n  return { n: intl.number(1), at: time.now() };\n}\n"),
  ]);
  let page = text(&tree, "app/routes/page.tsx");
  assert!(page.contains("import { intl, text } from \"@snapfire/fsr/client\";"), "{page}");
  assert!(page.contains("from \"@snapfire/fsr/client/react\"") && page.contains("import { set } from \"@snapfire/fsr/client\";"), "{page}");
  let loader = text(&tree, "app/routes/page.loader.ts");
  assert!(loader.contains("import type { Ctx } from \"@snapfire/fsr/host\";") && loader.contains("import { fail } from \"@snapfire/fsr/host\";"), "{loader}");
  assert!(loader.contains("import { intl, time } from \"@snapfire/fsr/host\";"), "{loader}");
}

#[test]
fn a_helper_reading_a_host_member_takes_the_host_side_and_others_the_client_side() {
  let tree = rewritten(&[(
    "app/src/clock.ts",
    "import { time, intl, native } from \"@snapfire/fsr-client/std\";\nexport const stamp = () => time.now();\nexport const label = (n: number) => intl.number(n);\nexport const pretty = native(\"fmt.pretty\", (n: number) => String(n));\n",
  )]);
  let clock = text(&tree, "app/src/clock.ts");
  assert!(clock.contains("import { native } from \"@snapfire/fsr\";"), "{clock}");
  assert!(clock.contains("import { intl } from \"@snapfire/fsr/client\";"), "{clock}");
  assert!(clock.contains("import { time } from \"@snapfire/fsr/host\";"), "{clock}");
}

#[test]
fn a_terminal_page_takes_the_host_side_and_the_terminal_root() {
  let tree = rewritten(&[(
    "app/routes/page.tsx",
    "import { Text } from \"@snapfire/fsr/terminal/ink\";\nimport { system, intl, type FileStat } from \"@snapfire/fsr-client/std\";\nexport default function P() {\n  return <Text>{intl.number(system.memory())}</Text>;\n}\n",
  )]);
  let page = text(&tree, "app/routes/page.tsx");
  assert!(page.contains("import { system } from \"@snapfire/fsr/terminal\";"), "{page}");
  assert!(page.contains("import { intl } from \"@snapfire/fsr/host\";"), "{page}");
  assert!(page.contains("import { type FileStat } from \"@snapfire/fsr/host/file\";"), "{page}");
}

#[test]
fn island_goes_to_the_components_and_the_rest_of_the_template_dialect_to_its_own_path() {
  let tree = rewritten(&[("app/routes/layout.tsx", "import { Island, type Children } from \"@snapfire/fsr-authoring/template\";\nexport default function L({ children }: { children: Children }) {\n  return <main><Island>{children}</Island></main>;\n}\n")]);
  let layout = text(&tree, "app/routes/layout.tsx");
  assert!(layout.contains("import { Island } from \"@snapfire/fsr/components\";") && layout.contains("import { type Children } from \"@snapfire/fsr/template\";"), "{layout}");
}

#[test]
fn a_vue_file_takes_the_client_side() {
  let tree = rewritten(&[("app/src/ui/Card.vue", "<script setup lang=\"ts\">\nimport { intl } from \"@snapfire/fsr-client/std\";\nimport { useStore } from \"@snapfire/fsr-client/vue\";\n</script>\n")]);
  let card = text(&tree, "app/src/ui/Card.vue");
  assert!(card.contains("from \"@snapfire/fsr/client\";") && card.contains("from \"@snapfire/fsr/client/vue\";") && !card.contains("fsr-client"), "{card}");
}

#[test]
fn the_import_map_renames_its_keys_and_lists_each_client_module() {
  let tree = rewritten(&[(
    "app/importmap.json",
    "{\n  \"imports\": {\n    \"@snapfire/fsr-client\": \"/static/js/fsr/index.js\",\n    \"@snapfire/fsr-client/std\": \"/static/js/fsr/std.js\",\n    \"@snapfire/fsr-client/react\": \"/static/js/fsr/react.js\"\n  }\n}\n",
  )]);
  let map: serde_json::Value = serde_json::from_str(text(&tree, "app/importmap.json")).unwrap();
  let imports = map["imports"].as_object().unwrap();
  assert_eq!(imports["@snapfire/fsr/client"], "/static/js/fsr/index.js");
  assert_eq!(imports["@snapfire/fsr/client/react"], "/static/js/fsr/react.js");
  assert_eq!(imports["@snapfire/fsr/client/intl"], "/static/js/fsr/intl.js");
  assert_eq!(imports["@snapfire/fsr"], "/static/js/fsr/native.js");
  assert!(!imports.keys().any(|k| k.contains("fsr-client")), "{map}");
}
