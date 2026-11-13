//! A host serving a stream of requests holds what it held after the first few hundred. Every kind of request a deployment sees is driven past the caches' capacities, then driven again while the live heap is counted. This target holds one test, since the allocator it counts with is the process's.

use std::alloc::{GlobalAlloc, Layout, System};
use std::path::PathBuf;
use std::sync::atomic::{AtomicIsize, Ordering};

use bytes::Bytes;
use http::{Request, StatusCode, header};
use http_body_util::BodyExt;
use snapfire_fsr_host::Host;

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

const PLAN: &str = r#"(plan 2)
(route /
  (node 0 shell#document (slot content (node 1 routes/index/page.tsx#default (source index)))))
(route |/hello/{name}|
  (node 0
    shell#document
    (slot content
      (node 1
        routes/hello/page.tsx#default
        (source hello)
        (cache-key routes/hello/page.tsx#default)))))
(source index
  lowered
  (module routes/index/page.loader.ts)
  (body (ret (obj (items (call shop list))))))
(source hello
  lowered
  (module routes/hello/page.loader.ts)
  (body
    (ret (obj (greeting (concat "hi " (param name) " via " (query from))) (items (call shop list))))))
(action index.mark
  lowered
  (module routes/index/actions.ts)
  (body (session-set marked () #t) (ret (session marked))))
"#;

const SHOP_OPENAPI: &str = r#"{
  "openapi": "3.0.3",
  "info": { "title": "Shop", "version": "1.0.0" },
  "paths": { "/list": { "get": { "operationId": "list", "responses": { "200": { "description": "the items",
    "content": { "application/json": { "schema": { "type": "array", "items": { "type": "string" } } } } } } } } }
}"#;

const CONFIG: &str = r#"
[app]
dir = "."

[server]
listen = "127.0.0.1:0"

[document]
title = "Soak"

[session]
key = "test-key"
capacity = 64

[cache]
capacity = 64
ttl = "1h"

[clients.shop]
transport = "mock"
"#;

const CAPACITY: usize = 64;
const WARM: usize = 8 * CAPACITY;
const COUNTED: usize = 4_000;
const GROWTH_LIMIT: isize = 64 << 10;

fn app_dir() -> PathBuf {
  let nanos = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
  let dir = snapfire_fsr_test_internal::scratch_root().join(format!("fsr-request-memory-{}-{nanos}", std::process::id()));
  std::fs::create_dir_all(dir.join("generated")).unwrap();
  std::fs::create_dir_all(dir.join("clients")).unwrap();
  let plan = snapfire_fsr_plan::Manifest::from_text(PLAN).unwrap().to_sexpr();
  std::fs::write(dir.join("generated/plan.sexp"), plan).unwrap();
  std::fs::write(dir.join("clients/shop.openapi.json"), SHOP_OPENAPI).unwrap();
  std::fs::write(dir.join("clients/shop.mock.json"), r#"{"list": ["socks", "hat"]}"#).unwrap();
  std::fs::write(dir.join("app.toml"), CONFIG).unwrap();
  dir
}

struct Kind {
  name: &'static str,
  status: StatusCode,
  request: fn(usize) -> Request<Bytes>,
}

fn get(path: String) -> Request<Bytes> {
  Request::get(path).body(Bytes::new()).unwrap()
}

const KINDS: &[Kind] = &[
  Kind { name: "page", status: StatusCode::OK, request: |_| get("/".to_owned()) },
  Kind { name: "payload", status: StatusCode::OK, request: |_| get("/?__payload".to_owned()) },
  Kind { name: "fragment", status: StatusCode::OK, request: |_| get("/?__fragment".to_owned()) },
  Kind { name: "a new path each request", status: StatusCode::NOT_FOUND, request: |i| get(format!("/nowhere/{i}")) },
  Kind { name: "a new query each request", status: StatusCode::OK, request: |i| get(format!("/?track={i}")) },
  Kind { name: "a new cached page each request", status: StatusCode::OK, request: |i| get(format!("/hello/n{i}?from=f{i}")) },
  Kind {
    name: "a new session each request",
    status: StatusCode::OK,
    request: |_| Request::post("/_sf/action/index.mark").header(header::CONTENT_TYPE, "application/json").body(Bytes::from("{}")).unwrap(),
  },
];

async fn drive(host: &Host, kind: &Kind, from: usize, count: usize) {
  for i in from..from + count {
    let response = host.handle((kind.request)(i)).await;
    assert_eq!(response.status(), kind.status, "{} request {i}", kind.name);
    response.into_body().collect().await.unwrap();
  }
}

#[test]
fn a_host_holds_no_more_after_thousands_of_requests_than_after_hundreds() {
  let dir = app_dir();
  let runtime = tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap();
  let host = Host::from(dir.join("app.toml")).unwrap().build().unwrap();
  let grown: Vec<(&str, isize)> = runtime.block_on(async {
    for kind in KINDS {
      drive(&host, kind, 0, WARM).await;
    }
    let mut grown = Vec::new();
    for kind in KINDS {
      let before = LIVE.load(Ordering::Relaxed);
      drive(&host, kind, WARM, COUNTED).await;
      grown.push((kind.name, LIVE.load(Ordering::Relaxed) - before));
    }
    grown
  });
  drop(host);
  std::fs::remove_dir_all(&dir).unwrap();
  let over: Vec<String> = grown.iter().filter(|(_, bytes)| *bytes > GROWTH_LIMIT).map(|(name, bytes)| format!("{name}: {bytes} bytes")).collect();
  assert!(over.is_empty(), "{COUNTED} more requests of each kind left the heap larger by over {GROWTH_LIMIT} bytes:\n  {}\nall kinds: {grown:?}", over.join("\n  "));
}
