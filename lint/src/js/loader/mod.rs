//! The `<script>` blocks of `.vue`, `.svelte` and `.astro` files.

mod astro;
mod partial;
mod source;
mod svelte;
mod vue;

pub(crate) use partial::{LINT_PARTIAL_LOADER_EXTENSIONS, PartialLoader};
pub(crate) use source::JavaScriptSource;
