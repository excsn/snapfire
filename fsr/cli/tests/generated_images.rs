//! An `opengraph-image.tsx` or `twitter-image.tsx` beside a page: lowered to a
//! component, drawn at its own URL with the page loader's data and named in
//! the page's head.

use std::path::PathBuf;

use bytes::Bytes;
use snapfire_fsr_cli::new::{create, NewOptions};
use snapfire_fsr_cli::{build, write, Options};
use snapfire_fsr_host::config::Config;
use snapfire_fsr_host::{Host, RenderMode};
use snapfire_fsr_runtime::SessionCell;

static NEXT: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);

const PAGE: &str = "export default function Page({ notice }: { notice: { title: string } }) {\n  return <main>{notice.title}</main>;\n}\n";

const LOADER: &str = r#"import { fail } from "@snapfire/fsr/host";
import type { Ctx } from "@snapfire/fsr/host";

const notices = [{ id: "roof", title: "Roof work on the east stair" }];

export async function load({ params }: Ctx<"/notice/{id}">) {
  const found = notices.filter((n) => n.id === params.id);
  if (found.length === 0) fail("not_found", `no notice ${params.id}`);
  return { notice: found[0] };
}
"#;

const IMAGE: &str = r##"export const size = { width: 600, height: 315 };
export const alt = "A notice";

export default function Image({ notice, params }: { notice: { title: string }; params: { id: string } }) {
  return (
    <div style={{ display: "flex", flexDirection: "column", width: "100%", height: "100%", padding: 32, backgroundColor: "#1e3a8a", color: "white" }}>
      <span style={{ fontSize: 20 }}>Notice {params.id}</span>
      <h1 style={{ fontSize: 48, margin: 0 }}>{notice.title}</h1>
    </div>
  );
}
"##;

fn project(tag: &str, files: &[(&str, &str)]) -> PathBuf {
  let n = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
  let root = std::env::temp_dir().join(format!("fsr-cli-generated-{}-{n}-{tag}", std::process::id()));
  let _ = std::fs::remove_dir_all(&root);
  create(&root, NewOptions::default().fetch(false)).unwrap();
  let app = root.join("app");
  for (path, text) in files {
    let file = app.join(path);
    std::fs::create_dir_all(file.parent().unwrap()).unwrap();
    std::fs::write(file, text).unwrap();
  }
  let config = root.join("config/app.toml");
  let text = std::fs::read_to_string(&config).unwrap().replacen("[document]\n", "[document]\norigin = \"https://board.example\"\n", 1);
  std::fs::write(&config, text).unwrap();
  root
}

fn get(host: &Host, path: &str) -> (u16, String, Vec<u8>) {
  let req = http::Request::get(path).header("accept", "image/avif,image/webp,*/*").body(Bytes::new()).unwrap();
  tokio::runtime::Runtime::new().unwrap().block_on(async {
    use http_body_util::BodyExt as _;
    let response = host.handle(req).await;
    let status = response.status().as_u16();
    let kind = response.headers().get("content-type").and_then(|v| v.to_str().ok()).unwrap_or("").to_owned();
    let bytes = response.into_body().collect().await.unwrap().to_bytes().to_vec();
    (status, kind, bytes)
  })
}

/// The width and height a PNG's header carries.
fn png_size(bytes: &[u8]) -> (u32, u32) {
  assert_eq!(&bytes[..8], b"\x89PNG\r\n\x1a\n");
  (u32::from_be_bytes(bytes[16..20].try_into().unwrap()), u32::from_be_bytes(bytes[20..24].try_into().unwrap()))
}

