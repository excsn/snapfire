//! MDX and markdown for SnapFire FSR. [`convert`] turns an `.mdx` or `.md`
//! file into a TSX module whose default export renders it, the way
//! `@mdx-js/mdx` compiles one, with a map from every byte it wrote back to the
//! source so a diagnostic names the line the author wrote.

use std::collections::{BTreeMap, BTreeSet, HashMap};

use markdown::mdast::{self, AttributeContent, AttributeValue, Node};

#[cfg(feature = "frontmatter")]
mod frontmatter;
#[cfg(feature = "highlight")]
pub mod highlight;
mod js;
mod slug;

/// What a file is written in, by its extension.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Format {
  /// `.mdx`: markdown with JSX, expressions and ESM.
  Mdx,
  /// `.md`: markdown alone, MDX's `md` format.
  Md,
}

impl Format {
  pub fn of(path: &str) -> Option<Format> {
    match path.rsplit_once('.').map(|(_, ext)| ext.to_ascii_lowercase()).as_deref() {
      Some("mdx") => Some(Format::Mdx),
      Some("md") => Some(Format::Md),
      _ => None,
    }
  }
}

/// Which module the conversion writes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Target {
  /// For the build to lower: each element the component map names is placed as
  /// the component it maps to, imported by name.
  Lowered,
  /// For the browser: the map is read at render time through
  /// `useMDXComponents` and the `components` prop, as MDX's own output does.
  Browser,
}

/// The attribute a class is written as, which is the application's JSX source.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClassAttr {
  ClassName,
  Class,
}

impl ClassAttr {
  fn name(self) -> &'static str {
    match self {
      ClassAttr::ClassName => "className",
      ClassAttr::Class => "class",
    }
  }
}

/// One element the lowered module places as a component: `key` is the tag a
/// markdown construct writes (`h1`, `a`, `pre`) or `wrapper`, and the
/// component is `export` of `specifier`, relative to the file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Mapped {
  pub key: String,
  pub specifier: String,
  pub export: String,
}

#[derive(Debug, Clone)]
pub struct Options {
  pub format: Format,
  pub target: Target,
  /// GFM: tables, strikethrough, task lists, footnotes and literal autolinks.
  pub gfm: bool,
  /// An `id` on every heading and a `toc` export listing them.
  pub heading_ids: bool,
  /// YAML or TOML frontmatter written as `export const meta`.
  pub frontmatter: bool,
  /// Fenced code highlighted into classed spans.
  pub highlight: bool,
  pub class_attr: ClassAttr,
  /// [`Target::Lowered`]: the elements placed as components.
  pub mapped: Vec<Mapped>,
  /// [`Target::Browser`]: the module whose `useMDXComponents` supplies the
  /// map, relative to the file; `None` when the application has none.
  pub provider: Option<String>,
}

impl Options {
  pub fn new(format: Format, target: Target) -> Self {
    Self { format, target, gfm: true, heading_ids: true, frontmatter: false, highlight: false, class_attr: ClassAttr::ClassName, mapped: Vec::new(), provider: None }
  }
}

/// A run of the written module and where it came from. A verbatim run is
/// source text copied as it stands, so each of its bytes maps to the byte at
/// the same distance from `original`; any other run maps whole to `original`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Segment {
  pub generated: usize,
  pub len: usize,
  pub original: usize,
  pub verbatim: bool,
}

/// One heading, for the `toc` export.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TocEntry {
  pub depth: u8,
  pub id: String,
  pub text: String,
}

/// Something the conversion left out, with the one-based line and column it sits at.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Warning {
  pub line: usize,
  pub column: usize,
  pub message: String,
}

#[derive(Debug, Clone)]
pub struct Converted {
  pub tsx: String,
  pub segments: Vec<Segment>,
  pub toc: Vec<TocEntry>,
  /// The file exports `meta`, written or from frontmatter.
  pub meta: bool,
  /// The names the file exports, `default` aside.
  pub exports: Vec<String>,
  /// The text of the first paragraph at the top level with any, whitespace
  /// collapsed; an expression or JSX inside it contributes nothing.
  pub excerpt: Option<String>,
  /// Every class the highlighter wrote.
  pub classes: BTreeSet<String>,
  /// The keys of [`Options::mapped`] the module places.
  pub placed: BTreeSet<String>,
  pub warnings: Vec<Warning>,
}

impl Converted {
  /// The one-based line and column in the source of byte `offset` of [`Converted::tsx`].
  pub fn origin(&self, source: &str, offset: usize) -> (usize, usize) {
    position(source, original_offset(&self.segments, offset))
  }
}

/// The source byte that written byte `offset` came from.
pub fn original_offset(segments: &[Segment], offset: usize) -> usize {
  let index = segments.partition_point(|s| s.generated <= offset);
  let Some(segment) = index.checked_sub(1).map(|i| segments[i]) else { return 0 };
  match segment.verbatim && offset < segment.generated + segment.len.max(1) {
    true => segment.original + (offset - segment.generated),
    false => segment.original,
  }
}

