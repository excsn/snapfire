//! Which React the renderer writes for. The browser's React hydrates over
//! this markup, so where two majors print the same props differently the
//! renderer writes what the application's vendored React would. Everything
//! not dispatched here prints the same under 18 and 19.

use std::borrow::Cow;

use snapfire_fsr_core::Value;

use super::{react18, react19};
use crate::interp::Fail;

/// A React major the renderer writes markup for.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub enum ReactMajor {
  #[default]
  V18,
  V19,
}

impl ReactMajor {
  /// Every major the renderer implements, oldest first.
  pub const ALL: [ReactMajor; 2] = [Self::V18, Self::V19];

  /// The major of a version such as `18.3.1`; `None` for one the renderer has no rules for.
  pub fn of(version: &str) -> Option<Self> {
    match version.split('.').next()? {
      "18" => Some(Self::V18),
      "19" => Some(Self::V19),
      _ => None,
    }
  }

  /// The major as React numbers it.
  pub fn number(self) -> u32 {
    match self {
      Self::V18 => 18,
      Self::V19 => 19,
    }
  }

  /// Writes an attribute of a custom element, which React passes through
  /// rather than checking against the attributes it knows.
  pub(super) fn custom_attribute(self, name: &str, value: &Value, out: &mut String) -> Result<(), Fail> {
    match self {
      Self::V18 => react18::custom_attribute(name, value, out),
      Self::V19 => react19::custom_attribute(name, value, out),
    }
  }

  /// Whether `name` is present or absent rather than valued: a truthy value writes `name=""`.
  pub(super) fn is_boolean(self, name: &str) -> bool {
    match self {
      Self::V18 => react18::is_boolean(name),
      Self::V19 => react19::is_boolean(name),
    }
  }

  /// Whether `true` on `name` writes nothing, which is what React does with a
  /// boolean on an attribute it does not know.
  pub(super) fn drops_true(self, name: &str) -> bool {
    match self {
      Self::V18 => react18::drops_true(name),
      Self::V19 => false,
    }
  }

  /// Whether an empty string on `name` of `tag` writes nothing rather than `name=""`.
  pub(super) fn drops_empty(self, tag: &str, name: &str) -> bool {
    match self {
      Self::V18 => false,
      Self::V19 => react19::drops_empty(tag, name),
    }
  }

  /// Whether this major may move `tag` into the document head. Such a tag is
  /// never baked, so a render sees its attributes and where it sits.
  pub(super) fn may_hoist(self, tag: &str) -> bool {
    match self {
      Self::V18 => false,
      Self::V19 => react19::may_hoist(tag),
    }
  }

  /// Whether `tag` with these attributes is moved into the head. The caller
  /// answers for where it sits: nothing inside `<svg>` or `<noscript>` moves.
  pub(super) fn hoists(self, tag: &str, attrs: &[(Cow<'_, str>, Value)]) -> bool {
    match self {
      Self::V18 => false,
      Self::V19 => react19::hoists(tag, attrs),
    }
  }
}

/// Whether `tag` names a custom element: it has a hyphen and is not one of
/// the SVG and MathML elements whose names have one.
pub(super) fn is_custom_element(tag: &str) -> bool {
  tag.contains('-') && !matches!(tag, "annotation-xml" | "color-profile" | "font-face" | "font-face-src" | "font-face-uri" | "font-face-format" | "font-face-name" | "missing-glyph")
}