#[test]
fn a_generated_image_is_drawn_from_the_page_data_at_its_own_url_and_named_in_the_head() {
  let root = project("drawn", &[("routes/notice/[id]/page.tsx", PAGE), ("routes/notice/[id]/page.loader.ts", LOADER), ("routes/notice/[id]/opengraph-image.tsx", IMAGE)]);
  let app = root.join("app");
  let built = build(&app, &Options::beside(&app)).unwrap_or_else(|e| panic!("builds: {e}"));
  write(&app, &built).unwrap();
  let host = Host::from_config(Config::load(&root).unwrap()).unwrap().build().unwrap_or_else(|e| panic!("boots: {e}"));

  let page = tokio::runtime::Runtime::new().unwrap().block_on(host.render_to_string("/notice/roof", RenderMode::Html, SessionCell::default())).unwrap();
  assert!(page.contains("<meta property=\"og:image\" content=\"https://board.example/notice/roof/opengraph-image.png?v="), "the page names its own image, absolute: {page}");
  assert!(page.contains("<meta property=\"og:image:width\" content=\"600\">") && page.contains("<meta property=\"og:image:height\" content=\"315\">"), "{page}");
  assert!(page.contains("<meta property=\"og:image:type\" content=\"image/png\">") && page.contains("<meta property=\"og:image:alt\" content=\"A notice\">"), "{page}");

  let (status, kind, bytes) = get(&host, "/notice/roof/opengraph-image.png");
  assert_eq!((status, kind.as_str()), (200, "image/png"), "{}", String::from_utf8_lossy(&bytes));
  assert_eq!(png_size(&bytes), (600, 315));

  let (status, _, _) = get(&host, "/notice/nowhere/opengraph-image.png");
  assert_eq!(status, 404, "the loader's not found answers the image too");
  std::fs::remove_dir_all(&root).unwrap();
}

#[test]
fn a_generated_image_the_build_cannot_draw_is_refused() {
  let stateful = "import { useState } from \"react\";\nexport default function Image() {\n  const [n] = useState(0);\n  return <div onClick={() => {}}>{n}</div>;\n}\n";
  let webp = "export const contentType = \"image/webp\";\nexport default function Image() {\n  return <div>x</div>;\n}\n";
  let computed = "const w = 1200;\nexport const size = { width: w, height: 630 };\nexport default function Image() {\n  return <div>x</div>;\n}\n";
  let plain = "export default function Image() {\n  return <div>x</div>;\n}\n";
  for (tag, files, says) in [
    ("state", vec![("routes/page.tsx", PAGE), ("routes/opengraph-image.tsx", stateful)], "state or a handler"),
    ("webp", vec![("routes/page.tsx", PAGE), ("routes/opengraph-image.tsx", webp)], "image/png` or `image/jpeg"),
    ("computed", vec![("routes/page.tsx", PAGE), ("routes/opengraph-image.tsx", computed)], "literal numbers"),
    ("orphan", vec![("routes/page.tsx", PAGE), ("routes/deep/opengraph-image.tsx", plain)], "there is none"),
  ] {
    let root = project(tag, &files);
    let app = root.join("app");
    let error = build(&app, &Options::beside(&app)).err().unwrap_or_else(|| panic!("{tag}: refused")).to_string();
    assert!(error.contains(says), "{tag}: {error}");
    std::fs::remove_dir_all(&root).unwrap();
  }
}

fn boot(root: &std::path::Path) -> Host {
  let app = root.join("app");
  let built = build(&app, &Options::beside(&app)).unwrap_or_else(|e| panic!("builds: {e}"));
  write(&app, &built).unwrap();
  Host::from_config(Config::load(root).unwrap()).unwrap().build().unwrap_or_else(|e| panic!("boots: {e}"))
}

fn page_html(host: &Host, path: &str) -> String {
  tokio::runtime::Runtime::new().unwrap().block_on(host.render_to_string(path, RenderMode::Html, SessionCell::default())).unwrap()
}

const LAYOUT: &str = "export default function Layout({ children }: { children: unknown }) {\n  return <section>{children}</section>;\n}\n";

