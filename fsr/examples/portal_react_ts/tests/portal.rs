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
  assert!(report.contains("/billing/invoice/{id}") && report.contains("billing:$root") && report.contains("billing:ledger         mock"), "{report}");
  assert!(report.contains("billing                ignored [session], the shell's"), "{report}");

  let response = portal.handle(Request::get("/billing").body(Bytes::new()).unwrap()).await;
  assert_eq!(response.status(), StatusCode::OK);
  assert_eq!(response.headers().get("x-portal").unwrap(), "billing", "the portal's middleware saw the site");
  assert_eq!(response.headers().get("x-billing").unwrap(), "invoices", "the site's middleware ran after it");
  let html = body_of(response).await;
  assert!(html.contains("class=\"brand\"") && html.contains("3 teams"), "the portal's header wraps the site: {html}");
  assert!(html.contains("Northwind") && html.contains("<!--sf-g:billing:routes/page.tsx#default-->") && html.contains("data-sf-module=\"billing:routes/layout.tsx#default\""), "{html}");
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

#[tokio::test]
async fn the_portal_mounts_the_blog_and_writes_its_pages_ahead() {
  let portal = portal();
  let report = portal.report().to_string();
  assert!(report.contains("blog                   at /blog from"), "{report}");
  assert!(report.contains("/blog/post/{slug}") && report.contains("blog:post.$slug"), "{report}");
  assert!(report.contains("render    /billing               billing:routes/page.tsx#default not rendered") || report.contains("/blog                  blog:routes/layout.tsx#default"), "the blog's subtrees under the session-reading layout are renderable ahead: {report}");

  let response = portal.handle(Request::get("/blog").body(Bytes::new()).unwrap()).await;
  assert_eq!(response.status(), StatusCode::OK);
  let html = body_of(response).await;
  assert!(html.contains("href=\"/blog/post/why-a-plan-file\"") && html.contains("href=\"/blog/static/css/blog.css\""), "{html}");
  assert!(html.contains("portal-main"), "under the portal's layout: {html}");

  let response = portal.handle(Request::get("/blog/post/static-under-a-shell").body(Bytes::new()).unwrap()).await;
  assert_eq!(response.status(), StatusCode::OK);
  let html = body_of(response).await;
  assert!(html.contains("<title>Static pages under a shell that reads the session · Blog</title>") && html.contains("<code>fsr prerender</code>"), "{html}");

  let response = portal.handle(Request::get("/blog/post/nope").body(Bytes::new()).unwrap()).await;
  assert_eq!(response.status(), StatusCode::NOT_FOUND, "a slug off the blog is a 404 document");
  assert!(body_of(response).await.contains("That did not load"), "rendered through the blog's error page");

  let out = std::env::temp_dir().join(format!("portal-prerender-{}", std::process::id()));
  let written = portal.prerender(&out).await.unwrap();
  assert!(written.iter().any(|(name, _)| name == "renders.json"), "the blog's fixed subtrees are rendered ahead: {written:?}");
  assert!(!written.iter().any(|(pattern, _)| pattern.starts_with("/blog")), "no document under the session-reading layout is written whole: {written:?}");
  std::fs::remove_dir_all(&out).unwrap();
}

