//! A segment another host renders. The assembler asks a [`RemoteSegment`]
//! for the subtree's eager wave and splices it in place of the plan node;
//! the remote's deferred slots stream through under slot ids this assembly
//! allocates, so the browser never sees two hosts.

use std::collections::{HashMap, VecDeque};
use std::sync::Arc;

use futures_util::future::BoxFuture;
use futures_util::lock::Mutex as AsyncMutex;
use futures_util::stream::{self, BoxStream, StreamExt};
use parking_lot::Mutex;
use snapfire_fsr_core::ext::FailureKind;
use snapfire_fsr_core::Fingerprint;
use snapfire_fsr_core::{Node, PlanNode, SlotId};
use snapfire_fsr_payload::DecodeError;

use crate::assembler::{error_node, AssembleError, PendingResolution, Resolved, Session};
use crate::ctx::RequestCtx;
use crate::data::LoadError;
use crate::meta::Meta;
use crate::segments::SegmentInfo;
use crate::store::Contribution;
use crate::wire::Row;

/// What renders a plan node in another process. `fetch` is one call per
/// request that reaches the node; the wave it answers with is spliced in.
pub trait RemoteSegment: Send + Sync {
  fn fetch(&self, ctx: &RequestCtx) -> BoxFuture<'static, Result<RemoteWave, LoadError>>;
}

/// A remote's eager wave: the subtree under its document's content slot,
/// that subtree's sidecar entry, what it said about the document and seeded,
/// the module and stylesheets its islands need and its fills as they come.
pub struct RemoteWave {
  pub node: Node,
  pub segment: SegmentInfo,
  pub meta: Meta,
  /// Paths relative to the content subtree, the remote document's own slot
  /// already stripped.
  pub contributions: Vec<Contribution>,
  pub entry: Option<String>,
  pub styles: Vec<String>,
  pub fills: BoxStream<'static, Result<RemoteFill, LoadError>>,
}

/// One resolution the remote streamed, under the remote's own slot id.
pub struct RemoteFill {
  pub slot: u32,
  pub node: Node,
  pub segments: Vec<SegmentInfo>,
  pub meta: Meta,
  pub contributions: Vec<Contribution>,
}

/// The slot a remote's page sits in under its own document.
const CONTENT: &str = "content";

/// The node `path` reaches inside `node`: an index steps into a `Seq`'s items
/// or an island's children, as a sidecar path does.
pub fn node_at<'a>(node: &'a Node, path: &[u32]) -> Option<&'a Node> {
  let mut at = node;
  for index in path {
    at = match at {
      Node::Seq(items) => items.get(*index as usize)?,
      Node::Client { children, .. } => children.get(*index as usize)?,
      _ => return None,
    };
  }
  Some(at)
}

fn strip_content(mut contributions: Vec<Contribution>) -> Vec<Contribution> {
  for contribution in &mut contributions {
    if contribution.path.first().is_some_and(|slot| slot == CONTENT) {
      contribution.path.remove(0);
    }
  }
  contributions
}