#[test]
fn an_image_beside_a_layout_covers_the_pages_under_it_with_the_layouts_data_and_a_nearer_one_replaces_it() {
  let board = "export default function Image({ board, params }: { board: string; params: { id?: string } }) {\n  return <div style={{ display: \"flex\", width: \"100%\", height: \"100%\", backgroundColor: \"#000\", color: \"#fff\" }}>{board} {params.id}</div>;\n}\n";
  let root = project("layout", &[
    ("routes/notice/layout.tsx", LAYOUT),
    ("routes/notice/layout.loader.ts", "export async function load() {\n  return { board: \"Main board\" };\n}\n"),
    ("routes/notice/opengraph-image.tsx", board),
    ("routes/notice/[id]/page.tsx", PAGE),
    ("routes/notice/[id]/page.loader.ts", LOADER),
    ("routes/notice/archive/page.tsx", "export default function Page() {\n  return <main>archive</main>;\n}\n"),
    ("routes/notice/archive/opengraph-image.tsx", "export const size = { width: 400, height: 200 };\nexport default function Image() {\n  return <div>archive</div>;\n}\n"),
  ]);
  let host = boot(&root);
  let page = page_html(&host, "/notice/roof");
  assert!(page.contains("<meta property=\"og:image\" content=\"https://board.example/notice/roof/opengraph-image.png?v="), "{page}");
  let (status, _, bytes) = get(&host, "/notice/roof/opengraph-image.png");
  assert_eq!(status, 200, "{}", String::from_utf8_lossy(&bytes));
  assert_eq!(png_size(&bytes), (1200, 630), "the layout's image at its default size");
  let archive = page_html(&host, "/notice/archive");
  assert!(archive.contains("/notice/archive/opengraph-image.png?v=") && archive.contains("<meta property=\"og:image:width\" content=\"400\">"), "the nearer image replaces the layout's: {archive}");
  let (_, _, bytes) = get(&host, "/notice/archive/opengraph-image.png");
  assert_eq!(png_size(&bytes), (400, 200));
  std::fs::remove_dir_all(&root).unwrap();
}

#[test]
fn an_image_with_its_own_loader_draws_from_that_loaders_data() {
  let root = project("loader", &[
    ("routes/about/page.tsx", "export default function Page() {\n  return <main>about</main>;\n}\n"),
    ("routes/about/opengraph-image.tsx", "export default function Image({ headline }: { headline: string }) {\n  return <div>{headline}</div>;\n}\n"),
    ("routes/about/opengraph-image.loader.ts", "import { fail } from \"@snapfire/fsr/host\";\nexport async function load() {\n  fail(\"not_found\", \"the card has no headline yet\");\n  return { headline: \"\" };\n}\n"),
  ]);
  let host = boot(&root);
  let (status, _, body) = get(&host, "/about/opengraph-image.png");
  assert_eq!(status, 404, "the image's own loader ran, not the page's absent one");
  assert!(String::from_utf8_lossy(&body).contains("the card has no headline yet"));
  std::fs::remove_dir_all(&root).unwrap();
}

#[test]
fn an_image_beside_a_page_covers_the_pages_under_it_with_that_pages_data() {
  let root = project("beside", &[
    ("routes/notice/[id]/page.tsx", PAGE),
    ("routes/notice/[id]/page.loader.ts", LOADER),
    ("routes/notice/[id]/opengraph-image.tsx", IMAGE),
    ("routes/notice/[id]/history/page.tsx", "export default function Page({ entries }: { entries: number }) {\n  return <main>{entries}</main>;\n}\n"),
    ("routes/notice/[id]/history/page.loader.ts", "export async function load() {\n  return { entries: 3 };\n}\n"),
  ]);
  let host = boot(&root);
  let page = page_html(&host, "/notice/roof/history");
  assert!(page.contains("<meta property=\"og:image\" content=\"https://board.example/notice/roof/history/opengraph-image.png?v="), "{page}");
  let (status, kind, bytes) = get(&host, "/notice/roof/history/opengraph-image.png");
  assert_eq!((status, kind.as_str()), (200, "image/png"), "{}", String::from_utf8_lossy(&bytes));
  assert_eq!(png_size(&bytes), (600, 315));
  let (status, _, body) = get(&host, "/notice/nowhere/history/opengraph-image.png");
  assert_eq!(status, 404, "the notice page's loader ran for the history page's params: {}", String::from_utf8_lossy(&body));
  assert!(String::from_utf8_lossy(&body).contains("no notice nowhere"));
  std::fs::remove_dir_all(&root).unwrap();
}

