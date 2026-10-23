//! Hoisting: which render-path work the server does for the browser and the
//! source rewrite that makes the browser read it instead of doing it again.
//!
//! Values: the lowerer wraps every helper call and formatting builtin as a
//! candidate; `decide` keeps a candidate when its inputs are props only. The
//! survivors become `Expr::Hoist` in the plan and a read in the bundle's copy
//! of the module. Subtrees: every element with children is a candidate;
//! `chunks` keeps the outermost ones that are static, so the renderer records
//! their inner markup and the browser hands it to React as the element's
//! inner HTML. Both are keyed by the module, the id and the enclosing loop
//! indices and both fall back to the original code on a miss.
//!
//! Islands are keyed the same way and never decided away: each placement
//! takes an id and the rewrite gives `<Island>` the key the server wrote on
//! the region, so a re-render pairs a placement with its region rather than
//! with whatever sits at its position. A placement of a component that keys
//! anything is keyed as well: the path under it names the placement, so two
//! placements of one component key what they hold apart. A component that
//! renders itself is such a pair.

use std::collections::HashMap;
use std::ops::Range;

use snapfire_fsr_ir::ast::{Component, Entry, Expr, Lit, Stmt, Tmpl};

/// The attribute the lowerer leaves on an element carrying a handler, a
/// `ref` or a spread, which the browser must render itself. Never printed.
pub const BOUND_ATTR: &str = "$bound";
/// The attribute marking an element whose inner markup the renderer records
/// under the id it holds. Never printed.
pub const CHUNK_ATTR: &str = "$chunk";
/// The attribute beside `CHUNK_ATTR` on a chunk under a branch on state,
/// which the renderer records whether or not the branch is taken, since the
/// browser may take it later. Never printed.
pub const UNTAKEN_ATTR: &str = "$untaken";

/// A subtree the server renders for the browser. `proven` says the browser's
/// reader can count on the markup being recorded, so the rewrite drops the
/// fallback; a chunk among the children a component or an island is given is
/// rendered by its slot, which the reader is not keyed for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub struct Chunk {
  pub id: u32,
  pub proven: bool,
}

/// The hoist candidates of one component as the lowerer meets them.
#[derive(Debug, Default)]
#[non_exhaustive]
pub struct Candidates {
  next: u32,
  /// Island placements are numbered apart from the hoist candidates, so
  /// placing a component never renumbers a hoist key.
  next_island: u32,
  /// Each value candidate's id, the byte range of its call in the source and
  /// the arrow ranges of the JSX `.map` callbacks it sits in, outermost first.
  pub sites: Vec<(u32, Range<usize>, Vec<Range<usize>>)>,
  /// Each subtree candidate's id, the element's byte range, its opening tag's
  /// byte range, whether it sits among JSX children and the callbacks it sits in.
  pub chunk_sites: Vec<(u32, Range<usize>, Range<usize>, bool, Vec<Range<usize>>)>,
  /// Each component placement's id, the offset just after its opening tag's
  /// name, the element's byte range, whether it sits among JSX children and
  /// the callbacks it sits in. `rewrite` keeps the placements that became
  /// islands and the keyed ones.
  pub island_sites: Vec<(u32, usize, Range<usize>, bool, Vec<Range<usize>>)>,
  /// Each `useId` call's id, numbered with the value candidates, the byte range of the call and the callbacks it sits in. Never decided away: both halves spell the id from the key.
  pub id_sites: Vec<(u32, Range<usize>, Vec<Range<usize>>)>,
  /// The `.map` callbacks being lowered, outermost first, and the statement loops among them by their statement's range.
  pub open_loops: Vec<Range<usize>>,
  /// The `for…of` and counted `for` loops whose bodies the build lowered, which the browser keys by iteration as it keys a `.map` callback.
  pub statement_loops: Vec<StatementLoop>,
}

/// A `for…of` or counted `for` a component's render lowered: the statement's range, its body's and the arguments of the `push` calls its body makes, which the rewrite marks with the iteration's path.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct StatementLoop {
  pub range: Range<usize>,
  pub body: Range<usize>,
  pub pushes: Vec<Range<usize>>,
}

impl StatementLoop {
  pub fn new(range: Range<usize>, body: Range<usize>, pushes: Vec<Range<usize>>) -> Self {
    Self { range, body, pushes }
  }
}

impl Candidates {
  pub(crate) fn add(&mut self, range: Range<usize>, expr: Expr) -> Expr {
    let id = self.next;
    self.next += 1;
    self.sites.push((id, range, self.open_loops.clone()));
    Expr::Hoist { id, expr: Box::new(expr) }
  }

  /// The marker attribute for an element with children, a subtree candidate.
  /// `as_child` says the element sits among JSX children, so its rewrite is braced.
  pub(crate) fn chunk(&mut self, range: Range<usize>, open: Range<usize>, as_child: bool) -> Entry {
    let id = self.next;
    self.next += 1;
    self.chunk_sites.push((id, range, open, as_child, self.open_loops.clone()));
    Entry::Field(CHUNK_ATTR.to_owned(), Expr::Lit(Lit::Int(id as i128)))
  }

