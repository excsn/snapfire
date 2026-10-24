use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;

use bytes::Bytes;
use http::{header, HeaderValue, Response, StatusCode};
use parking_lot::Mutex;
use snapfire_fsr_core::{Params, Value};
use snapfire_fsr_ogimage::{Card, Declarations, Element, Fonts, Format, ImageRef, Images, Run};
use snapfire_fsr_runtime::{FailureKind, Locale};

use crate::assets::AssetsManifest;
use crate::edge::text_response;
use crate::{Body, Host, Tables};

/// The rendered images a production host keeps, at most this many.
const CACHED: usize = 256;

/// What a host draws its generated card images with: the application's own
/// faces before the bundled Inter, the images the build placed, decoded on
/// first use, and the images already drawn.
pub(crate) struct Cards {
  fonts: Arc<Fonts>,
  /// The file each placed image's URL reads from.
  sources: HashMap<String, std::path::PathBuf>,
  images: Mutex<Images>,
  drawn: Mutex<HashMap<String, Arc<Vec<u8>>>>,
  /// What each Tailwind class means on a card, from the build's table.
  classes: CardClasses,
  /// Each mounted site's own table, by site name, for the images its modules draw.
  site_classes: HashMap<String, CardClasses>,
}

/// The build's table of card declarations by class: the class's place in Tailwind's output
/// order, the card width it starts to apply at and its declarations.
#[derive(Default)]
pub(crate) struct CardClasses(HashMap<String, (usize, Option<f32>, Vec<(String, String)>)>);

#[derive(serde::Deserialize)]
struct CardClass {
  class: String,
  #[serde(default)]
  min_width: Option<f32>,
  declarations: Vec<(String, String)>,
}

impl CardClasses {
  fn read(app: &Path) -> CardClasses {
    let file = app.join(crate::CARD_CLASSES_FILE);
    let Ok(text) = std::fs::read_to_string(&file) else { return CardClasses::default() };
    match serde_json::from_str::<Vec<CardClass>>(&text) {
      Ok(rows) => CardClasses(rows.into_iter().enumerate().map(|(order, row)| (row.class, (order, row.min_width, row.declarations))).collect()),
      Err(why) => {
        tracing::warn!(target: "fsr::card", file = %file.display(), %why, "the card class table does not read; classes draw nothing");
        CardClasses::default()
      }
    }
  }

  /// The declarations `class` lists for a card `width` wide, in the table's order.
  fn style_of(&self, class: &str, width: f32) -> String {
    let mut found: Vec<&(usize, Option<f32>, Vec<(String, String)>)> = class.split_whitespace().filter_map(|c| self.0.get(c)).filter(|(_, min, _)| min.is_none_or(|min| width >= min)).collect();
    found.sort_by_key(|(order, _, _)| *order);
    found.iter().flat_map(|(_, _, decls)| decls.iter()).map(|(p, v)| format!("{p}:{v};")).collect()
  }
}

impl Cards {
  /// Every face the build read under `[dirs] fonts`, in the order the
  /// manifest lists them, read from the bundle's copy under `out`, which a
  /// deploy tree carries. With no bundle it is read from the source under `app`. A face that does not read is skipped with a warning
  /// and the card falls back past it.
  /// A `[fonts.<key>]` with `scripts` adds its faces as the ones for text in
  /// those scripts; a script name Unicode does not have refuses the boot.
  pub(crate) fn new(app: &Path, out: Option<&Path>, manifest: Option<&AssetsManifest>, scripts: &dyn Fn(&str) -> Vec<String>) -> Result<Cards, String> {
    let file_of = |path: &str, source: &str| match out.filter(|_| !path.is_empty()) {
      Some(out) => out.join(path),
      None => app.join(source),
    };
    let mut fonts = Fonts::new();
    let mut sources = HashMap::new();
    if let Some(manifest) = manifest {
      for face in &manifest.fonts.faces {
        let file = file_of(&face.path, &face.source);
        let names = scripts(&face.key);
        let names: Vec<&str> = names.iter().map(String::as_str).collect();
        let bytes = match std::fs::read(&file) {
          Ok(bytes) => bytes,
          Err(why) => {
            tracing::warn!(target: "fsr::card", file = %file.display(), %why, "a font is left out of card images");
            continue;
          }
        };
        match fonts.add_for_scripts(bytes, &names) {
          Ok(_) => {}
          Err(e @ snapfire_fsr_ogimage::FontError::UnknownScript(_)) => return Err(format!("fonts.{}.scripts: {e}", face.key)),
          Err(why) => tracing::warn!(target: "fsr::card", file = %file.display(), %why, "a font is left out of card images"),
        }
      }
      for entry in &manifest.entries {
        let file = file_of(&entry.path, &entry.source);
        sources.insert(entry.src.clone(), file.clone());
        for variant in &entry.variants {
          sources.insert(variant.url.clone(), file.clone());
        }
      }
    }
    Ok(Cards { fonts: Arc::new(fonts), sources, images: Mutex::new(Images::new()), drawn: Mutex::new(HashMap::new()), classes: CardClasses::read(app), site_classes: HashMap::new() })
  }

