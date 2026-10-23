//! Hooks whose value React's server renderer gives without waiting on the
//! browser render that value on the server.

use std::path::PathBuf;
use std::sync::Arc;

use snapfire_fsr_core::ValueMap;
use snapfire_fsr_ir::render::{prepare, Components};
use snapfire_fsr_ir::Interpreter;
use snapfire_fsr_lower::component::ComponentSet;

fn app(tag: &str, files: &[(&str, &str)]) -> PathBuf {
  let dir = std::env::temp_dir().join(format!("fsr_server_hooks_{}_{tag}", std::process::id()));
  let _ = std::fs::remove_dir_all(&dir);
  for (name, text) in files {
    let path = dir.join(name);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, text).unwrap();
  }
  dir
}

fn html(tag: &str, files: &[(&str, &str)]) -> Result<String, String> {
  let dir = app(tag, files);
  let mut set = ComponentSet::new(&dir);
  let result = set.lower("routes/page.tsx#default").map_err(|e| e.to_string());
  std::fs::remove_dir_all(&dir).unwrap();
  result?;
  set.settle_fills();
  if let Some((module, residue)) = set.browser_only.first() {
    return Err(format!("{module}: {}", residue.message));
  }
  let library: Components = set.components.iter().map(|(module, component)| (module.clone(), Arc::new(prepare(component)))).collect();
  let module = "routes/page.tsx#default";
  Interpreter::default().with_consts(Some(Arc::new(set.consts.clone()))).render_module(module, &library[module], &ValueMap::default(), &library).map(|r| r.html).map_err(|e| format!("{e:?}"))
}

fn page(body: &str) -> Vec<(&'static str, String)> {
  vec![("routes/page.tsx", format!("import {{ useActionState, useDeferredValue, useEffectEvent, useOptimistic, useSyncExternalStore, useTransition }} from \"react\";\nimport {{ useFormStatus }} from \"react-dom\";\n{body}"))]
}

fn render(tag: &str, body: &str) -> Result<String, String> {
  let files = page(body);
  let files: Vec<(&str, &str)> = files.iter().map(|(n, t)| (*n, t.as_str())).collect();
  html(tag, &files)
}

#[test]
fn use_transition_is_not_pending() {
  let out = render("transition", "export default function Page() {\n  const [isPending, startTransition] = useTransition();\n  return <button onClick={() => startTransition(() => {})}>{isPending ? \"wait\" : \"go\"}</button>;\n}\n");
  assert_eq!(out.as_deref(), Ok("<button>go</button>"));
}

#[test]
fn use_action_state_is_its_initial_state_and_not_pending() {
  let out = render("action_state", "async function add(prev, data) {\n  return prev + 1;\n}\nexport default function Page() {\n  const [count, formAction, isPending] = useActionState(add, 3);\n  return <form action={formAction}><p>{count}</p>{isPending ? <i>saving</i> : null}<button>Add</button></form>;\n}\n");
  assert_eq!(out.as_deref(), Ok("<form action=\"javascript:throw new Error('React form unexpectedly submitted.')\"><p>3</p><button>Add</button></form>"));
}

#[test]
fn use_optimistic_is_the_value_passed() {
  let out = render("optimistic", "export default function Page() {\n  const [name, setName] = useOptimistic(\"Ada\");\n  return <p>{name}</p>;\n}\n");
  assert_eq!(out.as_deref(), Ok("<p>Ada</p>"));
}

#[test]
fn use_deferred_value_is_its_initial_value_when_given() {
  let out = render("deferred", "export default function Page() {\n  const q = useDeferredValue(\"typed\");\n  const r = useDeferredValue(\"typed\", \"\");\n  return <p>{q}|{r}</p>;\n}\n");
  assert_eq!(out.as_deref(), Ok("<p>typed|</p>"));
}

