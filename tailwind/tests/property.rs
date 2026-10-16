//! Properties of the parsers and the card rules over generated and arbitrary input: nothing
//! panics, answers are deterministic, a canonical spelling is a fixed point, printing what was
//! parsed reads back the same and the optimize pass accepts every build.

use bolero::check;
use proptest::prelude::*;
use snapfire_tailwind::{optimize, CompileOptions, Compiler, OptimizeOptions};

const CONFIG: &str = "@import \"tailwindcss\";\n@theme {\n  --color-brand: #1f6feb;\n}\n@utility pill {\n  @apply rounded-full px-2;\n}\n@custom-variant pointer-fine (@media (pointer: fine));\n";

fn compiler() -> Compiler {
  Compiler::new(CONFIG, CompileOptions::default().banner(false)).unwrap()
}

fn variant() -> impl Strategy<Value = String> {
  prop_oneof![
    Just(String::new()),
    prop::sample::select(vec!["hover:", "md:", "lg:", "dark:", "focus-visible:", "group-hover:", "peer-checked:", "aria-[current]:", "data-[state=open]:", "[&>*]:", "*:", "max-sm:", "not-first:", "has-[img]:", "pointer-fine:", "supports-[display:grid]:", "@md:"]).prop_map(str::to_owned),
  ]
}

fn root() -> impl Strategy<Value = &'static str> {
  prop::sample::select(vec!["flex", "grid", "p", "px", "m", "-m", "mt", "gap", "w", "h", "size", "bg", "text", "border", "ring", "rounded", "font", "shadow", "opacity", "translate-x", "rotate", "inset", "z", "leading", "tracking", "pill", "space-x", "divide-y", "from", "grid-cols", "col-span", "aspect", "line-clamp", "underline", "hidden", "sr-only"])
}

fn value() -> impl Strategy<Value = String> {
  prop_oneof![
    Just(String::new()),
    prop::sample::select(vec!["4", "1.5", "px", "full", "auto", "1/2", "red-500", "brand", "sky-500/50", "xl", "2xl", "bold", "inset", "none", "[12px]", "[#123456]", "[length:var(--x)]", "(--my-size)", "[calc(100%-1rem)]", "[url(x.png)]", "lg", "3", "infinity"]).prop_map(|v| format!("-{v}")),
  ]
}

fn candidate() -> impl Strategy<Value = String> {
  (variant(), variant(), root(), value(), prop::bool::ANY).prop_map(|(a, b, root, value, important)| {
    let core = format!("{root}{value}");
    let core = if important { format!("{core}!") } else { core };
    format!("{a}{b}{core}")
  })
}

fn rule() -> impl Strategy<Value = String> {
  let selector = prop::sample::select(vec![".a", ".b:hover", "a > b", ":root", "[data-x=\"1\"]", ".c .d", "&.e"]);
  let declaration = prop::sample::select(vec!["color: red", "margin: 0 .5rem", "--x: 1px", "background: url(x.png)", "width: calc(100% - 1rem)", "content: \"}\"", "@apply flex p-2"]);
  (selector, prop::collection::vec(declaration, 0..4)).prop_map(|(selector, declarations)| format!("{selector} {{ {}; }}", declarations.join("; ")))
}

fn stylesheet() -> impl Strategy<Value = String> {
  let block = prop_oneof![
    rule(),
    rule().prop_map(|r| format!("@media (width >= 40rem) {{ {r} }}")),
    rule().prop_map(|r| format!("@layer components {{ {r} }}")),
    Just("@theme { --color-x: #fff; --spacing: 0.25rem; }".to_owned()),
    Just("@utility tab-4 { tab-size: 4; }".to_owned()),
    Just("@source inline(\"bg-{red,blue}-500\");".to_owned()),
    Just("/* a comment with } and { */".to_owned()),
  ];
  (prop::bool::ANY, prop::collection::vec(block, 0..6)).prop_map(|(tailwind, blocks)| {
    let head = if tailwind { "@import \"tailwindcss\";\n" } else { "" };
    format!("{head}{}", blocks.join("\n"))
  })
}

