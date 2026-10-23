//! A route handler answers with a `Response`: text, bytes, JSON with a status of its own or a redirect.

use snapfire_fsr_core::Value;
use snapfire_fsr_ir::Interpreter;
use snapfire_fsr_runtime::{FailureKind, HttpResponse, RequestCtx};

fn answered(route: &str, method: &str) -> Result<Value, (FailureKind, String)> {
  let handlers = snapfire_fsr_lower::lower_handlers("routes/api/route.ts", route).unwrap_or_else(|e| panic!("{e}"));
  let handler = handlers.iter().find(|h| h.method == method).unwrap_or_else(|| panic!("no {method}"));
  let ctx = RequestCtx::anonymous(Default::default());
  futures::executor::block_on(Interpreter::default().run(&handler.body, &ctx, None)).map(|outcome| outcome.value).map_err(|fail| (fail.kind, fail.message))
}

fn response(route: &str, method: &str) -> HttpResponse {
  match answered(route, method) {
    Ok(value) => match HttpResponse::from_value(&value) {
      Some(Ok(response)) => response,
      other => panic!("not a response: {other:?} from {value:?}"),
    },
    Err(failed) => panic!("{failed:?}"),
  }
}

fn header<'r>(response: &'r HttpResponse, name: &str) -> Option<&'r str> {
  response.headers.iter().find(|(n, _)| n.eq_ignore_ascii_case(name)).map(|(_, v)| v.as_str())
}

#[test]
fn a_response_of_text_is_its_body_with_its_status_and_headers() {
  let r = response("export function GET() {\n  return new Response(\"User-agent: *\\nAllow: /\\n\", { status: 200, headers: { \"content-type\": \"text/plain\", \"cache-control\": \"max-age=60\" } });\n}\n", "GET");
  assert_eq!(r.status, 200);
  assert_eq!(r.body, b"User-agent: *\nAllow: /\n");
  assert_eq!(header(&r, "content-type"), Some("text/plain"));
  assert_eq!(header(&r, "cache-control"), Some("max-age=60"));
}

#[test]
fn a_string_body_without_a_type_is_plain_text_and_no_body_is_empty() {
  let r = response("export function GET() {\n  return new Response(\"hi\");\n}\n", "GET");
  assert_eq!((r.status, r.body.as_slice()), (200, &b"hi"[..]));
  assert_eq!(header(&r, "content-type"), Some("text/plain;charset=UTF-8"));
  let r = response("export function DELETE() {\n  return new Response(null, { status: 204 });\n}\n", "DELETE");
  assert_eq!((r.status, r.body.len()), (204, 0));
  assert_eq!(header(&r, "content-type"), None);
}

#[test]
fn a_byte_body_is_sent_as_its_bytes() {
  let r = response("export function GET() {\n  const pixel = new Uint8Array([71, 73, 70]);\n  return new Response(pixel, { headers: { \"content-type\": \"image/gif\" } });\n}\n", "GET");
  assert_eq!(r.body, b"GIF");
  assert_eq!(header(&r, "content-type"), Some("image/gif"));
}

