//! The five `shopping_react_ts` bodies as TypeScript, lowered and compared to
//! the IR the interpreter's own tests hand-write.

use snapfire_fsr_ir::ast::{ArithOp, CompareOp, Entry, Lit, Stmt};
use snapfire_fsr_ir::Expr;
use snapfire_fsr_lower::{lower_actions, lower_loader, LowerError, Residue};

const CATALOG: &str = r#"
import type { Ctx } from "../../generated/ctx";

export async function load({ params, services }: Ctx<"/">) {
  return { products: await services.shopping.listProducts({ tag: params.tag }) };
}
"#;

const PRODUCT: &str = r#"
export async function load({ params, services }: Ctx<"/product/{id}">) {
  return { product: await services.shopping.getProduct({ id: BigInt(params.id) }) };
}
"#;

const CART: &str = r#"
export async function load({ session, services }: Ctx) {
  const catalog = await services.shopping.listProducts({});
  const lines = catalog
    .filter((p) => session.cart[String(p.id)])
    .map((p) => ({ ...p, quantity: session.cart[String(p.id)] }));
  return { lines };
}
"#;

const ACTIONS: &str = r#"
import { action, fail } from "@snapfire/fsr";
import type { AddToCart } from "../../schemas/cart";

export const addToCart = action<AddToCart>(async ({ input, session }) => {
  const key = String(input.product_id);
  const wanted = (session.cart[key] ?? 0n) + input.quantity;
  if (wanted <= 0n) delete session.cart[key];
  else session.cart[key] = wanted;
  return { lines: session.cart };
});

export const checkout = action(async ({ session, services }) => {
  const lines = Object.entries(session.cart).map(([id, quantity]) => ({ product_id: BigInt(id), quantity }));
  if (lines.length === 0) fail("invalid", "the cart is empty");
  const order = await services.shopping.placeOrder({ lines });
  session.cart = {};
  return order;
});
"#;

#[test]
fn the_catalog_loader_lowers_to_one_call() {
  let body = lower_loader("page.loader.ts", CATALOG).unwrap();
  assert_eq!(
    body,
    vec![Stmt::Return(Expr::object(vec![(
      "products",
      Expr::call("shopping", "listProducts", vec![("tag", Expr::Param("tag".into()))]),
    )]))]
  );
}

#[test]
fn the_product_loader_coerces_the_param() {
  let body = lower_loader("page.loader.ts", PRODUCT).unwrap();
  assert_eq!(
    body,
    vec![Stmt::Return(Expr::object(vec![(
      "product",
      Expr::call("shopping", "getProduct", vec![("id", Expr::BigInt(Box::new(Expr::Param("id".into()))))]),
    )]))]
  );
}

#[test]
fn the_cart_loader_lowers_its_join() {
  let held = || Expr::Session("cart".into()).index(Expr::Str(Box::new(Expr::var("p").field("id"))));
  let body = lower_loader("page.loader.ts", CART).unwrap();
  assert_eq!(
    body,
    vec![
      Stmt::Let { name: "catalog".into(), expr: Expr::call("shopping", "listProducts", vec![]) },
      Stmt::Let {
        name: "lines".into(),
        expr: Expr::Map(
          Box::new(Expr::Filter(Box::new(Expr::var("catalog")), Box::new(Expr::lambda(&["p"], held())))),
          Box::new(Expr::lambda(
            &["p"],
            Expr::Object(vec![Entry::Spread(Expr::var("p")), Entry::Field("quantity".into(), held())]),
          )),
        ),
      },
      Stmt::Return(Expr::object(vec![("lines", Expr::var("lines"))])),
    ]
  );
}

