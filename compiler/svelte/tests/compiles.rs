use snapfire_compiler_wire::{Lang, Options, Outcome, Severity};
use snapfire_svelte::Compiler;

const CARD: &str = r#"<script lang="ts">
  let { title, count }: { title: string; count: number } = $props();
  let n = $state(count);
  const doubled = $derived(n * 2);
</script>

<div class="card">
  <h2>{title}</h2>
  <button onclick={() => n++}>{n} / {doubled}</button>
</div>

<style>
  .card { padding: 12px; }
</style>
"#;

const LEGACY: &str = r#"<script>
  export let title;
  export let count = 0;
  $: doubled = count * 2;
</script>

<div class="card">
  <h2>{title}</h2>
  <button on:click={() => count++}>{count} / {doubled}</button>
</div>

<style>
  .card { padding: 12px; }
</style>
"#;

fn compile(source: &str) -> Outcome {
  compile_with(source, &Options::default())
}

fn compile_with(source: &str, options: &Options) -> Outcome {
  let compiler = Compiler::new().expect("the compiler boots");
  compiler.compile("src/Card.svelte", source, options, &Default::default()).expect("the driver answers")
}

fn ok(outcome: Outcome) -> snapfire_compiler_wire::Compiled {
  match outcome {
    Outcome::Ok(compiled) => compiled,
    Outcome::Described(_) => panic!("described where a compile was asked"),
    Outcome::Failed { diagnostics } => panic!("refused: {diagnostics:?}"),
    Outcome::Needs { files } => panic!("asked for {files:?}"),
  }
}

#[test]
fn the_carried_compiler_names_its_version() {
  let compiler = Compiler::new().expect("the compiler boots");
  assert_eq!(compiler.version().expect("a version"), "5.57.2");
}

#[test]
fn a_runes_component_becomes_one_browser_module_and_its_styles() {
  let compiled = ok(compile(CARD));

  assert!(compiled.js.contains("from 'svelte/internal/client'"), "the module runs on Svelte's client runtime: {}", compiled.js);
  assert!(compiled.js.contains("export default function"), "one default export");
  assert!(!compiled.js.contains("svelte/internal/server"), "a browser build, not a server one");

  let css = compiled.css.expect("a style block produces css");
  assert!(css.contains(".card.svelte-"), "the selector is scoped to the component: {css}");
  assert!(css.contains("padding: 12px"), "the declaration survived");
  assert!(compiled.js.contains("svelte-"), "the markup carries the scope class the selector reads");
}

#[test]
fn a_legacy_component_compiles_for_the_browser_as_well() {
  let compiled = ok(compile(LEGACY));
  assert!(compiled.js.contains("from 'svelte/internal/client'"));
  assert!(compiled.js.contains("export default function"));
  assert!(compiled.css.expect("css").contains(".card.svelte-"));
}

#[test]
fn a_typed_script_is_stripped_by_svelte_so_the_module_is_javascript() {
  let compiled = ok(compile(CARD));
  assert_eq!(compiled.lang, Lang::Js);
  assert!(!compiled.js.contains("title: string"), "the annotation is gone: {}", compiled.js);
}

#[test]
fn a_component_with_no_style_has_no_stylesheet() {
  let compiled = ok(compile("<p>just markup</p>\n"));
  assert!(compiled.js.contains("export default function"));
  assert!(compiled.css.is_none());
}

#[test]
fn markup_that_does_not_parse_is_refused_with_its_line() {
  let outcome = compile("<div>\n  {#if open}\n    <p>x</p>\n</div>\n");
  let Outcome::Failed { diagnostics } = outcome else { panic!("an unclosed block should be refused") };
  assert!(!diagnostics.is_empty());
  assert_eq!(diagnostics[0].severity, Severity::Error);
  assert_eq!(diagnostics[0].file.as_deref(), Some("src/Card.svelte"));
  assert!(diagnostics[0].line.is_some(), "a location came back: {:?}", diagnostics[0]);
}

