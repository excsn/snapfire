use futures_util::stream::{self, FuturesUnordered, Stream, StreamExt};
use serde_json::{json, Value as Json};
use snapfire_fsr_core::Node;
use snapfire_fsr_payload::{node_to_row_json, HtmlSession, FORMAT_VERSION};

use crate::assembler::{Assembly, PendingResolution};
use crate::meta::Meta;
use crate::segments::SegmentInfo;

/// Installed once, ahead of the first fill. Moves a resolved template's content
/// into its slot and wakes the boot runtime to rescan.
pub const FILL_SCRIPT: &str = "<script>function __sfFill(n){var t=document.querySelector('template[data-sf-fill=\"'+n+'\"]'),s=document.querySelector('[data-sf-slot=\"'+n+'\"]');if(t&&s){s.replaceWith(t.content);t.remove();document.dispatchEvent(new CustomEvent('sf:fill',{detail:n}))}}function __sfHead(h){if(h.title!=null)document.title=h.title;if(h.description!=null){var m=document.querySelector('meta[name=\"description\"]');if(!m){m=document.createElement('meta');m.name='description';document.head.appendChild(m)}m.content=h.description}}</script>";

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
  if let Some(slot) = info.slot {
    obj.insert("s".to_owned(), json!(slot));
  } else {
    obj.insert("p".to_owned(), json!(info.path));
  }
  obj.insert("c".to_owned(), Json::Array(info.children.iter().map(segments_to_json).collect()));
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

/// The wire encoding of a streamed response: a `V` row, the `N` tree row, the
/// `G` segment sidecar row, an `H` row when the document has a title or a
/// description, then one `S` row per resolution in completion order, each
/// followed by an `H` row when the resolved segment described the document.
/// A resolution may introduce new slots, which join the set.
pub fn wire_stream(assembly: Assembly) -> impl Stream<Item = String> + Send {
  let mut header = format!(
    "V {}\nN {}\nG {}\n",
    json!({ "fmt": FORMAT_VERSION, "enc": "json" }),
    node_to_row_json(&assembly.tree),
    segments_to_json(&assembly.segments)
  );
  if !assembly.meta.is_empty() {
    header.push_str(&format!("H {}\n", meta_to_json(&assembly.meta)));
  }
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
      let here: Vec<(&[u32], &SegmentInfo)> = positioned.iter().filter(|(p, _)| p[0] == idx as u32).map(|(p, c)| (&p[1..], *c)).collect();
      match here.iter().find(|(p, _)| p.is_empty()) {
        Some((_, child)) => write_segment(session, item, child, out),
        None => write_positioned(session, item, &here, out),
      }
    }
  };
  match node {
    Node::Seq(items) => write_items(session, items, out),
    Node::Client { module, props, children, ssr: None } => {
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
  if !assembly.pending.is_empty() {
    first.push_str(FILL_SCRIPT);
  }
  let state = HtmlState { pending: PendingSet::new(assembly.pending), session };

  stream::once(async move { first }).chain(stream::unfold(state, |mut state| async move {
    let resolved = state.pending.set.next().await?;
    for p in resolved.pending {
      state.pending.set.push(p.future);
    }
    let slot = resolved.slot.0;
    let body = state.session.serialize(&resolved.node);
    let mut chunk = format!("<template data-sf-fill=\"{slot}\"><!--sf-g:{}-->{body}<!--/sf-g--></template><script>__sfFill({slot})", escape_key(&resolved.key));
    if !resolved.meta.is_empty() {
      chunk.push_str(&format!(";__sfHead({})", meta_to_json(&resolved.meta).to_string().replace('<', "\\u003c")));
    }
    chunk.push_str("</script>");
    Some((chunk, state))
  }))
}

struct HtmlState {
  pending: PendingSet,
  session: HtmlSession,
}
