use std::collections::HashMap;

use futures_util::stream::{self, FuturesUnordered, Stream, StreamExt};
use serde_json::{Value as Json, json};
use snapfire_fsr_core::Node;
use snapfire_fsr_payload::{FORMAT_VERSION, HtmlSession, node_to_row_json};

use crate::assembler::{Assembly, PendingResolution};
use crate::meta::Meta;
use crate::segments::SegmentInfo;

/// Installed once, ahead of the first fill. Moves a resolved template's content
/// into its slot and wakes the boot runtime to rescan.
pub const FILL_SCRIPT: &str = "<script>function __sfFill(n){var t=document.querySelector('template[data-sf-fill=\"'+n+'\"]'),s=document.querySelector('[data-sf-slot=\"'+n+'\"]');if(t&&s){s.replaceWith(t.content);t.remove();document.dispatchEvent(new CustomEvent('sf:fill',{detail:n}))}}function __sfHead(h){if(h.title!=null)document.title=h.title;if(h.description!=null){var m=document.querySelector('meta[name=\"description\"]');if(!m){m=document.createElement('meta');m.name='description';document.head.appendChild(m)}m.content=h.description}}function __sfStore(o){var g=window;if(g.__sfSeedApply){g.__sfSeedApply(o)}else{g.__sfSeed=Object.assign(g.__sfSeed||{},o)}}</script>";

/// The `T` row's body: the store keys a route seeded, as a value map.
pub fn seed_to_json(seed: &snapfire_fsr_core::Data) -> Json {
  snapfire_fsr_payload::value_to_json(&snapfire_fsr_core::Value::Map(seed.clone()))
}

/// The `H` row's body: only the fields a segment set, so a reader leaves
/// the rest alone.
pub fn meta_to_json(meta: &Meta) -> Json {
  let mut obj = serde_json::Map::new();
  if let Some(title) = &meta.title {
    obj.insert("title".to_owned(), json!(title));
  }
  if let Some(description) = &meta.description {
    obj.insert("description".to_owned(), json!(description));
  }
  Json::Object(obj)
}

pub fn segments_to_json(info: &SegmentInfo) -> Json {
  let mut obj = serde_json::Map::new();
  obj.insert("k".to_owned(), json!(info.key));
  if info.digest != 0 {
    obj.insert("d".to_owned(), json!(format!("{:016x}", info.digest)));
  }
  if !info.name.is_empty() {
    obj.insert("n".to_owned(), json!(info.name));
  }
  if let Some(slot) = info.slot {
    obj.insert("s".to_owned(), json!(slot));
  } else {
    obj.insert("p".to_owned(), json!(info.path));
  }
  obj.insert(
    "c".to_owned(),
    Json::Array(info.children.iter().map(segments_to_json).collect()),
  );
  if !info.keep.is_empty() {
    obj.insert("keep".to_owned(), json!(info.keep));
  }
  Json::Object(obj)
}

/// `-` and `%` are escaped so a key can never contain `--` and close the
/// HTML comment that delimits its region.
fn escape_key(key: &str) -> String {
  key.replace('%', "%25").replace('-', "%2D")
}

struct PendingSet {
  set: FuturesUnordered<futures_util::future::BoxFuture<'static, crate::assembler::Resolved>>,
}

impl PendingSet {
  fn new(pending: Vec<PendingResolution>) -> Self {
    let set = FuturesUnordered::new();
    for p in pending {
      set.push(p.future);
    }
    Self { set }
  }
}

/// The wire encoding of a streamed response: a `V` row, the `N` tree row, an
/// `H` row when the document has a title or a description, a `T` row when the
/// route seeds the store, an `L` row naming the locale when the request has
/// one, an `E` row naming a module the browser must load for the response's
/// islands when the head names one, a `D` row carrying the locale's message
/// catalog as a JSON object when the head holds one, a `C` row naming the
/// stylesheets this response needs beyond the document's own, then the `G` segment
/// sidecar row, which closes the eager wave: a navigator applies the tree the
/// moment it reads `G`. Then one `S` row per resolution in completion order,
/// each followed by an `H` row when the resolved segment described the
/// document and a `T` row when it seeded the store. A resolution may
/// introduce new slots, which join the set.
pub fn wire_stream(assembly: Assembly) -> impl Stream<Item = String> + Send {
  let mut header = format!(
    "V {}\nN {}\n",
    json!({ "fmt": FORMAT_VERSION, "enc": "json" }),
    node_to_row_json(&assembly.tree)
  );
  if !assembly.meta.is_empty() {
    header.push_str(&format!("H {}\n", meta_to_json(&assembly.meta)));
  }
  if !assembly.store.is_empty() {
    header.push_str(&format!("T {}\n", seed_to_json(&assembly.store)));
  }
  if !assembly.locale.tag.is_empty() {
    header.push_str(&format!("L {}\n", json!(assembly.locale.tag)));
  }
  if let Some(entry) = &assembly.entry {
    header.push_str(&format!("E {}\n", json!(entry)));
  }
  if !assembly.styles.is_empty() {
    header.push_str(&format!("C {}\n", json!(assembly.styles)));
  }
  if let Some(catalog) = &assembly.catalog {
    header.push_str(&format!("D {catalog}\n"));
  }
  header.push_str(&format!("G {}\n", segments_to_json(&assembly.segments)));
  let pending = PendingSet::new(assembly.pending);

  stream::once(async move { header }).chain(stream::unfold(pending, |mut state| async move {
    let resolved = state.set.next().await?;
    tracing::debug!(target: "fsr::stream", slot = resolved.slot.0, "slot resolved");
    for p in resolved.pending {
      state.set.push(p.future);
    }
    let mut row = format!("S {} {}\n", resolved.slot.0, node_to_row_json(&resolved.node));
    if !resolved.meta.is_empty() {
      row.push_str(&format!("H {}\n", meta_to_json(&resolved.meta)));
    }
    if !resolved.store.is_empty() {
      row.push_str(&format!("T {}\n", seed_to_json(&resolved.store)));
    }
    Some((row, state))
  }))
}

