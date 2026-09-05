mod common;

use common::app;
use futures::executor::block_on;
use futures_util::StreamExt;
use snapfire_fsr_host::RenderMode;
use snapfire_fsr_runtime::SessionCell;

fn chunks(path: &str, mode: RenderMode) -> Vec<String> {
  let host = app();
  block_on(async {
    let stream = host.render(path, mode, SessionCell::default()).await.unwrap();
    stream.collect::<Vec<_>>().await
  })
}

#[test]
fn slow_route_streams_wire_rows() {
  let rows = chunks("/slow/servers", RenderMode::Payload);
  assert_eq!(rows.len(), 2, "tree row then one resolution: {rows:?}");
  assert!(rows[0].starts_with("V {\"fmt\":1,\"enc\":\"json\"}\nN "));
  assert!(rows[0].contains("[\"p\",1,"), "Pending slot in the tree: {}", rows[0]);
  assert!(rows[0].contains("loading latency"), "fallback rendered from the loading template");
  assert!(rows[1].starts_with("S 1 "));
  assert!(rows[1].contains("components/LatencyChart.tsx#default"), "late island in the resolution");
}

#[test]
fn slow_route_streams_html_fill() {
  let parts = chunks("/slow/servers", RenderMode::Html);
  assert_eq!(parts.len(), 2);

  assert!(parts[0].contains("<div data-sf-slot=\"1\"><section class=\"card chart-late skl\" aria-busy=\"true\"><h2>Latency</h2><p class=\"skl-text\">loading latency</p>"));
  assert!(parts[0].contains("function __sfFill"), "fill script ships before any fill");
  assert!(parts[0].contains("sf-i0"), "the eager inline island takes id 0");
  assert!(!parts[0].contains("<template data-sf-fill"), "no template in the first flush");

  assert!(parts[1].starts_with("<template data-sf-fill=\"1\">"));
  assert!(parts[1].ends_with("<script>__sfFill(1)</script>"));
  assert!(parts[1].contains("sf-i1"), "the late island continues the id sequence");
  assert!(parts[1].contains("data-sf-props=\"sf-i1\""), "late island props ride inside the template");
}

#[test]
fn eager_route_is_a_single_chunk() {
  let parts = chunks("/dash/servers", RenderMode::Html);
  assert_eq!(parts.len(), 1);
  assert!(!parts[0].contains("function __sfFill"), "no fill script on a page with nothing pending");
}
