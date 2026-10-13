use snapfire_fsr_ogimage::{render, Card, CardError, Element, Fonts, Format, ImageRef, Images, Run};
use tiny_skia::Pixmap;

fn rendered(name: &str, card: &Card, images: &Images) -> (Pixmap, Vec<String>) {
  let out = render(card, &Fonts::new(), images, Format::Png).expect("renders");
  if let Ok(dir) = std::env::var("OGIMAGE_OUT") {
    std::fs::write(format!("{dir}/{name}.png"), &out.bytes).unwrap();
  }
  (Pixmap::decode_png(&out.bytes).unwrap(), out.warnings.iter().map(|w| w.to_string()).collect())
}

fn px(p: &Pixmap, x: u32, y: u32) -> [u8; 4] {
  let c = p.pixel(x, y).unwrap().demultiply();
  [c.red(), c.green(), c.blue(), c.alpha()]
}

/// The rows holding a pixel darker than `limit` in every channel.
fn ink_rows(p: &Pixmap, limit: u8) -> Vec<u32> {
  (0..p.height()).filter(|&y| (0..p.width()).any(|x| px(p, x, y)[..3].iter().all(|c| *c < limit) && px(p, x, y)[3] > 200)).collect()
}

fn solid_png(w: u32, h: u32, rgba: [u8; 4]) -> Vec<u8> {
  let mut p = Pixmap::new(w, h).unwrap();
  p.fill(tiny_skia::Color::from_rgba8(rgba[0], rgba[1], rgba[2], rgba[3]));
  p.encode_png().unwrap()
}

fn page(child: Element) -> Element {
  Element::boxed("div").with("width", "100%").with("height", "100%").with("background-color", "white").with("padding", "40px").child(child)
}

#[test]
fn a_card_is_its_size_with_its_background() {
  let card = Card::new(300, 200, Element::boxed("div").with("background-color", "#1e3a8a"));
  let (p, warnings) = rendered("background", &card, &Images::new());
  assert_eq!((p.width(), p.height()), (300, 200));
  assert_eq!(px(&p, 0, 0), [30, 58, 138, 255]);
  assert_eq!(px(&p, 299, 199), [30, 58, 138, 255]);
  assert!(warnings.is_empty());
}

#[test]
fn text_draws_with_the_bundled_face_and_wraps_at_the_box_width() {
  let short = Card::new(600, 400, page(Element::text("p", "Roof work").with("font-size", "40px").with("color", "black")));
  let long = Card::new(600, 400, page(Element::text("p", "Roof work on the east stair closes it from Monday to Wednesday").with("font-size", "40px").with("color", "black")));
  let (a, wa) = rendered("text-short", &short, &Images::new());
  let (b, _) = rendered("text-long", &long, &Images::new());
  assert!(wa.is_empty(), "{wa:?}");
  let rows_a = ink_rows(&a, 80);
  let rows_b = ink_rows(&b, 80);
  assert!(!rows_a.is_empty());
  let span = |r: &[u32]| r.last().unwrap() - r.first().unwrap();
  assert!(span(&rows_b) > 2 * span(&rows_a), "{} lines vs {}", span(&rows_b), span(&rows_a));
}

#[test]
fn a_line_clamp_stops_the_text_at_that_many_lines() {
  let text = "Roof work on the east stair closes it from Monday to Wednesday while the flashing is replaced, so use the west stair";
  let open = Card::new(600, 600, page(Element::text("p", text).with("font-size", "40px")));
  let clamped = Card::new(600, 600, page(Element::text("p", text).with("font-size", "40px").with("line-clamp", "2")));
  let (a, _) = rendered("clamp-open", &open, &Images::new());
  let (b, _) = rendered("clamp-two", &clamped, &Images::new());
  let last = |p: &Pixmap| *ink_rows(p, 80).last().unwrap();
  assert!(last(&b) < 40 + 2 * 50 + 10, "clamped ink ends at {}", last(&b));
  assert!(last(&a) > last(&b) + 80);
}

#[test]
fn nowrap_with_text_overflow_ellipsis_keeps_one_line_inside_the_box() {
  let card = Card::new(400, 200, page(Element::text("p", "A title far too long to fit on one line of this card").with("font-size", "32px").with("white-space", "nowrap").with("text-overflow", "ellipsis").with("overflow", "hidden").with("width", "320px")));
  let (p, _) = rendered("ellipsis", &card, &Images::new());
  let rows = ink_rows(&p, 80);
  assert!(rows.last().unwrap() - rows.first().unwrap() < 40);
  let right = (0..p.width()).rev().find(|&x| (0..p.height()).any(|y| px(&p, x, y)[0] < 80)).unwrap();
  assert!(right <= 40 + 320, "ink reaches x = {right}");
}

