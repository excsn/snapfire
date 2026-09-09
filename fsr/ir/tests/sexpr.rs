//! The syntax layer under generated input: every term the printer can write
//! must read back as itself; no input at all may panic the parser.

use proptest::prelude::*;
use proptest::strategy::BoxedStrategy;
use snapfire_fsr_ir::ast::{
  ArithOp, Body, Builtin, CompareOp, Component, Entry, Expr, Handler, Lit, LogicOp, Stmt, Tmpl,
};
use snapfire_fsr_ir::sexpr::{
  component_from_sx, component_to_sx, expr_from_sx, expr_to_sx, parse, print, stmt_from_sx,
  stmt_to_sx, tmpl_from_sx, tmpl_to_sx, Sx,
};

/// Names and text as awkward as the format allows: empty, whitespace, every
/// delimiter, the atoms that mean something else bare, plus arbitrary unicode.
fn text() -> BoxedStrategy<String> {
  prop_oneof![
    2 => "[a-zA-Z$#.:@/_-]{0,10}",
    1 => "(?s).{0,10}",
    1 => prop_oneof![
      Just(String::new()),
      Just("nil".to_owned()),
      Just("#t".to_owned()),
      Just("#f".to_owned()),
      Just("42".to_owned()),
      Just("-0.0".to_owned()),
      Just("inf".to_owned()),
      Just("NaN".to_owned()),
      Just("1e400".to_owned()),
      Just("(".to_owned()),
      Just(")".to_owned()),
      Just("{".to_owned()),
      Just("}".to_owned()),
      Just("|".to_owned()),
      Just("\"".to_owned()),
      Just("\\".to_owned()),
      Just(";".to_owned()),
      Just(" ".to_owned()),
      Just("a b".to_owned()),
      Just("\n\t\r".to_owned()),
      Just("...".to_owned()),
      Just("=".to_owned()),
      Just(":".to_owned()),
      Just("@".to_owned()),
      Just("\u{0}".to_owned()),
      Just("é🌍".to_owned()),
    ],
  ]
  .boxed()
}

fn sx() -> BoxedStrategy<Sx> {
  let leaf = prop_oneof![text().prop_map(Sx::Sym), text().prop_map(Sx::Str)];
  leaf.prop_recursive(5, 48, 4, |inner| {
    prop_oneof![
      prop::collection::vec(inner.clone(), 0..4).prop_map(Sx::List),
      inner.prop_map(|x| Sx::Interp(Box::new(x))),
    ]
  })
  .boxed()
}

fn lit() -> BoxedStrategy<Lit> {
  prop_oneof![
    Just(Lit::Null),
    any::<bool>().prop_map(Lit::Bool),
    any::<i128>().prop_map(Lit::Int),
    any::<f64>().prop_map(Lit::Float),
    text().prop_map(Lit::Str),
  ]
  .boxed()
}

