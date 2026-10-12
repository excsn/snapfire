//! A route directory's `opengraph-image` and `twitter-image`: served as
//! their hashed originals and written into the head of every route under the
//! directory's layout or page, a nearer one replacing them.

use std::path::{Path, PathBuf};

use snapfire_fsr_cli::new::{create, NewOptions};
use snapfire_fsr_cli::{build, write, Options};
use snapfire_fsr_host::config::Config;
use snapfire_fsr_host::{Host, RenderMode};
use snapfire_fsr_runtime::SessionCell;

static NEXT: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);

/// 12 by 6.
const WIDE: &[u8] = &[137, 80, 78, 71, 13, 10, 26, 10, 0, 0, 0, 13, 73, 72, 68, 82, 0, 0, 0, 12, 0, 0, 0, 6, 8, 2, 0, 0, 0, 120, 140, 232, 214, 0, 0, 0, 17, 73, 68, 65, 84, 120, 156, 99, 104, 112, 80, 32, 136, 24, 134, 170, 34, 0, 110, 115, 63, 1, 126, 138, 9, 9, 0, 0, 0, 0, 73, 69, 78, 68, 174, 66, 96, 130];
/// 8 by 4.
const SMALL: &[u8] = &[137, 80, 78, 71, 13, 10, 26, 10, 0, 0, 0, 13, 73, 72, 68, 82, 0, 0, 0, 8, 0, 0, 0, 4, 8, 2, 0, 0, 0, 60, 175, 233, 167, 0, 0, 0, 17, 73, 68, 65, 84, 120, 156, 99, 104, 112, 80, 192, 138, 24, 168, 39, 1, 0, 132, 175, 28, 1, 90, 183, 236, 52, 0, 0, 0, 0, 73, 69, 78, 68, 174, 66, 96, 130];

const PAGE: &str = "export default function Page() {\n  return <main>page</main>;\n}\n";

fn project(tag: &str, files: &[(&str, &[u8])]) -> PathBuf {
  let n = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
  let root = std::env::temp_dir().join(format!("fsr-cli-social-{}-{n}-{tag}", std::process::id()));
  let _ = std::fs::remove_dir_all(&root);
  create(&root, NewOptions { fetch: false, ..NewOptions::default() }).unwrap();
  let app = root.join("app");
  for (path, bytes) in files {
    let file = app.join(path);
    std::fs::create_dir_all(file.parent().unwrap()).unwrap();
    std::fs::write(file, bytes).unwrap();
  }
  let config = root.join("config/app.toml");
  let text = std::fs::read_to_string(&config).unwrap().replacen("[document]\n", "[document]\norigin = \"https://shop.example\"\n", 1);
  std::fs::write(&config, text).unwrap();
  root
}

fn html(host: &Host, path: &str) -> String {
  tokio::runtime::Runtime::new().unwrap().block_on(host.render_to_string(path, RenderMode::Html, SessionCell::default())).unwrap()
}

#[test]
fn a_card_image_covers_the_routes_under_it_and_a_nearer_one_replaces_it() {
  let root = project("cards", &[
    ("routes/opengraph-image.png", WIDE),
    ("routes/about/page.tsx", PAGE.as_bytes()),
    ("routes/about/opengraph-image.png", SMALL),
    ("routes/about/opengraph-image.alt.txt", b"The team on the roof\n"),
    ("routes/about/twitter-image.png", SMALL),
    ("routes/about/team/page.tsx", PAGE.as_bytes()),
  ]);
  let app = root.join("app");
  let built = build(&app, &Options::beside(&app)).unwrap_or_else(|e| panic!("builds: {e}"));
  let entries: Vec<_> = built.assets.entries.iter().filter(|e| e.source.ends_with("-image.png")).collect();
  assert_eq!(entries.len(), 3, "{entries:?}");
  assert!(entries.iter().all(|e| e.variants.is_empty()), "a crawler fetches one URL, so no variant is written: {entries:?}");
  let about = entries.iter().find(|e| e.source == "routes/about/opengraph-image.png").unwrap();
  write(&app, &built).unwrap();
  let host = Host::from_config(Config::load(&root).unwrap()).unwrap().build().unwrap_or_else(|e| panic!("boots: {e}"));

  let home = html(&host, "/");
  let wide = entries.iter().find(|e| e.source == "routes/opengraph-image.png").unwrap();
  assert!(home.contains(&format!("<meta property=\"og:image\" content=\"https://shop.example{}\">", wide.src)), "the root layout's image, absolute against the origin: {home}");
  assert!(home.contains("<meta property=\"og:image:width\" content=\"12\">") && home.contains("<meta property=\"og:image:height\" content=\"6\">") && home.contains("<meta property=\"og:image:type\" content=\"image/png\">"), "{home}");
  assert!(!home.contains("twitter:image"), "{home}");

  for path in ["/about", "/about/team"] {
    let page = html(&host, path);
    assert!(page.contains(&format!("<meta property=\"og:image\" content=\"https://shop.example{}\">", about.src)), "{path} takes the nearer image: {page}");
    assert!(!page.contains(&wide.src), "{path}: the root's is replaced: {page}");
    assert!(page.contains("<meta property=\"og:image:width\" content=\"8\">") && page.contains("<meta property=\"og:image:alt\" content=\"The team on the roof\">"), "{page}");
    assert!(page.contains("<meta name=\"twitter:image\" content=\"https://shop.example") && page.contains("<meta name=\"twitter:image:height\" content=\"4\">"), "{page}");
  }
  std::fs::remove_dir_all(&root).unwrap();
}

#[test]
fn a_card_image_the_build_cannot_place_is_refused() {
  for (tag, files, says) in [
    ("two", vec![("routes/opengraph-image.png", WIDE), ("routes/opengraph-image.webp", WIDE)], "keep one"),
    ("kind", vec![("routes/opengraph-image.bmp", WIDE)], "png, jpg, gif or webp"),
    ("orphan", vec![("routes/deep/opengraph-image.png", WIDE)], "there is none"),
  ] {
    let root = project(tag, &files);
    let app = root.join("app");
    let error = build(&app, &Options::beside(&app)).err().unwrap_or_else(|| panic!("{tag}: refused")).to_string();
    assert!(error.contains(says), "{tag}: {error}");
    std::fs::remove_dir_all(&root).unwrap();
  }
}
