use futures_util::stream::{self, FuturesUnordered, Stream, StreamExt};
use serde_json::{json, Value as Json};
use snapfire_fsr_core::Node;
use snapfire_fsr_payload::{node_to_row_json, HtmlSession, FORMAT_VERSION};

use crate::assembler::{Assembly, PendingResolution};
use crate::segments::SegmentInfo;

/// Installed once, ahead of the first fill. Moves a resolved template's content
/// into its slot and wakes the boot runtime to rescan.
pub const FILL_SCRIPT: &str = "<script>function __sfFill(n){var t=document.querySelector('template[data-sf-fill=\"'+n+'\"]'),s=document.querySelector('[data-sf-slot=\"'+n+'\"]');if(t&&s){s.replaceWith(t.content);t.remove();document.dispatchEvent(new CustomEvent('sf:fill',{detail:n}))}}</script>";

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
/// `G` segment sidecar row, then one `S` row per resolution in completion
/// order. A resolution may introduce new slots, which join the set.
pub fn wire_stream(assembly: Assembly) -> impl Stream<Item = String> + Send {
  let header = format!(
    "V {}\nN {}\nG {}\n",
    json!({ "fmt": FORMAT_VERSION, "enc": "json" }),
    node_to_row_json(&assembly.tree),
    segments_to_json(&assembly.segments)
  );
  let pending = PendingSet::new(assembly.pending);

  stream::once(async move { header }).chain(stream::unfold(pending, |mut state| async move {
    let resolved = state.set.next().await?;
    tracing::debug!(target: "fsr::stream", slot = resolved.slot.0, "slot resolved");
    for p in resolved.pending {
      state.set.push(p.future);
    }
    let row = format!("S {} {}\n", resolved.slot.0, node_to_row_json(&resolved.node));
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
    let positioned: Vec<(&u32, &SegmentInfo)> = info
      .children
      .iter()
      .filter(|c| c.slot.is_none())
      .filter_map(|c| c.path.first().map(|i| (i, c)))
      .collect();
    match node {
      Node::Seq(items) if !positioned.is_empty() => {
        for (idx, item) in items.iter().enumerate() {
          match positioned.iter().find(|(i, _)| **i == idx as u32) {
            Some((_, child)) => write_segment(session, item, child, out),
            None => out.push_str(&session.serialize(item)),
          }
        }
      }
      _ => out.push_str(&session.serialize(node)),
    }
  }

  out.push_str("<!--/sf-g-->");
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
    let chunk = format!("<template data-sf-fill=\"{slot}\">{body}</template><script>__sfFill({slot})</script>");
    Some((chunk, state))
  }))
}

struct HtmlState {
  pending: PendingSet,
  session: HtmlSession,
}
