use super::*;
use snapfire_fsr_ir::ast::{ArithOp, CompareOp};

fn app(files: &[(&str, &str)]) -> PathBuf {
  static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
  let dir = snapfire_fsr_test_internal::scratch_root().join(format!("fsr_component_{}_{}", std::process::id(), NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)));
  let _ = std::fs::remove_dir_all(&dir);
  for (name, source) in files {
    let path = dir.join(name);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, source).unwrap();
  }
  dir
}

fn lower(files: &[(&str, &str)], module: &str) -> Result<Vec<(String, Component)>, LowerError> {
  let mut set = ComponentSet::new(&app(files));
  set.lower(module)?;
  Ok(set.components)
}

/// The attributes the markup prints: without the build's `$` markers.
fn plain(attrs: &[Entry]) -> Vec<Entry> {
  attrs.iter().filter(|e| !matches!(e, Entry::Field(n, _) if n.starts_with('$'))).cloned().collect()
}

fn hoist_ids(component: &Component) -> Vec<u32> {
  let mut ids = Vec::new();
  component.visit(&mut |e| {
    if let Expr::Hoist { id, .. } = e {
      ids.push(*id);
    }
  });
  ids
}

fn chunk_ids(tmpl: &Tmpl, out: &mut Vec<(String, u32)>) {
  match tmpl {
    Tmpl::Baked { .. } => {}
    Tmpl::Element { tag, attrs, children } => {
      if let Some(Entry::Field(_, Expr::Lit(Lit::Int(id)))) = attrs.iter().find(|e| matches!(e, Entry::Field(n, _) if n == hoist::CHUNK_ATTR)) {
        out.push((tag.clone(), *id as u32));
      }
      children.iter().for_each(|c| chunk_ids(c, out));
    }
    Tmpl::Fragment(children) | Tmpl::Place { children, .. } | Tmpl::Component { children, .. } | Tmpl::Island { children, .. } => children.iter().for_each(|c| chunk_ids(c, out)),
    Tmpl::If { then, r#else, .. } => {
      chunk_ids(then, out);
      if let Some(e) = r#else {
        chunk_ids(e, out);
      }
    }
    Tmpl::For { body, .. } => chunk_ids(body, out),
    Tmpl::Let { then, .. } => chunk_ids(then, out),
    Tmpl::Text(_) | Tmpl::Marker(_) | Tmpl::Outlet { .. } | Tmpl::Html(_) | Tmpl::Expr(_) | Tmpl::Slot(_) => {}
  }
}

fn set(files: &[(&str, &str)], module: &str) -> ComponentSet {
  let mut set = ComponentSet::new(&app(files));
  set.lower(module).unwrap();
  set
}

fn placements(tmpl: &Tmpl, out: &mut Vec<(String, bool)>) {
  match tmpl {
    Tmpl::Component { module, children, keyed, .. } => {
      out.push((module.clone(), *keyed));
      children.iter().for_each(|c| placements(c, out));
    }
    Tmpl::Element { children, .. } | Tmpl::Fragment(children) | Tmpl::Island { children, .. } => children.iter().for_each(|c| placements(c, out)),
    Tmpl::If { then, r#else, .. } => {
      placements(then, out);
      if let Some(e) = r#else {
        placements(e, out);
      }
    }
    Tmpl::For { body, .. } => placements(body, out),
    Tmpl::Let { then, .. } => placements(then, out),
    _ => {}
  }
}

#[test]
fn a_placement_of_a_component_that_keys_anything_is_keyed_and_wrapped() {
  let files = [
    (
      "routes/index/page.tsx",
      r#"
import Card from "@src/ui/Card";
import Plain from "@src/ui/Plain";
import Tree from "@src/ui/Tree";
export default function Index({ items, tree }: { items: string[]; tree: { label: string; kids: any[] } }) {
  return (
    <div>
      {items.map((item) => <Card label={item} />)}
      <Card label="last" />
      <Plain />
      <Tree node={tree} />
    </div>
  );
}
"#,
    ),
    (
      "src/ui/Card.tsx",
      r#"
import { Island } from "@snapfire/fsr/client/react";
import Body from "@src/ui/Body";
export default function Card({ label }: { label: string }) {
  return <Island when="load"><Body label={label} /></Island>;
}
"#,
    ),
    ("src/ui/Body.tsx", "import { useState } from \"react\";\nexport default function Body({ label }: { label: string }) {\n  const [n, setN] = useState(0);\n  return <button onClick={() => setN(n + 1)}>{label}{n}</button>;\n}\n"),
    ("src/ui/Plain.tsx", "export default function Plain() {\n  return <i>plain</i>;\n}\n"),
    ("src/ui/Tree.tsx", "export default function Tree({ node }: { node: { label: string; kids: any[] } }) {\n  return <ul><li>{node.label}</li>{node.kids.map((kid) => <Tree node={kid} />)}</ul>;\n}\n"),
  ];
  let set = set(&files, "routes/index/page.tsx#default");
  let page = &set.components.iter().find(|(m, _)| m == "routes/index/page.tsx#default").unwrap().1;
  let mut placed = Vec::new();
  placements(&page.render, &mut placed);
  assert_eq!(
    placed,
    [("src/ui/Card.tsx#default".to_owned(), true), ("src/ui/Card.tsx#default".to_owned(), true), ("src/ui/Plain.tsx#default".to_owned(), false), ("src/ui/Tree.tsx#default".to_owned(), true)],
    "Card keys a region and Tree renders itself; Plain keys nothing"
  );
  let tree = &set.components.iter().find(|(m, _)| m == "src/ui/Tree.tsx#default").unwrap().1;
  let mut placed = Vec::new();
  placements(&tree.render, &mut placed);
  assert_eq!(placed, [("src/ui/Tree.tsx#default".to_owned(), true)], "a component met in its own cycle counts as keyed");
  let rewritten: HashMap<String, String> = set.rewritten().into_iter().collect();
  let source = &rewritten["routes/index/page.tsx"];
  assert!(source.contains("{items.map(__sfh.l((item) => __sfh.p(0, <Card label={item} />)))}"), "{source}");
  assert!(source.contains("{__sfh.p(1, <Card label=\"last\" />)}"), "{source}");
  assert!(source.contains("      <Plain />\n"), "an unkeyed placement is left as written: {source}");
  assert!(source.contains("{__sfh.p(3, <Tree node={tree} />)}"), "{source}");
  assert!(rewritten["src/ui/Tree.tsx"].contains("{node.kids.map(__sfh.l((kid) => __sfh.p(0, <Tree node={kid} />)))}"), "{}", rewritten["src/ui/Tree.tsx"]);
  assert!(!rewritten.contains_key("src/ui/Plain.tsx"), "a component that keys nothing is not rewritten");
}

#[test]
fn dangerously_set_inner_html_lowers_to_the_markup_it_names() {
  let files = [(
    "routes/index/page.tsx",
    r#"
export default function Page({ blip, note }: { blip: { body: string }; note: { __html: string } }) {
  return <article><div className="body" dangerouslySetInnerHTML={{ __html: blip.body }} /><aside dangerouslySetInnerHTML={note} /></article>;
}
"#,
  )];
  let set = set(&files, "routes/index/page.tsx#default");
  let component = &set.components[0].1;
  let Tmpl::Element { children, .. } = &component.render else { panic!("{:?}", component.render) };
  let Tmpl::Element { tag, attrs, children: inner } = &children[0] else { panic!("{:?}", children[0]) };
  assert_eq!(tag, "div");
  assert!(inner.is_empty(), "{inner:?}");
  assert_eq!(attrs[1], Entry::Field(RAW_ATTR.to_owned(), Expr::var("$props").field("blip").field("body")), "the object is written inline, so the IR holds what it named: {attrs:?}");
  let Tmpl::Element { attrs, .. } = &children[1] else { panic!("{:?}", children[1]) };
  assert_eq!(attrs[0], Entry::Field(RAW_ATTR.to_owned(), Expr::var("$props").field("note").field("__html")), "an object passed whole is read for its field: {attrs:?}");
  assert!(!format!("{component:?}").contains(UNLOWERED_ATTR), "the component lowers rather than falling to the browser: {component:?}");

  let map = |pairs: &[(&str, &str)]| {
    let mut out = snapfire_fsr_core::ValueMap::default();
    for (k, v) in pairs {
      out.insert((*k).to_owned(), snapfire_fsr_core::Value::str(*v));
    }
    out
  };
  let mut props = snapfire_fsr_core::ValueMap::default();
  props.insert("blip".to_owned(), snapfire_fsr_core::Value::Map(map(&[("body", "<p>a <b>markdown</b> blip</p>")])));
  props.insert("note".to_owned(), snapfire_fsr_core::Value::Map(map(&[("__html", "<i>&amp; a note</i>")])));
  let html = snapfire_fsr_ir::Interpreter::default().render(component, &props, &snapfire_fsr_ir::render::Components::new()).unwrap().html;
  assert_eq!(html, "<article><div class=\"body\"><p>a <b>markdown</b> blip</p></div><aside><i>&amp; a note</i></aside></article>", "the markdown a loader produced reaches the document as markup");
}

#[test]
fn a_helper_call_on_props_is_hoisted_and_one_on_state_is_not() {
  let files = [
    (
      "routes/cart/page.tsx",
      r#"
import { useState } from "react";
import { useStore } from "@snapfire/fsr/client/react";
import { money } from "@src/ui/money";
export default function Cart({ lines, total }: { lines: { price: number }[]; total: number }) {
  const [qty, setQty] = useState(1);
  const [tip] = useStore("cart/tip", 0);
  const grand = total + tip;
  return (
    <div>
      <b>{money(total)}</b>
      <i>{money(total * qty)}</i>
      <u>{money(grand)}</u>
      <ul>{lines.map((l) => <li key={l.price}>{money(l.price)}{money(l.price * qty)}</li>)}</ul>
      <button onClick={() => setQty(qty + 1)}>{money(1)}</button>
    </div>
  );
}
"#,
    ),
    ("src/ui/money.ts", "export function money(cents: number): string {\n  return `$${(cents / 100).toFixed(2)}`;\n}\n"),
  ];
  let set = set(&files, "routes/cart/page.tsx#default");
  let component = &set.components[0].1;
  assert_eq!(hoist_ids(component), vec![6, 10], "props only: money(l.price) and money(1); not qty, tip or grand; money(total) sits in the chunk whose markup the browser adopts: {component:?}");
  let mut chunks = Vec::new();
  chunk_ids(&component.render, &mut chunks);
  assert_eq!(chunks, vec![("b".to_owned(), 1)], "the one static element that does work; the others read state, hold a handler or only literals: {component:?}");
  assert_eq!(set.rewrites.len(), 1);
  let rewrite = &set.rewrites[0];
  assert_eq!(rewrite.module, "routes/cart/page.tsx#default");
  assert_eq!(rewrite.sites.len(), 2);
  assert_eq!(rewrite.chunks.len(), 1);
  assert!(rewrite.chunks[0].proven);
  assert_eq!(rewrite.loops.len(), 1, "the one .map callback holding a survivor");
  let (file, source) = set.rewritten().pop().unwrap();
  assert_eq!(file, "routes/cart/page.tsx");
  assert!(source.starts_with(hoist::IMPORT), "{source}");
  assert!(source.contains("{ const __sfh = __sfUseHoisted(\"routes/cart/page.tsx#default\"); "), "{source}");
  assert!(source.contains("{__sfh.c(1, (__sfHtml) => <b dangerouslySetInnerHTML={__sfHtml} />)}"), "a proven chunk is its hit alone: {source}");
  assert!(!source.contains("money(total)"), "the chunk's own read goes with its fallback: {source}");
  assert!(source.contains("<i>{money(total * qty)}</i>"), "a state read stays a call: {source}");
  assert!(source.contains("<ul>{lines.map(__sfh.l((l) => <li key={l.price}>{__sfh.r(6, () => (money(l.price)))}{money(l.price * qty)}</li>))}</ul>"), "{source}");
  assert!(source.contains("{__sfh.r(10, () => (money(1)))}</button>"), "{source}");
}

