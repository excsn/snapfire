//! A helper a loader hands `services` to: a parameter typed `Ctx["services"]` is the services root inside the helper, so its calls stay named in the plan.

use std::path::PathBuf;
use std::sync::Arc;

use futures::future::BoxFuture;
use snapfire_fsr_core::{Value, ValueMap};
use snapfire_fsr_ir::Interpreter;
use snapfire_fsr_ir::ast::Body;
use snapfire_fsr_lower::component::ComponentSet;
use snapfire_fsr_runtime::{FailureKind, RequestCtx, ServiceCaller, ServiceError, ServiceHandle};

struct Shop;

impl ServiceCaller for Shop {
  fn call(&self, service: &str, method: &str, _args: ValueMap) -> BoxFuture<'static, Result<Value, ServiceError>> {
    let (service, method) = (service.to_owned(), method.to_owned());
    Box::pin(async move {
      match (service.as_str(), method.as_str()) {
        ("shop", "get") => {
          let mut found = ValueMap::default();
          found.insert("stars".to_owned(), Value::Int(7));
          Ok(Value::Map(found))
        }
        _ => Err(ServiceError::new(FailureKind::Unavailable, service, method, "the shop is closed")),
      }
    })
  }
}

fn run(body: &Body) -> ValueMap {
  let mut ctx = RequestCtx::anonymous(Default::default());
  ctx.services = ServiceHandle::new(Arc::new(Shop));
  match futures::executor::block_on(Interpreter::default().run(body, &ctx, None)).map(|outcome| outcome.value) {
    Ok(Value::Map(fields)) => fields,
    other => panic!("{other:?}"),
  }
}

const HELPER: &str = "async function stars(services: Ctx[\"services\"]): Promise<number | null> {\n  try {\n    const found = await services.shop.get({});\n    return Number(found.stars);\n  } catch {\n    return null;\n  }\n}\nasync function closed(services: Ctx[\"services\"]): Promise<number | null> {\n  try {\n    await services.shop.list({});\n    return 1;\n  } catch {\n    return null;\n  }\n}\n";

#[test]
fn a_helper_whose_parameter_is_typed_ctx_services_calls_services_through_it() {
  let loader = format!("import type {{ Ctx }} from \"@snapfire/fsr/host\";\n{HELPER}export async function load({{ services }}: Ctx) {{\n  return {{ open: await stars(services), shut: await closed(services) }};\n}}\n");
  let body = snapfire_fsr_lower::lower_loader("page.loader.ts", &loader).unwrap_or_else(|e| panic!("{e}"));
  let fields = run(&body);
  assert_eq!(fields.get("open"), Some(&Value::F64(7.0)), "{fields:?}");
  assert_eq!(fields.get("shut"), Some(&Value::Null), "a failed call is caught inside the helper: {fields:?}");
}

#[test]
fn a_loader_hands_ctx_services_to_a_helper_without_destructuring_the_context() {
  let loader = format!("import type {{ Ctx }} from \"@snapfire/fsr/host\";\n{HELPER}export async function load(ctx: Ctx) {{\n  return {{ open: await stars(ctx.services) }};\n}}\n");
  let body = snapfire_fsr_lower::lower_loader("page.loader.ts", &loader).unwrap_or_else(|e| panic!("{e}"));
  assert_eq!(run(&body).get("open"), Some(&Value::F64(7.0)));
}

#[test]
fn an_imported_helper_typed_ctx_services_calls_services_through_it() {
  let dir: PathBuf = snapfire_fsr_test_internal::scratch_root().join(format!("fsr_service_helpers_{}", std::process::id()));
  let _ = std::fs::remove_dir_all(&dir);
  std::fs::create_dir_all(dir.join("src")).unwrap();
  std::fs::create_dir_all(dir.join("routes")).unwrap();
  std::fs::write(dir.join("src/stars.ts"), format!("import type {{ Ctx }} from \"@snapfire/fsr/host\";\nexport {HELPER}")).unwrap();
  std::fs::write(dir.join("routes/page.loader.ts"), "import type { Ctx } from \"@snapfire/fsr/host\";\nimport { stars } from \"../src/stars\";\nexport async function load({ services }: Ctx) {\n  return { open: await stars(services) };\n}\n").unwrap();
  let mut set = ComponentSet::new(&dir);
  let body = set.lower_loader("routes/page.loader.ts");
  std::fs::remove_dir_all(&dir).unwrap();
  let body = body.unwrap_or_else(|e| panic!("{e}"));
  assert_eq!(run(&body).get("open"), Some(&Value::F64(7.0)));
}

#[test]
fn a_helper_whose_parameter_is_not_typed_ctx_services_cannot_call_services_through_it() {
  let loader = "import type { Ctx } from \"@snapfire/fsr/host\";\nasync function stars(s: any) {\n  const found = await s.shop.get({});\n  return found.stars;\n}\nexport async function load({ services }: Ctx) {\n  return { open: await stars(services) };\n}\n";
  let refused = snapfire_fsr_lower::lower_loader("page.loader.ts", loader).unwrap_err().to_string();
  assert!(refused.contains("page.loader.ts:3:"), "the helper's call through an untyped parameter is refused where it stands: {refused}");
}