#[test]
fn use_effect_event_is_a_function_the_server_never_calls() {
  let out = render("effect_event", "export default function Page() {\n  const onTick = useEffectEvent(() => console.log(1));\n  return <p>tick</p>;\n}\n");
  assert_eq!(out.as_deref(), Ok("<p>tick</p>"));
}

#[test]
fn use_form_status_is_no_form_pending() {
  let out = render("form_status", "export default function Page() {\n  const { pending, data } = useFormStatus();\n  const status = useFormStatus();\n  return <button disabled={pending}>{data === null && !status.pending ? \"idle\" : \"busy\"}</button>;\n}\n");
  assert_eq!(out.as_deref(), Ok("<button>idle</button>"));
}

#[test]
fn use_sync_external_store_reads_the_server_snapshot() {
  let out = render("store", "function subscribe(cb) {\n  return () => {};\n}\nexport default function Page() {\n  const online = useSyncExternalStore(subscribe, () => navigator.onLine, () => true);\n  return <p>{online ? \"online\" : \"offline\"}</p>;\n}\n");
  assert_eq!(out.as_deref(), Ok("<p>online</p>"));
  let hook = "import { useSyncExternalStore } from \"react\";\nfunction subscribe(cb) {\n  return () => {};\n}\nexport function useOnline() {\n  return useSyncExternalStore(subscribe, () => navigator.onLine, () => false);\n}\n";
  let out = html("store_hook", &[("src/useOnline.ts", hook), ("routes/page.tsx", "import { useOnline } from \"@src/useOnline\";\nexport default function Page() {\n  const online = useOnline();\n  return <p>{online ? \"online\" : \"offline\"}</p>;\n}\n")]);
  assert_eq!(out.as_deref(), Ok("<p>offline</p>"));
}

#[test]
fn use_sync_external_store_without_a_server_snapshot_renders_in_the_browser() {
  let out = render("store_none", "function subscribe(cb) {\n  return () => {};\n}\nexport default function Page() {\n  const online = useSyncExternalStore(subscribe, () => navigator.onLine);\n  return <p>{online ? \"online\" : \"offline\"}</p>;\n}\n");
  assert!(out.as_ref().is_err_and(|e| e.contains("without `getServerSnapshot`")), "{out:?}");
}

#[test]
fn a_call_on_use_form_status_data_is_null_since_no_form_is_pending() {
  let out = render("form_status_data", "export default function Page() {\n  const { pending, data } = useFormStatus();\n  return <p>{data ? `Requesting ${data?.get(\"username\")}...` : \"idle\"}{pending ? \"!\" : \"\"}</p>;\n}\n");
  assert_eq!(out.as_deref(), Ok("<p>idle</p>"));
}

#[test]
fn use_link_status_is_no_link_pending() {
  let out = html("link_status", &[("routes/page.tsx", "import { useLinkStatus } from \"@snapfire/fsr-client/react\";\nexport default function Page() {\n  const { pending } = useLinkStatus();\n  const status = useLinkStatus();\n  return <span className={pending ? \"on\" : \"off\"}>{status.pending ? \"opening\" : \"\"}</span>;\n}\n")]);
  assert_eq!(out.as_deref(), Ok("<span class=\"off\"></span>"));
  let dir = app("link_status_owner", &[("routes/page.tsx", "import { useLinkStatus } from \"@snapfire/fsr-client/react\";\nexport default function Page() {\n  const { pending } = useLinkStatus();\n  return <span>{pending ? \"opening\" : \"\"}</span>;\n}\n")]);
  let mut set = ComponentSet::new(&dir);
  set.lower("routes/page.tsx#default").unwrap();
  std::fs::remove_dir_all(&dir).unwrap();
  let (_, page) = set.components.iter().find(|(m, _)| m == "routes/page.tsx#default").unwrap();
  assert_eq!(page.owner, snapfire_fsr_ir::ast::Owner::React, "the browser re-renders it when a link's navigation starts");
}