/// Every prefix and every single-byte deletion of `text` that is still a string.
fn mutations(text: &str) -> Vec<String> {
  let mut out = Vec::new();
  for (i, _) in text.char_indices() {
    out.push(text[..i].to_owned());
    let mut cut = text.to_owned();
    cut.remove(i);
    out.push(cut);
  }
  out
}

proptest! {
  #![proptest_config(ProptestConfig::with_cases(300))]

  #[test]
  fn building_any_candidates_never_panics_and_is_deterministic(candidates in prop::collection::vec(candidate(), 0..12)) {
    let first = compiler().build(candidates.iter().map(String::as_str));
    let second = compiler().build(candidates.iter().map(String::as_str));
    prop_assert_eq!(&first, &second);
  }

  #[test]
  fn the_optimize_pass_accepts_every_build(candidates in prop::collection::vec(candidate(), 0..12)) {
    let css = compiler().build(candidates.iter().map(String::as_str));
    prop_assert!(optimize(&css, &OptimizeOptions::default()).is_ok(), "{}", css);
  }

  #[test]
  fn a_canonical_spelling_is_its_own_canonical_spelling(candidate in candidate()) {
    let compiler = compiler();
    if let Some(canonical) = compiler.canonical_candidate(&candidate) {
      prop_assert_eq!(compiler.canonical_candidate(&canonical), Some(canonical));
    }
  }

  #[test]
  fn card_rules_never_panic_and_name_only_given_candidates(candidates in prop::collection::vec(candidate(), 0..12)) {
    let compiler = compiler();
    let rules = compiler.card_rules(candidates.iter().map(String::as_str));
    for rule in &rules {
      prop_assert!(candidates.contains(&rule.candidate), "{}", rule.candidate);
      prop_assert!(rule.declarations.iter().all(|(property, _)| !property.starts_with("--")), "{:?}", rule.declarations);
    }
    prop_assert_eq!(rules, compiler.card_rules(candidates.iter().map(String::as_str)));
  }

  #[test]
  fn any_generated_stylesheet_compiles_or_is_refused_without_panicking(css in stylesheet()) {
    if let Ok(mut compiler) = Compiler::new(&css, CompileOptions::default().banner(false)) {
      let _ = compiler.build(["flex", "bg-x", "tab-4"]);
    }
  }

  #[test]
  fn a_stylesheet_without_tailwind_reads_back_as_it_printed(css in stylesheet()) {
    let plain = css.replace("@import \"tailwindcss\";\n", "");
    let printed = |text: &str| Compiler::new(text, CompileOptions::default().banner(false)).ok().map(|mut c| c.build([]));
    if let Some(once) = printed(&plain) {
      prop_assert_eq!(printed(&once), Some(once.clone()), "{}", plain);
    }
  }

}

proptest! {
  #![proptest_config(ProptestConfig::with_cases(25))]

  #[test]
  fn a_cut_or_mutated_stylesheet_never_panics(css in stylesheet()) {
    for text in mutations(&css) {
      if let Ok(mut compiler) = Compiler::new(&text, CompileOptions::default()) {
        let _ = compiler.build(["flex"]);
      }
    }
  }
}

#[test]
fn fuzz_any_text_as_a_stylesheet() {
  check!().with_type::<String>().for_each(|css| {
    if let Ok(mut compiler) = Compiler::new(css, CompileOptions::default()) {
      let _ = compiler.build(["flex", "p-4"]);
    }
  });
}

#[test]
fn fuzz_any_text_as_candidates() {
  check!().with_type::<Vec<String>>().for_each(|candidates| {
    let mut compiler = compiler();
    let _ = compiler.build(candidates.iter().map(String::as_str));
    for candidate in candidates {
      let _ = compiler.canonical_candidate(candidate);
      let _ = compiler.candidate_css(candidate);
    }
    let _ = compiler.card_rules(candidates.iter().map(String::as_str));
  });
}