#[test]
fn response_json_writes_the_value_with_its_status() {
  let r = response("export function POST() {\n  return Response.json({ id: 7, ok: true }, { status: 201 });\n}\n", "POST");
  assert_eq!(r.status, 201);
  assert_eq!(header(&r, "content-type"), Some("application/json"));
  assert_eq!(std::str::from_utf8(&r.body).unwrap(), r#"{"id":7,"ok":true}"#);
}

#[test]
fn response_redirect_answers_its_status_and_location() {
  let r = response("export function GET() {\n  return Response.redirect(\"https://auth.example/authorize?client=1\", 302);\n}\n", "GET");
  assert_eq!(r.status, 302);
  assert_eq!(header(&r, "location"), Some("https://auth.example/authorize?client=1"));
  let r = response("export function GET() {\n  return Response.redirect(\"/signed-in\");\n}\n", "GET");
  assert_eq!((r.status, header(&r, "location")), (302, Some("/signed-in")));
}

#[test]
fn a_redirect_status_that_is_not_one_fails_as_javascript_does() {
  let failed = answered("export function GET() {\n  return Response.redirect(\"/x\", 200);\n}\n", "GET");
  assert!(failed.as_ref().is_err_and(|(_, message)| message.contains("200")), "{failed:?}");
}

#[test]
fn redirect_in_a_handler_answers_see_other_to_a_path_on_this_origin() {
  let r = response("import { redirect } from \"@snapfire/fsr\";\nexport function POST() {\n  redirect(\"/done\");\n}\n", "POST");
  assert_eq!((r.status, header(&r, "location")), (303, Some("/done")));
  let refused = answered("import { redirect } from \"@snapfire/fsr\";\nexport function POST() {\n  redirect(\"https://elsewhere.example/\");\n}\n", "POST");
  assert!(refused.as_ref().is_err_and(|(kind, _)| *kind == FailureKind::Invalid), "{refused:?}");
}

#[test]
fn a_plain_value_is_still_its_json() {
  let value = answered("export function GET() {\n  return { ok: true };\n}\n", "GET").unwrap();
  assert!(HttpResponse::from_value(&value).is_none(), "{value:?}");
}

mod middleware {
  use std::sync::Arc;

  use parking_lot::Mutex;
  use snapfire_fsr_core::{Value, ValueMap};
  use snapfire_fsr_ir::Interpreter;
  use snapfire_fsr_runtime::{HttpResponse, NextHandle, RequestCtx, Target};

  /// Runs a middleware at `path` with a `next` that answers 200 and records each target it was asked for.
  fn run(source: &str, path: &str) -> (Value, Vec<Target>) {
    let body = snapfire_fsr_lower::lower_middleware("middleware.ts", source).unwrap_or_else(|e| panic!("{e}"));
    let asked = Arc::new(Mutex::new(Vec::new()));
    let mut ctx = RequestCtx::anonymous(Default::default());
    ctx.path = path.to_owned();
    let record = asked.clone();
    let at = path.to_owned();
    ctx.next = Some(NextHandle::new(move |request| {
      let target = Target::of(&request, &at, "").unwrap();
      record.lock().push(target);
      let mut head = HttpResponse::new(200);
      head.downstream = Some(0);
      Box::pin(async move { Ok(head.to_value()) })
    }));
    let mut request = ValueMap::default();
    request.insert("method".to_owned(), Value::str("GET"));
    request.insert("path".to_owned(), Value::str(path));
    let value = futures::executor::block_on(Interpreter::default().run(&body, &ctx, Some(Value::Map(request)))).map(|o| o.value).unwrap_or_else(|f| panic!("{}", f.message));
    let asked = asked.lock().clone();
    (value, asked)
  }

  const MIDDLEWARE: &str = "import type { MiddlewareCtx } from \"@snapfire/fsr\";\n\nexport async function middleware({ request, next }: MiddlewareCtx) {\n  if (request.path === \"/dashboard\") return Response.redirect(\"/\", 307);\n  if (request.path === \"/fleet\") return next(new Request(\"/agents\"));\n  const response = await next();\n  response.headers.set(\"x-ops-console\", \"fsr\");\n  if (response.status === 404) response.headers.set(\"x-missing\", request.path);\n  return response;\n}\n";

  #[test]
  fn a_redirect_is_a_response_of_its_own_and_runs_no_route() {
    let (value, asked) = run(MIDDLEWARE, "/dashboard");
    let response = HttpResponse::from_value(&value).unwrap().unwrap();
    assert_eq!((response.status, response.location(), response.downstream), (307, Some("/"), None));
    assert!(asked.is_empty());
  }

  #[test]
  fn next_with_a_request_runs_the_route_for_its_path() {
    let (value, asked) = run(MIDDLEWARE, "/fleet");
    assert_eq!(asked, [Target { path: "/agents".to_owned(), query: String::new() }]);
    assert_eq!(HttpResponse::from_value(&value).unwrap().unwrap().downstream, Some(0));
  }

  #[test]
  fn the_routes_response_comes_back_with_the_header_set() {
    let (value, asked) = run(MIDDLEWARE, "/help");
    assert_eq!(asked, [Target { path: "/help".to_owned(), query: String::new() }]);
    let response = HttpResponse::from_value(&value).unwrap().unwrap();
    assert_eq!((response.status, response.header("x-ops-console"), response.header("x-missing"), response.downstream), (200, Some("fsr"), None, Some(0)));
  }
}
