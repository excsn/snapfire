use super::*;
use crate::ast::{Builtin, Expr, Lit, Stmt};

#[test]
fn markup_held_as_a_value_renders_where_it_is_interpolated_in_the_scope_it_was_written_in() {
  let el = |tag: &str, children: Vec<Tmpl>| Tmpl::Element { tag: tag.to_owned(), attrs: Vec::new(), children };
  let markup = |t: Tmpl| Expr::Markup(Arc::new(t));
  let var = |n: &str| Expr::Var(n.to_owned());
  let component = Component {
    body: vec![
      Stmt::Let { name: "n".to_owned(), expr: p("name") },
      Stmt::Let { name: "m".to_owned(), expr: markup(el("b", vec![Tmpl::Expr(var("n"))])) },
      Stmt::Let { name: "xs".to_owned(), expr: Expr::Array(vec![Entry::Item(markup(el("i", vec![Tmpl::Text("a".to_owned())]))), Entry::Item(var("m"))]) },
      Stmt::Let { name: "o".to_owned(), expr: Expr::object(vec![("body", markup(el("p", vec![Tmpl::Expr(var("n"))])))]) },
      Stmt::Let { name: "rows".to_owned(), expr: Expr::Map(Box::new(p("items")), Box::new(Expr::lambda(&["x"], markup(el("li", vec![Tmpl::Expr(var("x")), Tmpl::Expr(var("n"))]))))) },
      Stmt::Let { name: "n".to_owned(), expr: Expr::lit_str("later") },
    ],
    render: Tmpl::Fragment(vec![Tmpl::Expr(var("m")), el("ul", vec![Tmpl::Expr(var("xs"))]), Tmpl::Expr(var("o").field("body")), el("ol", vec![Tmpl::Expr(var("rows"))]), Tmpl::Expr(var("n"))]),
    state: Vec::new(),
    stores: Vec::new(),
    handlers: Vec::new(),
    owner: crate::ast::Owner::Fsr,
    shadow: None,
    scope: None,
  };
  let mut props = ValueMap::default();
  props.insert("name".to_owned(), Value::str("Ann"));
  props.insert("items".to_owned(), Value::seq(vec![Value::str("x"), Value::str("y")]));
  for component in [component.clone(), prepare(&component)] {
    let html = Interpreter::default().render(&component, &props, &Components::new()).unwrap().html;
    assert_eq!(html, "<b>Ann</b><ul><i>a</i><b>Ann</b></ul><p>Ann</p><ol><li>xAnn</li><li>yAnn</li></ol>later");
  }
}

#[test]
fn a_markup_value_passed_to_a_component_the_browser_renders_is_a_region_the_browser_adopts() {
  let component = Component {
    body: vec![Stmt::Let { name: "m".to_owned(), expr: Expr::Markup(Arc::new(Tmpl::Text("hi".to_owned()))) }],
    render: Tmpl::Island { module: "src/ui/Toggle.tsx#Toggle".to_owned(), props: vec![Entry::Field("label".to_owned(), Expr::Var("m".to_owned()))], children: Vec::new(), when: None, mode: None, id: 0, define: false },
    state: Vec::new(),
    stores: Vec::new(),
    handlers: Vec::new(),
    owner: crate::ast::Owner::Fsr,
    shadow: None,
    scope: None,
  };
  let rendered = Interpreter::default().render(&component, &ValueMap::default(), &Components::new()).unwrap();
  let island = &rendered.islands[0];
  let mut marker = ValueMap::default();
  marker.insert(MARKUP_PROP.to_owned(), Value::Int(0));
  assert_eq!(island.props.get("label"), Some(&Value::Map(marker)), "the prop crosses as the number of its region");
  assert_eq!(island.body.html, "<template data-sf-m=\"0\">hi</template>");
}

