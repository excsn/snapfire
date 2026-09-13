//! The authoring forms a feature is recognised in. Each of these lowered to
//! nothing or to the wrong thing, because the build matched a local spelling
//! or one declaration shape instead of what the module imported and exported.

use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;

use snapfire_fsr_core::{Value, ValueMap};
use snapfire_fsr_ir::ast::{Entry, Expr, Lit, Stmt};
use snapfire_fsr_ir::render::{prepare, Components, MAX_CALLS};
use snapfire_fsr_ir::{Interpreter, Tmpl};
use snapfire_fsr_lower::component::ComponentSet;
use snapfire_fsr_lower::{lower_actions, lower_handlers, lower_loader, lower_middleware, read_session_defaults};

static NEXT: AtomicU32 = AtomicU32::new(0);

fn app(files: &[(&str, &str)]) -> std::path::PathBuf {
  let n = NEXT.fetch_add(1, Ordering::Relaxed);
  let dir = std::env::temp_dir().join(format!("fsr_forms_{}_{n}", std::process::id()));
  let _ = std::fs::remove_dir_all(&dir);
  for (name, source) in files {
    let path = dir.join(name);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, source).unwrap();
  }
  dir
}

fn lower(files: &[(&str, &str)], module: &str) -> ComponentSet {
  let mut set = ComponentSet::new(&app(files));
  set.lower(module).unwrap();
  set
}

fn render_of<'a>(set: &'a ComponentSet, module: &str) -> &'a Tmpl {
  &set.components.iter().find(|(m, _)| m == module).unwrap_or_else(|| panic!("{module} did not lower")).1.render
}

fn library(set: &ComponentSet) -> Components {
  set.components.iter().map(|(module, component)| (module.clone(), Arc::new(prepare(component)))).collect()
}

fn render(library: &Components, module: &str, props: &[(&str, Value)]) -> Result<String, String> {
  let props: ValueMap = props.iter().map(|(name, value)| ((*name).to_owned(), value.clone())).collect();
  Interpreter::default().render_module(module, &library[module], &props, library).map(|rendered| rendered.html).map_err(|fail| fail.message)
}

fn verdicts<'a>(set: &'a ComponentSet, module: &str) -> (bool, Option<&'a bool>) {
  (set.components.iter().find(|(m, _)| m == module).unwrap_or_else(|| panic!("{module} did not lower")).1.hydrate, set.pure.get(module))
}

fn island_in(tmpl: &Tmpl) -> Option<(&str, Option<&str>, Option<&str>)> {
  match tmpl {
    Tmpl::Island { module, when, mode, .. } => Some((module, when.as_deref(), mode.as_deref())),
    Tmpl::Element { children, .. } | Tmpl::Fragment(children) => children.iter().find_map(island_in),
    _ => None,
  }
}

#[test]
fn a_component_that_renders_itself_is_a_call_the_renderer_makes() {
  let set = lower(&[("routes/page.tsx", "export function Tree({ depth }: { depth: number }) {\n  return <ul>{depth > 0 ? <Tree depth={depth - 1} /> : null}</ul>;\n}\n")], "routes/page.tsx#Tree");
  assert_eq!(set.components.len(), 1, "lowered once");
  assert_eq!(render(&library(&set), "routes/page.tsx#Tree", &[("depth", Value::F64(2.0))]).unwrap(), "<ul><ul><ul></ul></ul></ul>");
  assert_eq!(verdicts(&set, "routes/page.tsx#Tree"), (false, Some(&true)), "a call to itself neither hydrates it nor makes it impure");
}

#[test]
fn two_components_that_render_each_other_are_calls_the_renderer_makes() {
  let set = lower(
    &[
      ("routes/page.tsx", "import Odd from \"./odd\";\nexport default function Even({ n }: { n: number }) {\n  return <div>{n > 0 ? <Odd n={n - 1} /> : null}</div>;\n}\n"),
      ("routes/odd.tsx", "import Even from \"./page\";\nexport default function Odd({ n }: { n: number }) {\n  return <span>{n > 0 ? <Even n={n - 1} /> : null}</span>;\n}\n"),
    ],
    "routes/page.tsx#default",
  );
  assert_eq!(render(&library(&set), "routes/page.tsx#default", &[("n", Value::F64(3.0))]).unwrap(), "<div><span><div><span></span></div></span></div>");
  assert_eq!(verdicts(&set, "routes/page.tsx#default"), (false, Some(&true)), "Even read Odd while Odd was provisional");
  assert_eq!(verdicts(&set, "routes/odd.tsx#default"), (false, Some(&true)), "and Odd read Even while Even was still being lowered");
}

