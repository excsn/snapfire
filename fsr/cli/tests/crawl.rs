//! `[crawl]`: the robots text, and a sitemap of every page an anonymous
//! visitor is served, asked of the host through its own front door.

use std::path::{Path, PathBuf};

use bytes::Bytes;
use http::Request;
use http_body_util::BodyExt;
use snapfire_fsr_cli::new::{create, NewOptions};
use snapfire_fsr_cli::{build, write, Options};
use snapfire_fsr_host::config::Config;
use snapfire_fsr_host::Host;

static NEXT: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);

const PAGE: &str = "export default function Page() {\n  return <main>page</main>;\n}\n";
const LOADER: &str = "export async function load() {\n  return { n: 1 };\n}\n";
const DATED: &str = "import { modified } from \"@snapfire/fsr/head\";\nexport async function load() {\n  return { n: 1 };\n}\nexport const meta = () => ({ head: [modified(\"2026-07-04\")] });\n";

fn project(tag: &str, crawl: &str, extra: &str, files: &[(&str, &str)]) -> PathBuf {
  let n = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
  let root = std::env::temp_dir().join(format!("fsr-cli-crawl-{}-{n}-{tag}", std::process::id()));
  let _ = std::fs::remove_dir_all(&root);
  create(&root, NewOptions::default().fetch(false)).unwrap();
  let app = root.join("app");
  for (path, text) in files {
    let file = app.join(path);
    std::fs::create_dir_all(file.parent().unwrap()).unwrap();
    std::fs::write(file, text).unwrap();
  }
  let config = root.join("config/app.toml");
  let text = std::fs::read_to_string(&config).unwrap();
  let text = text.replacen("[document]\n", "[document]\norigin = \"https://shop.example\"\n", 1);
  std::fs::write(&config, format!("{text}\n{crawl}\n{extra}")).unwrap();
  root
}

fn host(root: &Path) -> Host {
  let app = root.join("app");
  let built = build(&app, &Options::beside(&app)).unwrap_or_else(|e| panic!("builds: {e}"));
  write(&app, &built).unwrap();
  Host::from_config(Config::load(root).unwrap()).unwrap().build().unwrap_or_else(|e| panic!("boots: {e}"))
}

fn get(host: &Host, path: &str) -> (u16, String, String) {
  let runtime = tokio::runtime::Runtime::new().unwrap();
  runtime.block_on(async {
    let response = host.handle(Request::get(path).body(Bytes::new()).unwrap()).await;
    let status = response.status().as_u16();
    let kind = response.headers().get("content-type").and_then(|v| v.to_str().ok()).unwrap_or("").to_owned();
    let body = response.into_body().collect().await.unwrap().to_bytes();
    (status, kind, String::from_utf8_lossy(&body).into_owned())
  })
}

fn site_files() -> Vec<(&'static str, &'static str)> {
  vec![
    ("routes/about/page.tsx", PAGE),
    ("routes/about/page.loader.ts", DATED),
    ("routes/hidden/page.tsx", PAGE),
    ("routes/hidden/page.loader.ts", "import { robots } from \"@snapfire/fsr/head\";\nexport async function load() {\n  return { n: 1 };\n}\nexport const meta = () => ({ head: [robots(\"noindex, follow\")] });\n"),
    ("routes/account/page.tsx", PAGE),
    ("routes/post/[slug]/page.tsx", PAGE),
    ("routes/post/[slug]/page.loader.ts", "import type { Ctx } from \"@snapfire/fsr\";\nimport { modified } from \"@snapfire/fsr/head\";\nconst POSTS = [{ slug: \"first\", updated: \"2026-05-01T08:00:00Z\" }, { slug: \"second\", updated: \"2026-05-02\" }];\nexport async function load({ params }: Ctx<\"/post/{slug}\">) {\n  return { updated: POSTS.find((p) => p.slug === params.slug)?.updated ?? \"\" };\n}\nexport const meta = ({ data }: { data: { updated: string } }) => ({ head: [modified(data.updated)] });\nexport const paths = () => POSTS.map((p) => ({ slug: p.slug }));\n"),
    ("routes/item/[id]/page.tsx", PAGE),
    ("routes/item/[id]/page.loader.ts", "import type { Ctx } from \"@snapfire/fsr\";\nexport async function load({ params }: Ctx<\"/item/{id}\">) {\n  return { id: params.id };\n}\n"),
    ("routes/api/ping/route.ts", "export async function GET() {\n  return { ok: true };\n}\n"),
    ("middleware.ts", "import type { MiddlewareCtx, MiddlewareResult } from \"@snapfire/fsr\";\nexport async function middleware({ request, identity }: MiddlewareCtx): Promise<MiddlewareResult> {\n  if (request.path.startsWith(\"/account\") && !identity?.subject) return { redirect: \"/\" };\n  return {};\n}\n"),
  ]
}

