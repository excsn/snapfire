use futures_util::stream;
use snapfire_fsr_core::{Data, ModuleId, Node, SlotName, Value};
use snapfire_fsr_runtime::{Chunk, Evaluator, Head, NodeChunks};

/// The document around a client-rendered route: doctype, the head slot, the
/// mount point, the content slot. It emits no application markup. The
/// `locale` prop the assembler injects becomes `lang`, in its BCP 47
/// spelling and `data-sf-locale`, in the application's. `[store] slot_order`
/// rides on the element as `data-sf-slot-order`, a JSON array, so the
/// browser store merges by the rule the server rendered by.
#[derive(Default)]
pub struct DocumentShell {
  pub slot_order: Vec<String>,
}

impl Evaluator for DocumentShell {
  fn evaluate(&self, _module: &ModuleId, props: &Data) -> NodeChunks {
    let tag = match props.get("locale") {
      Some(Value::Str(tag)) => tag.to_string(),
      _ => "en".to_owned(),
    };
    let slot_order = match self.slot_order.is_empty() {
      true => String::new(),
      false => format!(" data-sf-slot-order=\"{}\"", escape(&serde_json::to_string(&self.slot_order).expect("strings serialize"))),
    };
    let open = format!(
      "<!doctype html><html lang=\"{}\" data-sf-locale=\"{}\"{slot_order}><head>",
      escape(&tag.replace('_', "-")),
      escape(&tag)
    );
    Box::pin(stream::iter([
      Ok(Chunk::Node(Node::raw(open))),
      Ok(Chunk::Slot(SlotName("head".into()))),
      Ok(Chunk::Node(Node::raw("</head><body><div id=\"app\">"))),
      Ok(Chunk::Slot(SlotName("content".into()))),
      Ok(Chunk::Node(Node::raw("</div></body></html>"))),
    ]))
  }
}

/// What the head slot carries: the stylesheets, the inlined import map, the
/// preload links and the entry module, built once at boot, with the configured
/// title as the default a route's `meta` overrides. The preload links sit after
/// the import map, since a bare specifier in one resolves through the other.
pub fn head(title: &str, styles: &[String], import_map: Option<&str>, preload: &[String], entry: Option<&str>) -> Head {
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
  for href in preload {
    head.push_str("<link rel=\"modulepreload\" href=\"");
    head.push_str(&escape(href));
    head.push_str("\">");
  }
  if let Some(entry) = entry {
    head.push_str("<script type=\"module\" src=\"");
    head.push_str(&escape(entry));
    head.push_str("\"></script>");
  }
  Head::new(title, Node::raw(head))
}

/// The request's trace token, which a page script reads to fetch the trace of
/// the request that served it.
pub fn request_meta(token: &str) -> String {
  format!("<meta name=\"sf-request\" content=\"{}\">", escape(token))
}

/// One preload link a mounted site adds to a document on its own routes, for
/// a module of its own bundle that the shell's links do not already cover.
pub fn preload_link(href: &str) -> String {
  format!("<link rel=\"modulepreload\" href=\"{}\">", escape(href))
}

/// The entry script a mounted site adds to a document on its own routes. Its
/// stylesheets are `Head::styles` instead, since a payload navigation has to
/// add and remove them and cannot do that to raw markup.
pub fn site_entry(entry: &str) -> String {
  format!("<script type=\"module\" src=\"{}\"></script>", escape(entry))
}

/// Lists each Content-Security-Policy violation in a development document: what
/// was blocked or only reported, the directive, where it came from and the
/// `[document.csp]` or `[document.csp_report_only]` key that would admit it.
/// A violation from a browser extension is counted rather than listed. The
/// panel's styles are set through CSSOM, which no policy restricts.
const CSP_WATCH: &str = r#"(function(){var seen={},list=[],ext=0,box;function table(e){return e.disposition==="report"?"csp_report_only":"csp"}function foreign(e){var f=(e.sourceFile||"")+" "+(e.blockedURI||"");return /(moz|chrome|safari-web)-extension:/.test(f)||/^[0-9a-f]{8}-[0-9a-f]{4}-/.test(e.sourceFile||"")}function what(e){var b=e.blockedURI||"";if(b==="inline")return "an inline "+(/^style/.test(e.effectiveDirective)?"style":"script");if(b==="eval")return "eval";try{return new URL(b).origin}catch(x){return b}}function sources(e,d){var found=null;(e.originalPolicy||"").split(";").forEach(function(part){var w=part.trim().split(/\s+/);if(w[0]===d)found=w.slice(1).filter(function(v){return !/^'(sha256|sha384|sha512|nonce)-/.test(v)})});return found}function fix(e){var d=(e.effectiveDirective||"").replace(/-(elem|attr)$/,""),b=e.blockedURI||"",k="[document."+table(e)+"]",add=b==="inline"?null:b==="eval"?"'unsafe-eval'":what(e),own=sources(e,d),fallback=sources(e,"default-src");if(b==="inline")return "move it into a file the page loads or add its hash to "+d+" in "+k;if(own)return (b==="eval"?"stop calling eval or a":"a")+"dd \""+add+"\" to "+d+" in "+k;var list=(fallback||[]).concat([add]).map(function(v){return JSON.stringify(v)}).join(", ");return "add "+d+" = ["+list+"] to "+k+(fallback?", which carries over what default-src allowed":"")}function draw(){if(!document.body||!list.length)return;if(!box){box=document.createElement("pre");box.id="sf-dev-csp";box.style.cssText="position:fixed;top:8px;right:8px;max-width:min(560px,calc(100vw - 16px));max-height:40vh;overflow:auto;margin:0;padding:10px 12px;background:#1f1a0e;color:#ffe9b8;border:1px solid #6b5520;border-radius:8px;font:12px/1.5 ui-monospace,monospace;white-space:pre-wrap;z-index:2147483646;cursor:pointer";box.title="Click to hide";box.onclick=function(){box.remove();box=null;seen={};list=[];ext=0};document.body.appendChild(box)}var blocked=list.some(function(e){return e.disposition!=="report"}),t="fsr dev: the content security policy "+(blocked?"blocked":"would block")+" "+list.length+(list.length===1?" thing":" things")+"\n";list.forEach(function(e){t+="\n"+(e.disposition==="report"?"reported ":"blocked  ")+what(e)+" ("+e.effectiveDirective+")"+(e.sourceFile?" at "+e.sourceFile+(e.lineNumber?":"+e.lineNumber:""):"")+"\n  "+fix(e)+"\n"});if(ext)t+="\n"+ext+" more from browser extensions, which are not this application";box.textContent=t}addEventListener("securitypolicyviolation",function(e){if(foreign(e)){ext++}else{var id=[e.disposition,e.effectiveDirective,e.blockedURI,e.sourceFile,e.lineNumber].join(" ");if(seen[id])return;seen[id]=1;list.push(e)}draw()});document.addEventListener("DOMContentLoaded",draw)})();"#;