#[test]
fn state_on_a_cycle_hydrates_every_component_that_renders_it() {
  let set = lower(
    &[
      ("routes/page.tsx", "import { useState } from \"react\";\nimport Odd from \"./odd\";\nexport default function Even({ n }: { n: number }) {\n  const [open, setOpen] = useState(false);\n  return <div onClick={() => setOpen(!open)}>{n > 0 ? <Odd n={n - 1} /> : null}</div>;\n}\n"),
      ("routes/odd.tsx", "import Even from \"./page\";\nexport default function Odd({ n }: { n: number }) {\n  return <span>{n > 0 ? <Even n={n - 1} /> : null}</span>;\n}\n"),
    ],
    "routes/page.tsx#default",
  );
  assert_eq!(verdicts(&set, "routes/odd.tsx#default"), (true, Some(&false)), "Odd renders Even inline and Even has state, though Even was not done when Odd was lowered");
}

/// 2 MiB is a tokio worker's stack, which is where a host renders. This is a
/// debug build, which is what `fsr dev` serves from.
#[test]
fn a_render_nested_past_the_limit_fails_rather_than_overflowing_the_stack() {
  let set = lower(
    &[("routes/page.tsx", "export function Node({ node }: { node: { text: string; children: unknown[] } }) {\n  return <div>{node.text}{node.children.map((child, i) => <Node key={i} node={child} />)}</div>;\n}\n")],
    "routes/page.tsx#Node",
  );
  let library = library(&set);
  let nested = |levels: usize| {
    let mut node = Value::seq(Vec::new());
    for _ in 0..levels {
      let map: ValueMap = [("text".to_owned(), Value::str("x")), ("children".to_owned(), node)].into_iter().collect();
      node = Value::seq(vec![Value::Map(map)]);
    }
    let Value::Seq(mut top) = node else { unreachable!() };
    top.pop().unwrap()
  };
  let (within, past) = (nested(MAX_CALLS + 1), nested(MAX_CALLS + 2));
  let (within, past) = std::thread::Builder::new()
    .stack_size(2 << 20)
    .spawn(move || (render(&library, "routes/page.tsx#Node", &[("node", within)]), render(&library, "routes/page.tsx#Node", &[("node", past)])))
    .unwrap()
    .join()
    .unwrap();
  assert_eq!(within.map(|html| html.matches("<div>").count()), Ok(MAX_CALLS + 1), "the root and {MAX_CALLS} nested below it render");
  assert!(past.as_ref().is_err_and(|message| message.contains(&format!("nested {MAX_CALLS} components deep"))), "{past:?}");
}

#[test]
fn a_default_exported_function_is_a_component_under_its_own_name() {
  let set = lower(
    &[("routes/page.tsx", "export default function Card({ title }: { title: string }) {\n  return <h2>{title}</h2>;\n}\nexport function Deck() {\n  return <section><Card title=\"a\" /></section>;\n}\n")],
    "routes/page.tsx#Deck",
  );
  let modules: Vec<&str> = set.components.iter().map(|(m, _)| m.as_str()).collect();
  assert!(modules.contains(&"routes/page.tsx#default"), "Card is the default export, lowered once under that id: {modules:?}");
  assert!(!modules.contains(&"routes/page.tsx#Card"), "and not a second time under its name: {modules:?}");
}

#[test]
fn a_default_exported_function_is_a_value_under_its_own_name() {
  let dir = app(&[("routes/page.tsx", "export default function double(n: number) {\n  return n * 2;\n}\nexport function Page() {\n  return <p>{double(2)}</p>;\n}\n")]);
  let lowered = ComponentSet::new(&dir).lower("routes/page.tsx#Page");
  assert!(lowered.is_ok(), "{:?}", lowered.err().map(|e| e.to_string()));
}

