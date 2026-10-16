/// CSS declarations in source order, each a property and its value as
/// written: `("font-size", "72px")`.
pub type Declarations = Vec<(String, String)>;

/// One image to render: its size in pixels and the tree drawn into it.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub struct Card {
  pub width: u32,
  pub height: u32,
  pub root: Element,
}

impl Card {
  pub fn new(width: u32, height: u32, root: Element) -> Card {
    Card { width, height, root }
  }
}

/// A node of the tree. `tag` is only used to name the element in a warning
/// or an error.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub enum Element {
  /// A box laid out with flexbox, holding other elements.
  Box { tag: String, style: Declarations, children: Vec<Element> },
  /// A box holding one paragraph, made of runs that each add their own
  /// declarations to the box's.
  Text { tag: String, style: Declarations, runs: Vec<Run> },
  /// A replaced element. `width` and `height` are the attributes, in pixels.
  Image { tag: String, style: Declarations, src: ImageRef, width: Option<f32>, height: Option<f32> },
}

/// A stretch of text inside a [`Element::Text`], such as a `span`.
#[derive(Debug, Clone, PartialEq)]
pub struct Run {
  pub style: Declarations,
  pub text: String,
}

/// Where an image's pixels come from: a key given to [`crate::Images`], or
/// encoded PNG or JPEG bytes.
#[derive(Debug, Clone, PartialEq)]
pub enum ImageRef {
  Key(String),
  Bytes(Vec<u8>),
}

impl Element {
  pub fn boxed(tag: impl Into<String>) -> Element {
    Element::Box { tag: tag.into(), style: Vec::new(), children: Vec::new() }
  }

  pub fn text(tag: impl Into<String>, text: impl Into<String>) -> Element {
    Element::Text { tag: tag.into(), style: Vec::new(), runs: vec![Run { style: Vec::new(), text: text.into() }] }
  }

  pub fn image(tag: impl Into<String>, src: ImageRef) -> Element {
    Element::Image { tag: tag.into(), style: Vec::new(), src, width: None, height: None }
  }

  /// Appends a declaration.
  pub fn with(mut self, property: impl Into<String>, value: impl Into<String>) -> Element {
    self.style_mut().push((property.into(), value.into()));
    self
  }

  /// Appends a child to a box. On a text or an image element it does nothing.
  pub fn child(mut self, child: Element) -> Element {
    if let Element::Box { children, .. } = &mut self {
      children.push(child);
    }
    self
  }

  /// Appends a run to a text element. On a box or an image it does nothing.
  pub fn run(mut self, run: Run) -> Element {
    if let Element::Text { runs, .. } = &mut self {
      runs.push(run);
    }
    self
  }

  /// Sets an image's `width` and `height` attributes. On a box or a text
  /// element it does nothing.
  pub fn size(mut self, w: f32, h: f32) -> Element {
    if let Element::Image { width, height, .. } = &mut self {
      *width = Some(w);
      *height = Some(h);
    }
    self
  }

  pub fn tag(&self) -> &str {
    match self {
      Element::Box { tag, .. } | Element::Text { tag, .. } | Element::Image { tag, .. } => tag,
    }
  }

  pub fn style(&self) -> &Declarations {
    match self {
      Element::Box { style, .. } | Element::Text { style, .. } | Element::Image { style, .. } => style,
    }
  }

  fn style_mut(&mut self) -> &mut Declarations {
    match self {
      Element::Box { style, .. } | Element::Text { style, .. } | Element::Image { style, .. } => style,
    }
  }
}

impl Run {
  pub fn new(text: impl Into<String>) -> Run {
    Run { style: Vec::new(), text: text.into() }
  }

  pub fn with(mut self, property: impl Into<String>, value: impl Into<String>) -> Run {
    self.style.push((property.into(), value.into()));
    self
  }
}
