//! Svelte's own server renderer, reached through the plugin: the oracle a
//! lowered component's markup is compared against.

use std::path::PathBuf;

use snapfire_compiler_wire::Outcome;
use snapfire_svelte::Compiler;

fn runtime() -> PathBuf {
  PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/runtime")
}

fn render(compiler: &Compiler, source: &str, props: &str) -> String {
  let js = match compiler.ssr_module("src/C.svelte", source).expect("the driver answers") {
    Outcome::Ok(compiled) => compiled.js,
    other => panic!("{other:?}"),
  };
  compiler.render_module(&js, props, &runtime()).unwrap_or_else(|e| panic!("{e}\n{js}"))
}

#[test]
fn a_component_renders_through_svelte_server_with_its_props() {
  let compiler = Compiler::new().unwrap();
  let source = "<script>\n  let { title, rows = [] } = $props();\n  let n = $state(2);\n  const doubled = $derived(n * 2);\n</script>\n\n<h1 class=\"t\">{title}</h1>\n{#if rows.length}<ul>{#each rows as row (row)}<li>{row}</li>{/each}</ul>{:else}<p>none</p>{/if}\n<p>{n} {doubled}</p>\n";
  let body = render(&compiler, source, r#"{"title":"Hi & bye","rows":["a","b"]}"#);
  eprintln!("{body}");
  assert!(body.contains("<h1 class=\"t\">Hi &amp; bye</h1>"), "{body}");
  assert!(body.contains("<li>a</li>"), "{body}");
  assert!(body.contains("<!--["), "Svelte's hydration markers: {body}");
}
