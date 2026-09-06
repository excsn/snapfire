//! The board over a service that stalls on purpose: what the reader sees
//! before it answers, and in what order the rest arrives.

use std::path::Path;
use std::time::Duration;

use futures::StreamExt;
use snapfire_fsr_host::{Config, Host, RenderMode};
use snapfire_fsr_runtime::SessionCell;

fn board(pause: Duration) -> Host {
  let config = Config::load(Path::new(env!("CARGO_MANIFEST_DIR"))).unwrap();
  Host::from_config(config).unwrap().services_over(arrivals_react_ts::backend::board(pause)).build().unwrap()
}

async fn parts(host: &Host) -> Vec<String> {
  host.render("/", RenderMode::Html, SessionCell::default()).await.unwrap().collect().await
}

#[tokio::test]
async fn the_document_goes_out_with_the_board_rendered_and_a_skeleton_for_each_panel() {
  let parts = parts(&board(Duration::from_millis(0))).await;
  let first = &parts[0];

  assert!(first.contains("<table class=\"arrivals\">"), "the board itself is in the first chunk: {first}");
  assert!(first.contains("BA 118") && first.contains("New York JFK"), "rendered, not left to the browser: {first}");
  assert!(first.contains("<h2>The field</h2>") && first.contains("skeleton"), "the weather panel is a skeleton: {first}");
  assert!(first.contains("<h2>Gate changes</h2>"), "and so is the gate panel: {first}");
  assert!(!first.contains("240°") && !first.contains("B14"), "neither service has answered yet: {first}");
  assert!(first.contains("data-sf-slot=\"1\"") && first.contains("data-sf-slot=\"2\""), "two holes to fill: {first}");
}

#[tokio::test]
async fn each_panel_arrives_in_its_own_chunk_in_the_order_the_services_answer() {
  let parts = parts(&board(Duration::from_millis(60))).await;
  assert_eq!(parts.len(), 3, "the document, then one fill per panel: {parts:?}");

  assert!(parts[1].contains("240° at 12 kt"), "the field reports first, at one pause: {}", parts[1]);
  assert!(parts[1].contains("Overcast") || parts[1].contains("overcast"), "{}", parts[1]);
  assert!(parts[2].contains("B14"), "the gate system reports second, at two: {}", parts[2]);
  assert!(parts[2].contains("A07"), "{}", parts[2]);

  for fill in &parts[1..] {
    assert!(!fill.contains("skeleton"), "a fill replaces its skeleton rather than carrying one: {fill}");
  }
}

#[tokio::test]
async fn the_whole_document_holds_together_once_every_service_has_answered() {
  let whole = parts(&board(Duration::from_millis(0))).await.join("");
  for expected in ["BA 118", "Amsterdam", "240° at 12 kt", "<dd>11<!-- -->°C</dd>", "B14", "A07"] {
    assert!(whole.contains(expected), "{expected} is in the settled document: {whole}");
  }
  assert!(whole.contains("<title>Arrivals</title>"), "{whole}");
}

#[tokio::test]
async fn every_component_renders_on_the_server_so_a_stall_is_the_only_thing_the_reader_waits_for() {
  let host = board(Duration::from_millis(0));
  let report = host.report().to_string();
  for module in [
    "routes/layout.tsx#default",
    "routes/page.tsx#default",
    "routes/slots/weather/page.tsx#default",
    "routes/slots/gates/page.tsx#default",
  ] {
    assert!(report.contains(&format!("{module} lowered")) || report.contains(&format!("{module}     lowered")), "{module} is lowered: {report}");
  }
  let rendered: Vec<&str> = report.lines().filter(|line| line.contains("#default")).collect();
  assert_eq!(rendered.len(), 6, "six components, layout and loading files included: {rendered:?}");
  assert!(rendered.iter().all(|line| line.ends_with("lowered")), "nothing fell back to the browser: {rendered:?}");
}
