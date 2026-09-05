#![allow(dead_code)]

use std::time::Duration;

use bytes::Bytes;
use futures::executor::block_on;
use http::{header, Request, Response};
use snapfire_fsr_host::{Body, Host, RenderMode};
use snapfire_fsr_runtime::SessionCell;

pub fn app() -> Host {
  advanced_tera_app::build(Duration::ZERO).unwrap()
}

/// A render outside the edge: no middleware, no session cookie, no token.
pub fn render(host: &Host, path: &str) -> String {
  block_on(host.render_to_string(path, RenderMode::Html, SessionCell::default())).unwrap()
}

pub fn get(host: &Host, path: &str, cookie: Option<&str>) -> Response<Body> {
  let mut request = Request::get(path);
  if let Some(cookie) = cookie {
    request = request.header(header::COOKIE, cookie);
  }
  block_on(host.handle(request.body(Bytes::new()).unwrap()))
}

pub fn post_form(host: &Host, path: &str, cookie: Option<&str>, body: &str, referer: Option<&str>) -> Response<Body> {
  let mut request = Request::post(path).header(header::CONTENT_TYPE, "application/x-www-form-urlencoded");
  if let Some(cookie) = cookie {
    request = request.header(header::COOKIE, cookie);
  }
  if let Some(referer) = referer {
    request = request.header(header::REFERER, referer);
  }
  block_on(host.handle(request.body(Bytes::from(body.to_owned())).unwrap()))
}

pub fn text(response: Response<Body>) -> String {
  block_on(async { String::from_utf8(http_body_util::BodyExt::collect(response.into_body()).await.unwrap().to_bytes().to_vec()).unwrap() })
}

pub fn location(response: &Response<Body>) -> String {
  response.headers().get(header::LOCATION).expect("a location").to_str().unwrap().to_owned()
}

pub fn session_cookie(response: &Response<Body>) -> String {
  response
    .headers()
    .get_all(header::SET_COOKIE)
    .iter()
    .map(|v| v.to_str().unwrap())
    .find(|v| v.starts_with("sf_session="))
    .expect("a session cookie")
    .split(';')
    .next()
    .unwrap()
    .to_owned()
}

pub fn csrf_in(html: &str) -> String {
  let start = html.find("name=\"_csrf\" value=\"").map(|i| i + 20).expect("a hidden csrf input");
  html[start..start + html[start..].find('"').unwrap()].to_owned()
}