fn expr() -> BoxedStrategy<Expr> {
  let leaf = prop_oneof![
    lit().prop_map(Expr::Lit),
    text().prop_map(Expr::Var),
    text().prop_map(Expr::Param),
    text().prop_map(Expr::Query),
    text().prop_map(Expr::Session),
    text().prop_map(Expr::Store),
    text().prop_map(Expr::Const),
    prop::collection::vec(text(), 0..3).prop_map(Expr::Identity),
    Just(Expr::Locale),
    Just(Expr::Path),
    Just(Expr::Host),
    Just(Expr::Input),
    Just(Expr::Now),
  ];
  leaf.prop_recursive(4, 64, 3, |inner| {
    let entry = entry_of(inner.clone());
    prop_oneof![
      prop::collection::vec(entry.clone(), 0..3).prop_map(Expr::Object),
      prop::collection::vec(entry, 0..3).prop_map(Expr::Array),
      (inner.clone(), text()).prop_map(|(e, n)| Expr::Field(Box::new(e), n)),
      (inner.clone(), inner.clone()).prop_map(|(a, b)| Expr::Index(Box::new(a), Box::new(b))),
      (arith(), inner.clone(), inner.clone()).prop_map(|(o, a, b)| Expr::Arith(o, Box::new(a), Box::new(b))),
      (compare(), inner.clone(), inner.clone()).prop_map(|(o, a, b)| Expr::Compare(o, Box::new(a), Box::new(b))),
      (logic(), inner.clone(), inner.clone()).prop_map(|(o, a, b)| Expr::Logic(o, Box::new(a), Box::new(b))),
      inner.clone().prop_map(|e| Expr::Not(Box::new(e))),
      (inner.clone(), inner.clone()).prop_map(|(a, b)| Expr::Coalesce(Box::new(a), Box::new(b))),
      (inner.clone(), inner.clone(), inner.clone()).prop_map(|(a, b, c)| Expr::Ternary(Box::new(a), Box::new(b), Box::new(c))),
      prop::collection::vec(inner.clone(), 0..3).prop_map(Expr::Template),
      (text(), text(), prop::collection::vec((text(), inner.clone()), 0..3))
        .prop_map(|(service, method, args)| Expr::Call { service, method, args }),
      (text(), text(), prop::collection::vec((text(), inner.clone()), 0..2), any::<bool>())
        .prop_map(|(module, method, args, sync)| Expr::NativeCall { module, method, args, sync }),
      (prop::collection::vec(text(), 0..3), inner.clone())
        .prop_map(|(params, body)| Expr::Lambda { params, body: Box::new(body) }),
      (inner.clone(), prop::collection::vec(inner.clone(), 0..3))
        .prop_map(|(f, args)| Expr::Apply { f: Box::new(f), args }),
      (builtin(), prop::collection::vec(inner.clone(), 0..3))
        .prop_map(|(name, args)| Expr::Builtin { name, args }),
      (text(), text(), prop::collection::vec(inner.clone(), 0..3))
        .prop_map(|(module, name, args)| Expr::Ext { module, name, args }),
      (inner.clone(), inner.clone()).prop_map(|(a, b)| Expr::Map(Box::new(a), Box::new(b))),
      (inner.clone(), inner.clone()).prop_map(|(a, b)| Expr::Filter(Box::new(a), Box::new(b))),
      (inner.clone(), inner.clone(), inner.clone()).prop_map(|(a, b, c)| Expr::Reduce(Box::new(a), Box::new(b), Box::new(c))),
      (inner.clone(), inner.clone()).prop_map(|(a, b)| Expr::Find(Box::new(a), Box::new(b))),
      (inner.clone(), inner.clone()).prop_map(|(a, b)| Expr::FindIndex(Box::new(a), Box::new(b))),
      (inner.clone(), inner.clone()).prop_map(|(a, b)| Expr::Some(Box::new(a), Box::new(b))),
      (inner.clone(), inner.clone()).prop_map(|(a, b)| Expr::Every(Box::new(a), Box::new(b))),
      inner.clone().prop_map(|e| Expr::Entries(Box::new(e))),
      inner.clone().prop_map(|e| Expr::Keys(Box::new(e))),
      inner.clone().prop_map(|e| Expr::Values(Box::new(e))),
      inner.clone().prop_map(|e| Expr::Length(Box::new(e))),
      inner.clone().prop_map(|e| Expr::Str(Box::new(e))),
      inner.clone().prop_map(|e| Expr::Num(Box::new(e))),
      inner.clone().prop_map(|e| Expr::BigInt(Box::new(e))),
      (any::<u32>(), inner).prop_map(|(id, e)| Expr::Hoist { id, expr: Box::new(e) }),
    ]
  })
  .boxed()
}

fn entry_of(e: BoxedStrategy<Expr>) -> BoxedStrategy<Entry> {
  prop_oneof![
    (text(), e.clone()).prop_map(|(n, v)| Entry::Field(n, v)),
    e.clone().prop_map(Entry::Item),
    e.clone().prop_map(Entry::Spread),
    (e.clone(), e).prop_map(|(k, v)| Entry::Computed(k, v)),
  ]
  .boxed()
}

fn arith() -> BoxedStrategy<ArithOp> {
  prop_oneof![Just(ArithOp::Add), Just(ArithOp::Sub), Just(ArithOp::Mul), Just(ArithOp::Div), Just(ArithOp::Rem)]
  .boxed()
}

