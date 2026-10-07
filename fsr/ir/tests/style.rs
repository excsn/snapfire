//! How a style object is written. A plan from an older build holds its style
//! keys already in CSS spelling, so the renderer reads either spelling.

use snapfire_fsr_core::{Value, ValueMap};
use snapfire_fsr_ir::ast::{Component, Entry, Expr, Tmpl};
use snapfire_fsr_ir::render::Components;
use snapfire_fsr_ir::Interpreter;

fn styled(pairs: &[(&str, Value)]) -> String {
  let component = Component::new(
    snapfire_fsr_ir::Owner::React,
    Vec::new(),
    Tmpl::Element { tag: "div".to_owned(), attrs: vec![Entry::Field("style".to_owned(), Expr::Field(Box::new(Expr::var("$props")), "s".to_owned()))], children: Vec::new() },
  );
  let style: ValueMap = pairs.iter().map(|(k, v)| ((*k).to_owned(), v.clone())).collect();
  let mut props = ValueMap::default();
  props.insert("s".to_owned(), Value::Map(style));
  Interpreter::default().render(&component, &props, &Components::new()).unwrap().html
}

#[test]
fn a_unitless_property_takes_no_px_in_either_spelling() {
  let camel = styled(&[("lineHeight", Value::F64(1.5)), ("zIndex", Value::Int(2)), ("WebkitLineClamp", Value::Int(3)), ("marginTop", Value::Int(4))]);
  let kebab = styled(&[("line-height", Value::F64(1.5)), ("z-index", Value::Int(2)), ("-webkit-line-clamp", Value::Int(3)), ("margin-top", Value::Int(4))]);
  let expected = r#"<div style="line-height:1.5;z-index:2;-webkit-line-clamp:3;margin-top:4px"></div>"#;
  assert_eq!(camel, expected);
  assert_eq!(kebab, expected, "a style key a plan holds in CSS spelling");
}
