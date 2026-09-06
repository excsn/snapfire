//! The board over a field whose clock a test holds still, and over services
//! that stall on purpose: what the reader sees before they answer, in what
//! order the rest arrives and what the board says at a given minute.

use std::path::Path;
use std::time::Duration;

use futures::StreamExt;
use snapfire_fsr_host::{Config, Host, RenderMode};
use snapfire_fsr_runtime::SessionCell;

/// 08:25, by which time two arrivals have landed, one is late, a departure is
/// boarding and two gates have moved.
const MORNING: i64 = 505;

fn board(pause: Duration) -> Host {
  let config = Config::load(Path::new(env!("CARGO_MANIFEST_DIR"))).unwrap();
  Host::from_config(config).unwrap().services_over(arrivals_react_ts::backend::at(MORNING, pause)).build().unwrap()
}

async fn parts(host: &Host) -> Vec<String> {
  host.render("/", RenderMode::Html, SessionCell::default()).await.unwrap().collect().await
}

#[tokio::test]
async fn the_document_goes_out_with_the_board_rendered_and_a_skeleton_for_each_panel() {
  let parts = parts(&board(Duration::from_millis(0))).await;
  let first = &parts[0];

  assert!(first.contains("Field time"), "the clock is in the first chunk: {first}");
  assert!(first.contains("<strong>08:25</strong>"), "{first}");
  assert!(first.contains("<h2>Arrivals</h2>") && first.contains("<h2>Departures</h2>"), "both boards: {first}");
  assert!(first.contains("Frankfurt") && first.contains("Madrid"), "rendered, not left to the browser: {first}");
  assert!(first.contains("<h2>The field</h2>") && first.contains("skeleton"), "the weather panel is a skeleton: {first}");
  assert!(first.contains("<h2>Gate changes</h2>"), "and so is the gate panel: {first}");
  assert!(!first.contains("Wind") && !first.contains("class=\"at\""), "neither service has answered yet: {first}");
  assert!(first.contains("data-sf-slot=\"1\"") && first.contains("data-sf-slot=\"2\""), "two holes to fill: {first}");
}

#[tokio::test]
async fn each_panel_arrives_in_its_own_chunk_in_the_order_the_services_answer() {
  let parts = parts(&board(Duration::from_millis(60))).await;
  assert_eq!(parts.len(), 3, "the document, then one fill per panel: {parts:?}");

  assert!(parts[1].contains("Wind"), "the field reports first, at one pause: {}", parts[1]);
  assert!(parts[2].contains("B14") && parts[2].contains("A07"), "the gate system reports second, at two: {}", parts[2]);

  for fill in &parts[1..] {
    assert!(!fill.contains("skeleton"), "a fill replaces its skeleton rather than carrying one: {fill}");
  }
}

#[tokio::test]
async fn the_clock_decides_what_every_flight_says() {
  let whole = parts(&board(Duration::from_millis(0))).await.join("");

  assert!(whole.contains("<td class=\"flight\">LH 906</td>"), "a flight that has landed is still on the board for an hour: {whole}");
  for (flight, status) in [("AF 1680", "delayed"), ("KL 1007", "on time"), ("SK 1512", "delayed"), ("KL 1008", "departed"), ("AZ 205", "boarding")] {
    let row = whole.split(flight).nth(1).unwrap_or_default();
    let cell = row.split("class=\"status\">").nth(1).unwrap_or_default();
    assert!(cell.starts_with(status), "{flight} reads {status} at 08:25: {}", &cell[..cell.len().min(40)]);
  }
  assert!(!whole.contains("BA 118"), "an arrival more than an hour gone has left the board: {whole}");
  assert!(whole.contains("<td class=\"time\">08:05</td>") && whole.contains("<td class=\"time\">08:40</td>"), "scheduled and expected are both shown: {whole}");
}

#[tokio::test]
async fn a_gate_change_reaches_the_flight_and_the_panel() {
  let whole = parts(&board(Duration::from_millis(0))).await.join("");
  assert!(whole.contains("<td class=\"gate\">B14</td>"), "AF 1680 shows the gate it was moved to: {whole}");
  assert!(whole.contains("<span class=\"was\">B11</span>"), "and the panel says where it came from: {whole}");
  assert!(whole.contains("<span class=\"at\">07:32</span>"), "with the minute the field announced it: {whole}");
}

#[tokio::test]
async fn every_component_renders_on_the_server_so_a_stall_is_the_only_thing_the_reader_waits_for() {
  let host = board(Duration::from_millis(0));
  let report = host.report().to_string();
  let rendered: Vec<&str> = report.lines().filter(|line| line.contains("#default") || line.contains("#Table")).collect();
  assert!(rendered.iter().all(|line| line.ends_with("lowered")), "nothing but the live island falls to the browser: {rendered:?}");
  assert!(report.contains("src/ui/Live.tsx#default"), "the island that follows the field is registered: {report}");
}
