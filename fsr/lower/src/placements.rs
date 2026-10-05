//! The placements every front end shares: `Link`, `Picture` and `Mount`. A
//! front end recognises one by where its tag is imported from and hands over
//! its attributes as written; what the markup is gets decided here, once, for
//! JSX, a Vue template and any front end after them.

use std::borrow::Cow;
use std::rc::Rc;

use snapfire_fsr_ir::ast::{Builtin, CompareOp, Entry, Expr, Lit, LogicOp, Tmpl};
use snapfire_fsr_ir::render::html_attr_name;
use swc_core::common::{Span, Spanned};
use swc_core::ecma::ast as js;

use crate::assets::{self, AssetResolver, ImageFacts, ImageRequest};
use crate::component::{asset_facts, asset_path, find_import, is_asset_record, HeadRow, RAW_ESCAPE};
use crate::{prop_name, Lowered, Lowerer, Residue};

/// Where an attribute was written, for the residue that refuses it.
#[derive(Debug, Clone, Copy)]
pub(crate) enum At {
  Span(Span),
  Line(usize, usize),
}

/// An attribute as the front end read it.
pub(crate) enum Value<'v> {
  /// Written with no value, which is `true`.
  Present,
  /// A static string.
  Text(String),
  /// An expression, lowered here.
  Script(Cow<'v, js::Expr>),
  /// `{...rest}`, whose entries become attributes.
  Spread(Cow<'v, js::Expr>),
}

pub(crate) struct Attr<'v> {
  /// The name as written: `className`, `aria-current`, `href`. Empty for a spread.
  pub name: String,
  pub value: Value<'v>,
  pub at: At,
}

/// What a placement needs from the front end that met it.
pub(crate) trait Placer<'p> {
  fn lowerer(&mut self) -> &mut Lowerer<'p>;
  fn residue_at(&self, at: At, message: String) -> Residue;
  fn assets(&self) -> Rc<dyn AssetResolver>;
  /// A head row the component asks for, such as a priority image's preload.
  fn head(&mut self, row: HeadRow);
}

fn refuse<'p>(cx: &impl Placer<'p>, at: At, message: impl Into<String>) -> Residue {
  cx.residue_at(at, message.into())
}

/// The attribute's value lowered, `true` when it was written bare.
pub(crate) fn value<'p>(cx: &mut impl Placer<'p>, attr: &Attr<'_>) -> Lowered<Expr> {
  match &attr.value {
    Value::Present => Ok(Expr::Lit(Lit::Bool(true))),
    Value::Text(text) => Ok(Expr::lit_str(text.clone())),
    Value::Script(expr) => cx.lowerer().expr(expr),
    Value::Spread(_) => Err(refuse(cx, attr.at, "a spread where one value is wanted")),
  }
}

/// `style`: a string as written, or an object literal keyed by CSS name,
/// which the renderer serialises the way the owner's framework does.
pub(crate) fn style<'p>(cx: &mut impl Placer<'p>, attr: &Attr<'_>) -> Lowered<Expr> {
  let Value::Script(expr) = &attr.value else { return value(cx, attr) };
  let js::Expr::Object(obj) = &**expr else {
    return Err(refuse(cx, At::Span(expr.span()), "a style that is not an object literal"));
  };
  let mut entries = Vec::new();
  for prop in &obj.props {
    let (key, value) = match prop {
      js::PropOrSpread::Spread(spread) => {
        entries.push(Entry::Spread(cx.lowerer().expr(&spread.expr)?));
        continue;
      }
      js::PropOrSpread::Prop(p) => match &**p {
        js::Prop::KeyValue(kv) => {
          let Some(key) = prop_name(&kv.key) else { return Err(refuse(cx, At::Span(kv.key.span()), "a computed style property")) };
          (key, cx.lowerer().expr(&kv.value)?)
        }
        js::Prop::Shorthand(id) => (id.sym.to_string(), cx.lowerer().ident(id)?),
        other => return Err(refuse(cx, At::Span(other.span()), "a method in a style")),
      },
    };
    entries.push(Entry::Field(css_name(&key), value));
  }
  Ok(Expr::Object(entries))
}

pub(crate) fn css_name(key: &str) -> String {
  let mut out = String::with_capacity(key.len() + 4);
  for c in key.chars() {
    if c.is_ascii_uppercase() {
      out.push('-');
      out.push(c.to_ascii_lowercase());
    } else {
      out.push(c);
    }
  }
  out
}