  /// The id of a component placement, which is its region key's id when the
  /// placement turns out to be an island. `at` is the end of the opening
  /// tag's name, where the key prop is spliced in; `range` is the element and
  /// `as_child` says it sits among JSX children, for a keyed placement's wrap.
  pub(crate) fn island(&mut self, at: usize, range: Range<usize>, as_child: bool) -> u32 {
    let id = self.next_island;
    self.next_island += 1;
    self.island_sites.push((id, at, range, as_child, self.open_loops.clone()));
    id
  }

  /// The expression for a `useId` call at `range`.
  pub(crate) fn id(&mut self, range: Range<usize>) -> Expr {
    let id = self.next;
    self.next += 1;
    self.id_sites.push((id, range, self.open_loops.clone()));
    Expr::HoistId(id)
  }

  /// The call ranges of the value candidates that stay in the browser: not
  /// kept, not inside a kept one, whose read covers them, and not inside a
  /// proven chunk, whose markup covers them.
  pub fn remaining(&self, kept: &[u32], chunks: &[Chunk]) -> Vec<Range<usize>> {
    let mut held: Vec<&Range<usize>> = self.sites.iter().filter(|(id, _, _)| kept.contains(id)).map(|(_, range, _)| range).collect();
    held.extend(self.chunk_sites.iter().filter(|(id, ..)| chunks.iter().any(|c| c.id == *id && c.proven)).map(|(_, range, ..)| range));
    self
      .sites
      .iter()
      .filter(|(id, range, _)| !kept.contains(id) && !held.iter().any(|h| h.start <= range.start && range.end <= h.end))
      .map(|(_, range, _)| range.clone())
      .collect()
  }

  /// The rewrite for the candidates in `values` and `chunks`, the island
  /// placements in `islands` and the keyed placements in `keyed`; `None`
  /// when there is none of them.
  #[allow(clippy::too_many_arguments)]
  pub fn rewrite(self, values: &[u32], chunks: &[Chunk], islands: &[u32], keyed: &[u32], file: &str, module: &str, hook: Hook) -> Option<Rewrite> {
    let mut sites = Vec::new();
    let mut chunk_sites = Vec::new();
    let mut loops: Vec<Range<usize>> = Vec::new();
    let mut remember = |enclosing: Vec<Range<usize>>| {
      for range in enclosing {
        if !loops.contains(&range) {
          loops.push(range);
        }
      }
    };
    for (id, range, enclosing) in self.sites {
      if values.contains(&id) {
        sites.push((id, range));
        remember(enclosing);
      }
    }
    for (id, range, open, as_child, enclosing) in self.chunk_sites {
      if let Some(chunk) = chunks.iter().find(|c| c.id == id) {
        chunk_sites.push(ChunkSite { id, range, open, as_child, proven: chunk.proven });
        remember(enclosing);
      }
    }
    let mut island_sites = Vec::new();
    let mut placements = Vec::new();
    for (id, at, range, as_child, enclosing) in self.island_sites {
      if islands.contains(&id) {
        island_sites.push((id, at));
        remember(enclosing);
      } else if keyed.contains(&id) {
        placements.push((id, range, as_child));
        remember(enclosing);
      }
    }
    let mut ids = Vec::new();
    for (id, range, enclosing) in self.id_sites {
      ids.push((id, range));
      remember(enclosing);
    }
    if sites.is_empty() && chunk_sites.is_empty() && island_sites.is_empty() && placements.is_empty() && ids.is_empty() {
      return None;
    }
    let statement_loops: Vec<StatementLoop> = self.statement_loops.into_iter().filter(|l| loops.contains(&l.range)).collect();
    loops.retain(|range| !statement_loops.iter().any(|l| l.range == *range));
    Some(Rewrite { file: file.to_owned(), module: module.to_owned(), hook, sites, chunks: chunk_sites, islands: island_sites, placements, ids, loops, statement_loops, browser: Vec::new() })
  }
}

/// A surviving subtree candidate in the source: its element range, its
/// opening tag's range and whether it sits among JSX children. A proven one
/// is rewritten to the hit alone; the rest keep the element as the fallback.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct ChunkSite {
  pub id: u32,
  pub range: Range<usize>,
  pub open: Range<usize>,
  pub as_child: bool,
  pub proven: bool,
}

/// Where the reader hook goes: after the `{` of a block body or around an
/// arrow's expression body, which becomes a block returning it.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub enum Hook {
  Block { after: usize },
  Expression(Range<usize>),
}

/// The rewrite of one component's source for the bundle.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub struct Rewrite {
  pub file: String,
  pub module: String,
  pub hook: Hook,
  /// The surviving value candidates: id and call range.
  pub sites: Vec<(u32, Range<usize>)>,
  /// The surviving subtree candidates.
  pub chunks: Vec<ChunkSite>,
  /// The island placements: id and the offset the reader call is spliced at.
  pub islands: Vec<(u32, usize)>,
  /// The keyed placements: id, element range and whether the element sits
  /// among JSX children. Each is wrapped so what the component it places
  /// keys sits below the placement.
  pub placements: Vec<(u32, Range<usize>, bool)>,
  /// The `useId` calls: id and call range. Each is replaced by the reader's spelling of the id.
  pub ids: Vec<(u32, Range<usize>)>,
  /// The `.map` callbacks whose bodies the survivors sit in, as arrow ranges.
  pub loops: Vec<Range<usize>>,
  /// The statement loops the survivors sit in.
  pub statement_loops: Vec<StatementLoop>,
  /// The expressions whose value only the browser knows, which the server rendered as null: each is wrapped so it is null through hydration, matching the server's markup, and computed once the component has mounted.
  pub browser: Vec<Range<usize>>,
}