impl RemoteWave {
  /// The wave out of a remote host's payload rows: the eager rows up to the
  /// sidecar, then its fills. `source` names the remote in an error. A remote
  /// whose root has no content segment or whose rows do not read is an
  /// internal failure; the transport's are the caller's to name.
  pub async fn from_rows(source: &str, mut rows: BoxStream<'static, Result<Row, DecodeError>>) -> Result<Self, LoadError> {
    let fail = |message: String| LoadError::new(source, message);
    let mut tree = None;
    let mut meta = Meta::default();
    let mut contributions = Vec::new();
    let mut entry = None;
    let mut styles = Vec::new();
    let segments = loop {
      match rows.next().await {
        None => return Err(fail("the answer ended before its segment sidecar".to_owned())),
        Some(Err(e)) => return Err(fail(e.to_string())),
        Some(Ok(Row::Tree(node))) => tree = Some(node),
        Some(Ok(Row::Head(head))) => meta.merge(head),
        Some(Ok(Row::Store(seeded))) => contributions.extend(seeded),
        Some(Ok(Row::Entry(module))) => entry = Some(module),
        Some(Ok(Row::Styles(urls))) => styles.extend(urls),
        Some(Ok(Row::Segments(root))) => break root,
        Some(Ok(Row::Fill { .. })) => return Err(fail("a fill arrived before the segment sidecar".to_owned())),
        Some(Ok(_)) => {}
      }
    };
    let tree = tree.ok_or_else(|| fail("the answer carries no tree".to_owned()))?;
    let segment = segments
      .children
      .into_iter()
      .find(|child| child.name == CONTENT)
      .ok_or_else(|| fail("the remote's document has no content segment".to_owned()))?;
    let node = node_at(&tree, &segment.path)
      .cloned()
      .ok_or_else(|| fail("the content segment's path reaches nothing in the tree".to_owned()))?;
    let fills = group_fills(source.to_owned(), rows);
    Ok(Self { node, segment, meta, contributions: strip_content(contributions), entry, styles, fills })
  }
}

struct Grouping {
  source: String,
  rows: BoxStream<'static, Result<Row, DecodeError>>,
  held: Option<RemoteFill>,
  seeded: Vec<Contribution>,
  out: VecDeque<Result<RemoteFill, LoadError>>,
  done: bool,
}

impl Grouping {
  fn release(&mut self) {
    if let Some(held) = self.held.take() {
      self.out.push_back(Ok(held));
    }
  }
}

/// The rows after the sidecar as fills: a `T` row seeds the `S` row after
/// it and an `H` row describes the `S` row before it.
fn group_fills(source: String, rows: BoxStream<'static, Result<Row, DecodeError>>) -> BoxStream<'static, Result<RemoteFill, LoadError>> {
  let grouping = Grouping { source, rows, held: None, seeded: Vec::new(), out: VecDeque::new(), done: false };
  stream::unfold(grouping, |mut g| async move {
    loop {
      if let Some(next) = g.out.pop_front() {
        return Some((next, g));
      }
      if g.done {
        return None;
      }
      match g.rows.next().await {
        None => {
          g.done = true;
          g.release();
        }
        Some(Err(e)) => {
          g.done = true;
          g.release();
          g.out.push_back(Err(LoadError::new(g.source.clone(), e.to_string())));
        }
        Some(Ok(Row::Store(seeded))) => {
          g.release();
          g.seeded = seeded;
        }
        Some(Ok(Row::Fill { slot, node, segments })) => {
          g.release();
          let contributions = strip_content(std::mem::take(&mut g.seeded));
          g.held = Some(RemoteFill { slot, node, segments, meta: Meta::default(), contributions });
        }
        Some(Ok(Row::Head(meta))) => {
          if let Some(mut held) = g.held.take() {
            held.meta = meta;
            g.out.push_back(Ok(held));
          }
        }
        Some(Ok(_)) => {}
      }
    }
  })
  .boxed()
}

/// What the remote nodes of one assembly said beyond their markup, gathered
/// while the tree builds and folded into the assembly after.
#[derive(Default)]
pub(crate) struct RemoteOut {
  pub(crate) contributions: Vec<Contribution>,
  pub(crate) failed: Option<FailureKind>,
}

/// Every remote node of `plan` outside its deferred subtrees, which fetch on
/// their own wave.
fn collect_remotes<'p>(node: &'p PlanNode, is_root: bool, remotes: &HashMap<String, Arc<dyn RemoteSegment>>, out: &mut Vec<&'p PlanNode>) {
  if node.deferred && !is_root {
    return;
  }
  if remotes.contains_key(&node.module.to_string()) {
    out.push(node);
  }
  for (_, child) in &node.children {
    collect_remotes(child, false, remotes, out);
  }
}

