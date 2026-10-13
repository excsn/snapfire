//! Cases from Tailwind v4.3.2's test files that the fixture extractor cannot read: loaders
//! that depend on the id, assertions on `sources`, `features` or warnings, loops that
//! build their own class lists. Each test names the Tailwind test it ports.

use snapfire_tailwind::{
  CompileOptions, Compiler, Error, Features, LoadStylesheet, Loaded, OptimizeOptions, Source, optimize,
  tailwind_stylesheet,
};

fn pretty(input: &str) -> String {
  let trimmed = input.trim();
  if trimmed.is_empty() {
    String::new()
  } else {
    format!("\n{trimmed}\n")
  }
}

fn options<'a>(base: &str, load: Option<Box<LoadStylesheet<'a>>>) -> CompileOptions<'a> {
  CompileOptions {
    base: base.to_owned(),
    load_stylesheet: load,
    banner: false,
    ..CompileOptions::default()
  }
}

fn run_with(candidates: &[&str], css: &str, options: CompileOptions) -> Result<String, Error> {
  let mut compiler = Compiler::new(css, options)?;
  let built = compiler.build(candidates.iter().copied());
  Ok(pretty(&optimize(&built, &OptimizeOptions::default())?))
}

fn run(candidates: &[&str], css: &str) -> String {
  run_with(candidates, css, options("", None)).unwrap()
}

fn loaded(content: &str, base: &str) -> Result<Loaded, Error> {
  Ok(Loaded {
    path: String::new(),
    base: base.to_owned(),
    content: content.to_owned(),
  })
}

/// `index.test.ts`: `@apply > should be usable with CSS mixins`, the unoptimized half.
#[test]
fn apply_is_usable_with_css_mixins_before_optimizing() {
  let input = ".foo {\n  /* Utility usage */\n  @apply underline;\n\n  /* CSS mixin usage */\n  @apply --my-mixin-1;\n  @apply --my-mixin-1();\n  @apply --my-mixin-1 --my-mixin-2;\n  @apply --my-mixin-1() --my-mixin-2();\n  @apply --my-mixin-3 {\n    color: red;\n  }\n}";
  let mut compiler = Compiler::new(input, options("", None)).unwrap();
  assert_eq!(
    compiler.build([]),
    ".foo {\n  text-decoration-line: underline;\n  @apply --my-mixin-1;\n  @apply --my-mixin-1();\n  @apply --my-mixin-1 --my-mixin-2;\n  @apply --my-mixin-1() --my-mixin-2();\n  @apply --my-mixin-3 {\n    color: red;\n  }\n}\n"
  );
}

/// `index.test.ts`: `@source > emits @source files`, `emits multiple @source files` and
/// `emits negated @source files`.
#[test]
fn source_emits_files() {
  let source = |pattern: &str, negated: bool| Source {
    pattern: pattern.to_owned(),
    base: "/root".to_owned(),
    negated,
  };

  let compiler = Compiler::new("@source \"./foo/bar/*.ts\";", options("/root", None)).unwrap();
  assert_eq!(compiler.sources(), &[source("./foo/bar/*.ts", false)]);

  let compiler = Compiler::new(
    "@source \"./foo/**/*.ts\";\n@source \"./php/secr3t/smarty.php\";",
    options("/root", None),
  )
  .unwrap();
  assert_eq!(
    compiler.sources(),
    &[source("./foo/**/*.ts", false), source("./php/secr3t/smarty.php", false)]
  );

  let compiler = Compiler::new(
    "@source not \"./foo/**/*.ts\";\n@source not \"./php/secr3t/smarty.php\";",
    options("/root", None),
  )
  .unwrap();
  assert_eq!(
    compiler.sources(),
    &[source("./foo/**/*.ts", true), source("./php/secr3t/smarty.php", true)]
  );
}

/// `index.test.ts`: `should error when \`layer(…)\` is used, but it's not the first param`.
#[test]
fn layer_must_come_first_in_an_import() {
  let err = run_with(
    &[],
    "@import './bar.css' supports(display: grid) layer(utilities);",
    options(
      "",
      Some(Box::new(|_, _| loaded(".foo {\n  @apply underline;\n}", "/bar.css"))),
    ),
  )
  .unwrap_err();
  assert_eq!(
    err.to_string(),
    "`layer(…)` in an `@import` should come before any other functions or conditions"
  );
}