/// Unwraps every value candidate in `component` that is not props only and
/// returns the ids that stay. `state` names the bindings the browser can change.
pub fn decide(component: &mut Component, state: &[String]) -> Vec<u32> {
  let mut tainted: Vec<String> = state.to_vec();
  let mut kept = Vec::new();
  for stmt in &mut component.body {
    let Stmt::Let { name, expr } = stmt else { continue };
    let dirty = reads_tainted(expr, &tainted);
    strip(expr, &tainted, &mut kept, false, false);
    if dirty {
      tainted.push(name.clone());
    }
  }
  tmpl(&mut component.render, &mut tainted, &mut kept);
  kept
}

fn tmpl(t: &mut Tmpl, tainted: &mut Vec<String>, kept: &mut Vec<u32>) {
  match t {
    // `Baked` is what a loaded component becomes, so the lowerer never meets one.
    Tmpl::Text(_) | Tmpl::Marker(_) | Tmpl::Outlet { .. } | Tmpl::Slot(_) | Tmpl::Baked { .. } => {}
    Tmpl::Expr(e) | Tmpl::Html(e) => strip(e, tainted, kept, false, false),
    Tmpl::Element { attrs, children, .. } => {
      entries(attrs, tainted, kept);
      children.iter_mut().for_each(|c| tmpl(c, tainted, kept));
    }
    Tmpl::Fragment(children) | Tmpl::Place { children, .. } => children.iter_mut().for_each(|c| tmpl(c, tainted, kept)),
    Tmpl::If { cond, then, r#else } => {
      strip(cond, tainted, kept, false, false);
      tmpl(then, tainted, kept);
      if let Some(other) = r#else {
        tmpl(other, tainted, kept);
      }
    }
    Tmpl::For { over, params, body } => {
      let dirty = reads_tainted(over, tainted);
      strip(over, tainted, kept, false, false);
      let depth = tainted.len();
      if dirty {
        tainted.extend(params.iter().cloned());
      }
      tmpl(body, tainted, kept);
      tainted.truncate(depth);
    }
    Tmpl::Let { name, expr, then } => {
      let dirty = reads_tainted(expr, tainted);
      strip(expr, tainted, kept, false, false);
      let depth = tainted.len();
      if dirty {
        tainted.push(name.clone());
      }
      tmpl(then, tainted, kept);
      tainted.truncate(depth);
    }
    Tmpl::Component { props, children, .. } | Tmpl::Island { props, children, .. } => {
      entries(props, tainted, kept);
      children.iter_mut().for_each(|c| tmpl(c, tainted, kept));
    }
  }
}

fn entries(entries: &mut [Entry], tainted: &[String], kept: &mut Vec<u32>) {
  for entry in entries {
    match entry {
      Entry::Baked(_) => {}
      Entry::Field(_, e) | Entry::Item(e) | Entry::Spread(e) => strip(e, tainted, kept, false, false),
      Entry::Computed(k, v) => {
        strip(k, tainted, kept, false, false);
        strip(v, tainted, kept, false, false);
      }
    }
  }
}

/// True when the expression reads a tainted name or anything the request
/// or the browser can change under it.
fn reads_tainted(expr: &Expr, tainted: &[String]) -> bool {
  let mut free = Vec::new();
  expr.free_vars(&mut free);
  if free.iter().any(|name| tainted.contains(name)) {
    return true;
  }
  let mut ambient = false;
  expr.visit(&mut |e| {
    if matches!(e, Expr::Store(_) | Expr::Context(_) | Expr::Now | Expr::Param(_) | Expr::Query(_) | Expr::ParamsAll | Expr::QueryAll | Expr::Session(_) | Expr::Identity(_) | Expr::Input | Expr::Call { .. }) {
      ambient = true;
    }
  });
  ambient
}