/// The one-based line and column, in characters, of byte `offset` of `source`.
pub fn position(source: &str, offset: usize) -> (usize, usize) {
  let offset = offset.min(source.len());
  let before = &source[..source.floor_char_boundary(offset)];
  let line = before.matches('\n').count() + 1;
  let column = before.rsplit('\n').next().map(|l| l.chars().count()).unwrap_or(0) + 1;
  (line, column)
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum Error {
  /// The file does not parse as what its extension says.
  #[error("{line}:{column}: {message}")]
  Syntax { line: usize, column: usize, message: String },
  /// An option asks for what this build of the crate does not carry.
  #[error("{0} needs snapfire_fsr_mdx's `{1}` feature")]
  Feature(&'static str, &'static str),
}

/// The tags markdown constructs write, which the component map can replace.
pub const ELEMENTS: &[&str] = &["a", "blockquote", "br", "code", "del", "em", "h1", "h2", "h3", "h4", "h5", "h6", "hr", "img", "input", "li", "ol", "p", "pre", "section", "strong", "sup", "table", "tbody", "td", "th", "thead", "tr", "ul"];

/// The key that wraps the whole content, as MDX's `wrapper`.
pub const WRAPPER: &str = "wrapper";

/// The name the generated default export carries.
pub const CONTENT: &str = "MDXContent";

pub fn convert(source: &str, options: &Options) -> Result<Converted, Error> {
  if options.highlight && !cfg!(feature = "highlight") {
    return Err(Error::Feature("highlighted code", "highlight"));
  }
  if options.frontmatter && !cfg!(feature = "frontmatter") {
    return Err(Error::Feature("frontmatter", "frontmatter"));
  }
  let parse = parse_options(options);
  let root = markdown::to_mdast(source, &parse).map_err(|message| {
    let (line, column) = match message.place.as_deref() {
      Some(markdown::message::Place::Point(p)) => (p.line, p.column),
      Some(markdown::message::Place::Position(p)) => (p.start.line, p.start.column),
      None => (1, 1),
    };
    Error::Syntax { line, column, message: message.reason }
  })?;
  let mut emitter = Emitter::new(source, options);
  emitter.collect(&root);
  emitter.module(&root)?;
  Ok(emitter.finish())
}

fn parse_options(options: &Options) -> markdown::ParseOptions {
  let mut constructs = match options.format {
    Format::Mdx => markdown::Constructs::mdx(),
    Format::Md => markdown::Constructs::default(),
  };
  if options.gfm {
    let gfm = markdown::Constructs::gfm();
    constructs.gfm_autolink_literal = gfm.gfm_autolink_literal;
    constructs.gfm_footnote_definition = gfm.gfm_footnote_definition;
    constructs.gfm_label_start_footnote = gfm.gfm_label_start_footnote;
    constructs.gfm_strikethrough = gfm.gfm_strikethrough;
    constructs.gfm_table = gfm.gfm_table;
    constructs.gfm_task_list_item = gfm.gfm_task_list_item;
  }
  constructs.frontmatter = options.frontmatter;
  let mdx = options.format == Format::Mdx;
  markdown::ParseOptions {
    constructs,
    mdx_esm_parse: mdx.then(|| Box::new(js::esm) as Box<markdown::MdxEsmParse>),
    mdx_expression_parse: mdx.then(|| Box::new(js::expression) as Box<markdown::MdxExpressionParse>),
    ..markdown::ParseOptions::default()
  }
}

fn json(text: &str) -> String {
  serde_json::to_string(text).expect("a string serializes")
}

fn start(node: &Node) -> usize {
  node.position().map(|p| p.start.offset).unwrap_or(0)
}

/// The text a heading or a link reads as, for an id and a table of contents.
fn plain(nodes: &[Node]) -> String {
  let mut out = String::new();
  for node in nodes {
    match node {
      Node::Text(t) => out.push_str(&t.value),
      Node::InlineCode(c) => out.push_str(&c.value),
      Node::Image(i) => out.push_str(&i.alt),
      other => {
        if let Some(children) = other.children() {
          out.push_str(&plain(children));
        }
      }
    }
  }
  out
}

struct Footnote {
  number: usize,
  refs: usize,
}

struct Emitter<'a> {
  src: &'a str,
  options: &'a Options,
  out: String,
  segments: Vec<Segment>,
  definitions: HashMap<String, (String, Option<String>)>,
  footnote_defs: HashMap<String, &'a mdast::FootnoteDefinition>,
  footnotes: BTreeMap<String, Footnote>,
  footnote_order: Vec<String>,
  slugger: slug::Slugger,
  toc: Vec<TocEntry>,
  meta: bool,
  exports: Vec<String>,
  excerpt: Option<String>,
  layout: Option<String>,
  classes: BTreeSet<String>,
  placed: BTreeSet<String>,
  used: BTreeSet<&'static str>,
  warnings: Vec<Warning>,
  mapped: HashMap<&'a str, &'a Mapped>,
}

impl<'a> Emitter<'a> {
  fn new(src: &'a str, options: &'a Options) -> Self {
    Self {
      src,
      options,
      out: String::new(),
      segments: Vec::new(),
      definitions: HashMap::new(),
      footnote_defs: HashMap::new(),
      footnotes: BTreeMap::new(),
      footnote_order: Vec::new(),
      slugger: slug::Slugger::default(),
      toc: Vec::new(),
      meta: false,
      exports: Vec::new(),
      excerpt: None,
      layout: None,
      classes: BTreeSet::new(),
      placed: BTreeSet::new(),
      used: BTreeSet::new(),
      warnings: Vec::new(),
      mapped: options.mapped.iter().map(|m| (m.key.as_str(), m)).collect(),
    }
  }

