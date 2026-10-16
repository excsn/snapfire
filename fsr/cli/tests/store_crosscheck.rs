//! The browser's store and the server's agree: generated contributions, slot
//! orders and derives, merged by `runtime::store` here and replayed through
//! `store.ts` by a spec, read as the same values and the same pending keys.

use std::collections::BTreeSet;
use std::path::PathBuf;
use std::sync::Arc;

use snapfire_fsr_cli::{test, Options};
use snapfire_fsr_core::{Data, Value};
use snapfire_fsr_runtime::RequestCtx;
use snapfire_fsr_runtime::store::{apply_derives, derive_order, derived_pending, merge_contributions, pending_keys, Contribution, Derived};

const CASES: usize = 400;
const KEYS: [&str; 5] = ["k0", "k1", "k2", "k3", "k4"];
const SLOTS: [&str; 3] = ["a", "b", "c"];

struct Rng(u64);

impl Rng {
  fn next(&mut self) -> u64 {
    self.0 = self.0.wrapping_add(0x9e37_79b9_7f4a_7c15);
    let mut z = self.0;
    z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    z ^ (z >> 31)
  }

  fn below(&mut self, n: usize) -> usize {
    (self.next() % n as u64) as usize
  }

  fn chance(&mut self, percent: u64) -> bool {
    self.next() % 100 < percent
  }

  fn subset<'a>(&mut self, from: &[&'a str], percent: u64) -> Vec<&'a str> {
    from.iter().copied().filter(|_| self.chance(percent)).collect()
  }

  fn shuffle<T>(&mut self, items: &mut [T]) {
    for i in (1..items.len()).rev() {
      items.swap(i, self.below(i + 1));
    }
  }
}

struct Case {
  slot_order: Vec<String>,
  contributions: Vec<Contribution>,
  /// Each derived key and its sources, in the order the browser registers them.
  derives: Vec<(String, Vec<String>)>,
}

fn case(rng: &mut Rng) -> Case {
  let mut slot_order: Vec<String> = rng.subset(&SLOTS, 50).into_iter().map(str::to_owned).collect();
  rng.shuffle(&mut slot_order);
  let mut contributions: Vec<Contribution> = Vec::new();
  for i in 0..1 + rng.below(5) {
    let path: Vec<String> = (0..rng.below(3)).map(|_| SLOTS[rng.below(SLOTS.len())].to_owned()).collect();
    if contributions.iter().any(|c| c.path == path) {
      continue;
    }
    let segment = format!("s{i}");
    contributions.push(if rng.chance(30) {
      let awaits = rng.subset(&KEYS, 40);
      Contribution::promised(segment, path, if awaits.is_empty() { vec![KEYS[0].to_owned()] } else { awaits.into_iter().map(str::to_owned).collect() })
    } else {
      Contribution::seeded(segment, path, rng.subset(&KEYS, 40).into_iter().map(|key| (key.to_owned(), Value::Int(rng.below(10) as i128))).collect())
    });
  }
  let mut derives: Vec<(String, Vec<String>)> = Vec::new();
  for (at, key) in KEYS.iter().enumerate().skip(1) {
    if rng.chance(35) {
      derives.push(((*key).to_owned(), rng.subset(&KEYS[..at], 50).into_iter().map(str::to_owned).collect()));
    }
  }
  rng.shuffle(&mut derives);
  Case { slot_order, contributions, derives }
}

/// What the server reads every key as and which keys it holds readers of.
fn server(case: &Case) -> (Data, BTreeSet<String>) {
  let derives = derive_order(
    case
      .derives
      .iter()
      .map(|(key, sources)| {
        let read = sources.clone();
        Derived::new(key.clone(), sources.clone(), Some(Arc::new(move |_: &RequestCtx, store: &Data| Some(Value::Int(read.iter().map(|s| match store.get(s) { Some(Value::Int(n)) => *n, _ => 0 }).sum())))))
      })
      .collect(),
  );
  let mut store = merge_contributions(&case.contributions, &case.slot_order);
  apply_derives(&RequestCtx::anonymous(Default::default()), &mut store, &derives);
  let mut pending = pending_keys(&case.contributions, &case.slot_order);
  derived_pending(&mut pending, &derives);
  (store, pending)
}

fn strings(items: &[String]) -> String {
  format!("[{}]", items.iter().map(|s| format!("\"{s}\"")).collect::<Vec<_>>().join(","))
}

