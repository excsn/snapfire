//! Properties of the card reader over generated classes and arbitrary text: nothing panics,
//! answers are deterministic and only given classes come back, with no custom property left.

use std::collections::BTreeSet;

use bolero::check;
use proptest::prelude::*;
use snapfire_fsr_cli::cards::card_rules;
use snapfire_tailwind::{CompileOptions, Compiler, PolyfillFlags};

const CONFIG: &str = "@import \"tailwindcss\";\n@theme {\n  --color-brand: #1f6feb;\n}\n@utility pill {\n  @apply rounded-full px-2;\n}\n";

fn built(classes: &[String]) -> String {
  let mut compiler = Compiler::new(CONFIG, CompileOptions::default().banner(false).polyfills(PolyfillFlags::NONE)).unwrap();
  compiler.build(classes.iter().map(String::as_str))
}

fn class() -> impl Strategy<Value = String> {
  let variant = prop::sample::select(vec!["", "hover:", "md:", "lg:", "2xl:", "dark:", "[&>*]:", "max-sm:", "@md:"]);
  let root = prop::sample::select(vec!["flex", "p", "px", "-m", "gap", "w", "bg", "text", "border", "rounded", "font", "shadow", "opacity", "inset", "leading", "pill", "space-x", "underline", "hidden"]);
  let value = prop::sample::select(vec!["", "-4", "-1.5", "-px", "-full", "-1/2", "-red-500", "-brand", "-sky-500/50", "-xl", "-bold", "-[12px]", "-[#123456]", "-(--my-size)", "-infinity"]);
  (variant, root, value).prop_map(|(v, r, x)| format!("{v}{r}{x}"))
}

proptest! {
  #[test]
  fn card_rules_never_panic_and_name_only_given_classes(classes in prop::collection::vec(class(), 0..12)) {
    let css = built(&classes);
    let given: BTreeSet<String> = classes.iter().cloned().collect();
    let rules = card_rules(&css, &given);
    for rule in &rules {
      prop_assert!(given.contains(&rule.class), "{}", rule.class);
      prop_assert!(rule.declarations.iter().all(|(property, _)| !property.starts_with("--")), "{:?}", rule.declarations);
    }
    prop_assert_eq!(rules, card_rules(&css, &given));
  }
}

#[test]
fn fuzz_any_text_as_a_stylesheet() {
  check!().with_type::<(String, Vec<String>)>().for_each(|(css, classes)| {
    let _ = card_rules(css, &classes.iter().cloned().collect());
  });
}
