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
  assert_eq!(derived.written.len(), 6, "four variants and two faces: {:?}", derived.written);
  assert_eq!(derived.kept, 0);
  let hash = snapfire_fsr_assets::hash::of(&fixture("hero.png"));
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
  assert_eq!(again.kept, 6);
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
