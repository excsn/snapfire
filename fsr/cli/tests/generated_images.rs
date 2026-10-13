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

const LOADER: &str = r#"import { fail } from "@snapfire/fsr";
import type { Ctx } from "@snapfire/fsr";

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
  create(&root, NewOptions { fetch: false, ..NewOptions::default() }).unwrap();
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
