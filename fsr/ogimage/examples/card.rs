use snapfire_fsr_ogimage::{render, Card, Element, Fonts, Format, Images, Run};

fn main() {
  let out = std::env::args().nth(1).unwrap_or_else(|| "card.png".into());
  let root = Element::boxed("div")
    .with("flex-direction", "column")
    .with("justify-content", "space-between")
    .with("width", "100%")
    .with("height", "100%")
    .with("padding", "64px")
    .with("background-image", "linear-gradient(135deg, #0f172a 0%, #1e3a8a 100%)")
    .with("color", "white")
    .child(
      Element::boxed("div")
        .with("align-items", "center")
        .with("gap", "16px")
        .child(Element::boxed("div").with("width", "48px").with("height", "48px").with("border-radius", "12px").with("background-color", "#f97316").with("box-shadow", "0 8px 24px rgba(249,115,22,0.5)"))
        .child(Element::text("span", "Noticeboard").with("font-size", "28px").with("font-weight", "700").with("opacity", "0.8")),
    )
    .child(Element::text("h1", "Roof work on the east stair closes it from Monday to Wednesday while the flashing is replaced").with("font-size", "72px").with("font-weight", "700").with("line-height", "1.1").with("line-clamp", "2"))
    .child(
      Element::Text {
        tag: "p".into(),
        style: vec![("font-size".into(), "28px".into())],
        runs: vec![Run::new("Posted 15 September 2026 · "), Run::new("noticeboard.example").with("text-decoration", "underline").with("color", "#fdba74")],
      },
    );
  let card = Card::new(1200, 630, root);
  let runs: u32 = std::env::args().nth(2).and_then(|n| n.parse().ok()).unwrap_or(1);
  let fonts = Fonts::new();
  let images = Images::new();
  let start = std::time::Instant::now();
  let mut rendered = render(&card, &fonts, &images, Format::Png).expect("renders");
  for _ in 1..runs {
    rendered = render(&card, &fonts, &images, Format::Png).expect("renders");
  }
  let took = start.elapsed() / runs;
  for w in &rendered.warnings {
    eprintln!("warning: {w}");
  }
  std::fs::write(&out, &rendered.bytes).expect("writes");
  println!("{out}: {} bytes in {took:?}", rendered.bytes.len());
}