  /// The tables of the sites mounted from `apps`, each a site's name and its app directory.
  pub(crate) fn with_sites(mut self, apps: &[(String, std::path::PathBuf)]) -> Cards {
    for (name, app) in apps {
      self.site_classes.insert(name.clone(), CardClasses::read(app));
    }
    self
  }

  /// The table for an image `module` draws: its site's when a mounted site owns it.
  fn classes_for(&self, module: &str) -> &CardClasses {
    module.split_once(':').and_then(|(site, _)| self.site_classes.get(site)).unwrap_or(&self.classes)
  }

  /// The images `card` names, each an image the build placed, read from its
  /// source file once. A URL the build did not place stays unknown, which the
  /// renderer refuses by name: nothing is fetched.
  fn images_for(&self, card: &Card) -> Images {
    let mut keys = Vec::new();
    image_keys(&card.root, &mut keys);
    let mut images = self.images.lock();
    for key in keys {
      if images.contains(&key) {
        continue;
      }
      let Some(file) = self.sources.get(&key).cloned() else { continue };
      match std::fs::read(&file) {
        Ok(bytes) => {
          if let Err(e) = images.insert(key.clone(), &bytes) {
            tracing::warn!(target: "fsr::card", file = %file.display(), error = %e, "an image is left out of card images");
          }
        }
        Err(e) => tracing::warn!(target: "fsr::card", file = %file.display(), error = %e, "an image is left out of card images"),
      }
    }
    images.clone()
  }
}

fn image_keys(element: &Element, out: &mut Vec<String>) {
  if let Element::Image { src: ImageRef::Key(key), .. } = element {
    out.push(key.clone());
  }
  for (name, value) in element.style() {
    if name.eq_ignore_ascii_case("background-image") || name.eq_ignore_ascii_case("background") {
      let mut rest = value.as_str();
      while let Some(at) = rest.find("url(") {
        let after = &rest[at + 4..];
        let Some(end) = after.find(')') else { break };
        out.push(after[..end].trim().trim_matches(|c| c == '"' || c == '\'').to_owned());
        rest = &after[end..];
      }
    }
  }
  if let Element::Box { children, .. } = element {
    for child in children {
      image_keys(child, out);
    }
  }
}

/// Why an image was not drawn: the status it answers with and the reason.
pub(crate) type Refused = (StatusCode, String);

/// The card image file names a page's images sit under, beside its document.
const STEMS: [&str; 2] = ["opengraph-image", "twitter-image"];

fn sniffed(bytes: &[u8]) -> &'static str {
  if bytes.starts_with(&[0x89, b'P', b'N', b'G']) { "image/png" } else { "image/jpeg" }
}

impl Host {
  /// A generated card image. Outside development a prerender's file for the
  /// path answers when the directory is this build's; otherwise it is drawn.
  pub(crate) async fn card_image(&self, t: &Tables, entry: &snapfire_fsr_plan::ImageEntry, params: Params, path: &str, incoming: crate::edge::Incoming, locale: &Locale) -> Response<Body> {
    let live = t.dev_bundle.is_some();
    if !live {
      if let Some(bytes) = prerendered_image(t, path, locale) {
        let kind = sniffed(&bytes);
        return image_response(kind, bytes, false);
      }
    }
    match self.draw_card(t, entry, params, path, incoming, locale).await {
      Ok((bytes, kind)) => image_response(&kind, bytes, live),
      Err((status, why)) => text_response(status, why),
    }
  }