#[test]
fn a_default_exported_component_that_renders_itself_calls_itself_by_its_default_id() {
  let set = lower(&[("routes/page.tsx", "export default function Tree({ depth }: { depth: number }) {\n  return <ul>{depth > 0 ? <Tree depth={depth - 1} /> : null}</ul>;\n}\n")], "routes/page.tsx#default");
  assert_eq!(render(&library(&set), "routes/page.tsx#default", &[("depth", Value::F64(1.0))]).unwrap(), "<ul><ul></ul></ul>");
}

const BODY: &str = "async ({ input }: ActionCtx<AddInput>) => ({ n: input.n })";

#[test]
fn an_action_is_recognised_by_the_name_it_was_imported_under() {
  let plain = lower_actions("a.ts", &format!("import {{ action }} from \"@snapfire/fsr\";\nexport const add = action({BODY});\n")).unwrap();
  let aliased = lower_actions("a.ts", &format!("import {{ action as act }} from \"@snapfire/fsr\";\nexport const add = act({BODY});\n")).unwrap();
  let namespaced = lower_actions("a.ts", &format!("import * as fsr from \"@snapfire/fsr\";\nexport const add = fsr.action({BODY});\n")).unwrap();

  for (form, lowered) in [("aliased", &aliased), ("namespaced", &namespaced)] {
    assert_eq!(lowered.len(), 1, "an {form} `action` is still an action: {lowered:?}");
    assert_eq!(lowered[0].export, "add");
    assert_eq!(lowered[0].input.as_deref(), Some("AddInput"), "and still declares its input");
    assert_eq!(lowered[0].body, plain[0].body, "with the body the bare form lowers to");
  }
}

#[test]
fn an_action_is_found_however_the_module_exports_it() {
  let beside = lower_actions("a.ts", &format!("import {{ action }} from \"@snapfire/fsr\";\nexport const add = action({BODY}), drop = action({BODY});\n")).unwrap();
  assert_eq!(beside.iter().map(|a| a.export.as_str()).collect::<Vec<_>>(), ["add", "drop"], "both declarators of one `export const`: {beside:?}");

  let apart = lower_actions("a.ts", &format!("import {{ action }} from \"@snapfire/fsr\";\nconst add = action({BODY});\nexport {{ add }};\n")).unwrap();
  assert_eq!(apart.len(), 1, "a declaration and an `export {{ … }}` that names it: {apart:?}");
  assert_eq!(apart[0].export, "add");

  let renamed = lower_actions("a.ts", &format!("import {{ action }} from \"@snapfire/fsr\";\nconst add = action({BODY});\nexport {{ add as plus }};\n")).unwrap();
  assert_eq!(renamed[0].export, "plus", "the exported name is the action's, not the local one: {renamed:?}");
}

#[test]
fn a_body_may_return_from_a_concise_arrow_as_meta_and_store_always_could() {
  let loader = lower_loader("l.ts", "export const load = async ({ params }) => ({ id: params.id });\n").unwrap();
  assert_eq!(loader, vec![Stmt::Return(Expr::Object(vec![Entry::Field("id".to_owned(), Expr::Param("id".to_owned()))]))], "{loader:?}");

  let handler = lower_handlers("route.ts", "export const GET = async ({ params }) => ({ id: params.id });\n").unwrap();
  assert_eq!(handler.len(), 1);
  assert_eq!(handler[0].method, "GET");
  assert_eq!(handler[0].body, loader, "a route handler reads the same as a loader: {handler:?}");

  let middleware = lower_middleware("middleware.ts", "export const middleware = ({ request }) => ({ redirect: request.path });\n").unwrap();
  assert_eq!(middleware.len(), 1, "{middleware:?}");
}

#[test]
fn fail_is_a_guard_under_whatever_name_it_was_imported() {
  let plain = lower_loader("l.ts", "import { fail } from \"@snapfire/fsr\";\nexport async function load({ params }) {\n  if (!params.id) fail(\"invalid\", \"no id\");\n  return { id: params.id };\n}\n").unwrap();
  let aliased = lower_loader("l.ts", "import { fail as bail } from \"@snapfire/fsr\";\nexport async function load({ params }) {\n  if (!params.id) bail(\"invalid\", \"no id\");\n  return { id: params.id };\n}\n").unwrap();
  let namespaced = lower_loader("l.ts", "import * as fsr from \"@snapfire/fsr\";\nexport async function load({ params }) {\n  if (!params.id) fsr.fail(\"invalid\", \"no id\");\n  return { id: params.id };\n}\n").unwrap();

  assert!(matches!(plain.first(), Some(Stmt::Guard { .. })), "{plain:?}");
  assert_eq!(aliased, plain, "an aliased `fail` guards: {aliased:?}");
  assert_eq!(namespaced, plain, "a namespaced `fail` guards: {namespaced:?}");
}