#[test]
fn a_style_language_the_plugin_does_not_carry_is_named_rather_than_emitted_wrong() {
  let outcome = compile("<p class=\"a\">x</p>\n<style lang=\"scss\">$c: red; .a { color: $c }</style>\n");
  let Outcome::Failed { diagnostics } = outcome else { panic!("scss should be refused") };
  assert!(diagnostics.iter().any(|d| d.message.contains("scss")), "the language is named: {diagnostics:?}");
}

#[test]
fn a_warning_is_reported_without_stopping_the_compile() {
  let compiled = ok(compile("<img src=\"a.png\">\n"));
  assert!(compiled.diagnostics.iter().any(|d| d.severity == Severity::Warning && d.message.contains("alt")), "{:?}", compiled.diagnostics);
}

#[test]
fn production_leaves_out_what_development_carries_for_the_developer() {
  let development = ok(compile_with(CARD, &Options { production: false, ..Options::default() }));
  let production = ok(compile_with(CARD, &Options { production: true, ..Options::default() }));
  assert!(development.js.contains("src/Card.svelte"), "development names the file for the runtime's warnings");
  assert!(!production.js.contains("src/Card.svelte"), "production does not");
}

#[test]
fn one_context_compiles_many_components() {
  let compiler = Compiler::new().expect("the compiler boots");
  for i in 0..25 {
    let source = CARD.replace("padding: 12px", &format!("padding: {i}px"));
    let outcome = compiler.compile(&format!("src/Card{i}.svelte"), &source, &Options::default(), &Default::default()).expect("answers");
    assert!(matches!(outcome, Outcome::Ok(_)), "component {i} compiled");
  }
}

/// A `.svelte.ts` module as the host sends it: its types already stripped.
const COUNTER: &str = r#"export const counter = $state({ n: 0 });

export function bump(by = 1) {
  counter.n += by;
}
"#;

#[test]
fn a_runes_module_the_host_stripped_compiles_to_javascript_on_svelte_s_runtime() {
  let compiler = Compiler::new().expect("the compiler boots");
  let compiled = ok(compiler.compile("src/counter.svelte.ts", COUNTER, &Options::default(), &Default::default()).expect("answers"));
  assert_eq!(compiled.lang, Lang::Js);
  assert!(compiled.js.contains("from 'svelte/internal/client'"), "the rune became a runtime call: {}", compiled.js);
  assert!(!compiled.js.contains("$state("), "no rune is left for the browser: {}", compiled.js);
  assert!(compiled.js.contains("export function bump"), "the module keeps its exports");
  assert!(compiled.css.is_none());
}

#[test]
fn a_plain_javascript_runes_module_compiles_too() {
  let compiler = Compiler::new().expect("the compiler boots");
  let compiled = ok(compiler.compile("src/counter.svelte.js", "export const counter = $state({ n: 0 });\n", &Options::default(), &Default::default()).expect("answers"));
  assert!(!compiled.js.contains("$state("), "{}", compiled.js);
}

#[test]
fn a_source_map_comes_back_when_one_is_asked_for() {
  let with = ok(compile_with(CARD, &Options { source_map: true, ..Options::default() }));
  let map = with.source_map.expect("a map");
  assert!(map.contains("\"mappings\""), "a source map: {map}");
  assert!(map.contains("Card.svelte"), "naming the component: {map}");
  assert!(ok(compile(CARD)).source_map.is_none(), "and none when it is not");
}

/// A stand-in for an application's own `compiler/index.js`: the same global, a version of its own.
const OTHER_COMPILER: &str = r#"(function () {
  globalThis.svelte = {
    VERSION: "9.9.9",
    parse: function () { return {}; },
    compile: function () { return { js: { code: "// compiled by 9.9.9\nexport default function Card() {}\n", map: null }, css: null, warnings: [] }; },
    compileModule: function () { return { js: { code: "// module by 9.9.9\n", map: null }, css: null, warnings: [] }; }
  };
})();
"#;