#[test]
fn a_component_a_browser_only_one_imports_keeps_its_chunks_jsx() {
  let card = "import { money } from \"@src/ui/money\";\nexport default function Card({ total }: { total: number }) {\n  return <div><b>{money(total)}</b></div>;\n}\n";
  let files = [
    ("routes/a/page.tsx", "import Card from \"@src/ui/Card\";\nexport default function A({ total }: { total: number }) {\n  return <Card total={total} />;\n}\n"),
    ("routes/b/page.tsx", "import Shell from \"@src/ui/Shell\";\nexport default function B({ total }: { total: number }) {\n  return <Shell total={total} />;\n}\n"),
    ("src/ui/Shell.tsx", "import Card from \"./Card\";\nexport default function Shell({ total }: { total: number }) {\n  const clock = new Clock();\n  return <section data-clock={String(clock)}><Card total={total} /></section>;\n}\n"),
    ("src/ui/Card.tsx", card),
    ("src/ui/money.ts", "export function money(cents: number): string {\n  return `$${(cents / 100).toFixed(2)}`;\n}\n"),
  ];
  let mut set = ComponentSet::new(&app(&files));
  set.lower("routes/a/page.tsx#default").unwrap();
  set.lower("routes/b/page.tsx#default").unwrap();
  assert!(set.browser_only.iter().any(|(m, _)| m == "src/ui/Shell.tsx#default"), "{:?}", set.browser_only);
  set.settle_proofs();
  let rewrite = set.rewrites.iter().find(|r| r.module == "src/ui/Card.tsx#default").expect("Card is rewritten");
  assert!(!rewrite.chunks.is_empty() && rewrite.chunks.iter().all(|c| !c.proven), "Shell mounts Card fresh in the browser: {:?}", rewrite.chunks);
  let (_, source) = set.rewritten().into_iter().find(|(f, _)| f == "src/ui/Card.tsx").unwrap();
  assert!(source.contains("money(total)"), "the chunk keeps the JSX it falls back to: {source}");
}

#[test]
fn a_handler_calls_an_action_and_the_call_lowers_to_act() {
  let files = [
    (
      "routes/index/page.tsx",
      r#"
import { Island } from "@snapfire/fsr/client/react";
import { Lot } from "@src/Lot";
export default function Page() {
  return <Island mode="server"><Lot size={10} /></Island>;
}
"#,
    ),
    (
      "src/Lot.tsx",
      r#"
import { useState } from "react";
import { action } from "@snapfire/fsr/client";
import { actions } from "@generated/client";
const step = action("desk.lot");
const save = action("desk.save");
function shout() {}
export function Lot({ size }: { size: number }) {
  const [open, setOpen] = useState(false);
  async function keep() {
    await actions.$root.keep({ size });
  }
  return (
    <div>
      <button onClick={() => void step({ by: 10 })}>+</button>
      <button onClick={() => { setOpen(!open); void save({ size, open: !open }); }}>save</button>
      <button onClick={() => void save()}>bare</button>
      <button onClick={() => void keep()}>generated</button>
      <button onClick={() => void shout()}>shout</button>
    </div>
  );
}
"#,
    ),
  ];
  let set = set(&files, "routes/index/page.tsx#default");
  let lot = &set.components.iter().find(|(m, _)| m == "src/Lot.tsx#Lot").unwrap().1;
  assert_eq!(lot.handlers.len(), 4, "{:?}", lot.handlers);
  assert_eq!(lot.handlers[0].body, vec![Stmt::Act { action: "desk.lot".to_owned(), input: Expr::Object(vec![Entry::Field("by".to_owned(), Expr::Lit(Lit::Float(10.0)))]) }, Stmt::Return(Expr::Object(Vec::new()))], "a handler that only calls an action sets no state and is still a handler");
  let flipped = Expr::Not(Box::new(Expr::var("open")));
  assert_eq!(
    lot.handlers[1].body,
    vec![
      Stmt::Act { action: "desk.save".to_owned(), input: Expr::Object(vec![Entry::Field("size".to_owned(), Expr::var("$props").field("size")), Entry::Field("open".to_owned(), flipped.clone())]) },
      Stmt::Return(Expr::Object(vec![Entry::Field("open".to_owned(), flipped)])),
    ],
    "the input reads props and state; the patch follows"
  );
  assert_eq!(lot.handlers[2].body, vec![Stmt::Act { action: "desk.save".to_owned(), input: Expr::Object(Vec::new()) }, Stmt::Return(Expr::Object(Vec::new()))], "no argument is an empty input");
  assert_eq!(
    lot.handlers[3].body,
    vec![Stmt::Act { action: "$root.keep".to_owned(), input: Expr::Object(vec![Entry::Field("size".to_owned(), Expr::var("$props").field("size"))]) }, Stmt::Return(Expr::Object(Vec::new()))],
    "the generated client's own call is an act too, its id the property path"
  );
  let Tmpl::Element { children, .. } = &lot.render else { panic!() };
  let Tmpl::Element { attrs, .. } = &children[4] else { panic!() };
  let unlowered = attrs.iter().find_map(|e| match e {
    Entry::Field(n, Expr::Lit(Lit::Str(why))) if n == UNLOWERED_ATTR => Some(why.clone()),
    _ => None,
  });
  assert!(unlowered.is_some_and(|why| why.contains("not a state setter, a write to the store, an action or a handler")), "a call to something else is still residue: {attrs:?}");
}