/// When the navigator calls a link the current page.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Match {
  /// That path exactly.
  Exact,
  /// That path and anything under it.
  Prefix,
}

impl Match {
  fn marker(self) -> &'static str {
    match self {
      Match::Exact => "exact",
      Match::Prefix => "prefix",
    }
  }

  /// The `aria-current` a hit writes. A section link takes `true` rather than
  /// `page` so that a nav marking both the section and the page inside it
  /// still names one page.
  fn current(self) -> &'static str {
    match self {
      Match::Exact => "page",
      Match::Prefix => "true",
    }
  }
}

/// The two attributes an active link carries: the rule, which the navigator
/// reads to keep the mark right after a navigation the layout does not
/// re-render for and `aria-current` for the page `by` names, the request's
/// path or the document's. An `href` carrying a query or a fragment never
/// matches, since the path the request matched holds neither.
fn active_attrs(rule: Match, href: Expr, by: Expr) -> [Entry; 2] {
  let hit = Expr::Compare(CompareOp::Eq, Box::new(by.clone()), Box::new(href.clone()));
  let hit = match rule {
    Match::Exact => hit,
    Match::Prefix => {
      let under = Expr::Builtin { name: Builtin::StartsWith, args: vec![by, Expr::Template(vec![href, Expr::lit_str("/")])] };
      Expr::Logic(LogicOp::Or, Box::new(hit), Box::new(under))
    }
  };
  let current = Expr::Ternary(Box::new(hit), Box::new(Expr::lit_str(rule.current())), Box::new(Expr::Lit(Lit::Null)));
  [Entry::Field("data-sf-link".to_owned(), Expr::lit_str(rule.marker())), Entry::Field("aria-current".to_owned(), current)]
}

/// A `Link`'s `keep` as the navigator reads it: `"true"` or `"false"`, since a
/// false attribute is dropped from the markup and the navigator reads a
/// missing one as leaving it the choice.
fn keep_value(value: Expr) -> Expr {
  match value {
    Expr::Lit(Lit::Bool(keep)) => Expr::lit_str(if keep { "true" } else { "false" }),
    value => Expr::Ternary(Box::new(value), Box::new(Expr::lit_str("true")), Box::new(Expr::lit_str("false"))),
  }
}

/// `<Link href=… match=… current=… full into prefetch native keep>`: an
/// `<a>` with the marks the navigator reads.
pub(crate) fn link<'p>(cx: &mut impl Placer<'p>, attrs: &[Attr<'_>], children: Vec<Tmpl>) -> Lowered<Tmpl> {
  let mut out = Vec::new();
  let mut href = None;
  let mut rule = Some(Match::Exact);
  let mut marked = false;
  let mut by_document = false;
  for attr in attrs {
    if let Value::Spread(expr) = &attr.value {
      out.push(Entry::Spread(cx.lowerer().expr(expr)?));
      continue;
    }
    match attr.name.as_str() {
      "key" | "ref" => continue,
      "style" => {
        out.push(Entry::Field("style".to_owned(), style(cx, attr)?));
        continue;
      }
      "match" => {
        rule = match value(cx, attr)? {
          Expr::Lit(Lit::Str(rule)) if rule == "exact" => Some(Match::Exact),
          Expr::Lit(Lit::Str(rule)) if rule == "prefix" => Some(Match::Prefix),
          Expr::Lit(Lit::Str(rule)) if rule == "none" => None,
          _ => return Err(refuse(cx, attr.at, "a `<Link>`'s `match` is \"exact\", \"prefix\" or \"none\", written out")),
        };
        continue;
      }
      "current" => {
        by_document = match value(cx, attr)? {
          Expr::Lit(Lit::Str(by)) if by == "url" => false,
          Expr::Lit(Lit::Str(by)) if by == "document" => true,
          _ => return Err(refuse(cx, attr.at, "a `<Link>`'s `current` is \"url\" or \"document\", written out")),
        };
        continue;
      }
      _ => {}
    }
    let lowered = value(cx, attr)?;
    if attr.name == "href" {
      href = Some(lowered.clone());
    }
    if attr.name == "aria-current" {
      marked = true;
    }
    let (name, lowered) = match attr.name.as_str() {
      "full" => ("data-sf-full", lowered),
      "into" => ("data-sf-into", lowered),
      "prefetch" => ("data-sf-prefetch", lowered),
      "native" => ("data-sf-native", lowered),
      "keep" => ("data-sf-keep", keep_value(lowered)),
      other => (html_attr_name(other), lowered),
    };
    out.push(Entry::Field(name.to_owned(), lowered));
  }
  if let (Some(rule), Some(href), false) = (rule, href, marked) {
    if by_document {
      out.push(Entry::Field("data-sf-current".to_owned(), Expr::lit_str("document")));
    }
    out.extend(active_attrs(rule, href, if by_document { Expr::Document } else { Expr::Path }));
  }
  Ok(Tmpl::Element { tag: "a".to_owned(), attrs: out, children })
}