fn compare() -> BoxedStrategy<CompareOp> {
  prop_oneof![
    Just(CompareOp::Eq), Just(CompareOp::Ne), Just(CompareOp::Lt),
    Just(CompareOp::Le), Just(CompareOp::Gt), Just(CompareOp::Ge)
  ]
  .boxed()
}

fn logic() -> BoxedStrategy<LogicOp> {
  prop_oneof![Just(LogicOp::And), Just(LogicOp::Or)]
  .boxed()
}

fn builtin() -> BoxedStrategy<Builtin> {
  prop_oneof![
    Just(Builtin::Round), Just(Builtin::Floor), Just(Builtin::Ceil), Just(Builtin::Abs),
    Just(Builtin::Min), Just(Builtin::Max), Just(Builtin::ToFixed), Just(Builtin::Repeat),
    Just(Builtin::Join), Just(Builtin::Trim), Just(Builtin::Upper), Just(Builtin::Lower),
    Just(Builtin::Includes), Just(Builtin::EncodeUriComponent), Just(Builtin::LocaleNumber),
    Just(Builtin::Range), Just(Builtin::Omit)
  ]
  .boxed()
}

fn tmpl() -> BoxedStrategy<Tmpl> {
  let leaf = prop_oneof![
    text().prop_map(Tmpl::Text),
    expr().prop_map(Tmpl::Expr),
    text().prop_map(Tmpl::Slot),
  ];
  leaf.prop_recursive(4, 40, 3, |inner| {
    prop_oneof![
      (text(), prop::collection::vec(entry_of(expr()), 0..3), prop::collection::vec(inner.clone(), 0..3))
        .prop_map(|(tag, attrs, children)| Tmpl::Element { tag, attrs, children }),
      prop::collection::vec(inner.clone(), 0..3).prop_map(Tmpl::Fragment),
      (expr(), inner.clone(), prop::option::of(inner.clone()))
        .prop_map(|(cond, then, r#else)| Tmpl::If { cond, then: Box::new(then), r#else: r#else.map(Box::new) }),
      (expr(), prop::collection::vec(text(), 0..3), inner.clone())
        .prop_map(|(over, params, body)| Tmpl::For { over, params, body: Box::new(body) }),
      (text(), expr(), inner.clone())
        .prop_map(|(name, e, then)| Tmpl::Let { name, expr: e, then: Box::new(then) }),
      (text(), prop::collection::vec(entry_of(expr()), 0..2), prop::collection::vec(inner.clone(), 0..2), any::<u32>())
        .prop_map(|(module, props, children, id)| Tmpl::Component { module, props, children, id }),
      (text(), prop::collection::vec(entry_of(expr()), 0..2), prop::collection::vec(inner.clone(), 0..2),
       prop::option::of(text()), prop::option::of(text()), any::<u32>())
        .prop_map(|(module, props, children, when, mode, id)| Tmpl::Island { module, props, children, when, mode, id }),
      (text(), prop::option::of(text()), prop::collection::vec(inner, 0..2))
        .prop_map(|(open, tag, children)| Tmpl::Baked { open, tag, children }),
    ]
  })
  .boxed()
}

fn stmt() -> BoxedStrategy<Stmt> {
  let leaf = prop_oneof![
    (text(), expr()).prop_map(|(name, e)| Stmt::Let { name, expr: e }),
    expr().prop_map(Stmt::Return),
    expr().prop_map(Stmt::Expr),
    (expr(), text(), text()).prop_map(|(cond, kind, message)| Stmt::Guard { cond, kind, message }),
    (text(), prop::collection::vec(expr(), 0..2), expr())
      .prop_map(|(key, path, value)| Stmt::SessionSet { key, path, value }),
    (text(), prop::collection::vec(expr(), 0..2)).prop_map(|(key, path)| Stmt::SessionDelete { key, path }),
  ];
  leaf.prop_recursive(3, 24, 3, |inner| {
    prop_oneof![
      (expr(), prop::collection::vec(inner.clone(), 0..3), prop::collection::vec(inner.clone(), 0..3))
        .prop_map(|(cond, then, r#else)| Stmt::If { cond, then, r#else }),
      (text(), expr(), prop::collection::vec(inner, 0..3))
        .prop_map(|(name, over, body)| Stmt::ForOf { name, over, body }),
    ]
  })
  .boxed()
}

fn body() -> BoxedStrategy<Body> {
  prop::collection::vec(stmt(), 0..4)
  .boxed()
}

fn component() -> BoxedStrategy<Component> {
  (body(), tmpl(), prop::collection::vec(text(), 0..3),
   prop::collection::vec((text(), body()), 0..3))
    .prop_map(|(body, render, state, handlers)| Component {
      body,
      render,
      state,
      handlers: handlers.into_iter().map(|(event, body)| Handler { event, body }).collect(),
    })
  .boxed()
}

proptest! {
  #![proptest_config(ProptestConfig { cases: 512, ..ProptestConfig::default() })]

  /// The syntax layer: whatever the printer writes, the parser reads back.
  #[test]
  fn a_form_survives_print_and_parse(form in sx()) {
    let text = print(std::slice::from_ref(&form));
    let back = parse(&text).map_err(|e| TestCaseError::fail(format!("{e}\n{text}")))?;
    prop_assert_eq!(&back, &vec![form]);
  }

  /// Several forms in one file keep their order and their boundaries.
  #[test]
  fn a_file_of_forms_survives(forms in prop::collection::vec(sx(), 0..6)) {
    let text = print(&forms);
    prop_assert_eq!(parse(&text).unwrap(), forms);
  }

  /// No input panics the parser, whatever it is.
  #[test]
  fn arbitrary_text_never_panics(src in "(?s).{0,300}") {
    let _ = parse(&src);
  }

  /// Nor does anything built only from the format's own characters, which is
  /// where a parser is most likely to run off the end.
  #[test]
  fn arbitrary_delimiters_never_panic(src in r#"[()\{\}"|;\\ \n\ta1]{0,80}"#) {
    let _ = parse(&src);
  }

  #[test]
  fn an_expression_survives(e in expr()) {
    let sx = expr_to_sx(&e);
    prop_assert_eq!(expr_from_sx(&sx).map_err(|err| TestCaseError::fail(err.to_string()))?, e);
  }

  #[test]
  fn an_expression_survives_the_text(e in expr()) {
    let text = print(std::slice::from_ref(&expr_to_sx(&e)));
    let forms = parse(&text).map_err(|err| TestCaseError::fail(format!("{err}\n{text}")))?;
    prop_assert_eq!(forms.len(), 1);
    prop_assert_eq!(expr_from_sx(&forms[0]).map_err(|err| TestCaseError::fail(err.to_string()))?, e);
  }

  #[test]
  fn a_template_survives(t in tmpl()) {
    let sx = tmpl_to_sx(&t);
    prop_assert_eq!(tmpl_from_sx(&sx).map_err(|err| TestCaseError::fail(err.to_string()))?, t);
  }

  #[test]
  fn a_template_survives_the_text(t in tmpl()) {
    let text = print(std::slice::from_ref(&tmpl_to_sx(&t)));
    let forms = parse(&text).map_err(|err| TestCaseError::fail(format!("{err}\n{text}")))?;
    prop_assert_eq!(tmpl_from_sx(&forms[0]).map_err(|err| TestCaseError::fail(err.to_string()))?, t);
  }

  #[test]
  fn a_statement_survives(s in stmt()) {
    let sx = stmt_to_sx(&s);
    prop_assert_eq!(stmt_from_sx(&sx).map_err(|err| TestCaseError::fail(err.to_string()))?, s);
  }

  #[test]
  fn a_component_survives_the_text(c in component()) {
    let text = print(std::slice::from_ref(&component_to_sx(&c)));
    let forms = parse(&text).map_err(|err| TestCaseError::fail(format!("{err}\n{text}")))?;
    prop_assert_eq!(component_from_sx(&forms[0]).map_err(|err| TestCaseError::fail(err.to_string()))?, c);
  }

  /// Reading an arbitrary tree as a typed term either works or errors; it may
  /// never panic, which is what an index or an unwrap in the reader would do.
  #[test]
  fn reading_an_arbitrary_form_never_panics(form in sx()) {
    let _ = expr_from_sx(&form);
    let _ = tmpl_from_sx(&form);
    let _ = stmt_from_sx(&form);
    let _ = component_from_sx(&form);
  }
}

/// Every way the text can be malformed, with the message it must give. A
/// parser that stops saying which line is a parser nobody can debug.
#[test]
fn malformed_text_is_refused_by_line() {
  let cases: &[(&str, &str)] = &[
    ("(a b", "unclosed `("),
    ("(a (b c)", "unclosed `("),
    (")", "unbalanced `)`"),
    ("}", "unbalanced `}`"),
    ("{a", "unclosed `{`"),
    ("{a b}", "unclosed `{`"),
    ("\"open", "ends inside a quoted term"),
    ("|open", "ends inside a quoted term"),
    ("\"trailing\\", "ends after `\\`"),
  ];
  for (src, want) in cases {
    let err = parse(src).expect_err(&format!("`{src}` must not parse")).to_string();
    assert!(err.contains(want), "`{src}`: wanted `{want}`, got `{err}`");
    assert!(err.starts_with("line "), "`{src}`: no line in `{err}`");
  }
}

/// The line a syntax error names is the line the fault is on, not the first.
#[test]
fn a_syntax_error_names_its_own_line() {
  let err = parse("(a b)\n(c d)\n(e \"open\n").unwrap_err().to_string();
  assert!(err.starts_with("line 3"), "{err}");
  let err = parse("; a comment\n; another\n)").unwrap_err().to_string();
  assert!(err.starts_with("line 3"), "{err}");
}

/// A comment runs to the end of its line and takes no term with it.
#[test]
fn comments_and_whitespace_are_skipped() {
  assert_eq!(parse("; nothing here\n").unwrap(), Vec::new());
  assert_eq!(parse("(a ; trailing\n b)").unwrap(), parse("(a b)").unwrap());
  assert_eq!(parse("  \n\t (a)  \n ").unwrap(), parse("(a)").unwrap());
  assert_eq!(parse("\"; not a comment\"").unwrap(), vec![Sx::Str("; not a comment".to_owned())]);
}

/// A shape error says what it wanted and what it found, for every reader.
#[test]
fn malformed_shapes_are_refused() {
  let cases: &[(&str, &str)] = &[
    ("(nope 1)", "not an expression form"),
    ("(. 1)", "`.` takes 2 terms"),
    ("(+ 1)", "`+` takes 2 terms"),
    ("(if 1 2)", "`if` takes 3 terms"),
    ("(fn 1 2)", "expected a list"),
    ("(call one)", "`call` takes at least 2"),
    ("(hoist x 1)", "expected a number"),
    ("({a} 1)", "an expression form starts with a symbol"),
  ];
  for (src, want) in cases {
    let form = &parse(src).expect("lexes")[0];
    let err = expr_from_sx(form).expect_err(&format!("`{src}` must not read")).to_string();
    assert!(err.contains(want), "`{src}`: wanted `{want}`, got `{err}`");
  }
}

#[test]
fn malformed_templates_and_statements_are_refused() {
  let bad_tmpl: &[(&str, &str)] = &[
    ("(nope)", "not a template form"),
    ("(el div)", "`el` takes at least 2"),
    ("(el div 1)", "expected a list"),
    ("(if a b c d)", "at most one else"),
    ("(comp m 0)", "`comp` takes at least 3"),
    ("(island m 0 nil nil)", "`island` takes at least 5"),
    ("(comp m x ())", "expected a number"),
    ("(baked open nil)", "expected a string"),
  ];
  for (src, want) in bad_tmpl {
    let form = &parse(src).expect("lexes")[0];
    let err = tmpl_from_sx(form).expect_err(&format!("`{src}` must not read")).to_string();
    assert!(err.contains(want), "`{src}`: wanted `{want}`, got `{err}`");
  }
  assert!(tmpl_from_sx(&Sx::Sym("bare".to_owned())).unwrap_err().to_string().contains("text is quoted"));

  let bad_stmt: &[(&str, &str)] = &[
    ("(nope)", "not a statement form"),
    ("(let x)", "`let` takes 2 terms"),
    ("(ret)", "`ret` takes 1 terms"),
    ("(guard a b)", "`guard` takes 3 terms"),
    ("(if a (b) (c) (d))", "at most one else"),
    ("(session-set k v)", "`session-set` takes 3"),
  ];
  for (src, want) in bad_stmt {
    let form = &parse(src).expect("lexes")[0];
    let err = stmt_from_sx(form).expect_err(&format!("`{src}` must not read")).to_string();
    assert!(err.contains(want), "`{src}`: wanted `{want}`, got `{err}`");
  }
}

#[test]
fn malformed_components_are_refused() {
  let cases: &[(&str, &str)] = &[
    ("(nope (render \"x\"))", "a component is `(component"),
    ("(component (state a))", "needs a `(render ...)`"),
    ("(component (nope) (render \"x\"))", "not a component section"),
    ("(component (\"s\") (render \"x\"))", "starts with a symbol"),
    ("(component (render \"a\" \"b\"))", "`render` takes 1 terms"),
    ("(component (on) (render \"x\"))", "`on` takes at least 1"),
  ];
  for (src, want) in cases {
    let form = &parse(src).expect("lexes")[0];
    let err = component_from_sx(form).expect_err(&format!("`{src}` must not read")).to_string();
    assert!(err.contains(want), "`{src}`: wanted `{want}`, got `{err}`");
  }
}

/// `{...}` is a template term and means nothing in an expression.
#[test]
fn an_interpolation_is_refused_in_an_expression() {
  let err = expr_from_sx(&parse("{a}").unwrap()[0]).unwrap_err().to_string();
  assert!(err.contains("belongs in a template"), "{err}");
}

proptest! {
  #![proptest_config(ProptestConfig { cases: 400, ..ProptestConfig::default() })]

  /// Mutating real output: a byte flipped, dropped or inserted in a printed
  /// component must leave the parser erroring or reading, never panicking and
  /// never reading past the end.
  #[test]
  fn a_mutated_component_never_panics(c in component(), at in 0usize..4096, byte in prop::sample::select(
    vec![b'(', b')', b'{', b'}', b'"', b'|', b'\\', b';', b' ', b'\n', b'a', b'0']
  ), op in 0u8..3) {
    let text = print(std::slice::from_ref(&component_to_sx(&c)));
    let mut bytes = text.into_bytes();
    if bytes.is_empty() { return Ok(()); }
    let at = at % bytes.len();
    match op {
      0 => bytes[at] = byte,
      1 => { bytes.remove(at); }
      _ => bytes.insert(at, byte),
    }
    let src = String::from_utf8_lossy(&bytes).into_owned();
    if let Ok(forms) = parse(&src) {
      for form in &forms {
        let _ = component_from_sx(form);
      }
    }
  }

  /// Truncation at every length: the parser must never run off the end.
  #[test]
  fn a_truncated_component_never_panics(c in component(), cut in 0usize..4096) {
    let text = print(std::slice::from_ref(&component_to_sx(&c)));
    let cut = if text.is_empty() { 0 } else { cut % text.len() };
    let src: String = text.chars().take(cut).collect();
    if let Ok(forms) = parse(&src) {
      for form in &forms {
        let _ = component_from_sx(form);
      }
    }
  }
}

/// A form whose head is not a symbol, in every position that dispatches on one.
#[test]
fn a_form_without_a_symbol_head_is_refused() {
  let form = &parse("((a) b)").expect("lexes")[0];
  for err in [
    expr_from_sx(form).unwrap_err().to_string(),
    tmpl_from_sx(form).unwrap_err().to_string(),
    stmt_from_sx(form).unwrap_err().to_string(),
  ] {
    assert!(err.contains("starts with a symbol"), "{err}");
  }
  let err = expr_from_sx(&parse("(obj ((a) 1))").unwrap()[0]).unwrap_err().to_string();
  assert!(err.contains("an entry starts with a symbol"), "{err}");
  let err = component_from_sx(&parse("(component ((a)) (render \"x\"))").unwrap()[0]).unwrap_err().to_string();
  assert!(err.contains("a component section starts with a symbol"), "{err}");
}

/// The terms a reader asks for by kind say which kind they wanted.
#[test]
fn a_term_of_the_wrong_kind_is_refused() {
  let err = expr_from_sx(&parse("(param \"x\")").unwrap()[0]).unwrap_err().to_string();
  assert!(err.contains("expected a symbol, found a string"), "{err}");
  let err = expr_from_sx(&parse("(fn x 1)").unwrap()[0]).unwrap_err().to_string();
  assert!(err.contains("expected a list, found a symbol"), "{err}");
  let err = tmpl_from_sx(&parse("(baked \"open\" 1 \"x\")").unwrap()[0]).unwrap_err().to_string();
  assert!(err.contains("expected a string or `nil`, found a symbol"), "{err}");
  let err = expr_from_sx(&parse("(call a b (x))").unwrap()[0]).unwrap_err().to_string();
  assert!(err.contains("a named argument is `(name expr)`"), "{err}");
}

/// An arity message names the form and both counts.
#[test]
fn an_arity_message_names_the_form_and_the_counts() {
  let err = expr_from_sx(&parse("(. 1)").unwrap()[0]).unwrap_err().to_string();
  assert_eq!(err, "`.` takes 2 terms, found 1");
  let err = expr_from_sx(&parse("(call a)").unwrap()[0]).unwrap_err().to_string();
  assert_eq!(err, "`call` takes at least 2 terms, found 1");
}

/// A file that ends where a term was expected says so rather than indexing past it.
#[test]
fn a_file_that_ends_inside_a_term_is_refused() {
  let err = parse("{").unwrap_err().to_string();
  assert!(err.contains("ends inside a form"), "{err}");
}

/// An escape carries the character after it, whole. `\é` is `é`, not the first
/// byte of one: the printer never writes that sequence, so only hand-written
/// or damaged input reaches it.
#[test]
fn an_escape_carries_a_whole_character() {
  for (src, want) in [
    (r#""\é""#, "é"),
    (r#""\🌍""#, "🌍"),
    (r#""\q""#, "q"),
    (r#""\\""#, "\\"),
    (r#""\"""#, "\""),
    (r#""\n""#, "\n"),
    (r#""\t""#, "\t"),
    (r#""\r""#, "\r"),
  ] {
    assert_eq!(parse(src).unwrap_or_else(|e| panic!("{src}: {e}")), vec![Sx::Str(want.to_owned())]);
  }
  assert_eq!(parse(r#"|\é|"#).unwrap(), vec![Sx::Sym("é".to_owned())]);
}

proptest! {
  #![proptest_config(ProptestConfig { cases: 512, ..ProptestConfig::default() })]

  /// Any character after a backslash comes back as itself, bar the three the
  /// format spells.
  #[test]
  fn any_escaped_character_comes_back_whole(c in any::<char>()) {
    prop_assume!(!matches!(c, 'n' | 'r' | 't'));
    let src = format!("\"\\{c}\"");
    prop_assert_eq!(parse(&src).map_err(|e| TestCaseError::fail(e.to_string()))?, vec![Sx::Str(c.to_string())]);
  }
}

// Everything above generates from the printer's output, which is a strictly
// smaller language than the parser accepts: the printer escapes four
// characters, quotes a symbol only when it must and never writes a comment, so
// a round-trip property can never reach the rest of what a hand-written or
// generated file may contain. These generate the text instead.

/// One valid spelling of `sx`, chosen among the several the grammar allows:
/// a symbol may be bare or `|quoted|`, any character inside a quoted term may
/// be written with a backslash, while whitespace may be any run of blanks or
/// comments.
fn spell(sx: &Sx, rng: &mut impl RngChoice, out: &mut String) {
  match sx {
    Sx::Sym(s) => {
      if s.is_empty() || rng.yes() || !bare_ok(s) {
        out.push('|');
        escape_into(s, '|', rng, out);
        out.push('|');
      } else {
        out.push_str(s);
      }
    }
    Sx::Str(s) => {
      out.push('"');
      escape_into(s, '"', rng, out);
      out.push('"');
    }
    Sx::Interp(inner) => {
      out.push('{');
      spell(inner, rng, out);
      out.push('}');
    }
    Sx::List(items) => {
      out.push('(');
      for (i, item) in items.iter().enumerate() {
        if i > 0 {
          gap(rng, out);
        }
        spell(item, rng, out);
      }
      out.push(')');
    }
  }
}

fn bare_ok(s: &str) -> bool {
  !s.is_empty() && !s.bytes().any(|b| b.is_ascii_whitespace() || b"()\"|;{}\\".contains(&b))
}

/// A character may be written plainly or behind a backslash; `n`, `r` and `t`
/// are the three the format spells, so those stay plain.
fn escape_into(s: &str, quote: char, rng: &mut impl RngChoice, out: &mut String) {
  for c in s.chars() {
    match c {
      '\\' => out.push_str("\\\\"),
      '\n' => out.push_str("\\n"),
      '\r' => out.push_str("\\r"),
      '\t' => out.push_str("\\t"),
      c if c == quote => {
        out.push('\\');
        out.push(c);
      }
      'n' | 'r' | 't' => out.push(c),
      c if rng.yes() => {
        out.push('\\');
        out.push(c);
      }
      c => out.push(c),
    }
  }
}

/// Whatever may sit between two terms: blanks, newlines and comments.
fn gap(rng: &mut impl RngChoice, out: &mut String) {
  match rng.pick(4) {
    0 => out.push(' '),
    1 => out.push_str("  \t "),
    2 => out.push_str("\n  "),
    _ => out.push_str(" ; a comment\n  "),
  }
}

trait RngChoice {
  fn pick(&mut self, n: usize) -> usize;
  fn yes(&mut self) -> bool {
    self.pick(2) == 0
  }
}

/// A reproducible stream taken from the case proptest generated, so a failure
/// shrinks with the rest of the input rather than drifting.
struct Bits {
  bytes: Vec<u8>,
  at: usize,
}

impl RngChoice for Bits {
  fn pick(&mut self, n: usize) -> usize {
    if self.bytes.is_empty() {
      return 0;
    }
    let b = self.bytes[self.at % self.bytes.len()];
    self.at += 1;
    b as usize % n
  }
}

proptest! {
  #![proptest_config(ProptestConfig { cases: 512, ..ProptestConfig::default() })]

  /// Any valid spelling of a form parses to that form. This reaches the
  /// escapes, the quoting and the comments the printer never writes.
  #[test]
  fn any_spelling_of_a_form_parses_to_it(form in sx(), bytes in prop::collection::vec(any::<u8>(), 1..32)) {
    let mut rng = Bits { bytes, at: 0 };
    let mut text = String::new();
    spell(&form, &mut rng, &mut text);
    let parsed = parse(&text).map_err(|e| TestCaseError::fail(format!("{e}\n{text}")))?;
    prop_assert_eq!(&parsed, &vec![form], "spelled as: {}", text);
  }

  /// Reading text and printing it gives text that reads the same: the printer
  /// normalises a spelling, it never changes what the term is.
  #[test]
  fn printing_a_parsed_form_is_idempotent(form in sx(), bytes in prop::collection::vec(any::<u8>(), 1..32)) {
    let mut rng = Bits { bytes, at: 0 };
    let mut text = String::new();
    spell(&form, &mut rng, &mut text);
    let once = parse(&text).map_err(|e| TestCaseError::fail(e.to_string()))?;
    let twice = parse(&print(&once)).map_err(|e| TestCaseError::fail(e.to_string()))?;
    prop_assert_eq!(once, twice);
  }
}

/// Nesting is bounded. `form` recurses, so without a limit a file of nothing
/// but `(` overflows the stack, and a stack overflow aborts the process: a
/// corrupt plan file would take the host down at boot with no error to report.
/// The deepest tree any real plan has reached is 23.
#[test]
fn a_deeply_nested_file_is_refused_rather_than_crashing() {
  for n in [300usize, 10_000, 200_000] {
    let err = parse(&"(".repeat(n)).expect_err("must not parse").to_string();
    assert!(err.contains("nested deeper than"), "{n}: {err}");
  }
  let err = parse(&"{".repeat(10_000)).expect_err("must not parse").to_string();
  assert!(err.contains("nested deeper than"), "{err}");
}

/// The bound is above anything a plan reaches and below anything that hurts.
#[test]
fn nesting_up_to_the_bound_still_reads() {
  let deep = format!("{}{}", "(".repeat(255), ")".repeat(255));
  assert!(parse(&deep).is_ok(), "255 deep must read");
  let over = format!("{}{}", "(".repeat(257), ")".repeat(257));
  assert!(parse(&over).is_err(), "257 deep must not");
}