#[test]
fn a_default_export_that_names_a_local_declaration_is_the_component() {
  let arrow = lower(&[("routes/a/page.tsx", "const Page = ({ n }: { n: number }) => <p>{n}</p>;\nexport default Page;\n")], "routes/a/page.tsx#default");
  let declared = lower(&[("routes/a/page.tsx", "function Page({ n }: { n: number }) {\n  return <p>{n}</p>;\n}\nexport default Page;\n")], "routes/a/page.tsx#default");
  let inline = lower(&[("routes/a/page.tsx", "export default function Page({ n }: { n: number }) {\n  return <p>{n}</p>;\n}\n")], "routes/a/page.tsx#default");

  let expected = render_of(&inline, "routes/a/page.tsx#default");
  assert_eq!(render_of(&arrow, "routes/a/page.tsx#default"), expected, "a const arrow exported below itself");
  assert_eq!(render_of(&declared, "routes/a/page.tsx#default"), expected, "a function declared and exported below itself");
}

#[test]
fn a_dialect_tag_is_read_from_its_import_rather_than_its_spelling() {
  let help = ("src/ui/Help.tsx", "export function Help() {\n  return <p>help</p>;\n}\n");
  let island = lower(
    &[
      help,
      ("routes/a/page.tsx", "import { Island as Zone } from \"@snapfire/fsr-client/react\";\nimport { Help } from \"../../src/ui/Help\";\nexport default function A() {\n  return <Zone when=\"idle\"><Help /></Zone>;\n}\n"),
    ],
    "routes/a/page.tsx#default",
  );
  assert_eq!(island_in(render_of(&island, "routes/a/page.tsx#default")), Some(("src/ui/Help.tsx#Help", Some("idle"), None)));

  let link = lower(&[("routes/a/page.tsx", "import { Link as A } from \"@snapfire/fsr-client/react\";\nexport default function P() {\n  return <A href=\"/x\">x</A>;\n}\n")], "routes/a/page.tsx#default");
  let Tmpl::Element { tag, attrs, .. } = render_of(&link, "routes/a/page.tsx#default") else { panic!("{:?}", render_of(&link, "routes/a/page.tsx#default")) };
  assert_eq!(tag, "a", "an aliased `Link` is still an anchor");
  assert_eq!(attrs[0], Entry::Field("href".to_owned(), Expr::lit_str("/x")));

  let mut slot = ComponentSet::new(&app(&[("routes/layout.tsx", "import { Slot as Region } from \"@snapfire/fsr-client/react\";\nexport default function L({ children }: { children: unknown }) {\n  return <div>{children}<Region name=\"modal\" /></div>;\n}\n")]));
  slot.layouts.push("routes/layout.tsx#default".to_owned());
  slot.slots.push(("routes/layout.tsx#default".to_owned(), vec!["modal".to_owned()]));
  slot.lower("routes/layout.tsx#default").unwrap();
  let Tmpl::Element { children, .. } = render_of(&slot, "routes/layout.tsx#default") else { panic!("{:?}", render_of(&slot, "routes/layout.tsx#default")) };
  assert_eq!(children[1], Tmpl::Element { tag: "sf-s".to_owned(), attrs: vec![Entry::Field("data-sf-name".to_owned(), Expr::lit_str("modal"))], children: vec![Tmpl::Slot("modal".to_owned())] }, "an aliased `Slot` is still a slot: {children:?}");
}

#[test]
fn an_aliased_island_declaration_still_places_the_island() {
  let set = lower(
    &[
      ("src/ui/Help.tsx", "import { island as isle } from \"@snapfire/fsr-client/react\";\nexport function Help() {\n  return <p>help</p>;\n}\nexport const Lazy = isle(Help, { when: \"idle\" });\n"),
      ("routes/a/page.tsx", "import { Lazy } from \"../../src/ui/Help\";\nexport default function A() {\n  return <main><Lazy /></main>;\n}\n"),
    ],
    "routes/a/page.tsx#default",
  );
  assert_eq!(island_in(render_of(&set, "routes/a/page.tsx#default")), Some(("src/ui/Help.tsx#Help", Some("idle"), None)));
}

