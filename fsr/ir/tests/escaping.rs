//! What reaches the markup. Attribute values and text are escaped; an
//! attribute *name* is written as it stands, and a `{...spread}` takes its
//! names from a runtime value, so a name from data is the question here.

use proptest::prelude::*;
use snapfire_fsr_ir::ast::{Component, Entry, Expr, Tmpl};
use snapfire_fsr_ir::render::Components;
use snapfire_fsr_ir::Interpreter;
use snapfire_fsr_core::{Value, ValueMap};

fn render(attrs: Vec<Entry>, props: ValueMap) -> Result<String, String> {
  let component = Component::new(
    Vec::new(),
    Tmpl::Element { tag: "div".to_owned(), attrs, children: vec![Tmpl::Text("in".to_owned())] },
  );
  Interpreter::default()
    .render(&component, &props, &Components::new())
    .map(|r| r.html)
    .map_err(|e| e.to_string())
}

fn map_of(pairs: &[(&str, Value)]) -> ValueMap {
  let mut map = ValueMap::default();
  for (k, v) in pairs {
    map.insert((*k).to_owned(), v.clone());
  }
  map
}

/// Text a value carries cannot open or close a tag.
#[test]
fn a_value_in_text_cannot_reach_the_markup() {
  let component = Component::new(
    Vec::new(),
    Tmpl::Element {
      tag: "div".to_owned(),
      attrs: Vec::new(),
      children: vec![Tmpl::Expr(Expr::Field(Box::new(Expr::var("$props")), "t".to_owned()))],
    },
  );
  for hostile in ["<script>alert(1)</script>", "</div><script>x</script>", "&lt;", "a & b"] {
    let html = Interpreter::default()
      .render(&component, &map_of(&[("t", Value::str(hostile))]), &Components::new())
      .expect("renders")
      .html;
    let inner = html.trim_start_matches("<div>").trim_end_matches("</div>");
    assert!(!inner.contains('<'), "{hostile:?} produced {html}");
    assert!(!inner.contains('>'), "{hostile:?} produced {html}");
  }
}

/// A value in an attribute cannot close the quote and add another attribute.
#[test]
fn a_value_in_an_attribute_cannot_close_its_quote() {
  for hostile in ["\" onerror=\"alert(1)", "\"><script>x</script>", "' onload='x"] {
    let html = render(
      vec![Entry::Field("title".to_owned(), Expr::Field(Box::new(Expr::var("$props")), "v".to_owned()))],
      map_of(&[("v", Value::str(hostile))]),
    )
    .expect("renders");
    // the text may appear inside the value; what matters is that it is inside it
    assert_eq!(html.matches('"').count(), 2, "{hostile:?} produced {html}");
    assert!(!html.contains("\" onerror"), "{hostile:?} produced {html}");
  }
}

/// A spread takes its names from data. A name holding a space, a quote or a
/// bracket must not become another attribute or close the tag.
#[test]
fn a_spread_name_from_data_cannot_forge_an_attribute() {
  for hostile in [
    "x onerror=alert(1)",
    "x\" onerror=\"alert(1)",
    "x><script>alert(1)</script",
    "x=\"y\"",
    " ",
    "<",
  ] {
    let out = render(
      vec![Entry::Spread(Expr::Field(Box::new(Expr::var("$props")), "p".to_owned()))],
      map_of(&[("p", Value::Map(map_of(&[(hostile, Value::str("v"))])))]),
    );
    match out {
      Err(_) => {}
      Ok(html) => {
        assert!(!html.contains(" onerror="), "name {hostile:?} forged a handler: {html}");
        assert!(!html.contains("<script"), "name {hostile:?} opened a tag: {html}");
        assert_eq!(html.matches('<').count(), 2, "name {hostile:?} produced {html}");
        assert_eq!(html.matches('"').count() % 2, 0, "name {hostile:?} produced {html}");
      }
    }
  }
}

proptest! {
  #![proptest_config(ProptestConfig { cases: 512, ..ProptestConfig::default() })]

  /// The same over generated names: whatever a spread carries, the element
  /// that comes out has one opening tag and one closing tag.
  #[test]
  fn a_spread_name_never_breaks_the_element(
    name in prop_oneof![
      "[a-zA-Z0-9-]{0,8}",
      "(?s).{0,10}",
      Just("x onerror=y".to_owned()),
      Just("\">".to_owned()),
      Just("/><b".to_owned()),
    ],
    value in "[a-zA-Z0-9 ]{0,8}",
  ) {
    if let Ok(html) = render(
      vec![Entry::Spread(Expr::Field(Box::new(Expr::var("$props")), "p".to_owned()))],
      map_of(&[("p", Value::Map(map_of(&[(name.as_str(), Value::str(value))])))]),
    ) {
      prop_assert_eq!(html.matches('<').count(), 2, "name {:?} produced {}", name, html);
      prop_assert_eq!(html.matches('>').count(), 2, "name {:?} produced {}", name, html);
    }
  }
}

/// A name that is already a legal attribute is written, because that is what a
/// spread is for. `onerror` spelled in lowercase is a legal attribute name and
/// React passes it through too, so whether FSR should drop lowercase `on*`
/// from a spread is a decision rather than a defect; this records what it does
/// today so a change to it is deliberate.
#[test]
fn a_legal_name_from_a_spread_is_written() {
  let html = render(
    vec![Entry::Spread(Expr::Field(Box::new(Expr::var("$props")), "p".to_owned()))],
    map_of(&[("p", Value::Map(map_of(&[("onerror", Value::str("x"))])))]),
  )
  .expect("renders");
  assert_eq!(html, "<div onerror=\"x\">in</div>");
}
