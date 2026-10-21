//! Each case that builds, rendered by the IR and by React's `renderToString` in QuickJS: whether the two agree once what legitimately differs is taken out, and what each render costs. Needs the network once, for react-dom/server. `PATTERNS_LABEL` names the run; its table goes to `results/<label>.tsv`.

use std::sync::Arc;
use std::time::Duration;

use criterion::{black_box, BenchmarkId, Criterion};
use patterns::react::react_engine;
use patterns::{app_of, cases, flatten, normalised, outcome, root, without_fsr_markers, PAGE};
use snapfire_fsr_core::ValueMap;
use snapfire_fsr_engine::Engine;
use snapfire_fsr_ir::render::Components;
use snapfire_fsr_ir::Interpreter;

struct Ready {
  name: String,
  component: Arc<snapfire_fsr_ir::ast::Component>,
  components: Arc<Components>,
  engine: Option<Engine>,
}

fn bench(c: &mut Criterion) {
  let label = std::env::var("PATTERNS_LABEL").unwrap_or_else(|_| "current".to_owned());
  let scratch = std::env::temp_dir().join(format!("fsr-patterns-bench-{label}"));
  let _ = std::fs::remove_dir_all(&scratch);
  let cache = scratch.join("react-dom-server");
  let rt = tokio::runtime::Builder::new_current_thread().build().expect("runtime");
  let interpreter = Interpreter::default();
  let mut rows = vec!["case\toutcome\treact\tir_bytes\treact_bytes".to_owned()];
  let mut ready = Vec::new();
  for case in cases() {
    let app = app_of(&case, &scratch);
    let (line, built) = outcome(&app);
    let kind = line.split(':').next().unwrap_or_default().to_owned();
    let Some(built) = built else {
      rows.push(format!("{}\t{kind}\t-\t-\t-", case.name));
      continue;
    };
    let components: Arc<Components> = Arc::new(built.manifest.components.iter().map(|c| (c.module.clone(), Arc::new(snapfire_fsr_ir::render::prepare(&c.body)))).collect());
    let Some(component) = components.get(PAGE).cloned() else {
      rows.push(format!("{}\t{kind}\tno page\t-\t-", case.name));
      continue;
    };
    let ir = interpreter.render(&component, &ValueMap::default(), &components).map(|r| flatten(&r));
    let react = react_engine(&app, &built, &cache, &rt).and_then(|engine| {
      let html = engine.eval_string("__render({})").map_err(|e| format!("react: {e}"))?;
      Ok((engine, html))
    });
    let (verdict, ir_bytes, react_bytes, engine) = match (&ir, react) {
      (Ok(ir), Ok((engine, html))) => {
        std::fs::write(scratch.join(format!("{}.ir.html", case.name)), ir).ok();
        std::fs::write(scratch.join(format!("{}.react.html", case.name)), &html).ok();
        let verdict = match (normalised(ir) == normalised(&html), normalised(&without_fsr_markers(ir)) == normalised(&html)) {
          (true, _) => "same",
          (false, true) => "equivalent",
          (false, false) => "differs",
        };
        (verdict.to_owned(), ir.len().to_string(), html.len().to_string(), Some(engine))
      }
      (Err(e), _) => (format!("ir fails: {}", e.message.lines().next().unwrap_or_default()), "-".to_owned(), "-".to_owned(), None),
      (Ok(_), Err(e)) => (format!("react fails: {}", e.lines().next().unwrap_or_default()), "-".to_owned(), "-".to_owned(), None),
    };
    rows.push(format!("{}\t{kind}\t{verdict}\t{ir_bytes}\t{react_bytes}", case.name));
    if ir.is_ok() {
      ready.push(Ready { name: case.name.clone(), component, components, engine });
    }
  }
  let table = rows.join("\n") + "\n";
  std::fs::create_dir_all(root().join("results")).expect("results/");
  std::fs::write(root().join("results").join(format!("{label}.tsv")), &table).expect("results/");
  eprintln!("{table}");
  for case in &ready {
    c.bench_with_input(BenchmarkId::new(format!("{label}/ir"), &case.name), case, |b, case| b.iter(|| interpreter.render(black_box(&case.component), black_box(&ValueMap::default()), &case.components).expect("ir renders")));
    if let Some(engine) = &case.engine {
      c.bench_with_input(BenchmarkId::new(format!("{label}/react"), &case.name), case, |b, _| b.iter(|| engine.eval_string("__render({})").expect("react renders")));
    }
  }
}

fn main() {
  let mut c = Criterion::default().sample_size(20).measurement_time(Duration::from_secs(1)).warm_up_time(Duration::from_millis(300)).configure_from_args();
  bench(&mut c);
  c.final_summary();
}