  fn finish(self) -> Converted {
    Converted { tsx: self.out, segments: self.segments, toc: self.toc, meta: self.meta, exports: self.exports, excerpt: self.excerpt, classes: self.classes, placed: self.placed, warnings: self.warnings }
  }

  fn push(&mut self, text: &str, original: usize, verbatim: bool) {
    if text.is_empty() {
      return;
    }
    self.segments.push(Segment { generated: self.out.len(), len: text.len(), original, verbatim });
    self.out.push_str(text);
  }

  fn synth(&mut self, text: &str, at: usize) {
    self.push(text, at, false);
  }

  /// `value` as markdown-rs collected it, each run mapped to where it sits in the source.
  fn verbatim(&mut self, value: &str, stops: &[(usize, usize)], fallback: usize) {
    if stops.is_empty() {
      self.push(value, fallback, false);
      return;
    }
    for (i, (relative, absolute)) in stops.iter().enumerate() {
      let end = stops.get(i + 1).map(|s| s.0).unwrap_or(value.len());
      if *relative < end {
        self.push(&value[*relative..end], *absolute, true);
      }
    }
  }

  fn warn(&mut self, at: usize, message: impl Into<String>) {
    let (line, column) = position(self.src, at);
    self.warnings.push(Warning { line, column, message: message.into() });
  }

  fn collect(&mut self, node: &'a Node) {
    match node {
      Node::Definition(d) => {
        self.definitions.entry(d.identifier.clone()).or_insert_with(|| (d.url.clone(), d.title.clone()));
      }
      Node::FootnoteDefinition(f) => {
        self.footnote_defs.entry(f.identifier.clone()).or_insert(f);
      }
      _ => {}
    }
    if let Some(children) = node.children() {
      for child in children {
        self.collect(child);
      }
    }
  }

  fn module(&mut self, root: &'a Node) -> Result<(), Error> {
    let children = root.children().map(Vec::as_slice).unwrap_or(&[]);
    self.excerpt = children.iter().find_map(|node| match node {
      Node::Paragraph(p) => Some(plain(&p.children).split_whitespace().collect::<Vec<_>>().join(" ")).filter(|text| !text.is_empty()),
      _ => None,
    });
    let mut frontmatter = None;
    for node in children {
      match node {
        Node::MdxjsEsm(esm) => self.esm(esm)?,
        Node::Yaml(y) => frontmatter = Some((start(node), self.frontmatter_yaml(&y.value, start(node))?)),
        Node::Toml(t) => frontmatter = Some((start(node), self.frontmatter_toml(&t.value, start(node))?)),
        _ => {}
      }
    }
    if let Some((at, value)) = frontmatter {
      if self.meta {
        let (line, column) = position(self.src, at);
        return Err(Error::Syntax { line, column, message: "frontmatter and `export const meta` both set the file's meta; keep one".to_owned() });
      }
      self.synth(&format!("export const meta = {value};\n"), at);
      self.meta = true;
      self.exports.push("meta".to_owned());
    }
    match self.options.target {
      Target::Lowered => self.lowered_body(children),
      Target::Browser => self.browser_body(children),
    }
    if self.options.heading_ids {
      let entries: Vec<String> = self.toc.iter().map(|e| format!("{{ depth: {}, id: {}, text: {} }}", e.depth, json(&e.id), json(&e.text))).collect();
      self.synth(&format!("\nexport const toc = [{}];\n", entries.join(", ")), 0);
      self.exports.push("toc".to_owned());
    }
    Ok(())
  }

  fn lowered_body(&mut self, children: &'a [Node]) {
    let mark = self.out.len();
    // The wrapper takes the content as its children and no props, since a
    // spread of the page's props reads every prop the request can carry.
    let wrapped = self.layout.clone().or_else(|| self.mapped.contains_key(WRAPPER).then(|| format!("Mdx_{WRAPPER}")));
    if wrapped.as_deref() == Some(format!("Mdx_{WRAPPER}").as_str()) {
      self.placed.insert(WRAPPER.to_owned());
    }
    let (open, close) = match &wrapped {
      Some(wrapper) => (format!("<{wrapper}>"), format!("</{wrapper}>")),
      None => (String::new(), String::new()),
    };
    self.synth(&format!("\nexport default function {CONTENT}(props) {{\n  return {open}<>\n"), 0);
    self.flow(children);
    self.footnote_section();
    self.synth(&format!("</>{close};\n}}\n"), self.src.len());
    let imports: Vec<String> = self.placed.iter().filter_map(|key| self.mapped.get(key.as_str())).map(|m| format!("import {{ {} as Mdx_{} }} from {};\n", m.export, m.key, json(&m.specifier))).collect();
    self.insert_at(mark, &imports.concat());
  }

