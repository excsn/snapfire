use snapfire_fsr_ir::ast::{ArithOp, Builtin, CompareOp, Entry, Expr, Handler, Lit, LogicOp, Stmt, Tmpl};
use snapfire_fsr_ir::Component;
use snapfire_fsr_plan::{
  ActionEntry, Child, ComponentEntry, HandlerEntry, Manifest, Node, RouteEntry, RowOwner,
  SourceEntry,
};

fn v(name: &str) -> Box<Expr> {
  Box::new(Expr::Var(name.to_owned()))
}

fn s(text: &str) -> Expr {
  Expr::Lit(Lit::Str(text.to_owned()))
}

/// Every `Expr` variant once, plus the atoms whose bare spelling would other-
/// wise read back as something else.
fn every_expr() -> Vec<Expr> {
  vec![
    Expr::Param("id".to_owned()),
    Expr::Query("page".to_owned()),
    Expr::Session("cart".to_owned()),
    Expr::Store("theme".to_owned()),
    Expr::Identity(vec!["claims".to_owned(), "sub".to_owned()]),
    Expr::Locale,
    Expr::Path,
    Expr::Input,
    Expr::Now,
    Expr::Var("$props".to_owned()),
    Expr::Var("nil".to_owned()),
    Expr::Var("#t".to_owned()),
    Expr::Var("42".to_owned()),
    Expr::Var("a b".to_owned()),
    Expr::Const("src/x.ts#RATE".to_owned()),
    Expr::Lit(Lit::Null),
    Expr::Lit(Lit::Bool(true)),
    Expr::Lit(Lit::Bool(false)),
    Expr::Lit(Lit::Int(i128::MIN)),
    Expr::Lit(Lit::Int(0)),
    Expr::Lit(Lit::Float(1.5)),
    Expr::Lit(Lit::Float(2.0)),
    Expr::Lit(Lit::Float(f64::INFINITY)),
    Expr::Lit(Lit::Float(f64::NEG_INFINITY)),
    s(""),
    s("a \"quoted\" \\ line\nand a tab\t"),
    s("nil"),
    Expr::Object(every_entry()),
    Expr::Array(vec![Entry::Item(s("one")), Entry::Spread(*v("rest"))]),
    Expr::Field(v("row"), "total".to_owned()),
    Expr::Index(v("rows"), Box::new(Expr::Lit(Lit::Int(2)))),
    Expr::Arith(ArithOp::Add, v("a"), v("b")),
    Expr::Arith(ArithOp::Sub, v("a"), v("b")),
    Expr::Arith(ArithOp::Mul, v("a"), v("b")),
    Expr::Arith(ArithOp::Div, v("a"), v("b")),
    Expr::Arith(ArithOp::Rem, v("a"), v("b")),
    Expr::Compare(CompareOp::Eq, v("a"), v("b")),
    Expr::Compare(CompareOp::Ne, v("a"), v("b")),
    Expr::Compare(CompareOp::Lt, v("a"), v("b")),
    Expr::Compare(CompareOp::Le, v("a"), v("b")),
    Expr::Compare(CompareOp::Gt, v("a"), v("b")),
    Expr::Compare(CompareOp::Ge, v("a"), v("b")),
    Expr::Logic(LogicOp::And, v("a"), v("b")),
    Expr::Logic(LogicOp::Or, v("a"), v("b")),
    Expr::Not(v("a")),
    Expr::Coalesce(v("a"), v("b")),
    Expr::Ternary(v("a"), v("b"), v("c")),
    Expr::Template(vec![s("total "), *v("n")]),
    Expr::Call {
      service: "cart".to_owned(),
      method: "add".to_owned(),
      args: vec![("id".to_owned(), *v("id")), ("qty".to_owned(), Expr::Lit(Lit::Int(1)))],
    },
    Expr::Call { service: "cart".to_owned(), method: "clear".to_owned(), args: Vec::new() },
    Expr::NativeCall {
      module: "img".to_owned(),
      method: "thumb".to_owned(),
      args: vec![("src".to_owned(), *v("src"))],
      sync: false,
    },
    Expr::NativeCall {
      module: "img".to_owned(),
      method: "blur".to_owned(),
      args: Vec::new(),
      sync: true,
    },
    Expr::Lambda { params: vec!["n".to_owned(), "q".to_owned()], body: v("n") },
    Expr::Lambda { params: Vec::new(), body: v("n") },
    Expr::Apply { f: v("helper"), args: vec![*v("a"), *v("b")] },
    Expr::Builtin { name: Builtin::Round, args: vec![*v("n")] },
    Expr::Builtin { name: Builtin::ToFixed, args: vec![*v("n"), Expr::Lit(Lit::Int(2))] },
    Expr::Builtin { name: Builtin::EncodeUriComponent, args: vec![*v("q")] },
    Expr::Builtin { name: Builtin::LocaleNumber, args: vec![*v("n")] },
    Expr::Builtin { name: Builtin::Range, args: vec![Expr::Lit(Lit::Int(3))] },
    Expr::Builtin { name: Builtin::Omit, args: vec![*v("o"), s("key")] },
    Expr::Ext { module: "intl".to_owned(), name: "number".to_owned(), args: vec![*v("n")] },
    Expr::Map(v("rows"), v("f")),
    Expr::Filter(v("rows"), v("f")),
    Expr::Reduce(v("rows"), v("seed"), v("f")),
    Expr::Find(v("rows"), v("f")),
    Expr::FindIndex(v("rows"), v("f")),
    Expr::Some(v("rows"), v("f")),
    Expr::Every(v("rows"), v("f")),
    Expr::Entries(v("o")),
    Expr::Keys(v("o")),
    Expr::Values(v("o")),
    Expr::Length(v("rows")),
    Expr::Str(v("n")),
    Expr::Num(v("s")),
    Expr::BigInt(v("n")),
    Expr::Hoist { id: 7, expr: v("n") },
  ]
}