fn strip(expr: &mut Expr, tainted: &[String], kept: &mut Vec<u32>, in_lambda: bool, in_hoist: bool) {
  if let Expr::Hoist { id, expr: inner } = expr {
    if in_lambda || in_hoist || reads_tainted(inner, tainted) {
      let mut taken = std::mem::replace(inner.as_mut(), Expr::Lit(Lit::Null));
      strip(&mut taken, tainted, kept, in_lambda, in_hoist);
      *expr = taken;
    } else {
      kept.push(*id);
      strip(inner, tainted, kept, in_lambda, true);
    }
    return;
  }
  match expr {
    Expr::Hoist { .. } => unreachable!("handled above"),
    Expr::Markup(inner) => tmpl(std::sync::Arc::make_mut(inner), &mut tainted.to_vec(), kept),
    Expr::Folded(_) | Expr::Param(_) | Expr::Query(_) | Expr::ParamsAll | Expr::QueryAll | Expr::Session(_) | Expr::Store(_) | Expr::Context(_) | Expr::Identity(_) | Expr::Locale | Expr::Path | Expr::Document | Expr::Host | Expr::Origin | Expr::Address | Expr::Config(_) | Expr::Input | Expr::Now | Expr::Var(_) | Expr::Const(_) | Expr::Lit(_) | Expr::Handler(_) | Expr::HoistId(_) => {}
    Expr::Next(e) | Expr::After(e) => strip(e, tainted, kept, in_lambda, in_hoist),
    Expr::Call { args, .. } | Expr::NativeCall { args, .. } => args.iter_mut().for_each(|(_, e)| strip(e, tainted, kept, in_lambda, in_hoist)),
    Expr::Object(entries) | Expr::Array(entries) => entries.iter_mut().for_each(|entry| match entry {
      Entry::Baked(_) => {}
      Entry::Field(_, e) | Entry::Item(e) | Entry::Spread(e) => strip(e, tainted, kept, in_lambda, in_hoist),
      Entry::Computed(k, v) => {
        strip(k, tainted, kept, in_lambda, in_hoist);
        strip(v, tainted, kept, in_lambda, in_hoist);
      }
    }),
    Expr::Field(e, _) | Expr::Not(e) | Expr::Entries(e) | Expr::Keys(e) | Expr::Values(e) | Expr::Length(e) | Expr::Str(e) | Expr::Num(e) | Expr::BigInt(e) | Expr::Cell(e) | Expr::ReadCell(e) | Expr::WriteStore(_, e) => strip(e, tainted, kept, in_lambda, in_hoist),
    Expr::WriteCell(a, b) | Expr::ModuleCell { key: a, init: b } => {
      strip(a, tainted, kept, in_lambda, in_hoist);
      strip(b, tainted, kept, in_lambda, in_hoist);
    }
    Expr::Index(a, b) | Expr::Arith(_, a, b) | Expr::Compare(_, a, b) | Expr::Logic(_, a, b) | Expr::Coalesce(a, b) | Expr::Map(a, b) | Expr::Filter(a, b) | Expr::Find(a, b) | Expr::FindIndex(a, b) | Expr::Some(a, b) | Expr::Every(a, b) | Expr::Sort(a, b) | Expr::FlatMap(a, b) | Expr::ApplyOf(a, b) => {
      strip(a, tainted, kept, in_lambda, in_hoist);
      strip(b, tainted, kept, in_lambda, in_hoist);
    }
    Expr::Ternary(a, b, c) | Expr::Reduce(a, b, c) | Expr::ReplaceWith(a, b, c) => {
      strip(a, tainted, kept, in_lambda, in_hoist);
      strip(b, tainted, kept, in_lambda, in_hoist);
      strip(c, tainted, kept, in_lambda, in_hoist);
    }
    Expr::Template(parts) => parts.iter_mut().for_each(|e| strip(e, tainted, kept, in_lambda, in_hoist)),
    Expr::Builtin { args, .. } | Expr::Ext { args, .. } => args.iter_mut().for_each(|e| strip(e, tainted, kept, in_lambda, in_hoist)),
    Expr::Apply { f, args } => {
      strip(f, tainted, kept, in_lambda, in_hoist);
      args.iter_mut().for_each(|e| strip(e, tainted, kept, in_lambda, in_hoist));
    }
    Expr::Lambda { body, .. } => strip(body, tainted, kept, true, in_hoist),
    Expr::Block(body) => strip_body(body, tainted, kept, in_hoist),
  }
}

