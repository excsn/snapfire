//! `<Picture>` and the `<img>` rewrite: what the markup becomes for an
//! imported image, an image served as it is, a remote one through a named
//! source, the escape and the priority mark.

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;

use snapfire_fsr_core::{Value, ValueMap};
use snapfire_fsr_ir::ast::{Entry, Expr, Lit};
use snapfire_fsr_ir::render::{prepare, Components};
use snapfire_fsr_ir::{Interpreter, Tmpl};
use snapfire_fsr_lower::assets::{AssetResolver, ImageFacts, ImageRequest};
use snapfire_fsr_lower::component::ComponentSet;

static NEXT: AtomicU32 = AtomicU32::new(0);

fn app(files: &[(&str, &str)]) -> std::path::PathBuf {
  let n = NEXT.fetch_add(1, Ordering::Relaxed);
  let dir = std::env::temp_dir().join(format!("fsr_pictures_{}_{n}", std::process::id()));
  let _ = std::fs::remove_dir_all(&dir);
  for (name, source) in files {
    let path = dir.join(name);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, source).unwrap();
  }
  dir
}

/// Answers for `src/img/hero.png`, 1600 by 900, and `src/img/logo.svg`,
/// served as it is, and records every request it was asked.
#[derive(Default)]
struct Fake {
  requests: RefCell<Vec<(String, ImageRequest)>>,
}

const HERO: &str = "/static/js/app/src/img/hero.0a1b2c3d.png";

impl AssetResolver for Fake {
  fn image(&self, path: &str, request: &ImageRequest) -> Option<ImageFacts> {
    self.requests.borrow_mut().push((path.to_owned(), request.clone()));
    match path {
      "src/img/hero.png" => {
        let widths = request.widths.clone().unwrap_or_else(|| vec![640, 960, 1280]);
        let srcset = |ext: &str| widths.iter().map(|w| format!("/static/js/app/src/img/hero.0a1b2c3d.{w}.{ext} {w}w")).chain(std::iter::once(format!("/static/js/app/src/img/hero.0a1b2c3d.1600.{ext} 1600w"))).collect::<Vec<_>>().join(", ");
        Some(ImageFacts { src: HERO.to_owned(), width: 1600, height: 900, sources: vec![("image/avif".to_owned(), srcset("avif")), ("image/webp".to_owned(), srcset("webp"))] })
      }
      "src/img/logo.svg" => Some(ImageFacts { src: "/static/js/app/src/img/logo.9f9f9f9f.svg".to_owned(), width: 120, height: 40, sources: Vec::new() }),
      _ => None,
    }
  }
  fn font(&self, path: &str) -> Option<String> {
    (path == "src/fonts/inter.woff2").then(|| "/static/js/app/src/fonts/inter.8b1d0e77.woff2".to_owned())
  }
  fn source(&self, name: &str) -> Option<String> {
    (name == "cms").then(|| "https://img.example.com/{src}?w={width}&auto=format".to_owned())
  }
  fn widths(&self) -> Vec<u32> {
    vec![640, 1280]
  }
}

fn lower_with(files: &[(&str, &str)], module: &str, rewrite: bool) -> (ComponentSet, Rc<Fake>) {
  let fake = Rc::new(Fake::default());
  let mut set = ComponentSet::new(&app(files)).with_assets(fake.clone(), rewrite);
  set.lower(module).unwrap();
  (set, fake)
}

fn render_of<'a>(set: &'a ComponentSet, module: &str) -> &'a Tmpl {
  &set.components.iter().find(|(m, _)| m == module).unwrap_or_else(|| panic!("{module} did not lower")).1.render
}

fn html(set: &ComponentSet, module: &str) -> String {
  let library: Components = set.components.iter().map(|(module, component)| (module.clone(), Arc::new(prepare(component)))).collect();
  let props: ValueMap = ValueMap::default();
  Interpreter::default().render_module(module, &library[module], &props, &library).map(|r| r.html).unwrap()
}

