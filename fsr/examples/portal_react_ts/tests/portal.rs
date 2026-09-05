use std::sync::Arc;

use bytes::Bytes;
use http::{header, Request, StatusCode};
use http_body_util::BodyExt;
use snapfire_fsr_host::Host;

fn portal() -> Arc<Host> {
  let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
  let builder = Host::from(&root).unwrap();
  let builder = snapfire_fsr_sites::mount_all(builder).unwrap();
  Arc::new(builder.build().unwrap())
}

async fn body_of(response: http::Response<snapfire_fsr_host::Body>) -> String {
  String::from_utf8(response.into_body().collect().await.unwrap().to_bytes().to_vec()).unwrap()
}

fn cookie_of(response: &http::Response<snapfire_fsr_host::Body>) -> String {
  response.headers().get(header::SET_COOKIE).unwrap().to_str().unwrap().split(';').next().unwrap().to_owned()
}

#[tokio::test]
async fn the_portal_mounts_billing_under_its_root_layout() {
  let portal = portal();
  let report = portal.report().to_string();
  assert!(report.contains("sites     billing                at /billing from"), "{report}");
  assert!(report.contains("/billing/invoice/{id}") && report.contains("billing:index") && report.contains("billing:ledger         mock"), "{report}");
  assert!(report.contains("ignored [static /static/js/fsr, static /static/js/vendor, session]"), "{report}");

  let response = portal.handle(Request::get("/billing").body(Bytes::new()).unwrap()).await;
  assert_eq!(response.status(), StatusCode::OK);
  assert_eq!(response.headers().get("x-portal").unwrap(), "billing", "the portal's middleware saw the site");
  assert_eq!(response.headers().get("x-billing").unwrap(), "invoices", "the site's middleware ran after it");
  let html = body_of(response).await;
  assert!(html.contains("class=\"brand\"") && html.contains("3 teams"), "the portal's header wraps the site: {html}");
  assert!(html.contains("Northwind") && html.contains("data-sf-module=\"billing:routes/index/page.tsx#default\""), "{html}");
  assert!(html.contains("href=\"/billing/static/css/billing.css\""), "the site's stylesheet rides under its prefix: {html}");

  let response = portal.handle(Request::get("/").body(Bytes::new()).unwrap()).await;
  assert_eq!(response.headers().get("x-portal").unwrap(), "portal");
  assert!(response.headers().get("x-billing").is_none());
  let html = body_of(response).await;
  assert!(html.contains("Billing") && html.contains("/billing") && !html.contains("billing.css"), "{html}");

  let payload = body_of(portal.handle(Request::get("/billing/invoice/1?__payload").body(Bytes::new()).unwrap()).await).await;
  assert!(payload.contains("Northwind") && payload.contains("T {") && payload.contains("portal/who"), "the site's page carries the portal's seed: {payload}");
}

#[tokio::test]
async fn one_sign_in_covers_the_site_and_its_guard() {
  let portal = portal();
  let response = portal.handle(Request::get("/billing/overdue").body(Bytes::new()).unwrap()).await;
  assert_eq!(response.status(), StatusCode::TEMPORARY_REDIRECT);
  assert_eq!(response.headers().get(header::LOCATION).unwrap(), "/auth/login?return_to=/billing/overdue", "the site's guard sends an anonymous visitor to the portal's login");

  let response = portal.handle(Request::get("/login").body(Bytes::new()).unwrap()).await;
  let cookie = cookie_of(&response);
  let response = portal
    .handle(Request::post("/auth/callback").header(header::COOKIE, &cookie).header(header::CONTENT_TYPE, "application/x-www-form-urlencoded").body(Bytes::from("user=alice&password=wonder")).unwrap())
    .await;
  assert_eq!(response.status(), StatusCode::SEE_OTHER, "{}", body_of(response).await);
  let response = portal.handle(Request::get("/billing/overdue").header(header::COOKIE, &cookie).body(Bytes::new()).unwrap()).await;
  assert_eq!(response.status(), StatusCode::OK);
  let html = body_of(response).await;
  assert!(html.contains("alice") && html.contains("overdue"), "the site's loader read the portal's identity: {html}");
}

#[tokio::test]
async fn the_sites_status_names_the_mount() {
  let portal = portal();
  let status = body_of(portal.handle(Request::get("/__fsr/sites").body(Bytes::new()).unwrap()).await).await;
  let json: serde_json::Value = serde_json::from_str(&status).unwrap();
  assert_eq!(json["sites"][0]["name"], "billing");
  assert_eq!(json["sites"][0]["at"], "/billing");
  assert_eq!(json["sites"][0]["version"], "path");
  assert_eq!(json["sites"][0]["hash"].as_str().unwrap().len(), 16);
}