fn strip_body(body: &mut [Stmt], tainted: &[String], kept: &mut Vec<u32>, in_hoist: bool) {
  for stmt in body {
    match stmt {
      Stmt::Let { expr, .. } | Stmt::Set { expr, .. } | Stmt::Return(expr) | Stmt::Expr(expr) => strip(expr, tainted, kept, true, in_hoist),
      Stmt::If { cond, then, r#else } => {
        strip(cond, tainted, kept, true, in_hoist);
        strip_body(then, tainted, kept, in_hoist);
        strip_body(r#else, tainted, kept, in_hoist);
      }
      Stmt::ForOf { over, body, .. } => {
        strip(over, tainted, kept, true, in_hoist);
        strip_body(body, tainted, kept, in_hoist);
      }
      Stmt::Guard { cond, message, .. } => {
        strip(cond, tainted, kept, true, in_hoist);
        strip(message, tainted, kept, true, in_hoist);
      }
      Stmt::Act { input, .. } => strip(input, tainted, kept, true, in_hoist),
      Stmt::SessionSet { path, value, .. } => {
        path.iter_mut().for_each(|e| strip(e, tainted, kept, true, in_hoist));
        strip(value, tainted, kept, true, in_hoist);
      }
      Stmt::SessionDelete { path, .. } => path.iter_mut().for_each(|e| strip(e, tainted, kept, true, in_hoist)),
      Stmt::SessionExtend { seconds } => strip(seconds, tainted, kept, true, in_hoist),
      Stmt::Try { body, handler, finally, .. } => {
        strip_body(body, tainted, kept, in_hoist);
        handler.iter_mut().for_each(|handler| strip_body(handler, tainted, kept, in_hoist));
        strip_body(finally, tainted, kept, in_hoist);
      }
    }
  }
}

fn entry_exprs(entries: &[Entry]) -> impl Iterator<Item = &Expr> {
  entries.iter().flat_map(|entry| match entry {
    Entry::Field(_, e) | Entry::Item(e) | Entry::Spread(e) => vec![e],
    Entry::Computed(k, v) => vec![k, v],
    Entry::Baked(_) => Vec::new(),
  })
}

fn has_attr(attrs: &[Entry], name: &str) -> bool {
  attrs.iter().any(|entry| matches!(entry, Entry::Field(n, _) if n == name))
}

/// True when the subtree reads nothing tainted, carries no handler, holds no
/// island or slot and renders only pure components: what the server can
/// render once for the island's whole life.
pub fn static_tree(t: &Tmpl, pure: &HashMap<String, bool>) -> bool {
  is_static(t, &[], pure)
}

fn is_static(t: &Tmpl, tainted: &[String], pure: &HashMap<String, bool>) -> bool {
  match t {
    Tmpl::Text(_) | Tmpl::Marker(_) | Tmpl::Baked { .. } => true,
    Tmpl::Expr(e) | Tmpl::Html(e) => !reads_tainted(e, tainted),
    Tmpl::Element { tag, attrs, children } => {
      tag != "sf-s" && !has_attr(attrs, BOUND_ATTR) && entry_exprs(attrs).all(|e| !reads_tainted(e, tainted)) && children.iter().all(|c| is_static(c, tainted, pure))
    }
    Tmpl::Fragment(children) => children.iter().all(|c| is_static(c, tainted, pure)),
    Tmpl::If { cond, then, r#else } => !reads_tainted(cond, tainted) && is_static(then, tainted, pure) && r#else.as_ref().is_none_or(|e| is_static(e, tainted, pure)),
    Tmpl::For { over, body, .. } => !reads_tainted(over, tainted) && is_static(body, tainted, pure),
    Tmpl::Let { expr, then, .. } => !reads_tainted(expr, tainted) && is_static(then, tainted, pure),
    Tmpl::Component { module, props, children, .. } => {
      pure.get(module).copied().unwrap_or(false) && entry_exprs(props).all(|e| !reads_tainted(e, tainted)) && children.iter().all(|c| is_static(c, tainted, pure))
    }
    Tmpl::Island { .. } | Tmpl::Slot(_) | Tmpl::Place { .. } | Tmpl::Outlet { .. } => false,
  }
}

/// True when React would compute something in the subtree: an interpolation,
/// a branch, a loop, a binding, a component or an attribute that is not a
/// literal. A subtree of literal markup is left to React, which costs nothing.
fn does_work(t: &Tmpl) -> bool {
  match t {
    Tmpl::Text(_) | Tmpl::Marker(_) | Tmpl::Outlet { .. } | Tmpl::Slot(_) | Tmpl::Baked { .. } => false,
    Tmpl::Expr(_) | Tmpl::Html(_) | Tmpl::If { .. } | Tmpl::For { .. } | Tmpl::Let { .. } | Tmpl::Component { .. } | Tmpl::Island { .. } => true,
    Tmpl::Element { attrs, children, .. } => {
      attrs.iter().any(|entry| match entry {
        Entry::Field(name, Expr::Lit(_)) => name == CHUNK_ATTR && false,
        Entry::Field(name, _) if name == CHUNK_ATTR || name == BOUND_ATTR => false,
        _ => true,
      }) || children.iter().any(does_work)
    }
    Tmpl::Fragment(children) => children.iter().any(does_work),
    Tmpl::Place { .. } => true,
  }
}

/// Keeps the outermost static subtrees of `component` that do work, removes
/// every other `CHUNK_ATTR` and returns the chunks kept. A subtree inside a
/// loop over state is never kept: the browser can grow the loop past what
/// the server rendered. One under a branch on state is kept and marked
/// `UNTAKEN_ATTR`, so the server records it in either branch.
pub fn chunks(component: &mut Component, state: &[String], pure: &HashMap<String, bool>) -> Vec<Chunk> {
  let mut tainted: Vec<String> = state.to_vec();
  for stmt in &component.body {
    let Stmt::Let { name, expr } = stmt else { continue };
    if reads_tainted(expr, &tainted) {
      tainted.push(name.clone());
    }
  }
  let mut kept = Vec::new();
  choose(&mut component.render, &mut tainted, pure, Where::default(), &mut kept);
  kept
}

/// The name a component reads its props through, which `decide` and `chunks`
/// count as state for a component some placement passes state.
pub const PROPS: &str = "$props";

/// The modules `component` places with a prop that reads `state` or anything
/// the request or the browser can change, whose props therefore move after
/// hydration.
pub fn placed_with_state(component: &Component, state: &[String]) -> Vec<String> {
  let mut tainted: Vec<String> = state.to_vec();
  for stmt in &component.body {
    let Stmt::Let { name, expr } = stmt else { continue };
    if reads_tainted(expr, &tainted) {
      tainted.push(name.clone());
    }
  }
  let mut out = Vec::new();
  placements_reading(&component.render, &mut tainted, &mut out);
  out
}

fn placements_reading(t: &Tmpl, tainted: &mut Vec<String>, out: &mut Vec<String>) {
  match t {
    Tmpl::Text(_) | Tmpl::Marker(_) | Tmpl::Outlet { .. } | Tmpl::Html(_) | Tmpl::Expr(_) | Tmpl::Slot(_) | Tmpl::Baked { .. } => {}
    Tmpl::Element { children, .. } | Tmpl::Fragment(children) | Tmpl::Place { children, .. } | Tmpl::Island { children, .. } => children.iter().for_each(|c| placements_reading(c, tainted, out)),
    Tmpl::If { then, r#else, .. } => {
      placements_reading(then, tainted, out);
      if let Some(other) = r#else {
        placements_reading(other, tainted, out);
      }
    }
    Tmpl::For { over, params, body } => {
      let depth = tainted.len();
      if reads_tainted(over, tainted) {
        tainted.extend(params.iter().cloned());
      }
      placements_reading(body, tainted, out);
      tainted.truncate(depth);
    }
    Tmpl::Let { name, expr, then } => {
      let depth = tainted.len();
      if reads_tainted(expr, tainted) {
        tainted.push(name.clone());
      }
      placements_reading(then, tainted, out);
      tainted.truncate(depth);
    }
    Tmpl::Component { module, props, children, .. } => {
      if entry_exprs(props).any(|e| reads_tainted(e, tainted)) && !out.contains(module) {
        out.push(module.clone());
      }
      children.iter().for_each(|c| placements_reading(c, tainted, out));
    }
  }
}

/// `chunks` again over a component already decided once, dropping the
/// `UNTAKEN_ATTR` marks the first pass left so a chunk kept again is marked once.
pub fn rechunk(component: &mut Component, state: &[String], pure: &HashMap<String, bool>) -> Vec<Chunk> {
  clear_untaken(&mut component.render);
  chunks(component, state, pure)
}

fn clear_untaken(t: &mut Tmpl) {
  match t {
    Tmpl::Text(_) | Tmpl::Marker(_) | Tmpl::Outlet { .. } | Tmpl::Html(_) | Tmpl::Expr(_) | Tmpl::Slot(_) | Tmpl::Baked { .. } => {}
    Tmpl::Element { attrs, children, .. } => {
      attrs.retain(|entry| !matches!(entry, Entry::Field(n, _) if n == UNTAKEN_ATTR));
      children.iter_mut().for_each(clear_untaken);
    }
    Tmpl::Fragment(children) | Tmpl::Place { children, .. } | Tmpl::Component { children, .. } | Tmpl::Island { children, .. } => children.iter_mut().for_each(clear_untaken),
    Tmpl::If { then, r#else, .. } => {
      clear_untaken(then);
      if let Some(other) = r#else {
        clear_untaken(other);
      }
    }
    Tmpl::For { body, .. } => clear_untaken(body),
    Tmpl::Let { then, .. } => clear_untaken(then),
  }
}