fn attr<'a>(attrs: &'a [Entry], name: &str) -> Option<&'a Expr> {
  attrs.iter().find_map(|e| match e {
    Entry::Field(n, v) if n == name => Some(v),
    _ => None,
  })
}

fn string(expr: Option<&Expr>) -> String {
  match expr {
    Some(Expr::Lit(Lit::Str(s))) => s.clone(),
    Some(Expr::Lit(Lit::Int(n))) => n.to_string(),
    other => panic!("not a literal: {other:?}"),
  }
}

fn picture_of(tmpl: &Tmpl) -> (&[Tmpl], &[Entry]) {
  let Tmpl::Element { tag, children, .. } = tmpl else { panic!("not an element: {tmpl:?}") };
  assert_eq!(tag, "picture");
  let Some(Tmpl::Element { tag, attrs, .. }) = children.last() else { panic!("no img") };
  assert_eq!(tag, "img");
  (&children[..children.len() - 1], attrs)
}

const PAGE_WITH_PICTURE: &str = r#"import { Picture } from "@snapfire/fsr-authoring/template";
import hero from "../img/hero.png";
export default function Page() {
  return <Picture src={hero} alt="The harbour" className="hero" />;
}
"#;

#[test]
fn an_imported_image_becomes_a_picture_with_a_source_per_format_and_the_original_as_its_img() {
  let (set, fake) = lower_with(&[("src/ui/Page.tsx", PAGE_WITH_PICTURE)], "src/ui/Page.tsx#default", true);
  let (sources, img) = picture_of(render_of(&set, "src/ui/Page.tsx#default"));
  assert_eq!(sources.len(), 2);
  let Tmpl::Element { tag, attrs, .. } = &sources[0] else { panic!() };
  assert_eq!(tag, "source");
  assert_eq!(string(attr(attrs, "type")), "image/avif");
  assert!(string(attr(attrs, "srcset")).ends_with("hero.0a1b2c3d.1600.avif 1600w"));
  assert_eq!(string(attr(attrs, "sizes")), "(max-width: 1600px) 100vw, 1600px");
  assert_eq!(string(attr(img, "src")), HERO);
  assert_eq!(string(attr(img, "width")), "1600");
  assert_eq!(string(attr(img, "height")), "900");
  assert_eq!(string(attr(img, "alt")), "The harbour");
  assert_eq!(string(attr(img, "class")), "hero");
  assert_eq!(string(attr(img, "loading")), "lazy");
  assert_eq!(string(attr(img, "decoding")), "async");
  assert!(attr(img, "fetchpriority").is_none());
  assert_eq!(fake.requests.borrow()[0].0, "src/img/hero.png");
  assert!(set.heads.is_empty(), "nothing asked for a preload");

  let out = html(&set, "src/ui/Page.tsx#default");
  assert!(out.starts_with("<picture><source type=\"image/avif\" srcset=\""), "{out}");
  assert!(out.contains("<img src=\"/static/js/app/src/img/hero.0a1b2c3d.png\" width=\"1600\" height=\"900\""), "{out}");
  assert!(out.ends_with("</picture>"), "{out}");
}

