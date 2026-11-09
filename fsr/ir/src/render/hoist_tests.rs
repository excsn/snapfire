use super::*;
use crate::ast::{Builtin, Entry, Expr, Lit, Stmt, Tmpl};

fn hoist(id: u32, expr: Expr) -> Expr {
  Expr::Hoist { id, expr: Box::new(expr) }
}

fn fixed(expr: Expr) -> Expr {
  Expr::Builtin { name: Builtin::ToFixed, args: vec![expr, Expr::Lit(Lit::Float(1.0))] }
}

#[test]
fn hoisted_values_are_keyed_by_module_id_and_loop_indices() {
  let component = Component {
    body: vec![Stmt::Let { name: "total".to_owned(), expr: hoist(0, fixed(Expr::var("$props").field("total"))) }],
    render: Tmpl::Fragment(vec![
      Tmpl::Expr(Expr::var("total")),
      Tmpl::For {
        over: Expr::var("$props").field("prices"),
        params: vec!["p".to_owned()],
        body: Box::new(Tmpl::For {
          over: Expr::var("$props").field("taxes"),
          params: vec!["t".to_owned()],
          body: Box::new(Tmpl::Expr(hoist(1, fixed(Expr::Arith(crate::ast::ArithOp::Mul, Box::new(Expr::var("p")), Box::new(Expr::var("t"))))))),
        }),
      },
    ]), state: Vec::new(), stores: Vec::new(), handlers: Vec::new(), owner: crate::ast::Owner::React, shadow: None, scope: None
  };
  let mut props = ValueMap::default();
  props.insert("total".to_owned(), Value::F64(2.5));
  props.insert("prices".to_owned(), Value::seq(vec![Value::F64(1.0), Value::F64(2.0)]));
  props.insert("taxes".to_owned(), Value::seq(vec![Value::F64(1.0), Value::F64(1.5)]));
  let rendered = Interpreter::default().render_module("src/ui/Bill.tsx#Bill", &component, &props, &Components::new()).unwrap();
  assert_eq!(rendered.html, "2.51.01.52.03.0");
  let keys: Vec<&_> = rendered.hoisted.keys().collect();
  assert_eq!(keys, ["src/ui/Bill.tsx#Bill|0", "src/ui/Bill.tsx#Bill|1@0.0", "src/ui/Bill.tsx#Bill|1@0.1", "src/ui/Bill.tsx#Bill|1@1.0", "src/ui/Bill.tsx#Bill|1@1.1"]);
  assert_eq!(rendered.hoisted["src/ui/Bill.tsx#Bill|1@1.1"], Value::str("3.0"));
  assert!(Interpreter::default().render(&component, &props, &Components::new()).unwrap().hoisted.contains_key("|0"), "the plain render keys under an empty module");
}

#[test]
fn a_nested_component_keys_by_its_own_module_below_its_callers_loops_and_a_collision_is_dropped() {
  let mut library = Components::new();
  library.insert(
    "src/ui/Price.tsx#Price".to_owned(),
    Arc::new(Component { body: Vec::new(), render: Tmpl::Expr(hoist(0, fixed(Expr::var("$props").field("cents")))), state: Vec::new(), stores: Vec::new(), handlers: Vec::new(), owner: crate::ast::Owner::React, shadow: None, scope: None }),
  );
  let price = |cents: Expr| Tmpl::Component { module: "src/ui/Price.tsx#Price".to_owned(), props: vec![Entry::Field("cents".to_owned(), cents)], children: Vec::new(), id: 0, keyed: false };
  let page = Component {
    body: Vec::new(),
    render: Tmpl::Fragment(vec![
      price(Expr::Lit(Lit::Float(1.0))),
      Tmpl::For { over: Expr::var("$props").field("items"), params: vec!["it".to_owned()], body: Box::new(price(Expr::var("it"))) },
      Tmpl::Expr(hoist(0, fixed(Expr::Lit(Lit::Float(9.0))))),
    ]), state: Vec::new(), stores: Vec::new(), handlers: Vec::new(), owner: crate::ast::Owner::React, shadow: None, scope: None
  };
  let mut props = ValueMap::default();
  props.insert("items".to_owned(), Value::seq(vec![Value::F64(2.0), Value::F64(3.0)]));
  let rendered = Interpreter::default().render_module("routes/index/page.tsx#default", &page, &props, &library).unwrap();
  assert_eq!(rendered.html, "1.02.03.09.0");
  let keys: Vec<&_> = rendered.hoisted.keys().collect();
  assert_eq!(keys, ["src/ui/Price.tsx#Price|0", "src/ui/Price.tsx#Price|0@0", "src/ui/Price.tsx#Price|0@1", "routes/index/page.tsx#default|0"]);
  assert_eq!(rendered.hoisted["src/ui/Price.tsx#Price|0@1"], Value::str("3.0"));

  let twice = Component { body: Vec::new(), render: Tmpl::Fragment(vec![price(Expr::Lit(Lit::Float(1.0))), price(Expr::Lit(Lit::Float(1.0))), price(Expr::Lit(Lit::Float(2.0)))]), state: Vec::new(), stores: Vec::new(), handlers: Vec::new(), owner: crate::ast::Owner::React, shadow: None, scope: None };
  let rendered = Interpreter::default().render_module("routes/index/page.tsx#default", &twice, &ValueMap::default(), &library).unwrap();
  assert!(rendered.hoisted.is_empty(), "Price placed three times outside a loop shares one key: 1.0 twice agrees, 2.0 drops it: {:?}", rendered.hoisted);
}

