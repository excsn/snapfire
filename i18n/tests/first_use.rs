//! The first read of each kind of data leaves the heap about as it found it. This target holds one test, since the allocator it counts with is the process's.

use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{AtomicIsize, Ordering};

use snapfire_i18n::{Currency, Display, NameKind, Percent, Relative, Unit, Width, ZoneRules};

struct Counting;

static LIVE: AtomicIsize = AtomicIsize::new(0);

unsafe impl GlobalAlloc for Counting {
  unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
    LIVE.fetch_add(layout.size() as isize, Ordering::Relaxed);
    unsafe { System.alloc(layout) }
  }

  unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
    LIVE.fetch_sub(layout.size() as isize, Ordering::Relaxed);
    unsafe { System.dealloc(ptr, layout) }
  }

  unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
    LIVE.fetch_add(new_size as isize - layout.size() as isize, Ordering::Relaxed);
    unsafe { System.realloc(ptr, layout, new_size) }
  }
}

#[global_allocator]
static ALLOCATOR: Counting = Counting;

struct NoRules;

impl ZoneRules for NoRules {
  fn observed(&self, _: &str, _: f64) -> Option<(i32, i32)> {
    Some((-18_000, 0))
  }

  fn saving_in_year(&self, _: &str, _: f64) -> Option<i32> {
    None
  }
}

const HELD_LIMIT: isize = 64 << 10;

#[test]
fn the_first_read_of_each_kind_of_data_holds_no_table_of_it() {
  let before = LIVE.load(Ordering::Relaxed);
  assert_eq!(Currency::new("en-US", "USD", Display::Symbol).unwrap().shown(), "$");
  let _ = Percent::new("de");
  let _ = Relative::new("fr", Unit::Day, Width::Long);
  assert_eq!(snapfire_i18n::zone_name("en-US", "America/New_York", false, 0.0, &NoRules), "Eastern Standard Time");
  assert_eq!(snapfire_i18n::numeric_date("de", 2024, 9, 10).as_deref(), Some("10.9.2024"));
  assert_eq!(snapfire_i18n::display_name("en", NameKind::Region, "US", "long", false).as_deref(), Some("United States"));
  let held = LIVE.load(Ordering::Relaxed) - before;
  assert!(held <= HELD_LIMIT, "the first reads left {held} bytes on the heap, over {HELD_LIMIT}");
}