  /// Draws a generated card image: the image's data loads for the params,
  /// its component renders through the interpreter with that data as its
  /// props, and the markup is drawn. A loader that answers not found refuses
  /// with a 404, any other failure with a 500 and the reason logged.
  pub(crate) async fn draw_card(&self, t: &Tables, entry: &snapfire_fsr_plan::ImageEntry, params: Params, path: &str, incoming: crate::edge::Incoming, locale: &Locale) -> Result<(Vec<u8>, String), Refused> {
    let server = |why: String| (StatusCode::INTERNAL_SERVER_ERROR, why);
    let cards = t.cards.as_ref().ok_or((StatusCode::NOT_FOUND, "card images are not drawn by this host".to_owned()))?;
    let evaluator = t.app.lowered.as_ref().ok_or_else(|| server("the image's component was not lowered".to_owned()))?;
    let mut params = params;
    let listed_id = params.shift_remove(snapfire_fsr_plan::IMAGE_PARAM);
    let ctx = self.ctx(t, incoming, params.clone(), Params::new(), path, locale.clone());
    let (width, height, content_type) = match &listed_id {
      None => (entry.width, entry.height, entry.content_type.clone()),
      Some(id) => {
        let listed = snapfire_fsr::listed_images(evaluator.interpreter(), entry, &ctx).await.map_err(|why| {
          tracing::warn!(target: "fsr::card", image = %entry.pattern, %why, "the image list failed");
          server(format!("the image list failed: {why}"))
        })?;
        match listed.into_iter().find(|image| image.id == *id) {
          Some(image) => (image.width, image.height, image.content_type),
          None => return Err((StatusCode::NOT_FOUND, format!("`images` lists no image `{id}`"))),
        }
      }
    };
    let mut props = match entry.source.as_ref().and_then(|id| t.app.runtime.sources.get(&snapfire_fsr_core::DataSourceId(id.clone()))) {
      Some(source) => match source.load(&ctx).await {
        Ok(data) => data,
        Err(e) if e.kind == FailureKind::NotFound => return Err((StatusCode::NOT_FOUND, e.message)),
        Err(e) => {
          tracing::warn!(target: "fsr::card", image = %entry.pattern, error = %e.message, "the image's loader failed");
          return Err(server(format!("the image's data did not load: {}", e.message)));
        }
      },
      None => Default::default(),
    };
    let mut params_map = snapfire_fsr_core::value::ValueMap::default();
    for (k, v) in &params {
      params_map.insert(k.clone(), Value::str(v.clone()));
    }
    props.insert("params".to_owned(), Value::Map(params_map));
    if !locale.tag.is_empty() {
      props.insert("locale".to_owned(), Value::str(locale.tag.clone()));
    }
    if let Some(id) = &listed_id {
      props.insert("id".to_owned(), Value::str(id.clone()));
    }

    let key = format!("{path}\u{0}{}", snapfire_fsr_payload::value_to_json(&Value::Map(props.clone())));
    let live = t.dev_bundle.is_some();
    if !live {
      if let Some(bytes) = cards.drawn.lock().get(&key).cloned() {
        return Ok((bytes.as_ref().clone(), content_type));
      }
    }
    let components = evaluator.components();
    let component = components.get(&entry.module).cloned().ok_or_else(|| server(format!("`{}` was not lowered", entry.module)))?;
    let html = evaluator.interpreter().render_module(&entry.module, &component, &props, &components).map_err(|fail| {
      tracing::warn!(target: "fsr::card", image = %entry.pattern, error = %fail.message, "the image's component failed");
      server(format!("the image's component failed: {}", fail.message))
    })?.html;
    let card = Card::new(width, height, parse_with(&html, cards.classes_for(&entry.module), width as f32));
    let images = cards.images_for(&card);
    let fonts = cards.fonts.clone();
    let format = if content_type == "image/jpeg" { Format::Jpeg { quality: 85 } } else { Format::Png };
    let rendered = match tokio::task::spawn_blocking(move || snapfire_fsr_ogimage::render(&card, &fonts, &images, format)).await {
      Ok(Ok(rendered)) => rendered,
      Ok(Err(e)) => {
        tracing::warn!(target: "fsr::card", image = %entry.pattern, error = %e, "the image did not draw");
        return Err(server(format!("the image did not draw: {e}")));
      }
      Err(e) => return Err(server(format!("the image did not draw: {e}"))),
    };
    for warning in &rendered.warnings {
      tracing::warn!(target: "fsr::card", image = %entry.pattern, %warning, "card image");
    }
    if !live {
      let mut drawn = cards.drawn.lock();
      if drawn.len() >= CACHED {
        drawn.clear();
      }
      drawn.insert(key, Arc::new(rendered.bytes.clone()));
    }
    Ok((rendered.bytes, content_type))
  }