  fn browser_body(&mut self, children: &'a [Node]) {
    let mark = self.out.len();
    self.synth("\nfunction _createMdxContent(props) {\n", 0);
    let defaults_mark = self.out.len();
    self.synth("  return <>\n", 0);
    self.flow(children);
    self.footnote_section();
    self.synth("</>;\n}\n", self.src.len());
    let used: Vec<String> = self.used.iter().map(|tag| format!("{tag}: {}", json(tag))).collect();
    let provide = if self.options.provider.is_some() { "..._provideComponents(), " } else { "" };
    self.insert_at(defaults_mark, &format!("  const _components = {{ {}{}{provide}...props.components }};\n", used.join(", "), if used.is_empty() { "" } else { ", " }));
    let layout = match &self.layout {
      Some(layout) => format!("  const MDXWrapper = {layout};\n"),
      None => format!("  const {{ {WRAPPER}: MDXWrapper }} = {{ {provide}...props.components }};\n"),
    };
    self.synth(&format!("\nexport default function {CONTENT}(props = {{}}) {{\n{layout}  return MDXWrapper ? <MDXWrapper><_createMdxContent {{...props}} /></MDXWrapper> : _createMdxContent(props);\n}}\n"), 0);
    if let Some(provider) = &self.options.provider {
      let import = format!("import {{ useMDXComponents as _provideComponents }} from {};\n", json(provider));
      self.insert_at(mark, &import);
    }
  }

  /// Inserts synthetic `text` at byte `at` of the output, moving every later segment.
  fn insert_at(&mut self, at: usize, text: &str) {
    if text.is_empty() {
      return;
    }
    self.out.insert_str(at, text);
    let index = self.segments.partition_point(|s| s.generated < at);
    for segment in &mut self.segments[index..] {
      segment.generated += text.len();
    }
    self.segments.insert(index, Segment { generated: at, len: text.len(), original: 0, verbatim: false });
  }

  fn esm(&mut self, esm: &'a mdast::MdxjsEsm) -> Result<(), Error> {
    let facts = js::facts(&esm.value);
    if facts.exports.iter().any(|e| e == "meta") {
      self.meta = true;
    }
    self.exports.extend(facts.exports.iter().cloned());
    let at = esm.stops.first().map(|s| s.1).unwrap_or(0);
    match facts.default {
      None => self.verbatim(&esm.value, &esm.stops, at),
      Some(default) => {
        if self.layout.is_some() {
          let (line, column) = position(self.src, at);
          return Err(Error::Syntax { line, column, message: "a second `export default`; a file has one layout".to_owned() });
        }
        let named = default.name.filter(|n| n.starts_with(|c: char| c.is_ascii_uppercase()));
        let replacement = match &named {
          Some(_) => "",
          None => "const MDXLayout = ",
        };
        let stops = &esm.stops;
        let shift = |rel: usize| stops.iter().rev().find(|s| s.0 <= rel).map(|s| s.1 + (rel - s.0)).unwrap_or(at);
        let before = sub_stops(stops, 0, default.keywords.start);
        self.verbatim(&esm.value[..default.keywords.start], &before, at);
        self.synth(replacement, shift(default.keywords.start));
        let after = sub_stops(stops, default.keywords.end, esm.value.len());
        self.verbatim(&esm.value[default.keywords.end..], &after, shift(default.keywords.end));
        self.layout = Some(named.unwrap_or_else(|| "MDXLayout".to_owned()));
      }
    }
    self.synth("\n", at);
    Ok(())
  }

  #[cfg(feature = "frontmatter")]
  fn frontmatter_yaml(&mut self, text: &str, at: usize) -> Result<String, Error> {
    frontmatter::yaml(text).map(|v| v.to_string()).map_err(|message| self.syntax(at, format!("frontmatter: {message}")))
  }

  #[cfg(not(feature = "frontmatter"))]
  fn frontmatter_yaml(&mut self, _text: &str, _at: usize) -> Result<String, Error> {
    Err(Error::Feature("frontmatter", "frontmatter"))
  }

  #[cfg(feature = "frontmatter")]
  fn frontmatter_toml(&mut self, text: &str, at: usize) -> Result<String, Error> {
    frontmatter::toml(text).map(|v| v.to_string()).map_err(|message| self.syntax(at, format!("frontmatter: {message}")))
  }

  #[cfg(not(feature = "frontmatter"))]
  fn frontmatter_toml(&mut self, _text: &str, _at: usize) -> Result<String, Error> {
    Err(Error::Feature("frontmatter", "frontmatter"))
  }

  #[allow(dead_code)]
  fn syntax(&self, at: usize, message: String) -> Error {
    let (line, column) = position(self.src, at);
    Error::Syntax { line, column, message }
  }

  /// The tag a markdown construct writes `html` as.
  fn tag(&mut self, html: &'static str) -> String {
    match self.options.target {
      Target::Lowered => match self.mapped.contains_key(html) {
        true => {
          self.placed.insert(html.to_owned());
          format!("Mdx_{html}")
        }
        false => html.to_owned(),
      },
      Target::Browser => {
        self.used.insert(html);
        format!("_components.{html}")
      }
    }
  }

  fn open(&mut self, tag: &str, attrs: &[(&str, String)], at: usize, empty: bool) {
    let mut text = format!("<{tag}");
    for (name, value) in attrs {
      text.push(' ');
      text.push_str(name);
      text.push('=');
      text.push('{');
      text.push_str(value);
      text.push('}');
    }
    text.push_str(if empty { " />" } else { ">" });
    self.synth(&text, at);
  }

  fn close(&mut self, tag: &str, at: usize) {
    self.synth(&format!("</{tag}>"), at);
  }

