//! Middleware's `next`: the rest of the chain runs for the request it names and its response comes back with the body held aside.

use futures::executor::block_on;
use futures::future::BoxFuture;
use snapfire_fsr_core::{Params, Value, ValueMap};
use snapfire_fsr_runtime::{drive_middleware, ActionError, HttpResponse, Passed, RequestCtx, Target};

fn ctx() -> RequestCtx {
  let mut ctx = RequestCtx::anonymous(Params::new());
  ctx.path = "/shop".to_owned();
  ctx
}

/// What the route answers for a target: a 200 naming the path, its body the path as text.
async fn route(target: Target) -> Result<(HttpResponse, String), ActionError> {
  let mut head = HttpResponse::new(200);
  head.headers.push(("x-path".to_owned(), target.path.clone()));
  Ok((head, format!("body of {}", target.path)))
}

fn run(body: impl FnOnce(RequestCtx) -> BoxFuture<'static, Result<Value, ActionError>>) -> Passed<String> {
  block_on(drive_middleware(ctx(), "", body, route)).unwrap()
}

#[test]
fn returning_nothing_without_next_goes_on() {
  assert!(matches!(run(|_| Box::pin(async { Ok(Value::Null) })), Passed::Continue));
}

#[test]
fn a_response_of_its_own_is_the_answer() {
  let answered = run(|_| Box::pin(async { Ok(HttpResponse::new(503).to_value()) }));
  assert!(matches!(answered, Passed::Answer(r) if r.status == 503));
}

#[test]
fn the_routes_response_with_a_header_set_keeps_the_routes_body() {
  let passed = run(|ctx| {
    Box::pin(async move {
      let next = ctx.next.clone().expect("next is set");
      let value = next.call(Value::Null).await?;
      let mut response = HttpResponse::from_value(&value).unwrap().unwrap();
      response.headers.push(("x-frame-options".to_owned(), "DENY".to_owned()));
      Ok(response.to_value())
    })
  });
  let Passed::Route(head, body) = passed else { panic!("the route's answer") };
  assert_eq!((head.status, head.header("x-path"), head.header("x-frame-options")), (200, Some("/shop"), Some("DENY")));
  assert_eq!(body, "body of /shop");
}

#[test]
fn next_with_another_path_runs_the_route_for_it_under_the_same_url() {
  let passed = run(|ctx| {
    Box::pin(async move {
      let mut request = ValueMap::default();
      request.insert("url".to_owned(), Value::str("/catalog?page=2"));
      let request = Value::Variant { tag: snapfire_fsr_runtime::REQUEST_TAG.to_owned(), payload: Some(Box::new(Value::Map(request))) };
      ctx.next.clone().unwrap().call(request).await
    })
  });
  let Passed::Route(head, body) = passed else { panic!("the route's answer") };
  assert_eq!(head.header("x-path"), Some("/catalog"));
  assert_eq!(body, "body of /catalog");
}

#[test]
fn nothing_returned_after_next_is_the_routes_answer_as_it_came() {
  let passed = run(|ctx| {
    Box::pin(async move {
      ctx.next.clone().unwrap().call(Value::str("/a")).await?;
      Ok(Value::Undefined)
    })
  });
  assert!(matches!(passed, Passed::Route(head, body) if head.header("x-path") == Some("/a") && body == "body of /a"));
}

#[test]
fn a_value_that_is_not_a_response_is_refused() {
  let refused = block_on(drive_middleware(ctx(), "", |_| Box::pin(async { Ok(Value::str("go")) }), route));
  assert!(refused.is_err_and(|e| e.message.contains("Response")));
}

#[test]
fn a_header_the_route_sets_twice_comes_back_twice() {
  let passed = block_on(drive_middleware(
    ctx(),
    "",
    |ctx| Box::pin(async move { ctx.next.clone().unwrap().call(Value::Null).await }),
    |_target| async {
      let mut head = HttpResponse::new(200);
      head.headers.push(("set-cookie".to_owned(), "a=1".to_owned()));
      head.headers.push(("set-cookie".to_owned(), "b=2".to_owned()));
      Ok((head, String::new()))
    },
  ))
  .unwrap();
  let Passed::Route(head, _) = passed else { panic!("the route's answer") };
  let cookies: Vec<&str> = head.headers.iter().filter(|(n, _)| n == "set-cookie").map(|(_, v)| v.as_str()).collect();
  assert_eq!(cookies, ["a=1", "b=2"]);
}