#[test]
fn prepare_folds_what_is_constant_and_leaves_what_reads_the_scope() {
  let object = Expr::Object(vec![Entry::Field("a".to_owned(), Expr::Lit(Lit::Str("x".to_owned()))), Entry::Field("n".to_owned(), Expr::Lit(Lit::Int(1)))]);
  let template = Expr::Template(vec![Expr::Lit(Lit::Str("p-".to_owned())), Expr::Lit(Lit::Str("q".to_owned()))]);
  let read = Expr::Field(Box::new(Expr::Var("s".to_owned())), "a".to_owned());
  let test = Expr::Compare(crate::ast::CompareOp::Eq, Box::new(Expr::Var("$props".to_owned()).field("k")), Box::new(Expr::Lit(Lit::Str("v".to_owned()))));
  let component = Component {
    body: Vec::new(),
    render: Tmpl::Let {
      name: "s".to_owned(),
      expr: object,
      then: Box::new(Tmpl::Fragment(vec![Tmpl::Expr(template), Tmpl::Expr(read.clone()), Tmpl::If { cond: test, then: Box::new(Tmpl::Text("yes".to_owned())), r#else: Some(Box::new(Tmpl::Text("no".to_owned()))) }])),
    },
    state: Vec::new(),
    stores: Vec::new(),
    handlers: Vec::new(),
    owner: crate::ast::Owner::Fsr,
    shadow: None,
    scope: None,
  };
  let prepared = prepare(&component);
  let Tmpl::Let { expr, then, .. } = &prepared.render else { panic!() };
  assert_eq!(*expr, Expr::Object(vec![Entry::Field("a".to_owned(), Expr::Folded(Value::str("x"))), Entry::Field("n".to_owned(), Expr::Folded(Value::Int(1)))]), "an object literal stays a new object and what it holds folds");
  let Tmpl::Fragment(children) = &**then else { panic!() };
  assert_eq!(children[0], Tmpl::Expr(Expr::Folded(Value::str("p-q"))));
  assert_eq!(children[1], Tmpl::Expr(read), "a read of the scope keeps its shape");
  let Tmpl::If { cond: Expr::Compare(_, _, literal), .. } = &children[2] else { panic!() };
  assert_eq!(**literal, Expr::Folded(Value::str("v")));
  let mut props = ValueMap::default();
  props.insert("k".to_owned(), Value::str("v"));
  assert_eq!(Interpreter::default().render(&prepared, &props, &Components::new()).unwrap().html, "p-qxyes");
}
fn props(entries: &[(&str, Value)]) -> ValueMap {
  entries.iter().map(|(k, v)| ((*k).to_owned(), v.clone())).collect()
}

fn p(name: &str) -> Expr {
  Expr::var("$props").field(name)
}

#[test]
fn an_svg_attribute_reaches_the_document_the_way_svg_spells_it() {
  for (jsx, printed) in [
    ("markerEnd", "marker-end"),
    ("strokeWidth", "stroke-width"),
    ("strokeDasharray", "stroke-dasharray"),
    ("fillRule", "fill-rule"),
    ("clipPath", "clip-path"),
    ("stopColor", "stop-color"),
    ("textAnchor", "text-anchor"),
    ("dominantBaseline", "dominant-baseline"),
    ("vectorEffect", "vector-effect"),
    ("paintOrder", "paint-order"),
    ("xlinkHref", "xlink:href"),
  ] {
    assert_eq!(html_attr_name(jsx), printed);
  }
  for camel in ["viewBox", "refX", "refY", "markerWidth", "markerHeight", "preserveAspectRatio", "gradientUnits", "patternUnits", "spreadMethod", "startOffset"] {
    assert_eq!(html_attr_name(camel), camel, "SVG spells this one in camelCase itself");
  }
}

#[test]
fn an_svg_element_prints_its_dashed_attributes() {
  let component = Component {
    body: Vec::new(),
    render: Tmpl::Element {
      tag: "path".to_owned(),
      attrs: vec![
        Entry::Field(html_attr_name("markerEnd").to_owned(), Expr::lit_str("url(#a)")),
        Entry::Field(html_attr_name("strokeWidth").to_owned(), Expr::Lit(Lit::Int(2))),
        Entry::Field(html_attr_name("viewBox").to_owned(), Expr::lit_str("0 0 8 8")),
      ],
      children: Vec::new(),
    },
    state: Vec::new(),
    stores: Vec::new(),
    handlers: Vec::new(),
    owner: crate::ast::Owner::React,
    shadow: None, scope: None,
  };
  let html = Interpreter::default().render(&component, &ValueMap::default(), &Components::new()).unwrap().html;
  assert_eq!(html, "<path marker-end=\"url(#a)\" stroke-width=\"2\" viewBox=\"0 0 8 8\"></path>");
}

#[test]
fn markup_an_application_produced_reaches_the_document_unescaped() {
  let raw = |html: Expr, children: Vec<Tmpl>| Component {
    body: Vec::new(),
    render: Tmpl::Element { tag: "div".to_owned(), attrs: vec![Entry::Field("class".to_owned(), Expr::lit_str("md")), Entry::Field(RAW_ATTR.to_owned(), html)], children },
    state: Vec::new(),
    stores: Vec::new(),
    handlers: Vec::new(),
    owner: crate::ast::Owner::React,
    shadow: None, scope: None,
  };
  let render = |component: &Component, props: &ValueMap| Interpreter::default().render(component, props, &Components::new()).unwrap().html;

  let component = raw(p("body"), Vec::new());
  let html = render(&component, &props(&[("body", Value::str("<p>a <b>markdown</b> blip</p>".to_owned()))]));
  assert_eq!(html, "<div class=\"md\"><p>a <b>markdown</b> blip</p></div>", "the string is written as markup, not as text");

  assert_eq!(render(&component, &props(&[("body", Value::Null)])), "<div class=\"md\"></div>", "React renders nothing for a missing __html");

  let with_children = raw(Expr::lit_str("<i>from the loader</i>"), vec![Tmpl::Text("never rendered".to_owned())]);
  assert_eq!(render(&with_children, &ValueMap::default()), "<div class=\"md\"><i>from the loader</i></div>", "the markup replaces the children the way React refuses to have both");
}

#[test]
fn elements_text_and_interpolation_print_like_react() {
  let component = Component {
    body: vec![Stmt::Let { name: "n".to_owned(), expr: Expr::Length(Box::new(p("items"))) }],
    render: Tmpl::Element {
      tag: "p".to_owned(),
      attrs: vec![Entry::Field("class".to_owned(), Expr::lit_str("count")), Entry::Field("hidden".to_owned(), Expr::Lit(Lit::Bool(false))), Entry::Field("title".to_owned(), Expr::Lit(Lit::Null))],
      children: vec![Tmpl::Expr(Expr::var("n")), Tmpl::Text(" result".to_owned()), Tmpl::Expr(Expr::Ternary(Box::new(Expr::Compare(crate::ast::CompareOp::Eq, Box::new(Expr::var("n")), Box::new(Expr::Lit(Lit::Float(1.0))))), Box::new(Expr::lit_str("")), Box::new(Expr::lit_str("s")))), Tmpl::Text(" <3".to_owned())],
    }, state: Vec::new(), stores: Vec::new(), handlers: Vec::new(), owner: crate::ast::Owner::React, shadow: None, scope: None
  };
  let values = props(&[("items", Value::seq(vec![Value::Null, Value::Null]))]);
  let react = Interpreter::default().with_frameworks(Frameworks { react: Some(ReactMajor::V19), vue: None, svelte: None });
  assert_eq!(react.render(&component, &values, &Components::new()).unwrap().html, "<p class=\"count\">2<!-- --> result<!-- -->s<!-- --> &lt;3</p>");
  assert_eq!(Interpreter::default().render(&component, &values, &Components::new()).unwrap().html, "<p class=\"count\">2 results &lt;3</p>", "plain markup joins adjacent text");
}

#[test]
fn for_if_and_let_are_the_jsx_idioms() {
  let component = Component {
    body: Vec::new(),
    render: Tmpl::Element {
      tag: "ul".to_owned(),
      attrs: Vec::new(),
      children: vec![Tmpl::For {
        over: p("lines"),
        params: vec!["l".to_owned(), "i".to_owned()],
        body: Box::new(Tmpl::Let {
          name: "qty".to_owned(),
          expr: Expr::Num(Box::new(Expr::var("l").field("quantity"))),
          then: Box::new(Tmpl::Element {
            tag: "li".to_owned(),
            attrs: vec![Entry::Field("data-i".to_owned(), Expr::var("i"))],
            children: vec![Tmpl::Expr(Expr::var("qty")), Tmpl::If { cond: Expr::Compare(crate::ast::CompareOp::Gt, Box::new(Expr::var("qty")), Box::new(Expr::Lit(Lit::Float(1.0)))), then: Box::new(Tmpl::Element { tag: "b".to_owned(), attrs: Vec::new(), children: vec![Tmpl::Text("many".to_owned())] }), r#else: None }],
          }),
        }),
      }],
    }, state: Vec::new(), stores: Vec::new(), handlers: Vec::new(), owner: crate::ast::Owner::React, shadow: None, scope: None
  };
  let lines = Value::seq(vec![Value::Map(props(&[("quantity", Value::Int(1))])), Value::Map(props(&[("quantity", Value::Int(3))]))]);
  let html = (Interpreter::default().render(&component, &props(&[("lines", lines)]), &Components::new())).unwrap().html;
  assert_eq!(html, "<ul><li data-i=\"0\">1</li><li data-i=\"1\">3<b>many</b></li></ul>");
}

#[test]
fn a_component_renders_another_with_its_own_props() {
  let mut library = Components::new();
  library.insert(
    "src/ui/Stars.tsx#Stars".to_owned(),
    Arc::new(Component {
      body: vec![Stmt::Let { name: "full".to_owned(), expr: Expr::Builtin { name: Builtin::Round, args: vec![p("rating")] } }],
      render: Tmpl::Element {
        tag: "span".to_owned(),
        attrs: vec![Entry::Field("title".to_owned(), Expr::Template(vec![Expr::Builtin { name: Builtin::ToFixed, args: vec![p("rating"), Expr::Lit(Lit::Float(1.0))] }, Expr::lit_str(" out of 5")]))],
        children: vec![Tmpl::Expr(Expr::Arith(crate::ast::ArithOp::Add, Box::new(Expr::Builtin { name: Builtin::Repeat, args: vec![Expr::lit_str("★"), Expr::var("full")] }), Box::new(Expr::Builtin { name: Builtin::Repeat, args: vec![Expr::lit_str("☆"), Expr::Arith(crate::ast::ArithOp::Sub, Box::new(Expr::Lit(Lit::Float(5.0))), Box::new(Expr::var("full")))] })))],
      }, state: Vec::new(), stores: Vec::new(), handlers: Vec::new(), owner: crate::ast::Owner::React, shadow: None, scope: None
    }),
  );
  let page = Component {
    body: Vec::new(),
    render: Tmpl::Fragment(vec![Tmpl::Component { module: "src/ui/Stars.tsx#Stars".to_owned(), props: vec![Entry::Field("rating".to_owned(), p("product").field("rating"))], children: Vec::new(), id: 0, keyed: false }, Tmpl::Expr(p("product").field("name"))]), state: Vec::new(), stores: Vec::new(), handlers: Vec::new(), owner: crate::ast::Owner::React, shadow: None, scope: None
  };
  let product = Value::Map(props(&[("rating", Value::F64(4.5)), ("name", Value::str("Filament"))]));
  let html = (Interpreter::default().render(&page, &props(&[("product", product)]), &library)).unwrap().html;
  assert_eq!(html, "<span title=\"4.5 out of 5\">★★★★★</span>Filament");
}

#[test]
fn children_render_in_the_callers_scope_and_spreads_merge_in_order() {
  let mut library = Components::new();
  library.insert(
    "src/ui/Page.tsx#Page".to_owned(),
    Arc::new(Component {
      body: Vec::new(),
      render: Tmpl::Element {
        tag: "main".to_owned(),
        attrs: vec![Entry::Field("class".to_owned(), p("className"))],
        children: vec![Tmpl::Element { tag: "h1".to_owned(), attrs: Vec::new(), children: vec![Tmpl::Expr(p("title"))] }, Tmpl::Component { module: "src/ui/Card.tsx#Card".to_owned(), props: Vec::new(), children: vec![Tmpl::Slot("content".to_owned())], id: 0, keyed: false }],
      }, state: Vec::new(), stores: Vec::new(), handlers: Vec::new(), owner: crate::ast::Owner::React, shadow: None, scope: None
    }),
  );
  library.insert(
    "src/ui/Card.tsx#Card".to_owned(),
    Arc::new(Component { body: Vec::new(), render: Tmpl::Element { tag: "div".to_owned(), attrs: vec![Entry::Field("class".to_owned(), Expr::lit_str("card"))], children: vec![Tmpl::Slot("content".to_owned()), Tmpl::Slot("content".to_owned())] }, state: Vec::new(), stores: Vec::new(), handlers: Vec::new(), owner: crate::ast::Owner::React, shadow: None, scope: None }),
  );
  let page = Component {
    body: vec![Stmt::Let { name: "header".to_owned(), expr: Expr::Object(vec![Entry::Field("title".to_owned(), Expr::lit_str("Picks")), Entry::Field("className".to_owned(), Expr::lit_str("wrong"))]) }],
    render: Tmpl::Component {
      module: "src/ui/Page.tsx#Page".to_owned(),
      props: vec![Entry::Spread(Expr::var("header")), Entry::Field("className".to_owned(), Expr::lit_str("catalog"))],
      children: vec![Tmpl::For {
        over: p("items"),
        params: vec!["it".to_owned()],
        body: Box::new(Tmpl::Element { tag: "p".to_owned(), attrs: vec![Entry::Spread(Expr::var("it").field("attrs")), Entry::Field("class".to_owned(), Expr::lit_str("item"))], children: vec![Tmpl::Expr(Expr::var("it").field("name")), Tmpl::Text(" for ".to_owned()), Tmpl::Expr(p("title"))] }),
      }],
      id: 0,
      keyed: false,
    }, state: Vec::new(), stores: Vec::new(), handlers: Vec::new(), owner: crate::ast::Owner::React, shadow: None, scope: None
  };
  let mut attrs = ValueMap::default();
  attrs.insert("className".to_owned(), Value::str("ignored"));
  attrs.insert("dataId".to_owned(), Value::Int(7));
  attrs.insert("onClick".to_owned(), Value::str("handler"));
  attrs.insert("hidden".to_owned(), Value::Bool(true));
  let items = Value::seq(vec![Value::Map(props(&[("name", Value::str("A")), ("attrs", Value::Map(attrs))])), Value::Map(props(&[("name", Value::str("B")), ("attrs", Value::Null)]))]);
  let html = (Interpreter::default().render(&page, &props(&[("items", items), ("title", Value::str("outer"))]), &library)).unwrap().html;
  assert_eq!(html, "<main class=\"catalog\"><h1>Picks</h1><div class=\"card\"><p class=\"item\" dataId=\"7\" hidden=\"\">A for outer</p><p class=\"item\">B for outer</p><p class=\"item\" dataId=\"7\" hidden=\"\">A for outer</p><p class=\"item\">B for outer</p></div></main>");
}

#[test]
fn void_elements_boolean_attributes_and_escaping() {
  let component = Component {
    body: Vec::new(),
    render: Tmpl::Fragment(vec![
      Tmpl::Element { tag: "input".to_owned(), attrs: vec![Entry::Field("value".to_owned(), Expr::lit_str("a \"b\" & c")), Entry::Field("disabled".to_owned(), Expr::Lit(Lit::Bool(true))), Entry::Field("aria-hidden".to_owned(), Expr::Lit(Lit::Bool(true)))], children: Vec::new() },
      Tmpl::Element { tag: "br".to_owned(), attrs: Vec::new(), children: Vec::new() },
      Tmpl::Expr(Expr::Lit(Lit::Bool(true))),
      Tmpl::Expr(Expr::Lit(Lit::Null)),
    ]), state: Vec::new(), stores: Vec::new(), handlers: Vec::new(), owner: crate::ast::Owner::React, shadow: None, scope: None
  };
  let react = Interpreter::default().with_frameworks(Frameworks { react: Some(ReactMajor::V19), vue: None, svelte: None });
  assert_eq!(react.render(&component, &ValueMap::default(), &Components::new()).unwrap().html, "<input value=\"a &quot;b&quot; &amp; c\" disabled=\"\" aria-hidden=\"true\"/><br/>");
  assert_eq!(Interpreter::default().render(&component, &ValueMap::default(), &Components::new()).unwrap().html, "<input value=\"a &quot;b&quot; &amp; c\" disabled=\"\" aria-hidden=\"true\"><br>", "plain markup closes no void element");
}

#[test]
fn a_video_and_its_track_take_react_doms_spellings() {
  let field = |name: &str, expr: Expr| Entry::Field(name.to_owned(), expr);
  let component = Component {
    body: Vec::new(),
    render: Tmpl::Element {
      tag: "video".to_owned(),
      attrs: vec![field("playsInline", Expr::Lit(Lit::Bool(true))), field("autoPlay", Expr::Lit(Lit::Bool(true))), field("controls", Expr::Lit(Lit::Bool(true))), field("disablePictureInPicture", Expr::Lit(Lit::Bool(false)))],
      children: vec![Tmpl::Element { tag: "track".to_owned(), attrs: vec![field("srcLang", Expr::lit_str("en")), field("default", Expr::Lit(Lit::Bool(true)))], children: Vec::new() }],
    },
    state: Vec::new(), stores: Vec::new(), handlers: Vec::new(), owner: crate::ast::Owner::React, shadow: None, scope: None
  };
  let react = Interpreter::default().with_frameworks(Frameworks { react: Some(ReactMajor::V18), vue: None, svelte: None });
  assert_eq!(react.render(&component, &ValueMap::default(), &Components::new()).unwrap().html, "<video playsinline=\"\" autoplay=\"\" controls=\"\"><track srcLang=\"en\" default=\"\"/></video>");
}

#[test]
fn a_store_read_takes_the_seed_and_falls_back_without_one() {
  let read = Expr::Coalesce(Box::new(Expr::Store("cart/count".to_owned())), Box::new(Expr::Lit(Lit::Float(0.0))));
  let inner = Component { body: vec![Stmt::Let { name: "n".to_owned(), expr: read.clone() }], render: Tmpl::Element { tag: "b".to_owned(), attrs: Vec::new(), children: vec![Tmpl::Expr(Expr::var("n"))] }, state: Vec::new(), stores: Vec::new(), handlers: Vec::new(), owner: crate::ast::Owner::React, shadow: None, scope: None };
  let mut library = Components::new();
  library.insert("src/ui/Badge.tsx#Badge".to_owned(), Arc::new(inner));
  let outer = Component {
    body: vec![Stmt::Let { name: "n".to_owned(), expr: read }],
    render: Tmpl::Fragment(vec![
      Tmpl::Expr(Expr::var("n")),
      Tmpl::Component { module: "src/ui/Badge.tsx#Badge".to_owned(), props: Vec::new(), children: Vec::new(), id: 0, keyed: false },
    ]), state: Vec::new(), stores: Vec::new(), handlers: Vec::new(), owner: crate::ast::Owner::React, shadow: None, scope: None
  };
  let render = |props: ValueMap| Interpreter::default().render(&outer, &props, &library).unwrap().html;
  assert_eq!(render(ValueMap::default()), "0<b>0</b>", "no seed leaves both reads on the fallback");
  let mut store = ValueMap::default();
  store.insert("cart/count".to_owned(), Value::Int(3));
  let mut props = ValueMap::default();
  props.insert("$store".to_owned(), Value::Map(store));
  assert_eq!(render(props), "3<b>3</b>", "a nested component reads the seed without a prop");
}

#[test]
fn a_slot_placement_shows_its_fallback_until_the_plan_fills_it() {
  let filled = Expr::Builtin { name: Builtin::Includes, args: vec![Expr::Coalesce(Box::new(Expr::Var("$props".to_owned()).field("$slots")), Box::new(Expr::Array(Vec::new()))), Expr::lit_str("modal")] };
  let component = Component {
    body: Vec::new(),
    render: Tmpl::Element { tag: "sf-s".to_owned(), attrs: Vec::new(), children: vec![Tmpl::If { cond: filled, then: Box::new(Tmpl::Slot("modal".to_owned())), r#else: Some(Box::new(Tmpl::Text("closed".to_owned()))) }] }, state: Vec::new(), stores: Vec::new(), handlers: Vec::new(), owner: crate::ast::Owner::React, shadow: None, scope: None
  };
  let render = |props: ValueMap| Interpreter::default().render(&component, &props, &Components::new()).unwrap().html;
  assert_eq!(render(ValueMap::default()), "<sf-s>closed</sf-s>", "no $slots at all shows the fallback");
  let mut props = ValueMap::default();
  props.insert("$slots".to_owned(), Value::seq(vec![Value::str("content")]));
  assert_eq!(render(props.clone()), "<sf-s>closed</sf-s>");
  props.insert("$slots".to_owned(), Value::seq(vec![Value::str("content"), Value::str("modal")]));
  assert_eq!(render(props), format!("<sf-s>{}</sf-s>", slot_mark("modal")));
}

#[test]
fn an_island_rendered_alone_writes_an_empty_children_region_where_it_places_its_children() {
  let component = Component {
    body: Vec::new(),
    render: Tmpl::Element { tag: "div".to_owned(), attrs: Vec::new(), children: vec![Tmpl::Slot("content".to_owned())] }, state: Vec::new(), stores: Vec::new(), handlers: Vec::new(), owner: crate::ast::Owner::React, shadow: None, scope: None
  };
  let island = Interpreter::default().render_island("src/ui/Box.tsx#default", &component, &ValueMap::default(), &Components::new()).unwrap();
  assert_eq!(island.html, format!("<div>{CHILDREN_OPEN}</sf-s></div>"));
  let root = Interpreter::default().render_module("src/ui/Box.tsx#default", &component, &ValueMap::default(), &Components::new()).unwrap();
  assert_eq!(root.html, format!("<div>{}</div>", slot_mark("content")), "a root component still leaves the mark its caller fills");
}

#[test]
fn builtins_follow_javascript() {
  let cases: Vec<(Expr, &str)> = vec![
    (Expr::Builtin { name: Builtin::ToFixed, args: vec![Expr::Lit(Lit::Float(24.0)), Expr::Lit(Lit::Float(2.0))] }, "24.00"),
    (Expr::Builtin { name: Builtin::ToFixed, args: vec![Expr::Lit(Lit::Float(2.345)), Expr::Lit(Lit::Float(2.0))] }, "2.35"),
    (Expr::Builtin { name: Builtin::Round, args: vec![Expr::Lit(Lit::Float(2.5))] }, "3"),
    (Expr::Builtin { name: Builtin::Round, args: vec![Expr::Lit(Lit::Float(-2.5))] }, "-2"),
    (Expr::Builtin { name: Builtin::LocaleNumber, args: vec![Expr::Lit(Lit::Float(1234567.0))] }, "1,234,567"),
    (Expr::Builtin { name: Builtin::LocaleNumber, args: vec![Expr::Lit(Lit::Int(1834))] }, "1,834"),
    (Expr::Builtin { name: Builtin::Join, args: vec![Expr::Array(vec![crate::ast::Entry::Item(Expr::lit_str("A1")), crate::ast::Entry::Item(Expr::lit_str("B2"))]), Expr::lit_str(", ")] }, "A1, B2"),
    (Expr::Builtin { name: Builtin::EncodeUriComponent, args: vec![Expr::lit_str("a b&c/é")] }, "a%20b%26c%2F%C3%A9"),
    (Expr::Builtin { name: Builtin::Min, args: vec![Expr::Lit(Lit::Int(12)), Expr::Lit(Lit::Float(10.0))] }, "10"),
    (Expr::Map(Box::new(Expr::Builtin { name: Builtin::Range, args: vec![Expr::Lit(Lit::Float(3.0))] }), Box::new(Expr::lambda(&["_", "i"], Expr::Arith(crate::ast::ArithOp::Add, Box::new(Expr::var("i")), Box::new(Expr::Lit(Lit::Float(1.0))))))), "123"),
  ];
  for (expr, expected) in cases {
    let component = Component { body: Vec::new(), render: Tmpl::Expr(expr.clone()), state: Vec::new(), stores: Vec::new(), handlers: Vec::new(), owner: crate::ast::Owner::React, shadow: None, scope: None };
    let html = (Interpreter::default().render(&component, &ValueMap::default(), &Components::new())).unwrap().html;
    assert_eq!(html, expected, "{expr:?}");
  }
}

fn decoded(text: &str) -> Result<String, String> {
  let expr = Expr::Builtin { name: Builtin::DecodeUriComponent, args: vec![Expr::lit_str(text)] };
  let component = Component { body: Vec::new(), render: Tmpl::Expr(expr), state: Vec::new(), stores: Vec::new(), handlers: Vec::new(), owner: crate::ast::Owner::React, shadow: None, scope: None };
  Interpreter::default().render(&component, &ValueMap::default(), &Components::new()).map(|out| out.html).map_err(|e| e.to_string())
}

#[test]
fn decode_uri_component_reverses_encode_and_throws_a_uri_error_on_a_malformed_escape() {
  assert_eq!(decoded("%2Fservices%2Fa%20b%C3%A9%f0%9f%98%80").unwrap(), "/services/a bé😀");
  assert_eq!(decoded("plain").unwrap(), "plain");
  for malformed in ["%", "%4", "%zz", "%+F", "%C3", "%E0%A4%A", "%ED%A0%80", "%C0%AF"] {
    let err = decoded(malformed).unwrap_err();
    assert!(err.contains("URI malformed"), "{malformed}: {err}");
  }
}
