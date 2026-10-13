use std::sync::Arc;

use bytes::Bytes;
use http::{Request, StatusCode};
use http_body_util::BodyExt;
use snapfire_fsr_host::Host;

fn reports() -> Arc<Host> {
  Arc::new(Host::from(env!("CARGO_MANIFEST_DIR")).and_then(|builder| builder.build()).unwrap())
}

async fn body_of(response: http::Response<snapfire_fsr_host::Body>) -> String {
  String::from_utf8(response.into_body().collect().await.unwrap().to_bytes().to_vec()).unwrap()
}

#[tokio::test]
async fn the_site_runs_alone_under_its_prefix_and_describes_itself_to_a_shell() {
  let host = reports();
  let report = host.report().to_string();
  assert!(report.contains("site      reports                at /reports"), "{report}");
  assert!(report.contains("/reports/{id}") && report.contains("services  reports:metrics"), "{report}");

  let response = host.handle(Request::get("/reports").body(Bytes::new()).unwrap()).await;
  assert_eq!(response.status(), StatusCode::OK);
  let html = body_of(response).await;
  assert!(html.contains("Revenue by region") && html.contains("data-sf-module=\"reports:routes/layout.tsx#default\""), "{html}");
  assert!(html.contains("href=\"/reports/1\""), "links are literal: {html}");

  let response = host.handle(Request::get("/reports/1").body(Bytes::new()).unwrap()).await;
  let html = body_of(response).await;
  assert!(html.contains("<title>Reports</title>") && html.contains("Adding up the rows") && html.contains("21100.00") && html.contains("__sfHead({\"title\":\"Revenue by region · Reports\","), "the rows and the title stream in after the loading segment: {html}");

  let status = body_of(host.handle(Request::get("/__fsr/sites").body(Bytes::new()).unwrap()).await).await;
  let json: serde_json::Value = serde_json::from_str(&status).unwrap();
  assert_eq!(json["site"]["name"], "reports");
  assert_eq!(json["site"]["at"], "/reports");
  assert_eq!(json["site"]["entry"], "/reports/static/js/app/src/main.js");
  assert_eq!(json["site"]["styles"][0], "/reports/static/js/app/styles/reports.css");

  let response = host.handle(Request::get("/").body(Bytes::new()).unwrap()).await;
  assert_eq!(response.status(), StatusCode::NOT_FOUND, "nothing lives outside the prefix");
}
