//! The class scan over what Svelte's own compiler makes of components whose
//! classes are known. Every class a component names is found. No cut or
//! mutation of compiled output makes the scan panic.

use std::path::Path;

use proptest::prelude::*;
use snapfire_compiler_wire::{Options, Outcome};
use snapfire_svelte::Compiler;

use super::{source_classes, unescape};

thread_local! {
  static COMPILER: Compiler = Compiler::new().expect("the compiler boots");
}

fn compile(source: &str) -> String {
  COMPILER.with(|compiler| match compiler.compile("src/C.svelte", source, &Options { production: true, ..Options::default() }, &Default::default()).expect("the driver answers") {
    Outcome::Ok(compiled) => compiled.js,
    other => panic!("{other:?}:\n{source}"),
  })
}

fn token() -> impl Strategy<Value = String> {
  prop::sample::select(vec!["p-4", "text-lg", "md:flex", "hover:bg-red-500", "w-1/2", "[&>*]:mt-2", "bg-[#123456]", "-mt-2", "font-bold", "ring-2", "é-x", "data-[open]:block"]).prop_map(str::to_owned)
}

fn tag() -> impl Strategy<Value = &'static str> {
  prop::sample::select(vec!["div", "p", "span", "button", "svg", "a"])
}

/// One element and the classes it names, written each way a component names a class.
fn element() -> impl Strategy<Value = (String, Vec<String>)> {
  let static_list = (tag(), prop::collection::vec(token(), 1..4)).prop_map(|(tag, tokens)| (format!("<{tag} class=\"{}\">x</{tag}>", tokens.join(" ")), tokens));
  let directive = (tag(), prop::sample::select(vec!["ring-2", "font-bold", "underline", "p-4", "é-x"])).prop_map(|(tag, token)| (format!("<{tag} class:{token}={{on}}>x</{tag}>"), vec![token.to_owned()]));
  let literal = (tag(), prop::collection::vec(token(), 1..3)).prop_map(|(tag, tokens)| (format!("<{tag} class={{on ? \"{}\" : \"base\"}}>x</{tag}>", tokens.join(" ")), [tokens, vec!["base".to_owned()]].concat()));
  let attributes = (tag(), token()).prop_map(|(tag, token)| (format!("<{tag} title=\"class=&quot;nope&quot;\" class=\"{token}\" data-k=\"v\">x</{tag}>"), vec![token]));
  let nested = (tag(), token(), token()).prop_map(|(tag, outer, inner)| (format!("<{tag} class=\"{outer}\"><span class=\"{inner}\">{{label}}</span></{tag}>"), vec![outer, inner]));
  prop_oneof![static_list, directive, literal, attributes, nested]
}

fn component() -> impl Strategy<Value = (String, Vec<String>)> {
  (prop::bool::ANY, prop::collection::vec(element(), 1..5)).prop_map(|(runes, elements)| {
    let script = match runes {
      true => "<script>\n  let { on = false, label = \"l\" } = $props();\n</script>\n",
      false => "<script>\n  export let on = false;\n  export let label = \"l\";\n</script>\n",
    };
    let markup: Vec<String> = elements.iter().map(|(m, _)| m.clone()).collect();
    let expected = elements.into_iter().flat_map(|(_, c)| c).collect();
    (format!("{script}{}\n", markup.join("\n")), expected)
  })
}

proptest! {
  #![proptest_config(ProptestConfig::with_cases(128))]

  #[test]
  fn the_scan_finds_every_class_a_compiled_component_names((source, expected) in component()) {
    let js = compile(&source);
    let found = source_classes(Path::new("src/C.svelte"), &js);
    for class in &expected {
      prop_assert!(found.contains(class), "{} missing from {:?}\n{}\n{}", class, found, source, js);
    }
    prop_assert!(!found.iter().any(|c| c == "nope" || c.contains("class=")), "{:?}", found);
  }
}

#[test]
fn no_cut_or_mutation_of_compiled_output_panics_the_scan() {
  let js = compile("<script>\n  let { on = false } = $props();\n</script>\n<div class=\"p-4 é-x\"><p class={on ? \"a\" : \"b\"} class:ring-2={on}>t</p></div>\n");
  for (cut, _) in js.char_indices() {
    let _ = source_classes(Path::new("src/C.svelte"), &js[..cut]);
    let _ = source_classes(Path::new("src/C.svelte"), &js[cut..]);
  }
  bolero::check!().with_iterations(2000).for_each(|bytes: &[u8]| {
    let mut mutated = js.clone().into_bytes();
    for pair in bytes.chunks(2) {
      if let [at, byte] = pair {
        let at = *at as usize * mutated.len() / 256;
        if at < mutated.len() {
          mutated[at] = *byte;
        }
      }
    }
    let _ = source_classes(Path::new("src/C.svelte"), &String::from_utf8_lossy(&mutated));
  });
}

proptest! {
  #[test]
  fn unescaping_reads_back_what_an_attribute_escape_wrote(value in "[ -~é🙂]{0,24}") {
    let escaped = value.replace('&', "&amp;").replace('"', "&quot;").replace('<', "&#60;").replace('>', "&#x3e;");
    let read = unescape(&escaped).into_owned();
    prop_assert_eq!(read, value);
  }
}

#[test]
fn unescaping_any_text_never_panics() {
  bolero::check!().for_each(|bytes: &[u8]| {
    let _ = unescape(&String::from_utf8_lossy(bytes));
  });
  for text in ["&", "&;", "&amp", "&#;", "&#x;", "&#xD800;", "&#99999999999;", "&é;", "&amp;&", "a&b;c"] {
    let _ = unescape(text);
  }
}