#[test]
fn both_actions_lower_with_their_input_types() {
  let actions = lower_actions("actions.ts", ACTIONS).unwrap();
  assert_eq!(actions.len(), 2);

  let add = &actions[0];
  assert_eq!(add.export, "addToCart");
  assert_eq!(add.input.as_deref(), Some("AddToCart"));
  assert_eq!(
    add.body,
    vec![
      Stmt::Let { name: "key".into(), expr: Expr::Str(Box::new(Expr::Input.field("product_id"))) },
      Stmt::Let {
        name: "wanted".into(),
        expr: Expr::Arith(
          ArithOp::Add,
          Box::new(Expr::Coalesce(
            Box::new(Expr::Session("cart".into()).index(Expr::var("key"))),
            Box::new(Expr::lit_int(0)),
          )),
          Box::new(Expr::Input.field("quantity")),
        ),
      },
      Stmt::If {
        cond: Expr::Compare(CompareOp::Le, Box::new(Expr::var("wanted")), Box::new(Expr::lit_int(0))),
        then: vec![Stmt::SessionDelete { key: "cart".into(), path: vec![Expr::var("key")] }],
        r#else: vec![Stmt::SessionSet { key: "cart".into(), path: vec![Expr::var("key")], value: Expr::var("wanted") }],
      },
      Stmt::Return(Expr::object(vec![("lines", Expr::Session("cart".into()))])),
    ]
  );

  let checkout = &actions[1];
  assert_eq!(checkout.export, "checkout");
  assert_eq!(checkout.input, None);
  assert_eq!(
    checkout.body,
    vec![
      Stmt::Let {
        name: "lines".into(),
        expr: Expr::Map(
          Box::new(Expr::Entries(Box::new(Expr::Session("cart".into())))),
          Box::new(Expr::lambda(
            &["$0"],
            Expr::object(vec![
              ("product_id", Expr::BigInt(Box::new(Expr::var("$0").index(Expr::Lit(Lit::Float(0.0)))))),
              ("quantity", Expr::var("$0").index(Expr::Lit(Lit::Float(1.0)))),
            ]),
          )),
        ),
      },
      Stmt::Guard {
        cond: Expr::Compare(CompareOp::Eq, Box::new(Expr::Length(Box::new(Expr::var("lines")))), Box::new(Expr::Lit(Lit::Float(0.0)))),
        kind: "invalid".into(),
        message: "the cart is empty".into(),
      },
      Stmt::Let { name: "order".into(), expr: Expr::call("shopping", "placeOrder", vec![("lines", Expr::var("lines"))]) },
      Stmt::SessionSet { key: "cart".into(), path: vec![], value: Expr::Object(vec![]) },
      Stmt::Return(Expr::var("order")),
    ]
  );
}

#[test]
fn ctx_as_a_single_parameter_reads_the_same_way() {
  let src = r#"
export async function load(ctx: Ctx) {
  return { product: await ctx.services.shopping.getProduct({ id: BigInt(ctx.params.id) }), who: ctx.identity.subject, at: ctx.now };
}
"#;
  let body = lower_loader("page.loader.ts", src).unwrap();
  let Stmt::Return(Expr::Object(entries)) = &body[0] else { panic!("{body:?}") };
  assert_eq!(entries[1], Entry::Field("who".into(), Expr::Identity(vec!["subject".into()])));
  assert_eq!(entries[2], Entry::Field("at".into(), Expr::Now));
}

fn residue(err: LowerError) -> Residue {
  match err {
    LowerError::Residue(r) => r,
    other => panic!("expected residue, got {other}"),
  }
}

#[test]
fn an_import_the_build_cannot_follow_is_residue_with_its_line() {
  let src = r#"
import slugify from "slugify";

export async function load({ params }: Ctx) {
  return { slug: slugify(params.name) };
}
"#;
  let r = residue(lower_loader("app/routes/x/page.loader.ts", src).unwrap_err());
  assert_eq!((r.line, r.column), (5, 18), "{r}");
  assert!(r.message.contains("`slugify`"), "{r}");
  assert!(r.to_string().starts_with("app/routes/x/page.loader.ts:5:18:"), "{r}");
}

#[test]
fn try_is_residue() {
  let src = r#"
export async function load({ services }: Ctx) {
  try {
    return { p: await services.shopping.listProducts({}) };
  } catch (e) {
    return { p: [] };
  }
}
"#;
  let r = residue(lower_loader("page.loader.ts", src).unwrap_err());
  assert_eq!(r.line, 3);
  assert_eq!(r.message, "`try`");
}

#[test]
fn a_lambda_with_statements_is_residue() {
  let src = r#"
export async function load({ services }: Ctx) {
  const xs = await services.shopping.listProducts({});
  return { names: xs.map((p) => { const n = p.name; return n; }) };
}
"#;
  let r = residue(lower_loader("page.loader.ts", src).unwrap_err());
  assert_eq!(r.line, 4);
  assert!(r.message.contains("one expression"), "{r}");
}

#[test]
fn a_write_outside_the_session_is_residue() {
  let src = r#"
export const bump = action(async ({ input }) => {
  input.count = 1;
});
"#;
  let r = residue(lower_actions("actions.ts", src).unwrap_err());
  assert_eq!(r.line, 3);
  assert!(r.message.contains("session"), "{r}");
}

#[test]
fn an_action_lowers_from_an_expression_body_and_a_function_expression() {
  let src = r#"
export const add = action(async ({ input }: ActionCtx<Add>) => input.n);
export const twice = action<Add>(async function ({ input }) {
  return input.n * 2n;
});
"#;
  let actions = lower_actions("actions.ts", src).unwrap();
  assert_eq!(actions.len(), 2);
  assert_eq!(actions[0].export, "add");
  assert_eq!(actions[0].input.as_deref(), Some("Add"));
  assert_eq!(actions[0].body, vec![Stmt::Return(Expr::Input.field("n"))]);
  assert_eq!(actions[1].export, "twice");
  assert_eq!(actions[1].input.as_deref(), Some("Add"));
  assert!(matches!(&actions[1].body[0], Stmt::Return(_)));
}