#[test]
fn an_aliased_action_import_in_a_handler_still_lowers_to_act() {
  let set = lower(
    &[
      ("src/Lot.tsx", "import { action as act } from \"@snapfire/fsr-client\";\nconst save = act(\"desk.save\");\nexport function Lot() {\n  return <button onClick={() => void save({ by: 1 })}>s</button>;\n}\n"),
      ("routes/a/page.tsx", "import { Island } from \"@snapfire/fsr-client/react\";\nimport { Lot } from \"../../src/Lot\";\nexport default function A() {\n  return <Island mode=\"server\"><Lot /></Island>;\n}\n"),
    ],
    "routes/a/page.tsx#default",
  );
  let (_, lot) = set.components.iter().find(|(m, _)| m == "src/Lot.tsx#Lot").unwrap();
  assert_eq!(lot.handlers.len(), 1, "{lot:?}");
  assert!(matches!(&lot.handlers[0].body[0], Stmt::Act { action, .. } if action == "desk.save"), "{:?}", lot.handlers[0]);
  assert!(!format!("{:?}", lot.render).contains("$unlowered"), "nothing fell to a browser the server island has no module for: {:?}", lot.render);
}

#[test]
fn an_aliased_store_key_is_still_the_key_it_names() {
  let set = lower(
    &[
      ("src/store.ts", "import { key as k } from \"@snapfire/fsr-client/store\";\nexport const cartCount = k<number>(\"cart/count\");\n"),
      ("routes/a/page.tsx", "import { useStore } from \"@snapfire/fsr-client/react\";\nimport { cartCount } from \"../../src/store\";\nexport default function P() {\n  const [n] = useStore(cartCount, 0);\n  return <p>{n}</p>;\n}\n"),
    ],
    "routes/a/page.tsx#default",
  );
  let (_, page) = set.components.iter().find(|(m, _)| m == "routes/a/page.tsx#default").unwrap();
  assert!(format!("{page:?}").contains("cart/count"), "the key the aliased declaration names reaches the component: {page:?}");
  assert!(!format!("{page:?}").contains("$unlowered"), "{page:?}");
}

#[test]
fn a_handler_that_reaches_an_action_through_a_binding_the_build_cannot_follow_is_refused() {
  let mut set = ComponentSet::new(&app(&[
    ("src/Lot.tsx", "import { actions } from \"../generated/client\";\nconst { $root } = actions;\nexport function Lot() {\n  return <button onClick={() => void $root.play({})}>s</button>;\n}\n"),
    ("routes/a/page.tsx", "import { Island } from \"@snapfire/fsr-client/react\";\nimport { Lot } from \"../../src/Lot\";\nexport default function A() {\n  return <Island mode=\"server\"><Lot /></Island>;\n}\n"),
  ]));
  set.lower("routes/a/page.tsx#default").unwrap();
  let (_, lot) = set.components.iter().find(|(m, _)| m == "src/Lot.tsx#Lot").unwrap();
  let Tmpl::Element { attrs, .. } = &lot.render else { panic!("{:?}", lot.render) };
  let why = attrs.iter().find_map(|e| match e {
    Entry::Field(n, Expr::Lit(Lit::Str(why))) if n == "$unlowered" => Some(why.as_str()),
    _ => None,
  });
  assert!(why.is_some_and(|w| w.contains("`.play()` in a handler")), "destructuring the generated client is refused rather than silently dropped: {attrs:?}");
}