/// The timing an island or a `Mount` takes, refused unless it is written out.
pub(crate) fn timing<'p>(cx: &impl Placer<'p>, value: Expr, at: At) -> Lowered<String> {
  match value {
    Expr::Lit(Lit::Str(timing)) if matches!(timing.as_str(), "load" | "visible" | "idle") => Ok(timing),
    _ => Err(refuse(cx, at, "an island's `when` is \"load\", \"visible\" or \"idle\", written out")),
  }
}

/// `<Mount module=… props=… when=…>`: the empty region the adapter's
/// `Mount` renders and then fills in the browser, since the island it places
/// is named at run time and the server renders none of it.
pub(crate) fn mount<'p>(cx: &mut impl Placer<'p>, attrs: &[Attr<'_>], at: At) -> Lowered<Tmpl> {
  let mut out = vec![Entry::Field("data-sf-island".to_owned(), Expr::lit_str(""))];
  for attr in attrs {
    if let Value::Spread(_) = attr.value {
      return Err(refuse(cx, at, "a spread on `<Mount>`"));
    }
    match attr.name.as_str() {
      "when" => {
        let lowered = value(cx, attr)?;
        let when = timing(cx, lowered, attr.at)?;
        out.push(Entry::Field("data-sf-when".to_owned(), Expr::lit_str(when)));
      }
      "module" | "props" | "key" => {}
      _ => return Err(refuse(cx, attr.at, "`<Mount>` takes `module`, `props` and `when` and nothing else")),
    }
  }
  Ok(Tmpl::Element { tag: "sf-s".to_owned(), attrs: out, children: Vec::new() })
}

enum PictureSrc {
  Local(String),
  Value(Expr),
  /// `PHOTOS[key]` where `PHOTOS` is an object of imported images, declared
  /// here or imported: the key expression and each entry's image, one branch
  /// per entry.
  Keyed(Expr, Vec<(String, ImageFacts)>),
}

/// `template` with `{src}` as `value` and `{width}` as `width`, as the
/// concatenation the renderer evaluates.
fn fill_template(template: &str, value: &Expr, width: u32) -> Expr {
  let mut parts: Vec<Expr> = Vec::new();
  let mut rest = template;
  loop {
    let src_at = rest.find("{src}");
    let width_at = rest.find("{width}");
    let next = match (src_at, width_at) {
      (Some(a), Some(b)) => Some(if a < b { (a, "{src}") } else { (b, "{width}") }),
      (Some(a), None) => Some((a, "{src}")),
      (None, Some(b)) => Some((b, "{width}")),
      (None, None) => None,
    };
    let Some((at, token)) = next else {
      if !rest.is_empty() {
        parts.push(Expr::lit_str(rest));
      }
      break;
    };
    if at > 0 {
      parts.push(Expr::lit_str(&rest[..at]));
    }
    match token {
      "{src}" => parts.push(value.clone()),
      _ => parts.push(Expr::lit_str(width.to_string())),
    }
    rest = &rest[at + token.len()..];
  }
  match parts.len() {
    1 => parts.pop().unwrap(),
    _ => Expr::Template(parts),
  }
}

/// A number literal with no fraction, however it was spelled.
pub(crate) fn whole_number(expr: &Expr) -> Option<i128> {
  match expr {
    Expr::Lit(Lit::Int(n)) => Some(*n),
    Expr::Lit(Lit::Float(f)) if f.fract() == 0.0 => Some(*f as i128),
    _ => None,
  }
}