struct Fills {
  stream: BoxStream<'static, Result<RemoteFill, LoadError>>,
  stash: HashMap<u32, RemoteFill>,
  ended: bool,
}

/// One remote's fills shared by every slot it left pending: the slot ids it
/// used mapped to this assembly's and its stream read by whichever slot is
/// waiting, the others' fills stashed until they ask.
struct Splicer {
  session: Arc<Session>,
  source: String,
  prefix: Vec<String>,
  ids: Mutex<HashMap<u32, SlotId>>,
  fills: AsyncMutex<Fills>,
}

impl Splicer {
  fn id_for(&self, remote: u32) -> SlotId {
    let mut ids = self.ids.lock();
    *ids.entry(remote).or_insert_with(|| self.session.allocate_slot())
  }

  fn prefixed(&self, mut contributions: Vec<Contribution>) -> Vec<Contribution> {
    for contribution in &mut contributions {
      let mut path = self.prefix.clone();
      path.append(&mut contribution.path);
      contribution.path = path;
    }
    contributions
  }

  /// The subtree and its sidecar entries under this assembly's slot ids,
  /// with one pending resolution per slot the remote left open.
  fn adopt(self: &Arc<Self>, node: Node, segments: Vec<SegmentInfo>, out_pending: &mut Vec<PendingResolution>) -> (Node, Vec<SegmentInfo>) {
    let node = self.renumber_node(node);
    let segments = segments.into_iter().map(|segment| self.renumber_segment(segment, out_pending)).collect();
    (node, segments)
  }

  fn renumber_node(&self, node: Node) -> Node {
    match node {
      Node::Pending { slot, fallback } => Node::Pending { slot: self.id_for(slot.0), fallback: Box::new(self.renumber_node(*fallback)) },
      Node::Seq(items) => Node::Seq(items.into_iter().map(|item| self.renumber_node(item)).collect()),
      Node::Client { module, props, children, ssr } => Node::Client {
        module,
        props,
        children: children.into_iter().map(|child| self.renumber_node(child)).collect(),
        ssr: ssr.map(|inner| Box::new(self.renumber_node(*inner))),
      },
      other => other,
    }
  }

  fn renumber_segment(self: &Arc<Self>, mut segment: SegmentInfo, out_pending: &mut Vec<PendingResolution>) -> SegmentInfo {
    if let Some(remote) = segment.slot {
      let slot = self.id_for(remote);
      segment.slot = Some(slot.0);
      out_pending.push(PendingResolution { slot, key: segment.key.clone(), future: self.resolution(remote, slot, segment.key.clone()) });
    }
    segment.children = segment.children.into_iter().map(|child| self.renumber_segment(child, out_pending)).collect();
    segment
  }

  fn resolution(self: &Arc<Self>, remote: u32, slot: SlotId, key: String) -> BoxFuture<'static, Resolved> {
    let splicer = Arc::clone(self);
    Box::pin(async move {
      match splicer.take(remote).await {
        Ok(fill) => {
          let mut pending = Vec::new();
          let (node, segments) = splicer.adopt(fill.node, fill.segments, &mut pending);
          Resolved { slot, key, node, segments, pending, meta: fill.meta, contributions: splicer.prefixed(fill.contributions) }
        }
        Err(e) => Resolved { slot, key, node: error_node(&e.to_string()), segments: Vec::new(), pending: Vec::new(), meta: Meta::default(), contributions: Vec::new() },
      }
    })
  }

  /// The fill for `remote`, reading the stream until it arrives and keeping
  /// every other slot's fill for its own wait.
  async fn take(&self, remote: u32) -> Result<RemoteFill, LoadError> {
    let mut fills = self.fills.lock().await;
    loop {
      if let Some(fill) = fills.stash.remove(&remote) {
        return Ok(fill);
      }
      if fills.ended {
        return Err(LoadError::new(self.source.clone(), format!("the stream ended before slot {remote} resolved")));
      }
      match fills.stream.next().await {
        None => fills.ended = true,
        Some(Err(e)) => {
          fills.ended = true;
          return Err(e);
        }
        Some(Ok(fill)) if fill.slot == remote => return Ok(fill),
        Some(Ok(fill)) => {
          fills.stash.insert(fill.slot, fill);
        }
      }
    }
  }
}

