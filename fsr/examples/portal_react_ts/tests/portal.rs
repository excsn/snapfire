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

/// The README's walkthrough, done once across every test process for the site sources as they stand: billing packed and installed into the cache, status packed into a store and fetched by the first boot. Each process serves its own copy of the store for the reloads that fetch from it.
fn deployed() -> &'static Deployed {
  static DEPLOYED: OnceLock<Deployed> = OnceLock::new();
  DEPLOYED.get_or_init(|| {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let scratch = std::env::temp_dir().join(format!("portal-sites-{}", std::process::id()));
    pack(&root.join("sites/status"), "1.0.0", &scratch.join("store/status-1.0.0.tar.gz")).unwrap();
    let store = HttpStore::new(serve(scratch.join("store")));
    let markers = root.join("deploy/.once");
    let key = format!("walkthrough-{}-{}", snapfire_fsr_sites::hash_dir(&root.join("sites/billing")).unwrap(), snapfire_fsr_sites::hash_dir(&root.join("sites/status")).unwrap());
    snapfire_fsr_core::lock::once(&markers, &key, || -> Result<(), String> {
      let cache = Cache::new(root.join("deploy/sites"));
      let archive = scratch.join("billing-1.0.0.tar.gz");
      pack(&root.join("sites/billing"), "1.0.0", &archive).map_err(|e| e.to_string())?;
      let _ = std::fs::remove_dir_all(cache.path("billing", "1.0.0"));
      cache.install(&ArchiveStore::new(archive), "billing", "billing", "1.0.0", None).map_err(|e| e.to_string())?;
      let _ = std::fs::remove_dir_all(cache.path("status", "1.0.0"));
      // The pins `fsr sites install --layer local` writes, for the artifacts this
      // run packed, so the table holds against what the cache holds.
      let billing_hash = snapfire_fsr_sites::hash_dir(&cache.path("billing", "1.0.0")).map_err(|e| e.to_string())?;
      std::fs::write(root.join("config/local.toml"), format!("[sites.billing]\nhash = \"{billing_hash}\"\n")).map_err(|e| e.to_string())?;
      snapfire_fsr_sites::mount_all_with(Host::from(&root).unwrap(), &store).map_err(|e| e.to_string())?.build().map_err(|e| e.to_string())?;
      std::fs::write(markers.join(format!("{key}.fetched")), if cache.holds("status", "1.0.0") { "true" } else { "false" }).map_err(|e| e.to_string())?;
      let status_hash = snapfire_fsr_sites::hash_dir(&cache.path("status", "1.0.0")).map_err(|e| e.to_string())?;
      std::fs::write(root.join("config/local.toml"), format!("[sites.billing]\nhash = \"{billing_hash}\"\n\n[sites.status]\nhash = \"{status_hash}\"\n")).map_err(|e| e.to_string())
    })
    .unwrap();
    let fetched = std::fs::read_to_string(markers.join(format!("{key}.fetched"))).is_ok_and(|text| text == "true");
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
  response.headers().get_all(header::SET_COOKIE).iter().map(|v| v.to_str().unwrap()).find(|v| v.starts_with("sf_session=")).expect("a session cookie").split(';').next().unwrap().to_owned()
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
  assert!(html.contains(">Acme</a>") && html.contains("3 teams"), "the portal's header wraps the site: {html}");
  assert!(html.contains("Northwind") && html.contains("<!--sf-g:billing:routes/page.tsx#default-->") && html.contains("data-sf-module=\"billing:routes/layout.tsx#default\""), "{html}");
  assert!(html.contains("href=\"/billing/static/js/app/styles/billing.css\""), "the site's stylesheet rides under its prefix: {html}");

  let response = portal.handle(Request::get("/").body(Bytes::new()).unwrap()).await;
  assert_eq!(response.headers().get("x-portal").unwrap(), "portal");
  assert!(response.headers().get("x-billing").is_none());
  let html = body_of(response).await;
  assert!(html.contains("Billing") && html.contains("/billing") && !html.contains("billing.css"), "{html}");

  let payload = body_of(portal.handle(Request::get("/billing/invoice/1?__payload").body(Bytes::new()).unwrap()).await).await;
  assert!(payload.contains("Northwind") && payload.contains("\nT [{") && payload.contains("portal/who"), "the site's page carries the portal's seed: {payload}");
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
  let signed_in = response.headers().get_all(header::SET_COOKIE).iter().map(|v| v.to_str().unwrap()).find(|v| v.starts_with("sf_session=")).map(|v| v.split(';').next().unwrap().to_owned());
  assert_eq!(response.status(), StatusCode::SEE_OTHER, "{}", body_of(response).await);
  let cookie = signed_in.expect("the sign-in's session cookie");
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
  let response = portal
    .handle(Request::post("/auth/callback").header(header::COOKIE, &cookie).header(header::CONTENT_TYPE, "application/x-www-form-urlencoded").body(Bytes::from("user=alice&password=wonder")).unwrap())
    .await;
  let cookie = cookie_of(&response);

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
  assert_eq!(json["sites"][2]["name"], "reports");
  assert_eq!(json["sites"][2]["version"], "path", "a linked working tree");
  assert_eq!(json["sites"][3]["name"], "status");
  assert_eq!(json["sites"][3]["version"], "1.0.0", "fetched from the store");
}

#[tokio::test]
async fn the_first_boot_fetches_status_from_the_store() {
  assert!(deployed().fetched, "status@1.0.0 was not in the cache before the boot and is after it");
  let portal = portal();
  let response = portal.handle(Request::get("/status").body(Bytes::new()).unwrap()).await;
  assert_eq!(response.status(), StatusCode::OK);
  let html = body_of(response).await;
  assert!(html.contains("<h1>Status</h1>") && html.contains("<main class=\"mx-auto"), "under the portal's layout: {html}");
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
  assert!(html.contains("href=\"/blog/post/why-a-plan-file\"") && html.contains("href=\"/blog/static/js/app/styles/blog.css\""), "{html}");
  assert!(html.contains("<main class=\"mx-auto"), "under the portal's layout: {html}");

  let response = portal.handle(Request::get("/blog/post/static-under-a-shell").body(Bytes::new()).unwrap()).await;
  assert_eq!(response.status(), StatusCode::OK);
  let html = body_of(response).await;
  assert!(html.contains("<title>Static pages under a shell that reads the session · Blog</title>") && html.contains("<code>fsr prerender</code>"), "the markdown post and its frontmatter title: {html}");

  let response = portal.handle(Request::get("/blog/post/why-a-plan-file").body(Bytes::new()).unwrap()).await;
  assert_eq!(response.status(), StatusCode::OK);
  let html = body_of(response).await;
  assert!(html.contains("<pre class=\"hl-code\" data-language=\"ts\">"), "the MDX post's code was highlighted at build: {html}");
  assert!(html.contains("href=\"/blog/static/js/app/styles/code.css\""), "the site links its code sheet: {html}");
  assert!(html.contains("<aside class=\"my-4 rounded-lg"), "a component the MDX post imports: {html}");

  let response = portal.handle(Request::get("/blog/post/drain-before-stop").body(Bytes::new()).unwrap()).await;
  assert_eq!(response.status(), StatusCode::OK, "a post two directories under content/posts");
  assert!(body_of(response).await.contains("<h1 class=\"mb-2 font-serif text-3xl\">Drain before stop</h1>"));
  let response = portal.handle(Request::get("/blog/post/nope").body(Bytes::new()).unwrap()).await;
  assert_eq!(response.status(), StatusCode::NOT_FOUND, "a slug off the blog is a 404 document");
  assert!(body_of(response).await.contains("That did not load"), "rendered through the blog's error page");

  let out = std::env::temp_dir().join(format!("portal-prerender-{}", std::process::id()));
  let written = portal.prerender(&out).await.unwrap();
  assert!(written.iter().any(|(name, _)| name == "renders.json"), "the blog's fixed subtrees are rendered ahead: {written:?}");
  assert!(!written.iter().any(|(pattern, _)| pattern.starts_with("/blog")), "no document under the session-reading layout is written whole: {written:?}");
  std::fs::remove_dir_all(&out).unwrap();
}


#[tokio::test]
async fn the_portal_mounts_reports_with_a_client_of_its_own() {
  let portal = portal();
  let report = portal.report().to_string();
  assert!(report.contains("reports                at /reports from"), "{report}");

  let response = portal.handle(Request::get("/reports").body(Bytes::new()).unwrap()).await;
  assert_eq!(response.status(), StatusCode::OK);
  assert_eq!(response.headers().get("x-portal").unwrap(), "reports", "the portal's middleware saw the site");
  let html = body_of(response).await;
  assert!(html.contains(">Acme</a>") && html.contains("3 teams"), "the portal's header wraps the site: {html}");
  assert!(html.contains("<title>Reports</title>"), "the site's title: {html}");
  assert!(html.contains("Revenue by region") && html.contains("data-sf-module=\"reports:routes/layout.tsx#default\""), "rendered from the site's metrics client: {html}");
  assert!(html.contains("href=\"/reports/static/js/app/styles/reports.css\"") && html.contains("src=\"/reports/static/js/app/src/main.js\""), "the site's stylesheet and entry in the portal's head: {html}");
  assert!(html.contains("<!--sf-g:reports:routes/page.tsx#default-->"), "the site's segment keys in the portal's document: {html}");

  let css = portal.handle(Request::get("/reports/static/js/app/styles/reports.css").body(Bytes::new()).unwrap()).await;
  assert_eq!(css.status(), StatusCode::OK);
  assert!(body_of(css).await.contains(".figures"), "the site's static file");

  let html = body_of(portal.handle(Request::get("/reports/1").body(Bytes::new()).unwrap()).await).await;
  assert!(html.contains("Adding up the rows") && html.contains("<template data-sf-fill="), "the deferred segment streams through: {html}");
  assert!(html.contains("21100.00"), "{html}");
  assert_eq!(html.matches("href=\"/reports/static/js/app/styles/reports.css\"").count(), 1, "a page after the first links the site's stylesheet once: {html}");

  let payload = body_of(portal.handle(Request::get("/reports?__payload").body(Bytes::new()).unwrap()).await).await;
  assert!(payload.contains("E \"/reports/static/js/app/src/main.js\"") && payload.contains("\"k\":\"reports:routes/layout.tsx#default\""), "a navigation into the site carries its entry and keys: {payload}");
}

/// A candidate holding a billing whose ledger is down is refused by billing's
/// own probe list before it swaps in; a candidate whose directory lists no
/// teams by the portal's own check. The running tables serve on either way.
#[tokio::test]
async fn a_reload_is_refused_by_a_sites_probe_or_the_portals_check_and_the_running_version_serves_on() {
  use snapfire_fsr_core::Value;
  use snapfire_fsr_service::MockTransport;

  let portal = portal();
  let root: &Path = Path::new(env!("CARGO_MANIFEST_DIR"));
  let deployed = deployed();
  let before = body_of(portal.handle(Request::get("/billing").body(Bytes::new()).unwrap()).await).await;
  assert!(before.contains("Northwind"));

  let candidate = || snapfire_fsr_sites::mount_all_with(Host::from(root).unwrap(), &deployed.store).unwrap();
  let ledger_down = Arc::new(
    MockTransport::new()
      .returns("directory.listTeams", Value::seq(vec![Value::str("one")]))
      .fails("billing:ledger.listInvoices", snapfire_fsr_core::ext::FailureKind::Unavailable, "the ledger is down"),
  );
  let e = portal.reload_with_checked(candidate().services_over(ledger_down)).await.unwrap_err().to_string();
  assert!(e.contains("reload refused by a probe") && e.contains("billing") && e.contains("the ledger is down"), "{e}");
  let after = body_of(portal.handle(Request::get("/billing").body(Bytes::new()).unwrap()).await).await;
  assert!(after.contains("Northwind"), "the running version serves on");

  let shell_host = Arc::new(snapfire_fsr_sites::mount_all_with(Host::from(root).unwrap(), &deployed.store).unwrap().reload_check(Arc::new(portal_react_ts_check::TeamsListed)).build().unwrap());
  let invoice = || {
    let mut map = snapfire_fsr_core::ValueMap::default();
    map.insert("id".to_owned(), Value::int(1));
    map.insert("customer".to_owned(), Value::str("Northwind"));
    map.insert("total".to_owned(), Value::F64(1250.5));
    map.insert("status".to_owned(), Value::str("open"));
    Value::Map(map)
  };
  let no_teams = Arc::new(
    MockTransport::new()
      .returns("directory.listTeams", Value::seq(Vec::new()))
      .returns("billing:ledger.listInvoices", Value::seq(vec![invoice()]))
      .returns("billing:ledger.getInvoice", invoice()),
  );
  let e = shell_host.reload_with_checked(candidate().services_over(no_teams)).await.unwrap_err().to_string();
  assert!(e.contains("listing no teams"), "the portal's own check names its reason: {e}");
}

/// The portal's check, as `src/main.rs` registers it.
mod portal_react_ts_check {
  use futures_util::future::BoxFuture;
  use http::StatusCode;
  use snapfire_fsr_host::{Candidate, ReloadCheck};
  use snapfire_fsr_runtime::SessionCell;

  pub struct TeamsListed;

  impl ReloadCheck for TeamsListed {
    fn check<'a>(&'a self, candidate: &'a Candidate<'a>) -> BoxFuture<'a, Result<(), String>> {
      Box::pin(async move {
        let (status, html) = candidate.render("/", SessionCell::default()).await.map_err(|e| e.to_string())?;
        if status != StatusCode::OK || html.contains("0 teams") {
          return Err(format!("/ answered {status} listing no teams"));
        }
        Ok(())
      })
    }
  }
}