/// Unwraps every value hoist among the children of a proven chunk in
/// `chunks`, whose markup the browser adopts whole, and returns their ids.
/// A hoist in the chunk's opening tag stays: the hit reads it.
pub fn cover(component: &mut Component, chunks: &[Chunk]) -> Vec<u32> {
  let mut covered = Vec::new();
  cover_tmpl(&mut component.render, chunks, false, &mut covered);
  covered
}

fn cover_tmpl(t: &mut Tmpl, chunks: &[Chunk], inside: bool, covered: &mut Vec<u32>) {
  match t {
    Tmpl::Text(_) | Tmpl::Marker(_) | Tmpl::Outlet { .. } | Tmpl::Slot(_) | Tmpl::Baked { .. } => {}
    Tmpl::Expr(e) | Tmpl::Html(e) => cover_expr(e, inside, covered),
    Tmpl::Element { attrs, children, .. } => {
      let proven = take_chunk(attrs).is_some_and(|id| {
        attrs.push(Entry::Field(CHUNK_ATTR.to_owned(), Expr::Lit(Lit::Int(id as i128))));
        chunks.iter().any(|c| c.id == id && c.proven)
      });
      entries_mut(attrs).for_each(|e| cover_expr(e, inside, covered));
      children.iter_mut().for_each(|c| cover_tmpl(c, chunks, inside || proven, covered));
    }
    Tmpl::Fragment(children) | Tmpl::Place { children, .. } => children.iter_mut().for_each(|c| cover_tmpl(c, chunks, inside, covered)),
    Tmpl::If { cond, then, r#else } => {
      cover_expr(cond, inside, covered);
      cover_tmpl(then, chunks, inside, covered);
      if let Some(other) = r#else {
        cover_tmpl(other, chunks, inside, covered);
      }
    }
    Tmpl::For { over, body, .. } => {
      cover_expr(over, inside, covered);
      cover_tmpl(body, chunks, inside, covered);
    }
    Tmpl::Let { expr, then, .. } => {
      cover_expr(expr, inside, covered);
      cover_tmpl(then, chunks, inside, covered);
    }
    Tmpl::Component { props, children, .. } => {
      entries_mut(props).for_each(|e| cover_expr(e, inside, covered));
      children.iter_mut().for_each(|c| cover_tmpl(c, chunks, inside, covered));
    }
    Tmpl::Island { children, .. } => children.iter_mut().for_each(|c| cover_tmpl(c, chunks, inside, covered)),
  }
}

fn cover_expr(e: &mut Expr, inside: bool, covered: &mut Vec<u32>) {
  if !inside {
    return;
  }
  e.visit(&mut |x| {
    if let Expr::Hoist { id, .. } = x {
      covered.push(*id);
    }
  });
  strip(e, &[], &mut Vec::new(), true, false);
}

