//! Svelte's own server renderer, reached through the plugin: the oracle a
//! lowered component's markup is compared against.

use std::path::PathBuf;

use snapfire_compiler_wire::Outcome;
use snapfire_svelte::Compiler;

fn runtime() -> PathBuf {
  PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/runtime")
}

/// Renders the first of `components`, each a path and its source, the rest reachable by import.
fn render(compiler: &Compiler, components: &[(&str, &str)], props: &str) -> String {
  let mut modules = std::collections::BTreeMap::new();
  for (path, source) in components {
    let js = match compiler.ssr_module(path, source).expect("the driver answers") {
      Outcome::Ok(compiled) => compiled.js,
      other => panic!("{other:?}"),
    };
    modules.insert(format!("/{path}"), js);
  }
  let entry = format!("/{}", components[0].0);
  compiler.render_module(&entry, &modules, props, &runtime()).unwrap_or_else(|e| panic!("{e}"))
}

#[test]
fn a_component_renders_through_svelte_server_with_its_props() {
  let compiler = Compiler::new().unwrap();
  let source = "<script>\n  let { title, rows = [] } = $props();\n  let n = $state(2);\n  const doubled = $derived(n * 2);\n</script>\n\n<h1 class=\"t\">{title}</h1>\n{#if rows.length}<ul>{#each rows as row (row)}<li>{row}</li>{/each}</ul>{:else}<p>none</p>{/if}\n<p>{n} {doubled}</p>\n";
  let body = render(&compiler, &[("src/C.svelte", source)], r#"{"title":"Hi & bye","rows":["a","b"]}"#);
  eprintln!("{body}");
  assert!(body.contains("<h1 class=\"t\">Hi &amp; bye</h1>"), "{body}");
  assert!(body.contains("<li>a</li>"), "{body}");
  assert!(body.contains("<!--["), "Svelte's hydration markers: {body}");
}

#[test]
fn a_component_renders_the_components_it_imports() {
  let compiler = Compiler::new().unwrap();
  let parent = "<script>\n  import Card from './Card.svelte';\n  let { titles } = $props();\n</script>\n\n{#each titles as title}<Card {title}><b>{title}</b></Card>{/each}\n";
  let card = "<script>\n  let { title, children } = $props();\n</script>\n\n<section><h2>{title}</h2>{@render children?.()}</section>\n";
  let body = render(&compiler, &[("src/List.svelte", parent), ("src/Card.svelte", card)], r#"{"titles":["a","b"]}"#);
  assert!(body.contains("<section><h2>a</h2>"), "{body}");
  assert!(body.contains("<b>b</b>"), "{body}");
}
