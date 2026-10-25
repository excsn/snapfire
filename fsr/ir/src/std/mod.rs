//! The standard library's Rust half: `intl`, `text`, `time`, `crypto`, `encoding`, `id`,
//! `log`, `terminal` with `big_text`, `path`, `file`, `system` and `clipboard`, one function per
//! row of `ext::STANDARD`. A `render` member's browser half is the platform's
//! own form or the client library's `std` module, and the two agree byte for
//! byte: `ir/tests/intl_browser.rs` and `ir/tests/platform_browser.rs` check the
//! Rust half against Chrome's output, `engine/tests/std_agreement.rs` against
//! `std.js` in QuickJS. A `terminal` member has no browser half.

pub mod big_text;
pub mod clipboard;
pub mod crypto;
pub mod encoding;
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
  encoding::register(extensions);
  id::register(extensions);
  i18n::register(extensions);
  terminal::register(extensions);
  big_text::register(extensions);
  path::register(extensions);
  system::register(extensions);
  clipboard::register(extensions);
  log::register(extensions);
}