#[test]
fn a_module_listing_its_images_names_each_in_the_head_and_draws_each_at_its_id() {
  let listed = "export function images({ params }: { params: { id: string } }) {\n  return [{ id: \"small\", size: { width: 300, height: 150 }, alt: `Small ${params.id}` }, { id: \"wide\" }];\n}\n\nexport default function Image({ id }: { id: string }) {\n  return <div style={{ display: \"flex\", width: \"100%\", height: \"100%\" }}>{id}</div>;\n}\n";
  let root = project("listed", &[("routes/notice/[id]/page.tsx", PAGE), ("routes/notice/[id]/page.loader.ts", LOADER), ("routes/notice/[id]/opengraph-image.tsx", listed)]);
  let host = boot(&root);
  let page = page_html(&host, "/notice/roof");
  assert!(page.contains("<meta property=\"og:image\" content=\"https://board.example/notice/roof/opengraph-image/small?v="), "{page}");
  assert!(page.contains("<meta property=\"og:image\" content=\"https://board.example/notice/roof/opengraph-image/wide?v="), "every listed image keeps its row: {page}");
  assert!(page.contains("<meta property=\"og:image:width\" content=\"300\">") && page.contains("<meta property=\"og:image:width\" content=\"1200\">") && page.contains("<meta property=\"og:image:alt\" content=\"Small roof\">"), "{page}");
  let (status, kind, bytes) = get(&host, "/notice/roof/opengraph-image/small");
  assert_eq!((status, kind.as_str()), (200, "image/png"), "{}", String::from_utf8_lossy(&bytes));
  assert_eq!(png_size(&bytes), (300, 150));
  let (_, _, bytes) = get(&host, "/notice/roof/opengraph-image/wide");
  assert_eq!(png_size(&bytes), (1200, 630));
  let (status, _, _) = get(&host, "/notice/roof/opengraph-image/nope");
  assert_eq!(status, 404, "an id the list does not name");
  std::fs::remove_dir_all(&root).unwrap();
}

#[test]
fn a_prerender_writes_each_pages_images_and_production_serves_them() {
  let paths = format!("{LOADER}\nexport const paths = () => [{{ id: \"roof\" }}];\n");
  let root = project("prerender", &[("routes/notice/[id]/page.tsx", PAGE), ("routes/notice/[id]/page.loader.ts", &paths), ("routes/notice/[id]/opengraph-image.tsx", IMAGE)]);
  let config = root.join("config/app.toml");
  let text = std::fs::read_to_string(&config).unwrap().replacen("[server]\n", "[server]\nprerender = \"dist/prerender\"\n", 1);
  std::fs::write(&config, text).unwrap();
  let host = boot(&root);
  let dir = root.join("app/dist/prerender");
  let written = tokio::runtime::Runtime::new().unwrap().block_on(host.prerender(&dir)).unwrap();
  let image = dir.join("notice/roof/opengraph-image.png");
  assert!(written.iter().any(|(served, file)| served == "/notice/roof/opengraph-image.png" && *file == image), "{written:?}");
  assert_eq!(png_size(&std::fs::read(&image).unwrap()), (600, 315));

  std::fs::write(&image, solid_png()).unwrap();
  let mut production = Config::load(&root).unwrap();
  production.server.dev = Some(false);
  production.session.key = Some("a-test-key-of-sixty-four-characters-for-the-production-boot-0123".to_owned());
  let host = Host::from_config(production).unwrap().build().unwrap();
  let (status, kind, bytes) = get(&host, "/notice/roof/opengraph-image.png");
  assert_eq!((status, kind.as_str()), (200, "image/png"));
  assert_eq!(png_size(&bytes), (2, 1), "production answers from the prerendered file");
  std::fs::remove_dir_all(&root).unwrap();
}