fn json(case: &Case) -> String {
  let (store, pending) = server(case);
  let contributions: Vec<String> = case
    .contributions
    .iter()
    .map(|c| {
      let values: Vec<String> = c.values.iter().map(|(k, v)| format!("\"{k}\":{}", match v { Value::Int(n) => n.to_string(), _ => unreachable!() })).collect();
      let awaits = if c.awaits.is_empty() { String::new() } else { format!(",\"w\":{}", strings(&c.awaits)) };
      format!("{{\"k\":\"{}\",\"p\":{},\"v\":{{{}}}{awaits}}}", c.segment, strings(&c.path), values.join(","))
    })
    .collect();
  let derives: Vec<String> = case.derives.iter().map(|(key, sources)| format!("{{\"key\":\"{key}\",\"sources\":{}}}", strings(sources))).collect();
  let values: Vec<String> = KEYS.iter().filter_map(|k| store.get(*k).map(|v| format!("\"{k}\":{}", match v { Value::Int(n) => n.to_string(), _ => unreachable!() }))).collect();
  let pending: Vec<String> = pending.into_iter().collect();
  format!(
    "{{\"slotOrder\":{},\"contributions\":[{}],\"derives\":[{}],\"values\":{{{}}},\"pending\":{}}}",
    strings(&case.slot_order),
    contributions.join(","),
    derives.join(","),
    values.join(","),
    strings(&pending)
  )
}

const SPEC: &str = r#"import { contribute, derive, get, isPending, reset, setSlotOrder } from "@snapfire/fsr-client/store";
import { expect, test } from "@snapfire/fsr-client/testing";

interface Case {
  slotOrder: string[];
  contributions: { k: string; p: string[]; v: { [key: string]: number }; w?: string[] }[];
  derives: { key: string; sources: string[] }[];
  values: { [key: string]: number };
  pending: string[];
}

const KEYS = ["k0", "k1", "k2", "k3", "k4"];
const CASES: Case[] = __CASES__;

test("the browser's store reads every generated case as the server's does", () => {
  const wrong: string[] = [];
  CASES.forEach((c, i) => {
    const at = (name: string) => `c${i}/${name}`;
    reset();
    setSlotOrder(c.slotOrder);
    for (const d of c.derives) {
      derive(at(d.key), d.sources.map(at), (read) => d.sources.reduce((sum, s) => sum + ((read(at(s)) as number | undefined) ?? 0), 0));
    }
    contribute(c.contributions.map((x) => ({ k: x.k, p: x.p, v: Object.fromEntries(Object.entries(x.v).map(([k, v]) => [at(k), v])), ...(x.w ? { w: x.w.map(at) } : {}) })));
    const values: { [key: string]: number } = {};
    for (const k of KEYS) {
      const value = get<number>(at(k));
      if (value !== undefined) values[k] = value;
    }
    const pending = KEYS.filter((k) => isPending(at(k)));
    const browser = JSON.stringify({ values, pending });
    const server = JSON.stringify({ values: Object.fromEntries(KEYS.filter((k) => k in c.values).map((k) => [k, c.values[k]])), pending: c.pending });
    if (browser !== server) wrong.push(`case ${i}: browser ${browser}, server ${server}, given ${JSON.stringify(c)}`);
  });
  expect(wrong.length, wrong.slice(0, 5).join("\n")).toEqual(0);
});
"#;

fn app(spec: &str) -> PathBuf {
  let root = std::env::temp_dir().join(format!("fsr-cli-store-crosscheck-{}", std::process::id()));
  let _ = std::fs::remove_dir_all(&root);
  let dir = root.join("app");
  std::fs::create_dir_all(root.join("config")).unwrap();
  std::fs::write(root.join("config/app.toml"), "[server]\nlisten = \"127.0.0.1:0\"\n\n[session]\nkey = \"crosscheck-key\"\nttl = \"1h\"\n").unwrap();
  for (name, source) in [
    ("importmap.json", r#"{"imports":{"@snapfire/fsr-client":"/static/js/fsr/index.js","@snapfire/fsr-client/store":"/static/js/fsr/store.js"}}"#),
    ("routes/page.tsx", "export default function Page() {\n  return <p>page</p>;\n}\n"),
    ("tests/store.spec.ts", spec),
  ] {
    let path = dir.join(name);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, source).unwrap();
  }
  dir
}

#[test]
fn the_browser_store_and_the_server_store_agree_on_generated_cases() {
  let mut rng = Rng(0x5eed_9630);
  let cases: Vec<String> = (0..CASES).map(|_| json(&case(&mut rng))).collect();
  let dir = app(&SPEC.replace("__CASES__", &format!("[\n{}\n]", cases.join(",\n"))));
  let summary = test::run(&dir, &Options::beside(&dir), &snapfire_fsr_cli::typecheck::Typecheck::default(), None).unwrap();
  let _ = std::fs::remove_dir_all(dir.parent().unwrap());
  assert_eq!((summary.failed, summary.passed), (0, 1), "{summary}");
}