#[test]
fn an_action_of_something_other_than_a_function_is_refused() {
  let src = "const helper = async () => 1n;\nexport const add = action(helper);\n";
  let r = residue(lower_actions("actions.ts", src).unwrap_err());
  assert_eq!(r.line, 2);
  assert!(r.message.contains("`add`") && r.message.contains("action"), "{r}");
}

#[test]
fn a_missing_load_export_and_a_parse_error_name_themselves() {
  let err = lower_loader("page.loader.ts", "export const other = 1;").unwrap_err();
  assert!(matches!(err, LowerError::MissingExport { ref export, .. } if export == "load"), "{err}");

  let err = lower_loader("page.loader.ts", "export async function load( {").unwrap_err();
  assert!(matches!(err, LowerError::Parse { .. }), "{err}");
}

#[test]
fn optional_chaining_and_computed_keys_lower_as_reads_and_entries() {
  let src = r#"
export const addToCart = action<AddToCart>(async ({ input, session }) => {
  const key = String(input.product_id);
  const held = session.cart ?? {};
  const wanted = (held[key] ?? 0n) + input.quantity;
  if (wanted <= 0n) delete session.cart?.[key];
  else session.cart = { ...held, [key]: wanted };
  return { lines: session.cart ?? {} };
});
"#;
  let actions = lower_actions("actions.ts", src).unwrap();
  let body = &actions[0].body;
  assert_eq!(body[1], Stmt::Let { name: "held".into(), expr: Expr::Coalesce(Box::new(Expr::Session("cart".into())), Box::new(Expr::Object(vec![]))) });
  let Stmt::If { then, r#else, .. } = &body[3] else { panic!("{body:?}") };
  assert_eq!(then[0], Stmt::SessionDelete { key: "cart".into(), path: vec![Expr::var("key")] });
  assert_eq!(
    r#else[0],
    Stmt::SessionSet {
      key: "cart".into(),
      path: vec![],
      value: Expr::Object(vec![Entry::Spread(Expr::var("held")), Entry::Computed(Expr::var("key"), Expr::var("wanted"))]),
    }
  );
}

#[test]
fn the_input_type_reads_from_the_parameter_annotation() {
  let annotated = "import { action } from \"@snapfire/fsr\";\nimport type { ActionCtx } from \"@snapfire/fsr\";\nimport type { AddToCart } from \"../../schemas/cart\";\n\nexport const addToCart = action(async ({ input, session }: ActionCtx<AddToCart>) => {\n  session.cart = { ...session.cart, [String(input.product_id)]: input.quantity };\n});\n\nexport const remove = action(async (ctx: ActionCtx<RemoveFromCart>) => {\n  delete ctx.session.cart[String(ctx.input.product_id)];\n});\n\nexport const checkout = action(async ({ session }: ActionCtx) => {\n  session.cart = {};\n});\n";
  let actions = lower_actions("actions.ts", annotated).unwrap();
  assert_eq!(actions.iter().map(|a| (a.export.as_str(), a.input.as_deref())).collect::<Vec<_>>(), vec![("addToCart", Some("AddToCart")), ("remove", Some("RemoveFromCart")), ("checkout", None)]);
  let older = lower_actions("actions.ts", ACTIONS).unwrap();
  assert_eq!(older[0].input.as_deref(), Some("AddToCart"), "the type-argument spelling still reads");
}

#[test]
fn a_loader_reads_the_locale_whole_and_through_the_context() {
  let src = r#"
import type { Ctx } from "../../generated/ctx";

export async function load({ locale, services }: Ctx) {
  const copy = await services.shopping.listProducts({ tag: locale });
  return { copy, french: locale === "fr_FR" };
}
"#;
  let body = lower_loader("routes/help/page.loader.ts", src).unwrap();
  assert_eq!(
    body,
    vec![
      Stmt::Let { name: "copy".to_owned(), expr: Expr::call("shopping", "listProducts", vec![("tag", Expr::Locale)]) },
      Stmt::Return(Expr::object(vec![("copy", Expr::var("copy")), ("french", Expr::Compare(CompareOp::Eq, Box::new(Expr::Locale), Box::new(Expr::lit_str("fr_FR"))))])),
    ]
  );

  let via_ctx = r#"
export async function load(ctx: Ctx) {
  return { locale: ctx.locale };
}
"#;
  let body = lower_loader("routes/help/page.loader.ts", via_ctx).unwrap();
  assert_eq!(body, vec![Stmt::Return(Expr::object(vec![("locale", Expr::Locale)]))]);
  assert!(!snapfire_fsr_ir::body_reads_request(&body), "the locale alone leaves a route prerenderable, once per locale");
}