#[test]
fn a_chunk_records_its_inner_markup_and_the_markers_never_print() {
  let component = Component {
    body: Vec::new(),
    render: Tmpl::Element {
      tag: "ul".to_owned(),
      attrs: vec![Entry::Field("class".to_owned(), Expr::lit_str("list")), Entry::Field(CHUNK_ATTR.to_owned(), Expr::Lit(Lit::Int(4)))],
      children: vec![Tmpl::For {
        over: Expr::var("$props").field("items"),
        params: vec!["it".to_owned()],
        body: Box::new(Tmpl::Element {
          tag: "li".to_owned(),
          attrs: vec![Entry::Field("$bound".to_owned(), Expr::Lit(Lit::Bool(true)))],
          children: vec![Tmpl::Expr(hoist(1, fixed(Expr::var("it"))))],
        }),
      }],
    }, state: Vec::new(), stores: Vec::new(), handlers: Vec::new(), owner: crate::ast::Owner::React, shadow: None, scope: None
  };
  let mut props = ValueMap::default();
  props.insert("items".to_owned(), Value::seq(vec![Value::F64(1.0), Value::F64(2.0)]));
  let rendered = Interpreter::default().render_module("routes/index/page.tsx#default", &component, &props, &Components::new()).unwrap();
  assert_eq!(rendered.html, "<ul class=\"list\"><li>1.0</li><li>2.0</li></ul>");
  assert_eq!(rendered.hoisted["routes/index/page.tsx#default|4"], Value::str("<li>1.0</li><li>2.0</li>"));
  assert_eq!(rendered.hoisted["routes/index/page.tsx#default|1@1"], Value::str("2.0"), "a value inside a chunk is still recorded, for the fallback");
}

