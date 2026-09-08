//! How the renderer scales, on the axes the three storefront pages cannot show.
//!
//! `list` renders the app's own catalogue route at four list lengths, which is
//! the axis every per-item optimisation is judged on. `depth` and `compute` are
//! synthetic IR rather than app source, so the shape under test is exactly the
//! one named and nothing else moves with it. `threads` renders on many cores at
//! once, which no other bench here touches and which is where an `Arc` in
//! `Value` has to pay for its atomics.
//!
//! It also writes `props-list-<n>.json` beside the other fixtures so
//! `render.node.mjs` can put React in V8 on the same list lengths.

use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use criterion::{BenchmarkId, Criterion, black_box, criterion_group, criterion_main};
use snapfire_fsr_cli::spec::prepare;
use snapfire_fsr_cli::{Options, build};
use snapfire_fsr_core::{Value, ValueMap};
use snapfire_fsr_ir::render::Components;
use snapfire_fsr_ir::{ArithOp, Component, Entry, Expr, Interpreter, Lit, Tmpl};
use snapfire_fsr_payload::value_to_json;

#[cfg(feature = "mimalloc")]
#[global_allocator]
static ALLOC: mimalloc::MiMalloc = mimalloc::MiMalloc;

const LIST_LENGTHS: &[usize] = &[1, 12, 100, 1000];
const DEPTHS: &[usize] = &[1, 4, 16, 64];
const COMPUTE_OPS: &[usize] = &[0, 8, 64, 512];
const THREADS: &[usize] = &[1, 2, 4, 8];
const RENDERS_PER_THREAD: usize = 50;

const CATALOGUE: &str = "routes/page.tsx#default";

fn map(entries: Vec<(&str, Value)>) -> Value {
  Value::Map(entries.into_iter().map(|(k, v)| (k.to_owned(), v)).collect())
}

fn product(id: i128) -> Value {
  map(vec![
    ("id", Value::Int(id)),
    ("name", Value::str(format!("Filament {id}"))),
    ("brand", Value::str("Prusa")),
    ("category", Value::str(if id % 3 == 0 { "tools" } else { "printing" })),
    ("price_cents", Value::Int(2400 + id * 100)),
    ("list_price_cents", Value::Int(2900 + id * 100)),
    ("image", map(vec![("color", Value::str("#e8d5b5")), ("emoji", Value::str("🧵"))])),
    ("rating", Value::F64(4.5)),
    ("reviews", Value::Int(12 * id)),
    ("stock", Value::Int(id % 4 * 3)),
    ("description", Value::str("A spool of filament for the printer on your desk, wound tight and dried before shipping.")),
    ("tags", Value::seq(vec![Value::str("pla"), Value::str("1.75mm")])),
    ("attributes", Value::seq(vec![map(vec![("name", Value::str("Ingredients")), ("value", Value::str("PLA"))]), map(vec![("name", Value::str("Weight")), ("value", Value::str("1 kg"))])])),
  ])
}

fn catalogue_props(n: usize) -> ValueMap {
  let products: Vec<Value> = (1..=n as i128).map(product).collect();
  let Value::Map(props) = map(vec![
    ("products", Value::seq(products)),
    ("q", Value::str("")),
    ("category", Value::str("printing")),
    ("cartCount", Value::Int(2)),
  ]) else {
    unreachable!()
  };
  props
}

/// `depth` components, each rendering the next inside one element, with the
/// innermost printing a prop. Every level is a real component boundary, which
/// is what the scope and slot handling is charged for.
fn nested(depth: usize) -> (Components, Arc<Component>) {
  let mut library: Components = Components::new();
  let leaf = Component {
    body: Vec::new(),
    render: Tmpl::Element {
      tag: "span".to_owned(),
      attrs: vec![Entry::Field("class".to_owned(), Expr::lit_str("leaf"))],
      children: vec![Tmpl::Expr(Expr::var("$props").field("label"))],
    },
    state: Vec::new(),
    handlers: Vec::new(),
  };
  library.insert("depth/leaf".to_owned(), Arc::new(leaf));
  let mut inner = "depth/leaf".to_owned();
  for level in 0..depth {
    let name = format!("depth/level{level}");
    let component = Component {
      body: Vec::new(),
      render: Tmpl::Element {
        tag: "div".to_owned(),
        attrs: vec![Entry::Field("class".to_owned(), Expr::lit_str("level"))],
        children: vec![Tmpl::Component {
          module: inner.clone(),
          props: vec![Entry::Field("label".to_owned(), Expr::var("$props").field("label"))],
          children: Vec::new(),
          id: 0,
        }],
      },
      state: Vec::new(),
      handlers: Vec::new(),
    };
    library.insert(name.clone(), Arc::new(component));
    inner = name;
  }
  let root = library.get(&inner).cloned().expect("the outermost level");
  (library, root)
}

