//! Hoisting: which render-path work the server does for the browser, and the
//! source rewrite that makes the browser read it instead of doing it again.
//!
//! Values: the lowerer wraps every helper call and formatting builtin as a
//! candidate; `decide` keeps a candidate when its inputs are props only. The
//! survivors become `Expr::Hoist` in the plan and a read in the bundle's copy
//! of the module. Subtrees: every element with children is a candidate;
//! `chunks` keeps the outermost ones that are static, so the renderer records
//! their inner markup and the browser hands it to React as the element's
//! inner HTML. Both are keyed by the module, the id and the enclosing loop
//! indices, and both fall back to the original code on a miss.

use std::collections::HashMap;
use std::ops::Range;

use snapfire_fsr_ir::ast::{Component, Entry, Expr, Lit, Stmt, Tmpl};

/// The attribute the lowerer leaves on an element carrying a handler, a
/// `ref` or a spread, which the browser must render itself. Never printed.
pub const BOUND_ATTR: &str = "$bound";
/// The attribute marking an element whose inner markup the renderer records
/// under the id it holds. Never printed.
pub const CHUNK_ATTR: &str = "$chunk";

/// The hoist candidates of one component as the lowerer meets them.
#[derive(Debug, Default)]
pub struct Candidates {
  next: u32,
  /// Each value candidate's id, the byte range of its call in the source and
  /// the arrow ranges of the JSX `.map` callbacks it sits in, outermost first.
  pub sites: Vec<(u32, Range<usize>, Vec<Range<usize>>)>,
  /// Each subtree candidate's id, the element's byte range, its opening tag's
  /// byte range, whether it sits among JSX children and the callbacks it sits in.
  pub chunk_sites: Vec<(u32, Range<usize>, Range<usize>, bool, Vec<Range<usize>>)>,
  /// The `.map` callbacks being lowered, outermost first.
  pub open_loops: Vec<Range<usize>>,
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

  /// The call ranges of the value candidates that stay in the browser: not
  /// kept, and not inside a kept one, whose read covers them.
  pub fn remaining(&self, kept: &[u32]) -> Vec<Range<usize>> {
    let held: Vec<&Range<usize>> = self.sites.iter().filter(|(id, _, _)| kept.contains(id)).map(|(_, range, _)| range).collect();
    self
      .sites
      .iter()
      .filter(|(id, range, _)| !kept.contains(id) && !held.iter().any(|h| h.start <= range.start && range.end <= h.end))
      .map(|(_, range, _)| range.clone())
      .collect()
  }

  /// The rewrite for the candidates in `values` and `chunks`, or `None` when none survived.
  pub fn rewrite(self, values: &[u32], chunks: &[u32], file: &str, module: &str, hook: Hook) -> Option<Rewrite> {
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
      if chunks.contains(&id) {
        chunk_sites.push((id, range, open, as_child));
        remember(enclosing);
      }
    }
    if sites.is_empty() && chunk_sites.is_empty() {
      return None;
    }
    Some(Rewrite { file: file.to_owned(), module: module.to_owned(), hook, sites, chunks: chunk_sites, loops })
  }
}

/// Where the reader hook goes: after the `{` of a block body, or around an
/// arrow's expression body, which becomes a block returning it.
#[derive(Debug, Clone, PartialEq)]
pub enum Hook {
  Block { after: usize },
  Expression(Range<usize>),
}