/// Serializes a segment's subtree wrapped in its comment delimiters, recursing
/// into child segments at their sidecar positions. Slot-addressed (deferred)
/// children are skipped: their DOM region is the `data-sf-slot` element.
fn write_segment(session: &mut HtmlSession, node: &Node, info: &SegmentInfo, out: &mut String) {
  out.push_str(&format!("<!--sf-g:{}-->", escape_key(&info.key)));

  if let Some(inner) = info.children.iter().find(|c| c.slot.is_none() && c.path.is_empty()) {
    write_segment(session, node, inner, out);
  } else {
    let positioned: Vec<(&[u32], &SegmentInfo)> = info
      .children
      .iter()
      .filter(|c| c.slot.is_none() && !c.path.is_empty())
      .map(|c| (c.path.as_slice(), c))
      .collect();
    write_positioned(session, node, &positioned, out);
  }

  out.push_str("<!--/sf-g-->");
}

/// Writes `node` with each positioned child segment wrapped at its path,
/// descending through `Seq` items and an island's children alike.
fn write_positioned(session: &mut HtmlSession, node: &Node, positioned: &[(&[u32], &SegmentInfo)], out: &mut String) {
  if positioned.is_empty() {
    out.push_str(&session.serialize(node));
    return;
  }
  let write_items = |session: &mut HtmlSession, items: &[Node], out: &mut String| {
    for (idx, item) in items.iter().enumerate() {
      let here: Vec<(&[u32], &SegmentInfo)> = positioned
        .iter()
        .filter(|(p, _)| p[0] == idx as u32)
        .map(|(p, c)| (&p[1..], *c))
        .collect();
      match here.iter().find(|(p, _)| p.is_empty()) {
        Some((_, child)) => write_segment(session, item, child, out),
        None => write_positioned(session, item, &here, out),
      }
    }
  };
  match node {
    Node::Seq(items) => write_items(session, items, out),
    Node::Client {
      module,
      props,
      children,
      ssr: None,
    } => {
      let (open, close) = session.client_wrapper(module, props);
      out.push_str(&open);
      write_items(session, children, out);
      out.push_str(&close);
    }
    _ => out.push_str(&session.serialize(node)),
  }
}

/// The first-response encoding of a streamed page: the tree with segment
/// delimiters, the sidecar as an inert script for the navigator, fallbacks in
/// place and the fill script, then one inert template plus fill call per
/// resolution. Island ids stay unique across the whole response because one
/// `HtmlSession` spans it.
pub fn html_stream(assembly: Assembly) -> impl Stream<Item = String> + Send {
  let mut session = HtmlSession::new();
  let mut first = String::new();
  write_segment(&mut session, &assembly.tree, &assembly.segments, &mut first);
  first.push_str(&format!(
    "<script type=\"application/json\" data-sf-segments>{}</script>",
    segments_to_json(&assembly.segments).to_string().replace('<', "\\u003c")
  ));
  if !assembly.store.is_empty() {
    first.push_str(&format!(
      "<script type=\"application/json\" data-sf-store>{}</script>",
      seed_to_json(&assembly.store).to_string().replace('<', "\\u003c")
    ));
  }
  if !assembly.pending.is_empty() {
    first.push_str(FILL_SCRIPT);
  }
  let state = HtmlState {
    pending: PendingSet::new(assembly.pending),
    session,
  };

  stream::once(async move { first }).chain(stream::unfold(state, |mut state| async move {
    let resolved = state.pending.set.next().await?;
    for p in resolved.pending {
      state.pending.set.push(p.future);
    }
    let slot = resolved.slot.0;
    let body = state.session.serialize(&resolved.node);
    let mut chunk = format!(
      "<template data-sf-fill=\"{slot}\"><!--sf-g:{}-->{body}<!--/sf-g--></template><script>__sfFill({slot})",
      escape_key(&resolved.key)
    );
    if !resolved.meta.is_empty() {
      chunk.push_str(&format!(
        ";__sfHead({})",
        meta_to_json(&resolved.meta).to_string().replace('<', "\\u003c")
      ));
    }
    if !resolved.store.is_empty() {
      chunk.push_str(&format!(
        ";__sfStore({})",
        seed_to_json(&resolved.store).to_string().replace('<', "\\u003c")
      ));
    }
    chunk.push_str("</script>");
    Some((chunk, state))
  }))
}

