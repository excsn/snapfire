//! Every spelling TypeScript allows for one feature, crossed with every other.
//! A feature matched by its local name or by one declaration shape lowers to
//! nothing for the spellings it does not match, so each combination must
//! lower to what the plainest spelling lowers to.

use snapfire_fsr_core::{Params, Value};
use snapfire_fsr_ir::{Body, Interpreter};
use snapfire_fsr_lower::{lower_actions, lower_loader};
use snapfire_fsr_runtime::RequestCtx;

/// How the module reaches `action`, and the callee that spelling calls.
const ACTION_IMPORTS: &[(&str, &str)] = &[
  ("import { action } from \"@snapfire/fsr\";", "action"),
  ("import { action as act } from \"@snapfire/fsr\";", "act"),
  ("import * as fsr from \"@snapfire/fsr\";", "fsr.action"),
  ("import { fail, action } from \"@snapfire/fsr\";", "action"),
  ("import type { ActionCtx } from \"@snapfire/fsr\";\nimport { action } from \"@snapfire/fsr\";", "action"),
];

const ACTION_BODIES: &[&str] = &[
  "async ({ input }: ActionCtx<AddInput>) => ({ n: input.n })",
  "async ({ input }: ActionCtx<AddInput>) => { return { n: input.n }; }",
  "async function ({ input }: ActionCtx<AddInput>) { return { n: input.n }; }",
];

/// Two actions, `add` and `drop`, declared and exported each way a module can.
fn action_modules(call: &str, body: &str) -> Vec<(&'static str, String)> {
  let c = |_: ()| format!("{call}({body})");
  let x = c(());
  vec![
    ("one export each", format!("export const add = {x};\nexport const drop = {x};\n")),
    ("two declarators", format!("export const add = {x}, drop = {x};\n")),
    ("an export list", format!("const add = {x};\nconst drop = {x};\nexport {{ add, drop }};\n")),
    ("renamed in the list", format!("const a1 = {x};\nconst d1 = {x};\nexport {{ a1 as add, d1 as drop }};\n")),
    ("two lists", format!("const add = {x};\nconst drop = {x};\nexport {{ add }};\nexport {{ drop }};\n")),
    ("declarators then a list", format!("const add = {x}, drop = {x};\nexport {{ add, drop }};\n")),
    ("inline and listed", format!("export const add = {x};\nconst drop = {x};\nexport {{ drop }};\n")),
  ]
}

#[test]
fn every_spelling_of_an_action_lowers_like_the_plainest() {
  let reference = lower_actions("a.ts", &format!("import {{ action }} from \"@snapfire/fsr\";\nexport const add = action({});\n", ACTION_BODIES[0])).unwrap();
  let mut failed = Vec::new();
  let mut tried = 0;
  for (import, call) in ACTION_IMPORTS {
    for body in ACTION_BODIES {
      for (shape, exports) in action_modules(call, body) {
        tried += 1;
        let source = format!("{import}\n{exports}");
        let what = format!("{call} / {shape} / {body}");
        match lower_actions("a.ts", &source) {
          Err(e) => failed.push(format!("{what}: refused: {e}")),
          Ok(lowered) => {
            let mut names: Vec<&str> = lowered.iter().map(|a| a.export.as_str()).collect();
            names.sort();
            if names != ["add", "drop"] {
              failed.push(format!("{what}: lowered {names:?}"));
            } else if let Some(a) = lowered.iter().find(|a| a.input.as_deref() != Some("AddInput") || a.body != reference[0].body) {
              failed.push(format!("{what}: `{}` lowered differently: {:?} {:?}", a.export, a.input, a.body));
            }
          }
        }
      }
    }
  }
  assert!(failed.is_empty(), "{} of {tried} spellings:\n{}", failed.len(), failed.join("\n"));
}

/// `load`, returning `{ id }` from the route's params, written each way a module can.
const LOADERS: &[(&str, &str)] = &[
  ("an exported function", "export async function load({ params }) {\n  return { id: params.id };\n}\n"),
  ("an exported arrow", "export const load = async ({ params }) => ({ id: params.id });\n"),
  ("an arrow with a block", "export const load = async ({ params }) => {\n  return { id: params.id };\n};\n"),
  ("an exported function expression", "export const load = async function ({ params }) {\n  return { id: params.id };\n};\n"),
  ("a function in an export list", "async function load({ params }) {\n  return { id: params.id };\n}\nexport { load };\n"),
  ("a function renamed in the list", "async function fetchIt({ params }) {\n  return { id: params.id };\n}\nexport { fetchIt as load };\n"),
  ("an arrow in an export list", "const load = async ({ params }) => ({ id: params.id });\nexport { load };\n"),
  ("the context by name", "export async function load(ctx) {\n  return { id: ctx.params.id };\n}\n"),
  ("the params destructured inside", "export async function load(ctx) {\n  const { params } = ctx;\n  return { id: params.id };\n}\n"),
  ("a typed context", "import type { LoaderCtx } from \"@snapfire/fsr\";\nexport async function load({ params }: LoaderCtx) {\n  return { id: params.id };\n}\n"),
  ("a field shorthand", "export async function load({ params }) {\n  const id = params.id;\n  return { id };\n}\n"),
  ("the context under another name", "export async function load(ctx) {\n  const c = ctx;\n  return { id: c.params.id };\n}\n"),
  ("an arrow in parentheses", "export const load = (async ({ params }) => ({ id: params.id }));\n"),
  ("an arrow that satisfies a type", "import type { Loader } from \"@snapfire/fsr\";\nexport const load = (async ({ params }) => ({ id: params.id })) satisfies Loader;\n"),
  ("an arrow cast to a type", "import type { Loader } from \"@snapfire/fsr\";\nexport const load = (async ({ params }) => ({ id: params.id })) as Loader;\n"),
  ("a typed binding", "import type { Loader } from \"@snapfire/fsr\";\nexport const load: Loader = async ({ params }) => ({ id: params.id });\n"),
];

/// What `body` returns for the route `/item/7`.
fn returned(body: &Body) -> Result<Value, String> {
  let mut params = Params::new();
  params.insert("id".to_owned(), "7".to_owned());
  let ctx = RequestCtx::anonymous(params);
  futures::executor::block_on(Interpreter::default().run(body, &ctx, None)).map(|outcome| outcome.value).map_err(|fail| fail.message)
}

#[test]
fn every_spelling_of_a_loader_returns_what_the_plainest_returns() {
  let reference = returned(&lower_loader("l.ts", LOADERS[0].1).unwrap()).unwrap();
  let failed: Vec<String> = LOADERS
    .iter()
    .filter_map(|(what, source)| match lower_loader("l.ts", source).map_err(|e| e.to_string()).and_then(|body| returned(&body)) {
      Err(e) => Some(format!("{what}: {e}")),
      Ok(value) if value != reference => Some(format!("{what}: returned {value:?}")),
      Ok(_) => None,
    })
    .collect();
  assert!(failed.is_empty(), "{} of {} spellings, against {reference:?}:\n{}", failed.len(), LOADERS.len(), failed.join("\n"));
}
