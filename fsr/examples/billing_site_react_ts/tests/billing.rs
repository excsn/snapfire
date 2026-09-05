use std::sync::Arc;

use bytes::Bytes;
use http::{Request, StatusCode};
use http_body_util::BodyExt;
use snapfire_fsr_host::Host;

fn billing() -> Arc<Host> {
  Arc::new(Host::from(env!("CARGO_MANIFEST_DIR")).and_then(|builder| builder.build()).unwrap())
}

async fn body_of(response: http::Response<snapfire_fsr_host::Body>) -> String {
  String::from_utf8(response.into_body().collect().await.unwrap().to_bytes().to_vec()).unwrap()
}

#[tokio::test]
async fn the_site_runs_alone_under_its_prefix_with_its_ids_prefixed() {
  let host = billing();
  let report = host.report().to_string();
  assert!(report.contains("site      billing                at /billing"), "{report}");
  assert!(report.contains("/billing/invoice/{id}") && report.contains("billing:invoice.pay") && report.contains("services  billing:ledger"), "{report}");

  let response = host.handle(Request::get("/billing").body(Bytes::new()).unwrap()).await;
  assert_eq!(response.status(), StatusCode::OK);
  assert_eq!(response.headers().get("x-billing").unwrap(), "invoices");
  let html = body_of(response).await;
  assert!(html.contains("Northwind") && html.contains("Contoso") && html.contains("data-sf-module=\"billing:routes/layout.tsx#default\""), "{html}");
  assert!(html.contains("href=\"/billing/invoice/1\""), "links are literal: {html}");

  let response = host.handle(Request::get("/billing/invoice/1").body(Bytes::new()).unwrap()).await;
  let html = body_of(response).await;
  assert!(html.contains("<title>Northwind · Billing</title>") && html.contains("1250.5"), "{html}");

  let response = host.handle(Request::get("/billing/overdue").body(Bytes::new()).unwrap()).await;
  assert_eq!(response.status(), StatusCode::TEMPORARY_REDIRECT, "the guard holds alone too");

  let response = host.handle(Request::get("/").body(Bytes::new()).unwrap()).await;
  assert_eq!(response.status(), StatusCode::NOT_FOUND, "nothing lives outside the prefix");

  let response = host
    .handle(Request::post("/_sf/action/billing:invoice.pay").header("content-type", "application/json").body(Bytes::from(r#"{"id": 1}"#)).unwrap())
    .await;
  assert_eq!(response.status(), StatusCode::OK);
  assert!(body_of(response).await.contains("paid"));
}