/// One element whose text is `ops` chained additions over a prop. This is the
/// shape a JIT is built for and the interpreter is not, so it is where the
/// comparison is expected to invert.
fn arithmetic(ops: usize) -> (Components, Arc<Component>) {
  let mut expr = Expr::var("$props").field("n");
  for i in 0..ops {
    expr = Expr::Arith(ArithOp::Add, Box::new(expr), Box::new(Expr::Lit(Lit::Int(i as i128))));
  }
  let component = Component {
    body: Vec::new(),
    render: Tmpl::Element { tag: "p".to_owned(), attrs: Vec::new(), children: vec![Tmpl::Expr(expr)] },
    state: Vec::new(),
    handlers: Vec::new(),
  };
  (Components::new(), Arc::new(component))
}

fn machine_state() -> String {
  let power = std::process::Command::new("pmset").args(["-g"]).output().ok().map(|o| String::from_utf8_lossy(&o.stdout).lines().find(|l| l.contains("powermode")).map(|l| l.trim().to_owned()).unwrap_or_default()).unwrap_or_default();
  let load = std::process::Command::new("uptime").output().ok().map(|o| String::from_utf8_lossy(&o.stdout).trim().to_owned()).unwrap_or_default();
  format!("{power}; {load}")
}

fn bench(c: &mut Criterion) {
  eprintln!("machine before: {}", machine_state());
  let app = Path::new(env!("CARGO_MANIFEST_DIR")).join("app");
  let workspace_snapfirec = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../target/debug/snapfirec");
  if std::env::var_os("SNAPFIREC").is_none() && workspace_snapfirec.is_file() {
    unsafe { std::env::set_var("SNAPFIREC", &workspace_snapfirec) };
  }
  let built = build(&app, &Options::default()).expect("fsr build app");
  let components: Arc<Components> = Arc::new(built.manifest.components.iter().map(|c| (c.module.clone(), Arc::new(snapfire_fsr_ir::render::prepare(&c.body)))).collect());
  let catalogue = components.get(CATALOGUE).cloned().expect("the catalogue page lowered");
  let interpreter = Interpreter::default();
  let prepared = prepare(&app).expect("fsr test's preparation");

  for &n in LIST_LENGTHS {
    let props = catalogue_props(n);
    let html = interpreter.render(&catalogue, &props, &components).expect("renders").html;
    eprintln!("list/{n}: {} bytes of markup", html.len());
    std::fs::write(prepared.test_dir.join(format!("props-list-{n}.json")), value_to_json(&Value::Map(props.clone())).to_string()).expect("dump");
    std::fs::write(prepared.test_dir.join(format!("render-list-{n}.ir.html")), &html).expect("dump");
    c.bench_with_input(BenchmarkId::new("scale/list", n), &props, |b, props| {
      b.iter(|| interpreter.render(black_box(&catalogue), black_box(props), &components).expect("renders"))
    });
  }

  for &depth in DEPTHS {
    let (library, root) = nested(depth);
    let Value::Map(props) = map(vec![("label", Value::str("deep"))]) else { unreachable!() };
    c.bench_with_input(BenchmarkId::new("scale/depth", depth), &props, |b, props| {
      b.iter(|| interpreter.render(black_box(&root), black_box(props), &library).expect("renders"))
    });
  }

  for &ops in COMPUTE_OPS {
    let (library, root) = arithmetic(ops);
    let Value::Map(props) = map(vec![("n", Value::Int(1))]) else { unreachable!() };
    c.bench_with_input(BenchmarkId::new("scale/compute", ops), &props, |b, props| {
      b.iter(|| interpreter.render(black_box(&root), black_box(props), &library).expect("renders"))
    });
  }

  // `RENDERS_PER_THREAD` renders per thread per iteration, so one thread spawn
  // is amortised over many renders rather than being most of what is timed.
  // Wall time divided by `threads * RENDERS_PER_THREAD` is the per-render cost,
  // and a flat column across the rows is linear scaling.
  let props = Arc::new(catalogue_props(12));
  for &threads in THREADS {
    let interpreter = Arc::new(Interpreter::default());
    c.bench_with_input(BenchmarkId::new("scale/threads", threads), &threads, |b, &threads| {
      b.iter(|| {
        let done = Arc::new(AtomicUsize::new(0));
        std::thread::scope(|scope| {
          for _ in 0..threads {
            let (interpreter, components, catalogue, props, done) = (Arc::clone(&interpreter), Arc::clone(&components), Arc::clone(&catalogue), Arc::clone(&props), Arc::clone(&done));
            scope.spawn(move || {
              let mut bytes = 0;
              for _ in 0..RENDERS_PER_THREAD {
                bytes += interpreter.render(&catalogue, &props, &components).expect("renders").html.len();
              }
              done.fetch_add(bytes, Ordering::Relaxed);
            });
          }
        });
        black_box(done.load(Ordering::Relaxed))
      })
    });
  }
}

fn machine_after(_c: &mut Criterion) {
  eprintln!("machine after: {}", machine_state());
}

criterion_group!(benches, bench, machine_after);
criterion_main!(benches);