fn every_entry() -> Vec<Entry> {
  vec![
    Entry::Field("class".to_owned(), s("wide")),
    Entry::Field("...".to_owned(), s("a field whose name is a marker")),
    Entry::Item(s("loose")),
    Entry::Spread(*v("rest")),
    Entry::Computed(s("k"), *v("val")),
  ]
}

fn every_stmt() -> Vec<Stmt> {
  vec![
    Stmt::Let { name: "n".to_owned(), expr: *v("$props") },
    Stmt::If { cond: *v("ok"), then: vec![Stmt::Return(s("yes"))], r#else: Vec::new() },
    Stmt::If {
      cond: *v("ok"),
      then: vec![Stmt::Return(s("yes"))],
      r#else: vec![Stmt::Return(s("no"))],
    },
    Stmt::ForOf { name: "row".to_owned(), over: *v("rows"), body: vec![Stmt::Expr(*v("row"))] },
    Stmt::ForOf { name: "row".to_owned(), over: *v("rows"), body: Vec::new() },
    Stmt::Return(*v("out")),
    Stmt::Guard {
      cond: *v("missing"),
      kind: "NotFound".to_owned(),
      message: "no such order".to_owned(),
    },
    Stmt::SessionSet { key: "cart".to_owned(), path: vec![s("items")], value: *v("next") },
    Stmt::SessionSet { key: "cart".to_owned(), path: Vec::new(), value: *v("next") },
    Stmt::SessionDelete { key: "cart".to_owned(), path: vec![s("items")] },
    Stmt::SessionDelete { key: "cart".to_owned(), path: Vec::new() },
    Stmt::Expr(*v("side")),
  ]
}

fn every_tmpl() -> Vec<Tmpl> {
  vec![
    Tmpl::Text("plain".to_owned()),
    Tmpl::Text("<>&\"{}".to_owned()),
    Tmpl::Expr(*v("n")),
    Tmpl::Element { tag: "div".to_owned(), attrs: every_entry(), children: vec![Tmpl::Text("in".to_owned())] },
    Tmpl::Element { tag: "br".to_owned(), attrs: Vec::new(), children: Vec::new() },
    Tmpl::Fragment(vec![Tmpl::Text("a".to_owned()), Tmpl::Text("b".to_owned())]),
    Tmpl::Fragment(Vec::new()),
    Tmpl::If {
      cond: *v("ok"),
      then: Box::new(Tmpl::Text("yes".to_owned())),
      r#else: Some(Box::new(Tmpl::Text("no".to_owned()))),
    },
    Tmpl::If { cond: *v("ok"), then: Box::new(Tmpl::Text("yes".to_owned())), r#else: None },
    Tmpl::For {
      over: *v("rows"),
      params: vec!["row".to_owned(), "i".to_owned()],
      body: Box::new(Tmpl::Expr(*v("row"))),
    },
    Tmpl::Let {
      name: "n".to_owned(),
      expr: *v("count"),
      then: Box::new(Tmpl::Expr(*v("n"))),
    },
    Tmpl::Component {
      module: "src/Card.tsx#Card".to_owned(),
      props: vec![Entry::Field("title".to_owned(), s("hi"))],
      children: vec![Tmpl::Text("child".to_owned())],
      id: 3,
    },
    Tmpl::Component { module: "src/Card.tsx#Card".to_owned(), props: Vec::new(), children: Vec::new(), id: 0 },
    Tmpl::Island {
      module: "src/Cart.tsx#Cart".to_owned(),
      props: vec![Entry::Field("open".to_owned(), Expr::Lit(Lit::Bool(true)))],
      children: vec![Tmpl::Text("fallback".to_owned())],
      when: Some("idle".to_owned()),
      mode: Some("server".to_owned()),
      id: 4,
    },
    Tmpl::Island {
      module: "src/Cart.tsx#Cart".to_owned(),
      props: Vec::new(),
      children: Vec::new(),
      when: None,
      mode: None,
      id: 0,
    },
    Tmpl::Slot("content".to_owned()),
    Tmpl::Baked {
      open: "<p class=\"x\">".to_owned(),
      tag: Some("p".to_owned()),
      children: vec![Tmpl::Text("in".to_owned())],
    },
    Tmpl::Baked { open: "<br/>".to_owned(), tag: None, children: Vec::new() },
  ]
}

/// A manifest that reaches every row, every section and every variant of the
/// IR, so a variant added without a form to print it fails here.
fn every_manifest() -> Manifest {
  let body = every_stmt();
  let component = Component {
    body: body.clone(),
    render: Tmpl::Fragment(every_tmpl()),
    state: vec!["count".to_owned(), "open".to_owned()],
    handlers: vec![
      Handler { event: "click".to_owned(), body: body.clone() },
      Handler { event: "submit".to_owned(), body: Vec::new() },
    ],
  };
  let node = Node {
    id: 0,
    module: "shell#document".to_owned(),
    source: Some("index".to_owned()),
    deferred: true,
    fallback: Some("routes/loading.tsx#default".to_owned()),
    error: Some("routes/error.tsx#default".to_owned()),
    cache_key: Some("page".to_owned()),
    children: vec![Child {
      slot: "content".to_owned(),
      node: Node {
        id: 1,
        module: "routes/page.tsx#default".to_owned(),
        source: None,
        deferred: false,
        fallback: None,
        error: None,
        cache_key: None,
        children: Vec::new(),
        keep: vec!["modal".to_owned()],
      },
    }],
    keep: Vec::new(),
  };
  let mut consts = snapfire_fsr_ir::ast::Consts::new();
  for (i, expr) in every_expr().into_iter().enumerate() {
    consts.insert(format!("src/x.ts#c{i}"), expr);
  }
  Manifest {
    version: 2,
    routes: vec![RouteEntry { pattern: "/".to_owned(), plan: node.clone() }],
    sources: vec![
      SourceEntry {
        id: "index".to_owned(),
        owner: RowOwner::Lowered,
        module: Some("routes/page.loader.ts".to_owned()),
        export: Some("load".to_owned()),
        reason: Some("declared".to_owned()),
        body: Some(body.clone()),
        meta: Some(body.clone()),
        store: Some(body.clone()),
      },
      SourceEntry::rust("declared"),
    ],
    actions: vec![
      ActionEntry {
        id: "cart.add".to_owned(),
        owner: RowOwner::Lowered,
        module: Some("routes/actions.ts".to_owned()),
        export: Some("add".to_owned()),
        input: Some("AddInput".to_owned()),
        reason: None,
        body: Some(body.clone()),
      },
      ActionEntry::rust("cart.clear"),
    ],
    components: vec![ComponentEntry { module: "routes/page.tsx#default".to_owned(), body: component }],
    consts,
    not_found: Some(node.clone()),
    handlers: vec![
      HandlerEntry::lowered("route.GET", "GET", "/api", "routes/route.ts", body.clone()),
      HandlerEntry::rust("route.POST", "POST", "/api"),
    ],
    middleware: Some(body),
    intercepts: vec![RouteEntry { pattern: "/modal".to_owned(), plan: node }],
  }
}

#[test]
fn every_variant_survives_the_round_trip() {
  let manifest = every_manifest();
  let text = manifest.to_sexpr();
  let back = Manifest::from_sexpr(&text).unwrap_or_else(|e| panic!("{e}\n{text}"));
  assert_eq!(manifest, back);
  assert_eq!(text, back.to_sexpr(), "printing is stable");
}

/// `NaN` is the one value equality cannot check, so it is checked by hand.
#[test]
fn a_nan_literal_survives_the_round_trip() {
  let mut manifest = every_manifest();
  manifest.consts.clear();
  manifest.consts.insert("src/x.ts#nan".to_owned(), Expr::Lit(Lit::Float(f64::NAN)));
  let back = Manifest::from_sexpr(&manifest.to_sexpr()).expect("reads");
  match back.consts.get("src/x.ts#nan") {
    Some(Expr::Lit(Lit::Float(f))) => assert!(f.is_nan()),
    other => panic!("{other:?}"),
  }
}

#[test]
fn a_plan_without_a_version_is_refused() {
  let e = Manifest::from_sexpr("(route / (node 0 shell#document))").unwrap_err();
  assert!(e.to_string().contains("(plan"), "{e}");
}

#[test]
fn a_json_plan_still_reads_through_from_text() {
  let mut manifest = every_manifest();
  manifest.consts.retain(|_, expr| !matches!(expr, Expr::Lit(Lit::Float(f)) if !f.is_finite()));
  let back = Manifest::from_text(&manifest.to_json()).expect("json still reads");
  assert_eq!(manifest, back);
  let back = Manifest::from_text(&manifest.to_sexpr()).expect("sexp reads");
  assert_eq!(manifest, back);
}

/// JSON has no term for a non-finite float, so `plan.json` wrote `null` and
/// read back a type error. `plan.sexp` carries them.
#[test]
fn a_non_finite_float_survives_the_sexp_a_json_plan_could_not_carry() {
  let mut manifest = every_manifest();
  manifest.consts.clear();
  manifest.consts.insert("src/x.ts#inf".to_owned(), Expr::Lit(Lit::Float(f64::INFINITY)));
  assert_eq!(manifest, Manifest::from_sexpr(&manifest.to_sexpr()).expect("sexp reads"));
  assert!(Manifest::from_json(&manifest.to_json()).is_err());
}

/// The example plans are build output, so a clean checkout has none. Each one
/// that is there must survive the round trip byte for byte, whichever form the
/// build that wrote it used.
#[test]
fn example_plans_round_trip() {
  let root = concat!(env!("CARGO_MANIFEST_DIR"), "/../examples");
  for entry in std::fs::read_dir(root).expect("the examples directory") {
    let dir = entry.expect("a directory entry").path().join("app/generated");
    for file in ["plan.sexp", "plan.json"] {
      let path = dir.join(file);
      let Ok(source) = std::fs::read_to_string(&path) else { continue };
      let name = path.display();
      let manifest = Manifest::from_text(&source).unwrap_or_else(|e| panic!("{name}: {e}"));
      let text = manifest.to_sexpr();
      let back = Manifest::from_sexpr(&text).unwrap_or_else(|e| panic!("{name}: {e}"));
      assert_eq!(manifest, back, "{name} did not survive the round trip");
      assert_eq!(text, back.to_sexpr(), "{name} does not print the same twice");
      if file == "plan.sexp" {
        assert_eq!(source, text, "{name} is not what the printer writes");
      }
    }
  }
}

// The expression and template layers are generated exhaustively in
// `snapfire_fsr_ir`'s own suite; here the generators stay small and the
// pressure is on the rows, the sections and the node tree.

use proptest::prelude::*;
use proptest::strategy::BoxedStrategy;

fn text() -> BoxedStrategy<String> {
  prop_oneof![
    2 => "[a-zA-Z0-9$#./:@_-]{0,12}",
    1 => "(?s).{0,8}",
    1 => prop_oneof![
      Just(String::new()),
      Just("nil".to_owned()),
      Just("a b".to_owned()),
      Just("(".to_owned()),
      Just("|".to_owned()),
      Just("\"".to_owned()),
      Just(";".to_owned()),
      Just("\n".to_owned()),
      Just("é🌍".to_owned()),
    ],
  ]
  .boxed()
}

fn small_expr() -> BoxedStrategy<Expr> {
  prop_oneof![
    text().prop_map(|s| Expr::Lit(Lit::Str(s))),
    any::<i128>().prop_map(|n| Expr::Lit(Lit::Int(n))),
    any::<f64>().prop_map(|f| Expr::Lit(Lit::Float(f))),
    text().prop_map(Expr::Var),
    text().prop_map(Expr::Param),
    Just(Expr::Locale),
  ]
  .boxed()
}

fn small_body() -> BoxedStrategy<Vec<Stmt>> {
  prop::collection::vec(
    prop_oneof![
      (text(), small_expr()).prop_map(|(name, expr)| Stmt::Let { name, expr }),
      small_expr().prop_map(Stmt::Return),
      (small_expr(), text(), text()).prop_map(|(cond, kind, message)| Stmt::Guard { cond, kind, message }),
    ],
    0..3,
  )
  .boxed()
}

fn owner() -> BoxedStrategy<RowOwner> {
  prop_oneof![Just(RowOwner::Lowered), Just(RowOwner::Engine), Just(RowOwner::Rust)].boxed()
}

fn node() -> BoxedStrategy<Node> {
  let leaf = (
    any::<u32>(),
    text(),
    prop::option::of(text()),
    any::<bool>(),
    prop::option::of(text()),
    prop::option::of(text()),
    prop::option::of(text()),
    prop::collection::vec(text(), 0..3),
  )
    .prop_map(|(id, module, source, deferred, fallback, error, cache_key, keep)| Node {
      id,
      module,
      source,
      deferred,
      fallback,
      error,
      cache_key,
      children: Vec::new(),
      keep,
    });
  leaf
    .prop_recursive(3, 12, 3, |inner| {
      (inner.clone(), prop::collection::vec((text(), inner), 0..3)).prop_map(|(mut parent, kids)| {
        parent.children = kids.into_iter().map(|(slot, node)| Child { slot, node }).collect();
        parent
      })
    })
    .boxed()
}

fn manifest() -> BoxedStrategy<Manifest> {
  (
    prop::collection::vec((text(), node()), 0..3),
    prop::collection::vec(
      (text(), owner(), prop::option::of(text()), prop::option::of(text()), prop::option::of(text()),
       prop::option::of(small_body()), prop::option::of(small_body()), prop::option::of(small_body())),
      0..3,
    ),
    prop::collection::vec(
      (text(), owner(), prop::option::of(text()), prop::option::of(text()), prop::option::of(text()), prop::option::of(small_body())),
      0..3,
    ),
    prop::collection::vec(
      (text(), text(), text(), owner(), prop::option::of(text()), prop::option::of(text()), prop::option::of(small_body())),
      0..3,
    ),
    prop::option::of(node()),
    prop::option::of(small_body()),
    prop::collection::vec((text(), node()), 0..2),
    prop::collection::vec((text(), small_expr()), 0..3),
  )
    .prop_map(|(routes, sources, actions, handlers, not_found, middleware, intercepts, consts)| Manifest {
      version: 2,
      routes: routes.into_iter().map(|(pattern, plan)| RouteEntry { pattern, plan }).collect(),
      sources: sources
        .into_iter()
        .map(|(id, owner, module, export, reason, body, meta, store)| SourceEntry {
          // a `lowered` row must carry a body, which is what the reader checks
          owner: if body.is_none() && owner == RowOwner::Lowered { RowOwner::Rust } else { owner },
          id, module, export, reason, body, meta, store,
        })
        .collect(),
      actions: actions
        .into_iter()
        .map(|(id, owner, module, export, input, body)| ActionEntry {
          owner: if body.is_none() && owner == RowOwner::Lowered { RowOwner::Rust } else { owner },
          id, module, export, input, reason: None, body,
        })
        .collect(),
      components: Vec::new(),
      consts: consts.into_iter().collect(),
      not_found,
      handlers: handlers
        .into_iter()
        .map(|(id, method, pattern, owner, module, input, body)| HandlerEntry {
          id, method, pattern, owner, module, input, reason: None, body,
        })
        .collect(),
      middleware,
      intercepts: intercepts.into_iter().map(|(pattern, plan)| RouteEntry { pattern, plan }).collect(),
    })
    .boxed()
}

proptest! {
  #![proptest_config(ProptestConfig { cases: 384, ..ProptestConfig::default() })]

  /// A generated manifest survives the text, and printing it twice is stable.
  #[test]
  fn a_manifest_survives_the_text(m in manifest()) {
    let text = m.to_sexpr();
    let back = Manifest::from_sexpr(&text).map_err(|e| TestCaseError::fail(format!("{e}\n{text}")))?;
    prop_assert_eq!(&back, &m);
    prop_assert_eq!(back.to_sexpr(), text);
  }

  /// Whatever the text is, reading a plan is an answer or an error.
  #[test]
  fn reading_arbitrary_text_as_a_plan_never_panics(src in "(?s).{0,400}") {
    let _ = Manifest::from_sexpr(&src);
    let _ = Manifest::from_text(&src);
  }

  #[test]
  fn plan_shaped_noise_never_panics(src in r#"[()"|; \na-z0-9]{0,200}"#) {
    let _ = Manifest::from_sexpr(&src);
  }

  /// One byte changed in a real plan leaves it readable or refused, never a panic.
  #[test]
  fn a_mutated_plan_never_panics(at in 0usize..65536, byte in prop::sample::select(
    vec![b'(', b')', b'"', b'|', b'\\', b';', b' ', b'\n', b'a', b'0']
  ), op in 0u8..3) {
    let text = every_manifest().to_sexpr();
    let mut bytes = text.into_bytes();
    let at = at % bytes.len();
    match op {
      0 => bytes[at] = byte,
      1 => { bytes.remove(at); }
      _ => bytes.insert(at, byte),
    }
    let _ = Manifest::from_sexpr(&String::from_utf8_lossy(&bytes));
  }

  #[test]
  fn a_truncated_plan_never_panics(cut in 0usize..65536) {
    let text = every_manifest().to_sexpr();
    let src: String = text.chars().take(cut % text.chars().count()).collect();
    let _ = Manifest::from_sexpr(&src);
  }
}

/// Every way a plan file can be malformed at the manifest layer.
#[test]
fn malformed_plans_are_refused() {
  let cases: &[(&str, &str)] = &[
    ("(plan 2)\n(nope)", "not a plan form"),
    ("(plan 2)\n(route /)", "`(route pattern node)`"),
    ("(plan 2)\n(route / (nope 0 m))", "a plan node is `(node"),
    ("(plan 2)\n(route / (node 0))", "needs an id and a module"),
    ("(plan 2)\n(route / (node x m))", "a node id is a number"),
    ("(plan 2)\n(route / (node 0 m (nope)))", "not a node section"),
    ("(plan 2)\n(route / (node 0 m (slot a)))", "`(slot name node)`"),
    ("(plan 2)\n(not-found)", "`not-found` takes one node"),
    ("(plan 2)\n(source id)", "needs an id and an owner"),
    ("(plan 2)\n(source id nope)", "not a row owner"),
    ("(plan 2)\n(source id rust (nope x))", "not a source section"),
    ("(plan 2)\n(action id rust (nope x))", "not an action section"),
    ("(plan 2)\n(handler id GET /)", "an id, a method, a pattern and an owner"),
    ("(plan 2)\n(handler id GET / rust (nope x))", "not a handler section"),
    ("(plan 2)\n(const only)", "`(const name expr)`"),
    ("(plan 2)\n(component)", "needs a module id"),
    ("(plan x)", "a plan version is a number"),
    ("(route / (node 0 m))", "no `(plan <version>)`"),
    ("(plan 99)", "version 99"),
    ("(plan 0)", "version 0"),
  ];
  for (src, want) in cases {
    let err = Manifest::from_sexpr(src).expect_err(&format!("`{src}` must not read")).to_string();
    assert!(err.contains(want), "`{src}`: wanted `{want}`, got `{err}`");
  }
}

/// A `lowered` row with no body is refused, the way the JSON reader refuses it.
#[test]
fn a_lowered_row_without_a_body_is_refused() {
  let err = Manifest::from_sexpr("(plan 2)\n(source index lowered)").unwrap_err().to_string();
  assert!(err.contains("`index`") && err.contains("no body"), "{err}");
  let err = Manifest::from_sexpr("(plan 2)\n(action cart.add lowered)").unwrap_err().to_string();
  assert!(err.contains("`cart.add`") && err.contains("no body"), "{err}");
}

/// The manifest layer's own term errors: a head that is not a symbol, a name
/// that is not one, and a section with nothing after its name.
#[test]
fn malformed_plan_terms_are_refused() {
  let cases: &[(&str, &str)] = &[
    ("(plan 2)\n((a) b)", "a form starts with a symbol"),
    ("(plan 2)\n(source (a) rust)", "expected a name"),
    ("(plan 2)\n(route / (node 0 m (source)))", "this node section needs a value"),
    ("(plan 2)\n(source id rust (module))", "`module` needs a value"),
    ("(plan)", "`plan` needs a value"),
  ];
  for (src, want) in cases {
    let err = Manifest::from_sexpr(src).expect_err(&format!("`{src}` must not read")).to_string();
    assert!(err.contains(want), "`{src}`: wanted `{want}`, got `{err}`");
  }
}