fn compiler_file(name: &str, text: &str) -> String {
  let path = std::env::temp_dir().join(format!("snapfire-svelte-{name}-{}.js", std::process::id()));
  std::fs::write(&path, text).unwrap();
  path.to_string_lossy().into_owned()
}

#[test]
fn a_compiler_the_host_names_compiles_in_place_of_the_carried_one() {
  let path = compiler_file("other", OTHER_COMPILER);
  let compiler = Compiler::new().expect("the compiler boots");
  let named = Options { compiler: Some(path.clone()), ..Options::default() };
  let compiled = ok(compiler.compile("src/Card.svelte", CARD, &named, &Default::default()).expect("answers"));
  assert!(compiled.js.contains("compiled by 9.9.9"), "the application's compiler answered: {}", compiled.js);
  let module = ok(compiler.compile("src/counter.svelte.ts", COUNTER, &named, &Default::default()).expect("answers"));
  assert!(module.js.contains("module by 9.9.9"), "{}", module.js);
  let carried = ok(compiler.compile("src/Card.svelte", CARD, &Options::default(), &Default::default()).expect("answers"));
  assert!(carried.js.contains("from 'svelte/internal/client'"), "with no path the carried compiler answers: {}", carried.js);
  std::fs::remove_file(path).unwrap();
}

#[test]
fn a_compiler_path_that_holds_nothing_is_refused_by_name() {
  let compiler = Compiler::new().expect("the compiler boots");
  let missing = std::env::temp_dir().join("snapfire-svelte-no-such-compiler.js").to_string_lossy().into_owned();
  let named = Options { compiler: Some(missing.clone()), ..Options::default() };
  let Outcome::Failed { diagnostics } = compiler.compile("src/Card.svelte", CARD, &named, &Default::default()).expect("answers") else { panic!("refused") };
  assert!(diagnostics[0].message.contains(&missing), "{diagnostics:?}");
}

fn describe(source: &str) -> Outcome {
  let compiler = Compiler::new().expect("the compiler boots");
  compiler.describe("src/Card.svelte", source, &Options::default()).expect("the driver answers")
}

#[test]
fn a_description_carries_the_scripts_where_they_stand_and_the_error_that_keeps_it_foreign() {
  let source = "<script module>\n  import { a } from \"./a.js\";\n</script>\n\n<script lang=\"ts\">\n  import { b } from \"@src/b\";\n  let n: number = a + b;\n</script>\n\n<p>{n}</p>\n";
  let Outcome::Described(described) = describe(source) else { panic!("not described") };
  let script = described.script.expect("a script");
  assert_eq!(script.lang, Lang::Ts);
  assert_eq!((script.line, script.column), (1, 1));
  assert_eq!(script.content.len(), source.len());
  assert_eq!(script.content.lines().nth(1), Some("  import { a } from \"./a.js\";"));
  assert_eq!(script.content.lines().nth(5), Some("  import { b } from \"@src/b\";"));
  assert!(!script.content.contains("<p>") && !script.content.contains("script"), "{}", script.content);
  assert!(described.template.is_some());
  assert_eq!(described.diagnostics.len(), 1);
  assert_eq!(described.diagnostics[0].severity, Severity::Error);
  assert!(described.diagnostics[0].message.contains("does not describe a component yet"));
}

#[test]
fn a_legacy_component_is_described_as_javascript_and_one_without_a_script_carries_none() {
  let Outcome::Described(legacy) = describe(LEGACY) else { panic!("not described") };
  assert_eq!(legacy.script.expect("a script").lang, Lang::Js);
  let Outcome::Described(bare) = describe("<p>hi</p>\n") else { panic!("not described") };
  assert!(bare.script.is_none());
  assert_eq!(bare.diagnostics[0].severity, Severity::Error);
}