#[test]
fn a_plain_img_of_an_imported_asset_is_rewritten_unless_escaped_or_the_rewrite_is_off() {
  let page = r#"import hero from "../img/hero.png";
export default function Page() {
  return <div><img src={hero.src} alt="a" /><img src={hero.src} alt="b" data-sf-raw /></div>;
}
"#;
  let (set, _) = lower_with(&[("src/ui/Page.tsx", page)], "src/ui/Page.tsx#default", true);
  let Tmpl::Element { children, .. } = render_of(&set, "src/ui/Page.tsx#default") else { panic!() };
  let (_, img) = picture_of(&children[0]);
  assert_eq!(string(attr(img, "alt")), "a");
  let Tmpl::Element { tag, attrs, .. } = &children[1] else { panic!() };
  assert_eq!(tag, "img", "the escaped one stays an img");
  assert!(attr(attrs, "data-sf-raw").is_none(), "the escape is stripped: {attrs:?}");
  assert_eq!(string(attr(attrs, "width")), "1600");
  assert_eq!(string(attr(attrs, "height")), "900");
  let out = html(&set, "src/ui/Page.tsx#default");
  assert!(out.contains("<img src=\"/static/js/app/src/img/hero.0a1b2c3d.png\" alt=\"b\" width=\"1600\" height=\"900\"/>"), "{out}");

  let (set, _) = lower_with(&[("src/ui/Page.tsx", page)], "src/ui/Page.tsx#default", false);
  let Tmpl::Element { children, .. } = render_of(&set, "src/ui/Page.tsx#default") else { panic!() };
  let Tmpl::Element { tag, .. } = &children[0] else { panic!() };
  assert_eq!(tag, "img", "with the rewrite off a plain img is a plain img");
  let out = html(&set, "src/ui/Page.tsx#default");
  assert!(out.starts_with("<div><img src=\"/static/js/app/src/img/hero.0a1b2c3d.png\" alt=\"a\"/>"), "{out}");
}

#[test]
fn the_asset_binding_is_an_object_a_component_reads_any_field_of() {
  let page = r#"import hero from "../img/hero.png";
import inter from "../fonts/inter.woff2";
export default function Page() {
  return <p data-w={hero.width} data-font={inter}>{hero.height}</p>;
}
"#;
  let (set, _) = lower_with(&[("src/ui/Page.tsx", page)], "src/ui/Page.tsx#default", true);
  let out = html(&set, "src/ui/Page.tsx#default");
  assert_eq!(out, "<p data-w=\"1600\" data-font=\"/static/js/app/src/fonts/inter.8b1d0e77.woff2\">900</p>");
}

#[test]
fn a_priority_picture_loads_eagerly_and_asks_the_head_for_a_preload_that_the_placing_page_inherits() {
  let hero = r#"import { Picture } from "@snapfire/fsr-authoring/template";
import hero from "../img/hero.png";
export default function Hero() {
  return <Picture src={hero} alt="x" priority sizes="50vw" widths={[320, 640]} quality={{ avif: 70 }} />;
}
"#;
  let page = r#"import Hero from "./Hero";
export default function Page() {
  return <main><Hero /></main>;
}
"#;
  let (set, fake) = lower_with(&[("src/ui/Hero.tsx", hero), ("src/ui/Page.tsx", page)], "src/ui/Page.tsx#default", true);
  let hero_module = "src/ui/Hero.tsx#default";
  let (sources, img) = picture_of(render_of(&set, hero_module));
  assert_eq!(string(attr(img, "loading")), "eager");
  assert_eq!(string(attr(img, "fetchpriority")), "high");
  let Tmpl::Element { attrs, .. } = &sources[0] else { panic!() };
  assert_eq!(string(attr(attrs, "sizes")), "50vw");
  assert!(string(attr(attrs, "srcset")).starts_with("/static/js/app/src/img/hero.0a1b2c3d.320.avif 320w, "), "{attrs:?}");

  let requests = fake.requests.borrow();
  let request = &requests.iter().find(|(p, _)| p == "src/img/hero.png").unwrap().1;
  assert_eq!(request.widths, Some(vec![320, 640]));
  assert_eq!(request.quality.as_ref().unwrap().get("avif"), Some(&70));
  drop(requests);

  let row = &set.heads[hero_module][0];
  assert_eq!(row.tag, "link");
  assert_eq!(row.attrs[0], ("rel".to_owned(), "preload".to_owned()));
  assert_eq!(row.attrs[1], ("as".to_owned(), "image".to_owned()));
  assert_eq!(row.attrs[2], ("type".to_owned(), "image/avif".to_owned()));
  assert!(row.attrs[3].1.ends_with("1600.avif 1600w"));
  assert_eq!(row.attrs[4], ("imagesizes".to_owned(), "50vw".to_owned()));
  assert_eq!(set.heads["src/ui/Page.tsx#default"], set.heads[hero_module], "the page placing the hero carries its preload");
}