#[test]
fn a_hook_reached_through_a_namespace_import_is_still_that_hook() {
  let bare = lower(&[("routes/a/page.tsx", "import { useState } from \"react\";\nexport default function P() {\n  const [n, setN] = useState(0);\n  return <button onClick={() => setN(n + 1)}>{n}</button>;\n}\n")], "routes/a/page.tsx#default");
  let namespaced = lower(&[("routes/a/page.tsx", "import * as React from \"react\";\nexport default function P() {\n  const [n, setN] = React.useState(0);\n  return <button onClick={() => setN(n + 1)}>{n}</button>;\n}\n")], "routes/a/page.tsx#default");
  assert_eq!(render_of(&namespaced, "routes/a/page.tsx#default"), render_of(&bare, "routes/a/page.tsx#default"), "`React.useState` is `useState`");

  let store = lower(
    &[
      ("src/store.ts", "import { key } from \"@snapfire/fsr-client/store\";\nexport const cartCount = key<number>(\"cart/count\");\n"),
      ("routes/a/page.tsx", "import * as sf from \"@snapfire/fsr-client/react\";\nimport { cartCount } from \"../../src/store\";\nexport default function P() {\n  const [n] = sf.useStore(cartCount, 0);\n  return <p>{n}</p>;\n}\n"),
    ],
    "routes/a/page.tsx#default",
  );
  let (_, page) = store.components.iter().find(|(m, _)| m == "routes/a/page.tsx#default").unwrap();
  assert!(format!("{page:?}").contains("cart/count"), "and so is a namespaced `useStore`: {page:?}");
}

#[test]
fn a_method_call_on_a_local_object_is_not_mistaken_for_a_hook() {
  let mut set = ComponentSet::new(&app(&[("routes/a/page.tsx", "const box = { useThing: () => 1 };\nexport default function P() {\n  const n = box.useThing();\n  return <p>{n}</p>;\n}\n")]));
  let err = set.lower("routes/a/page.tsx#default").unwrap_err().to_string();
  assert!(!err.contains("`useThing`"), "the hook rule only follows a namespace import, so this is an ordinary unfollowable call: {err}");
}

#[test]
fn an_actions_module_export_the_build_cannot_read_is_an_error() {
  let unknown = lower_actions("a.ts", &format!("import {{ action }} from \"@snapfire/fsr\";\nexport const add = action({BODY});\nexport const drop = wrap(async () => 1);\n")).unwrap_err().to_string();
  assert!(unknown.contains("`drop`") && unknown.contains("does not recognise"), "an unrecognised call is refused rather than skipped: {unknown}");

  let foreign = lower_actions("a.ts", "import { pay } from \"./shared\";\nexport { pay };\n").unwrap_err().to_string();
  assert!(foreign.contains("`pay`") && foreign.contains("does not declare"), "a name this module does not declare is refused: {foreign}");

  let reexport = lower_actions("a.ts", "export { pay } from \"./shared\";\n").unwrap_err().to_string();
  assert!(reexport.contains("`pay`") && reexport.contains("does not declare"), "so is a re-export: {reexport}");

  let destructured = lower_actions("a.ts", "const bundle = { pay: 1, refund: 2 };\nexport const { pay, refund } = bundle;\n").unwrap_err().to_string();
  assert!(destructured.contains("`pay`") && destructured.contains("destructuring"), "so is a destructuring export: {destructured}");
}

#[test]
fn an_actions_module_still_passes_over_what_is_plainly_not_an_action() {
  let lowered = lower_actions(
    "a.ts",
    &format!("import {{ action }} from \"@snapfire/fsr\";\nexport type Pair = {{ a: number }};\nexport interface Row {{ b: string }}\nexport const LIMIT = 10;\nexport const TAGS = [\"a\"];\nexport function helper(n: number) {{ return n; }}\nexport const add = action({BODY});\n"),
  )
  .unwrap();
  assert_eq!(lowered.iter().map(|a| a.export.as_str()).collect::<Vec<_>>(), ["add"], "a type, a literal, an array and a plain function are skipped on what they are: {lowered:?}");
}
#[test]
fn a_session_module_refuses_a_defaults_export_the_build_cannot_read() {
  let read = read_session_defaults("session.ts", "export const defaults = { cart: {}, tip: 0 };\n").unwrap();
  assert_eq!(read.len(), 2, "the shape the build reads still reads: {read:?}");

  let apart = read_session_defaults("session.ts", "const defaults = { tip: 0 };\nexport { defaults };\n").unwrap_err().to_string();
  assert!(apart.contains("apart from its declaration"), "an export specifier is refused rather than leaving the session silently empty: {apart}");

  let destructured = read_session_defaults("session.ts", "const both = { defaults: {} };\nexport const { defaults } = both;\n").unwrap_err().to_string();
  assert!(destructured.contains("destructuring"), "so is a destructuring export: {destructured}");
}