fn reference_tree(id: &str, base: &str) -> Result<Loaded, Error> {
  if id == "./foo/baz.css" {
    return loaded(
      ".foo {\n  color: red;\n}\n@utility foo {\n  color: red;\n}\n@theme {\n  --breakpoint-md: 768px;\n}\n@custom-variant hocus (&:hover, &:focus);",
      base,
    );
  }
  loaded("@import './foo/baz.css';", "/root/foo")
}

/// `index.test.ts`: `` `@reference "…" imports` > recursively removes styles ``.
#[test]
fn reference_recursively_removes_styles() {
  let out = run_with(
    &[],
    "@reference './foo/bar.css';\n\n.bar {\n  @apply md:hocus:foo;\n}",
    options("", Some(Box::new(reference_tree))),
  )
  .unwrap();
  assert_eq!(
    out,
    "\n@media (min-width: 768px) {\n  .bar:hover, .bar:focus {\n    color: red;\n  }\n}\n"
  );
}

/// `index.test.ts`: `` `@reference "…" imports` > does not generate utilities ``.
#[test]
fn reference_does_not_generate_utilities() {
  let out = run_with(
    &["text-underline", "border"],
    "@reference './foo/bar.css';",
    options(
      "",
      Some(Box::new(|id, base| {
        if id == "./foo/baz.css" {
          return loaded("@layer utilities {\n  @tailwind utilities;\n}", base);
        }
        loaded("@import './foo/baz.css';", "/root/foo")
      })),
    ),
  )
  .unwrap();
  assert_eq!(out, "");
}

/// `index.test.ts`: `` `@reference "…" imports` > emits CSS variable fallback and keyframes
/// defined inside @reference-ed files ``.
#[test]
fn reference_emits_variable_fallbacks_and_keyframes() {
  let load = |id: &str, base: &str| match id {
    "./one.css" => loaded("@import './two.css' layer(two);", base),
    "./two.css" => loaded("@import './three.css' layer(three);", base),
    "./three.css" => loaded(
      ".foo {\n  color: red;\n}\n@theme {\n  --color-red: red;\n  --animate-wiggle: wiggle 1s ease-in-out infinite;\n  @keyframes wiggle {\n    0%,\n    100% {\n      transform: rotate(-3deg);\n    }\n    50% {\n      transform: rotate(3deg);\n    }\n  }\n}",
      base,
    ),
    _ => Err(Error::Message("unreachable".to_owned())),
  };
  let out = run_with(
    &[],
    "@reference './one.css';\n.bar {\n  @apply text-red animate-wiggle;\n}",
    options("", Some(Box::new(load))),
  )
  .unwrap();
  assert_eq!(
    out,
    "\n.bar {\n  animation: var(--animate-wiggle, wiggle 1s ease-in-out infinite);\n  color: var(--color-red, red);\n}\n\n@keyframes wiggle {\n  0%, 100% {\n    transform: rotate(-3deg);\n  }\n\n  50% {\n    transform: rotate(3deg);\n  }\n}\n"
  );
}

/// `index.test.ts`: `` `@reference "…" imports` > supports `@import "…" reference` syntax ``.
#[test]
fn reference_through_import_syntax() {
  let load = |_: &str, _: &str| {
    loaded(
      ".foo {\n  color: red;\n}\n@utility foo {\n  color: red;\n}\n@theme {\n  --breakpoint-md: 768px;\n}\n@custom-variant hocus (&:hover, &:focus);",
      "/root/foo",
    )
  };
  let out = run_with(
    &[],
    "@import './foo/bar.css' reference;\n\n.bar {\n  @apply md:hocus:foo;\n}",
    options("", Some(Box::new(load))),
  )
  .unwrap();
  assert_eq!(
    out,
    "\n@media (min-width: 768px) {\n  .bar:hover, .bar:focus {\n    color: red;\n  }\n}\n"
  );
}

