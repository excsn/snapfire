//! `fromNextMetadata`: a Next.js `Metadata` object returned from `meta`, written into the document head the host serves.

use std::path::{Path, PathBuf};

use bytes::Bytes;
use http::Request;
use http_body_util::BodyExt;
use snapfire_fsr_cli::new::{create, NewOptions};
use snapfire_fsr_cli::{build, write, Options};
use snapfire_fsr_host::config::Config;
use snapfire_fsr_host::Host;

fn project(files: &[(&str, &str)]) -> PathBuf {
  let root = std::env::temp_dir().join(format!("fsr-cli-next-metadata-{}", std::process::id()));
  let _ = std::fs::remove_dir_all(&root);
  create(&root, NewOptions::default().fetch(false)).unwrap();
  for (path, text) in files {
    let file = root.join("app").join(path);
    std::fs::create_dir_all(file.parent().unwrap()).unwrap();
    std::fs::write(file, text).unwrap();
  }
  root
}

fn document(root: &Path, path: &str) -> String {
  let app = root.join("app");
  let built = build(&app, &Options::beside(&app)).unwrap_or_else(|e| panic!("builds: {e}"));
  write(&app, &built).unwrap();
  let host = Host::from_config(Config::load(root).unwrap()).unwrap().build().unwrap_or_else(|e| panic!("boots: {e}"));
  let runtime = tokio::runtime::Runtime::new().unwrap();
  runtime.block_on(async {
    let response = host.handle(Request::get(path).body(Bytes::new()).unwrap()).await;
    let body = response.into_body().collect().await.unwrap().to_bytes();
    String::from_utf8_lossy(&body).into_owned()
  })
}

const LOADER: &str = r#"import { fromNextMetadata } from "@snapfire/fsr/head";
export async function load() {
  return { page: { title: "Gift Cards", description: "Buy gift cards", image: "https://cdn.example/a.webp" } };
}
export const meta = ({ data }) =>
  fromNextMetadata({
    title: { absolute: data.page.title },
    description: data.page.description,
    alternates: { canonical: "/giftcards", languages: { de: "/de/giftcards" } },
    openGraph: { images: [{ url: data.page.image, width: 1552, height: 810, alt: "cards" }] },
    twitter: { card: "summary_large_image", images: [data.page.image] },
    robots: { index: true, follow: false },
    authors: { name: "Corp" },
  });
"#;

#[test]
fn a_next_metadata_object_is_written_as_the_head_it_describes() {
  let root = project(&[("routes/page.tsx", "export default function Page() {\n  return <main>page</main>;\n}\n"), ("routes/page.loader.ts", LOADER)]);
  let html = document(&root, "/");
  let head = html.split("</head>").next().unwrap_or_default();
  assert!(head.contains("<title>Gift Cards</title>"), "an absolute title is the whole title: {head}");
  assert!(head.contains("<meta name=\"description\" content=\"Buy gift cards\">"), "{head}");
  assert!(head.contains("rel=\"canonical\""), "{head}");
  assert!(head.contains("hreflang=\"de\""), "{head}");
  for row in ["og:image\" content=\"https://cdn.example/a.webp\"", "og:image:width\" content=\"1552\"", "og:image:alt\" content=\"cards\"", "twitter:card\" content=\"summary_large_image\"", "twitter:image\" content=\"https://cdn.example/a.webp\"", "content=\"index, nofollow\"", "name=\"author\" content=\"Corp\""] {
    assert!(head.contains(row), "{row}: {head}");
  }
  std::fs::remove_dir_all(&root).unwrap();
}