/// A 2 by 1 PNG.
fn solid_png() -> Vec<u8> {
  vec![137, 80, 78, 71, 13, 10, 26, 10, 0, 0, 0, 13, 73, 72, 68, 82, 0, 0, 0, 2, 0, 0, 0, 1, 8, 2, 0, 0, 0, 123, 64, 232, 221, 0, 0, 0, 14, 73, 68, 65, 84, 120, 156, 99, 96, 96, 96, 0, 0, 0, 6, 0, 1, 5, 103, 225, 201, 0, 0, 0, 0, 73, 69, 78, 68, 174, 66, 96, 130]
}

#[test]
fn a_font_entry_names_the_scripts_its_faces_draw_and_an_unknown_one_refuses_the_boot() {
  let woff2 = std::fs::read(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/Fraunces-vietnamese.woff2")).unwrap();
  for (scripts, boots) in [("[\"Latin\"]", true), ("[\"Klingon\"]", false)] {
    let root = project("scripts", &[("routes/page.tsx", PAGE), ("routes/opengraph-image.tsx", "export default function Image() {\n  return <div>Ơ</div>;\n}\n")]);
    std::fs::create_dir_all(root.join("app/fonts")).unwrap();
    std::fs::write(root.join("app/fonts/Fraunces-vietnamese.woff2"), &woff2).unwrap();
    let config = root.join("config/app.toml");
    let text = std::fs::read_to_string(&config).unwrap() + &format!("\n[fonts.serif]\nfamily = \"Fraunces\"\nscripts = {scripts}\n");
    std::fs::write(&config, text).unwrap();
    let app = root.join("app");
    let built = build(&app, &Options::beside(&app)).unwrap_or_else(|e| panic!("builds: {e}"));
    write(&app, &built).unwrap();
    let booted = Host::from_config(Config::load(&root).unwrap()).unwrap().build();
    match (booted, boots) {
      (Ok(_), true) => {}
      (Err(e), false) => assert!(e.to_string().contains("fonts.serif.scripts") && e.to_string().contains("Klingon"), "{e}"),
      (Ok(_), false) => panic!("an unknown script booted"),
      (Err(e), true) => panic!("refused: {e}"),
    }
    std::fs::remove_dir_all(&root).unwrap();
  }
}

#[test]
fn an_image_with_its_own_loader_gets_a_generated_props_type() {
  let root = project("typed", &[
    ("routes/page.tsx", "export default function Page() {\n  return <main>home</main>;\n}\n"),
    ("routes/opengraph-image.tsx", "import type { RootOpengraphImageProps } from \"@generated/client\";\nexport default function Image({ count }: RootOpengraphImageProps) {\n  return <div>{count}</div>;\n}\n"),
    ("routes/opengraph-image.loader.ts", "export async function load() {\n  return { count: 3, latest: \"A post\" };\n}\n"),
    ("routes/notice/[id]/page.tsx", PAGE),
    ("routes/notice/[id]/page.loader.ts", LOADER),
    ("routes/notice/[id]/twitter-image.tsx", "export default function Image({ title }: { title: string }) {\n  return <div>{title}</div>;\n}\n"),
    ("routes/notice/[id]/twitter-image.loader.ts", "export async function load() {\n  return { title: \"Notice\" };\n}\n"),
    ("routes/notice/[id]/opengraph-image.tsx", IMAGE),
  ]);
  let app = root.join("app");
  let built = build(&app, &Options::beside(&app)).unwrap_or_else(|e| panic!("builds: {e}"));
  let client = &built.files.iter().find(|(name, _)| name == "generated/client.ts").unwrap().1;
  assert!(client.contains("export type RootOpengraphImageProps = Wire<Awaited<ReturnType<typeof import(\"../routes/opengraph-image.loader\")[\"load\"]>>>;"), "{client}");
  assert!(client.contains("export type NoticeIdTwitterImageProps = Wire<Awaited<ReturnType<typeof import(\"../routes/notice/[id]/twitter-image.loader\")[\"load\"]>>>;"), "{client}");
  assert_eq!(client.matches("export type NoticeIdProps =").count(), 1, "an image drawing from its page's data adds no second page type: {client}");
  std::fs::remove_dir_all(&root).unwrap();
}

const DOC_PAGE: &str = "export default function Page({ doc }: { doc: { title: string } }) {\n  return <main>{doc.title}</main>;\n}\n";

const DOC_LOADER: &str = r#"import { fail } from "@snapfire/fsr/host";
import type { Ctx } from "@snapfire/fsr/host";

export async function load({ params }: Ctx<"/docs/{*rest}">) {
  if (params.rest === "missing") fail("not_found", "no such doc");
  return { doc: { title: `Doc ${params.rest}` } };
}
"#;

const DOC_IMAGE: &str = r##"export const size = { width: 600, height: 315 };

export default function Image({ doc }: { doc: { title: string } }) {
  return <div style={{ display: "flex", width: "100%", height: "100%", backgroundColor: "#1e3a8a", color: "white" }}>{doc.title}</div>;
}
"##;

#[test]
fn an_image_beside_a_catch_all_page_is_drawn_at_the_pages_own_url() {
  let root = project("catch-all", &[("routes/docs/[...rest]/page.tsx", DOC_PAGE), ("routes/docs/[...rest]/page.loader.ts", DOC_LOADER), ("routes/docs/[...rest]/opengraph-image.tsx", DOC_IMAGE)]);
  let host = boot(&root);
  let page = page_html(&host, "/docs/guide/intro");
  assert!(page.contains("<main>Doc guide/intro</main>"), "the page still answers its own URL: {page}");
  assert!(page.contains("<meta property=\"og:image\" content=\"https://board.example/docs/guide/intro/opengraph-image.png?v="), "{page}");
  let (status, kind, bytes) = get(&host, "/docs/guide/intro/opengraph-image.png");
  assert_eq!((status, kind.as_str()), (200, "image/png"), "{}", String::from_utf8_lossy(&bytes));
  assert_eq!(png_size(&bytes), (600, 315));
  let (status, _, _) = get(&host, "/docs/missing/opengraph-image.png");
  assert_eq!(status, 404, "the loader's not found answers the image too");
  std::fs::remove_dir_all(&root).unwrap();
}

#[test]
fn a_catch_all_page_listing_its_images_draws_each_at_its_id() {
  let listed = "export function images() {\n  return [{ id: \"small\", size: { width: 300, height: 150 } }];\n}\n\nexport default function Image({ id }: { id: string }) {\n  return <div style={{ display: \"flex\", width: \"100%\", height: \"100%\" }}>{id}</div>;\n}\n";
  let root = project("catch-all-listed", &[("routes/docs/[...rest]/page.tsx", DOC_PAGE), ("routes/docs/[...rest]/page.loader.ts", DOC_LOADER), ("routes/docs/[...rest]/opengraph-image.tsx", listed)]);
  let host = boot(&root);
  let page = page_html(&host, "/docs/guide/intro");
  assert!(page.contains("<meta property=\"og:image\" content=\"https://board.example/docs/guide/intro/opengraph-image/small?v="), "{page}");
  let (status, kind, bytes) = get(&host, "/docs/guide/intro/opengraph-image/small");
  assert_eq!((status, kind.as_str()), (200, "image/png"), "{}", String::from_utf8_lossy(&bytes));
  assert_eq!(png_size(&bytes), (300, 150));
  let (status, _, _) = get(&host, "/docs/guide/intro/opengraph-image/nope");
  assert_eq!(status, 404, "an id the list does not name");
  std::fs::remove_dir_all(&root).unwrap();
}