#[test]
fn a_description_of_what_does_not_parse_or_a_style_it_cannot_carry_fails_with_the_cause() {
  let Outcome::Failed { diagnostics } = describe("<p>{</p>\n") else { panic!("described a broken component") };
  assert_eq!(diagnostics[0].line, Some(1));
  let Outcome::Failed { diagnostics } = describe("<p>hi</p>\n<style lang=\"scss\">\n</style>\n") else { panic!("described scss") };
  assert!(diagnostics[0].message.contains("lang=\"scss\""), "{diagnostics:?}");
}

fn template(source: &str) -> serde_json::Value {
  let Outcome::Described(described) = describe(source) else { panic!("not described") };
  described.template.expect("a template")
}

fn kinds(fragment: &serde_json::Value) -> Vec<String> {
  fragment["nodes"].as_array().unwrap().iter().map(|n| match n["type"].as_str().unwrap() {
    "text" => format!("text {:?}", n["data"].as_str().unwrap()),
    other => other.to_owned(),
  }).collect()
}

#[test]
fn a_template_is_described_with_whitespace_cleaned_as_svelte_cleans_it() {
  let t = template("\n\n<p>a</p>\n\n  <p>b</p>\n");
  assert_eq!(kinds(&t["fragment"]), ["element", "text \" \"", "element"]);
  let t = template("<ul>\n  <li>a</li>\n</ul><svg>\n  <path d=\"M0\"/>\n</svg><p>  a  {b}  c  </p>");
  let nodes = t["fragment"]["nodes"].as_array().unwrap();
  assert_eq!(kinds(&nodes[0]["fragment"]), ["element"], "a list keeps no whitespace around its one item");
  assert_eq!(kinds(&nodes[1]["fragment"]), ["element"], "whitespace between svg nodes goes");
  assert_eq!(kinds(&nodes[2]["fragment"]), ["text \"a  \"", "expr", "text \"  c\""], "text beside an expression keeps its own spacing");
  let t = template("<pre>\n<b>a</b>\n</pre><pre>\n  a\n</pre>");
  assert_eq!(kinds(&t["fragment"]["nodes"][0]["fragment"]), ["element", "text \"\\n\""], "a pre drops a first text that is only a newline");
  assert_eq!(kinds(&t["fragment"]["nodes"][1]["fragment"]), ["text \"\\n  a\\n\""], "and keeps one that holds more");
}

#[test]
fn each_fragment_says_whether_it_is_text_first_or_one_static_component() {
  let t = template("hello");
  assert_eq!(t["fragment"]["text_first"], true);
  let t = template("<script>import C from './C.svelte'; let { xs } = $props();</script>{#if xs}<C/>{/if}<div>{xs}</div>");
  let branch = &t["fragment"]["nodes"][0]["consequent"];
  assert_eq!(branch["standalone"], true);
  assert_eq!(branch["text_first"], false, "an if branch never adds the anchor");
  assert_eq!(t["fragment"]["nodes"][1]["fragment"]["text_first"], false, "nor does an element's content");
}

#[test]
fn a_component_or_a_render_is_dynamic_unless_its_name_binds_as_svelte_calls_normal() {
  let t = template("<script>import C from './C.svelte'; let { children, Other } = $props(); let Local = $state(C);</script>{#snippet row()}<i>r</i>{/snippet}<C/><Other/><Local/>{@render row()}{@render children?.()}");
  let nodes = t["fragment"]["nodes"].as_array().unwrap();
  let dynamic: Vec<bool> = nodes.iter().map(|n| n["dynamic"].as_bool().unwrap()).collect();
  assert_eq!(dynamic, [false, true, true, false, true]);
  assert_eq!(t["fragment"]["hoisted"][0]["type"], "snippet");
  assert_eq!(t["runes"], true);
  let legacy = template("<script>export let a = 1;</script><p>{a}</p>");
  assert_eq!(legacy["runes"], false);
}
