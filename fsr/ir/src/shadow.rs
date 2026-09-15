//! The shadow root an element template declares by making its root a
//! `<template shadowrootmode>`.

use crate::ast::{Entry, Expr, Lit, ShadowMode, ShadowRoot, Tmpl};
use crate::render::{KEY_ATTR, RAW_ATTR};

const MODE: &str = "shadowrootmode";
const DELEGATES_FOCUS: &str = "shadowrootdelegatesfocus";
const CLONABLE: &str = "shadowrootclonable";
const SERIALIZABLE: &str = "shadowrootserializable";

/// Why an element template's root `<template>` cannot be read as its shadow
/// root.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ShadowRootError {
  #[error("its root `<template>` has no `shadowrootmode`, so the shadow root would hold an inert template; write `shadowrootmode=\"open\"` or `shadowrootmode=\"closed\"`")]
  NoMode,
  #[error("`{name}` on its root `<template>` is not {expected} written out; the element's class needs it before any script runs")]
  NotLiteral { name: String, expected: &'static str },
  #[error("`shadowrootmode=\"{mode}\"` on its root `<template>` is neither `open` nor `closed`")]
  Mode { mode: String },
  #[error("`{name}` on its root `<template>` would be lost, since the parser turns that template into the shadow root and keeps only `shadowrootmode`, `shadowrootdelegatesfocus`, `shadowrootclonable` and `shadowrootserializable`")]
  Attribute { name: String },
  #[error("a spread on its root `<template>` cannot be read by the build; write the shadow root's attributes out")]
  Spread,
  #[error("its `<template shadowrootmode>` shares the root with other nodes or sits in a branch; a shadow root's template must be the template's only root")]
  NotAlone,
}

impl ShadowRoot {
  /// Reads the root `<template>` of an element template as its shadow root
  /// and leaves that template's children as the render. `None` when the root
  /// is anything else, a single child of a root fragment counting as the root.
  pub fn take(render: &mut Tmpl) -> Result<Option<ShadowRoot>, ShadowRootError> {
    let root = match &mut *render {
      Tmpl::Fragment(children) if children.len() == 1 => &mut children[0],
      other => other,
    };
    let Tmpl::Element { tag, attrs, children } = root else {
      return if declared_at_root(root) { Err(ShadowRootError::NotAlone) } else { Ok(None) };
    };
    if tag != "template" {
      return Ok(None);
    }
    let shadow = read(attrs)?;
    let children = std::mem::take(children);
    *render = Tmpl::Fragment(children);
    Ok(Some(shadow))
  }
}

fn declared_at_root(tmpl: &Tmpl) -> bool {
  match tmpl {
    Tmpl::Element { tag, attrs, .. } => tag == "template" && attrs.iter().any(|a| matches!(a, Entry::Field(name, _) if name == MODE)),
    Tmpl::Fragment(children) => children.iter().any(declared_at_root),
    Tmpl::If { then, r#else, .. } => declared_at_root(then) || r#else.as_deref().is_some_and(declared_at_root),
    _ => false,
  }
}

fn read(attrs: &[Entry]) -> Result<ShadowRoot, ShadowRootError> {
  let mut mode = None;
  let mut shadow = ShadowRoot::default();
  for attr in attrs {
    let Entry::Field(name, value) = attr else {
      return Err(ShadowRootError::Spread);
    };
    match name.as_str() {
      MODE => match value {
        Expr::Lit(Lit::Str(given)) => mode = Some(ShadowMode::of(given).ok_or_else(|| ShadowRootError::Mode { mode: given.clone() })?),
        _ => return Err(ShadowRootError::NotLiteral { name: name.clone(), expected: "the string `open` or `closed`" }),
      },
      DELEGATES_FOCUS | CLONABLE | SERIALIZABLE => {
        let Expr::Lit(Lit::Bool(on)) = value else {
          return Err(ShadowRootError::NotLiteral { name: name.clone(), expected: "`true` or `false`" });
        };
        match name.as_str() {
          DELEGATES_FOCUS => shadow.delegates_focus = *on,
          CLONABLE => shadow.clonable = *on,
          _ => shadow.serializable = *on,
        }
      }
      RAW_ATTR => return Err(ShadowRootError::Attribute { name: "dangerouslySetInnerHTML".to_owned() }),
      KEY_ATTR => return Err(ShadowRootError::Attribute { name: "key".to_owned() }),
      other => return Err(ShadowRootError::Attribute { name: other.to_owned() }),
    }
  }
  shadow.mode = mode.ok_or(ShadowRootError::NoMode)?;
  Ok(shadow)
}