/// The entries of a record of imported images bound as a global, as key
/// and image, or `None` for any other value.
fn asset_record(expr: &Expr) -> Option<Vec<(String, ImageFacts)>> {
  if !is_asset_record(expr) {
    return None;
  }
  let Expr::Object(entries) = expr else { return None };
  entries
    .iter()
    .map(|e| match e {
      Entry::Field(key, value) => asset_facts(value).map(|facts| (key.clone(), facts)),
      _ => None,
    })
    .collect()
}

/// The app-relative path of the image `local` imports, when it is one.
pub(crate) fn asset_import(lowerer: &Lowerer<'_>, local: &str) -> Option<String> {
  let (source, imported) = find_import(lowerer.parsed, local)?;
  if imported != "default" || assets::kind(&source) != Some(assets::Kind::Image) {
    return None;
  }
  asset_path(&lowerer.parsed.file, &source)
}

/// A `src`: the imported asset itself, its `.src`, an entry of an object
/// of imports indexed by a key, or any other value.
fn picture_src<'p>(cx: &mut impl Placer<'p>, attr: &Attr<'_>) -> Lowered<PictureSrc> {
  if let Value::Script(expr) = &attr.value {
    match &**expr {
      js::Expr::Ident(id) => {
        if let Some(path) = asset_import(cx.lowerer(), id.sym.as_ref()) {
          return Ok(PictureSrc::Local(path));
        }
      }
      js::Expr::Member(m) => {
        if let (js::Expr::Ident(obj), js::MemberProp::Ident(prop)) = (&*m.obj, &m.prop) {
          if prop.sym.as_ref() == "src" {
            if let Some(path) = asset_import(cx.lowerer(), obj.sym.as_ref()) {
              return Ok(PictureSrc::Local(path));
            }
          }
        }
        if let (js::Expr::Ident(obj), js::MemberProp::Computed(key)) = (&*m.obj, &m.prop) {
          let bound = cx.lowerer().ident(obj)?;
          if let Some(entries) = asset_record(&bound) {
            let key = cx.lowerer().expr(&key.expr)?;
            return Ok(PictureSrc::Keyed(key, entries));
          }
        }
      }
      _ => {}
    }
  }
  match value(cx, attr)? {
    Expr::Lit(Lit::Str(literal)) if literal.starts_with('.') => Err(refuse(cx, attr.at, format!("`src=\"{literal}\"` is a path, which resolves against the page in a browser; import the file and pass the import"))),
    lowered => Ok(PictureSrc::Value(lowered)),
  }
}

fn picture_widths<'p>(cx: &mut impl Placer<'p>, attr: &Attr<'_>) -> Lowered<Vec<u32>> {
  let message = "a `<Picture>`'s `widths` is an array of positive numbers written out";
  let Expr::Array(items) = value(cx, attr)? else {
    return Err(refuse(cx, attr.at, "a `<Picture>`'s `widths` is an array of numbers written out"));
  };
  let mut widths = Vec::new();
  for item in items {
    match item {
      Entry::Item(expr) => match whole_number(&expr) {
        Some(n) if n > 0 => widths.push(n as u32),
        _ => return Err(refuse(cx, attr.at, message)),
      },
      _ => return Err(refuse(cx, attr.at, message)),
    }
  }
  Ok(widths)
}

fn picture_quality<'p>(cx: &mut impl Placer<'p>, attr: &Attr<'_>) -> Lowered<std::collections::BTreeMap<String, u8>> {
  let message = "a `<Picture>`'s `quality` is a number or `{ avif, webp }` of numbers, 0 to 100, written out";
  let mut quality = std::collections::BTreeMap::new();
  match value(cx, attr)? {
    Expr::Object(entries) => {
      for entry in entries {
        match entry {
          Entry::Field(format, v) if format == "avif" || format == "webp" => match whole_number(&v) {
            Some(n) if (0..=100).contains(&n) => {
              quality.insert(format, n as u8);
            }
            _ => return Err(refuse(cx, attr.at, message)),
          },
          _ => return Err(refuse(cx, attr.at, message)),
        }
      }
    }
    lowered => match whole_number(&lowered) {
      Some(n) if (0..=100).contains(&n) => {
        quality.insert("avif".to_owned(), n as u8);
        quality.insert("webp".to_owned(), n as u8);
      }
      _ => return Err(refuse(cx, attr.at, message)),
    },
  }
  Ok(quality)
}