  /// Writes the card images of the page at `path` into `dir`, the directory
  /// its document went to: `<stem>.png` or `.jpg`, or `<stem>/<id>` per
  /// image a module lists. An image that fails to draw is logged and its
  /// file from the last run kept in `held`.
  pub(crate) async fn prerender_images(&self, t: &Tables, path: &str, dir: &std::path::Path, locale: &Locale, written: &mut Vec<(String, std::path::PathBuf)>, held: &mut Vec<std::path::PathBuf>) -> Result<(), crate::HostError> {
    if t.cards.is_none() {
      return Ok(());
    }
    let base = path.trim_end_matches('/');
    for stem in STEMS {
      let mut found: Vec<(String, std::path::PathBuf)> = Vec::new();
      for ext in ["png", "jpg"] {
        let url = format!("{base}/{stem}.{ext}");
        if t.app.images.match_path(&url).is_some() {
          found.push((url, dir.join(format!("{stem}.{ext}"))));
        }
      }
      let probe = format!("{base}/{stem}/_");
      if let Some((entry, params)) = t.app.images.match_path(&probe).filter(|(entry, _)| entry.images.is_some()) {
        let mut params = params;
        params.shift_remove(snapfire_fsr_plan::IMAGE_PARAM);
        let ctx = self.ctx(t, crate::edge::Incoming::anonymous(snapfire_fsr_runtime::SessionCell::default()), params, Params::new(), path, locale.clone());
        let evaluator = t.app.lowered.as_ref().expect("an image's component is lowered");
        match snapfire_fsr::listed_images(evaluator.interpreter(), entry, &ctx).await {
          Ok(listed) => found.extend(listed.into_iter().map(|image| (format!("{base}/{stem}/{}", image.id), dir.join(stem).join(&image.id)))),
          Err(why) => tracing::warn!(target: "fsr::host", path = %path, %why, "prerender: the image list failed, so no image under it is written"),
        }
      }
      for (url, file) in found {
        let Some((entry, params)) = t.app.images.match_path(&url) else { continue };
        let incoming = crate::edge::Incoming::anonymous(snapfire_fsr_runtime::SessionCell::default());
        match self.draw_card(t, entry, params, &url, incoming, locale).await {
          Ok((bytes, _)) => {
            if let Some(parent) = file.parent() {
              std::fs::create_dir_all(parent).map_err(|e| crate::HostError::Io(parent.to_path_buf(), e))?;
            }
            crate::prerender::write_whole(&file, &bytes)?;
            written.push((url, file));
          }
          Err((_, why)) => {
            tracing::warn!(target: "fsr::host", image = %url, %why, "prerender: the image did not draw, so no new file is written for it and the last run's stays");
            held.push(file);
          }
        }
      }
    }
    Ok(())
  }
}

/// A prerender's file for a card image's path, when the directory is this
/// build's.
fn prerendered_image(t: &Tables, path: &str, locale: &Locale) -> Option<Vec<u8>> {
  let dir = t.prerendered.as_ref()?;
  if !t.prerender_current.load(std::sync::atomic::Ordering::Acquire) {
    return None;
  }
  let root = if locale.is_default { dir.clone() } else { dir.join(&locale.tag) };
  std::fs::read(root.join(path.trim_matches('/'))).ok()
}

fn image_response(content_type: &str, bytes: Vec<u8>, live: bool) -> Response<Body> {
  use http_body_util::BodyExt as _;
  let mut response = Response::new(http_body_util::Full::new(Bytes::from(bytes)).map_err(|never| match never {}).boxed_unsync());
  let headers = response.headers_mut();
  headers.insert(header::CONTENT_TYPE, HeaderValue::from_str(content_type).unwrap_or(HeaderValue::from_static("image/png")));
  headers.insert(header::CACHE_CONTROL, HeaderValue::from_static(if live { "no-cache" } else { "public, max-age=3600" }));
  response
}

#[derive(Debug, Clone, PartialEq)]
enum Node {
  El { tag: String, attrs: Vec<(String, String)>, children: Vec<Node> },
  Text(String),
}

const VOID: [&str; 8] = ["img", "br", "hr", "meta", "link", "input", "source", "wbr"];

/// Elements that stay inside a paragraph: their text joins the parent's
/// runs, with their declarations added to their runs'.
const INLINE: [&str; 13] = ["span", "b", "strong", "i", "em", "a", "small", "code", "s", "u", "mark", "sub", "sup"];