#[tokio::test]
async fn the_sitemap_lists_what_an_anonymous_visitor_reaches_across_the_sites() {
  let portal = portal();
  let get = |host: Arc<Host>, path: &'static str| async move { body_of(host.handle(Request::get(path).body(Bytes::new()).unwrap()).await).await };

  let robots = get(portal.clone(), "/robots.txt").await;
  assert!(robots.contains("Disallow: /auth/") && robots.contains("Sitemap: http://127.0.0.1:8100/sitemap.xml"), "{robots}");

  let own = get(portal.clone(), "/sitemap.xml").await;
  for listed in ["8100/</loc>", "8100/login</loc>", "8100/billing</loc>", "8100/blog</loc>", "8100/blog/post/why-a-plan-file</loc>", "8100/status</loc>", "8100/reports</loc>", "8100/reports/1</loc>", "8100/reports/3</loc>"] {
    assert!(own.contains(listed), "{listed}: {own}");
  }
  assert!(!own.contains("/account"), "the portal's middleware turns a signed-out visitor away: {own}");
  assert!(!own.contains("/billing/overdue"), "billing's middleware does the same under the mount: {own}");
}

#[tokio::test]
async fn a_card_image_is_drawn_for_the_shell_and_each_site_and_named_in_its_page() {
  let portal = portal();
  let get = |path: &'static str, accept: &'static str| {
    let portal = portal.clone();
    async move { portal.handle(Request::get(path).header(header::ACCEPT, accept).body(Bytes::new()).unwrap()).await }
  };
  for (page, image) in [
    ("/", "/opengraph-image.png"),
    ("/blog/post/why-a-plan-file", "/blog/post/why-a-plan-file/opengraph-image.png"),
    ("/reports/1", "/reports/1/opengraph-image.png"),
  ] {
    let html = body_of(get(page, "text/html").await).await;
    assert!(html.contains(&format!("<meta property=\"og:image\" content=\"http://127.0.0.1:8100{image}?v=")), "{page} names its image: {html}");
    assert!(html.contains("<meta property=\"og:image:width\" content=\"1200\">"), "{page}: {html}");
    let response = get(image, "image/avif,image/webp,*/*").await;
    assert_eq!(response.status(), StatusCode::OK, "{image}");
    assert_eq!(response.headers().get(header::CONTENT_TYPE).unwrap(), "image/png", "{image}");
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    assert_eq!(&bytes[..8], b"\x89PNG\r\n\x1a\n", "{image}");
    assert_eq!((u32::from_be_bytes(bytes[16..20].try_into().unwrap()), u32::from_be_bytes(bytes[20..24].try_into().unwrap())), (1200, 630), "{image}");
  }
  let missing = get("/blog/post/nowhere/opengraph-image.png", "*/*").await;
  assert_eq!(missing.status(), StatusCode::NOT_FOUND, "the post's loader answers not found for its image too");

  for page in ["/blog", "/blog/tags"] {
    let html = body_of(get(page, "text/html").await).await;
    assert!(html.contains(&format!("<meta property=\"og:image\" content=\"http://127.0.0.1:8100{page}/opengraph-image.png?v=")), "the blog's root image covers {page}, the post's nearer one stays its own: {html}");
    let image = get(Box::leak(format!("{page}/opengraph-image.png").into_boxed_str()), "*/*").await;
    assert_eq!(image.status(), StatusCode::OK, "{page}: drawn from the image's own loader");
  }

  let html = body_of(get("/reports/1", "text/html").await).await;
  for id in ["total", "rows"] {
    assert!(html.contains(&format!("<meta name=\"twitter:image\" content=\"http://127.0.0.1:8100/reports/1/twitter-image/{id}?v=")), "each listed image named: {html}");
  }
  let rows = get("/reports/1/twitter-image/rows", "*/*").await;
  assert_eq!(rows.status(), StatusCode::OK);
  let bytes = rows.into_body().collect().await.unwrap().to_bytes();
  assert_eq!((u32::from_be_bytes(bytes[16..20].try_into().unwrap()), u32::from_be_bytes(bytes[20..24].try_into().unwrap())), (1200, 900), "the listed image's own size");
}

