//! npm version ranges under generated input. A range is whatever a package's
//! `package.json` on the registry says, so nothing here may panic, and each
//! operator must select the versions node-semver's definition of it selects.

use bolero::check;
use proptest::prelude::*;
use snapfire_fsr_cli::types::range::{Ident, Range, Version};

fn stable() -> impl Strategy<Value = Version> {
  (0u64..4, 0u64..4, 0u64..4).prop_map(|(a, b, c)| Version::new(a, b, c))
}

fn any_version() -> impl Strategy<Value = Version> {
  let pre = prop_oneof![
    3 => Just(Vec::new()),
    1 => prop_oneof![Just("alpha"), Just("beta"), Just("rc")].prop_map(|s| vec![Ident::Alpha(s.to_owned())]),
    1 => (0u64..3).prop_map(|n| vec![Ident::Num(n)]),
    1 => (prop_oneof![Just("alpha"), Just("rc")], 0u64..3).prop_map(|(s, n)| vec![Ident::Alpha(s.to_owned()), Ident::Num(n)]),
  ];
  ((0u64..4, 0u64..4, 0u64..4), pre).prop_map(|((a, b, c), pre)| Version { major: a, minor: b, patch: c, pre })
}

fn holds(range: &str, v: &Version) -> bool {
  Range::parse(range).unwrap_or_else(|| panic!("{range:?} parses")).satisfies(v)
}

proptest! {
  #[test]
  fn a_version_prints_as_it_parses(v in any_version()) {
    prop_assert_eq!(Version::parse(&v.to_string()), Some(v));
  }

  #[test]
  fn an_exact_version_selects_itself_alone(r in any_version(), v in any_version()) {
    prop_assert_eq!(holds(&r.to_string(), &v), r == v);
  }

  #[test]
  fn a_caret_allows_changes_that_leave_the_leftmost_nonzero_number(r in stable(), v in stable()) {
    let same = match (r.major, r.minor) {
      (0, 0) => (v.major, v.minor, v.patch) == (0, 0, r.patch),
      (0, minor) => v.major == 0 && v.minor == minor,
      (major, _) => v.major == major,
    };
    prop_assert_eq!(holds(&format!("^{r}"), &v), v >= r && same);
  }

  #[test]
  fn a_tilde_allows_patch_changes(r in stable(), v in stable()) {
    prop_assert_eq!(holds(&format!("~{r}"), &v), v >= r && v.major == r.major && v.minor == r.minor);
  }

  #[test]
  fn an_x_selects_every_value_in_its_place(a in 0u64..4, b in 0u64..4, v in stable(), x in prop_oneof![Just("x"), Just("X"), Just("*")]) {
    prop_assert_eq!(holds(&format!("{a}.{b}.{x}"), &v), v.major == a && v.minor == b);
    prop_assert_eq!(holds(&format!("{a}.{b}"), &v), v.major == a && v.minor == b);
    prop_assert_eq!(holds(&format!("{a}.{x}"), &v), v.major == a);
    prop_assert_eq!(holds(&format!("{a}"), &v), v.major == a);
    prop_assert!(holds(x, &v));
  }

  #[test]
  fn a_primitive_compares_against_a_full_version(r in stable(), v in stable()) {
    prop_assert_eq!(holds(&format!(">={r}"), &v), v >= r);
    prop_assert_eq!(holds(&format!(">{r}"), &v), v > r);
    prop_assert_eq!(holds(&format!("<={r}"), &v), v <= r);
    prop_assert_eq!(holds(&format!("<{r}"), &v), v < r);
    prop_assert_eq!(holds(&format!(">= {r}"), &v), v >= r);
  }

  #[test]
  fn a_hyphen_is_inclusive_at_both_ends(lo in stable(), hi in stable(), v in stable()) {
    prop_assert_eq!(holds(&format!("{lo} - {hi}"), &v), v >= lo && v <= hi);
  }

  #[test]
  fn or_is_a_union_and_a_space_an_intersection(a in stable(), b in stable(), v in stable(), ops in (0usize..4, 0usize..4)) {
    let op = ["^", "~", ">=", "<"];
    let (x, y) = (format!("{}{a}", op[ops.0]), format!("{}{b}", op[ops.1]));
    prop_assert_eq!(holds(&format!("{x} || {y}"), &v), holds(&x, &v) || holds(&y, &v));
    prop_assert_eq!(holds(&format!("{x} {y}"), &v), holds(&x, &v) && holds(&y, &v));
  }

  #[test]
  fn a_prerelease_satisfies_only_a_range_naming_its_tuple(r in stable(), v in any_version().prop_filter("a prerelease", |v| !v.pre.is_empty())) {
    let named = (r.major, r.minor, r.patch) == (v.major, v.minor, v.patch);
    for range in [format!("^{r}"), format!("~{r}"), format!(">={r}"), format!("{}.x", r.major), "*".to_owned()] {
      prop_assert!(!holds(&range, &v), "{} satisfies {:?}", v, range);
    }
    prop_assert_eq!(holds(&format!(">={r}-0"), &v), named);
  }

  #[test]
  fn parsing_never_panics(text in "(?s).{0,24}") {
    let _ = Range::parse(&text);
    let _ = Version::parse(&text);
  }

  #[test]
  fn a_range_made_of_range_characters_never_panics(text in "[0-9xX*^~<>=|. vV+-]{0,24}", v in any_version()) {
    if let Some(range) = Range::parse(&text) {
      let _ = range.satisfies(&v);
    }
  }
}

#[test]
fn fuzz_range_and_version_parsing() {
  check!().with_type::<(String, String)>().for_each(|(range, version)| {
    let parsed = Range::parse(range);
    if let (Some(range), Some(version)) = (parsed, Version::parse(version)) {
      let _ = range.satisfies(&version);
    }
  });
}