fn decode(text: &str) -> String {
  if !text.contains('&') {
    return text.to_owned();
  }
  let mut out = String::with_capacity(text.len());
  let mut rest = text;
  while let Some(at) = rest.find('&') {
    out.push_str(&rest[..at]);
    rest = &rest[at..];
    let Some(end) = rest[..rest.len().min(12)].find(';') else {
      out.push('&');
      rest = &rest[1..];
      continue;
    };
    let entity = &rest[1..end];
    let decoded = match entity {
      "amp" => Some('&'),
      "lt" => Some('<'),
      "gt" => Some('>'),
      "quot" => Some('"'),
      "apos" => Some('\''),
      "nbsp" => Some('\u{a0}'),
      _ => entity.strip_prefix("#x").or_else(|| entity.strip_prefix("#X")).and_then(|h| u32::from_str_radix(h, 16).ok()).or_else(|| entity.strip_prefix('#').and_then(|d| d.parse().ok())).and_then(char::from_u32),
    };
    match decoded {
      Some(c) => {
        out.push(c);
        rest = &rest[end + 1..];
      }
      None => {
        out.push('&');
        rest = &rest[1..];
      }
    }
  }
  out.push_str(rest);
  out
}

/// The elements and text of markup FSR's renderer wrote: tags, quoted
/// attributes, text, comments and entities. A stray `</x>` closes up to the
/// nearest open `x`.
fn nodes(html: &str) -> Vec<Node> {
  let mut stack: Vec<(String, Vec<(String, String)>, Vec<Node>)> = vec![(String::new(), Vec::new(), Vec::new())];
  let bytes = html.as_bytes();
  let mut i = 0;
  while i < bytes.len() {
    if html[i..].starts_with("<!--") {
      i = html[i..].find("-->").map(|e| i + e + 3).unwrap_or(bytes.len());
    } else if html[i..].starts_with("</") {
      let end = html[i..].find('>').map(|e| i + e).unwrap_or(bytes.len());
      let tag = html[i + 2..end].trim().to_ascii_lowercase();
      if stack.iter().skip(1).any(|(t, _, _)| *t == tag) {
        while let Some((t, attrs, children)) = stack.pop() {
          stack.last_mut().expect("the root stays").2.push(Node::El { tag: t.clone(), attrs, children });
          if t == tag {
            break;
          }
        }
      }
      i = end + 1;
    } else if bytes[i] == b'<' && html[i + 1..].starts_with(|c: char| c.is_ascii_alphabetic()) {
      let (tag, attrs, end, self_closing) = open_tag(html, i + 1);
      i = end;
      if self_closing || VOID.contains(&tag.as_str()) {
        stack.last_mut().expect("the root stays").2.push(Node::El { tag, attrs, children: Vec::new() });
      } else {
        stack.push((tag, attrs, Vec::new()));
      }
    } else {
      let next = i + html[i..].chars().next().map_or(1, char::len_utf8);
      let end = html[next..].find('<').map(|e| next + e).unwrap_or(bytes.len());
      stack.last_mut().expect("the root stays").2.push(Node::Text(decode(&html[i..end])));
      i = end;
    }
  }
  while stack.len() > 1 {
    let (tag, attrs, children) = stack.pop().expect("more than the root");
    stack.last_mut().expect("the root stays").2.push(Node::El { tag, attrs, children });
  }
  stack.pop().map(|(_, _, children)| children).unwrap_or_default()
}

