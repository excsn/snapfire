//! The standard library's Rust half: `intl`, `text`, `time`, `crypto`, `id`,
//! `log`, `terminal` with `big_text`, `path`, `file`, `system` and `clipboard`, one function per
//! row of `ext::STANDARD`. The browser half is
//! the client library's `std` module and the two agree byte for byte on every
//! `render` member; `fsr/ir/tests/conformance.rs` diffs them. A `terminal`
//! member has no browser half.

pub mod big_text;
pub mod clipboard;
pub mod crypto;
pub mod frontmatter;
pub mod i18n;
pub mod id;
pub mod intl;
pub mod log;
pub mod path;
pub mod system;
pub mod terminal;
pub mod text;
pub mod time;

use crate::ext::Extensions;

pub fn register(extensions: &mut Extensions) {
  intl::register(extensions);
  text::register(extensions);
  frontmatter::register(extensions);
  time::register(extensions);
  crypto::register(extensions);
  id::register(extensions);
  i18n::register(extensions);
  terminal::register(extensions);
  big_text::register(extensions);
  path::register(extensions);
  system::register(extensions);
  clipboard::register(extensions);
  log::register(extensions);
}