/// The live-refresh script a development document carries, with the bundle
/// id the document was rendered against. Every event names the bundle the
/// server sees now: a different one reloads, since the page's modules
/// changed; the same one re-links the stylesheets and asks the client
/// library to refresh the route in place or reloads when no client library
/// is on the page. The first event after a connect is the greeting and does
/// nothing on its own, so a reconnect after a restart refreshes and a fresh
/// load does not.
/// The stream closes on `pagehide` and reopens when the page comes back from
/// the back-forward cache: a cached page holding it open keeps a connection
/// to the dev server, and a few of those stall the next navigation behind
/// the browser's per-host limit.
pub fn dev_script(bundle: &str, nonce: Option<&str>) -> String {
  let nonce = nonce.map(|n| format!(" nonce=\"{}\"", escape(n))).unwrap_or_default();
  format!(
    "<script{nonce}>{CSP_WATCH}(function(){{if(typeof EventSource===\"undefined\")return;var b=\"{}\",first,s;function shown(m){{var o=document.getElementById(\"sf-dev-error\");if(!m){{if(o)o.remove();return}}if(!o){{o=document.createElement(\"pre\");o.id=\"sf-dev-error\";o.setAttribute(\"style\",\"position:fixed;inset:auto 0 0 0;max-height:50vh;overflow:auto;margin:0;padding:12px 16px;background:#2b0f0f;color:#ffd7d7;font:12px/1.5 ui-monospace,monospace;white-space:pre-wrap;z-index:2147483647\");document.body.appendChild(o)}}o.textContent=\"fsr dev: the last build was refused\\n\\n\"+m}}function open(){{first=true;s=new EventSource(\"/__fsr/events\");s.onmessage=function(e){{var d={{}};try{{d=JSON.parse(e.data)}}catch(x){{}}shown(d.error);if(d.error)return;if(d.bundle&&d.bundle!==b)return location.reload();if(first){{first=false;return}}document.querySelectorAll(\"link[rel=stylesheet]\").forEach(function(l){{var u=new URL(l.href);u.searchParams.set(\"__sf\",Date.now());l.href=u.href}});var f=window.__sf&&window.__sf.refresh;f?f():location.reload()}}}}open();addEventListener(\"pagehide\",function(){{s.close()}});addEventListener(\"pageshow\",function(e){{if(e.persisted)open()}})}})()</script>",
    escape(bundle)
  )
}

/// The canonical link a prefixed request for the default locale carries, so
/// `/en_US/about` and `/about` are one page to a crawler. A crawler reads the
/// href as an absolute URL only, so `document.origin` goes in front of it when
/// the deployment names one.
pub fn canonical(origin: Option<&str>, path: &str) -> String {
  let href = match origin {
    Some(origin) => format!("{origin}{path}"),
    None => path.to_owned(),
  };
  format!("<link rel=\"canonical\" href=\"{}\">", escape(&href))
}

pub(crate) fn escape(text: &str) -> String {
  text
    .replace('&', "&amp;")
    .replace('<', "&lt;")
    .replace('>', "&gt;")
    .replace('"', "&quot;")
}

/// The locale's message catalog, embedded for the client's `t`: a JSON
/// script the boot adopts, `</` escaped so no message can close it.
pub fn catalog_script(tag: &str, json: &str) -> String {
  format!(
    "<script type=\"application/json\" data-sf-i18n=\"{}\">{}</script>",
    escape(tag),
    json.replace("</", "<\\/")
  )
}