fn open_tag(html: &str, from: usize) -> (String, Vec<(String, String)>, usize, bool) {
  let bytes = html.as_bytes();
  let mut i = from;
  while i < bytes.len() && !bytes[i].is_ascii_whitespace() && bytes[i] != b'>' && bytes[i] != b'/' {
    i += 1;
  }
  let tag = html[from..i].to_ascii_lowercase();
  let mut attrs = Vec::new();
  loop {
    while i < bytes.len() && bytes[i].is_ascii_whitespace() {
      i += 1;
    }
    if i >= bytes.len() {
      return (tag, attrs, i, false);
    }
    if bytes[i] == b'>' {
      return (tag, attrs, i + 1, false);
    }
    if html[i..].starts_with("/>") {
      return (tag, attrs, i + 2, true);
    }
    let start = i;
    while i < bytes.len() && !bytes[i].is_ascii_whitespace() && bytes[i] != b'=' && bytes[i] != b'>' && !html[i..].starts_with("/>") {
      i += 1;
    }
    let name = html[start..i].to_ascii_lowercase();
    let mut value = String::new();
    if i < bytes.len() && bytes[i] == b'=' {
      i += 1;
      if i < bytes.len() && (bytes[i] == b'"' || bytes[i] == b'\'') {
        let quote = bytes[i] as char;
        let end = html[i + 1..].find(quote).map(|e| i + 1 + e).unwrap_or(bytes.len());
        value = decode(&html[i + 1..end]);
        i = (end + 1).min(bytes.len());
      } else {
        let begin = i;
        while i < bytes.len() && !bytes[i].is_ascii_whitespace() && bytes[i] != b'>' {
          i += 1;
        }
        value = decode(&html[begin..i]);
      }
    }
    if name.is_empty() {
      i += 1;
      continue;
    }
    attrs.push((name, value));
  }
}

/// A `style` attribute's declarations in order, split at `;` outside
/// parentheses and quotes.
fn declarations(style: &str) -> Declarations {
  let mut out = Vec::new();
  let mut depth = 0i32;
  let mut quote: Option<char> = None;
  let mut start = 0;
  let push = |part: &str, out: &mut Declarations| {
    if let Some((name, value)) = part.split_once(':') {
      let (name, value) = (name.trim(), value.trim());
      if !name.is_empty() && !value.is_empty() {
        out.push((name.to_owned(), value.to_owned()));
      }
    }
  };
  for (i, c) in style.char_indices() {
    match (quote, c) {
      (Some(q), c) if c == q => quote = None,
      (Some(_), _) => {}
      (None, '"' | '\'') => quote = Some(c),
      (None, '(') => depth += 1,
      (None, ')') => depth -= 1,
      (None, ';') if depth == 0 => {
        push(&style[start..i], &mut out);
        start = i + 1;
      }
      _ => {}
    }
  }
  push(&style[start..], &mut out);
  out
}

fn attr<'a>(attrs: &'a [(String, String)], name: &str) -> Option<&'a str> {
  attrs.iter().find(|(n, _)| n == name).map(|(_, v)| v.as_str())
}

/// The declarations an element brings to its own box or runs: its `style`
/// with what its tag means first, so `<b>` is bold unless it says otherwise.
fn element_style(tag: &str, attrs: &[(String, String)]) -> Declarations {
  let mut style = match tag {
    "b" | "strong" => vec![("font-weight".to_owned(), "700".to_owned())],
    "i" | "em" => vec![("font-style".to_owned(), "italic".to_owned())],
    "u" => vec![("text-decoration".to_owned(), "underline".to_owned())],
    "s" => vec![("text-decoration".to_owned(), "line-through".to_owned())],
    _ => Vec::new(),
  };
  style.extend(attr(attrs, "style").map(declarations).unwrap_or_default());
  style
}

/// Whether an element sets `display` itself. One that does lays its
/// children out as flex items, each its own element, as a flex row of tags
/// does; one that does not keeps its text and inline elements one paragraph.
fn declares_display(attrs: &[(String, String)]) -> bool {
  attr(attrs, "style").is_some_and(|style| declarations(style).iter().any(|(name, _)| name.eq_ignore_ascii_case("display")))
}

fn is_text(node: &Node) -> bool {
  match node {
    Node::Text(_) => true,
    Node::El { tag, children, .. } => (INLINE.contains(&tag.as_str()) || tag == "br") && children.iter().all(is_text),
  }
}

fn runs(nodes: &[Node], inherited: &Declarations, out: &mut Vec<Run>) {
  for node in nodes {
    match node {
      Node::Text(text) => out.push(Run { style: inherited.clone(), text: text.clone() }),
      Node::El { tag, .. } if tag == "br" => out.push(Run { style: inherited.clone(), text: "\n".to_owned() }),
      Node::El { tag, attrs, children } => {
        let mut style = inherited.clone();
        style.extend(element_style(tag, attrs));
        runs(children, &style, out);
      }
    }
  }
}

fn classed(nodes: &mut [Node], classes: &CardClasses, width: f32) {
  for node in nodes {
    let Node::El { attrs, children, .. } = node else { continue };
    if let Some(class) = attr(attrs, "class").map(str::to_owned) {
      let from_classes = classes.style_of(&class, width);
      if !from_classes.is_empty() {
        let own = attr(attrs, "style").unwrap_or_default().to_owned();
        attrs.retain(|(name, _)| name != "style");
        attrs.push(("style".to_owned(), format!("{from_classes}{own}")));
      }
    }
    classed(children, classes, width);
  }
}

