use futures::executor::block_on;
use std::time::Duration;

use advanced_tera_app::{build_app, render, AppError, RenderMode};

#[test]
fn html_serves_the_walked_page_shape() {
  let app = build_app(Duration::ZERO);
  let html = block_on(render(&app, "/dash/servers", RenderMode::Html)).unwrap();

  assert!(html.starts_with("<!--sf-g:"), "the root segment delimiter leads: {html}");
  assert!(html.contains("<!doctype html>"), "layout owns the shell: {html}");
  assert!(html.contains("data-sf-segments"), "the navigator sidecar ships with the page");
  assert!(html.contains("<title>servers (2 servers) - SnapFire FSR</title>"), "metadata computed from loader data: {html}");
  assert!(html.contains("<nav>SnapFire FSR "), "layout loader data rendered");
  assert!(html.contains("<a href=\"/slow/servers\">"), "nav links for the navigator");
  assert!(html.contains("<h1>servers</h1>"), "route param reached the template");
  assert!(html.contains("<td>web-1</td>") && html.contains("<td>0.41</td>"), "page loader data rendered");
  assert!(
    html.contains("<sf-i id=\"sf-i0\" data-sf-module=\"components/ServerChart.tsx#default\"></sf-i>"),
    "island survives as an empty marker under the null-ssr path: {html}"
  );
  assert!(
    html.contains("data-sf-props=\"sf-i0\">{\"series\":{\"$\":\"ta\",\"k\":\"f64\","),
    "island props embed as tagged JSON: {html}"
  );
  assert!(!html.contains('\u{F8FF}'), "no marker delimiters leak into output");
}

#[test]
fn payload_mode_serves_wire_rows() {
  let app = build_app(Duration::ZERO);
  let payload = block_on(render(&app, "/dash/servers", RenderMode::Payload)).unwrap();

  assert!(payload.starts_with("V {\"fmt\":1,\"enc\":\"json\"}\nN "), "version row then tree row: {payload}");
  assert!(payload.contains("[\"c\",{\"m\":\"components/ServerChart.tsx#default\""), "island rides as a client row");
  assert!(payload.contains("\"$\":\"ta\""), "typed array props survive the JSON-backed template context");
}

#[test]
fn unmatched_paths_are_not_found() {
  let app = build_app(Duration::ZERO);
  let err = block_on(render(&app, "/nope", RenderMode::Html)).unwrap_err();
  assert!(matches!(err, AppError::NotFound(_)));
}

#[test]
fn island_props_round_trip_through_the_degraded_tera_context() {
  use snapfire_fsr_core::{TypedArray, Value};
  use snapfire_fsr_payload::json_to_value;

  let app = build_app(Duration::ZERO);
  let html = block_on(render(&app, "/dash/servers", RenderMode::Html)).unwrap();

  let props_json = html
    .split_once("data-sf-props=\"sf-i0\">")
    .and_then(|(_, rest)| rest.split_once("</script>"))
    .map(|(json, _)| json)
    .unwrap();
  let parsed: serde_json::Value = serde_json::from_str(props_json).unwrap();
  let decoded = json_to_value(&parsed).unwrap();
  let Value::Map(map) = decoded else { panic!("props decode to a map") };
  let Value::TypedArray(TypedArray::F64(series)) = &map["series"] else {
    panic!("series came back as a real typed array, not a degraded view")
  };
  assert_eq!(series, &vec![12.0, 15.5, 9.25]);
}

#[test]
fn a_down_backend_degrades_the_segment_never_the_page() {
  let app = build_app(Duration::ZERO);
  let html = block_on(render(&app, "/dash/down", RenderMode::Html)).unwrap();

  assert!(html.contains("<nav>SnapFire FSR "), "the layout survives the failure");
  assert!(html.contains("<title>down (2 servers) - SnapFire FSR</title>"), "metadata still computes");
  assert!(html.contains("<h2>Backend unavailable</h2>"), "the route's error partial renders: {html}");
  assert!(html.contains("the servers backend is unreachable"));
  assert!(!html.contains("<td>web-1</td>"), "no partial data leaks into the failed segment");
}

#[test]
fn the_index_lists_the_routes_and_the_dev_credentials() {
  let app = build_app(Duration::ZERO);
  let html = block_on(render(&app, "/", RenderMode::Html)).unwrap();

  for route in ["/dash/servers", "/slow/servers", "/dash/down", "/login"] {
    assert!(html.contains(&format!("href=\"{route}\"")), "the index links {route}: {html}");
  }
  assert!(html.contains("alice"), "the dev credentials are on the page: {html}");
  assert!(html.contains("<nav>SnapFire FSR "), "the index sits in the layout");
}