fn entries_mut(entries: &mut [Entry]) -> impl Iterator<Item = &mut Expr> {
  entries.iter_mut().flat_map(|entry| match entry {
    Entry::Field(_, e) | Entry::Item(e) | Entry::Spread(e) => vec![e],
    Entry::Computed(k, v) => vec![k, v],
    Entry::Baked(_) => Vec::new(),
  })
}

/// Where a chunk candidate sits in its component.
#[derive(Debug, Clone, Copy, Default)]
struct Where {
  /// Under an `If` whose condition reads state.
  untaken: bool,
  /// Among the children a component or an island is given.
  slot: bool,
}

fn take_chunk(attrs: &mut Vec<Entry>) -> Option<u32> {
  let at = attrs.iter().position(|entry| matches!(entry, Entry::Field(n, _) if n == CHUNK_ATTR))?;
  let Entry::Field(_, Expr::Lit(Lit::Int(id))) = attrs.remove(at) else { return None };
  Some(id as u32)
}

fn choose(t: &mut Tmpl, tainted: &mut Vec<String>, pure: &HashMap<String, bool>, at: Where, kept: &mut Vec<Chunk>) {
  match t {
    Tmpl::Text(_) | Tmpl::Marker(_) | Tmpl::Outlet { .. } | Tmpl::Html(_) | Tmpl::Expr(_) | Tmpl::Slot(_) | Tmpl::Baked { .. } => {}
    Tmpl::Element { attrs, children, .. } => {
      let id = take_chunk(attrs);
      let whole = Tmpl::Element { tag: String::new(), attrs: attrs.clone(), children: children.clone() };
      if let Some(id) = id {
        if is_static(&whole, tainted, pure) && does_work(&whole) {
          kept.push(Chunk { id, proven: !at.slot });
          attrs.push(Entry::Field(CHUNK_ATTR.to_owned(), Expr::Lit(Lit::Int(id as i128))));
          if at.untaken {
            attrs.push(Entry::Field(UNTAKEN_ATTR.to_owned(), Expr::Lit(Lit::Bool(true))));
          }
          children.iter_mut().for_each(clear_chunks);
          return;
        }
      }
      children.iter_mut().for_each(|c| choose(c, tainted, pure, at, kept));
    }
    Tmpl::Fragment(children) | Tmpl::Place { children, .. } => children.iter_mut().for_each(|c| choose(c, tainted, pure, at, kept)),
    Tmpl::If { cond, then, r#else } => {
      let at = Where { untaken: at.untaken || reads_tainted(cond, tainted), ..at };
      choose(then, tainted, pure, at, kept);
      if let Some(other) = r#else {
        choose(other, tainted, pure, at, kept);
      }
    }
    Tmpl::For { over, body, .. } => match reads_tainted(over, tainted) {
      true => clear_chunks(body),
      false => choose(body, tainted, pure, at, kept),
    },
    Tmpl::Let { name, expr, then } => {
      let depth = tainted.len();
      if reads_tainted(expr, tainted) {
        tainted.push(name.clone());
      }
      choose(then, tainted, pure, at, kept);
      tainted.truncate(depth);
    }
    Tmpl::Component { children, .. } | Tmpl::Island { children, .. } => children.iter_mut().for_each(|c| choose(c, tainted, pure, Where { slot: true, ..at }, kept)),
  }
}

/// Removes every `CHUNK_ATTR` beneath a kept chunk, since the outermost one holds the markup.
fn clear_chunks(t: &mut Tmpl) {
  match t {
    Tmpl::Text(_) | Tmpl::Marker(_) | Tmpl::Outlet { .. } | Tmpl::Html(_) | Tmpl::Expr(_) | Tmpl::Slot(_) | Tmpl::Baked { .. } => {}
    Tmpl::Element { attrs, children, .. } => {
      take_chunk(attrs);
      children.iter_mut().for_each(clear_chunks);
    }
    Tmpl::Fragment(children) | Tmpl::Place { children, .. } => children.iter_mut().for_each(clear_chunks),
    Tmpl::If { then, r#else, .. } => {
      clear_chunks(then);
      if let Some(other) = r#else {
        clear_chunks(other);
      }
    }
    Tmpl::For { body, .. } => clear_chunks(body),
    Tmpl::Let { then, .. } => clear_chunks(then),
    Tmpl::Component { children, .. } | Tmpl::Island { children, .. } => children.iter_mut().for_each(clear_chunks),
  }
}

/// The name the rewrite binds the reader to inside a component.
const READER: &str = "__sfh";

/// The prop an island placement carries its region key on, read by `Island`.
const KEY_PROP: &str = "__sfKey";
/// The import the rewrite adds at the top of a file.
pub const IMPORT: &str = "import { useHoisted as __sfUseHoisted, useBrowser as __sfUseBrowser } from \"@snapfire/fsr-client/react\";\n";

/// One splice: the range replaced (empty for an insertion), the text and the
/// order among edits at one position, lower first.
struct Edit {
  start: usize,
  end: usize,
  rank: u8,
  text: String,
}