fn image_ref(src: &str) -> ImageRef {
  if let Some(data) = src.strip_prefix("data:") {
    if let Some((meta, payload)) = data.split_once(',') {
      if meta.ends_with(";base64") {
        use base64::Engine as _;
        if let Ok(bytes) = base64::engine::general_purpose::STANDARD.decode(payload.trim()) {
          return ImageRef::Bytes(bytes);
        }
      }
    }
  }
  ImageRef::Key(src.to_owned())
}

fn element(node: &Node) -> Option<Element> {
  match node {
    Node::Text(text) if text.trim().is_empty() => None,
    Node::Text(text) => Some(Element::Text { tag: "#text".to_owned(), style: Vec::new(), runs: vec![Run::new(text.clone())] }),
    Node::El { tag, attrs, .. } if tag == "img" => {
      let size = |name: &str| attr(attrs, name).and_then(|v| v.trim_end_matches("px").parse::<f32>().ok());
      Some(Element::Image { tag: tag.clone(), style: element_style(tag, attrs), src: image_ref(attr(attrs, "src").unwrap_or_default()), width: size("width"), height: size("height") })
    }
    Node::El { tag, attrs, children } if !children.is_empty() && children.iter().all(is_text) && (children.iter().all(|c| matches!(c, Node::Text(_))) || !declares_display(attrs)) => {
      let mut out = Vec::new();
      runs(children, &Vec::new(), &mut out);
      Some(Element::Text { tag: tag.clone(), style: element_style(tag, attrs), runs: out })
    }
    Node::El { tag, attrs, children } => Some(Element::Box { tag: tag.clone(), style: element_style(tag, attrs), children: children.iter().filter_map(element).collect() }),
  }
}

/// The card a component's markup draws. One top element is the root; more
/// than one sit in a box the card's size.
#[cfg(test)]
pub(crate) fn parse(html: &str) -> Element {
  parse_with(html, &CardClasses::default(), 1200.0)
}

/// `parse` with each element's `class` written as declarations ahead of its own `style`, so an
/// inline declaration still wins over a class, the way it does on the page.
pub(crate) fn parse_with(html: &str, classes: &CardClasses, width: f32) -> Element {
  let mut parsed = nodes(html);
  if !classes.0.is_empty() {
    classed(&mut parsed, classes, width);
  }
  let mut top: Vec<Element> = parsed.iter().filter_map(element).collect();
  if top.len() == 1 {
    return top.remove(0);
  }
  Element::Box { tag: "div".to_owned(), style: Vec::new(), children: top }
}

#[cfg(test)]
mod tests {
  use super::*;

