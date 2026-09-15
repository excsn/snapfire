//! Whose server markup a render matches. A component is written the way the
//! framework that hydrates it would write it, at the major the application
//! vendors. Markup nothing hydrates follows the renderer's own rules.

use std::borrow::Cow;

use snapfire_fsr_core::Value;

use super::react::ReactMajor;
use super::BOOLEAN;
use crate::ast::HydratedBy;

/// The frameworks an application vendors, each at the major whose markup the
/// renderer writes. A framework left `None` is one the application does not
/// vendor, so a component it would hydrate is written as plain markup.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Frameworks {
  pub react: Option<ReactMajor>,
}

/// The rules a render is under at one point in the tree.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) enum Markup {
  /// Nothing hydrates this markup, so there is no framework to agree with.
  #[default]
  Plain,
  React(ReactMajor),
}

impl Markup {
  /// Every set of rules, so a bake keeps only what all of them print alike.
  pub(super) fn every() -> impl Iterator<Item = Markup> {
    std::iter::once(Markup::Plain).chain(ReactMajor::ALL.into_iter().map(Markup::React))
  }

  /// The rules a component renders under. A component a vendored framework
  /// hydrates takes that framework's rules. One nothing hydrates renders
  /// inside its caller's tree, so it keeps its caller's.
  pub(crate) fn of(hydrated_by: Option<HydratedBy>, frameworks: Frameworks, caller: Markup) -> Markup {
    match hydrated_by {
      Some(HydratedBy::React) => frameworks.react.map_or(Markup::Plain, Markup::React),
      None => caller,
    }
  }

  /// Whether `name` is present or absent rather than valued: a truthy value writes `name=""`.
  pub(super) fn is_boolean(self, name: &str) -> bool {
    match self {
      Markup::Plain => BOOLEAN.contains(&name),
      Markup::React(major) => major.is_boolean(name),
    }
  }

  /// Whether `true` on `name` writes nothing.
  pub(super) fn drops_true(self, name: &str) -> bool {
    match self {
      Markup::Plain => false,
      Markup::React(major) => major.drops_true(name),
    }
  }

  /// Whether an empty string on `name` of `tag` writes nothing rather than `name=""`.
  pub(super) fn drops_empty(self, tag: &str, name: &str) -> bool {
    match self {
      Markup::Plain => false,
      Markup::React(major) => major.drops_empty(tag, name),
    }
  }

  /// Whether these rules may move `tag` into the document head.
  pub(super) fn may_hoist(self, tag: &str) -> bool {
    match self {
      Markup::Plain => false,
      Markup::React(major) => major.may_hoist(tag),
    }
  }

  /// Whether `tag` with these attributes is moved into the head. The caller
  /// answers for where it sits: nothing inside `<svg>` or `<noscript>` moves.
  pub(super) fn hoists(self, tag: &str, attrs: &[(Cow<'_, str>, Value)]) -> bool {
    match self {
      Markup::Plain => false,
      Markup::React(major) => major.hoists(tag, attrs),
    }
  }
}
