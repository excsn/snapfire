use std::io::{BufRead, BufReader, Write};
use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::sync::{Arc, OnceLock};

use bytes::Bytes;
use http::{header, Request, StatusCode};
use http_body_util::BodyExt;
use snapfire_fsr_host::Host;
use snapfire_fsr_sites::{pack, ArchiveStore, Cache, HttpStore};

/// Serves `dir` over HTTP on a free port, the store the configuration's
/// `http://127.0.0.1:8199` stands for in the README.
fn serve(dir: PathBuf) -> String {
  let listener = TcpListener::bind("127.0.0.1:0").unwrap();
  let base = format!("http://{}", listener.local_addr().unwrap());
  std::thread::spawn(move || {
    for stream in listener.incoming().flatten() {
      let mut reader = BufReader::new(stream.try_clone().unwrap());
      let mut first = String::new();
      reader.read_line(&mut first).unwrap();
      let path = first.split_whitespace().nth(1).unwrap_or("/").to_owned();
      let mut line = String::new();
      while reader.read_line(&mut line).unwrap() > 0 && line != "\r\n" {
        line.clear();
      }
      let (status, body) = match std::fs::read(dir.join(path.trim_start_matches('/'))) {
        Ok(bytes) => ("200 OK", bytes),
        Err(_) => ("404 Not Found", Vec::new()),
      };
      let mut stream = stream;
      write!(stream, "HTTP/1.1 {status}\r\ncontent-length: {}\r\nconnection: close\r\n\r\n", body.len()).unwrap();
      stream.write_all(&body).unwrap();
    }
  });
  base
}

struct Deployed {
  store: HttpStore,
  fetched: bool,
}

/// The README's walkthrough, done once: billing packed and installed into the
/// cache, status packed into a store and fetched by the first boot.
fn deployed() -> &'static Deployed {
  static DEPLOYED: OnceLock<Deployed> = OnceLock::new();
  DEPLOYED.get_or_init(|| {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let scratch = std::env::temp_dir().join(format!("portal-sites-{}", std::process::id()));
    let cache = Cache::new(root.join("deploy/sites"));
    let archive = scratch.join("billing-1.0.0.tar.gz");
    pack(&root.join("sites/billing"), "1.0.0", &archive).unwrap();
    let _ = std::fs::remove_dir_all(cache.path("billing", "1.0.0"));
    cache.install(&ArchiveStore { archive }, "billing", "billing", "1.0.0", None).unwrap();

    pack(&root.join("sites/status"), "1.0.0", &scratch.join("store/status-1.0.0.tar.gz")).unwrap();
    let _ = std::fs::remove_dir_all(cache.path("status", "1.0.0"));
    let store = HttpStore::new(serve(scratch.join("store")));
    snapfire_fsr_sites::mount_all_with(Host::from(&root).unwrap(), &store).unwrap().build().unwrap();
    let fetched = cache.holds("status", "1.0.0");
    Deployed { store, fetched }
  })
}

fn portal() -> Arc<Host> {
  let deployed = deployed();
  let root: &Path = Path::new(env!("CARGO_MANIFEST_DIR"));
  let builder = snapfire_fsr_sites::mount_all_with(Host::from(root).unwrap(), &deployed.store).unwrap();
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

/// The `<a>` for `href`, from the first `<nav>` onwards.
fn anchor<'a>(html: &'a str, href: &str) -> &'a str {
  let nav = html.find("<nav").expect("a nav");
  let at = html[nav..].find(&format!("href=\"{href}\"")).map(|i| nav + i).unwrap_or_else(|| panic!("no link to {href} in {html}"));
  let start = html[..at].rfind('<').unwrap();
  &html[start..at + html[at..].find('>').unwrap()]
}

#[tokio::test]
async fn the_nav_marks_the_page_being_shown_across_the_mount() {
  let portal = portal();
  let html = body_of(portal.handle(Request::get("/").body(Bytes::new()).unwrap()).await).await;
  assert!(anchor(&html, "/").contains("aria-current=\"page\""), "the portal's own page is the current one: {}", anchor(&html, "/"));
  assert!(!anchor(&html, "/billing").contains("aria-current"), "{}", anchor(&html, "/billing"));

  let response = portal.handle(Request::get("/login").body(Bytes::new()).unwrap()).await;
  let cookie = cookie_of(&response);
  portal
    .handle(Request::post("/auth/callback").header(header::COOKIE, &cookie).header(header::CONTENT_TYPE, "application/x-www-form-urlencoded").body(Bytes::from("user=alice&password=wonder")).unwrap())
    .await;

  let html = body_of(portal.handle(Request::get("/billing").header(header::COOKIE, &cookie).body(Bytes::new()).unwrap()).await).await;
  assert!(anchor(&html, "/billing").contains("aria-current=\"true\""), "a section link is marked on the page it covers: {}", anchor(&html, "/billing"));
  assert!(!anchor(&html, "/").contains("aria-current"), "and the one it does not cover is left alone: {}", anchor(&html, "/"));
  assert!(html.matches("aria-current=\"page\"").count() == 1, "the site's own nav marks its invoices link and nothing else does: {html}");

  let html = body_of(portal.handle(Request::get("/billing/overdue").header(header::COOKIE, &cookie).body(Bytes::new()).unwrap()).await).await;
  assert!(anchor(&html, "/billing").contains("aria-current=\"true\""), "{}", anchor(&html, "/billing"));
  assert!(anchor(&html, "/billing/overdue").contains("aria-current=\"page\""), "the mark moved with the page: {}", anchor(&html, "/billing/overdue"));

  let again = body_of(portal.handle(Request::get("/billing").header(header::COOKIE, &cookie).body(Bytes::new()).unwrap()).await).await;
  assert!(!anchor(&again, "/billing/overdue").contains("aria-current"), "no render memoized under one path is served under another: {}", anchor(&again, "/billing/overdue"));
}

#[tokio::test]
async fn the_sites_status_names_the_mount() {
  let portal = portal();
  let status = body_of(portal.handle(Request::get("/__fsr/sites").body(Bytes::new()).unwrap()).await).await;
  let json: serde_json::Value = serde_json::from_str(&status).unwrap();
  assert_eq!(json["sites"][0]["name"], "billing");
  assert_eq!(json["sites"][0]["at"], "/billing");
  assert_eq!(json["sites"][0]["version"], "1.0.0", "installed from its archive");
  assert_eq!(json["sites"][0]["hash"].as_str().unwrap().len(), 16);
  assert_eq!(json["sites"][1]["name"], "blog");
  assert_eq!(json["sites"][1]["version"], "path", "a linked working tree");
  assert_eq!(json["sites"][2]["name"], "status");
  assert_eq!(json["sites"][2]["version"], "1.0.0", "fetched from the store");
}

#[tokio::test]
async fn the_first_boot_fetches_status_from_the_store() {
  assert!(deployed().fetched, "status@1.0.0 was not in the cache before the boot and is after it");
  let portal = portal();
  let response = portal.handle(Request::get("/status").body(Bytes::new()).unwrap()).await;
  assert_eq!(response.status(), StatusCode::OK);
  let html = body_of(response).await;
  assert!(html.contains("<h1>Status</h1>") && html.contains("portal-main"), "under the portal's layout: {html}");
  assert!(html.contains("1 of 4 systems needs attention."), "{html}");
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