  /// A deploy tree carries the bundle's hashed copies and none of the sources, so a face is read from the copy the manifest names.
  #[test]
  fn a_face_is_read_from_the_bundles_copy_which_a_deploy_tree_carries() {
    let root = std::env::temp_dir().join(format!("fsr-card-fonts-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    let (app, out) = (root.join("app"), root.join("serve/static/js/app"));
    std::fs::create_dir_all(out.join("fonts")).unwrap();
    std::fs::write(out.join("fonts/Fraunces.3f2a9c1e.woff2"), std::fs::read(concat!(env!("CARGO_MANIFEST_DIR"), "/../cli/tests/fixtures/Fraunces-vietnamese.woff2")).unwrap()).unwrap();
    let mut manifest = AssetsManifest::default();
    manifest.fonts.faces.push(crate::assets::Face { key: "serif".to_owned(), family: "Fraunces".to_owned(), weight: 400, weight_range: None, style: "normal".to_owned(), source: "fonts/Fraunces.woff2".to_owned(), url: "/static/js/app/fonts/Fraunces.3f2a9c1e.woff2".to_owned(), path: "fonts/Fraunces.3f2a9c1e.woff2".to_owned(), unicode_range: None, preload: false });
    let cards = Cards::new(&app, Some(&out), Some(&manifest), &|_| Vec::new()).unwrap();
    assert!(cards.fonts.faces().iter().any(|face| face.family == "Fraunces"), "the face read from the bundle's copy");
    std::fs::remove_dir_all(&root).unwrap();
  }

  #[test]
  fn markup_becomes_boxes_text_and_runs() {
    let html = r#"<div style="display:flex;flex-direction:column;background-image:linear-gradient(135deg, #000 0%, #fff 100%)"><h1 style="font-size:72px">Roof work &amp; more</h1><!-- --><p>Posted <b>today</b> by <span style="color:red">the board</span></p><img src="/static/x.png" width="48" height="48"/></div>"#;
    let Element::Box { style, children, .. } = parse(html) else { panic!("a box") };
    assert_eq!(style[2], ("background-image".to_owned(), "linear-gradient(135deg, #000 0%, #fff 100%)".to_owned()));
    assert_eq!(children.len(), 3);
    let Element::Text { runs, .. } = &children[0] else { panic!("text") };
    assert_eq!(runs[0].text, "Roof work & more");
    let Element::Text { runs, .. } = &children[1] else { panic!("text") };
    assert_eq!(runs.iter().map(|r| r.text.as_str()).collect::<Vec<_>>(), ["Posted ", "today", " by ", "the board"]);
    assert_eq!(runs[1].style, vec![("font-weight".to_owned(), "700".to_owned())]);
    assert_eq!(runs[3].style, vec![("color".to_owned(), "red".to_owned())]);
    let Element::Image { src, width, .. } = &children[2] else { panic!("image") };
    assert_eq!(src, &ImageRef::Key("/static/x.png".to_owned()));
    assert_eq!(*width, Some(48.0));
  }

  #[test]
  fn a_box_that_sets_display_lays_its_spans_out_as_items() {
    let Element::Box { children, .. } = parse(r#"<div style="display:flex;gap:16px"><span>2026-09-02</span><span style="padding:4px 16px;background-color:#fde68a">design</span></div>"#) else { panic!("a box") };
    assert_eq!(children.len(), 2);
    assert!(matches!(&children[1], Element::Text { style, .. } if style.iter().any(|(n, _)| n == "padding")));
  }

  #[test]
  fn classes_draw_from_the_table_before_the_inline_style() {
    let row = |order: usize, min: Option<f32>, decls: &[(&str, &str)]| (order, min, decls.iter().map(|(p, v)| (p.to_string(), v.to_string())).collect::<Vec<_>>());
    let classes = CardClasses(
      [
        ("flex".to_owned(), row(0, None, &[("display", "flex")])),
        ("text-4xl".to_owned(), row(1, None, &[("font-size", "2.25rem")])),
        ("md:text-6xl".to_owned(), row(2, Some(768.0), &[("font-size", "3.75rem")])),
      ]
      .into_iter()
      .collect(),
    );
    let html = r#"<div class="md:text-6xl text-4xl flex hover:bg-red-500" style="gap:4px"><p>x</p></div>"#;
    let Element::Box { style, .. } = parse_with(html, &classes, 1200.0) else { panic!("a box") };
    assert_eq!(style, [("display", "flex"), ("font-size", "2.25rem"), ("font-size", "3.75rem"), ("gap", "4px")].map(|(p, v)| (p.to_owned(), v.to_owned())));
    let Element::Box { style, .. } = parse_with(html, &classes, 600.0) else { panic!("a box") };
    assert_eq!(style, [("display", "flex"), ("font-size", "2.25rem"), ("gap", "4px")].map(|(p, v)| (p.to_owned(), v.to_owned())));
  }

  #[test]
  fn text_beside_elements_gets_its_own_element() {
    let Element::Box { children, .. } = parse("<div>Hello<div>there</div></div>") else { panic!("a box") };
    assert!(matches!(&children[0], Element::Text { tag, .. } if tag == "#text"));
    assert!(matches!(&children[1], Element::Text { tag, .. } if tag == "div"));
  }

  #[test]
  fn text_that_starts_with_a_multibyte_character_parses() {
    let Element::Text { runs, .. } = parse("<div><span>●</span>SnapFire ● FSR</div>") else { panic!("text") };
    assert_eq!(runs.iter().map(|r| r.text.as_str()).collect::<Vec<_>>(), ["●", "SnapFire ● FSR"]);
  }

  #[test]
  fn entities_and_a_data_uri_decode() {
    assert_eq!(decode("a &lt;b&gt; &#39;c&#x27; &quot;d&quot; &nbsp;&bogus"), "a <b> 'c' \"d\" \u{a0}&bogus");
    assert_eq!(image_ref("data:image/png;base64,iVBORw=="), ImageRef::Bytes(vec![0x89, b'P', b'N', b'G']));
  }
}