/// The rewrite of one component's source for the bundle.
#[derive(Debug, Clone, PartialEq)]
pub struct Rewrite {
  pub file: String,
  pub module: String,
  pub hook: Hook,
  /// The surviving value candidates: id and call range.
  pub sites: Vec<(u32, Range<usize>)>,
  /// The surviving subtree candidates: id, element range, opening tag range
  /// and whether the element sits among JSX children.
  pub chunks: Vec<(u32, Range<usize>, Range<usize>, bool)>,
  /// The `.map` callbacks whose bodies the survivors sit in, as arrow ranges.
  pub loops: Vec<Range<usize>>,
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
    Tmpl::Text(_) | Tmpl::Slot(_) => {}
    Tmpl::Expr(e) => strip(e, tainted, kept, false, false),
    Tmpl::Element { attrs, children, .. } => {
      entries(attrs, tainted, kept);
      children.iter_mut().for_each(|c| tmpl(c, tainted, kept));
    }
    Tmpl::Fragment(children) => children.iter_mut().for_each(|c| tmpl(c, tainted, kept)),
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
    if matches!(e, Expr::Store(_) | Expr::Now | Expr::Param(_) | Expr::Query(_) | Expr::Session(_) | Expr::Identity(_) | Expr::Input | Expr::Call { .. }) {
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
    Expr::Param(_) | Expr::Query(_) | Expr::Session(_) | Expr::Store(_) | Expr::Identity(_) | Expr::Locale | Expr::Input | Expr::Now | Expr::Var(_) | Expr::Lit(_) => {}
    Expr::Call { args, .. } => args.iter_mut().for_each(|(_, e)| strip(e, tainted, kept, in_lambda, in_hoist)),
    Expr::Object(entries) | Expr::Array(entries) => entries.iter_mut().for_each(|entry| match entry {
      Entry::Field(_, e) | Entry::Item(e) | Entry::Spread(e) => strip(e, tainted, kept, in_lambda, in_hoist),
      Entry::Computed(k, v) => {
        strip(k, tainted, kept, in_lambda, in_hoist);
        strip(v, tainted, kept, in_lambda, in_hoist);
      }
    }),
    Expr::Field(e, _) | Expr::Not(e) | Expr::Entries(e) | Expr::Keys(e) | Expr::Values(e) | Expr::Length(e) | Expr::Str(e) | Expr::Num(e) | Expr::BigInt(e) => strip(e, tainted, kept, in_lambda, in_hoist),
    Expr::Index(a, b) | Expr::Arith(_, a, b) | Expr::Compare(_, a, b) | Expr::Logic(_, a, b) | Expr::Coalesce(a, b) | Expr::Map(a, b) | Expr::Filter(a, b) | Expr::Find(a, b) | Expr::FindIndex(a, b) | Expr::Some(a, b) | Expr::Every(a, b) => {
      strip(a, tainted, kept, in_lambda, in_hoist);
      strip(b, tainted, kept, in_lambda, in_hoist);
    }
    Expr::Ternary(a, b, c) | Expr::Reduce(a, b, c) => {
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
  }
}

fn entry_exprs(entries: &[Entry]) -> impl Iterator<Item = &Expr> {
  entries.iter().flat_map(|entry| match entry {
    Entry::Field(_, e) | Entry::Item(e) | Entry::Spread(e) => vec![e],
    Entry::Computed(k, v) => vec![k, v],
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
    Tmpl::Text(_) => true,
    Tmpl::Expr(e) => !reads_tainted(e, tainted),
    Tmpl::Element { tag, attrs, children } => {
      tag != "sf-s" && !has_attr(attrs, BOUND_ATTR) && entry_exprs(attrs).all(|e| !reads_tainted(e, tainted)) && children.iter().all(|c| is_static(c, tainted, pure))
    }
    Tmpl::Fragment(children) => children.iter().all(|c| is_static(c, tainted, pure)),
    Tmpl::If { cond, then, r#else } => !reads_tainted(cond, tainted) && is_static(then, tainted, pure) && r#else.as_ref().is_none_or(|e| is_static(e, tainted, pure)),
    Tmpl::For { over, body, .. } => !reads_tainted(over, tainted) && is_static(body, tainted, pure),
    Tmpl::Let { expr, then, .. } => !reads_tainted(expr, tainted) && is_static(then, tainted, pure),
    Tmpl::Component { module, props, children } => {
      pure.get(module).copied().unwrap_or(false) && entry_exprs(props).all(|e| !reads_tainted(e, tainted)) && children.iter().all(|c| is_static(c, tainted, pure))
    }
    Tmpl::Island { .. } | Tmpl::Slot(_) => false,
  }
}

/// True when React would compute something in the subtree: an interpolation,
/// a branch, a loop, a binding, a component or an attribute that is not a
/// literal. A subtree of literal markup is left to React, which costs nothing.
fn does_work(t: &Tmpl) -> bool {
  match t {
    Tmpl::Text(_) | Tmpl::Slot(_) => false,
    Tmpl::Expr(_) | Tmpl::If { .. } | Tmpl::For { .. } | Tmpl::Let { .. } | Tmpl::Component { .. } | Tmpl::Island { .. } => true,
    Tmpl::Element { attrs, children, .. } => {
      attrs.iter().any(|entry| match entry {
        Entry::Field(name, Expr::Lit(_)) => name == CHUNK_ATTR && false,
        Entry::Field(name, _) if name == CHUNK_ATTR || name == BOUND_ATTR => false,
        _ => true,
      }) || children.iter().any(does_work)
    }
    Tmpl::Fragment(children) => children.iter().any(does_work),
  }
}

/// Keeps the outermost static subtrees of `component` that do work, removes
/// every other `CHUNK_ATTR` and returns the ids kept.
pub fn chunks(component: &mut Component, state: &[String], pure: &HashMap<String, bool>) -> Vec<u32> {
  let mut tainted: Vec<String> = state.to_vec();
  for stmt in &component.body {
    let Stmt::Let { name, expr } = stmt else { continue };
    if reads_tainted(expr, &tainted) {
      tainted.push(name.clone());
    }
  }
  let mut kept = Vec::new();
  choose(&mut component.render, &mut tainted, pure, &mut kept);
  kept
}

fn take_chunk(attrs: &mut Vec<Entry>) -> Option<u32> {
  let at = attrs.iter().position(|entry| matches!(entry, Entry::Field(n, _) if n == CHUNK_ATTR))?;
  let Entry::Field(_, Expr::Lit(Lit::Int(id))) = attrs.remove(at) else { return None };
  Some(id as u32)
}

fn choose(t: &mut Tmpl, tainted: &mut Vec<String>, pure: &HashMap<String, bool>, kept: &mut Vec<u32>) {
  match t {
    Tmpl::Text(_) | Tmpl::Expr(_) | Tmpl::Slot(_) => {}
    Tmpl::Element { attrs, children, .. } => {
      let id = take_chunk(attrs);
      let whole = Tmpl::Element { tag: String::new(), attrs: attrs.clone(), children: children.clone() };
      if let Some(id) = id {
        if is_static(&whole, tainted, pure) && does_work(&whole) {
          kept.push(id);
          attrs.push(Entry::Field(CHUNK_ATTR.to_owned(), Expr::Lit(Lit::Int(id as i128))));
          children.iter_mut().for_each(clear_chunks);
          return;
        }
      }
      children.iter_mut().for_each(|c| choose(c, tainted, pure, kept));
    }
    Tmpl::Fragment(children) => children.iter_mut().for_each(|c| choose(c, tainted, pure, kept)),
    Tmpl::If { then, r#else, .. } => {
      choose(then, tainted, pure, kept);
      if let Some(other) = r#else {
        choose(other, tainted, pure, kept);
      }
    }
    Tmpl::For { over, params, body } => {
      let depth = tainted.len();
      if reads_tainted(over, tainted) {
        tainted.extend(params.iter().cloned());
      }
      choose(body, tainted, pure, kept);
      tainted.truncate(depth);
    }
    Tmpl::Let { name, expr, then } => {
      let depth = tainted.len();
      if reads_tainted(expr, tainted) {
        tainted.push(name.clone());
      }
      choose(then, tainted, pure, kept);
      tainted.truncate(depth);
    }
    Tmpl::Component { children, .. } | Tmpl::Island { children, .. } => children.iter_mut().for_each(|c| choose(c, tainted, pure, kept)),
  }
}

/// Removes every `CHUNK_ATTR` beneath a kept chunk, since the outermost one holds the markup.
fn clear_chunks(t: &mut Tmpl) {
  match t {
    Tmpl::Text(_) | Tmpl::Expr(_) | Tmpl::Slot(_) => {}
    Tmpl::Element { attrs, children, .. } => {
      take_chunk(attrs);
      children.iter_mut().for_each(clear_chunks);
    }
    Tmpl::Fragment(children) => children.iter_mut().for_each(clear_chunks),
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
/// The import the rewrite adds at the top of a file.
pub const IMPORT: &str = "import { useHoisted as __sfUseHoisted } from \"@snapfire/fsr-client/react\";\n";

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
/// one position the lower rank is applied first and so ends up after.
pub fn apply(source: &str, rewrites: &[&Rewrite]) -> String {
  let mut edits: Vec<Edit> = Vec::new();
  fn insert(edits: &mut Vec<Edit>, at: usize, rank: u8, text: String) {
    edits.push(Edit { start: at, end: at, rank, text });
  }
  for rewrite in rewrites {
    match &rewrite.hook {
      Hook::Block { after } => insert(&mut edits, *after, 0, format!(" const {READER} = __sfUseHoisted({}); ", quoted(&rewrite.module))),
      Hook::Expression(range) => {
        insert(&mut edits, range.start, 1, format!("{{ const {READER} = __sfUseHoisted({}); return (", quoted(&rewrite.module)));
        insert(&mut edits, range.end, 0, "); }".to_owned());
      }
    }
    for range in &rewrite.loops {
      insert(&mut edits, range.start, 0, format!("{READER}.l("));
      insert(&mut edits, range.end, 0, ")".to_owned());
    }
    for (id, range, open, as_child) in &rewrite.chunks {
      let inner = spliced(source, open.clone(), &rewrite.sites);
      let inner = inner.strip_prefix('<').and_then(|s| s.strip_suffix('>')).unwrap_or(&inner).trim_end();
      let (before, after) = if *as_child { ("{", "))}") } else { ("", "))") };
      insert(&mut edits, range.start, 0, format!("{before}{READER}.c({id}, (__sfHtml) => <{inner} dangerouslySetInnerHTML={{__sfHtml}} />, () => ("));
      insert(&mut edits, range.end, 1, after.to_owned());
    }
    for (id, range) in &rewrite.sites {
      edits.push(Edit { start: range.start, end: range.end, rank: 0, text: format!("{READER}.r({id}, () => ({}))", &source[range.clone()]) });
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
