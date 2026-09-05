use futures_util::stream;
use snapfire_fsr_core::{Data, ModuleId, Node, SlotName};
use snapfire_fsr_runtime::{Chunk, Evaluator, Head, NodeChunks};

/// The document around a client-rendered route: doctype, the head slot, the
/// mount point, the content slot. It emits no application markup.
pub struct DocumentShell;

impl Evaluator for DocumentShell {
  fn evaluate(&self, _module: &ModuleId, _props: &Data) -> NodeChunks {
    Box::pin(stream::iter([
      Ok(Chunk::Node(Node::raw("<!doctype html><html lang=\"en\"><head>"))),
      Ok(Chunk::Slot(SlotName("head".into()))),
      Ok(Chunk::Node(Node::raw("</head><body><div id=\"app\">"))),
      Ok(Chunk::Slot(SlotName("content".into()))),
      Ok(Chunk::Node(Node::raw("</div></body></html>"))),
    ]))
  }
}

/// What the head slot carries: the stylesheets, the inlined import map and
/// the entry module, built once at boot, with the configured title as the
/// default a route's `meta` overrides.
pub fn head(title: &str, styles: &[String], import_map: Option<&str>, entry: Option<&str>) -> Head {
  let mut head = String::new();
  head.push_str("<meta charset=\"utf-8\"><meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">");
  for href in styles {
    head.push_str("<link rel=\"stylesheet\" href=\"");
    head.push_str(&escape(href));
    head.push_str("\">");
  }
  if let Some(map) = import_map {
    head.push_str("<script type=\"importmap\">");
    head.push_str(map);
    head.push_str("</script>");
  }
  if let Some(entry) = entry {
    head.push_str("<script type=\"module\" src=\"");
    head.push_str(&escape(entry));
    head.push_str("\"></script>");
  }
  Head::new(title, Node::raw(head))
}

/// The live-refresh script a development document carries, with the bundle
/// id the document was rendered against. Every event names the bundle the
/// server sees now: a different one reloads, since the page's modules
/// changed; the same one re-links the stylesheets and asks the client
/// library to refresh the route in place, or reloads when no client library
/// is on the page. The first event after a connect is the greeting and does
/// nothing on its own, so a reconnect after a restart refreshes and a fresh
/// load does not.
pub fn dev_script(bundle: &str) -> String {
  format!("<script>(function(){{if(typeof EventSource===\"undefined\")return;var b=\"{}\",first=true,s=new EventSource(\"/__fsr/events\");s.onmessage=function(e){{var d={{}};try{{d=JSON.parse(e.data)}}catch(x){{}}if(d.bundle&&d.bundle!==b)return location.reload();if(first){{first=false;return}}document.querySelectorAll(\"link[rel=stylesheet]\").forEach(function(l){{var u=new URL(l.href);u.searchParams.set(\"__sf\",Date.now());l.href=u.href}});var f=window.__sf&&window.__sf.refresh;f?f():location.reload()}}}})()</script>", escape(bundle))
}

fn escape(text: &str) -> String {
  text.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;")
}
