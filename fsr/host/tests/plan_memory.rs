use std::alloc::{GlobalAlloc, Layout, System};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering::Relaxed};

use snapfire_fsr_host::{Config, Host};

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

/// Heap bytes `f` holds at its highest above what was live when it started.
fn peak_of<T>(f: impl FnOnce() -> T) -> (T, usize) {
  let base = LIVE.load(Relaxed);
  PEAK.store(base, Relaxed);
  let out = f();
  (out, PEAK.load(Relaxed) - base)
}

fn dir(name: &str) -> PathBuf {
  let nanos = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
  let dir = snapfire_fsr_test_internal::scratch_root().join(format!("fsr-plan-memory-{name}-{}-{nanos}", std::process::id()));
  std::fs::create_dir_all(&dir).unwrap();
  dir
}

const PLAN: &str = "(plan 2)\n(route / (node 0 shell#document (slot content (node 1 routes/index/page.tsx#default))))\n";

/// A page whose body is `lines` lines of highlighted code, one element per
/// token, the shape an MDX code block lowers to.
fn highlighted_page(lines: usize) -> String {
  let mut render = String::from("(<> (el pre ((class \"hl-code\")) (el code ()");
  for i in 0..lines {
    render.push_str(&format!(
      " (el span ((data-line \"\")) (el span ((class \"hl-source hl-rust\")) (el span ((class \"hl-storage hl-type hl-function hl-rust\")) \"fn\") \" \" (el span ((class \"hl-entity hl-name hl-function hl-rust\")) \"line_{i}\") (el span ((class \"hl-punctuation hl-section hl-parameters hl-begin hl-rust\")) \"(\") (el span ((class \"hl-punctuation hl-section hl-parameters hl-end hl-rust\")) \")\"))) \"\\n\""
    ));
  }
  render.push_str(")))");
  format!("(component routes/index/page.tsx#default (owner fsr) (render {render}))\n")
}

/// Writes an application with the build facts a bundled one carries and
/// returns its plan text.
fn app(root: &Path, lines: usize) -> String {
  std::fs::create_dir_all(root.join("app/generated")).unwrap();
  std::fs::create_dir_all(root.join("app/dist")).unwrap();
  let text = PLAN.to_owned() + &highlighted_page(lines);
  std::fs::write(root.join("app/generated/plan.sexp"), &text).unwrap();
  std::fs::write(
    root.join("app/dist/.snapfire-build.json"),
    r#"{"version":1,"publicPath":"/static/js/app","entries":["src/main.js"],"outputs":["src/main.js"]}"#,
  )
  .unwrap();
  std::fs::write(root.join("app.toml"), "[app]\ndir = \"app\"\n[document]\ntitle = \"t\"\n[session]\nkey = \"k\"\n").unwrap();
  text
}

fn host_peak(lines: usize) -> (usize, usize) {
  let root = dir(&format!("lines-{lines}"));
  let text = app(&root, lines);
  let (manifest, _) = peak_of(|| snapfire_fsr_plan::Manifest::from_text(&text).unwrap());
  let held = {
    let base = LIVE.load(Relaxed);
    drop(manifest);
    base - LIVE.load(Relaxed)
  };
  let (host, peak) = peak_of(|| Host::from_config(Config::load(&root).unwrap()).unwrap().build().unwrap());
  assert!(host.report().app.routes.iter().any(|r| r.0 == "/"));
  drop(host);
  std::fs::remove_dir_all(&root).unwrap();
  (held, peak)
}

#[test]
fn building_a_host_holds_its_plan_at_most_twice_at_peak() {
  let (_, small) = host_peak(10);
  let (manifest, large) = host_peak(4_000);
  let build = large - small;
  assert!(manifest > 10 << 20, "the fixture's manifest is {manifest} bytes, too small to measure against");
  assert!(
    build < 2 * manifest,
    "building a host whose plan's manifest holds {manifest} bytes peaked {build} bytes above a near-empty plan, {:.2}x",
    build as f64 / manifest as f64
  );
}
