//! `fsr build` over an application with an imported image and a font
//! directory: the manifest it writes, the plan rows it adds, the files the
//! derive pass puts under `dist/` and the declarations an author imports
//! against.

use std::path::{Path, PathBuf};

use snapfire_fsr_cli::new::{create, NewOptions};
use snapfire_fsr_cli::{assets, build, write, Built, Options};

static NEXT: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);

fn root(tag: &str) -> PathBuf {
  let n = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
  let dir = std::env::temp_dir().join(format!("fsr-cli-assets-{}-{n}-{tag}", std::process::id()));
  let _ = std::fs::remove_dir_all(&dir);
  dir
}

fn fixture(name: &str) -> Vec<u8> {
  std::fs::read(Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures").join(name)).unwrap()
}

const PAGE: &str = r#"import { Picture } from "@snapfire/fsr-authoring/template";
import hero from "../src/img/hero.png";
import inter from "../fonts/Inter-Regular.woff2";

export default function Page() {
  return (
    <section>
      <Picture src={hero} alt="The harbour" priority />
      <img src={hero.src} alt="raw" data-sf-raw />
      <span data-font={inter}>{hero.width}</span>
    </section>
  );
}
"#;

/// A React scaffold with the fixtures in place and the asset sections written, built.
fn built(tag: &str, toml: &str) -> (PathBuf, Built) {
  let root = root(tag);
  create(&root, NewOptions { fetch: false, with: vec!["react".to_owned()], ..NewOptions::default() }).unwrap();
  let app = root.join("app");
  std::fs::write(app.join("importmap.json"), r#"{"imports":{"@snapfire/fsr-client/react":"/r","react":"/r","react-dom/client":"/d"}}"#).unwrap();
  std::fs::create_dir_all(app.join("vendor")).unwrap();
  std::fs::write(app.join("vendor/.fsr-vendor.json"), r#"{"packages":{"react":{"version":"18.3.1"},"react-dom":{"version":"18.3.1"}}}"#).unwrap();
  std::fs::create_dir_all(app.join("src/img")).unwrap();
  std::fs::create_dir_all(app.join("fonts")).unwrap();
  std::fs::write(app.join("src/img/hero.png"), fixture("hero.png")).unwrap();
  std::fs::write(app.join("fonts/Inter-Regular.woff2"), fixture("Inter-Regular.woff2")).unwrap();
  std::fs::write(app.join("fonts/Inter-Bold.woff2"), fixture("Inter-Bold.woff2")).unwrap();
  std::fs::write(app.join("routes/page.tsx"), PAGE).unwrap();
  let config = root.join("config/app.toml");
  let mut text = std::fs::read_to_string(&config).unwrap();
  text.push_str(toml);
  std::fs::write(&config, text).unwrap();
  let built = build(&app, &Options::beside(&app)).unwrap();
  (app, built)
}

const SECTIONS: &str = "\n[images]\nwidths = [80]\n\n[fonts.sans]\nfamily = \"Inter\"\nfallback = \"Arial\"\n";

#[test]
fn the_build_sees_the_image_and_the_fonts_and_writes_the_manifest_the_plan_rows_and_the_declarations() {
  let (app, built) = built("manifest", SECTIONS);
  let hash = snapfire_fsr_assets::hash::of(&fixture("hero.png"));

  let entry = built.assets.entries.iter().find(|e| e.source == "src/img/hero.png").expect("the page's image is an entry");
  assert_eq!((entry.width, entry.height), (160, 100));
  assert_eq!(entry.src, format!("/static/js/app/src/img/hero.{hash}.png"));
  assert_eq!(entry.widths, [80, 160], "the policy width below the source, then the source");
  assert!(!entry.passthrough);
  assert_eq!(entry.variants.len(), 4);
  assert!(entry.variants.iter().any(|v| v.path == format!("src/img/hero.{hash}.80.avif") && v.url == format!("/static/js/app/src/img/hero.{hash}.80.avif")), "{:?}", entry.variants);

  assert_eq!(built.assets.images.widths, [80]);
  assert_eq!(built.assets.images.formats, ["avif", "webp"]);
  let fonts = &built.assets.fonts;
  assert_eq!(fonts.faces.len(), 2);
  let regular = fonts.faces.iter().find(|f| f.weight == 400).unwrap();
  assert_eq!(regular.key, "sans");
  assert_eq!(regular.family, "Inter");
  assert!(regular.preload, "the regular weight is preloaded by default");
  assert!(!fonts.faces.iter().find(|f| f.weight == 700).unwrap().preload);
  assert!(regular.url.starts_with("/static/js/app/fonts/Inter-Regular.") && regular.url.ends_with(".woff2"), "{}", regular.url);
  assert!(fonts.css.contains("@font-face{font-family:\"Inter\";font-style:normal;font-weight:400;font-display:swap;src:url(/static/js/app/fonts/Inter-Regular."), "{}", fonts.css);
  assert!(fonts.css.contains("font-family: \"Inter Fallback\"; src: local(\"Arial\"); size-adjust: "), "{}", fonts.css);
  assert!(fonts.css.contains(":root{--font-sans:\"Inter\", \"Inter Fallback\", sans-serif;}"), "{}", fonts.css);
  assert_eq!(fonts.preload, vec![regular.url.clone()]);

  let plan = built.manifest.components.iter().find(|c| c.module == "routes/page.tsx#default").expect("the page lowered");
  assert_eq!(plan.head.len(), 1, "the priority picture asks for one preload");
  assert_eq!(plan.head[0].tag, "link");
  assert!(plan.head[0].attrs.iter().any(|(k, v)| k == "imagesrcset" && v.contains(&format!("hero.{hash}.160.avif 160w"))), "{:?}", plan.head[0]);
  let text = built.manifest.to_sexpr();
  let head_at = text.find("(head link").expect("the row is printed");
  assert!(text[head_at..].trim_start_matches("(head link").trim_start().starts_with("\"rel\""), "{}", &text[head_at..head_at + 80]);

  let report = built.report.to_string();
  assert!(report.contains("image     src/img/hero.png"), "{report}");
  assert!(report.contains("160x100, 4 variants"), "{report}");
  assert!(report.contains("font      Inter 700 normal from fonts/Inter-Bold.woff2"), "{report}");
  assert!(report.contains("          Inter 400 normal from fonts/Inter-Regular.woff2"), "{report}");

  let files: Vec<&str> = built.files.iter().map(|(name, _)| name.as_str()).collect();
  assert!(files.contains(&"generated/assets.json"), "{files:?}");
  assert!(files.contains(&"generated/assets.d.ts"), "{files:?}");
  let map: serde_json::Value = serde_json::from_str(&built.files.iter().find(|(n, _)| n == "generated/assets.map.json").expect("the compiler's map").1).unwrap();
  assert_eq!(map["version"], 1);
  assert_eq!(map["assets"]["src/img/hero.png"]["url"], format!("/static/js/app/src/img/hero.{hash}.png"));
  assert_eq!((map["assets"]["src/img/hero.png"]["width"].as_u64(), map["assets"]["src/img/hero.png"]["height"].as_u64()), (Some(160), Some(100)));
  assert_eq!(map["assets"]["fonts/Inter-Regular.woff2"]["url"], regular.url, "a face the page imports is one row, the directory's");
  assert!(map["assets"]["fonts/Inter-Regular.woff2"].get("width").is_none());
  assert!(map["assets"]["fonts/Inter-Bold.woff2"]["url"].as_str().unwrap().ends_with(".woff2"), "every face is in the map, imported or not");
  assert!(fonts.referenced.is_empty(), "a face under the directory is not a referenced file too: {:?}", fonts.referenced);
  let declarations = &built.files.iter().find(|(n, _)| n == "generated/assets.d.ts").unwrap().1;
  assert!(declarations.contains("declare module \"*.png\" { const asset: import(\"@snapfire/fsr-authoring/template\").ImageAsset; export default asset; }"), "{declarations}");
  assert!(declarations.contains("declare module \"*.woff2\" { const url: string; export default url; }"), "{declarations}");

  write(&app, &built).unwrap();
  let manifest: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(app.join("generated/assets.json")).unwrap()).unwrap();
  assert_eq!(manifest["version"], 1);
  assert_eq!(manifest["entries"][0]["source"], "src/img/hero.png");
  assert!(app.join("types/@snapfire/fsr-authoring/template.d.ts").is_file());
  assert!(std::fs::read_to_string(app.join("types/@snapfire/fsr-authoring/template.d.ts")).unwrap().contains("export function Picture(props: PictureProps): TemplateNode;"));
}

#[test]
fn the_derive_pass_writes_the_variants_and_the_font_copies_once_and_keeps_them_after() {
  let (app, built) = built("derive", SECTIONS);
  let dist = app.join("dist");
  let derived = assets::derive(&app, &dist, &built.assets).unwrap();
  assert_eq!(derived.written.len(), 7, "the original, four variants and two faces; nothing bundled here, so the original is placed too: {:?}", derived.written);
  assert_eq!(derived.kept, 0);
  let hash = snapfire_fsr_assets::hash::of(&fixture("hero.png"));
  assert_eq!(built.assets.entries[0].path, format!("src/img/hero.{hash}.png"));
  assert_eq!(std::fs::read(dist.join(format!("src/img/hero.{hash}.png"))).unwrap(), fixture("hero.png"));
  for (width, format, magic) in [(80u32, "avif", &b"ftypavif"[..]), (160, "avif", &b"ftypavif"[..]), (80, "webp", &b"WEBP"[..]), (160, "webp", &b"WEBP"[..])] {
    let path = dist.join(format!("src/img/hero.{hash}.{width}.{format}"));
    let bytes = std::fs::read(&path).unwrap_or_else(|_| panic!("{} was not written", path.display()));
    let at = if format == "avif" { 4 } else { 8 };
    assert_eq!(&bytes[at..at + magic.len()], magic, "{}", path.display());
  }
  let webp = std::fs::read(dist.join(format!("src/img/hero.{hash}.80.webp"))).unwrap();
  let decoded = snapfire_fsr_assets::Source::from_bytes(Path::new("x.webp"), &webp).unwrap();
  assert_eq!((decoded.width(), decoded.height()), (80, 50));
  let regular = built.assets.fonts.faces.iter().find(|f| f.weight == 400).unwrap();
  assert_eq!(std::fs::read(dist.join(&regular.path)).unwrap(), fixture("Inter-Regular.woff2"));

  let again = assets::derive(&app, &dist, &built.assets).unwrap();
  assert!(again.written.is_empty(), "{:?}", again.written);
  assert_eq!(again.kept, 7);

  std::fs::remove_file(dist.join(format!("src/img/hero.{hash}.png"))).unwrap();
  let replaced = assets::derive(&app, &dist, &built.assets).unwrap();
  assert_eq!(replaced.written, vec![dist.join(format!("src/img/hero.{hash}.png"))], "an original the bundle did not place is copied from the source");
  assert_eq!(std::fs::read(dist.join(format!("src/img/hero.{hash}.png"))).unwrap(), fixture("hero.png"));
}

const TAGGED_PAGE: &str = r#"import { Picture } from "@snapfire/fsr-authoring/template";
import photo from "../src/img/photo.jpg";

export default function Page() {
  return <Picture src={photo} alt="A phone photo" />;
}
"#;

#[test]
fn a_tagged_photo_is_upright_in_the_manifest_the_markup_and_its_variants() {
  let root = root("tagged");
  create(&root, NewOptions { fetch: false, with: vec!["react".to_owned()], ..NewOptions::default() }).unwrap();
  let app = root.join("app");
  std::fs::write(app.join("importmap.json"), r#"{"imports":{"@snapfire/fsr-client/react":"/r","react":"/r","react-dom/client":"/d"}}"#).unwrap();
  std::fs::create_dir_all(app.join("vendor")).unwrap();
  std::fs::write(app.join("vendor/.fsr-vendor.json"), r#"{"packages":{"react":{"version":"18.3.1"},"react-dom":{"version":"18.3.1"}}}"#).unwrap();
  std::fs::create_dir_all(app.join("src/img")).unwrap();
  std::fs::write(app.join("src/img/photo.jpg"), fixture("photo.jpg")).unwrap();
  std::fs::write(app.join("routes/page.tsx"), TAGGED_PAGE).unwrap();
  let config = root.join("config/app.toml");
  let mut text = std::fs::read_to_string(&config).unwrap();
  text.push_str("\n[images]\nwidths = [80]\n");
  std::fs::write(&config, text).unwrap();
  let built = build(&app, &Options::beside(&app)).unwrap();

  let entry = built.assets.entries.iter().find(|e| e.source == "src/img/photo.jpg").expect("the photo is an entry");
  assert_eq!((entry.width, entry.height), (160, 320), "stored 320x160 with EXIF orientation 6");
  assert_eq!(entry.widths, [80, 160]);
  let text = built.manifest.to_sexpr();
  assert!(text.contains("(width 160)") && text.contains("(height 320)"), "the element carries the displayed size: {text}");

  let dist = app.join("dist");
  let derived = assets::derive(&app, &dist, &built.assets).unwrap();
  let hash = snapfire_fsr_assets::hash::of(&fixture("photo.jpg"));
  assert_eq!(derived.written.len(), 5, "{:?}", derived.written);
  assert_eq!(std::fs::read(dist.join(format!("src/img/photo.{hash}.jpg"))).unwrap(), fixture("photo.jpg"), "the original is served as saved, tag and all");
  let webp = std::fs::read(dist.join(format!("src/img/photo.{hash}.80.webp"))).unwrap();
  let decoded = snapfire_fsr_assets::Source::from_bytes(Path::new("x.webp"), &webp).unwrap();
  assert_eq!((decoded.width(), decoded.height()), (80, 160), "the variant is upright");
  assert_eq!(snapfire_fsr_assets::Header::from_bytes(Path::new("x.webp"), &webp).unwrap().orientation, 1, "and carries no tag");
}

#[test]
fn reconciling_what_the_bundle_reports_adds_entries_fonts_and_map_rows_and_refuses_what_it_cannot_serve() {
  let (app, mut built) = built("reconcile", SECTIONS);
  std::fs::write(app.join("src/img/photo.jpg"), fixture("photo.jpg")).unwrap();
  std::fs::create_dir_all(app.join("src/type")).unwrap();
  std::fs::write(app.join("src/type/Inter-Bold.woff2"), fixture("Inter-Bold.woff2")).unwrap();
  assert!(built.assets.entries.iter().all(|e| e.source != "src/img/photo.jpg"), "nothing placed it yet");

  let refreshed = built.adopt(&["src/img/photo.jpg".to_owned(), "src/type/Inter-Bold.woff2".to_owned()]).unwrap();
  assert_eq!(refreshed.len(), 2);
  let photo = built.assets.entries.iter().find(|e| e.source == "src/img/photo.jpg").expect("adopted into the manifest");
  assert_eq!((photo.width, photo.height), (160, 320));
  assert_eq!(photo.widths, [80, 160]);
  assert_eq!(built.assets.fonts.referenced.len(), 1);
  assert_eq!(built.assets.fonts.referenced[0].source, "src/type/Inter-Bold.woff2");
  assert!(built.assets.fonts.referenced[0].path.starts_with("src/type/Inter-Bold.") && built.assets.fonts.referenced[0].path.ends_with(".woff2"));
  let map: serde_json::Value = serde_json::from_str(&refreshed[1].1).unwrap();
  assert!(map["assets"]["src/img/photo.jpg"]["url"].as_str().unwrap().contains("/src/img/photo."), "{map}");
  assert_eq!(map["assets"]["src/type/Inter-Bold.woff2"]["url"], built.assets.fonts.referenced[0].url);
  let manifest_file = &built.files.iter().find(|(n, _)| n == "generated/assets.json").unwrap().1;
  assert!(manifest_file.contains("src/type/Inter-Bold.woff2"), "the file to write carries the adopted font");

  let dist = app.join("dist");
  let derived = assets::derive(&app, &dist, &built.assets).unwrap();
  assert!(derived.written.iter().any(|p| p.ends_with(&built.assets.fonts.referenced[0].path)), "the referenced font is placed: {:?}", derived.written);
  assert!(derived.written.iter().any(|p| p.to_string_lossy().contains("src/img/photo.") && p.to_string_lossy().ends_with(".80.avif")), "{:?}", derived.written);

  let err = built.adopt(&["src/img/nothing.png".to_owned()]).unwrap_err().to_string();
  assert!(err.contains("the bundle names `src/img/nothing.png`, which is not an image or a font this build can serve"), "{err}");
  let err = built.adopt(&["src/notes.txt".to_owned()]).unwrap_err().to_string();
  assert!(err.contains("`src/notes.txt`"), "{err}");
}

#[test]
fn an_unknown_fallback_and_a_key_naming_no_file_are_refused_by_name() {
  let root = root("refused");
  create(&root, NewOptions { fetch: false, with: vec!["react".to_owned()], ..NewOptions::default() }).unwrap();
  let app = root.join("app");
  std::fs::create_dir_all(app.join("fonts")).unwrap();
  std::fs::write(app.join("fonts/Inter-Regular.woff2"), fixture("Inter-Regular.woff2")).unwrap();
  let sections = |toml: &str| {
    let config = root.join("config/app.toml");
    let base: String = std::fs::read_to_string(&config).unwrap().lines().take_while(|l| !l.starts_with("[fonts")).collect::<Vec<_>>().join("\n");
    std::fs::write(&config, format!("{base}\n{toml}\n")).unwrap();
    assets::Sections::of(&app)
  };
  let err = assets::fonts(&app, "/static/js/app", &sections("[fonts.sans]\nfamily = \"Inter\"\nfallback = \"Comic Sans MS\"\n")).unwrap_err().to_string();
  assert!(err.contains("fonts.sans.fallback `Comic Sans MS` is not a face this build knows"), "{err}");
  let err = assets::fonts(&app, "/static/js/app", &sections("[fonts.mono]\nfamily = \"JetBrains Mono\"\n")).unwrap_err().to_string();
  assert!(err.contains("fonts.mono: no file under fonts/ is the family `JetBrains Mono`"), "{err}");
  let err = assets::fonts(&app, "/static/js/app", &sections("[fonts.sans]\nfiles = [\"Missing.woff2\"]\n")).unwrap_err().to_string();
  assert!(err.contains("fonts.sans.files names `Missing.woff2`"), "{err}");

  let (fonts, lines) = assets::fonts(&app, "/static/js/app", &sections("[fonts.display]\nfamily = \"Fraunces\"\nremote = \"https://fonts.googleapis.com/css2?family=Fraunces\"\n")).unwrap();
  assert_eq!(fonts.faces.len(), 1, "the undeclared Inter is inferred under its own slug");
  assert_eq!(fonts.faces[0].key, "inter");
  assert_eq!(fonts.remote[0].preconnect, ["https://fonts.googleapis.com", "https://fonts.gstatic.com"]);
  assert_eq!(fonts.variables["--font-display"], "\"Fraunces\", sans-serif");
  assert_eq!(fonts.variables["--font-inter"], "\"Inter\", \"Inter Fallback\", sans-serif");
  assert!(lines.iter().any(|l| l == "Fraunces from https://fonts.googleapis.com/css2?family=Fraunces"), "{lines:?}");
}

#[test]
fn a_dirs_entry_moves_the_font_directory_and_a_base_prefixes_every_url() {
  let (_, built) = built("dirs", "\n[dirs]\nfonts = \"assets/fonts\"\n\n[images]\nwidths = [80]\nbase = \"https://cdn.example.com\"\n\n[fonts]\nbase = \"https://fonts.example.com\"\n");
  assert!(built.assets.fonts.faces.is_empty(), "nothing under assets/fonts/, so no face: {:?}", built.assets.fonts.faces);
  let entry = &built.assets.entries[0];
  assert!(entry.src.starts_with("https://cdn.example.com/static/js/app/src/img/hero."), "{}", entry.src);
  assert!(entry.variants[0].url.starts_with("https://cdn.example.com/static/js/app/"), "{}", entry.variants[0].url);
  assert_eq!(built.assets.images.base.as_deref(), Some("https://cdn.example.com"));
}

/// A provider standing in for Google Fonts on a local port: one stylesheet
/// with two subsets and the files it names.
fn provider(woff2: Vec<u8>) -> String {
  use std::io::{Read, Write};
  let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
  let base = format!("http://{}", listener.local_addr().unwrap());
  let css_base = base.clone();
  std::thread::spawn(move || {
    for stream in listener.incoming().take(3) {
      let Ok(mut stream) = stream else { continue };
      let mut buf = [0u8; 4096];
      let n = stream.read(&mut buf).unwrap_or(0);
      let request = String::from_utf8_lossy(&buf[..n]);
      let path = request.lines().next().unwrap_or_default().split_whitespace().nth(1).unwrap_or("/").to_owned();
      let (kind, body): (&str, Vec<u8>) = if path.starts_with("/css2") {
        let css = format!(
          "/* latin-ext */\n@font-face {{\n  font-family: 'Inter';\n  font-style: normal;\n  font-weight: 400;\n  font-display: swap;\n  src: url({css_base}/s/inter/ext.woff2) format('woff2');\n  unicode-range: U+0100-02BA;\n}}\n/* latin */\n@font-face {{\n  font-family: 'Inter';\n  font-style: normal;\n  font-weight: 400;\n  font-display: swap;\n  src: url({css_base}/s/inter/latin.woff2) format('woff2');\n  unicode-range: U+0000-00FF;\n}}\n"
        );
        ("text/css", css.into_bytes())
      } else {
        ("font/woff2", woff2.clone())
      };
      let _ = stream.write_all(format!("HTTP/1.1 200 OK\r\ncontent-type: {kind}\r\ncontent-length: {}\r\nconnection: close\r\n\r\n", body.len()).as_bytes());
      let _ = stream.write_all(&body);
    }
  });
  base
}

#[test]
fn fonts_add_fetches_each_subset_into_the_directory_with_its_range_and_the_build_reads_them() {
  let root = root("fonts-add");
  create(&root, NewOptions { fetch: false, with: vec!["react".to_owned()], ..NewOptions::default() }).unwrap();
  let app = root.join("app");
  let base = provider(fixture("Inter-Regular.woff2"));
  let written = assets::add_from(&app, "google:Inter@400", &base).unwrap();
  let names: Vec<String> = written.iter().map(|p| p.file_name().unwrap().to_string_lossy().into_owned()).collect();
  assert_eq!(names, ["Inter-400-latin-ext.woff2", "Inter-400-latin.woff2"]);
  assert_eq!(std::fs::read_to_string(app.join("fonts/Inter-400-latin.woff2.range")).unwrap().trim(), "U+0000-00FF");
  assert_eq!(std::fs::read(app.join("fonts/Inter-400-latin.woff2")).unwrap(), fixture("Inter-Regular.woff2"));

  let (fonts, _) = assets::fonts(&app, "/static/js/app", &assets::Sections::of(&app)).unwrap();
  assert_eq!(fonts.faces.len(), 2);
  assert!(fonts.css.contains("unicode-range:U+0000-00FF;"), "{}", fonts.css);
  assert!(fonts.css.contains("unicode-range:U+0100-02BA;"), "{}", fonts.css);
  assert_eq!(fonts.variables["--font-inter"], "\"Inter\", \"Inter Fallback\", sans-serif");
  assert_eq!(fonts.preload.len(), 1, "one subset is the regular face the default preloads: {:?}", fonts.preload);

  let second = std::env::temp_dir().join(format!("fsr-cli-assets-{}-variable", std::process::id()));
  let _ = std::fs::remove_dir_all(&second);
  create(&second, NewOptions { fetch: false, with: vec!["react".to_owned()], ..NewOptions::default() }).unwrap();
  let app = second.join("app");
  let base = provider(fixture("Fraunces-vietnamese.woff2"));
  let written = assets::add_from(&app, "google:Fraunces@400,700", &base).unwrap();
  let names: Vec<String> = written.iter().map(|p| p.file_name().unwrap().to_string_lossy().into_owned()).collect();
  assert_eq!(names, ["Fraunces-latin-ext.woff2", "Fraunces-latin.woff2"], "a variable file is one per subset, with no weight in its name");
  let (fonts, lines) = assets::fonts(&app, "/static/js/app", &assets::Sections::of(&app)).unwrap();
  assert_eq!(fonts.faces.len(), 2);
  assert_eq!(fonts.faces[0].weight_range, Some((100, 900)));
  assert_eq!(fonts.faces[0].weight, 100);
  assert!(fonts.css.contains("font-weight:100 900;"), "the axis is the declared range: {}", fonts.css);
  assert!(!fonts.css.contains("Fraunces Fallback"), "a subset with no a to z sizes no fallback: {}", fonts.css);
  assert_eq!(fonts.variables["--font-fraunces"], "\"Fraunces\", sans-serif");
  assert!(lines.iter().any(|l| l.starts_with("Fraunces: no fallback face sized")), "{lines:?}");
  assert!(lines.iter().any(|l| l == "Fraunces 100-900 normal from fonts/Fraunces-latin.woff2"), "{lines:?}");
}