impl Session {
  /// Fetches every remote node of `plan` at once, beside the eager loads, and
  /// gives the head what their islands need before the document renders.
  /// Returns the ids fetched, whose waves [`Session::remote_meta`] reads.
  pub(crate) async fn prefetch_remotes(self: &Arc<Self>, plan: &PlanNode) -> Vec<u32> {
    let mut nodes = Vec::new();
    collect_remotes(plan, true, &self.runtime.remotes, &mut nodes);
    if nodes.is_empty() {
      return Vec::new();
    }
    let fetches = nodes.iter().map(|node| {
      let remote = &self.runtime.remotes[&node.module.to_string()];
      let ctx = self.ctx_of(node.id.0);
      let id = node.id.0;
      async move { (id, remote.fetch(ctx).await) }
    });
    let fetched = futures_util::future::join_all(fetches).await;
    let mut head = self.head.write();
    let mut waves = self.waves.lock();
    for (id, wave) in fetched {
      if let Ok(wave) = &wave {
        if wave.entry.is_some() {
          head.entry = wave.entry.clone();
        }
        for style in &wave.styles {
          if !head.styles.contains(style) {
            head.styles.push(style.clone());
          }
        }
      }
      waves.insert(id, wave);
    }
    nodes.iter().map(|node| node.id.0).collect()
  }

  /// What the prefetched waves of `ids` say about the document, folded in
  /// order so the eager wave's head carries a remote page's title.
  pub(crate) fn remote_meta(&self, ids: &[u32], meta: &mut Meta) {
    let waves = self.waves.lock();
    for id in ids {
      if let Some(Ok(wave)) = waves.get(id) {
        meta.merge(wave.meta.clone());
      }
    }
  }

  /// The plan node rendered by `remote`, spliced in: its subtree and sidecar
  /// under this assembly's slot ids, its fills pending, what it described,
  /// seeded and needs loaded recorded for the assembly. A remote that fails
  /// renders the node's error module and marks the page failed with the kind.
  pub(crate) async fn build_remote(
    self: &Arc<Self>,
    node: &PlanNode,
    remote: &Arc<dyn RemoteSegment>,
    plan_path: &[String],
    out_pending: &mut Vec<PendingResolution>,
  ) -> Result<(Node, Vec<SegmentInfo>, bool, u64, Option<String>), AssembleError> {
    let prefetched = self.waves.lock().remove(&node.id.0);
    let fetched = match prefetched {
      Some(fetched) => fetched,
      None => remote.fetch(self.ctx_of(node.id.0)).await,
    };
    let wave = match fetched {
      Ok(wave) => wave,
      Err(failure) => {
        tracing::warn!(target: "fsr::remote", node = node.id.0, error = %failure, "remote segment failed");
        self.remote_out.lock().failed.get_or_insert(failure.kind);
        let rendered = self.error_segment(node, &failure).await?;
        let digest = rendered.fingerprint();
        return Ok((rendered, Vec::new(), false, digest, None));
      }
    };
    let splicer = Arc::new(Splicer {
      session: Arc::clone(self),
      source: node.module.to_string(),
      prefix: plan_path.to_vec(),
      ids: Mutex::new(HashMap::new()),
      fills: AsyncMutex::new(Fills { stream: wave.fills, stash: HashMap::new(), ended: false }),
    });
    let (rendered, children) = splicer.adopt(wave.node, wave.segment.children, out_pending);
    self.remote_out.lock().contributions.extend(splicer.prefixed(wave.contributions));
    Ok((rendered, children, false, wave.segment.digest, Some(wave.segment.key)))
  }
}
