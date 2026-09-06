//! The standard library's Rust half: `intl`, `text`, `time`, `crypto` and
//! `id`, one function per row of `ext::STANDARD`. The browser half is the
//! client library's `std` module, and the two agree byte for byte on every
//! `render` member; `fsr/ir/tests/conformance.rs` diffs them.

pub mod crypto;
pub mod i18n;
pub mod id;
pub mod intl;
pub mod text;
pub mod time;

use crate::ext::Extensions;

pub fn register(extensions: &mut Extensions) {
  intl::register(extensions);
  text::register(extensions);
  time::register(extensions);
  crypto::register(extensions);
  id::register(extensions);
  i18n::register(extensions);
}