struct HtmlState {
  pending: PendingSet,
  session: HtmlSession,
}

/// The slot a layout's `children` fill, which is the chain from the shell down to the page.
const CONTENT: &str = "content";

/// One segment of a route as markup with nothing around it: the page when
/// `slot` is `None`, otherwise the parallel slot it names, wherever that slot
/// sits on the route. Every deferred segment inside it is resolved first, so
/// nothing streams and no fallback is written; there are no segment
/// delimiters and no sidecar. The store the route seeded follows the markup as
/// the same inert script a document carries, so whatever swaps the fragment in
/// can adopt it. `None` when the route has no slot by that name.
pub async fn fragment_html(assembly: Assembly, slot: Option<&str>) -> Option<String> {
  let Assembly { tree, pending, segments, mut store, .. } = assembly;
  let mut fills: HashMap<u32, Node> = HashMap::new();
  let mut set = PendingSet::new(pending);
  while let Some(resolved) = set.set.next().await {
    for p in resolved.pending {
      set.set.push(p.future);
    }
    store.extend(resolved.store);
    fills.insert(resolved.slot.0, resolved.node);
  }
  let target = match slot {
    None => page_node(&tree, &segments, &fills)?,
    Some(name) => slot_node(&tree, &segments, &fills, name)?,
  };
  let mut target = target.clone();
  fill(&mut target, &fills);
  let mut out = HtmlSession::new().serialize(&target);
  if !store.is_empty() {
    out.push_str(&format!(
      "<script type=\"application/json\" data-sf-store>{}</script>",
      seed_to_json(&store).to_string().replace('<', "\\u003c")
    ));
  }
  Some(out)
}

/// Where a child segment's subtree is: at its sidecar path inside the parent's
/// node or among the fills when it was deferred.
fn segment_node<'a>(parent: &'a Node, info: &SegmentInfo, fills: &'a HashMap<u32, Node>) -> Option<&'a Node> {
  match info.slot {
    Some(id) => fills.get(&id),
    None => node_at(parent, &info.path),
  }
}

/// The same addressing `write_positioned` walks: an index steps into a `Seq`
/// item or an island's child.
fn node_at<'a>(node: &'a Node, path: &[u32]) -> Option<&'a Node> {
  let Some((&first, rest)) = path.split_first() else { return Some(node) };
  let items = match node {
    Node::Seq(items) => items,
    Node::Client { children, ssr: None, .. } => children,
    _ => return None,
  };
  node_at(items.get(first as usize)?, rest)
}

/// The innermost segment reached through `content` slots.
fn page_node<'a>(node: &'a Node, info: &SegmentInfo, fills: &'a HashMap<u32, Node>) -> Option<&'a Node> {
  match info.children.iter().find(|c| c.name == CONTENT) {
    Some(child) => page_node(segment_node(node, child, fills)?, child, fills),
    None => Some(node),
  }
}

/// The first segment named `name`, depth first in sidecar order.
fn slot_node<'a>(node: &'a Node, info: &SegmentInfo, fills: &'a HashMap<u32, Node>, name: &str) -> Option<&'a Node> {
  for child in &info.children {
    let Some(child_node) = segment_node(node, child, fills) else { continue };
    if child.name == name {
      return Some(child_node);
    }
    if let Some(found) = slot_node(child_node, child, fills, name) {
      return Some(found);
    }
  }
  None
}

/// Replaces every `Pending` under `node` with what resolved for its slot, recursing into what it put there.
fn fill(node: &mut Node, fills: &HashMap<u32, Node>) {
  match node {
    Node::Pending { slot, .. } => {
      if let Some(resolved) = fills.get(&slot.0) {
        *node = resolved.clone();
        fill(node, fills);
      }
    }
    Node::Seq(items) => items.iter_mut().for_each(|item| fill(item, fills)),
    Node::Client { children, ssr, .. } => {
      children.iter_mut().for_each(|child| fill(child, fills));
      if let Some(ssr) = ssr {
        fill(ssr, fills);
      }
    }
    Node::Text(_) | Node::Raw(_) | Node::Slot(_) => {}
  }
}