#[test]
fn an_image_served_as_it_is_gets_an_img_alone() {
  let page = r#"import { Picture } from "@snapfire/fsr-authoring/template";
import logo from "../img/logo.svg";
export default function Page() {
  return <Picture src={logo} alt="logo" />;
}
"#;
  let (set, _) = lower_with(&[("src/ui/Page.tsx", page)], "src/ui/Page.tsx#default", true);
  let out = html(&set, "src/ui/Page.tsx#default");
  assert_eq!(out, "<img src=\"/static/js/app/src/img/logo.9f9f9f9f.svg\" width=\"120\" height=\"40\" alt=\"logo\" loading=\"lazy\" decoding=\"async\"/>");
}

#[test]
fn a_string_src_through_a_named_source_writes_srcset_from_the_template() {
  let page = r#"import { Picture } from "@snapfire/fsr-authoring/template";
export default function Page() {
  const photo = "shots/dock.jpg";
  return <Picture src={photo} source="cms" alt="dock" width={800} height={600} />;
}
"#;
  let (set, _) = lower_with(&[("src/ui/Page.tsx", page)], "src/ui/Page.tsx#default", true);
  let out = html(&set, "src/ui/Page.tsx#default");
  assert_eq!(
    out,
    "<img src=\"https://img.example.com/shots/dock.jpg?w=1280&amp;auto=format\" srcset=\"https://img.example.com/shots/dock.jpg?w=640&amp;auto=format 640w, https://img.example.com/shots/dock.jpg?w=1280&amp;auto=format 1280w\" sizes=\"100vw\" alt=\"dock\" width=\"800\" height=\"600\" loading=\"lazy\" decoding=\"async\"/>"
  );
}

#[test]
fn a_string_src_with_no_source_is_an_img_as_written_and_a_path_literal_is_refused() {
  let page = r#"import { Picture } from "@snapfire/fsr-authoring/template";
export default function Page(props: { url: string }) {
  return <Picture src={props.url} alt="remote" />;
}
"#;
  let (set, _) = lower_with(&[("src/ui/Page.tsx", page)], "src/ui/Page.tsx#default", true);
  let Tmpl::Element { tag, attrs, .. } = render_of(&set, "src/ui/Page.tsx#default") else { panic!() };
  assert_eq!(tag, "img");
  assert!(attr(attrs, "srcset").is_none());

  let page = r#"import { Picture } from "@snapfire/fsr-authoring/template";
export default function Page() {
  return <Picture src="./hero.png" alt="x" />;
}
"#;
  let fake = Rc::new(Fake::default());
  let mut set = ComponentSet::new(&app(&[("src/ui/Page.tsx", page)])).with_assets(fake, true);
  let err = set.lower("src/ui/Page.tsx#default").unwrap_err().to_string();
  assert!(err.contains("import the file and pass the import"), "{err}");

  let page = r#"import { Picture } from "@snapfire/fsr-authoring/template";
export default function Page() {
  return <Picture src="x" source="nowhere" alt="x" />;
}
"#;
  let fake = Rc::new(Fake::default());
  let mut set = ComponentSet::new(&app(&[("src/ui/Page.tsx", page)])).with_assets(fake, true);
  let err = set.lower("src/ui/Page.tsx#default").unwrap_err().to_string();
  assert!(err.contains("`nowhere` is not an `[images.sources]` entry"), "{err}");
}

#[test]
fn an_asset_import_the_resolver_does_not_know_is_unbound_as_before() {
  let page = r#"import missing from "../img/missing.png";
export default function Page() {
  return <img src={missing.src} alt="x" />;
}
"#;
  let fake = Rc::new(Fake::default());
  let mut set = ComponentSet::new(&app(&[("src/ui/Page.tsx", page)])).with_assets(fake, true);
  let err = set.lower("src/ui/Page.tsx#default").unwrap_err().to_string();
  assert!(err.contains("is not an image under the app") || err.contains("not bound here"), "{err}");
  let _ = Value::Null;
}