#[test]
fn text_no_face_covers_warns_with_the_characters() {
  let card = Card::new(400, 200, page(Element::text("p", "Tokyo 東京")));
  let (_, warnings) = rendered("missing-glyphs", &card, &Images::new());
  assert_eq!(warnings.len(), 1, "{warnings:?}");
  assert!(warnings[0].contains("東京") && warnings[0].contains("U+6771"), "{}", warnings[0]);
  assert!(warnings[0].starts_with("div(1) > p(1)"), "{}", warnings[0]);
}

#[test]
fn an_unsupported_property_warns_and_a_bad_value_fails() {
  let warned = Card::new(100, 100, Element::boxed("div").with("filter", "blur(2px)").with("display", "grid"));
  let (_, warnings) = rendered("warned", &warned, &Images::new());
  assert_eq!(warnings.len(), 2, "{warnings:?}");
  let failed = Card::new(100, 100, Element::boxed("div").with("color", "notacolor"));
  match render(&failed, &Fonts::new(), &Images::new(), Format::Png) {
    Err(CardError::Value { property, value, .. }) => assert_eq!((property.as_str(), value.as_str()), ("color", "notacolor")),
    other => panic!("{other:?}"),
  }
}

#[test]
fn an_image_key_with_no_image_fails_and_never_fetches() {
  let card = Card::new(100, 100, Element::boxed("div").child(Element::image("img", ImageRef::Key("https://example.com/x.png".into()))));
  assert!(matches!(render(&card, &Fonts::new(), &Images::new(), Format::Png), Err(CardError::MissingImage { .. })));
  let bg = Card::new(100, 100, Element::boxed("div").with("background-image", "url(logo)"));
  assert!(matches!(render(&bg, &Fonts::new(), &Images::new(), Format::Png), Err(CardError::MissingImage { .. })));
}

#[test]
fn an_image_covers_its_box_and_takes_the_corner_radius() {
  let mut images = Images::new();
  images.insert("red", &solid_png(40, 20, [220, 0, 0, 255])).unwrap();
  let card = Card::new(200, 200, Element::boxed("div").with("background-color", "white").with("padding", "20px").child(Element::image("img", ImageRef::Key("red".into())).with("width", "160px").with("height", "160px").with("object-fit", "cover").with("border-radius", "80px")));
  let (p, warnings) = rendered("image-cover", &card, &images);
  assert!(warnings.is_empty(), "{warnings:?}");
  assert_eq!(px(&p, 100, 100), [220, 0, 0, 255]);
  assert_eq!(px(&p, 24, 24), [255, 255, 255, 255]);
}

#[test]
fn image_bytes_inline_decode_and_bad_bytes_fail() {
  let card = Card::new(50, 50, Element::boxed("div").child(Element::image("img", ImageRef::Bytes(solid_png(50, 50, [0, 0, 255, 255])))));
  let (p, _) = rendered("image-bytes", &card, &Images::new());
  assert_eq!(px(&p, 25, 25), [0, 0, 255, 255]);
  assert!(matches!(Images::new().insert("svg", b"<svg xmlns='http://www.w3.org/2000/svg'/>"), Err(CardError::Image { .. })));
}

#[test]
fn a_rounded_box_leaves_its_corners_clear_and_its_shadow_outside() {
  let card = Card::new(200, 200, Element::boxed("div").with("padding", "50px").child(Element::boxed("div").with("width", "100px").with("height", "100px").with("background-color", "#f97316").with("border-radius", "24px").with("box-shadow", "0 10px 20px rgba(0,0,0,0.5)")));
  let (p, _) = rendered("rounded-shadow", &card, &Images::new());
  assert_eq!(px(&p, 100, 100), [249, 115, 22, 255]);
  assert!(px(&p, 51, 51)[3] < 255, "corner {:?}", px(&p, 51, 51));
  assert!(px(&p, 100, 160)[3] > 20, "shadow below {:?}", px(&p, 100, 160));
  assert_eq!(px(&p, 5, 5)[3], 0);
}