#[tokio::test]
async fn a_reports_pages_head_elements_reach_the_portals_document() {
  let portal = portal();
  let html = body_of(portal.handle(Request::get("/reports").header(header::ACCEPT, "text/html").body(Bytes::new()).unwrap()).await).await;
  assert!(html.contains("<meta property=\"og:type\" content=\"website\">"), "the head element the reports index's meta returns: {html}");
}

#[tokio::test]
async fn the_reports_sites_get_handler_answers_through_the_portal_whatever_the_client_accepts() {
  let portal = portal();
  for accept in ["*/*", "application/json"] {
    let response = portal.handle(Request::get("/reports/1/rows").header(header::ACCEPT, accept).body(Bytes::new()).unwrap()).await;
    let status = response.status();
    let body = body_of(response).await;
    assert_eq!(status, StatusCode::OK, "{accept}: {body}");
    assert!(body.contains("Revenue by region"), "{accept}: answered by the reports site's handler: {body}");
  }
}

/// Classes only vendored packages name reach the shell's stylesheet: `@acme/ui` from the class
/// list its build wrote, a lookup table's included; `@acme/charts`, which ships none, from its
/// minified module.
#[tokio::test]
async fn the_shell_stylesheet_carries_the_classes_its_vendored_packages_name() {
  let portal = portal();
  let html = body_of(portal.handle(Request::get("/themes").body(Bytes::new()).unwrap()).await).await;
  assert!(html.contains("data-sf-module=\"src/ui/Vendored.tsx#Vendored\""), "{html}");
  let css = body_of(portal.handle(Request::get("/static/js/app/styles/app.css").body(Bytes::new()).unwrap()).await).await;
  for class in [".bg-violet-100", ".bg-teal-100", ".ring-inset", ".size-1\\.5", ".bg-fuchsia-500", ".text-fuchsia-700", ".tracking-tighter"] {
    assert!(css.contains(class), "{class}");
  }
}

#[tokio::test]
async fn the_shells_public_files_answer_beside_the_crawl_files_and_the_mounted_sites() {
  let portal = portal();
  let response = portal.handle(Request::get("/.well-known/security.txt").body(Bytes::new()).unwrap()).await;
  assert_eq!(response.status(), StatusCode::OK);
  assert!(body_of(response).await.starts_with("Contact: mailto:security@acme.example"));
  assert!(body_of(portal.handle(Request::get("/humans.txt").body(Bytes::new()).unwrap()).await).await.contains("Acme platform team"));
  assert!(body_of(portal.handle(Request::get("/robots.txt").body(Bytes::new()).unwrap()).await).await.contains("Disallow: /_sf/"), "[crawl] still writes robots.txt");
  let response = portal.handle(Request::get("/billing").body(Bytes::new()).unwrap()).await;
  assert_eq!(response.status(), StatusCode::OK, "a mounted site's path falls through to it");
}