#[test]
fn a_branch_in_a_handler_conditions_each_key_it_sets() {
  let files = [
    (
      "routes/index/page.tsx",
      r#"
import { Island } from "@snapfire/fsr/client/react";
import { Gate } from "@src/Gate";
export default function Page() {
  return <Island mode="server"><Gate limit={3} /></Island>;
}
"#,
    ),
    (
      "src/Gate.tsx",
      r#"
import { useState } from "react";
import { action } from "@snapfire/fsr/client";
const save = action("desk.save");
export function Gate({ limit }: { limit: number }) {
  const [n, setN] = useState(0);
  const [open, setOpen] = useState(false);
  const [note, setNote] = useState("");
  function bump() {
    setN(n + 1);
  }
  return (
    <div>
      <button onClick={() => { if (n < limit) setN(n + 1); }}>a</button>
      <button onClick={() => { setOpen(true); if (n >= limit) { setOpen(false); setNote("full"); } else if (n === 0) { setNote("first"); } else { const rest = limit - n; setNote(String(rest)); } }}>b</button>
      <button onClick={() => { if (!open) return; setN(0); }}>c</button>
      <button onClick={() => { if (open) { void save({ n }); bump(); } setNote("saved"); }}>d</button>
      <button onClick={() => { if (n > 0) { const x = 1; setN(x); } else { const x = 2; setN(x); } }}>e</button>
      <button onClick={(e) => { if (open) e.preventDefault(); }}>f</button>
    </div>
  );
}
"#,
    ),
  ];
  let set = set(&files, "routes/index/page.tsx#default");
  let gate = &set.components.iter().find(|(m, _)| m == "src/Gate.tsx#Gate").unwrap().1;
  assert_eq!(gate.handlers.len(), 5, "{:?}", gate.handlers);
  let num = |v: f64| Expr::Lit(Lit::Float(v));
  let n = || Expr::var("n");
  let limit = || Expr::var("$props").field("limit");
  let cmp = |op: CompareOp, a: Expr, b: Expr| Expr::Compare(op, Box::new(a), Box::new(b));
  let not = |e: Expr| Expr::Not(Box::new(e));
  let and = |a: Expr, b: Expr| Expr::Logic(LogicOp::And, Box::new(a), Box::new(b));
  let pick = |c: Expr, a: Expr, b: Expr| Expr::Ternary(Box::new(c), Box::new(a), Box::new(b));
  let plus_one = || Expr::Arith(snapfire_fsr_ir::ArithOp::Add, Box::new(Expr::var("n")), Box::new(Expr::Lit(Lit::Float(1.0))));
  let field = |name: &str, value: Expr| Entry::Field(name.to_owned(), value);

  assert_eq!(
    gate.handlers[0].body,
    vec![Stmt::Return(Expr::Object(vec![field("n", pick(cmp(CompareOp::Lt, n(), limit()), plus_one(), n()))]))],
    "a key set only inside a branch keeps the state where the branch did not run"
  );

  let full = cmp(CompareOp::Ge, n(), limit());
  let first = cmp(CompareOp::Eq, n(), num(0.0));
  let rest_reach = and(not(full.clone()), not(first.clone()));
  let [Stmt::Let { name: rest, expr: rest_init }, Stmt::Return(Expr::Object(patch))] = gate.handlers[1].body.as_slice() else { panic!("{:?}", gate.handlers[1].body) };
  assert_eq!(rest, "rest$1", "a branch's const is hoisted under its own name");
  assert_eq!(*rest_init, pick(rest_reach.clone(), Expr::Arith(snapfire_fsr_ir::ArithOp::Sub, Box::new(limit()), Box::new(n())), Expr::Lit(Lit::Null)));
  assert_eq!(patch[0], field("open", pick(full.clone(), Expr::Lit(Lit::Bool(false)), Expr::Lit(Lit::Bool(true)))), "a set before the branch is the prior of the set inside it");
  let Entry::Field(name, Expr::Ternary(reach, _, prior)) = &patch[1] else { panic!("{:?}", patch[1]) };
  assert_eq!(name, "note");
  assert_eq!(**reach, rest_reach, "an else if chain nests its conditions");
  assert_eq!(**prior, pick(and(not(full.clone()), first), Expr::lit_str("first"), pick(full, Expr::lit_str("full"), Expr::var("note"))));
  assert_eq!(patch.len(), 2);

  assert_eq!(
    gate.handlers[2].body,
    vec![Stmt::Return(Expr::Object(vec![field("n", pick(not(not(Expr::var("open"))), num(0.0), n()))]))],
    "a bare return inside a branch leaves the rest reachable only when the branch did not run"
  );

  let open = || Expr::var("open");
  assert_eq!(
    gate.handlers[3].body,
    vec![
      Stmt::If { cond: open(), then: vec![Stmt::Act { action: "desk.save".to_owned(), input: Expr::Object(vec![field("n", n())]) }], r#else: Vec::new() },
      Stmt::Return(Expr::Object(vec![field("n", pick(open(), plus_one(), n())), field("note", Expr::lit_str("saved"))])),
    ],
    "an action inside a branch runs under it and an inlined handler's set is conditioned the same way"
  );

  let positive = cmp(CompareOp::Gt, n(), num(0.0));
  assert_eq!(
    gate.handlers[4].body,
    vec![
      Stmt::Let { name: "x$1".to_owned(), expr: pick(positive.clone(), num(1.0), Expr::Lit(Lit::Null)) },
      Stmt::Let { name: "x$2".to_owned(), expr: pick(not(positive.clone()), num(2.0), Expr::Lit(Lit::Null)) },
      Stmt::Return(Expr::Object(vec![field("n", pick(not(positive.clone()), Expr::var("x$2"), pick(positive, Expr::var("x$1"), n())))])),
    ],
    "the same name declared in both branches binds twice"
  );

  let Tmpl::Element { children, .. } = &gate.render else { panic!() };
  let Tmpl::Element { attrs, .. } = &children[5] else { panic!() };
  let unlowered = attrs.iter().find_map(|e| match e {
    Entry::Field(n, Expr::Lit(Lit::Str(why))) if n == UNLOWERED_ATTR => Some(why.clone()),
    _ => None,
  });
  assert!(unlowered.is_some_and(|why| why.contains("sets no state")), "a branch that sets nothing is still a handler that sets nothing: {attrs:?}");
}

#[test]
fn a_react_handler_writes_the_store_through_set_transaction_and_optimistic() {
  let files = [
    (
      "routes/index/page.tsx",
      r#"
import { Island } from "@snapfire/fsr/client/react";
import { Sizes } from "@src/Sizes";
import { Stepper } from "@src/Stepper";
export default function Page() {
  return <main><Island mode="server"><Sizes /></Island><Island mode="server"><Stepper /></Island></main>;
}
"#,
    ),
    (
      "src/keys.ts",
      r#"
import { key } from "@snapfire/fsr/client/store";
export const size = key<string>("order/size");
export const price = key<number>("order/price");
export const qty = key<number>("order/qty");
"#,
    ),
    (
      "src/Sizes.tsx",
      r#"
import { useStore } from "@snapfire/fsr/client/react";
import { set, transaction } from "@snapfire/fsr/client/store";
import { price, size } from "@src/keys";
export function Sizes() {
  const [chosen] = useStore(size, "250g");
  return <button className={chosen} onClick={() => transaction(() => { set(size, "1kg"); set(price, 40); })}>1kg</button>;
}
"#,
    ),
    (
      "src/Stepper.tsx",
      r#"
import { useStore } from "@snapfire/fsr/client/react";
import { optimistic } from "@snapfire/fsr/client/store";
import { action } from "@snapfire/fsr/client";
import { qty } from "@src/keys";
const setQty = action("order.setQty");
export function Stepper() {
  const [count] = useStore(qty, 1);
  return (
    <div>
      <button onClick={() => optimistic(qty, count + 1, () => setQty({ qty: count + 1 }))}>+</button>
      <button onClick={async () => { try { await optimistic(qty, count - 1, () => setQty({ qty: count - 1 })); } catch (e) { console.error(e); } }}>-</button>
    </div>
  );
}
"#,
    ),
  ];
  let set = set(&files, "routes/index/page.tsx#default");
  let sizes = &set.components.iter().find(|(m, _)| m == "src/Sizes.tsx#Sizes").unwrap().1;
  assert_eq!(sizes.stores, [("chosen".to_owned(), "order/size".to_owned()), ("$store1".to_owned(), "order/price".to_owned())], "the key the component does not read gets a binding of its own: {:?}", sizes.stores);
  assert_eq!(sizes.body.last(), Some(&Stmt::Let { name: "$store1".to_owned(), expr: Expr::Store("order/price".to_owned()) }));
  assert_eq!(sizes.handlers.len(), 1);
  assert_eq!(sizes.handlers[0].body, vec![Stmt::Return(Expr::Object(vec![Entry::Field("chosen".to_owned(), Expr::lit_str("1kg")), Entry::Field("$store1".to_owned(), Expr::Lit(Lit::Float(40.0)))]))]);
  let stepper = &set.components.iter().find(|(m, _)| m == "src/Stepper.tsx#Stepper").unwrap().1;
  assert_eq!(stepper.stores, [("count".to_owned(), "order/qty".to_owned())]);
  assert_eq!(stepper.handlers.len(), 2);
  let up = Expr::Arith(snapfire_fsr_ir::ArithOp::Add, Box::new(Expr::var("count")), Box::new(Expr::Lit(Lit::Float(1.0))));
  assert_eq!(stepper.handlers[0].body, vec![Stmt::Act { action: "order.setQty".to_owned(), input: Expr::Object(vec![Entry::Field("qty".to_owned(), up.clone())]) }, Stmt::Return(Expr::Object(vec![Entry::Field("count".to_owned(), up)]))]);
  let down = Expr::Arith(snapfire_fsr_ir::ArithOp::Sub, Box::new(Expr::var("count")), Box::new(Expr::Lit(Lit::Float(1.0))));
  let body = &stepper.handlers[1].body;
  let act = Stmt::Act { action: "order.setQty".to_owned(), input: Expr::Object(vec![Entry::Field("qty".to_owned(), down.clone())]) };
  let tried = body.iter().find_map(|stmt| match stmt {
    Stmt::Try { body, .. } => Some(body),
    _ => None,
  });
  assert!(tried.is_some_and(|tried| tried.contains(&act)), "the action is dispatched inside the try: {body:?}");
  assert!(matches!(body.last(), Some(Stmt::Return(Expr::Object(fields))) if matches!(fields.as_slice(), [Entry::Field(name, Expr::Var(_))] if name == "count")), "the patch sets the count the try set: {body:?}");
}

#[test]
fn handlers_lower_to_state_patches_and_an_island_takes_a_mode() {
  let files = [
    (
      "routes/index/page.tsx",
      r#"
import { Island, island } from "@snapfire/fsr/client/react";
import { Stepper } from "@src/Stepper";
const Lazy = island(Stepper, { when: "idle", mode: "server" });
export default function Page() {
  return <main><Island mode="server"><Stepper start={1} /></Island><Lazy start={2} /><Island mode="browser"><Stepper start={3} /></Island></main>;
}
"#,
    ),
    (
      "src/Stepper.tsx",
      r#"
import { useState } from "react";
export function Stepper({ start, max = 9 }: { start: number; max?: number }) {
  const [n, setN] = useState(start);
  const [open, setOpen] = useState(false);
  const room = max - n;
  function reset() {
    setN(start);
    setOpen(false);
  }
  return (
    <div key="stepper">
      <button onClick={() => setN(n + 1)} disabled={room === 0}>+</button>
      <button onClick={() => setN((prev) => prev - 1)}>-</button>
      <input value={String(n)} onChange={(e) => { e.preventDefault(); const next = Number(e.target.value); setN(next); }} />
      <button onClick={reset}>reset</button>
      <button onClick={() => void reset()}>reset too</button>
      <button onClick={() => alert(n)}>shout</button>
      <label onClick={() => setOpen(!open)}>{open ? "open" : "closed"}</label>
      <ul>{[1, 2].map((i) => <li key={i}>{i}</li>)}</ul>
    </div>
  );
}
"#,
    ),
  ];
  let set = set(&files, "routes/index/page.tsx#default");
  let page = &set.components.iter().find(|(m, _)| m == "routes/index/page.tsx#default").unwrap().1;
  let Tmpl::Element { children, .. } = &page.render else { panic!() };
  assert!(matches!(&children[0], Tmpl::Island { mode: Some(m), when: None, .. } if m == "server"), "{:?}", children[0]);
  assert!(matches!(&children[1], Tmpl::Island { mode: Some(m), when: Some(w), .. } if m == "server" && w == "idle"), "{:?}", children[1]);
  assert!(matches!(&children[2], Tmpl::Island { mode: None, .. }), "browser is the default and spells as none: {:?}", children[2]);

  let stepper = &set.components.iter().find(|(m, _)| m == "src/Stepper.tsx#Stepper").unwrap().1;
  assert_eq!(stepper.state, ["n", "open"]);
  assert_eq!(stepper.handlers.len(), 6, "{:?}", stepper.handlers.iter().map(|h| &h.event).collect::<Vec<_>>());
  let events: Vec<&str> = stepper.handlers.iter().map(|h| h.event.as_str()).collect();
  assert_eq!(events, ["click", "click", "change", "click", "click", "click"]);
  assert_eq!(stepper.handlers[0].body, vec![Stmt::Return(Expr::Object(vec![Entry::Field("n".to_owned(), Expr::Arith(snapfire_fsr_ir::ArithOp::Add, Box::new(Expr::var("n")), Box::new(Expr::Lit(Lit::Float(1.0)))))]))]);
  assert_eq!(stepper.handlers[1].body, vec![Stmt::Return(Expr::Object(vec![Entry::Field("n".to_owned(), Expr::Arith(snapfire_fsr_ir::ArithOp::Sub, Box::new(Expr::var("n")), Box::new(Expr::Lit(Lit::Float(1.0)))))]))], "a functional update reads the state as prev");
  assert_eq!(stepper.handlers[2].body, vec![Stmt::Let { name: "next".to_owned(), expr: Expr::Num(Box::new(Expr::var("$event").field("target").field("value"))) }, Stmt::Return(Expr::Object(vec![Entry::Field("n".to_owned(), Expr::var("next"))]))], "preventDefault is dropped, the event reads through $event");
  let reset = vec![Stmt::Return(Expr::Object(vec![Entry::Field("n".to_owned(), Expr::var("$props").field("start")), Entry::Field("open".to_owned(), Expr::Lit(Lit::Bool(false)))]))];
  assert_eq!(stepper.handlers[3].body, reset, "a named handler by name");
  assert_eq!(stepper.handlers[4].body, reset, "a named handler called");
  assert_eq!(stepper.handlers[5].body, vec![Stmt::Return(Expr::Object(vec![Entry::Field("open".to_owned(), Expr::Not(Box::new(Expr::var("open"))))]))]);

  let Tmpl::Element { attrs, children, .. } = &stepper.render else { panic!() };
  assert!(attrs.contains(&Entry::Field(KEY_ATTR.to_owned(), Expr::lit_str("stepper"))), "{attrs:?}");
  let on = |i: usize| -> Vec<(String, Expr)> {
    let Tmpl::Element { attrs, .. } = &children[i] else { panic!() };
    attrs.iter().filter_map(|e| match e {
      Entry::Field(n, v) if n.starts_with(HANDLER_ATTR) || n == UNLOWERED_ATTR => Some((n.clone(), v.clone())),
      _ => None,
    }).collect()
  };
  assert_eq!(on(0), vec![(format!("{HANDLER_ATTR}click"), Expr::Lit(Lit::Int(0)))]);
  assert_eq!(on(2), vec![(format!("{HANDLER_ATTR}change"), Expr::Lit(Lit::Int(2)))]);
  let shout = on(5);
  assert_eq!(shout.len(), 1);
  assert!(matches!(&shout[0], (n, Expr::Lit(Lit::Str(why))) if n == UNLOWERED_ATTR && why.contains("a call to `alert`")), "{shout:?}");
}

#[test]
fn a_static_subtree_is_a_chunk_and_a_handler_state_read_island_or_impure_component_breaks_it() {
  let files = [
    (
      "routes/index/page.tsx",
      r#"
import { useState } from "react";
import { Island } from "@snapfire/fsr/client/react";
import { Price } from "@src/Price";
import { Counter } from "@src/Counter";
import { Chart } from "@src/Chart";
export default function Page({ items, note }: { items: { id: number; price: number }[]; note: string }) {
  const [open, setOpen] = useState(false);
  return (
    <main>
      <h1>Catalog</h1>
      <ul className="list">
        {items.map((it) => (
          <li key={it.id} title={String(it.id)}>
            <Price cents={it.price} />
          </li>
        ))}
      </ul>
      <section>
        <p>{note}</p>
        <Counter start={1} />
      </section>
      <aside>
        <p>{note}</p>
        <Island when="visible"><Chart series={note} /></Island>
      </aside>
      <div>
        <button onClick={() => setOpen(!open)}>{note}</button>
      </div>
      <footer>{open ? note : ""}</footer>
    </main>
  );
}
"#,
    ),
    ("src/Price.tsx", "export function Price({ cents }: { cents: number }) {\n  return <b>{(cents / 100).toFixed(2)}</b>;\n}\n"),
    ("src/Counter.tsx", "import { useState } from \"react\";\nexport function Counter({ start }: { start: number }) {\n  const [n, setN] = useState(start);\n  return <button onClick={() => setN(n + 1)}>{n}</button>;\n}\n"),
    ("src/Chart.tsx", "export function Chart({ series }: { series: string }) {\n  return <svg><title>{series}</title></svg>;\n}\n"),
  ];
  let set = set(&files, "routes/index/page.tsx#default");
  assert_eq!(set.pure.get("src/Price.tsx#Price"), Some(&true));
  assert_eq!(set.pure.get("src/Counter.tsx#Counter"), Some(&false), "state and a handler");
  assert_eq!(set.pure.get("routes/index/page.tsx#default"), Some(&false));
  let page = &set.components.iter().find(|(m, _)| m == "routes/index/page.tsx#default").unwrap().1;
  let mut chunks = Vec::new();
  chunk_ids(&page.render, &mut chunks);
  let tags: Vec<&str> = chunks.iter().map(|(t, _)| t.as_str()).collect();
  assert_eq!(tags, ["ul", "p", "p"], "the list with its pure Price cards is one chunk; the paragraphs beside the impure Counter and the island are chunks of their own; the h1 is literal, the button is bound and the footer reads state: {chunks:?}");
  let rewrite = set.rewrites.iter().find(|r| r.module == "routes/index/page.tsx#default").unwrap();
  assert_eq!(rewrite.chunks.len(), 3);
  assert!(rewrite.loops.is_empty(), "the loop sits inside the chunk, whose fallback renders it as written");
  let (_, source) = set.rewritten().into_iter().find(|(f, _)| f == "routes/index/page.tsx").unwrap();
  assert!(source.contains("{__sfh.c(2, (__sfHtml) => <ul className=\"list\" dangerouslySetInnerHTML={__sfHtml} />)}\n"), "a chunk among JSX children is braced and a proven one is its hit alone: {source}");
  assert!(!source.contains("<Price"), "the list's cards go with the fallback: {source}");
  assert!(source.contains("{__sfh.c(3, (__sfHtml) => <p dangerouslySetInnerHTML={__sfHtml} />)}"), "{source}");
  assert!(source.contains("<button onClick={() => setOpen(!open)}>{note}</button>"), "a bound element is untouched: {source}");
  assert_eq!(source.matches("__sfh.c(").count(), 3, "{source}");
}

const PANEL: &str = "export function Panel({ n }: { n: number }) {\n  return <div className=\"panel\"><b>{n}</b><i>{(n / 100).toFixed(2)}</i></div>;\n}\n";
const PANEL_ON_PROPS: &str = "import { Panel } from \"@src/Panel\";\nexport default function Fixed({ n }: { n: number }) {\n  return <main><Panel n={n} /></main>;\n}\n";
const PANEL_ON_STATE: &str = "import { useStore } from \"@snapfire/fsr/client/react\";\nimport { Panel } from \"@src/Panel\";\nexport default function Live() {\n  const [n] = useStore(\"demo/n\", 0);\n  return <main><Panel n={n} /></main>;\n}\n";

fn panel(set: &ComponentSet) -> (Vec<(String, u32)>, Vec<u32>, usize) {
  let component = &set.components.iter().find(|(m, _)| m == "src/Panel.tsx#Panel").unwrap().1;
  let mut chunks = Vec::new();
  chunk_ids(&component.render, &mut chunks);
  let rewritten = set.rewrites.iter().find(|r| r.module == "src/Panel.tsx#Panel").map(|r| r.chunks.len() + r.sites.len()).unwrap_or(0);
  (chunks, hoist_ids(component), rewritten)
}

#[test]
fn a_component_placed_only_with_props_keeps_its_chunks() {
  let set = set(&[("routes/fixed/page.tsx", PANEL_ON_PROPS), ("src/Panel.tsx", PANEL)], "routes/fixed/page.tsx#default");
  let (chunks, _, rewritten) = panel(&set);
  assert_eq!(chunks.iter().map(|(t, _)| t.as_str()).collect::<Vec<_>>(), ["div"], "props are fixed for the placement's life: {chunks:?}");
  assert!(rewritten > 0);
}

#[test]
fn a_component_placed_with_state_hoists_nothing_its_props_reach() {
  let set = set(&[("routes/live/page.tsx", PANEL_ON_STATE), ("src/Panel.tsx", PANEL)], "routes/live/page.tsx#default");
  let (chunks, hoists, rewritten) = panel(&set);
  assert!(chunks.is_empty(), "the store moves `n` after hydration, so markup rendered from the seed goes stale: {chunks:?}");
  assert!(hoists.is_empty(), "{hoists:?}");
  assert_eq!(rewritten, 0, "the browser renders the panel from its props on every render");
}

#[test]
fn a_placement_with_state_unhoists_a_component_another_placement_lowered_first() {
  let mut set = ComponentSet::new(&app(&[("routes/fixed/page.tsx", PANEL_ON_PROPS), ("routes/live/page.tsx", PANEL_ON_STATE), ("src/Panel.tsx", PANEL)]));
  set.lower("routes/fixed/page.tsx#default").unwrap();
  set.lower("routes/live/page.tsx#default").unwrap();
  let (chunks, hoists, rewritten) = panel(&set);
  assert!(chunks.is_empty() && hoists.is_empty() && rewritten == 0, "one placement with state decides for every placement, since the component is lowered once: {chunks:?} {hoists:?} {rewritten}");
}

#[test]
fn hoists_stay_out_of_lambdas_nested_calls_and_client_components() {
  let files = [
    (
      "routes/index/page.tsx",
      r#"
import { money, wrap } from "@src/ui/money";
export const Page = ({ items, n }: { items: number[]; n: number }) => (
  <p title={items.map((i) => money(i)).join(", ")}>{wrap(money(n))}{items.length.toLocaleString()}</p>
);
"#,
    ),
    ("src/ui/money.ts", "export function money(cents: number): string {\n  return `$${(cents / 100).toFixed(2)}`;\n}\nexport function wrap(s: string): string {\n  return `[${s}]`;\n}\n"),
  ];
  let set = set(&files, "routes/index/page.tsx#Page");
  let component = &set.components[0].1;
  let mut hoisted = Vec::new();
  component.visit(&mut |e| {
    if let Expr::Hoist { id, .. } = e {
      hoisted.push(*id);
    }
  });
  assert_eq!(hoisted, Vec::<u32>::new(), "wrap(money(n)) and toLocaleString sit in the chunk whose markup the browser adopts; the lambda's money(i) was never one: {component:?}");
  let (_, source) = set.rewritten().pop().unwrap();
  assert!(source.contains("=> { const __sfh = __sfUseHoisted(\"routes/index/page.tsx#Page\"); return (("), "an expression body becomes a block: {source}");
  assert!(source.trim_end().ends_with(")); };"), "{source}");
  assert!(source.contains("__sfh.c(4, (__sfHtml) => <p title={items.map((i) => money(i)).join(\", \")} dangerouslySetInnerHTML={__sfHtml} />)"), "the whole paragraph is static, so it is a chunk whose hit keeps the attribute: {source}");
  assert!(!source.contains("__sfh.r("), "the reads inside the chunk go with its fallback: {source}");
}

#[test]
fn jsx_lowers_to_elements_attributes_and_the_three_idioms() {
  let page = r#"
import type { Props } from "../../generated/client";
export default function Page({ items, q = "" }: Props) {
  const n = items.length;
  return (
    <main className="page" aria-label={`Results for ${q}`}>
      <h1>{n} result{n === 1 ? "" : "s"}</h1>
      {n === 0 ? <p>Nothing</p> : null}
      {q && <b>{q}</b>}
      <ul>
        {items.map((it) => (
          <li key={String(it.id)} onClick={() => go(it)}>
            {it.name}
          </li>
        ))}
      </ul>
    </main>
  );
}
"#;
  let lowered = lower(&[("routes/index/page.tsx", page)], "routes/index/page.tsx#default").unwrap();
  assert_eq!(lowered.len(), 1);
  let component = &lowered[0].1;
  assert_eq!(component.body, vec![Stmt::Let { name: "n".to_owned(), expr: Expr::Length(Box::new(Expr::var("$props").field("items"))) }]);
  let Tmpl::Element { tag, attrs, children } = &component.render else { panic!("{:?}", component.render) };
  assert_eq!(tag, "main");
  assert_eq!(attrs[0], Entry::Field("class".to_owned(), Expr::lit_str("page")));
  assert!(matches!(&attrs[1], Entry::Field(name, Expr::Template(_)) if name == "aria-label"));
  assert!(matches!(&children[1], Tmpl::If { r#else: Some(e), .. } if matches!(**e, Tmpl::Fragment(ref f) if f.is_empty())));
  assert!(matches!(&children[2], Tmpl::If { r#else: Some(e), .. } if matches!(**e, Tmpl::Expr(_))), "`c && <x />` renders `c` when it is falsy");
  let Tmpl::Element { children: ul, .. } = &children[3] else { panic!() };
  let Tmpl::For { params, body, .. } = &ul[0] else { panic!("{:?}", ul[0]) };
  assert_eq!(params, &["it".to_owned()]);
  let Tmpl::Element { attrs, children, .. } = &**body else { panic!() };
  assert!(plain(attrs).is_empty(), "key and handlers are dropped: {attrs:?}");
  assert!(attrs.contains(&Entry::Field(hoist::BOUND_ATTR.to_owned(), Expr::Lit(Lit::Bool(true)))), "a handler leaves its mark: {attrs:?}");
  assert_eq!(children, &vec![Tmpl::Expr(Expr::var("it").field("name"))]);
}

#[test]
fn helpers_components_and_state_resolve_across_local_modules() {
  let files = [
    (
      "routes/product/page.tsx",
      r#"
import { useState } from "react";
import { money } from "../../src/ui/money";
import { Stars } from "../../src/ui/Stars";
import { actions } from "../../generated/client";
export default function Product({ product }: { product: { price: number; rating: number } }) {
  const [quantity, setQuantity] = useState(1);
  async function add() { await actions.cart.add({ quantity }); }
  return (
    <div style={{ background: product.color, marginTop: 4 }}>
      <Stars rating={product.rating} />
      <span>{money(product.price)}</span>
      <select value={quantity} onChange={(e) => setQuantity(Number(e.target.value))} />
      <button onClick={() => void add()} disabled={product.price === 0}>Add</button>
    </div>
  );
}
"#,
    ),
    ("src/ui/money.ts", "export function money(cents: number): string {\n  if (cents === 0) return \"free\";\n  const dollars = cents / 100;\n  return `$${dollars.toFixed(2)}`;\n}\n"),
    ("src/ui/Stars.tsx", "import Swal from \"sweetalert2\";\nexport function Stars({ rating }: { rating: number }) {\n  const full = Math.round(rating);\n  return <span>{\"★\".repeat(full)}</span>;\n}\nexport function toast() { Swal.fire(); }\n"),
  ];
  let lowered = lower(&files, "routes/product/page.tsx#default").unwrap();
  let modules: Vec<&str> = lowered.iter().map(|(m, _)| m.as_str()).collect();
  assert_eq!(modules, ["src/ui/Stars.tsx#Stars", "routes/product/page.tsx#default"], "the referenced component lands first");
  let page = &lowered[1].1;
  assert_eq!(page.body, vec![Stmt::Let { name: "quantity".to_owned(), expr: Expr::Lit(Lit::Float(1.0)) }], "useState(x) reads as x");
  let Tmpl::Element { attrs, children, .. } = &page.render else { panic!() };
  assert!(matches!(&attrs[0], Entry::Field(name, Expr::Object(entries)) if name == "style" && entries.len() == 2));
  assert!(matches!(&children[0], Tmpl::Component { module, props, .. } if module == "src/ui/Stars.tsx#Stars" && props.len() == 1));
  let Tmpl::Element { attrs: span_attrs, children: span, .. } = &children[1] else { panic!() };
  assert!(span_attrs.iter().any(|a| matches!(a, Entry::Field(name, _) if name == hoist::CHUNK_ATTR)), "the span reads props only, so the server renders it: {span_attrs:?}");
  let Tmpl::Expr(expr) = &span[0] else { panic!("{:?}", span[0]) };
  assert!(matches!(expr, Expr::Apply { f, args } if matches!(**f, Expr::Lambda { .. }) && args.len() == 1), "the helper is inlined and, carried by the chunk's markup, no longer a hoist: {expr:?}");
  let Tmpl::Element { attrs, .. } = &children[3] else { panic!() };
  assert_eq!(plain(attrs).len(), 1, "onClick is dropped, disabled stays: {attrs:?}");
}

#[test]
fn children_spreads_and_hooks() {
  let files = [
    (
      "routes/index/page.tsx",
      r#"
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { Page } from "../../src/ui/Page";
export default function Catalog({ products, cartCount, q }: { products: { name: string }[]; cartCount: number; q: string }) {
  const [open, setOpen] = useState(false);
  const header = { cartCount, q };
  const count = useMemo(() => products.length, [products]);
  const box = useRef(null);
  const toggle = useCallback(() => setOpen(!open), [open]);
  const focus = () => box.current?.focus();
  useEffect(() => { document.title = q; }, [q]);
  return (
    <Page {...header} className="catalog">
      <h1 {...(open ? { hidden: true } : {})} className="title" ref={box}>{count}</h1>
      {products.map((p) => <p key={p.name}>{p.name}</p>)}
    </Page>
  );
}
"#,
    ),
    (
      "src/ui/Page.tsx",
      r#"
import { Header } from "./Header";
export function Page({ className, children, ...rest }: { className: string; children: React.ReactNode; cartCount: number; q: string }) {
  return <><Header {...rest} /><main className={`page ${className}`}>{children}</main></>;
}
"#,
    ),
    ("src/ui/Header.tsx", "export function Header(props: { cartCount: number; q: string }) {\n  return <header>{props.q}{props.children}</header>;\n}\n"),
  ];
  let lowered = lower(&files, "routes/index/page.tsx#default").unwrap();
  let layout = &lowered.iter().find(|(m, _)| m == "src/ui/Page.tsx#Page").unwrap().1;
  let Tmpl::Fragment(parts) = &layout.render else { panic!() };
  let rest = Entry::Spread(Expr::Builtin { name: Builtin::Omit, args: vec![Expr::var("$props"), Expr::lit_str("className"), Expr::lit_str("children")] });
  assert!(matches!(&parts[0], Tmpl::Component { props, .. } if props[0] == rest), "the rest is the props without what was named: {:?}", parts[0]);
  let mut files = files;
  files[1].1 = r#"
import { Header } from "./Header";
export function Page(props: { className: string; children: React.ReactNode; cartCount: number; q: string }) {
  return <><Header cartCount={props.cartCount} q={props.q} /><main className={`page ${props.className}`}>{props.children}</main></>;
}
"#;
  let lowered = lower(&files, "routes/index/page.tsx#default").unwrap();
  let page = &lowered.iter().find(|(m, _)| m == "routes/index/page.tsx#default").unwrap().1;
  let names: Vec<&str> = page.body.iter().map(|s| match s { Stmt::Let { name, .. } => name.as_str(), _ => "" }).collect();
  assert_eq!(names, ["open", "header", "count", "box"], "useCallback and the arrow are handlers, useEffect is dropped");
  assert_eq!(page.body[2], Stmt::Let { name: "count".to_owned(), expr: Expr::Length(Box::new(Expr::var("$props").field("products"))) });
  assert_eq!(page.body[3], Stmt::Let { name: "box".to_owned(), expr: Expr::Object(vec![Entry::Field("current".to_owned(), Expr::Lit(Lit::Null))]) });
  let Tmpl::Component { module, props, children, .. } = &page.render else { panic!("{:?}", page.render) };
  assert_eq!(module, "src/ui/Page.tsx#Page");
  assert!(matches!(&props[0], Entry::Spread(Expr::Var(v)) if v == "header"));
  assert_eq!(props[1], Entry::Field("className".to_owned(), Expr::lit_str("catalog")));
  let Tmpl::Element { attrs, children: h1, .. } = &children[0] else { panic!("{:?}", children[0]) };
  assert!(matches!(&attrs[0], Entry::Spread(Expr::Ternary(..))));
  assert_eq!(attrs[1], Entry::Field("class".to_owned(), Expr::lit_str("title")));
  assert_eq!(h1, &vec![Tmpl::Expr(Expr::var("count"))]);
  assert!(matches!(&children[1], Tmpl::For { .. }));
  let layout = &lowered.iter().find(|(m, _)| m == "src/ui/Page.tsx#Page").unwrap().1;
  let Tmpl::Fragment(parts) = &layout.render else { panic!() };
  let Tmpl::Element { children: main, .. } = &parts[1] else { panic!() };
  assert_eq!(main, &vec![Tmpl::Slot("content".to_owned())]);
  let header = &lowered.iter().find(|(m, _)| m == "src/ui/Header.tsx#Header").unwrap().1;
  let Tmpl::Element { children, .. } = &header.render else { panic!() };
  assert_eq!(children[1], Tmpl::Slot("content".to_owned()), "`props.children` is the slot when props are bound whole");
}

#[test]
fn an_app_s_own_use_id_is_its_hook_not_react_s() {
  let hook = "export function useId(prefix: string) {\n  return `${prefix}-field`;\n}\n";
  let page = "import { useId } from \"../../src/useId\";\nexport default function Page() {\n  const id = useId(\"name\");\n  return <label htmlFor={id}>Name</label>;\n}\n";
  let lowered = lower(&[("src/useId.ts", hook), ("routes/index/page.tsx", page)], "routes/index/page.tsx#default").unwrap();
  assert!(lowered.iter().any(|(m, _)| m == "routes/index/page.tsx#default"));
}

#[test]
fn a_module_level_iife_is_named_with_a_hint() {
  let page = "const env = (() => {\n  return \"prod\";\n})();\nexport default function Page() {\n  return <p>{env}</p>;\n}\n";
  let err = lower(&[("routes/index/page.tsx", page)], "routes/index/page.tsx#default").unwrap_err();
  let LowerError::Residue(residue) = err else { panic!("{err}") };
  assert_eq!(residue.message, "a function called where it is written");
  assert!(residue.hint.as_deref().is_some_and(|h| h.contains("named function")), "{residue:?}");
}

#[test]
fn a_default_export_of_a_package_component_is_residue_naming_the_package() {
  let layout = "import { BaseLayout } from \"shell-lib/layouts\";\nexport default BaseLayout;\n";
  let err = lower(&[("routes/layout.tsx", layout)], "routes/layout.tsx#default").unwrap_err();
  let LowerError::Residue(residue) = err else { panic!("{err}") };
  assert_eq!(residue.message, "`BaseLayout` comes from `shell-lib/layouts`, which the build cannot follow");
}

#[test]
fn residue_names_the_line_and_the_construct() {
  let page = "export default function Page() {\n  const params = new WeakMap();\n  return <a href={String(params)}>x</a>;\n}\n";
  let err = lower(&[("routes/index/page.tsx", page)], "routes/index/page.tsx#default").unwrap_err();
  assert_eq!(err.to_string(), "routes/index/page.tsx:2:18: `new WeakMap`");
  let page = "import { Chart } from \"chart-lib\";\nexport default function Page() {\n  return <Chart />;\n}\n";
  let err = lower(&[("routes/index/page.tsx", page)], "routes/index/page.tsx#default").unwrap_err();
  assert!(err.to_string().contains("`Chart` comes from `chart-lib`, which the build cannot follow"), "{err}");
  let page = "import { lazy } from \"react\";\nconst Chart = lazy(() => import(\"chart-lib\"));\nexport default function Page() {\n  return <Chart />;\n}\n";
  let err = lower(&[("routes/index/page.tsx", page)], "routes/index/page.tsx#default").unwrap_err();
  assert!(err.to_string().contains("`Chart` comes from `chart-lib`, which the build cannot follow"), "a lazy import names its specifier: {err}");
}

#[test]
fn jsx_text_follows_the_whitespace_rule_and_decodes_entities() {
  assert_eq!(jsx_text("\n    Hello\n    world\n  "), "Hello world");
  assert_eq!(jsx_text("a  b"), "a  b");
  assert_eq!(jsx_text("&minus; &amp; &#8230; &#x2192; &bogus;"), "\u{2212} & \u{2026} \u{2192} &bogus;");
}
#[test]
fn jsx_text_and_string_attributes_decode_every_html_entity_once() {
  assert_eq!(jsx_text("FSR&rsquo;s &hellip; &mdash; &eacute; &#8217;"), "FSR\u{2019}s \u{2026} \u{2014} \u{e9} \u{2019}");
  assert_eq!(jsx_text("&amp;lt;"), "&lt;");
  let page = r#"
export default function Page({ id }: { id: number }) {
  return <p data-id={id} title="Tom &amp; Jerry&rsquo;s">FSR&rsquo;s renderer &hellip;</p>;
}
"#;
  let lowered = lower(&[("routes/index/page.tsx", page)], "routes/index/page.tsx#default").unwrap();
  let Tmpl::Element { attrs, children, .. } = &lowered[0].1.render else { panic!("{:?}", lowered[0].1.render) };
  assert!(attrs.contains(&Entry::Field("title".to_owned(), Expr::lit_str("Tom & Jerry\u{2019}s"))), "{attrs:?}");
  assert_eq!(children, &vec![Tmpl::Text("FSR\u{2019}s renderer \u{2026}".to_owned())]);
}
#[test]
fn a_rest_in_a_destructuring_is_the_object_without_the_named_keys() {
  let page = r#"
export default function Page({ title, kind = "note", ...rest }: { title: string; kind?: string; id: number; hidden: boolean }) {
  const { id, ...attrs } = rest;
  return <section data-id={id} {...attrs}>{title}: {kind}</section>;
}
"#;
  let lowered = lower(&[("routes/index/page.tsx", page)], "routes/index/page.tsx#default").unwrap();
  let component = &lowered[0].1;
  let props = Expr::var("$props");
  assert_eq!(component.body, vec![Stmt::Let { name: "$d1".to_owned(), expr: Expr::Builtin { name: Builtin::Omit, args: vec![props.clone(), Expr::lit_str("title"), Expr::lit_str("kind")] } }]);
  let Tmpl::Element { attrs, .. } = &component.render else { panic!("{:?}", component.render) };
  assert_eq!(attrs[0], Entry::Field("data-id".to_owned(), Expr::var("$d1").field("id")));
  assert_eq!(attrs[1], Entry::Spread(Expr::Builtin { name: Builtin::Omit, args: vec![Expr::var("$d1"), Expr::lit_str("id")] }));
}

#[test]
fn a_member_expression_tag_names_an_export_of_a_namespace_import() {
  let files = [
    ("routes/index/page.tsx", "import * as Ui from \"../../src/ui\";\nimport * as React from \"react\";\nexport default function Page() {\n  return <React.Fragment><Ui.Card title=\"a\" /></React.Fragment>;\n}\n"),
    ("src/ui/index.tsx", "export function Card({ title }: { title: string }) {\n  return <div className=\"card\">{title}</div>;\n}\n"),
  ];
  let lowered = lower(&files, "routes/index/page.tsx#default").unwrap();
  let modules: Vec<&str> = lowered.iter().map(|(m, _)| m.as_str()).collect();
  assert_eq!(modules, ["src/ui/index.tsx#Card", "routes/index/page.tsx#default"]);
  let Tmpl::Fragment(children) = &lowered[1].1.render else { panic!("{:?}", lowered[1].1.render) };
  assert!(matches!(&children[0], Tmpl::Component { module, .. } if module == "src/ui/index.tsx#Card"));

  let page = "import { ui } from \"../../src/ui\";\nexport default function Page() {\n  return <ui.Card title=\"a\" />;\n}\n";
  let err = lower(&[("routes/index/page.tsx", page), ("src/ui/index.tsx", "export const ui = {};\n")], "routes/index/page.tsx#default").unwrap_err();
  assert!(err.to_string().contains("`ui` is not a namespace import"), "{err}");
}

#[test]
fn the_island_wrapper_and_the_island_alias_place_a_component_as_an_island() {
  let files = [
    (
      "routes/order/page.tsx",
      r#"
import { Island, island } from "@snapfire/fsr/client/react";
import { Help } from "../../src/ui/Help";
import { Chart } from "../../src/ui/Chart";
const LazyChart = island(Chart, { when: "idle" });
export default function Order({ id }: { id: number }) {
  return (
    <main>
      <Island when="visible">
        <Help id={id} />
      </Island>
      <LazyChart series={[id]} />
      <Island><Help id={0} /></Island>
    </main>
  );
}
"#,
    ),
    ("src/ui/Help.tsx", "export function Help({ id }: { id: number }) {\n  return <p>help {id}</p>;\n}\n"),
    ("src/ui/Chart.tsx", "export function Chart({ series }: { series: number[] }) {\n  return <svg>{series.length}</svg>;\n}\n"),
  ];
  let lowered = lower(&files, "routes/order/page.tsx#default").unwrap();
  let page = &lowered.iter().find(|(m, _)| m == "routes/order/page.tsx#default").unwrap().1;
  let Tmpl::Element { children, .. } = &page.render else { panic!("{:?}", page.render) };
  assert!(matches!(&children[0], Tmpl::Island { module, props, when, .. } if module == "src/ui/Help.tsx#Help" && props.len() == 1 && when.as_deref() == Some("visible")), "{:?}", children[0]);
  assert!(matches!(&children[1], Tmpl::Island { module, props, when, .. } if module == "src/ui/Chart.tsx#Chart" && matches!(&props[0], Entry::Field(name, _) if name == "series") && when.as_deref() == Some("idle")), "{:?}", children[1]);
  assert!(matches!(&children[2], Tmpl::Island { when: None, .. }), "no timing means the registry's: {:?}", children[2]);
  assert!(lowered.iter().any(|(m, _)| m == "src/ui/Help.tsx#Help") && lowered.iter().any(|(m, _)| m == "src/ui/Chart.tsx#Chart"), "an island's component is lowered like any other");
}

#[test]
fn a_layout_places_a_named_slot_by_element_and_by_prop() {
  let files = [
    (
      "routes/layout.tsx",
      "import { Slot } from \"@snapfire/fsr/client/react\";\nexport default function Layout({ children, feed, title }: { children: unknown; feed: unknown; title: string }) {\n  return <div><h1>{title}</h1>{children}<aside>{feed}</aside><Slot name=\"modal\" /></div>;\n}\n",
    ),
  ];
  let mut set = ComponentSet::new(&app(&files));
  set.layouts.push("routes/layout.tsx#default".to_owned());
  set.slots.push(("routes/layout.tsx#default".to_owned(), vec!["feed".to_owned()]));
  set.lower("routes/layout.tsx#default").unwrap();
  let layout = &set.components[0].1;
  let Tmpl::Element { children, .. } = &layout.render else { panic!("{:?}", layout.render) };
  assert_eq!(children[1], Tmpl::Element { tag: "sf-s".to_owned(), attrs: Vec::new(), children: vec![Tmpl::Slot("content".to_owned())] });
  let Tmpl::Element { children: aside, .. } = &children[2] else { panic!("{:?}", children[2]) };
  assert_eq!(aside[0], Tmpl::Element { tag: "sf-s".to_owned(), attrs: vec![Entry::Field("data-sf-name".to_owned(), Expr::lit_str("feed"))], children: vec![Tmpl::Slot("feed".to_owned())] }, "a prop named after a slots/ directory is that slot");
  assert_eq!(children[3], Tmpl::Element { tag: "sf-s".to_owned(), attrs: vec![Entry::Field("data-sf-name".to_owned(), Expr::lit_str("modal"))], children: vec![Tmpl::Slot("modal".to_owned())] });
}

#[test]
fn a_slot_outside_a_layout_is_residue() {
  let page = [("routes/a/page.tsx", "import { Slot } from \"@snapfire/fsr/client/react\";\nexport default function A() {\n  return <Slot name=\"x\" />;\n}\n")];
  let err = lower(&page, "routes/a/page.tsx#default").unwrap_err().to_string();
  assert!(err.contains("outside a layout"), "{err}");
}

#[test]
fn a_slot_placement_carries_its_fallback_by_element_and_by_prop() {
  let files = [(
    "routes/layout.tsx",
    "import { Slot } from \"@snapfire/fsr/client/react\";\nexport default function L({ children, feed }: { children: unknown; feed: unknown }) {\n  return <div>{children}{feed ?? <p>no feed</p>}<Slot name=\"modal\"><p>closed</p></Slot></div>;\n}\n",
  )];
  let mut set = ComponentSet::new(&app(&files));
  set.layouts.push("routes/layout.tsx#default".to_owned());
  set.slots.push(("routes/layout.tsx#default".to_owned(), vec!["feed".to_owned()]));
  set.lower("routes/layout.tsx#default").unwrap();
  let Tmpl::Element { children, .. } = &set.components[0].1.render else { panic!() };
  for (child, name, text) in [(&children[1], "feed", "no feed"), (&children[2], "modal", "closed")] {
    let Tmpl::Element { tag, attrs, children: inner } = child else { panic!("{child:?}") };
    assert_eq!(tag, "sf-s");
    assert_eq!(attrs, &vec![Entry::Field("data-sf-name".to_owned(), Expr::lit_str(name))]);
    let Tmpl::If { cond, then, r#else } = &inner[0] else { panic!("{:?}", inner[0]) };
    assert!(matches!(cond, Expr::Builtin { name: Builtin::Includes, args } if args[1] == Expr::lit_str(name)), "{cond:?}");
    assert_eq!(**then, Tmpl::Slot(name.to_owned()));
    let Some(fallback) = r#else else { panic!() };
    let Tmpl::Fragment(items) = &**fallback else { panic!("{fallback:?}") };
    let Tmpl::Element { children: p, .. } = &items[0] else { panic!("{:?}", items[0]) };
    assert_eq!(p, &vec![Tmpl::Text(text.to_owned())]);
  }
}

#[test]
fn a_store_read_lowers_to_the_key_with_its_initial_value() {
  let files = [
    (
      "routes/index/page.tsx",
      "import { useStore } from \"@snapfire/fsr/client/react\";\nimport { cartCount } from \"../../src/store\";\nexport default function P() {\n  const [items, setItems] = useStore(cartCount, 0);\n  const [name] = useStore(\"user/name\", \"guest\");\n  return <p onClick={() => setItems(items + 1)}>{name}{items}</p>;\n}\n",
    ),
    ("src/store.ts", "import { key } from \"@snapfire/fsr/client/store\";\nexport const cartCount = key<number>(\"cart/count\");\n"),
  ];
  let lowered = lower(&files, "routes/index/page.tsx#default").unwrap();
  let component = &lowered[0].1;
  assert_eq!(
    component.body,
    vec![
      Stmt::Let { name: "items".to_owned(), expr: Expr::Coalesce(Box::new(Expr::Store("cart/count".to_owned())), Box::new(Expr::Lit(Lit::Float(0.0)))) },
      Stmt::Let { name: "name".to_owned(), expr: Expr::Coalesce(Box::new(Expr::Store("user/name".to_owned())), Box::new(Expr::lit_str("guest"))) },
    ],
    "a key() through an import and a literal both lower to the key"
  );
  assert_eq!(component.stores, [("items".to_owned(), "cart/count".to_owned()), ("name".to_owned(), "user/name".to_owned())], "each binding is recorded with the key it reads");
  let Tmpl::Element { attrs, .. } = &component.render else { panic!("{:?}", component.render) };
  assert!(plain(attrs).is_empty(), "the setter is a handler: {attrs:?}");
}

#[test]
fn a_store_key_the_build_cannot_read_is_residue() {
  let files = [(
    "routes/index/page.tsx",
    "import { useStore } from \"@snapfire/fsr/client/react\";\nexport default function P({ id }: { id: string }) {\n  const [n] = useStore(id, 0);\n  return <p>{n}</p>;\n}\n",
  )];
  let err = lower(&files, "routes/index/page.tsx#default").unwrap_err().to_string();
  assert!(err.contains("`useStore` key"), "{err}");
}

#[test]
fn use_locale_lowers_to_the_locale_read() {
  let files = [(
    "routes/help/page.tsx",
    "import { useLocale } from \"@snapfire/fsr/client/react\";\nexport default function Help() {\n  const locale = useLocale();\n  return <p lang={locale}>{locale === \"fr_FR\" ? \"Bonjour\" : \"Hello\"}</p>;\n}\n",
  )];
  let lowered = lower(&files, "routes/help/page.tsx#default").unwrap();
  let component = &lowered[0].1;
  assert_eq!(component.body, vec![Stmt::Let { name: "locale".to_owned(), expr: Expr::Locale }]);
  let Tmpl::Element { attrs, .. } = &component.render else { panic!("{:?}", component.render) };
  assert_eq!(plain(attrs), [Entry::Field("lang".to_owned(), Expr::var("locale"))]);
}

#[test]
fn a_link_lowers_to_an_anchor_the_navigator_reads() {
  let page = [("routes/a/page.tsx", "import { Link } from \"@snapfire/fsr/client/react\";\nexport default function A({ id }: { id: number }) {\n  return <p><Link href={`/photo/${id}`} className=\"x\" full>full</Link><Link href=\"/photo/1\" into=\"modal\" prefetch=\"none\">quick</Link></p>;\n}\n")];
  let lowered = lower(&page, "routes/a/page.tsx#default").unwrap();
  let Tmpl::Element { children, .. } = &lowered[0].1.render else { panic!() };
  let Tmpl::Element { tag, attrs, children: text } = &children[0] else { panic!("{:?}", children[0]) };
  assert_eq!(tag, "a");
  assert!(matches!(&attrs[0], Entry::Field(name, _) if name == "href"));
  assert_eq!(attrs[1], Entry::Field("class".to_owned(), Expr::lit_str("x")));
  assert_eq!(attrs[2], Entry::Field("data-sf-full".to_owned(), Expr::Lit(Lit::Bool(true))));
  assert_eq!(text, &vec![Tmpl::Text("full".to_owned())]);
  let Tmpl::Element { attrs, .. } = &children[1] else { panic!("{:?}", children[1]) };
  assert_eq!(attrs[1], Entry::Field("data-sf-into".to_owned(), Expr::lit_str("modal")));
  assert_eq!(attrs[2], Entry::Field("data-sf-prefetch".to_owned(), Expr::lit_str("none")));
}

#[test]
fn a_link_writes_its_keep_out_since_a_false_attribute_is_dropped() {
  let page = [("routes/a/page.tsx", "import { Link } from \"@snapfire/fsr/client/react\";\nexport default function A({ on }: { on: boolean }) {\n  return <p><Link href=\"?at=1\" keep={false}>a</Link><Link href=\"?at=2\" keep>b</Link><Link href=\"?at=3\" keep={on}>c</Link></p>;\n}\n")];
  let lowered = lower(&page, "routes/a/page.tsx#default").unwrap();
  let Tmpl::Element { children, .. } = &lowered[0].1.render else { panic!() };
  let keep = |i: usize| match &children[i] {
    Tmpl::Element { attrs, .. } => attrs[1].clone(),
    other => panic!("{other:?}"),
  };
  assert_eq!(keep(0), Entry::Field("data-sf-keep".to_owned(), Expr::lit_str("false")));
  assert_eq!(keep(1), Entry::Field("data-sf-keep".to_owned(), Expr::lit_str("true")));
  assert!(matches!(keep(2), Entry::Field(name, Expr::Ternary(..)) if name == "data-sf-keep"), "a computed keep is spelled when it renders: {:?}", keep(2));
}

#[test]
fn a_link_carries_the_rule_that_marks_it_current_and_the_mark_itself() {
  let page = [("routes/a/page.tsx", "import { Link } from \"@snapfire/fsr/client/react\";\nexport default function A() {\n  return <p><Link href=\"/billing\">a</Link><Link href=\"/billing\" match=\"prefix\">b</Link><Link href=\"/billing\" match=\"none\">c</Link><Link href=\"/billing\" aria-current=\"page\">d</Link></p>;\n}\n")];
  let lowered = lower(&page, "routes/a/page.tsx#default").unwrap();
  let Tmpl::Element { children, .. } = &lowered[0].1.render else { panic!() };
  let attrs = |i: usize| match &children[i] {
    Tmpl::Element { attrs, .. } => attrs.clone(),
    other => panic!("{other:?}"),
  };
  let exact = attrs(0);
  assert_eq!(exact[1], Entry::Field("data-sf-link".to_owned(), Expr::lit_str("exact")));
  let hit = Expr::Compare(CompareOp::Eq, Box::new(Expr::Path), Box::new(Expr::lit_str("/billing")));
  assert_eq!(exact[2], Entry::Field("aria-current".to_owned(), Expr::Ternary(Box::new(hit), Box::new(Expr::lit_str("page")), Box::new(Expr::Lit(Lit::Null)))));
  let prefix = attrs(1);
  assert_eq!(prefix[1], Entry::Field("data-sf-link".to_owned(), Expr::lit_str("prefix")));
  assert!(matches!(&prefix[2], Entry::Field(name, Expr::Ternary(cond, hit, _)) if name == "aria-current" && matches!(**cond, Expr::Logic(LogicOp::Or, ..)) && **hit == Expr::lit_str("true")), "{:?}", prefix[2]);
  assert_eq!(attrs(2).len(), 1, "`match=\"none\"` leaves the anchor alone: {:?}", attrs(2));
  let written = attrs(3);
  assert_eq!(written.len(), 2, "an `aria-current` the author wrote stands: {written:?}");
  assert_eq!(written[1], Entry::Field("aria-current".to_owned(), Expr::lit_str("page")));
}

#[test]
fn a_link_marked_by_the_document_compares_the_documents_path_and_says_so() {
  let page = [("routes/a/page.tsx", "import { Link } from \"@snapfire/fsr/client/react\";\nexport default function A() {\n  return <p><Link href=\"/agents\" match=\"prefix\" current=\"document\">a</Link><Link href=\"/help\" current=\"url\">b</Link><Link href=\"/help\" current=\"page\">c</Link></p>;\n}\n")];
  let err = lower(&page, "routes/a/page.tsx#default").unwrap_err().to_string();
  assert!(err.contains("`current` is \"url\" or \"document\""), "{err}");
  let page = [("routes/a/page.tsx", "import { Link } from \"@snapfire/fsr/client/react\";\nexport default function A() {\n  return <p><Link href=\"/agents\" match=\"prefix\" current=\"document\">a</Link><Link href=\"/help\" current=\"url\">b</Link></p>;\n}\n")];
  let lowered = lower(&page, "routes/a/page.tsx#default").unwrap();
  let Tmpl::Element { children, .. } = &lowered[0].1.render else { panic!() };
  let Tmpl::Element { attrs, .. } = &children[0] else { panic!("{:?}", children[0]) };
  assert_eq!(attrs[1], Entry::Field("data-sf-current".to_owned(), Expr::lit_str("document")));
  assert_eq!(attrs[2], Entry::Field("data-sf-link".to_owned(), Expr::lit_str("prefix")));
  let Entry::Field(name, Expr::Ternary(cond, ..)) = &attrs[3] else { panic!("{:?}", attrs[3]) };
  assert_eq!(name, "aria-current");
  let mut by_document = false;
  cond.visit(&mut |e| by_document |= *e == Expr::Document);
  assert!(by_document, "{cond:?}");
  let Tmpl::Element { attrs, .. } = &children[1] else { panic!("{:?}", children[1]) };
  assert_eq!(attrs[1], Entry::Field("data-sf-link".to_owned(), Expr::lit_str("exact")), "`url` is the default and writes nothing of its own: {attrs:?}");
  let Entry::Field(_, Expr::Ternary(cond, ..)) = &attrs[2] else { panic!("{:?}", attrs[2]) };
  assert_eq!(**cond, Expr::Compare(CompareOp::Eq, Box::new(Expr::Path), Box::new(Expr::lit_str("/help"))));
}

#[test]
fn a_link_with_a_computed_match_is_residue() {
  let page = [("routes/a/page.tsx", "import { Link } from \"@snapfire/fsr/client/react\";\nexport default function A({ on }: { on: boolean }) {\n  return <Link href=\"/x\" match={on ? \"prefix\" : \"none\"}>a</Link>;\n}\n")];
  let err = lower(&page, "routes/a/page.tsx#default").unwrap_err().to_string();
  assert!(err.contains("written out"), "{err}");
}

#[test]
fn an_early_return_lowers_to_the_conditional_a_ternary_lowers_to() {
  let early = "export default function P({ live, steps }: { live: boolean; steps: number }) {\n  const count = steps + 1;\n  if (steps === 0) return null;\n  if (live) {\n    return <button>open</button>;\n  }\n  return <p>{count}</p>;\n}\n";
  let ternary = "export default function P({ live, steps }: { live: boolean; steps: number }) {\n  const count = steps + 1;\n  return steps === 0 ? null : live ? <button>open</button> : <p>{count}</p>;\n}\n";
  let early = lower(&[("routes/index/page.tsx", early)], "routes/index/page.tsx#default").unwrap();
  let ternary = lower(&[("routes/index/page.tsx", ternary)], "routes/index/page.tsx#default").unwrap();
  assert_eq!(early[0].1.body, ternary[0].1.body);
  assert_eq!(early[0].1.render, ternary[0].1.render);
  assert!(matches!(&early[0].1.render, Tmpl::If { then, r#else: Some(_), .. } if **then == Tmpl::Fragment(Vec::new())), "{:?}", early[0].1.render);
}

#[test]
fn a_hook_after_an_early_return_is_residue_and_returns_in_branches_lower() {
  let after = "import { useState } from \"react\";\nexport default function P({ live }: { live: boolean }) {\n  if (live) return <b>live</b>;\n  const [n] = useState(0);\n  return <p>{n}</p>;\n}\n";
  let err = lower(&[("routes/index/page.tsx", after)], "routes/index/page.tsx#default").unwrap_err().to_string();
  assert!(err.starts_with("routes/index/page.tsx:4:9: `useState` after an early `return`"), "{err}");
  let otherwise = "export default function P({ live }: { live: boolean }) {\n  if (live) return <b>live</b>;\n  else return <p>still</p>;\n}\n";
  let lowered = lower(&[("routes/index/page.tsx", otherwise)], "routes/index/page.tsx#default").unwrap();
  assert!(matches!(&lowered[0].1.render, Tmpl::If { r#else: Some(_), .. }), "{:?}", lowered[0].1.render);
}

#[test]
fn an_island_around_an_element_or_with_a_computed_timing_is_residue() {
  let element = [("routes/a/page.tsx", "import { Island } from \"@snapfire/fsr/client/react\";\nexport default function A() {\n  return <Island when=\"visible\"><p>x</p></Island>;\n}\n")];
  let err = lower(&element, "routes/a/page.tsx#default").unwrap_err().to_string();
  assert!(err.contains("an island must be a component"), "{err}");
  let timing = [("routes/b/page.tsx", "import { Island } from \"@snapfire/fsr/client/react\";\nimport { Help } from \"../../src/ui/Help\";\nexport default function B({ n }: { n: number }) {\n  return <Island when={n > 0 ? \"visible\" : \"load\"}><Help /></Island>;\n}\n"), ("src/ui/Help.tsx", "export function Help() {\n  return <p>help</p>;\n}\n")];
  let err = lower(&timing, "routes/b/page.tsx#default").unwrap_err().to_string();
  assert!(err.contains("written out"), "{err}");
}

#[test]
fn a_provider_tag_binds_its_value_around_its_children_and_the_component_hydrates() {
  let files = [
    ("src/theme.ts", "import { createContext } from \"react\";\nexport const Theme = createContext(\"light\");\n"),
    (
      "routes/layout.tsx",
      "import { Theme } from \"../src/theme\";\nexport default function Layout({ children, mode }: { children: unknown; mode: string }) {\n  return <Theme.Provider value={mode}><div class=\"shell\">{children}</div></Theme.Provider>;\n}\n",
    ),
  ];
  let mut set = ComponentSet::new(&app(&files));
  set.layouts.push("routes/layout.tsx#default".to_owned());
  set.lower("routes/layout.tsx#default").unwrap();
  let layout = &set.components[0].1;
  let Tmpl::Let { name, expr, then } = &layout.render else { panic!("{:?}", layout.render) };
  assert_eq!(name, "$ctx:src/theme.ts#Theme", "the context is named where `createContext` declares it");
  assert_eq!(expr, &Expr::var("$props").field("mode"));
  let Tmpl::Fragment(items) = &**then else { panic!("{then:?}") };
  let Tmpl::Element { tag, children, .. } = &items[0] else { panic!("{:?}", items[0]) };
  assert_eq!(tag, "div");
  assert_eq!(children[0], Tmpl::Element { tag: "sf-s".to_owned(), attrs: Vec::new(), children: vec![Tmpl::Slot("content".to_owned())] });
  assert_eq!(layout.owner, Owner::React, "a provider only exists in the browser, so the layout is a root");
}

#[test]
fn a_custom_hook_inlines_with_state_of_its_own_per_call_and_its_returned_setter_is_a_handler() {
  let files = [
    ("src/hooks.ts", "import { useState } from \"react\";\nexport function useCount(start: number) {\n  const [n, setN] = useState(start);\n  return { n, twice: n * 2, setN };\n}\n"),
    (
      "routes/a/page.tsx",
      "import { useCount } from \"../../src/hooks\";\nexport default function A() {\n  const one = useCount(1);\n  const { n, twice, setN } = useCount(5);\n  return <button onClick={() => setN(n + 1)}>{one.n} {n} {twice}</button>;\n}\n",
    ),
  ];
  let lowered = lower(&files, "routes/a/page.tsx#default").unwrap();
  let page = &lowered.iter().find(|(m, _)| m == "routes/a/page.tsx#default").unwrap().1;
  assert_eq!(page.state, vec!["useCount$1$n".to_owned(), "useCount$2$n".to_owned()], "each call holds its own state: {:?}", page.body);
  assert_eq!(page.handlers.len(), 1, "the returned setter is a handler the button's arrow calls: {:?}", page.render);
  let Tmpl::Element { attrs, .. } = &page.render else { panic!("{:?}", page.render) };
  assert!(attrs.iter().any(|a| matches!(a, Entry::Field(n, _) if n == "$on:click")), "{attrs:?}");
}

const NAME_HOOK: &str = "import { useState } from \"react\";\nexport function useName() {\n  const [name, setName] = useState(\"\");\n  const update = (next: string) => {\n    setName(next);\n  };\n  return [name, update];\n}\n";

const PREFIX_HOOK: &str = "import { useState } from \"react\";\nexport function useName(prefix: string) {\n  const [name, setName] = useState(\"\");\n  const update = (next: string) => {\n    setName(prefix + next);\n  };\n  return [name, update];\n}\n";

fn sets(body: &Body) -> Vec<(String, Expr)> {
  let Some(Stmt::Return(Expr::Object(fields))) = body.last() else { panic!("{body:?}") };
  fields.iter().map(|f| match f { Entry::Field(n, e) => (n.clone(), e.clone()), other => panic!("{other:?}") }).collect()
}

#[test]
fn a_function_a_custom_hook_returns_calls_the_hook_s_own_setter_when_the_caller_binds_it_to_the_setter_s_name() {
  let files = [
    ("src/hooks.ts", NAME_HOOK),
    ("routes/a/page.tsx", "import { useName } from \"../../src/hooks\";\nexport default function A() {\n  const [name, setName] = useName();\n  return <button onClick={() => setName(\"b\")}>{name}</button>;\n}\n"),
  ];
  let lowered = lower(&files, "routes/a/page.tsx#default").unwrap();
  let page = &lowered.iter().find(|(m, _)| m == "routes/a/page.tsx#default").unwrap().1;
  assert_eq!(page.state, vec!["useName$1$name".to_owned()], "{:?}", page.body);
  assert_eq!(page.handlers.len(), 1, "{:?} {:?}", page.body, page.render);
  let written = sets(&page.handlers[0].body);
  assert_eq!(written.iter().map(|(n, _)| n.as_str()).collect::<Vec<_>>(), vec!["useName$1$name"], "{written:?}");
  assert!(format!("{:?}", written[0].1).contains("\"b\""), "{written:?}");
}

#[test]
fn a_function_a_custom_hook_returns_passed_to_a_child_s_prop_calls_the_hook_s_own_setter() {
  let files = [
    ("src/hooks.ts", NAME_HOOK),
    ("src/Field.tsx", "export function Field({ onName }: { onName: (name: string) => void }) {\n  return <form onSubmit={(e) => {\n    e.preventDefault();\n    onName(\"b\");\n  }}><button>Set</button></form>;\n}\n"),
    ("routes/a/page.tsx", "import { useName } from \"../../src/hooks\";\nimport { Field } from \"../../src/Field\";\nexport default function A() {\n  const [name, setName] = useName();\n  return <main><p>{name}</p><Field onName={setName} /></main>;\n}\n"),
  ];
  let lowered = lower(&files, "routes/a/page.tsx#default").unwrap();
  let page = &lowered.iter().find(|(m, _)| m == "routes/a/page.tsx#default").unwrap().1;
  assert_eq!(page.state, vec!["useName$1$name".to_owned()], "{:?}", page.body);
  let written: Vec<_> = page.handlers.iter().flat_map(|h| sets(&h.body)).map(|(n, _)| n).collect();
  assert_eq!(written, vec!["useName$1$name".to_owned()], "{:?}", page.handlers);
}

#[test]
fn a_function_a_custom_hook_returns_reads_the_hook_s_parameters_as_the_call_passed_them() {
  let files = [
    ("src/hooks.ts", PREFIX_HOOK),
    ("routes/a/page.tsx", "import { useName } from \"../../src/hooks\";\nexport default function A() {\n  const prefix = \"page\";\n  const [name, update] = useName(\"x-\");\n  return <button onClick={() => update(prefix)}>{name}</button>;\n}\n"),
  ];
  let lowered = lower(&files, "routes/a/page.tsx#default").unwrap();
  let page = &lowered.iter().find(|(m, _)| m == "routes/a/page.tsx#default").unwrap().1;
  assert_eq!(page.handlers.len(), 1, "{:?} {:?}", page.body, page.render);
  let written = sets(&page.handlers[0].body);
  assert_eq!(written.len(), 1, "{written:?}");
  assert_eq!(written[0].0, "useName$1$name");
  assert_eq!(written[0].1, Expr::Arith(ArithOp::Add, Box::new(Expr::lit_str("x-")), Box::new(Expr::Var("prefix".to_owned()))), "the hook's `prefix` is the call's \"x-\" and the argument is the page's own `prefix`");
  assert!(page.body.contains(&Stmt::Let { name: "prefix".to_owned(), expr: Expr::lit_str("page") }), "{:?}", page.body);
}

#[test]
fn use_reducer_holds_its_initial_state_and_use_context_reads_the_nearest_provider() {
  let files = [
    ("src/theme.ts", "import { createContext } from \"react\";\nexport const Theme = createContext(\"light\");\n"),
    ("src/Label.tsx", "import { useContext } from \"react\";\nimport { Theme } from \"./theme\";\nexport function Label() {\n  const theme = useContext(Theme);\n  return <i>{theme}</i>;\n}\n"),
    (
      "routes/a/page.tsx",
      "import { useReducer } from \"react\";\nimport { Theme } from \"../../src/theme\";\nimport { Label } from \"../../src/Label\";\nexport default function A() {\n  const [n, dispatch] = useReducer((s: number, a: number) => s + a, 2);\n  return <Theme.Provider value=\"dark\"><b onClick={() => dispatch(1)}>{n}</b><Label /></Theme.Provider>;\n}\n",
    ),
  ];
  let lowered = lower(&files, "routes/a/page.tsx#default").unwrap();
  let page = &lowered.iter().find(|(m, _)| m == "routes/a/page.tsx#default").unwrap().1;
  assert_eq!(page.state, vec!["n".to_owned()]);
  assert_eq!(page.body, vec![Stmt::Let { name: "n".to_owned(), expr: Expr::Lit(Lit::Float(2.0)) }]);
  let label = &lowered.iter().find(|(m, _)| m == "src/Label.tsx#Label").unwrap().1;
  assert_eq!(label.body, vec![Stmt::Let { name: "theme".to_owned(), expr: Expr::Coalesce(Box::new(Expr::Context("src/theme.ts#Theme".to_owned())), Box::new(Expr::lit_str("light"))) }]);
}

#[test]
fn use_and_use_context_read_a_context_wherever_an_expression_stands() {
  let files = [
    ("src/theme.ts", "import { createContext } from \"react\";\nexport const Theme = createContext(\"light\");\n"),
    (
      "src/Tag.tsx",
      "import * as React from \"react\";\nimport { use, useContext } from \"react\";\nimport { Theme } from \"./theme\";\nexport function Tag({ compact }: { compact: boolean }) {\n  const a = use(Theme);\n  const b = compact ? use(Theme) : \"plain\";\n  const c = React.use(Theme);\n  return <i className={useContext(Theme)} title={React.useContext(Theme)}>{a} {b} {c} {use(Theme)}</i>;\n}\n",
    ),
    ("routes/a/page.tsx", "import { Theme } from \"../../src/theme\";\nimport { Tag } from \"../../src/Tag\";\nexport default function A() {\n  return <Theme.Provider value=\"dark\"><Tag compact={true} /></Theme.Provider>;\n}\n"),
  ];
  let lowered = lower(&files, "routes/a/page.tsx#default").unwrap();
  let tag = &lowered.iter().find(|(m, _)| m == "src/Tag.tsx#Tag").unwrap().1;
  let read = Expr::Coalesce(Box::new(Expr::Context("src/theme.ts#Theme".to_owned())), Box::new(Expr::lit_str("light")));
  assert_eq!(tag.body[0], Stmt::Let { name: "a".to_owned(), expr: read.clone() });
  assert_eq!(tag.body[2], Stmt::Let { name: "c".to_owned(), expr: read });
  let reads = format!("{:?} {:?}", tag.body, tag.render).matches("Context(\"src/theme.ts#Theme\")").count();
  assert_eq!(reads, 6, "{:?} {:?}", tag.body, tag.render);
}

#[test]
fn a_template_context_lowers_like_reacts_and_leaves_the_page_to_the_server() {
  let files = [
    ("src/reserved.ts", "import { createContext } from \"@snapfire/fsr/template\";\nexport const Reserved = createContext<string[]>([]);\n"),
    ("src/Row.tsx", "import { use } from \"@snapfire/fsr/template\";\nimport { Reserved } from \"./reserved\";\nexport function Row({ id }: { id: string }) {\n  return <li>{use(Reserved).includes(id) ? \"kept\" : \"free\"}</li>;\n}\n"),
    ("routes/a/page.tsx", "import { Reserved } from \"../../src/reserved\";\nimport { Row } from \"../../src/Row\";\nexport default function A({ reserved }: { reserved: string[] }) {\n  return <Reserved.Provider value={reserved}><ul><Row id=\"a\" /></ul></Reserved.Provider>;\n}\n"),
  ];
  let lowered = lower(&files, "routes/a/page.tsx#default").unwrap();
  let page = &lowered.iter().find(|(m, _)| m == "routes/a/page.tsx#default").unwrap().1;
  let Tmpl::Let { name, .. } = &page.render else { panic!("{:?}", page.render) };
  assert_eq!(name, "$ctx:src/reserved.ts#Reserved");
  assert_eq!(page.owner, Owner::Fsr, "the server resolves a template context, so nothing mounts the page");
  let row = &lowered.iter().find(|(m, _)| m == "src/Row.tsx#Row").unwrap().1;
  assert!(format!("{:?}", row.render).contains("Context(\"src/reserved.ts#Reserved\")"), "{:?}", row.render);
}

#[test]
fn the_template_use_of_something_other_than_a_context_is_residue() {
  let files = [("routes/a/page.tsx", "import { use } from \"@snapfire/fsr/template\";\nexport default function A({ theme }: { theme: string }) {\n  return <p>{use(theme)}</p>;\n}\n")];
  let err = lower(&files, "routes/a/page.tsx#default").unwrap_err().to_string();
  assert!(err.contains("`use` of something other than a `createContext`"), "{err}");
}

#[test]
fn use_of_something_other_than_a_context_stays_residue() {
  let files = [("routes/a/page.tsx", "import { use } from \"react\";\nexport default function A({ pending }: { pending: Promise<string> }) {\n  const name = use(pending);\n  return <p>{name}</p>;\n}\n")];
  let err = lower(&files, "routes/a/page.tsx#default").unwrap_err().to_string();
  assert!(err.contains("`use` of something other than a `createContext`"), "{err}");
  let inline = [("routes/a/page.tsx", "import { use } from \"react\";\nexport default function A({ pending }: { pending: Promise<string> }) {\n  return <p>{use(pending)}</p>;\n}\n")];
  let err = lower(&inline, "routes/a/page.tsx#default").unwrap_err().to_string();
  assert!(err.contains("`use` of something other than a `createContext`"), "{err}");
}

#[test]
fn use_id_is_a_let_of_its_numbered_id() {
  let id = [("routes/a/page.tsx", "import { useId } from \"react\";\nexport default function A() {\n  const id = useId();\n  return <label htmlFor={id}>x</label>;\n}\n")];
  let lowered = lower(&id, "routes/a/page.tsx#default").unwrap();
  let (_, component) = lowered.iter().find(|(m, _)| m == "routes/a/page.tsx#default").unwrap();
  assert_eq!(component.body, vec![Stmt::Let { name: "id".to_owned(), expr: Expr::HoistId(0) }]);
}

#[test]
fn a_hook_the_build_cannot_follow_stays_residue() {
  let unknown = [("routes/a/page.tsx", "import { useThing } from \"thing-lib\";\nexport default function A() {\n  const t = useThing();\n  return <p>{t}</p>;\n}\n")];
  let err = lower(&unknown, "routes/a/page.tsx#default").unwrap_err().to_string();
  assert!(err.contains("`useThing`, a hook the build cannot follow"), "{err}");
}

#[test]
fn a_provider_whose_object_is_not_a_context_is_residue() {
  let files = [("routes/a/page.tsx", "const Theme = { Provider: (p: { children: unknown }) => p.children };\nexport default function A() {\n  return <Theme.Provider><p>x</p></Theme.Provider>;\n}\n")];
  let err = lower(&files, "routes/a/page.tsx#default").unwrap_err().to_string();
  assert!(err.contains("`Theme` is not a `createContext` value"), "{err}");
}

#[test]
fn tree_is_refused_with_what_to_write_instead() {
  let layout = "import type { ReactNode } from \"react\";\nimport { tree } from \"@snapfire/fsr/client/react\";\nfunction Layout({ children }: { children: ReactNode }) {\n  return <main>{children}</main>;\n}\nexport default tree(Layout);\n";
  let mut set = ComponentSet::new(&app(&[("routes/layout.tsx", layout)]));
  set.layouts.push("routes/layout.tsx#default".to_owned());
  let err = set.lower("routes/layout.tsx#default").unwrap_err();
  assert!(matches!(err, LowerError::Retired(_)), "a hard error, not residue that sends the layout to the browser: {err}");
  assert!(err.to_string().contains("`tree(...)` is gone") && err.to_string().contains("export default"), "{err}");
}



#[test]
fn a_module_that_does_not_lower_is_answered_from_memory_the_second_time() {
  let files = [
    ("routes/a/page.tsx", "import { Broken } from \"@src/Broken\";\nexport default function A() {\n  return <Broken />;\n}\n"),
    ("routes/b/page.tsx", "import { Broken } from \"@src/Broken\";\nexport default function B() {\n  return <Broken />;\n}\n"),
    ("src/Broken.tsx", "export function Broken() {\n  return <p>{;\n}\n"),
  ];
  let dir = app(&files);
  let mut set = ComponentSet::new(&dir);
  let first = set.lower("routes/a/page.tsx#default").unwrap_err();
  assert!(first.to_string().contains("src/Broken.tsx: 2:14"), "{first}");
  std::fs::write(dir.join("src/Broken.tsx"), "export function Broken() {\n  return <p>fixed</p>;\n}\n").unwrap();
  assert_eq!(set.lower("routes/a/page.tsx#default").unwrap_err(), first, "the page answers the same error without reading again");
  let other = set.lower("routes/b/page.tsx#default").unwrap_err();
  assert!(other.to_string().starts_with("routes/b/page.tsx:3:10: src/Broken.tsx: 2:14"), "a second page reaching the module gets the failure it gave the first: {other}");
  std::fs::remove_dir_all(&dir).unwrap();
}