  fn element(&mut self, html: &'static str, attrs: &[(&str, String)], children: &'a [Node], at: usize, flow: bool) {
    let tag = self.tag(html);
    self.open(&tag, attrs, at, false);
    match flow {
      true => {
        self.synth("\n", at);
        self.flow(children);
      }
      false => self.phrasing(children),
    }
    self.close(&tag, at);
  }

  fn void(&mut self, html: &'static str, attrs: &[(&str, String)], at: usize) {
    let tag = self.tag(html);
    self.open(&tag, attrs, at, true);
  }

  fn text(&mut self, value: &str, at: usize) {
    if !value.is_empty() {
      self.synth(&format!("{{{}}}", json(value)), at);
    }
  }

  fn flow(&mut self, nodes: &'a [Node]) {
    for node in nodes {
      let before = self.out.len();
      self.node(node, true);
      if self.out.len() != before {
        self.synth("\n", start(node));
      }
    }
  }

  fn phrasing(&mut self, nodes: &'a [Node]) {
    for node in nodes {
      self.node(node, false);
    }
  }

  fn class(&self) -> &'static str {
    self.options.class_attr.name()
  }

  fn node(&mut self, node: &'a Node, flow: bool) {
    let at = start(node);
    match node {
      Node::Root(r) => self.flow(&r.children),
      Node::Paragraph(p) => self.element("p", &[], &p.children, at, false),
      Node::Heading(h) => {
        let html = ["h1", "h2", "h3", "h4", "h5", "h6"][usize::from(h.depth.clamp(1, 6) - 1)];
        let mut attrs = Vec::new();
        if self.options.heading_ids {
          let text = plain(&h.children);
          let id = self.slugger.slug(&text);
          if !id.is_empty() {
            attrs.push(("id", json(&id)));
            self.toc.push(TocEntry { depth: h.depth, id, text });
          }
        }
        self.element(html, &attrs, &h.children, at, false);
      }
      Node::ThematicBreak(_) => self.void("hr", &[], at),
      Node::Blockquote(b) => self.element("blockquote", &[], &b.children, at, true),
      Node::List(list) => self.list(list, at),
      Node::ListItem(item) => self.list_item(item, true, at),
      Node::Html(_) => self.warn(at, "raw HTML in markdown is not rendered, as MDX leaves it out; write it as JSX in an .mdx file"),
      Node::Code(code) => self.code(code, at),
      Node::Math(_) | Node::InlineMath(_) => {}
      Node::Definition(_) | Node::FootnoteDefinition(_) | Node::MdxjsEsm(_) | Node::Yaml(_) | Node::Toml(_) => {}
      Node::Text(t) => self.text(&t.value, at),
      Node::Emphasis(e) => self.element("em", &[], &e.children, at, false),
      Node::Strong(s) => self.element("strong", &[], &s.children, at, false),
      Node::Delete(d) => self.element("del", &[], &d.children, at, false),
      Node::InlineCode(c) => {
        let tag = self.tag("code");
        self.open(&tag, &[], at, false);
        self.text(&c.value, at);
        self.close(&tag, at);
      }
      Node::Break(_) => {
        self.void("br", &[], at);
        self.text("\n", at);
      }
      Node::Link(l) => {
        let mut attrs = vec![("href", json(&l.url))];
        if let Some(title) = &l.title {
          attrs.push(("title", json(title)));
        }
        self.element("a", &attrs, &l.children, at, false);
      }
      Node::Image(i) => self.image(&i.url, &i.alt, i.title.as_deref(), at),
      Node::LinkReference(r) => match self.definitions.get(&r.identifier).cloned() {
        Some((url, title)) => {
          let mut attrs = vec![("href", json(&url))];
          if let Some(title) = &title {
            attrs.push(("title", json(title)));
          }
          self.element("a", &attrs, &r.children, at, false);
        }
        None => self.phrasing(&r.children),
      },
      Node::ImageReference(r) => match self.definitions.get(&r.identifier).cloned() {
        Some((url, title)) => self.image(&url, &r.alt, title.as_deref(), at),
        None => self.text(&r.alt, at),
      },
      Node::FootnoteReference(f) => self.footnote_reference(&f.identifier, at),
      Node::Table(t) => self.table(t, at),
      Node::TableRow(_) | Node::TableCell(_) => {}
      Node::MdxFlowExpression(e) => self.expression(&e.value, &e.stops, at),
      Node::MdxTextExpression(e) => self.expression(&e.value, &e.stops, at),
      Node::MdxJsxFlowElement(el) => self.jsx(el.name.as_deref(), &el.attributes, &el.children, at, true),
      Node::MdxJsxTextElement(el) => self.jsx(el.name.as_deref(), &el.attributes, &el.children, at, false),
    }
    let _ = flow;
  }

  fn image(&mut self, url: &str, alt: &str, title: Option<&str>, at: usize) {
    let mut attrs = vec![("src", json(url)), ("alt", json(alt))];
    if let Some(title) = title {
      attrs.push(("title", json(title)));
    }
    self.void("img", &attrs, at);
  }

  fn expression(&mut self, value: &str, stops: &[(usize, usize)], at: usize) {
    self.synth("{", at);
    self.verbatim(value, stops, at + 1);
    self.synth("\n}", at);
  }

  fn jsx(&mut self, name: Option<&str>, attributes: &'a [AttributeContent], children: &'a [Node], at: usize, flow: bool) {
    let name = name.unwrap_or("");
    self.synth(&format!("<{name}"), at);
    for attribute in attributes {
      match attribute {
        AttributeContent::Property(p) => {
          self.synth(&format!(" {}", p.name), at);
          match &p.value {
            None => {}
            Some(AttributeValue::Literal(text)) => self.synth(&format!("={{{}}}", json(text)), at),
            Some(AttributeValue::Expression(e)) => {
              self.synth("={", at);
              self.verbatim(&e.value, &e.stops, at);
              self.synth("\n}", at);
            }
          }
        }
        AttributeContent::Expression(e) => {
          self.synth(" {", at);
          self.verbatim(&e.value, &e.stops, at);
          self.synth("\n}", at);
        }
      }
    }
    if children.is_empty() && !name.is_empty() {
      self.synth(" />", at);
      return;
    }
    self.synth(">", at);
    match flow {
      true => {
        self.synth("\n", at);
        self.flow(children);
      }
      false => self.phrasing(children),
    }
    self.synth(&format!("</{name}>"), at);
  }

  fn list(&mut self, list: &'a mdast::List, at: usize) {
    let html = if list.ordered { "ol" } else { "ul" };
    let mut attrs = Vec::new();
    if list.ordered {
      if let Some(n) = list.start.filter(|n| *n != 1) {
        attrs.push(("start", n.to_string()));
      }
    }
    let tasks = list.children.iter().any(|c| matches!(c, Node::ListItem(i) if i.checked.is_some()));
    if tasks {
      attrs.push((self.class(), json("contains-task-list")));
    }
    let loose = list.spread || list.children.iter().any(|c| matches!(c, Node::ListItem(i) if i.spread));
    let tag = self.tag(html);
    self.open(&tag, &attrs, at, false);
    self.synth("\n", at);
    for child in &list.children {
      if let Node::ListItem(item) = child {
        self.list_item(item, loose, start(child));
        self.synth("\n", start(child));
      }
    }
    self.close(&tag, at);
  }

  fn list_item(&mut self, item: &'a mdast::ListItem, loose: bool, at: usize) {
    let mut attrs = Vec::new();
    if item.checked.is_some() {
      attrs.push((self.class(), json("task-list-item")));
    }
    let tag = self.tag("li");
    self.open(&tag, &attrs, at, false);
    let mut checkbox = item.checked;
    for (i, child) in item.children.iter().enumerate() {
      let unwrap = !loose && matches!(child, Node::Paragraph(_));
      if loose {
        self.synth("\n", start(child));
      }
      match child {
        Node::Paragraph(p) if unwrap => {
          if let Some(checked) = checkbox.take() {
            self.checkbox(checked, at);
          }
          self.phrasing(&p.children);
        }
        Node::Paragraph(p) => {
          let ptag = self.tag("p");
          self.open(&ptag, &[], start(child), false);
          if let Some(checked) = checkbox.take() {
            self.checkbox(checked, at);
          }
          self.phrasing(&p.children);
          self.close(&ptag, start(child));
        }
        other => {
          if let Some(checked) = checkbox.take() {
            self.checkbox(checked, at);
          }
          if i > 0 || !loose {
            self.synth("\n", start(other));
          }
          self.node(other, true);
        }
      }
    }
    if let Some(checked) = checkbox.take() {
      self.checkbox(checked, at);
    }
    if loose {
      self.synth("\n", at);
    }
    self.close(&tag, at);
  }

  fn checkbox(&mut self, checked: bool, at: usize) {
    let mut attrs = vec![("type", json("checkbox")), ("disabled", "true".to_owned())];
    if checked {
      attrs.push(("checked", "true".to_owned()));
    }
    self.void("input", &attrs, at);
    self.text(" ", at);
  }

  fn table(&mut self, table: &'a mdast::Table, at: usize) {
    let tag = self.tag("table");
    self.open(&tag, &[], at, false);
    let mut rows = table.children.iter();
    if let Some(head) = rows.next() {
      let thead = self.tag("thead");
      self.open(&thead, &[], start(head), false);
      self.row(head, &table.align, "th");
      self.close(&thead, start(head));
    }
    let body: Vec<&'a Node> = rows.collect();
    if !body.is_empty() {
      let tbody = self.tag("tbody");
      self.open(&tbody, &[], start(body[0]), false);
      for row in body {
        self.row(row, &table.align, "td");
      }
      self.close(&tbody, at);
    }
    self.close(&tag, at);
  }

  fn row(&mut self, row: &'a Node, align: &[mdast::AlignKind], cell: &'static str) {
    let Node::TableRow(r) = row else { return };
    let tr = self.tag("tr");
    self.open(&tr, &[], start(row), false);
    for (i, child) in r.children.iter().enumerate() {
      let Node::TableCell(c) = child else { continue };
      let mut attrs = Vec::new();
      match align.get(i) {
        Some(mdast::AlignKind::Left) => attrs.push(("align", json("left"))),
        Some(mdast::AlignKind::Right) => attrs.push(("align", json("right"))),
        Some(mdast::AlignKind::Center) => attrs.push(("align", json("center"))),
        _ => {}
      }
      self.element(cell, &attrs, &c.children, start(child), false);
    }
    self.close(&tr, start(row));
  }

  fn footnote_reference(&mut self, identifier: &str, at: usize) {
    if !self.footnote_defs.contains_key(identifier) {
      self.text(&format!("[^{identifier}]"), at);
      return;
    }
    let next = self.footnotes.len() + 1;
    let note = self.footnotes.entry(identifier.to_owned()).or_insert(Footnote { number: next, refs: 0 });
    note.refs += 1;
    let (number, refs) = (note.number, note.refs);
    if refs == 1 {
      self.footnote_order.push(identifier.to_owned());
    }
    let id = footnote_id(identifier);
    let ref_id = if refs == 1 { format!("user-content-fnref-{id}") } else { format!("user-content-fnref-{id}-{refs}") };
    let sup = self.tag("sup");
    self.open(&sup, &[], at, false);
    let a = self.tag("a");
    self.open(&a, &[("href", json(&format!("#user-content-fn-{id}"))), ("id", json(&ref_id)), ("data-footnote-ref", json("")), ("aria-describedby", json("footnote-label"))], at, false);
    self.text(&number.to_string(), at);
    self.close(&a, at);
    self.close(&sup, at);
  }

  fn footnote_section(&mut self) {
    if self.footnote_order.is_empty() {
      return;
    }
    let at = self.src.len();
    let section = self.tag("section");
    let class = self.class();
    self.open(&section, &[("data-footnotes", json("")), (class, json("footnotes"))], at, false);
    let h2 = self.tag("h2");
    self.open(&h2, &[(class, json("sr-only")), ("id", json("footnote-label"))], at, false);
    self.text("Footnotes", at);
    self.close(&h2, at);
    let ol = self.tag("ol");
    self.open(&ol, &[], at, false);
    let mut i = 0;
    while i < self.footnote_order.len() {
      let identifier = self.footnote_order[i].clone();
      i += 1;
      let Some(def) = self.footnote_defs.get(identifier.as_str()).copied() else { continue };
      let id = footnote_id(&identifier);
      let refs = self.footnotes.get(&identifier).map(|n| n.refs).unwrap_or(1);
      let li = self.tag("li");
      let def_at = def.position.as_ref().map(|p| p.start.offset).unwrap_or(at);
      self.open(&li, &[("id", json(&format!("user-content-fn-{id}")))], def_at, false);
      self.synth("\n", def_at);
      let last = def.children.len().saturating_sub(1);
      for (n, child) in def.children.iter().enumerate() {
        match child {
          Node::Paragraph(p) if n == last => {
            let ptag = self.tag("p");
            self.open(&ptag, &[], start(child), false);
            self.phrasing(&p.children);
            self.backrefs(&id, refs, def_at);
            self.close(&ptag, start(child));
          }
          other => self.node(other, true),
        }
        self.synth("\n", start(child));
      }
      if !matches!(def.children.last(), Some(Node::Paragraph(_))) {
        self.backrefs(&id, refs, def_at);
      }
      self.close(&li, def_at);
    }
    self.close(&ol, at);
    self.close(&section, at);
    self.synth("\n", at);
  }

  fn backrefs(&mut self, id: &str, refs: usize, at: usize) {
    let class = self.class();
    for n in 1..=refs {
      let target = if n == 1 { format!("#user-content-fnref-{id}") } else { format!("#user-content-fnref-{id}-{n}") };
      let label = if n == 1 { "Back to reference 1".to_owned() } else { format!("Back to reference 1-{n}") };
      self.text(" ", at);
      let a = self.tag("a");
      self.open(&a, &[("href", json(&target)), ("data-footnote-backref", json("")), ("aria-label", json(&label)), (class, json("data-footnote-backref"))], at, false);
      self.text("↩", at);
      if n > 1 {
        let sup = self.tag("sup");
        self.open(&sup, &[], at, false);
        self.text(&n.to_string(), at);
        self.close(&sup, at);
      }
      self.close(&a, at);
    }
  }

  fn code(&mut self, code: &'a mdast::Code, at: usize) {
    let fence = Fence::read(code.meta.as_deref().unwrap_or(""));
    #[cfg(feature = "highlight")]
    if self.options.highlight {
      if let Some(lang) = code.lang.as_deref() {
        if let Some(lines) = highlight::lines(&code.value, lang) {
          self.highlighted(lang, &fence, &lines, at);
        } else {
          self.ungrammared(lang, &fence, &code.value, at);
        }
        return;
      }
    }
    let _ = &fence;
    let pre = self.tag("pre");
    self.open(&pre, &[], at, false);
    let ctag = self.tag("code");
    let attrs: Vec<(&str, String)> = code.lang.as_deref().map(|lang| (self.class(), json(&format!("language-{lang}")))).into_iter().collect();
    self.open(&ctag, &attrs, at, false);
    let mut value = code.value.clone();
    value.push('\n');
    self.text(&value, at);
    self.close(&ctag, at);
    self.close(&pre, at);
  }

  /// A fence whose language no grammar knows, as rehype-pretty-code writes it:
  /// its title and language kept, its text left unhighlighted.
  #[cfg(feature = "highlight")]
  fn ungrammared(&mut self, lang: &str, fence: &Fence, value: &str, at: usize) {
    let class = self.class();
    if let Some(title) = &fence.title {
      self.open("figure", &[("data-rehype-pretty-code-figure", json(""))], at, false);
      self.open("figcaption", &[("data-rehype-pretty-code-title", json("")), ("data-language", json(lang))], at, false);
      self.text(title, at);
      self.close("figcaption", at);
    }
    let pre = self.tag("pre");
    self.open(&pre, &[("data-language", json(lang))], at, false);
    let ctag = self.tag("code");
    self.open(&ctag, &[(class, json(&format!("language-{lang}"))), ("data-language", json(lang))], at, false);
    let mut text = value.to_owned();
    text.push('\n');
    self.text(&text, at);
    self.close(&ctag, at);
    self.close(&pre, at);
    if fence.title.is_some() {
      self.close("figure", at);
    }
  }

  #[cfg(feature = "highlight")]
  fn highlighted(&mut self, lang: &str, fence: &Fence, lines: &[Vec<highlight::Piece>], at: usize) {
    let class = self.class();
    self.open("figure", &[("data-rehype-pretty-code-figure", json(""))], at, false);
    if let Some(title) = &fence.title {
      self.open("figcaption", &[("data-rehype-pretty-code-title", json("")), ("data-language", json(lang))], at, false);
      self.text(title, at);
      self.close("figcaption", at);
    }
    let pre = self.tag("pre");
    self.classes.insert(highlight::CODE_CLASS.to_owned());
    self.open(&pre, &[(class, json(highlight::CODE_CLASS)), ("data-language", json(lang))], at, false);
    let ctag = self.tag("code");
    let mut attrs = vec![("data-language", json(lang))];
    if fence.line_numbers {
      attrs.push(("data-line-numbers", json("")));
    }
    self.open(&ctag, &attrs, at, false);
    for (n, line) in lines.iter().enumerate() {
      if n > 0 {
        self.text("\n", at);
      }
      let mut attrs = vec![("data-line", json(""))];
      if fence.marked.contains(&(n + 1)) {
        attrs.push(("data-highlighted-line", json("")));
      }
      self.open("span", &attrs, at, false);
      self.pieces(line, at);
      self.close("span", at);
    }
    self.close(&ctag, at);
    self.close(&pre, at);
    self.close("figure", at);
  }

  #[cfg(feature = "highlight")]
  fn pieces(&mut self, pieces: &[highlight::Piece], at: usize) {
    for piece in pieces {
      match piece {
        highlight::Piece::Text(text) => self.text(text, at),
        highlight::Piece::Span { classes, children } => {
          for class in classes.split(' ') {
            self.classes.insert(class.to_owned());
          }
          let attr = self.class();
          self.open("span", &[(attr, json(classes))], at, false);
          self.pieces(children, at);
          self.close("span", at);
        }
      }
    }
  }
}

