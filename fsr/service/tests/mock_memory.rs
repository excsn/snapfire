//! A mock transport answers from its canned responses and keeps nothing per call, so a host serving `transport = "mock"` holds the same memory after a million requests as after one. This target holds one test, since the allocator it counts with is the process's.

use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{AtomicIsize, Ordering};

use futures::executor::block_on;
use snapfire_fsr_core::{Value, ValueMap};
use snapfire_fsr_runtime::FailureKind;
use snapfire_fsr_service::{Call, MockTransport, Transport};

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

#[test]
fn a_mock_transport_holds_nothing_per_call() {
  let mock = MockTransport::new().returns("program.listTalks", Value::seq(vec![Value::str("keynote")])).fails("program.listSponsors", FailureKind::Unavailable, "the sponsors service is not answering");
  let call = |method: &str| {
    let mut args = ValueMap::default();
    args.insert("track".to_owned(), Value::str("all"));
    let _ = block_on(mock.call(Call::new("program", method, args)));
  };
  for _ in 0..100 {
    call("listTalks");
    call("listSponsors");
  }
  let before = LIVE.load(Ordering::Relaxed);
  for _ in 0..10_000 {
    call("listTalks");
    call("listSponsors");
  }
  let grown = LIVE.load(Ordering::Relaxed) - before;
  assert!(grown < 4096, "20,000 calls left {grown} bytes live");
}
