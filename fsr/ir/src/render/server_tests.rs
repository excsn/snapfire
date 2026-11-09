use super::*;
use crate::ast::{Entry, Expr, Handler, Lit, Stmt, Tmpl};

fn help() -> Component {
  let button = Tmpl::Element {
    tag: "button".to_owned(),
    attrs: vec![Entry::Field(format!("{HANDLER_ATTR}click"), Expr::Lit(Lit::Int(0))), Entry::Field(KEY_ATTR.to_owned(), Expr::lit_str("toggle"))],
    children: vec![Tmpl::Expr(Expr::Ternary(Box::new(Expr::var("open")), Box::new(Expr::lit_str("Hide")), Box::new(Expr::lit_str("Show"))))],
  };
  let list = Tmpl::If { cond: Expr::var("open"), then: Box::new(Tmpl::Element { tag: "ul".to_owned(), attrs: Vec::new(), children: vec![Tmpl::Text("mail".to_owned())] }), r#else: None };
  Component {
    body: vec![Stmt::Let { name: "open".to_owned(), expr: Expr::Lit(Lit::Bool(false)) }, Stmt::Let { name: "label".to_owned(), expr: Expr::Template(vec![Expr::lit_str("order "), Expr::var("$props").field("id")]) }],
    render: Tmpl::Element { tag: "section".to_owned(), attrs: Vec::new(), children: vec![Tmpl::Expr(Expr::var("label")), button, list] },
    state: vec!["open".to_owned()],
    stores: Vec::new(),
    handlers: vec![Handler { event: "click".to_owned(), body: vec![Stmt::Return(Expr::Object(vec![Entry::Field("open".to_owned(), Expr::Not(Box::new(Expr::var("open"))))]))] }],
    owner: crate::ast::Owner::React,
    shadow: None, scope: None,
  }
}

#[test]
fn handler_markers_and_keys_print_only_in_server_mode() {
  let mut library = Components::new();
  library.insert("src/ui/Help.tsx#Help".to_owned(), Arc::new(help()));
  let island = |mode: Option<&str>| Component { body: Vec::new(), render: Tmpl::Island { module: "src/ui/Help.tsx#Help".to_owned(), props: vec![Entry::Field("id".to_owned(), Expr::Lit(Lit::Int(7)))], children: Vec::new(), when: None, mode: mode.map(str::to_owned), id: 9, define: false }, state: Vec::new(), stores: Vec::new(), handlers: Vec::new(), owner: crate::ast::Owner::React, shadow: None, scope: None };
  let browser = Interpreter::default().render_module("page", &island(None), &ValueMap::default(), &library).unwrap();
  assert_eq!(browser.islands[0].body.html, "<section>order 7<button>Show</button></section>");
  assert!(browser.islands[0].mode.is_none() && !browser.islands[0].mount_props().contains_key(STATE_PROP));
  let server = Interpreter::default().render_module("page", &island(Some("server")), &ValueMap::default(), &library).unwrap();
  assert_eq!(server.islands[0].body.html, "<section>order 7<button data-sf-key=\"toggle\" data-sf-on=\"click:0\">Show</button></section>");
  assert_eq!(server.islands[0].mode.as_deref(), Some("server"));
  let props = server.islands[0].mount_props();
  assert_eq!(props.get(STATE_PROP), Some(&Value::Map(ValueMap::from_iter([("open".to_owned(), Value::Bool(false))]))), "{props:?}");
  let nodes = crate::bind::rendered_nodes(&server);
  assert_eq!(nodes[0], snapfire_fsr_core::Node::raw("<sf-s data-sf-island data-sf-region=\"page|i9\" data-sf-mode=\"server\">"));
}

#[test]
fn a_step_runs_the_handler_over_the_state_and_renders_from_the_result() {
  let library = Components::new();
  let component = help();
  let mut props = ValueMap::default();
  props.insert("id".to_owned(), Value::Int(7));
  let state = ValueMap::from_iter([("open".to_owned(), Value::Bool(false))]);
  let stepped = Interpreter::default().island_step("src/ui/Help.tsx#Help", &component, &props, &state, Some(HandlerRef::own(0)), &Value::Null, &library).unwrap();
  assert_eq!(stepped.state, ValueMap::from_iter([("open".to_owned(), Value::Bool(true))]));
  assert_eq!(stepped.rendered.html, "<section>order 7<button data-sf-key=\"toggle\" data-sf-on=\"click:0\">Hide</button><ul>mail</ul></section>");
  let again = Interpreter::default().island_step("src/ui/Help.tsx#Help", &component, &props, &stepped.state, Some(HandlerRef::own(0)), &Value::Null, &library).unwrap();
  assert_eq!(again.state["open"], Value::Bool(false));
  let as_is = Interpreter::default().island_step("src/ui/Help.tsx#Help", &component, &props, &stepped.state, None, &Value::Null, &library).unwrap();
  assert_eq!(as_is.state, stepped.state, "no handler renders from the state given");
  assert!(as_is.rendered.html.contains("<ul>mail</ul>"));
  let missing = Interpreter::default().island_step("src/ui/Help.tsx#Help", &component, &props, &state, Some(HandlerRef::own(3)), &Value::Null, &library).unwrap_err();
  assert!(missing.message.contains("no handler 3"), "{}", missing.message);
  assert!(stepped.acts.is_empty() && as_is.acts.is_empty());
}

#[test]
fn a_step_collects_the_actions_a_handler_calls_with_their_inputs_evaluated() {
  let library = Components::new();
  let mut component = help();
  component.handlers.push(Handler {
    event: "click".to_owned(),
    body: vec![
      Stmt::Act { action: "desk.save".to_owned(), input: Expr::Object(vec![Entry::Field("id".to_owned(), Expr::var("$props").field("id")), Entry::Field("open".to_owned(), Expr::var("open"))]) },
      Stmt::Return(Expr::Object(vec![Entry::Field("open".to_owned(), Expr::Lit(Lit::Bool(true)))])),
    ],
  });
  let mut props = ValueMap::default();
  props.insert("id".to_owned(), Value::Int(7));
  let state = ValueMap::from_iter([("open".to_owned(), Value::Bool(false))]);
  let stepped = Interpreter::default().island_step("src/ui/Help.tsx#Help", &component, &props, &state, Some(HandlerRef::own(1)), &Value::Null, &library).unwrap();
  let input = ValueMap::from_iter([("id".to_owned(), Value::Int(7)), ("open".to_owned(), Value::Bool(false))]);
  assert_eq!(stepped.acts, vec![("desk.save".to_owned(), Value::Map(input))], "the input reads the props and the state as they were when the handler ran");
  assert_eq!(stepped.state["open"], Value::Bool(true), "and the patch still applies");
  assert!(stepped.rendered.html.contains("<ul>mail</ul>"));
}
