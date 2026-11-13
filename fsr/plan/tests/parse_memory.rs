use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{AtomicUsize, Ordering::Relaxed};

use snapfire_fsr_plan::Manifest;

struct Counting;

static LIVE: AtomicUsize = AtomicUsize::new(0);
static PEAK: AtomicUsize = AtomicUsize::new(0);

unsafe impl GlobalAlloc for Counting {
  unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
    let p = unsafe { System.alloc(layout) };
    if !p.is_null() {
      PEAK.fetch_max(LIVE.fetch_add(layout.size(), Relaxed) + layout.size(), Relaxed);
    }
    p
  }
  unsafe fn dealloc(&self, p: *mut u8, layout: Layout) {
    unsafe { System.dealloc(p, layout) };
    LIVE.fetch_sub(layout.size(), Relaxed);
  }
  unsafe fn realloc(&self, p: *mut u8, layout: Layout, new: usize) -> *mut u8 {
    let q = unsafe { System.realloc(p, layout, new) };
    if !q.is_null() {
      if new >= layout.size() {
        PEAK.fetch_max(LIVE.fetch_add(new - layout.size(), Relaxed) + new - layout.size(), Relaxed);
      } else {
        LIVE.fetch_sub(layout.size() - new, Relaxed);
      }
    }
    q
  }
}

#[global_allocator]
static ALLOCATOR: Counting = Counting;

const PLAN: &str = r#"{"version":2,"routes":[{"pattern":"/","plan":{"id":0,"module":"shell#document","children":[{"slot":"content","node":{"id":1,"module":"routes/index/page.tsx#default"}}]}}],"sources":[],"actions":[],"handlers":[],"components":[]}"#;

/// `pages` pages of `lines` lines of highlighted code each, one element per
/// token, the shape an MDX code block lowers to.
fn highlighted_pages(pages: usize, lines: usize) -> String {
  let mut out = String::new();
  for page in 0..pages {
    let mut render = String::from("(<> (el pre ((class \"hl-code\")) (el code ()");
    for i in 0..lines {
      render.push_str(&format!(
        " (el span ((data-line \"\")) (el span ((class \"hl-source hl-rust\")) (el span ((class \"hl-storage hl-type hl-function hl-rust\")) \"fn\") \" \" (el span ((class \"hl-entity hl-name hl-function hl-rust\")) \"line_{i}\") (el span ((class \"hl-punctuation hl-section hl-parameters hl-begin hl-rust\")) \"(\") (el span ((class \"hl-punctuation hl-section hl-parameters hl-end hl-rust\")) \")\"))) \"\\n\""
      ));
    }
    render.push_str(")))");
    out.push_str(&format!("(component content/page-{page}.mdx#default (owner fsr) (render {render}))\n"));
  }
  out
}

#[test]
fn reading_a_plan_peaks_within_a_quarter_of_the_manifest_it_keeps() {
  let text = Manifest::from_json(PLAN).unwrap().to_sexpr() + &highlighted_pages(40, 100);
  let base = LIVE.load(Relaxed);
  PEAK.store(base, Relaxed);
  let manifest = Manifest::from_text(&text).unwrap();
  let peak = PEAK.load(Relaxed) - base;
  let kept = LIVE.load(Relaxed) - base;
  assert_eq!(manifest.components.len(), 40);
  assert!(kept > 10 << 20, "the fixture's manifest is {kept} bytes, too small to measure against");
  assert!(
    peak * 4 < kept * 5,
    "reading a plan into a manifest of {kept} bytes peaked at {peak} bytes, {:.2}x",
    peak as f64 / kept as f64
  );
}