#[test]
fn the_sitemap_lists_what_an_anonymous_visitor_is_served_and_robots_names_it() {
  let root = project("site", "[crawl]\nrobots = \"/robots.txt\"\nsitemap = \"/sitemap.xml\"\ndisallow = [\"/drafts/\"]\n", "", &site_files());
  let host = host(&root);
  let (status, kind, robots) = get(&host, "/robots.txt");
  assert_eq!(status, 200);
  assert!(kind.starts_with("text/plain"), "{kind}");
  assert_eq!(robots, "User-agent: *\nAllow: /\nDisallow: /__fsr/\nDisallow: /_sf/\nDisallow: /drafts/\nSitemap: https://shop.example/sitemap.xml\n");
  let (status, kind, sitemap) = get(&host, "/sitemap.xml");
  assert_eq!(status, 200, "{sitemap}");
  assert!(kind.starts_with("application/xml"), "{kind}");
  for listed in ["https://shop.example/</loc>", "https://shop.example/about</loc>", "https://shop.example/post/first</loc>", "https://shop.example/post/second</loc>"] {
    assert!(sitemap.contains(listed), "{listed} is listed: {sitemap}");
  }
  assert!(!sitemap.contains("/hidden"), "a page whose meta says noindex is left out: {sitemap}");
  assert!(!sitemap.contains("/account"), "a page the middleware redirects an anonymous visitor from is left out: {sitemap}");
  assert!(!sitemap.contains("/item"), "a parameter with no `paths` export is left out: {sitemap}");
  assert!(!sitemap.contains("/api/ping"), "a handler's JSON is no document: {sitemap}");
  assert!(!sitemap.contains("xhtml"), "one locale writes no alternates: {sitemap}");
  assert!(sitemap.contains("<loc>https://shop.example/about</loc>\n    <lastmod>2026-07-04</lastmod>"), "a page's modified date is its lastmod: {sitemap}");
  assert!(sitemap.contains("<loc>https://shop.example/post/first</loc>\n    <lastmod>2026-05-01T08:00:00Z</lastmod>"), "each path's date comes from its own render: {sitemap}");
  assert!(sitemap.contains("<loc>https://shop.example/post/second</loc>\n    <lastmod>2026-05-02</lastmod>"), "{sitemap}");
  assert!(sitemap.contains("<loc>https://shop.example/</loc>\n  </url>"), "a page naming no date has no lastmod: {sitemap}");
  let report = host.report().to_string();
  assert!(report.contains("/item/{id}") && report.contains("no `paths` export"), "{report}");
  std::fs::remove_dir_all(&root).unwrap();
}

#[test]
fn each_locale_is_listed_with_its_alternates() {
  let root = project("locales", "[crawl]\nsitemap = \"/sitemap.xml\"\n", "[locales]\nsupported = [\"en\", \"fr_FR\"]\n", &[("routes/about/page.tsx", PAGE)]);
  let host = host(&root);
  let (status, _, sitemap) = get(&host, "/sitemap.xml");
  assert_eq!(status, 200, "{sitemap}");
  assert!(sitemap.contains("<loc>https://shop.example/about</loc>") && sitemap.contains("<loc>https://shop.example/fr_FR/about</loc>"), "{sitemap}");
  assert!(sitemap.contains("hreflang=\"fr-FR\" href=\"https://shop.example/fr_FR/about\""), "{sitemap}");
  assert!(sitemap.contains("hreflang=\"x-default\" href=\"https://shop.example/about\""), "{sitemap}");
  assert!(sitemap.contains("<loc>https://shop.example/fr_FR</loc>"), "the root in a locale has no trailing slash: {sitemap}");
  std::fs::remove_dir_all(&root).unwrap();
}

#[test]
fn each_file_is_served_where_its_key_says_and_only_then() {
  let root = project("moved", "[crawl]\nsitemap = \"/crawl/map.xml\"\n", "", &[("routes/about/page.tsx", PAGE), ("routes/about/page.loader.ts", LOADER)]);
  let host = host(&root);
  assert_eq!(get(&host, "/robots.txt").0, 404);
  assert_eq!(get(&host, "/sitemap.xml").0, 404);
  let (status, _, sitemap) = get(&host, "/crawl/map.xml");
  assert_eq!(status, 200);
  assert!(sitemap.contains("https://shop.example/about"), "{sitemap}");
  std::fs::remove_dir_all(&root).unwrap();
}

#[test]
fn crawl_without_an_origin_refuses_to_load() {
  let root = project("origin", "[crawl]\nsitemap = \"/sitemap.xml\"\n", "", &[]);
  let config = root.join("config/app.toml");
  let text = std::fs::read_to_string(&config).unwrap().replace("origin = \"https://shop.example\"\n", "");
  std::fs::write(&config, text).unwrap();
  let error = Config::load(&root).err().expect("refused").to_string();
  assert!(error.contains("crawl needs `document.origin`"), "{error}");
  std::fs::remove_dir_all(&root).unwrap();
}

#[test]
fn the_application_refreshes_the_sitemap_when_it_chooses_and_requests_keep_the_one_held() {
  let root = project("refresh", "[crawl]\nsitemap = \"/sitemap.xml\"\n", "", &[("routes/about/page.tsx", PAGE)]);
  let host = host(&root);
  let runtime = tokio::runtime::Runtime::new().unwrap();
  runtime.block_on(host.refresh_sitemap()).expect("a refresh before any request builds the files");
  let (status, _, first) = get(&host, "/sitemap.xml");
  assert_eq!(status, 200);
  assert!(first.contains("https://shop.example/about</loc>"), "{first}");
  runtime.block_on(host.refresh_sitemap()).expect("and again");
  assert_eq!(get(&host, "/sitemap.xml").2, first, "the same pages build the same file");
  std::fs::remove_dir_all(&root).unwrap();
}