/// `stops` cut to the bytes `from..to` of the value they describe, renumbered from `from`.
fn sub_stops(stops: &[(usize, usize)], from: usize, to: usize) -> Vec<(usize, usize)> {
  let mut out = Vec::new();
  for (i, (relative, absolute)) in stops.iter().enumerate() {
    let end = stops.get(i + 1).map(|s| s.0).unwrap_or(usize::MAX);
    if end <= from || *relative >= to {
      continue;
    }
    let begin = (*relative).max(from);
    out.push((begin - from, absolute + (begin - relative)));
  }
  out
}

fn footnote_id(identifier: &str) -> String {
  identifier.to_lowercase().chars().map(|c| if c.is_whitespace() { '-' } else { c }).collect()
}

/// What a code fence's meta says, as `rehype-pretty-code` reads it:
/// `title="…"`, `{1,3-5}` for marked lines and `showLineNumbers`.
#[derive(Debug, Default, PartialEq, Eq)]
struct Fence {
  title: Option<String>,
  marked: BTreeSet<usize>,
  line_numbers: bool,
}

impl Fence {
  fn read(meta: &str) -> Fence {
    let mut fence = Fence::default();
    let mut rest = meta.trim();
    while !rest.is_empty() {
      if let Some(after) = rest.strip_prefix("title=") {
        let (quote, body) = match after.chars().next() {
          Some(q @ ('"' | '\'')) => (q, &after[1..]),
          _ => (' ', after),
        };
        let end = body.find(quote).unwrap_or(body.len());
        fence.title = Some(body[..end].to_owned());
        rest = body.get(end + 1..).unwrap_or("").trim_start();
        continue;
      }
      if let Some(after) = rest.strip_prefix('{') {
        let end = after.find('}').unwrap_or(after.len());
        for part in after[..end].split(',') {
          let part = part.trim();
          match part.split_once('-') {
            Some((a, b)) => {
              if let (Ok(a), Ok(b)) = (a.trim().parse::<usize>(), b.trim().parse::<usize>()) {
                fence.marked.extend(a..=b);
              }
            }
            None => {
              if let Ok(n) = part.parse::<usize>() {
                fence.marked.insert(n);
              }
            }
          }
        }
        rest = after.get(end + 1..).unwrap_or("").trim_start();
        continue;
      }
      let end = rest.find(char::is_whitespace).unwrap_or(rest.len());
      if &rest[..end] == "showLineNumbers" {
        fence.line_numbers = true;
      }
      rest = rest[end..].trim_start();
    }
    fence
  }
}

#[cfg(test)]
mod tests;