#[test]
fn a_chunk_under_a_branch_on_state_is_recorded_in_the_branch_not_taken() {
  let untaken = |id: i128, tag: &str, child: Tmpl| Tmpl::Element {
    tag: tag.to_owned(),
    attrs: vec![Entry::Field(UNTAKEN_ATTR.to_owned(), Expr::Lit(Lit::Bool(true))), Entry::Field(CHUNK_ATTR.to_owned(), Expr::Lit(Lit::Int(id)))],
    children: vec![child],
  };
  let noted = Tmpl::Element {
    tag: "p".to_owned(),
    attrs: Vec::new(),
    children: vec![Tmpl::Text("Noted: ".to_owned()), untaken(3, "strong", Tmpl::Expr(Expr::var("$props").field("title"))), Tmpl::Expr(Expr::var("pace"))],
  };
  let guarded = Tmpl::If {
    cond: Expr::Logic(crate::ast::LogicOp::And, Box::new(Expr::var("open")), Box::new(Expr::var("$props").field("image"))),
    then: Box::new(untaken(5, "em", Tmpl::Expr(Expr::var("$props").field("image").field("alt")))),
    r#else: None,
  };
  let component = Component {
    body: vec![Stmt::Let { name: "pace".to_owned(), expr: Expr::lit_str("") }, Stmt::Let { name: "open".to_owned(), expr: Expr::Lit(Lit::Bool(false)) }],
    render: Tmpl::Fragment(vec![
      Tmpl::If { cond: Expr::Compare(crate::ast::CompareOp::Eq, Box::new(Expr::var("pace")), Box::new(Expr::lit_str(""))), then: Box::new(Tmpl::Text("buttons".to_owned())), r#else: Some(Box::new(noted)) },
      Tmpl::If {
        cond: Expr::var("open"),
        then: Box::new(Tmpl::For { over: Expr::var("$props").field("items"), params: vec!["it".to_owned()], body: Box::new(untaken(4, "li", Tmpl::Expr(Expr::var("it")))) }),
        r#else: None,
      },
      guarded,
    ]),
    state: vec!["pace".to_owned(), "open".to_owned()],
    stores: Vec::new(),
    handlers: Vec::new(),
    owner: crate::ast::Owner::React,
    shadow: None,
    scope: None,
  };
  let mut props = ValueMap::default();
  props.insert("title".to_owned(), Value::str("Pace"));
  props.insert("items".to_owned(), Value::seq(vec![Value::F64(1.0), Value::F64(2.0)]));
  props.insert("image".to_owned(), Value::Null);
  let rendered = Interpreter::default().render_module("src/ui/Feedback.tsx#Feedback", &prepare(&component), &props, &Components::new()).unwrap();
  assert_eq!(rendered.html, "buttons", "{rendered:?}");
  let keys: Vec<&_> = rendered.hoisted.keys().collect();
  assert_eq!(keys, ["src/ui/Feedback.tsx#Feedback|3", "src/ui/Feedback.tsx#Feedback|4@0", "src/ui/Feedback.tsx#Feedback|4@1", "src/ui/Feedback.tsx#Feedback|5"], "the untaken branches' chunks, the loop's by iteration: {:?}", rendered.hoisted);
  assert_eq!(rendered.hoisted["src/ui/Feedback.tsx#Feedback|3"], Value::str("Pace"));
  assert_eq!(rendered.hoisted["src/ui/Feedback.tsx#Feedback|4@1"], Value::str("2"));
  assert_eq!(rendered.hoisted["src/ui/Feedback.tsx#Feedback|5"], Value::str(""), "a field of the null image reads as null, which prints as nothing");
}

#[test]
fn an_island_carries_its_own_table_in_its_props() {
  let mut library = Components::new();
  library.insert(
    "src/ui/Help.tsx#Help".to_owned(),
    Arc::new(Component { body: Vec::new(), render: Tmpl::Expr(hoist(0, fixed(Expr::var("$props").field("n")))), state: Vec::new(), stores: Vec::new(), handlers: Vec::new(), owner: crate::ast::Owner::React, shadow: None, scope: None }),
  );
  let page = Component {
    body: Vec::new(),
    render: Tmpl::Fragment(vec![
      Tmpl::Expr(hoist(0, fixed(Expr::Lit(Lit::Float(1.0))))),
      Tmpl::Island { module: "src/ui/Help.tsx#Help".to_owned(), props: vec![Entry::Field("n".to_owned(), Expr::Lit(Lit::Float(2.0)))], children: Vec::new(), when: None, mode: None, id: 9, define: false },
    ]), state: Vec::new(), stores: Vec::new(), handlers: Vec::new(), owner: crate::ast::Owner::React, shadow: None, scope: None
  };
  let rendered = Interpreter::default().render_module("routes/index/page.tsx#default", &page, &ValueMap::default(), &library).unwrap();
  let keys: Vec<&_> = rendered.hoisted.keys().collect();
  assert_eq!(keys, ["routes/index/page.tsx#default|0"], "the island's values are not the page's");
  assert_eq!(rendered.islands[0].body.hoisted.keys().collect::<Vec<_>>(), ["src/ui/Help.tsx#Help|0"]);
  let mount = rendered.islands[0].mount_props();
  assert_eq!(mount.get("n"), Some(&Value::F64(2.0)));
  let Some(Value::Map(table)) = mount.get(HOISTED_PROP) else { panic!("{mount:?}") };
  assert_eq!(table["src/ui/Help.tsx#Help|0"], Value::str("2.0"));
}

#[test]
fn mount_props_wrap_each_prop_the_types_prop_names_and_drop_the_types_prop() {
  let mut map = ValueMap::default();
  map.insert("a".to_owned(), Value::int(1i64));
  let mut types = ValueMap::default();
  for (name, tag) in [("m", typed::MAP), ("s", typed::SET), ("d", typed::DATE), ("b", typed::BIGINT), ("none", typed::DATE), ("shape", typed::MAP)] {
    types.insert(name.to_owned(), Value::str(tag));
  }
  let mut props = ValueMap::default();
  props.insert("m".to_owned(), Value::Map(map));
  props.insert("s".to_owned(), Value::seq(vec![Value::int(1i64)]));
  props.insert("d".to_owned(), Value::F64(5.0));
  props.insert("b".to_owned(), Value::Int(7));
  props.insert("none".to_owned(), Value::Null);
  props.insert("shape".to_owned(), Value::str("not a map"));
  props.insert("plain".to_owned(), Value::str("x"));
  props.insert(TYPES_PROP.to_owned(), Value::Map(types));
  let island = RenderedIsland { module: "m".to_owned(), props, when: None, mode: None, state: ValueMap::default(), reads: Vec::new(), rendered_store: ValueMap::default(), awaits: Vec::new(), key: String::new(), body: Rendered { html: String::new(), islands: Vec::new(), places: Vec::new(), hoisted: ValueMap::default(), whole: true }, rendered: true };
  let mount = island.mount_props();
  let wrapped = |tag: &str, payload: Value| Value::Variant { tag: tag.to_owned(), payload: Some(Box::new(payload)) };
  assert_eq!(mount.get("m"), Some(&wrapped(typed::MAP, Value::seq(vec![Value::seq(vec![Value::str("a"), Value::int(1i64)])]))));
  assert_eq!(mount.get("s"), Some(&wrapped(typed::SET, Value::seq(vec![Value::int(1i64)]))));
  assert_eq!(mount.get("d"), Some(&wrapped(typed::DATE, Value::F64(5.0))));
  assert_eq!(mount.get("b"), Some(&wrapped(typed::BIGINT, Value::Int(7))));
  assert_eq!(mount.get("none"), Some(&Value::Null), "a null prop is not a typed value");
  assert_eq!(mount.get("shape"), Some(&Value::str("not a map")), "a value of another shape is left as it is");
  assert_eq!(mount.get("plain"), Some(&Value::str("x")));
  assert!(!mount.contains_key(TYPES_PROP), "{mount:?}");
}

#[test]
fn a_shadow_root_inside_an_island_records_nothing_in_the_islands_table() {
  let mut library = Components::new();
  library.insert(
    "elements/x-box.tsx#default".to_owned(),
    Arc::new(Component { body: Vec::new(), render: Tmpl::Expr(hoist(0, fixed(Expr::var("$props").field("n")))), state: Vec::new(), stores: Vec::new(), handlers: Vec::new(), owner: crate::ast::Owner::Fsr, shadow: None, scope: None }),
  );
  library.insert(
    "src/ui/Help.tsx#Help".to_owned(),
    Arc::new(Component {
      body: Vec::new(),
      render: Tmpl::Element {
        tag: "x-box".to_owned(),
        attrs: vec![Entry::Field(SHADOW_ATTR.to_owned(), Expr::lit_str("elements/x-box.tsx#default")), Entry::Field("n".to_owned(), Expr::var("$props").field("n"))],
        children: Vec::new(),
      },
      state: Vec::new(),
      stores: Vec::new(),
      handlers: Vec::new(),
      owner: crate::ast::Owner::React,
      shadow: None, scope: None,
    }),
  );
  let page = Component {
    body: Vec::new(),
    render: Tmpl::Island { module: "src/ui/Help.tsx#Help".to_owned(), props: vec![Entry::Field("n".to_owned(), Expr::Lit(Lit::Float(2.0)))], children: Vec::new(), when: None, mode: None, id: 9, define: false },
    state: Vec::new(),
    stores: Vec::new(),
    handlers: Vec::new(),
    owner: crate::ast::Owner::React,
    shadow: None, scope: None,
  };
  let rendered = Interpreter::default().render_module("routes/index/page.tsx#default", &page, &ValueMap::default(), &library).unwrap();
  let island = &rendered.islands[0].body;
  assert!(island.html.contains("<template shadowrootmode=\"open\">2.0</template>"), "{}", island.html);
  assert!(island.hoisted.is_empty(), "the template's hoist 0 is not Help's: {:?}", island.hoisted);
}
