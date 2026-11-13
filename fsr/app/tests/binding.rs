use std::sync::Arc;

use snapfire_fsr::{App, BindError, Owner, Routes};
use snapfire_fsr_core::{ModuleId, NodeId, PlanNode, SlotName, ValueMap};
use snapfire_fsr_runtime::NullEvaluator;

const MANIFEST: &str = r#"(plan 1)
(route / (node 0 shell#document (slot content (node 1 app#Catalog (source catalog)))))
(route /cart (node 0 shell#document (slot content (node 1 app#Cart (source cart)))))
"#;

fn plan(module: &str) -> PlanNode {
  let content = PlanNode::new(NodeId(1), ModuleId::new("app", module));
  let mut shell = PlanNode::new(NodeId(0), ModuleId::new("shell", "document"));
  shell.children.push((SlotName("content".into()), content));
  shell
}

fn bound() -> snapfire_fsr::AppBuilder {
  App::from_manifest(MANIFEST)
    .unwrap()
    .source("catalog", |_ctx| async { Ok(ValueMap::default()) })
    .source("cart", |_ctx| async { Ok(ValueMap::default()) })
    .evaluator(|_: &ModuleId| true, Arc::new(NullEvaluator))
}

#[test]
fn a_plan_naming_a_source_nothing_answers_refuses_to_start() {
  let err = App::from_manifest(MANIFEST)
    .unwrap()
    .source("catalog", |_ctx| async { Ok(ValueMap::default()) })
    .build()
    .unwrap_err();

  assert_eq!(err, BindError::Unbound { name: "cart".into() });
  assert!(err.to_string().contains("which nothing answers"));
}

#[test]
fn an_override_that_names_nothing_refuses_to_start() {
  let err = bound()
    .source_override("pricing", |_ctx| async { Ok(ValueMap::default()) })
    .build()
    .unwrap_err();

  assert_eq!(err, BindError::OverridesNothing { name: "pricing".into() });
}

#[test]
fn the_report_says_who_answers_what() {
  let app = bound()
    .source_override("cart", |_ctx| async { Ok(ValueMap::default()) })
    .route("/about", plan("About"))
    .action("checkout", |_ctx, input| async move { Ok(input) })
    .build()
    .unwrap();

  assert_eq!(
    app.report.routes,
    vec![
      ("/".to_owned(), Owner::PlanFile),
      ("/about".to_owned(), Owner::Rust),
      ("/cart".to_owned(), Owner::PlanFile),
    ]
  );
  assert_eq!(
    app.report.sources,
    vec![("catalog".to_owned(), Owner::Rust), ("cart".to_owned(), Owner::RustOverride)]
  );
  assert_eq!(app.report.actions, vec![("checkout".to_owned(), Owner::Rust)]);

  let printed = app.report.to_string();
  assert!(printed.contains("cart"), "{printed}");
  assert!(printed.contains("rust override"), "{printed}");
}

#[test]
fn a_route_the_plan_file_claims_cannot_be_added_twice() {
  let err = bound().route("/", plan("Other")).build().unwrap_err();
  assert_eq!(err, BindError::Claimed("/".into()));
  assert!(err.to_string().contains("mark the Rust one as an override"));
}

#[test]
fn replacing_a_route_is_allowed_and_reported() {
  let app = bound().route_override("/", plan("Other")).build().unwrap();
  assert_eq!(app.report.routes[0], ("/".to_owned(), Owner::RustOverride));
  assert_eq!(app.report.routes.len(), 2, "the replacement did not add a route");
}

#[test]
fn a_pattern_the_matcher_refuses_names_itself() {
  let err = bound().route("/{unclosed", plan("Other")).build().unwrap_err();
  assert!(matches!(err, BindError::Pattern { .. }), "{err}");
}

#[test]
fn routes_alone_build_without_a_plan_file() {
  let app = App::builder(Routes::new().add("/only", plan("Only")))
    .evaluator(|_: &ModuleId| true, Arc::new(NullEvaluator))
    .build()
    .unwrap();

  assert_eq!(app.report.routes, vec![("/only".to_owned(), Owner::Rust)]);
  assert!(app.report.sources.is_empty());
}

#[test]
fn a_bad_manifest_reports_itself_rather_than_panicking() {
  assert!(matches!(App::from_manifest("nonsense"), Err(BindError::Plan(_))));
  assert!(matches!(App::from_manifest("(plan 99)\n"), Err(BindError::Plan(_))));
}

#[test]
fn the_plan_builder_numbers_nodes_so_the_caller_never_does() {
  use snapfire_fsr::{IntoPlan, Plan};

  let built = Plan::of("shell#document")
    .slot(
      "content",
      Plan::of("app#Catalog")
        .source("catalog")
        .error("app#Failed")
        .cache_key("catalog_page")
        .slot("aside", Plan::of("app#Recommended").source("aside").deferred().fallback("app#Loading")),
    )
    .into_plan()
    .unwrap();

  assert_eq!(built.id, NodeId(0));
  let (slot, content) = &built.children[0];
  assert_eq!(slot.0, "content");
  assert_eq!(content.id, NodeId(1), "ids run in tree order");
  assert_eq!(content.data_source.as_ref().unwrap().0, "catalog");
  assert_eq!(content.error, Some(ModuleId::new("app", "Failed")));

  let (aside_slot, aside) = &content.children[0];
  assert_eq!(aside_slot.0, "aside");
  assert_eq!(aside.id, NodeId(2));
  assert!(aside.deferred);
  assert_eq!(aside.fallback, Some(ModuleId::new("app", "Loading")));
}

#[test]
fn a_module_the_builder_cannot_parse_fails_the_build() {
  use snapfire_fsr::Plan;

  let err = App::builder(Routes::new().add("/x", Plan::of("no-hash")))
    .evaluator(|_: &ModuleId| true, Arc::new(NullEvaluator))
    .build()
    .unwrap_err();

  assert_eq!(err, BindError::Module { module: "no-hash".into() });
}

#[test]
fn a_plan_node_built_by_hand_is_still_accepted() {
  let app = App::builder(Routes::new().add("/raw", plan("Raw")))
    .evaluator(|_: &ModuleId| true, Arc::new(NullEvaluator))
    .build()
    .unwrap();
  assert_eq!(app.report.routes, vec![("/raw".to_owned(), Owner::Rust)]);
}

const LOWERED: &str = r#"(plan 2)
(route / (node 0 app#Catalog (source catalog)))
(source catalog
  lowered
  (module app/routes/index/page.loader.ts)
  (body (ret (obj (products (arr))))))
(action checkout lowered (module app/routes/cart/actions.ts) (body (ret "order-1")))
"#;

#[test]
fn a_lowered_row_binds_itself_and_the_report_says_so() {
  let app = App::from_manifest(LOWERED)
    .unwrap()
    .evaluator(|_: &ModuleId| true, Arc::new(NullEvaluator))
    .build()
    .unwrap();

  assert_eq!(app.report.sources, vec![("catalog".to_owned(), Owner::Lowered)]);
  assert_eq!(app.report.actions, vec![("checkout".to_owned(), Owner::Lowered)]);
  assert!(app.report.to_string().contains("lowered"), "{}", app.report);

  use snapfire_fsr_runtime::{assemble, EntryId, Resolver};
  let ctx = snapfire_fsr_runtime::RequestCtx::anonymous(Default::default());
  let plan = app.resolver.resolve(EntryId(0), &ctx.params).unwrap();
  let assembly = futures::executor::block_on(assemble(&app.runtime, &plan, &ctx, &snapfire_fsr_core::Node::raw(""))).unwrap();
  let printed = format!("{:?}", assembly.tree);
  assert!(printed.contains("products"), "the lowered body answered the source: {printed}");

  let order = futures::executor::block_on(app.actions.dispatch("checkout", ctx, snapfire_fsr_core::Value::Null)).unwrap();
  assert_eq!(order, snapfire_fsr_core::Value::str("order-1"));
}

fn compiled_catalog(_: &mut snapfire_fsr_ir::native::Frame<'_>) -> Result<snapfire_fsr_core::Value, snapfire_fsr_ir::Fail> {
  let mut fields = snapfire_fsr_core::Fields::default();
  fields.insert("ran".to_owned(), snapfire_fsr_core::Value::str("compiled"));
  Ok(snapfire_fsr_core::Value::Map(fields.into()))
}

fn catalog_with(plan: impl Fn(u64) -> u64) -> String {
  let parsed = snapfire_fsr_plan::Manifest::from_text(LOWERED).unwrap();
  let body = parsed.lowered_sources().next().and_then(|row| row.body.as_ref()).unwrap();
  let native = snapfire_fsr_ir::native::Native { source: "catalog", plan: plan(snapfire_fsr_ir::native::plan_hash(body, None)), body: compiled_catalog };
  let app = App::from_manifest(LOWERED)
    .unwrap()
    .bodies(Box::leak(Box::new([native])))
    .evaluator(|_: &ModuleId| true, Arc::new(NullEvaluator))
    .build()
    .unwrap();
  use snapfire_fsr_runtime::{assemble, EntryId, Resolver};
  let ctx = snapfire_fsr_runtime::RequestCtx::anonymous(Default::default());
  let plan = app.resolver.resolve(EntryId(0), &ctx.params).unwrap();
  let assembly = futures::executor::block_on(assemble(&app.runtime, &plan, &ctx, &snapfire_fsr_core::Node::raw(""))).unwrap();
  format!("{:?}", assembly.tree)
}

#[test]
fn a_compiled_body_answers_the_source_whose_plan_it_was_written_from() {
  let printed = catalog_with(|plan| plan);
  assert!(printed.contains("compiled") && !printed.contains("products"), "{printed}");
}

#[test]
fn a_compiled_body_written_from_another_plan_leaves_the_source_interpreted() {
  let printed = catalog_with(|plan| plan ^ 1);
  assert!(printed.contains("products") && !printed.contains("compiled"), "{printed}");
}

const LOWERED_HANDLER: &str = r#"(plan 2)
(handler api.products.GET GET /api/products lowered (body (ret "interpreted")))
"#;

fn compiled_handler(_: &mut snapfire_fsr_ir::native::Frame<'_>) -> Result<snapfire_fsr_core::Value, snapfire_fsr_ir::Fail> {
  Ok(snapfire_fsr_core::Value::str("compiled"))
}

fn handler_with(plan: impl Fn(u64) -> u64) -> snapfire_fsr_core::Value {
  let parsed = snapfire_fsr_plan::Manifest::from_text(LOWERED_HANDLER).unwrap();
  let body = parsed.handlers[0].body.as_ref().unwrap();
  let native = snapfire_fsr_ir::native::Native { source: "api.products.GET", plan: plan(snapfire_fsr_ir::native::plan_hash(body, None)), body: compiled_handler };
  let app = App::from_manifest(LOWERED_HANDLER)
    .unwrap()
    .bodies(Box::leak(Box::new([native])))
    .evaluator(|_: &ModuleId| true, Arc::new(NullEvaluator))
    .build()
    .unwrap();
  let ctx = snapfire_fsr_runtime::RequestCtx::anonymous(Default::default());
  futures::executor::block_on(app.handlers.dispatch("api.products.GET", ctx, snapfire_fsr_core::Value::Null)).unwrap()
}

#[test]
fn a_compiled_body_answers_the_route_handler_whose_plan_it_was_written_from() {
  assert_eq!(handler_with(|plan| plan), snapfire_fsr_core::Value::str("compiled"));
}

#[test]
fn a_compiled_body_written_from_another_plan_leaves_the_route_handler_interpreted() {
  assert_eq!(handler_with(|plan| plan ^ 1), snapfire_fsr_core::Value::str("interpreted"));
}

const LOWERED_EVERY_KIND: &str = r#"(plan 2)
(route / (node 0 app#Catalog (source catalog)))
(middleware (ret "next"))
(source catalog
  lowered
  (module app/routes/index/page.loader.ts)
  (body (ret (obj (products (arr))))))
(action checkout lowered (module app/routes/cart/actions.ts) (body (ret "order-1")))
(handler api.products.GET GET /api/products lowered (body (ret "interpreted")))
"#;

/// The app bound with a compiled body for each lowered row `compiled` names: `catalog`, `checkout`, `api.products.GET` or `middleware`.
fn every_kind_with(compiled: &[&str]) -> snapfire_fsr::App {
  let parsed = snapfire_fsr_plan::Manifest::from_text(LOWERED_EVERY_KIND).unwrap();
  let hash = |body: &snapfire_fsr_ir::Body| snapfire_fsr_ir::native::plan_hash(body, None);
  let rows: Vec<(&'static str, u64, snapfire_fsr_ir::native::NativeBody)> = vec![
    ("catalog", hash(parsed.sources[0].body.as_ref().unwrap()), compiled_catalog),
    ("checkout", hash(parsed.actions[0].body.as_ref().unwrap()), compiled_handler),
    ("api.products.GET", hash(parsed.handlers[0].body.as_ref().unwrap()), compiled_handler),
    ("middleware", hash(parsed.middleware.as_ref().unwrap()), compiled_handler),
  ];
  let natives: Vec<snapfire_fsr_ir::native::Native> = rows.into_iter().filter(|(id, _, _)| compiled.contains(id)).map(|(source, plan, body)| snapfire_fsr_ir::native::Native { source, plan, body }).collect();
  App::from_manifest(LOWERED_EVERY_KIND)
    .unwrap()
    .bodies(Box::leak(natives.into_boxed_slice()))
    .evaluator(|_: &ModuleId| true, Arc::new(NullEvaluator))
    .build()
    .unwrap()
}

#[test]
fn an_action_and_middleware_run_the_compiled_body_written_from_their_plan() {
  let app = every_kind_with(&["catalog", "checkout", "api.products.GET", "middleware"]);
  let ctx = snapfire_fsr_runtime::RequestCtx::anonymous(Default::default());
  let action = futures::executor::block_on(app.actions.dispatch("checkout", ctx.clone(), snapfire_fsr_core::Value::Null)).unwrap();
  assert_eq!(action, snapfire_fsr_core::Value::str("compiled"));
  let middleware = futures::executor::block_on(app.middleware.as_ref().unwrap().call(ctx, snapfire_fsr_core::Value::Null)).unwrap();
  assert_eq!(middleware, snapfire_fsr_core::Value::str("compiled"));
}

#[test]
fn the_report_says_which_lowered_rows_run_compiled_and_which_interpreted() {
  let app = every_kind_with(&["catalog", "api.products.GET"]);
  let printed = app.report.to_string();
  assert!(printed.contains("catalog                lowered, compiled"), "{printed}");
  assert!(printed.contains("checkout               lowered, interpreted"), "{printed}");
  assert!(printed.contains("GET /api/products      lowered, compiled"), "{printed}");
  assert!(printed.contains("middleware             lowered, interpreted"), "{printed}");
}

#[test]
fn the_report_names_no_compilation_when_the_host_was_handed_no_compiled_bodies() {
  let app = App::from_manifest(LOWERED_EVERY_KIND).unwrap().evaluator(|_: &ModuleId| true, Arc::new(NullEvaluator)).build().unwrap();
  let printed = app.report.to_string();
  assert!(!printed.contains("compiled") && !printed.contains("interpreted"), "{printed}");
}

#[test]
fn rust_takes_a_lowered_name_back_only_as_an_override() {
  let err = App::from_manifest(LOWERED)
    .unwrap()
    .source("catalog", |_ctx| async { Ok(ValueMap::default()) })
    .build()
    .unwrap_err();
  assert_eq!(err, BindError::Claimed("catalog".into()));

  let err = App::from_manifest(LOWERED)
    .unwrap()
    .action("checkout", |_ctx, input| async move { Ok(input) })
    .build()
    .unwrap_err();
  assert_eq!(err, BindError::ActionClaimed("checkout".into()));

  let app = App::from_manifest(LOWERED)
    .unwrap()
    .source_override("catalog", |_ctx| async { Ok(ValueMap::default()) })
    .action_override("checkout", |_ctx, input| async move { Ok(input) })
    .evaluator(|_: &ModuleId| true, Arc::new(NullEvaluator))
    .build()
    .unwrap();
  assert_eq!(app.report.sources, vec![("catalog".to_owned(), Owner::RustOverride)]);
  assert_eq!(app.report.actions, vec![("checkout".to_owned(), Owner::RustOverride)]);
}

#[test]
fn an_action_override_that_names_nothing_refuses_to_start() {
  let err = bound()
    .action_override("checkout", |_ctx, input| async move { Ok(input) })
    .build()
    .unwrap_err();
  assert_eq!(err, BindError::ActionOverridesNothing { id: "checkout".into() });
}

const IDENTIFIED: &str = r#"(plan 2)
(route /about (node 0 routes/about/page.tsx#default (source about)))
(route /account (node 0 routes/account/page.tsx#default (source about)))
(source about lowered (module routes/about/page.loader.ts) (body (ret (obj (title "About")))))
(component routes/about/page.tsx#default (owner react) (render (el h1 () {(. $props title)})))
(component routes/account/page.tsx#default
  (owner react)
  (render (el p () {(. (. $props identity) subject)})))
"#;

#[test]
fn a_page_reading_its_identity_prop_never_prerenders() {
  let app = App::from_manifest(IDENTIFIED).unwrap().build().unwrap();
  assert_eq!(app.report.prerenderable, vec!["/about".to_owned()], "{}", app.report);
}

const BEARER: &str = r#"(plan 2)
(route /posts (node 0 routes/posts/page.tsx#default (source posts)))
(route /cart (node 0 routes/cart/page.tsx#default (source cart)))
(route /faq (node 0 routes/faq/page.tsx#default (source faq)))
(source posts
  lowered
  (module routes/posts/page.loader.ts)
  (body (ret (obj (posts (call cms list))))))
(source cart
  lowered
  (module routes/cart/page.loader.ts)
  (body (ret (obj (who (identity subject)) (n (session count))))))
(source faq lowered (module routes/faq/page.loader.ts) (body (ret (obj (items (call docs list))))))
"#;

#[test]
fn a_route_whose_only_request_reads_are_the_identity_prerenders_for_anonymous_visitors() {
  let app = App::from_manifest(IDENTIFIED).unwrap().build().unwrap();
  assert_eq!(app.report.prerenderable, vec!["/about".to_owned()]);
  assert_eq!(app.report.prerenderable_anonymous, vec!["/account".to_owned()], "a page reading its identity prop: {}", app.report);

  let app = App::from_manifest(BEARER).unwrap().bearer_services(vec!["cms".to_owned()]).build().unwrap();
  assert_eq!(app.report.prerenderable, vec!["/faq".to_owned()], "a call through a client with no token reads nothing of the request");
  assert_eq!(app.report.prerenderable_anonymous, vec!["/posts".to_owned()], "a call through a bearer client depends on the identity; a session read is dynamic");
  let app = App::from_manifest(BEARER).unwrap().build().unwrap();
  assert_eq!(app.report.prerenderable, vec!["/posts".to_owned(), "/faq".to_owned()]);
  assert!(app.report.prerenderable_anonymous.is_empty());
}

const CLOCKED: &str = r#"(plan 2)
(route /now (node 0 routes/now/page.tsx#default (source now)))
(route /label (node 0 routes/label/page.tsx#default (source label)))
(source now
  lowered
  (module routes/now/page.loader.ts)
  (body (ret (obj (at (ext time now)) (id (ext id new))))))
(source label
  lowered
  (module routes/label/page.loader.ts)
  (body (ret (obj (n (ext intl number 3.5))))))
"#;

/// A standard member that answers per call, the clock or a fresh id, is read at request time; a render member over constants is not.
#[test]
fn a_loader_calling_a_body_member_of_the_standard_library_never_prerenders() {
  let app = App::from_manifest(CLOCKED).unwrap().build().unwrap();
  assert_eq!(app.report.prerenderable, vec!["/label".to_owned()], "{}", app.report);
  assert!(app.report.prerenderable_anonymous.is_empty(), "{}", app.report);
  let body = vec![snapfire_fsr_ir::Stmt::Return(snapfire_fsr_ir::Expr::Ext { module: "time".into(), name: "now".into(), args: vec![] })];
  assert!(snapfire_fsr_ir::body_reads_request(&body), "the IR's own analysis agrees");
}

const BLOG: &str = r#"(plan 2)
(route /blog (node 0 routes/blog/page.tsx#default (source blog)))
(route |/blog/{slug}| (node 0 routes/blog/post/page.tsx#default (source post)))
(source blog
  lowered
  (module routes/blog/page.loader.ts)
  (body (ret (obj (posts (arr (: "hello") (: "world")))))))
(source post
  lowered
  (module routes/blog/post/page.loader.ts)
  (body (ret (obj (slug (param slug)))))
  (paths (ret (arr (: (obj (slug "hello"))) (: (obj (slug "world")))))))
(component routes/blog/page.tsx#default (owner react) (render (el h1 () "blog")))
(component routes/blog/post/page.tsx#default (owner react) (render (el h1 () {(. $props slug)})))
"#;

#[test]
fn a_route_with_a_parameter_prerenders_when_its_loader_names_its_paths() {
  let app = App::from_manifest(BLOG).unwrap().build().unwrap();
  assert_eq!(app.report.prerenderable, vec!["/blog".to_owned(), "/blog/{slug}".to_owned()], "{}", app.report);
  assert_eq!(app.report.paths, vec!["/blog/{slug}".to_owned()]);
  assert!(app.paths.contains_key("/blog/{slug}"));
  assert_eq!(app.report.warmable, vec!["blog".to_owned()], "a source reading a parameter is rendered per set, never memoized by name");

  let without = BLOG.replace("\n  (paths (ret (arr (: (obj (slug \"hello\"))) (: (obj (slug \"world\"))))))", "");
  let app = App::from_manifest(&without).unwrap().build().unwrap();
  assert_eq!(app.report.prerenderable, vec!["/blog".to_owned()], "a parameter with no paths stays live");
  assert!(app.report.paths.is_empty());
}

#[test]
fn a_paths_body_reading_the_request_is_refused() {
  let reading = BLOG.replace(r#"(slug "world")"#, "(slug (session last))");
  let err = App::from_manifest(&reading).unwrap().build().err().expect("refused");
  assert!(matches!(err, BindError::PathsReadRequest { ref loader } if loader == "post"), "{err}");
}

#[test]
fn paths_on_a_route_with_no_parameter_is_refused() {
  let fixed = BLOG.replace("|/blog/{slug}|", "/blog/post");
  let err = App::from_manifest(&fixed).unwrap().build().err().expect("refused");
  assert!(matches!(err, BindError::PathsWithoutParameter { ref loader, ref pattern } if loader == "post" && pattern == "/blog/post"), "{err}");
}

const CONSOLE: &str = r#"(plan 2)
(route /help
  (node 0
    routes/layout.tsx#default
    (source layout)
    (cache-key routes/layout.tsx#default)
    (slot content
      (node 1 routes/help/page.tsx#default (source help) (cache-key routes/help/page.tsx#default)))))
(route /cart
  (node 0
    routes/layout.tsx#default
    (source layout)
    (cache-key routes/layout.tsx#default)
    (slot content (node 1 routes/cart/page.tsx#default (cache-key routes/cart/page.tsx#default)))))
(route /promo
  (node 0
    routes/promo/layout.tsx#default
    (source promo)
    (cache-key routes/promo/layout.tsx#default)
    (slot content (node 1 routes/cart/page.tsx#default (cache-key routes/cart/page.tsx#default)))))
(source layout
  lowered
  (module routes/layout.loader.ts)
  (body (ret (obj (who (session user)))))
  (store (ret (obj (cart/count 2)))))
(source help lowered (module routes/help/page.loader.ts) (body (ret (obj (title "Help")))))
(source promo
  lowered
  (module routes/promo/layout.loader.ts)
  (body (ret (obj (n 1))))
  (store (ret (obj (cart/count 0)))))
(component routes/layout.tsx#default
  (owner react)
  (render (el form ((data-token (. $props csrf_token))) (slot content))))
(component routes/promo/layout.tsx#default (owner react) (render (el div () (slot content))))
(component routes/help/page.tsx#default (owner react) (render (el h1 () {(. $props title)})))
(component routes/cart/page.tsx#default
  (owner react)
  (render (el p () (comp src/Badge.tsx#Badge 0 ()))))
(component src/Badge.tsx#Badge (owner react) (render (el b () {(store cart/count)})))
"#;

#[test]
fn a_subtree_is_classed_by_what_it_reads_rather_than_by_its_route() {
  use snapfire_fsr_runtime::{subtree_shape, Static};
  let app = App::from_manifest(CONSOLE).unwrap().build().unwrap();
  assert_eq!(app.report.prerenderable, vec!["/promo".to_owned()], "the other two sit under a layout reading the request: {}", app.report);
  let leaf = |module: &str| subtree_shape(&PlanNode::new(NodeId(0), ModuleId::new(module.split('#').next().unwrap(), "default")));
  let reads = &app.runtime.reads;

  let help = &reads[&leaf("routes/help/page.tsx#default")];
  assert_eq!(help.class, Static::Fixed, "a page reading nothing under a layout reading the token");
  assert!(help.store_keys.is_empty());

  let mut layout = PlanNode::new(NodeId(0), ModuleId::new("routes/layout.tsx", "default"));
  layout.children.push((SlotName("content".into()), PlanNode::new(NodeId(1), ModuleId::new("routes/help/page.tsx", "default"))));
  assert_eq!(reads[&subtree_shape(&layout)].class, Static::Dynamic, "the layout reads its csrf_token prop and its source reads the session");

  let cart = &reads[&leaf("routes/cart/page.tsx#default")];
  assert_eq!(cart.store_keys, vec!["cart/count".to_owned()], "a key read by a component the page places");
  assert_eq!(cart.class, Static::Fixed, "the key's value is in the memo key, so who seeds it does not class the page");

  let mut promo = PlanNode::new(NodeId(0), ModuleId::new("routes/promo/layout.tsx", "default"));
  promo.children.push((SlotName("content".into()), PlanNode::new(NodeId(1), ModuleId::new("routes/cart/page.tsx", "default"))));
  let promo = &reads[&subtree_shape(&promo)];
  assert_eq!(promo.class, Static::Fixed);
  assert_eq!(promo.store_keys, vec!["cart/count".to_owned()], "a layout's subtree lists what its page reads");
}

#[test]
fn a_build_renders_ahead_the_outermost_fixed_subtrees_of_a_route_it_cannot_write_whole() {
  let app = App::from_manifest(CONSOLE).unwrap().build().unwrap();
  let picked: Vec<(&str, String)> = app.renderable.iter().map(|(p, n)| (p.as_str(), n.module.to_string())).collect();
  assert_eq!(picked, vec![("/help", "routes/help/page.tsx#default".to_owned())], "{}", app.report);
  assert_eq!(app.report.renderable, vec![("/help".to_owned(), "routes/help/page.tsx#default".to_owned())]);
  assert!(!picked.iter().any(|(p, _)| *p == "/cart"), "the cart page reads a key the layout outside it seeds");
  assert!(!picked.iter().any(|(p, _)| *p == "/promo"), "a route written as a document has nothing to render ahead");
}

#[test]
fn a_loader_with_defaults_and_no_store_seeds_its_keys() {
  let manifest = CONSOLE.replacen("(store (ret (obj (cart/count 2))))", "(defaults (ret (obj (cart/count 2))))", 1);
  assert_ne!(manifest, CONSOLE);
  let app = App::from_manifest(&manifest).unwrap().build().unwrap();
  let seeds = app.runtime.stores.get("layout").expect("the layout's defaults are a store");
  assert_eq!(seeds.keys(), Some(vec!["cart/count".to_owned()]));
  let ctx = snapfire_fsr_runtime::RequestCtx::default();
  let data = snapfire_fsr_core::ValueMap::default();
  assert!(futures::executor::block_on(seeds.seed(&ctx, &data)).unwrap().is_empty(), "nothing is authoritative");
  assert_eq!(futures::executor::block_on(seeds.defaults(&ctx, &data)).unwrap().get("cart/count"), Some(&snapfire_fsr_core::Value::int(2i64)));
  assert!(!app.renderable.iter().any(|(p, _)| p == "/cart"), "the cart page reads a key the layout's defaults seed");
}

#[test]
fn a_page_the_build_could_not_lower_is_reported_at_boot_with_its_cause() {
  let manifest = r#"(plan 3)
(route / (node 0 shell#document (slot content (node 1 routes/page.tsx#default))))
(route /other (node 0 shell#document (slot content (node 1 routes/other/page.tsx#default))))
(client routes/page.tsx#default
  "src/ui/Stars.tsx:2:17"
  "`.slice()`, which is not a builtin"
  "the builtins are `map`, `filter`, ...")
(client routes/other/page.tsx#default
  "src/ui/Stars.tsx:2:17"
  "`.slice()`, which is not a builtin"
  "the builtins are `map`, `filter`, ...")
"#;
  let app = App::from_manifest(manifest).unwrap().evaluator(|_: &ModuleId| true, Arc::new(NullEvaluator)).build().unwrap();
  assert_eq!(app.report.clients.len(), 2);
  let printed = app.report.to_string();
  assert!(printed.contains("rendered  routes/page.tsx#default client      src/ui/Stars.tsx:2:17\n          routes/other/page.tsx#default client      src/ui/Stars.tsx:2:17\n"), "{printed}");
  assert!(printed.contains("client    src/ui/Stars.tsx:2:17  `.slice()`, which is not a builtin\n          the builtins are `map`, `filter`, ...\n"), "{printed}");
  assert_eq!(printed.matches("`.slice()`").count(), 1, "one cause is stated once: {printed}");
}