/// `<Picture src={hero} alt="…" sizes="…" priority widths={[…]} quality={…} source="cms">`,
/// or an `<img>` of an imported asset under the rewrite: a `<picture>`
/// with a `<source>` per format and the hashed original as its `<img>`,
/// an `<img>` alone for an image served as it is, and for a string `src`
/// an `<img>` whose `srcset` a named source's template writes.
pub(crate) fn picture<'p>(cx: &mut impl Placer<'p>, attrs: &[Attr<'_>], at: At, from_img: bool) -> Lowered<Tmpl> {
  let mut src: Option<PictureSrc> = None;
  let mut sizes: Option<Expr> = None;
  let mut priority = false;
  let mut request = ImageRequest::default();
  let mut source_name: Option<String> = None;
  let mut rest: Vec<Entry> = Vec::new();
  let mut explicit_loading = None;
  let mut explicit_decoding = None;
  let mut wrote_dimensions = false;
  for attr in attrs {
    if let Value::Spread(expr) = &attr.value {
      rest.push(Entry::Spread(cx.lowerer().expr(expr)?));
      continue;
    }
    match attr.name.as_str() {
      "key" | "ref" => {}
      name if name == RAW_ESCAPE => {}
      "src" => src = Some(picture_src(cx, attr)?),
      "sizes" => sizes = Some(value(cx, attr)?),
      "priority" => priority = matches!(value(cx, attr)?, Expr::Lit(Lit::Bool(true))),
      "fetchPriority" | "fetchpriority" => {
        if matches!(value(cx, attr)?, Expr::Lit(Lit::Str(ref v)) if v == "high") {
          priority = true;
        }
      }
      "widths" => request.widths = Some(picture_widths(cx, attr)?),
      "quality" => request.quality = Some(picture_quality(cx, attr)?),
      "source" => match value(cx, attr)? {
        Expr::Lit(Lit::Str(name)) => source_name = Some(name),
        _ => return Err(refuse(cx, attr.at, "a `<Picture>`'s `source` is a name written out")),
      },
      "loading" => explicit_loading = Some(value(cx, attr)?),
      "decoding" => explicit_decoding = Some(value(cx, attr)?),
      "style" => rest.push(Entry::Field("style".to_owned(), style(cx, attr)?)),
      "width" | "height" => {
        wrote_dimensions = true;
        let lowered = value(cx, attr)?;
        rest.push(Entry::Field(attr.name.clone(), lowered));
      }
      other => {
        let lowered = value(cx, attr)?;
        rest.push(Entry::Field(html_attr_name(other).to_owned(), lowered));
      }
    }
  }
  let Some(src) = src else {
    return Err(refuse(cx, at, if from_img { "an `<img>` with no `src`" } else { "a `<Picture>` needs a `src`" }));
  };
  let loading = explicit_loading.unwrap_or_else(|| Expr::lit_str(if priority { "eager" } else { "lazy" }));
  let decoding = explicit_decoding.unwrap_or_else(|| Expr::lit_str("async"));
  let resolver = cx.assets();
  match src {
    PictureSrc::Local(path) => {
      let Some(facts) = resolver.image(&path, &request) else {
        return Err(refuse(cx, at, format!("`{path}` is not an image under the app")));
      };
      Ok(picture_markup(cx, &facts, &rest, sizes, priority, true, loading, decoding, wrote_dimensions))
    }
    PictureSrc::Keyed(key, entries) => {
      // Lowered once per entry with the same attributes, then chained:
      // `key == "1.png" ? <picture one> : key == "2.png" ? … : nothing`.
      let mut chain: Option<Tmpl> = None;
      for (entry_key, facts) in entries.into_iter().rev() {
        let branch = picture_markup(cx, &facts, &rest, sizes.clone(), priority, false, loading.clone(), decoding.clone(), wrote_dimensions);
        let cond = Expr::Compare(CompareOp::Eq, Box::new(key.clone()), Box::new(Expr::lit_str(entry_key)));
        chain = Some(match chain {
          Some(rest_chain) => Tmpl::If { cond, then: Box::new(branch), r#else: Some(Box::new(rest_chain)) },
          None => Tmpl::If { cond, then: Box::new(branch), r#else: None },
        });
      }
      Ok(chain.expect("a record has an entry"))
    }
    PictureSrc::Value(lowered) => {
      let template = match &source_name {
        Some(name) => match resolver.source(name) {
          Some(template) => Some(template),
          None => return Err(refuse(cx, at, format!("`{name}` is not an `[images.sources]` entry"))),
        },
        None => None,
      };
      let mut img: Vec<Entry> = Vec::new();
      match template {
        Some(template) => {
          let widths = resolver.widths();
          let largest = widths.iter().copied().max().unwrap_or(1280);
          img.push(Entry::Field("src".to_owned(), fill_template(&template, &lowered, largest)));
          let mut parts: Vec<Expr> = Vec::new();
          for (i, width) in widths.iter().enumerate() {
            if i > 0 {
              parts.push(Expr::lit_str(", "));
            }
            parts.push(fill_template(&template, &lowered, *width));
            parts.push(Expr::lit_str(format!(" {width}w")));
          }
          if !parts.is_empty() {
            img.push(Entry::Field("srcset".to_owned(), Expr::Template(parts)));
            img.push(Entry::Field("sizes".to_owned(), sizes.unwrap_or_else(|| Expr::lit_str("100vw"))));
          }
        }
        None => img.push(Entry::Field("src".to_owned(), lowered)),
      }
      img.extend(rest);
      img.push(Entry::Field("loading".to_owned(), loading));
      img.push(Entry::Field("decoding".to_owned(), decoding));
      if priority {
        img.push(Entry::Field("fetchpriority".to_owned(), Expr::lit_str("high")));
      }
      Ok(Tmpl::Element { tag: "img".to_owned(), attrs: img, children: Vec::new() })
    }
  }
}

/// The markup for one local image: a `<picture>` with a `<source>` per
/// format and the hashed original as its `<img>`, or the `<img>` alone for
/// an image served as it is. A priority image also asks the head for a
/// preload of its first format when `preload`, which a keyed record never
/// is: the key is a value, so the page's `meta` names the one to preload.
#[allow(clippy::too_many_arguments)]
fn picture_markup<'p>(cx: &mut impl Placer<'p>, facts: &ImageFacts, rest: &[Entry], sizes: Option<Expr>, priority: bool, preload: bool, loading: Expr, decoding: Expr, wrote_dimensions: bool) -> Tmpl {
  let mut img: Vec<Entry> = vec![Entry::Field("src".to_owned(), Expr::lit_str(facts.src.clone()))];
  if !wrote_dimensions {
    img.push(Entry::Field("width".to_owned(), Expr::Lit(Lit::Int(i128::from(facts.width)))));
    img.push(Entry::Field("height".to_owned(), Expr::Lit(Lit::Int(i128::from(facts.height)))));
  }
  img.extend(rest.iter().cloned());
  img.push(Entry::Field("loading".to_owned(), loading));
  img.push(Entry::Field("decoding".to_owned(), decoding));
  if priority {
    img.push(Entry::Field("fetchpriority".to_owned(), Expr::lit_str("high")));
  }
  if facts.sources.is_empty() {
    return Tmpl::Element { tag: "img".to_owned(), attrs: img, children: Vec::new() };
  }
  let sizes = sizes.unwrap_or_else(|| Expr::lit_str(assets::default_sizes(facts.width)));
  let mut children = Vec::new();
  for (mime, srcset) in &facts.sources {
    children.push(Tmpl::Element {
      tag: "source".to_owned(),
      attrs: vec![
        Entry::Field("type".to_owned(), Expr::lit_str(mime.clone())),
        Entry::Field("srcset".to_owned(), Expr::lit_str(srcset.clone())),
        Entry::Field("sizes".to_owned(), sizes.clone()),
      ],
      children: Vec::new(),
    });
  }
  if priority && preload {
    if let (Some((mime, srcset)), Expr::Lit(Lit::Str(sizes))) = (facts.sources.first(), &sizes) {
      cx.head(HeadRow {
        tag: "link".to_owned(),
        attrs: vec![
          ("rel".to_owned(), "preload".to_owned()),
          ("as".to_owned(), "image".to_owned()),
          ("type".to_owned(), mime.clone()),
          ("imagesrcset".to_owned(), srcset.clone()),
          ("imagesizes".to_owned(), sizes.clone()),
          ("fetchpriority".to_owned(), "high".to_owned()),
        ],
      });
    }
  }
  children.push(Tmpl::Element { tag: "img".to_owned(), attrs: img, children: Vec::new() });
  Tmpl::Element { tag: "picture".to_owned(), attrs: Vec::new(), children }
}
