//! Utilities as card declarations: variables resolved, colors in sRGB, a breakpoint as a width.

use snapfire_tailwind::{CardRule, CompileOptions, Compiler};

fn rules(candidates: &[&str]) -> Vec<CardRule> {
  let options = CompileOptions::default().banner(false);
  let compiler = Compiler::new("@import \"tailwindcss\";\n@theme {\n  --color-brand: #e11d48;\n}", options).unwrap();
  compiler.card_rules(candidates.iter().copied())
}

fn of<'a>(rules: &'a [CardRule], candidate: &str) -> Option<&'a CardRule> {
  rules.iter().find(|r| r.candidate == candidate)
}

fn decls(rule: &CardRule) -> Vec<(&str, &str)> {
  rule.declarations.iter().map(|(p, v)| (p.as_str(), v.as_str())).collect()
}

#[test]
fn a_utility_becomes_plain_declarations() {
  let all = rules(&["flex", "gap-4", "p-8", "font-bold", "text-4xl", "bg-sky-500", "bg-brand", "bg-white/25", "rounded-full"]);
  assert_eq!(decls(of(&all, "flex").unwrap()), [("display", "flex")]);
  assert_eq!(decls(of(&all, "gap-4").unwrap()), [("gap", "1rem")]);
  assert_eq!(decls(of(&all, "p-8").unwrap()), [("padding", "2rem")]);
  assert_eq!(decls(of(&all, "font-bold").unwrap()), [("font-weight", "700")]);
  assert_eq!(decls(of(&all, "text-4xl").unwrap()), [("font-size", "2.25rem"), ("line-height", "1.1111")]);
  assert_eq!(decls(of(&all, "bg-sky-500").unwrap()), [("background-color", "#00a6f4")]);
  assert_eq!(decls(of(&all, "bg-brand").unwrap()), [("background-color", "#e11d48")]);
  assert_eq!(decls(of(&all, "bg-white/25").unwrap()), [("background-color", "rgba(255, 255, 255, 0.25)")]);
  assert_eq!(decls(of(&all, "rounded-full").unwrap()), [("border-radius", "9999px")]);
  let sides = rules(&["px-4", "py-1", "mx-auto"]);
  assert_eq!(decls(of(&sides, "px-4").unwrap()), [("padding-left", "1rem"), ("padding-right", "1rem")]);
  assert_eq!(decls(of(&sides, "py-1").unwrap()), [("padding-top", "0.25rem"), ("padding-bottom", "0.25rem")]);
  assert_eq!(decls(of(&sides, "mx-auto").unwrap()), [("margin-left", "auto"), ("margin-right", "auto")]);
}

#[test]
fn a_breakpoint_applies_from_its_width_and_an_interaction_never() {
  let all = rules(&["text-4xl", "md:text-6xl", "hover:bg-red-500", "dark:bg-black"]);
  let md = of(&all, "md:text-6xl").unwrap();
  assert_eq!(md.min_width, Some(768.0));
  assert_eq!(md.declarations[0], ("font-size".to_owned(), "3.75rem".to_owned()));
  assert!(of(&all, "hover:bg-red-500").is_none());
  assert!(of(&all, "dark:bg-black").is_none());
  let order: Vec<&str> = all.iter().map(|r| r.candidate.as_str()).collect();
  assert_eq!(order, ["text-4xl", "md:text-6xl"], "in output order, the breakpoint after the base");
}