/// `index.test.ts`: `feature detection`, the cases that need no JavaScript.
#[test]
fn feature_detection() {
  let features =
    |css: &str, load: Option<Box<LoadStylesheet<'static>>>| Compiler::new(css, options("", load)).unwrap().features();
  let empty = || -> Option<Box<LoadStylesheet<'static>>> { Some(Box::new(|_, base| loaded("", base))) };

  assert!(features("@tailwind utilities;", None).has(Features::UTILITIES));
  assert!(features(".foo {\n  @apply underline;\n}", None).has(Features::AT_APPLY));
  assert!(features("@import 'tailwindcss/preflight';", empty()).has(Features::AT_IMPORT));
  assert!(features("@reference 'tailwindcss/preflight';", empty()).has(Features::AT_IMPORT));
  assert!(
    features(
      "@theme {\n  --color-red: #f00;\n}\n\n.foo {\n  color: theme(--color-red);\n}",
      empty()
    )
    .has(Features::THEME_FUNCTION)
  );
  assert!(features(".foo {\n  @variant dark {\n    color: red;\n  }\n}", None).has(Features::VARIANTS));
  assert!(!features("@variant dark (&:is(.dark, .dark *));", None).has(Features::VARIANTS));

  let referenced = features(
    "@reference \"tailwindcss/utilities\";",
    Some(Box::new(|_, base| loaded("@tailwind utilities;", base))),
  );
  assert!(referenced.has(Features::AT_IMPORT));
  assert!(!referenced.has(Features::UTILITIES));

  let mut compiler = Compiler::new("@theme {\n  --tracking-narrower: -0.02em;\n}", options("", None)).unwrap();
  assert!(compiler.features().has(Features::AT_THEME));
  assert_eq!(compiler.build([]), "");
}

/// `index.test.ts`: `@custom-variant > @custom-variant must have a valid name`.
#[test]
fn custom_variant_names_are_validated() {
  for input in [
    "@custom-variant - (&);",
    "@custom-variant --- (&);",
    "@custom-variant _ (&);",
    "@custom-variant ___ (&);",
    "@custom-variant -foo (&);",
    "@custom-variant --foo (&);",
    "@custom-variant _foo (&);",
    "@custom-variant __foo (&);",
    "@custom-variant foo- (&);",
    "@custom-variant foo-- (&);",
    "@custom-variant foo_ (&);",
    "@custom-variant foo__ (&);",
  ] {
    assert!(Compiler::new(input, options("", None)).is_err(), "{input}");
  }
}

/// `utilities.test.ts`: `custom utilities must use a valid name definitions`.
#[test]
fn custom_utility_names_must_be_valid() {
  for input in [
    "@utility push-| {\n  right: 100%;\n}",
    "@utility ~push {\n  right: 100%;\n}",
    "@utility @push {\n  right: 100%;\n}",
  ] {
    let err = Compiler::new(input, options("", None)).err().expect("an invalid name");
    assert!(err.to_string().contains("should be alphanumeric"), "{input}: {err}");
  }
}

/// `utilities.test.ts`: `bare values with unsupported data types should result in a warning`.
#[test]
fn unsupported_bare_value_types_warn() {
  let input = "@utility paint-* {\n  paint: --value([color], color);\n}\n\n@tailwind utilities;";
  let mut compiler = Compiler::new(input, options("", None)).unwrap();
  assert_eq!(
    pretty(
      &optimize(
        &compiler.build(["paint-#0088cc", "paint-red"]),
        &OptimizeOptions::default()
      )
      .unwrap()
    ),
    ""
  );
  assert_eq!(
    compiler.warnings(),
    vec![
      "Unsupported bare value data type: \"color\".\nOnly valid data types are: \"number\", \"integer\", \"ratio\", \"percentage\".\n".to_owned(),
      "```css\n--value([color],color)\n                ^^^^^\n```".to_owned(),
    ]
  );
}

/// `utilities.test.ts`: `resolving unsupported bare values`.
#[test]
fn unsupported_bare_values_resolve_to_nothing() {
  let input = "@utility example-* {\n  --resolved-value: --value(color);\n}\n\n@tailwind utilities;";
  let mut compiler = Compiler::new(input, options("", None)).unwrap();
  assert_eq!(
    pretty(
      &optimize(
        &compiler.build(["example-#0088cc", "example-foo"]),
        &OptimizeOptions::default()
      )
      .unwrap()
    ),
    ""
  );
  let joined = format!("\n{}\n", compiler.warnings().join("\n").trim());
  assert_eq!(
    joined,
    "\nUnsupported bare value data type: \"color\".\nOnly valid data types are: \"number\", \"integer\", \"ratio\", \"percentage\".\n\n```css\n--value(color)\n        ^^^^^\n```\n"
  );
}