/// `source` with every rewrite in `rewrites` applied; all of them are for
/// this one file. Edits are applied from the end so earlier offsets hold; at
/// one position the lower rank is applied first and so ends up after. A
/// proven chunk replaces its element whole, so the edits inside it go.
pub fn apply(source: &str, rewrites: &[&Rewrite]) -> String {
  let mut edits: Vec<Edit> = Vec::new();
  fn insert(edits: &mut Vec<Edit>, at: usize, rank: u8, text: String) {
    edits.push(Edit { start: at, end: at, rank, text });
  }
  let replaced: Vec<Range<usize>> = rewrites.iter().flat_map(|r| r.chunks.iter().filter(|c| c.proven).map(|c| c.range.clone())).collect();
  let inside = |start: usize, end: usize| replaced.iter().any(|r| r.start < start && end <= r.end);
  for rewrite in rewrites {
    let browser_hook = if rewrite.browser.is_empty() { "" } else { " const __sfb = __sfUseBrowser();" };
    match &rewrite.hook {
      Hook::Block { after } => insert(&mut edits, *after, 0, format!(" const {READER} = __sfUseHoisted({});{browser_hook} ", quoted(&rewrite.module))),
      Hook::Expression(range) => {
        insert(&mut edits, range.start, 1, format!("{{ const {READER} = __sfUseHoisted({});{browser_hook} return (", quoted(&rewrite.module)));
        insert(&mut edits, range.end, 0, "); }".to_owned());
      }
    }
    for range in &rewrite.browser {
      if inside(range.start, range.end) {
        continue;
      }
      insert(&mut edits, range.start, 1, "__sfb(() => ".to_owned());
      insert(&mut edits, range.end, 0, ")".to_owned());
    }
    for range in &rewrite.loops {
      if inside(range.start, range.end) {
        continue;
      }
      insert(&mut edits, range.start, 0, format!("{READER}.l("));
      insert(&mut edits, range.end, 0, ")".to_owned());
    }
    for StatementLoop { range, body, pushes } in &rewrite.statement_loops {
      if inside(range.start, range.end) {
        continue;
      }
      let counter = format!("__sfI{}", range.start);
      insert(&mut edits, range.start, 9, format!("{{ let {counter} = 0; "));
      insert(&mut edits, body.start, 9, format!("{{ {READER}.e({counter}++); try {{ "));
      insert(&mut edits, body.end, 1, format!(" }} finally {{ {READER}.x(); }} }}"));
      insert(&mut edits, range.end, 0, " }".to_owned());
      for arg in pushes {
        insert(&mut edits, arg.start, 9, format!("{READER}.w("));
        insert(&mut edits, arg.end, 0, ")".to_owned());
      }
    }
    for ChunkSite { id, range, open, as_child, proven } in &rewrite.chunks {
      let inner = spliced(source, open.clone(), &rewrite.sites);
      let inner = inner.strip_prefix('<').and_then(|s| s.strip_suffix('>')).unwrap_or(&inner).trim_end();
      let hit = format!("{READER}.c({id}, (__sfHtml) => <{inner} dangerouslySetInnerHTML={{__sfHtml}} />");
      match proven {
        true => {
          let (before, after) = if *as_child { ("{", ")}") } else { ("", ")") };
          edits.push(Edit { start: range.start, end: range.end, rank: 0, text: format!("{before}{hit}{after}") });
        }
        false => {
          let (before, after) = if *as_child { ("{", "))}") } else { ("", "))") };
          insert(&mut edits, range.start, 0, format!("{before}{hit}, () => ("));
          insert(&mut edits, range.end, 1, after.to_owned());
        }
      }
    }
    for (id, range) in &rewrite.sites {
      if inside(range.start, range.end) {
        continue;
      }
      edits.push(Edit { start: range.start, end: range.end, rank: 0, text: format!("{READER}.r({id}, () => ({}))", &source[range.clone()]) });
    }
    for (id, at) in &rewrite.islands {
      insert(&mut edits, *at, 0, format!(" {KEY_PROP}={{{READER}.k({id})}}"));
    }
    for (id, range) in &rewrite.ids {
      edits.push(Edit { start: range.start, end: range.end, rank: 0, text: format!("{READER}.id({id})") });
    }
    for (id, range, as_child) in &rewrite.placements {
      let (before, after) = if *as_child { ("{", ")}") } else { ("", ")") };
      insert(&mut edits, range.start, 0, format!("{before}{READER}.p({id}, "));
      insert(&mut edits, range.end, 1, after.to_owned());
    }
  }
  edits.sort_by(|a, b| b.start.cmp(&a.start).then(b.end.cmp(&a.end)).then(a.rank.cmp(&b.rank)));
  let mut out = source.to_owned();
  for edit in edits {
    out.replace_range(edit.start..edit.end, &edit.text);
  }
  format!("{IMPORT}{out}")
}

/// `source[range]` with the value sites inside it replaced, for the hit
/// variant of a chunk's opening tag.
fn spliced(source: &str, range: Range<usize>, sites: &[(u32, Range<usize>)]) -> String {
  let mut out = source[range.clone()].to_owned();
  let mut inside: Vec<&(u32, Range<usize>)> = sites.iter().filter(|(_, r)| r.start >= range.start && r.end <= range.end).collect();
  inside.sort_by(|a, b| b.1.start.cmp(&a.1.start));
  for (id, r) in inside {
    let local = r.start - range.start..r.end - range.start;
    let text = format!("{READER}.r({id}, () => ({}))", &source[r.clone()]);
    out.replace_range(local, &text);
  }
  out
}

fn quoted(text: &str) -> String {
  format!("\"{}\"", text.replace('\\', "\\\\").replace('"', "\\\""))
}