#[test]
fn borders_draw_each_sides_color() {
  let card = Card::new(100, 100, Element::boxed("div").with("width", "100px").with("height", "100px").with("border-width", "10px").with("border-style", "solid").with("border-color", "red green blue black").with("background-color", "white"));
  let (p, _) = rendered("borders", &card, &Images::new());
  assert_eq!(px(&p, 50, 3), [255, 0, 0, 255]);
  assert_eq!(px(&p, 96, 50), [0, 128, 0, 255]);
  assert_eq!(px(&p, 50, 96), [0, 0, 255, 255]);
  assert_eq!(px(&p, 3, 50), [0, 0, 0, 255]);
  assert_eq!(px(&p, 50, 50), [255, 255, 255, 255]);
}

#[test]
fn a_linear_gradient_runs_from_its_first_stop_to_its_last() {
  let card = Card::new(200, 50, Element::boxed("div").with("background-image", "linear-gradient(to right, rgb(0,0,0), rgb(255,255,255))"));
  let (p, _) = rendered("gradient", &card, &Images::new());
  assert!(px(&p, 2, 25)[0] < 10);
  assert!(px(&p, 197, 25)[0] > 245);
  let mid = px(&p, 100, 25)[0];
  assert!((110..=145).contains(&mid), "middle {mid}");
}

#[test]
fn absolute_children_sit_at_their_offsets() {
  let card = Card::new(200, 100, Element::boxed("div").with("position", "relative").with("width", "100%").with("height", "100%").child(Element::boxed("div").with("position", "absolute").with("right", "10px").with("bottom", "10px").with("width", "20px").with("height", "20px").with("background-color", "red")));
  let (p, _) = rendered("absolute", &card, &Images::new());
  assert_eq!(px(&p, 180, 80), [255, 0, 0, 255]);
  assert_eq!(px(&p, 165, 80)[3], 0);
}

#[test]
fn opacity_and_transform_change_what_a_box_draws() {
  let card = Card::new(200, 100, Element::boxed("div").child(Element::boxed("div").with("width", "50px").with("height", "50px").with("background-color", "black").with("opacity", "0.5").with("transform", "translateX(100px)")));
  let (p, _) = rendered("opacity-transform", &card, &Images::new());
  assert_eq!(px(&p, 25, 25)[3], 0);
  let a = px(&p, 125, 25)[3];
  assert!((120..=135).contains(&a), "alpha {a}");
}

#[test]
fn runs_add_their_own_style_to_the_paragraph() {
  let card = Card::new(500, 120, page(Element::Text { tag: "p".into(), style: vec![("font-size".into(), "40px".into()), ("color".into(), "black".into())], runs: vec![Run::new("Plain "), Run::new("red").with("color", "rgb(255,0,0)").with("font-weight", "700")] }));
  let (p, warnings) = rendered("runs", &card, &Images::new());
  assert!(warnings.is_empty(), "{warnings:?}");
  let red = (0..p.width()).any(|x| (0..p.height()).any(|y| px(&p, x, y) == [255, 0, 0, 255]));
  let black = !ink_rows(&p, 40).is_empty();
  assert!(red && black);
}

#[test]
fn a_woff2_face_is_picked_by_its_family() {
  let bytes = std::fs::read(concat!(env!("CARGO_MANIFEST_DIR"), "/../cli/tests/fixtures/Fraunces-vietnamese.woff2")).unwrap();
  let mut fonts = Fonts::new();
  let added = fonts.add(bytes).unwrap();
  assert_eq!(added[0].family, "Fraunces");
  assert!(fonts.faces().last().unwrap().bundled);
  let card = Card::new(300, 100, page(Element::text("p", "Ơ ư").with("font-family", "Fraunces").with("font-size", "40px")));
  let out = render(&card, &fonts, &Images::new(), Format::Png).unwrap();
  assert!(out.warnings.is_empty(), "{:?}", out.warnings);
}

#[test]
fn jpeg_output_is_a_jpeg_and_a_bad_size_fails() {
  let card = Card::new(64, 64, Element::boxed("div").with("background-color", "red"));
  let out = render(&card, &Fonts::new(), &Images::new(), Format::Jpeg { quality: 85 }).unwrap();
  assert_eq!(&out.bytes[..3], &[0xFF, 0xD8, 0xFF]);
  assert!(matches!(render(&Card::new(0, 10, Element::boxed("div")), &Fonts::new(), &Images::new(), Format::Png), Err(CardError::Size { .. })));
}

#[test]
fn a_tree_deeper_than_the_limit_fails_instead_of_overflowing_the_stack() {
  let mut el = Element::boxed("div");
  for _ in 0..100 {
    el = Element::boxed("div").child(el);
  }
  assert!(matches!(render(&Card::new(10, 10, el), &Fonts::new(), &Images::new(), Format::Png), Err(CardError::Depth { .. })));
}