/// `variants.test.ts`: `variants with the same root are sorted deterministically`, over every
/// permutation of its six candidates.
#[test]
fn variants_with_the_same_root_sort_deterministically() {
  fn permute(items: &[&'static str]) -> Vec<Vec<&'static str>> {
    if items.len() <= 1 {
      return vec![items.to_vec()];
    }
    let mut out = Vec::new();
    for i in 0..items.len() {
      let mut rest = items.to_vec();
      let item = rest.remove(i);
      for mut permutation in permute(&rest) {
        permutation.insert(0, item);
        out.push(permutation);
      }
    }
    out
  }

  let expected = "\n.data-active\\:flex[data-active], .data-focus\\:flex[data-focus], .data-hover\\:flex[data-hover], .data-\\[bar\\]\\:flex[data-bar], .data-\\[baz\\]\\:flex[data-baz], .data-\\[foo\\]\\:flex[data-foo] {\n  display: flex;\n}\n";
  for list in permute(&[
    "data-hover:flex",
    "data-focus:flex",
    "data-active:flex",
    "data-[foo]:flex",
    "data-[bar]:flex",
    "data-[baz]:flex",
  ]) {
    assert_eq!(run(&list, "@tailwind utilities;"), expected, "{list:?}");
  }
}

/// `css-functions.test.ts`: `theme(…) > in declaration values > with default theme`.
#[test]
fn theme_reads_the_default_theme() {
  let theme = tailwind_stylesheet("tailwindcss/theme.css", "").unwrap().content;
  for (value, result) in [
    (
      "fontFamily.sans",
      "ui-sans-serif, system-ui, sans-serif, \"Apple Color Emoji\", \"Segoe UI Emoji\", \"Segoe UI Symbol\", \"Noto Color Emoji\"",
    ),
    ("maxWidth.xs", "20rem"),
    ("transitionTimingFunction.in-out", "cubic-bezier(.4, 0, .2, 1)"),
    ("letterSpacing.wide", ".025em"),
    ("lineHeight.tight", "1.25"),
    ("backgroundColor.red.500", "oklch(63.7% .237 25.331)"),
  ] {
    let compiled = run_with(
      &[],
      &format!("{theme}\n.custom {{\n  --custom-value: theme({value});\n}}"),
      options("", None),
    )
    .unwrap();
    let start = compiled.find(".custom {\n").expect("the custom rule") + ".custom {\n".len();
    let end = start + compiled[start..].find('}').unwrap();
    let rule = compiled[start..end].replacen("--custom-value:", "", 1);
    let rule = rule.trim();
    assert_eq!(&rule[..rule.len() - 1], result, "theme({value})");
  }
}

/// `candidate.test.ts`: `normalize candidates`, every variant against every candidate.
#[test]
fn candidates_print_in_canonical_form() {
  let candidates = [
    ("[color:red]", "[color:red]"),
    ("[color:red]/50", "[color:red]/50"),
    ("[color:red]/[0.5]", "[color:red]/[0.5]"),
    ("[color:red]/50!", "[color:red]/50!"),
    ("![color:red]/50", "[color:red]/50!"),
    ("[color:red]/[0.5]!", "[color:red]/[0.5]!"),
    ("box-border", "box-border"),
    ("underline!", "underline!"),
    ("!underline", "underline!"),
    ("-inset-full", "-inset-full"),
    ("bg-red-500", "bg-red-500"),
    ("bg-red-500/50", "bg-red-500/50"),
    ("bg-red-500/[0.5]", "bg-red-500/[0.5]"),
    ("bg-red-500!", "bg-red-500!"),
    ("!bg-red-500", "bg-red-500!"),
    ("bg-[#0088cc]/50", "bg-[#0088cc]/50"),
    ("bg-[#0088cc]/[0.5]", "bg-[#0088cc]/[0.5]"),
    ("bg-[#0088cc]!", "bg-[#0088cc]!"),
    ("!bg-[#0088cc]", "bg-[#0088cc]!"),
    ("bg-[var(--spacing)-1px]", "bg-[var(--spacing)-1px]"),
    ("bg-[var(--spacing)_-_1px]", "bg-[var(--spacing)-1px]"),
    ("bg-[var(--_spacing)]", "bg-(--_spacing)"),
    ("bg-(--_spacing)", "bg-(--_spacing)"),
    ("bg-[-1px_-1px]", "bg-[-1px_-1px]"),
    ("p-[round(to-zero,1px)]", "p-[round(to-zero,1px)]"),
    ("w-1/2", "w-1/2"),
    (
      "p-[calc((100vw-theme(maxWidth.2xl))_/_2)]",
      "p-[calc((100vw-theme(maxWidth.2xl))/2)]",
    ),
    ("content-[\"hello_world\"]", "content-[\"hello_world\"]"),
    ("content-[____\"hello_world\"___]", "content-[\"hello_world\"]"),
    ("bg-[no-repeat_url(/image_13.png)]", "bg-[no-repeat_url(/image_13.png)]"),
    (
      "bg-[var(--spacing-0_5,_var(--spacing-1_5,_3rem))]",
      "bg-(--spacing-0_5,var(--spacing-1_5,3rem))",
    ),
    ("[display:flex]", "[display:flex]"),
    ("[display:_flex]", "[display:flex]"),
    ("[display:flex_]", "[display:flex]"),
    ("[display:_flex_]", "[display:flex]"),
    ("w-[calc(100%-2rem)]", "w-[calc(100%-2rem)]"),
    ("w-[calc(100%_-_2rem)]", "w-[calc(100%-2rem)]"),
    ("!flex", "flex!"),
    ("flex!", "flex!"),
  ];
  let variants = [
    ("", ""),
    ("*:", "*:"),
    ("focus:", "focus:"),
    ("group-focus:", "group-focus:"),
    ("hover:focus:", "hover:focus:"),
    ("hover:group-focus:", "hover:group-focus:"),
    ("group-hover:focus:", "group-hover:focus:"),
    ("group-hover:group-focus:", "group-hover:group-focus:"),
    ("min-[10px]:", "min-[10px]:"),
    ("min-[calc(1000px_+_12em)]:", "min-[calc(1000px+12em)]:"),
    ("min-[calc(1000px_+12em)]:", "min-[calc(1000px+12em)]:"),
    ("min-[calc(1000px+_12em)]:", "min-[calc(1000px+12em)]:"),
    ("min-[calc(1000px___+___12em)]:", "min-[calc(1000px+12em)]:"),
    ("peer-[&_p]:", "peer-[&_p]:"),
    ("peer-[&_p]:hover:", "peer-[&_p]:hover:"),
    ("hover:peer-[&_p]:", "hover:peer-[&_p]:"),
    ("hover:peer-[&_p]:focus:", "hover:peer-[&_p]:focus:"),
    ("peer-[&:hover]:peer-[&_p]:", "peer-[&:hover]:peer-[&_p]:"),
    ("[p]:", "[p]:"),
    ("[_p_]:", "[p]:"),
    ("has-[p]:", "has-[p]:"),
    ("has-[_p_]:", "has-[p]:"),
    ("[&:is(p)]:", "[p]:"),
    ("[&:is(_p_)]:", "[p]:"),
    ("has-[&:is(p)]:", "has-[p]:"),
    ("has-[&:is(_p_)]:", "has-[p]:"),
    ("@xl:", "@xl:"),
    ("@[123px]:", "@[123px]:"),
    ("not-group-hover/name:", "not-group-hover/name:"),
    ("has-group-peer-hover/name:", "has-group-peer-hover/name:"),
    ("in-group-peer-hover/name:", "in-group-peer-hover/name:"),
  ];

  let compiler = Compiler::new("@tailwind utilities", options("", None)).unwrap();
  let mut failures = Vec::new();
  for (input_variant, output_variant) in variants {
    for (input_candidate, output_candidate) in candidates {
      let input = format!("{input_variant}{input_candidate}");
      let expected = format!("{output_variant}{output_candidate}");
      let printed = compiler.canonical_candidate(&input);
      if printed.as_deref() != Some(expected.as_str()) {
        failures.push(format!("{input} -> {printed:?}, expected {expected}"));
      }
    }
  }
  assert!(
    failures.is_empty(),
    "{} of {} differ:\n{}",
    failures.len(),
    candidates.len() * variants.len(),
    failures.join("\n")
  );
}
