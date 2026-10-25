//! The browser half of FSR, carried by the binary.
//!
//! `@snapfire/fsr/client` is an import map entry rather than a package
//! install, so the URLs that entry names are answered out of here and an
//! application holds no copy of the client. The files are the same version as
//! the host that serves them, which is what a `[[static]]` root pointed at a
//! checkout cannot promise.

use std::path::{Path, PathBuf};

/// The prefix the import map names. An application that configures a
/// `[[static]]` root on this route is serving its own client and the host
/// leaves the prefix alone.
pub const ROUTE: &str = "/static/js/fsr";

pub const MEDIA_TYPE: &str = "text/javascript; charset=utf-8";

/// Every module of the client, by the file name its URL ends in. The bodies are
/// the readable build with `client_readable` on and the minified one without it,
/// so a binary built without the feature carries one set of modules, not two.
#[cfg(feature = "client_readable")]
pub const FILES: &[(&str, &str)] = &[
  ("actions.js", include_str!("../embedded/client/actions.js")),
  ("auth.js", include_str!("../embedded/client/auth.js")),
  ("boot.js", include_str!("../embedded/client/boot.js")),
  ("elements.js", include_str!("../embedded/client/elements.js")),
  ("embed.js", include_str!("../embedded/client/embed.js")),
  ("events.js", include_str!("../embedded/client/events.js")),
  ("expect.js", include_str!("../embedded/client/expect.js")),
  ("harness.js", include_str!("../embedded/client/harness.js")),
  ("htmx.js", include_str!("../embedded/client/htmx.js")),
  ("index.js", include_str!("../embedded/client/index.js")),
  ("jsx-runtime.js", include_str!("../embedded/client/jsx-runtime.js")),
  ("live.js", include_str!("../embedded/client/live.js")),
  ("link.js", include_str!("../embedded/client/link.js")),
  ("locale.js", include_str!("../embedded/client/locale.js")),
  ("navigator.js", include_str!("../embedded/client/navigator.js")),
  ("picture.js", include_str!("../embedded/client/picture.js")),
  ("places.js", include_str!("../embedded/client/places.js")),
  ("pwa.js", include_str!("../embedded/client/pwa.js")),
  ("queries.js", include_str!("../embedded/client/queries.js")),
  ("react.js", include_str!("../embedded/client/react.js")),
  ("reader.js", include_str!("../embedded/client/reader.js")),
  ("render.js", include_str!("../embedded/client/render.js")),
  ("script.js", include_str!("../embedded/client/script.js")),
  ("server.js", include_str!("../embedded/client/server.js")),
  ("socket.js", include_str!("../embedded/client/socket.js")),
  ("std.js", include_str!("../embedded/client/std.js")),
  ("store.js", include_str!("../embedded/client/store.js")),
  ("svelte.js", include_str!("../embedded/client/svelte.js")),
  ("template.js", include_str!("../embedded/client/template.js")),
  ("testing.js", include_str!("../embedded/client/testing.js")),
  ("values.js", include_str!("../embedded/client/values.js")),
  ("vue.js", include_str!("../embedded/client/vue.js")),
  ("intl.js", include_str!("../embedded/client/intl.js")),
  ("text.js", include_str!("../embedded/client/text.js")),
  ("time.js", include_str!("../embedded/client/time.js")),
  ("crypto.js", include_str!("../embedded/client/crypto.js")),
  ("i18n.js", include_str!("../embedded/client/i18n.js")),
  ("log.js", include_str!("../embedded/client/log.js")),
  ("path.js", include_str!("../embedded/client/path.js")),
  ("standard.js", include_str!("../embedded/client/standard.js")),
  ("native.js", include_str!("../embedded/client/native.js")),
];

#[cfg(not(feature = "client_readable"))]
pub const FILES: &[(&str, &str)] = &[
  ("actions.js", include_str!("../embedded/client/actions.min.js")),
  ("auth.js", include_str!("../embedded/client/auth.min.js")),
  ("boot.js", include_str!("../embedded/client/boot.min.js")),
  ("elements.js", include_str!("../embedded/client/elements.min.js")),
  ("embed.js", include_str!("../embedded/client/embed.min.js")),
  ("events.js", include_str!("../embedded/client/events.min.js")),
  ("expect.js", include_str!("../embedded/client/expect.min.js")),
  ("harness.js", include_str!("../embedded/client/harness.min.js")),
  ("htmx.js", include_str!("../embedded/client/htmx.min.js")),
  ("index.js", include_str!("../embedded/client/index.min.js")),
  ("jsx-runtime.js", include_str!("../embedded/client/jsx-runtime.min.js")),
  ("live.js", include_str!("../embedded/client/live.min.js")),
  ("link.js", include_str!("../embedded/client/link.min.js")),
  ("locale.js", include_str!("../embedded/client/locale.min.js")),
  ("navigator.js", include_str!("../embedded/client/navigator.min.js")),
  ("picture.js", include_str!("../embedded/client/picture.min.js")),
  ("places.js", include_str!("../embedded/client/places.min.js")),
  ("pwa.js", include_str!("../embedded/client/pwa.min.js")),
  ("queries.js", include_str!("../embedded/client/queries.min.js")),
  ("react.js", include_str!("../embedded/client/react.min.js")),
  ("reader.js", include_str!("../embedded/client/reader.min.js")),
  ("render.js", include_str!("../embedded/client/render.min.js")),
  ("script.js", include_str!("../embedded/client/script.min.js")),
  ("server.js", include_str!("../embedded/client/server.min.js")),
  ("socket.js", include_str!("../embedded/client/socket.min.js")),
  ("std.js", include_str!("../embedded/client/std.min.js")),
  ("store.js", include_str!("../embedded/client/store.min.js")),
  ("svelte.js", include_str!("../embedded/client/svelte.min.js")),
  ("template.js", include_str!("../embedded/client/template.min.js")),
  ("testing.js", include_str!("../embedded/client/testing.min.js")),
  ("values.js", include_str!("../embedded/client/values.min.js")),
  ("vue.js", include_str!("../embedded/client/vue.min.js")),
  ("intl.js", include_str!("../embedded/client/intl.min.js")),
  ("text.js", include_str!("../embedded/client/text.min.js")),
  ("time.js", include_str!("../embedded/client/time.min.js")),
  ("crypto.js", include_str!("../embedded/client/crypto.min.js")),
  ("i18n.js", include_str!("../embedded/client/i18n.min.js")),
  ("log.js", include_str!("../embedded/client/log.min.js")),
  ("path.js", include_str!("../embedded/client/path.min.js")),
  ("standard.js", include_str!("../embedded/client/standard.min.js")),
  ("native.js", include_str!("../embedded/client/native.min.js")),
];

/// Whether the readable build is in this binary. `false` makes a configured
/// `document.client = "readable"` serve the minified build instead, which the
/// report's `client` row names.
pub const HAS_READABLE: bool = cfg!(feature = "client_readable");

/// The declarations for every module, written into an application's `types/`
/// by `fsr types` rather than served. They are read by an editor and by
/// `tsc --noEmit`, so they pair with [`FILES`] and an application cannot
/// typecheck against a client the host does not serve.
pub const TYPES: &[(&str, &str)] = &[
  ("actions.d.ts", include_str!("../embedded/client/actions.d.ts")),
  ("auth.d.ts", include_str!("../embedded/client/auth.d.ts")),
  ("boot.d.ts", include_str!("../embedded/client/boot.d.ts")),
  ("elements.d.ts", include_str!("../embedded/client/elements.d.ts")),
  ("embed.d.ts", include_str!("../embedded/client/embed.d.ts")),
  ("events.d.ts", include_str!("../embedded/client/events.d.ts")),
  ("expect.d.ts", include_str!("../embedded/client/expect.d.ts")),
  ("harness.d.ts", include_str!("../embedded/client/harness.d.ts")),
  ("htmx.d.ts", include_str!("../embedded/client/htmx.d.ts")),
  ("index.d.ts", include_str!("../embedded/client/index.d.ts")),
  ("jsx-runtime.d.ts", include_str!("../embedded/client/jsx-runtime.d.ts")),
  ("live.d.ts", include_str!("../embedded/client/live.d.ts")),
  ("link.d.ts", include_str!("../embedded/client/link.d.ts")),
  ("locale.d.ts", include_str!("../embedded/client/locale.d.ts")),
  ("navigator.d.ts", include_str!("../embedded/client/navigator.d.ts")),
  ("picture.d.ts", include_str!("../embedded/client/picture.d.ts")),
  ("places.d.ts", include_str!("../embedded/client/places.d.ts")),
  ("pwa.d.ts", include_str!("../embedded/client/pwa.d.ts")),
  ("queries.d.ts", include_str!("../embedded/client/queries.d.ts")),
  ("react.d.ts", include_str!("../embedded/client/react.d.ts")),
  ("reader.d.ts", include_str!("../embedded/client/reader.d.ts")),
  ("render.d.ts", include_str!("../embedded/client/render.d.ts")),
  ("script.d.ts", include_str!("../embedded/client/script.d.ts")),
  ("server.d.ts", include_str!("../embedded/client/server.d.ts")),
  ("socket.d.ts", include_str!("../embedded/client/socket.d.ts")),
  ("std.d.ts", include_str!("../embedded/client/std.d.ts")),
  ("store.d.ts", include_str!("../embedded/client/store.d.ts")),
  ("svelte.d.ts", include_str!("../embedded/client/svelte.d.ts")),
  ("template.d.ts", include_str!("../embedded/client/template.d.ts")),
  ("testing.d.ts", include_str!("../embedded/client/testing.d.ts")),
  ("values.d.ts", include_str!("../embedded/client/values.d.ts")),
  ("vue.d.ts", include_str!("../embedded/client/vue.d.ts")),
  ("intl.d.ts", include_str!("../embedded/client/intl.d.ts")),
  ("text.d.ts", include_str!("../embedded/client/text.d.ts")),
  ("time.d.ts", include_str!("../embedded/client/time.d.ts")),
  ("crypto.d.ts", include_str!("../embedded/client/crypto.d.ts")),
  ("i18n.d.ts", include_str!("../embedded/client/i18n.d.ts")),
  ("log.d.ts", include_str!("../embedded/client/log.d.ts")),
  ("path.d.ts", include_str!("../embedded/client/path.d.ts")),
  ("standard.d.ts", include_str!("../embedded/client/standard.d.ts")),
  ("native.d.ts", include_str!("../embedded/client/native.d.ts")),
  ("host/crypto.d.ts", include_str!("../embedded/client/host/crypto.d.ts")),
  ("host/file.d.ts", include_str!("../embedded/client/host/file.d.ts")),
  ("host/i18n.d.ts", include_str!("../embedded/client/host/i18n.d.ts")),
  ("host/id.d.ts", include_str!("../embedded/client/host/id.d.ts")),
  ("host/index.d.ts", include_str!("../embedded/client/host/index.d.ts")),
  ("host/intl.d.ts", include_str!("../embedded/client/host/intl.d.ts")),
  ("host/log.d.ts", include_str!("../embedded/client/host/log.d.ts")),
  ("host/path.d.ts", include_str!("../embedded/client/host/path.d.ts")),
  ("host/text.d.ts", include_str!("../embedded/client/host/text.d.ts")),
  ("host/time.d.ts", include_str!("../embedded/client/host/time.d.ts")),
  ("terminal/clipboard.d.ts", include_str!("../embedded/client/terminal/clipboard.d.ts")),
  ("terminal/index.d.ts", include_str!("../embedded/client/terminal/index.d.ts")),
  ("terminal/system.d.ts", include_str!("../embedded/client/terminal/system.d.ts")),
];

/// The same modules minified, keyed by the same plain names. A minified module
/// imports its siblings by their own `.min.js` names, which are answered too.
/// No `.d.ts` has a minified twin.
pub const MINIFIED: &[(&str, &str)] = &[
  ("actions.js", include_str!("../embedded/client/actions.min.js")),
  ("auth.js", include_str!("../embedded/client/auth.min.js")),
  ("boot.js", include_str!("../embedded/client/boot.min.js")),
  ("elements.js", include_str!("../embedded/client/elements.min.js")),
  ("embed.js", include_str!("../embedded/client/embed.min.js")),
  ("events.js", include_str!("../embedded/client/events.min.js")),
  ("expect.js", include_str!("../embedded/client/expect.min.js")),
  ("harness.js", include_str!("../embedded/client/harness.min.js")),
  ("htmx.js", include_str!("../embedded/client/htmx.min.js")),
  ("index.js", include_str!("../embedded/client/index.min.js")),
  ("jsx-runtime.js", include_str!("../embedded/client/jsx-runtime.min.js")),
  ("live.js", include_str!("../embedded/client/live.min.js")),
  ("link.js", include_str!("../embedded/client/link.min.js")),
  ("locale.js", include_str!("../embedded/client/locale.min.js")),
  ("navigator.js", include_str!("../embedded/client/navigator.min.js")),
  ("picture.js", include_str!("../embedded/client/picture.min.js")),
  ("places.js", include_str!("../embedded/client/places.min.js")),
  ("pwa.js", include_str!("../embedded/client/pwa.min.js")),
  ("queries.js", include_str!("../embedded/client/queries.min.js")),
  ("react.js", include_str!("../embedded/client/react.min.js")),
  ("reader.js", include_str!("../embedded/client/reader.min.js")),
  ("render.js", include_str!("../embedded/client/render.min.js")),
  ("script.js", include_str!("../embedded/client/script.min.js")),
  ("server.js", include_str!("../embedded/client/server.min.js")),
  ("socket.js", include_str!("../embedded/client/socket.min.js")),
  ("std.js", include_str!("../embedded/client/std.min.js")),
  ("store.js", include_str!("../embedded/client/store.min.js")),
  ("svelte.js", include_str!("../embedded/client/svelte.min.js")),
  ("template.js", include_str!("../embedded/client/template.min.js")),
  ("testing.js", include_str!("../embedded/client/testing.min.js")),
  ("values.js", include_str!("../embedded/client/values.min.js")),
  ("vue.js", include_str!("../embedded/client/vue.min.js")),
  ("intl.js", include_str!("../embedded/client/intl.min.js")),
  ("text.js", include_str!("../embedded/client/text.min.js")),
  ("time.js", include_str!("../embedded/client/time.min.js")),
  ("crypto.js", include_str!("../embedded/client/crypto.min.js")),
  ("i18n.js", include_str!("../embedded/client/i18n.min.js")),
  ("log.js", include_str!("../embedded/client/log.min.js")),
  ("path.js", include_str!("../embedded/client/path.min.js")),
  ("standard.js", include_str!("../embedded/client/standard.min.js")),
  ("native.js", include_str!("../embedded/client/native.min.js")),
];

/// Each module's source map with its TypeScript inlined, by the name its
/// module's `sourceMappingURL` gives it.
#[cfg(feature = "client_source_maps")]
pub const MAPS: &[(&str, &str)] = &[
  ("actions.js.map", include_str!("../embedded/client/actions.js.map")),
  ("actions.min.js.map", include_str!("../embedded/client/actions.min.js.map")),
  ("auth.js.map", include_str!("../embedded/client/auth.js.map")),
  ("auth.min.js.map", include_str!("../embedded/client/auth.min.js.map")),
  ("boot.js.map", include_str!("../embedded/client/boot.js.map")),
  ("boot.min.js.map", include_str!("../embedded/client/boot.min.js.map")),
  ("elements.js.map", include_str!("../embedded/client/elements.js.map")),
  ("elements.min.js.map", include_str!("../embedded/client/elements.min.js.map")),
  ("embed.js.map", include_str!("../embedded/client/embed.js.map")),
  ("embed.min.js.map", include_str!("../embedded/client/embed.min.js.map")),
  ("events.js.map", include_str!("../embedded/client/events.js.map")),
  ("events.min.js.map", include_str!("../embedded/client/events.min.js.map")),
  ("expect.js.map", include_str!("../embedded/client/expect.js.map")),
  ("expect.min.js.map", include_str!("../embedded/client/expect.min.js.map")),
  ("harness.js.map", include_str!("../embedded/client/harness.js.map")),
  ("harness.min.js.map", include_str!("../embedded/client/harness.min.js.map")),
  ("htmx.js.map", include_str!("../embedded/client/htmx.js.map")),
  ("htmx.min.js.map", include_str!("../embedded/client/htmx.min.js.map")),
  ("index.js.map", include_str!("../embedded/client/index.js.map")),
  ("index.min.js.map", include_str!("../embedded/client/index.min.js.map")),
  ("jsx-runtime.js.map", include_str!("../embedded/client/jsx-runtime.js.map")),
  ("jsx-runtime.min.js.map", include_str!("../embedded/client/jsx-runtime.min.js.map")),
  ("live.js.map", include_str!("../embedded/client/live.js.map")),
  ("live.min.js.map", include_str!("../embedded/client/live.min.js.map")),
  ("link.js.map", include_str!("../embedded/client/link.js.map")),
  ("link.min.js.map", include_str!("../embedded/client/link.min.js.map")),
  ("locale.js.map", include_str!("../embedded/client/locale.js.map")),
  ("locale.min.js.map", include_str!("../embedded/client/locale.min.js.map")),
  ("navigator.js.map", include_str!("../embedded/client/navigator.js.map")),
  ("navigator.min.js.map", include_str!("../embedded/client/navigator.min.js.map")),
  ("picture.js.map", include_str!("../embedded/client/picture.js.map")),
  ("picture.min.js.map", include_str!("../embedded/client/picture.min.js.map")),
  ("places.js.map", include_str!("../embedded/client/places.js.map")),
  ("places.min.js.map", include_str!("../embedded/client/places.min.js.map")),
  ("pwa.js.map", include_str!("../embedded/client/pwa.js.map")),
  ("pwa.min.js.map", include_str!("../embedded/client/pwa.min.js.map")),
  ("queries.js.map", include_str!("../embedded/client/queries.js.map")),
  ("queries.min.js.map", include_str!("../embedded/client/queries.min.js.map")),
  ("react.js.map", include_str!("../embedded/client/react.js.map")),
  ("react.min.js.map", include_str!("../embedded/client/react.min.js.map")),
  ("reader.js.map", include_str!("../embedded/client/reader.js.map")),
  ("reader.min.js.map", include_str!("../embedded/client/reader.min.js.map")),
  ("render.js.map", include_str!("../embedded/client/render.js.map")),
  ("render.min.js.map", include_str!("../embedded/client/render.min.js.map")),
  ("script.js.map", include_str!("../embedded/client/script.js.map")),
  ("script.min.js.map", include_str!("../embedded/client/script.min.js.map")),
  ("server.js.map", include_str!("../embedded/client/server.js.map")),
  ("server.min.js.map", include_str!("../embedded/client/server.min.js.map")),
  ("socket.js.map", include_str!("../embedded/client/socket.js.map")),
  ("socket.min.js.map", include_str!("../embedded/client/socket.min.js.map")),
  ("std.js.map", include_str!("../embedded/client/std.js.map")),
  ("std.min.js.map", include_str!("../embedded/client/std.min.js.map")),
  ("store.js.map", include_str!("../embedded/client/store.js.map")),
  ("store.min.js.map", include_str!("../embedded/client/store.min.js.map")),
  ("svelte.js.map", include_str!("../embedded/client/svelte.js.map")),
  ("svelte.min.js.map", include_str!("../embedded/client/svelte.min.js.map")),
  ("template.js.map", include_str!("../embedded/client/template.js.map")),
  ("template.min.js.map", include_str!("../embedded/client/template.min.js.map")),
  ("testing.js.map", include_str!("../embedded/client/testing.js.map")),
  ("testing.min.js.map", include_str!("../embedded/client/testing.min.js.map")),
  ("values.js.map", include_str!("../embedded/client/values.js.map")),
  ("values.min.js.map", include_str!("../embedded/client/values.min.js.map")),
  ("vue.js.map", include_str!("../embedded/client/vue.js.map")),
  ("vue.min.js.map", include_str!("../embedded/client/vue.min.js.map")),
  ("intl.js.map", include_str!("../embedded/client/intl.js.map")),
  ("intl.min.js.map", include_str!("../embedded/client/intl.min.js.map")),
  ("text.js.map", include_str!("../embedded/client/text.js.map")),
  ("text.min.js.map", include_str!("../embedded/client/text.min.js.map")),
  ("time.js.map", include_str!("../embedded/client/time.js.map")),
  ("time.min.js.map", include_str!("../embedded/client/time.min.js.map")),
  ("crypto.js.map", include_str!("../embedded/client/crypto.js.map")),
  ("crypto.min.js.map", include_str!("../embedded/client/crypto.min.js.map")),
  ("i18n.js.map", include_str!("../embedded/client/i18n.js.map")),
  ("i18n.min.js.map", include_str!("../embedded/client/i18n.min.js.map")),
  ("log.js.map", include_str!("../embedded/client/log.js.map")),
  ("log.min.js.map", include_str!("../embedded/client/log.min.js.map")),
  ("path.js.map", include_str!("../embedded/client/path.js.map")),
  ("path.min.js.map", include_str!("../embedded/client/path.min.js.map")),
  ("standard.js.map", include_str!("../embedded/client/standard.js.map")),
  ("standard.min.js.map", include_str!("../embedded/client/standard.min.js.map")),
  ("native.js.map", include_str!("../embedded/client/native.js.map")),
  ("native.min.js.map", include_str!("../embedded/client/native.min.js.map")),
];

/// The source map `name` names, carried under `client_source_maps`.
pub fn map(name: &str) -> Option<&'static str> {
  #[cfg(feature = "client_source_maps")]
  return MAPS.iter().find(|(file, _)| *file == name).map(|(_, body)| *body);
  #[cfg(not(feature = "client_source_maps"))]
  {
    let _ = name;
    None
  }
}

/// The module `name` names, from the readable build or the minified one.
/// `None` for anything the client does not carry. A name with a path separator
/// in it matches nothing, so the prefix is the whole of what this answers.
///
/// A `<name>.min.js` request is always the minified module whatever `minified`
/// says, since only a minified module asks for one and it would be answering
/// its own sibling.
pub fn get(name: &str, minified: bool) -> Option<&'static str> {
  if name.contains('/') || name.contains('\\') {
    return None;
  }
  if let Some(stem) = name.strip_suffix(".min.js") {
    let plain = format!("{stem}.js");
    return MINIFIED.iter().find(|(file, _)| *file == plain).map(|(_, body)| unmapped(body));
  }
  if minified && let Some(body) = MINIFIED.iter().find(|(file, _)| *file == name).map(|(_, body)| *body) {
    return Some(unmapped(body));
  }
  FILES.iter().find(|(file, _)| *file == name).map(|(_, body)| unmapped(body))
}

/// `body` without the trailing `sourceMappingURL` comment the build wrote
/// when the binary carries no maps; as written under `client_source_maps`.
fn unmapped(body: &'static str) -> &'static str {
  if cfg!(feature = "client_source_maps") {
    return body;
  }
  let trimmed = body.trim_end();
  match trimmed.rfind("//# sourceMappingURL=") {
    Some(at) if !trimmed[at..].contains('\n') => &trimmed[..at],
    _ => body,
  }
}

/// What the modules come to, for a report that says how much the binary is
/// answering out of itself: the build being served, plus the declarations,
/// which have one form.
pub fn bytes(minified: bool) -> usize {
  FILES
    .iter()
    .map(|(name, body)| match minified {
      true => MINIFIED.iter().find(|(f, _)| f == name).map_or(body.len(), |(_, m)| m.len()),
      false => body.len(),
    })
    .sum()
}

/// Writes the client into `dir`, for a deployment whose web server answers
/// [`ROUTE`] from disk before a request reaches the host. Returns what it
/// wrote.
pub fn write_to(dir: &Path, minified: bool) -> std::io::Result<Vec<PathBuf>> {
  std::fs::create_dir_all(dir)?;
  let mut written = Vec::new();
  for (name, _) in FILES {
    let path = dir.join(name);
    std::fs::write(&path, get(name, minified).unwrap_or_default())?;
    written.push(path);
  }
  // The minified graph names its siblings, so a tree that answers the prefix
  // from disk needs those spellings too.
  if minified {
    for (name, body) in MINIFIED {
      let path = dir.join(name.replace(".js", ".min.js"));
      std::fs::write(&path, unmapped(body))?;
      written.push(path);
    }
  }
  #[cfg(feature = "client_source_maps")]
  for (name, body) in MAPS {
    let path = dir.join(name);
    std::fs::write(&path, body)?;
    written.push(path);
  }
  let desktop_names: &[&str] = if minified { &[DESKTOP_FILE, "desktop.min.js"] } else { &[DESKTOP_FILE] };
  #[cfg(feature = "client_source_maps")]
  let desktop_names: Vec<&str> = desktop_names.iter().copied().chain(DESKTOP_MAPS.iter().map(|(name, _)| *name)).collect();
  for name in desktop_names.iter().copied() {
    let path = dir.join(name);
    std::fs::write(&path, desktop(name, minified).unwrap_or_default())?;
    written.push(path);
  }
  Ok(written)
}

/// The specifiers `name` imports: `./sibling.js` for another embedded module,
/// a bare specifier for anything the page's import map resolves. Read out of
/// the module text, which is the only place the client's own graph is written.
pub fn imports(name: &str, minified: bool) -> Vec<&'static str> {
  let Some(body) = get(name, minified) else { return Vec::new() };
  let mut found = Vec::new();
  for keyword in ["from", "import"] {
    let mut rest = body;
    while let Some(at) = rest.find(keyword) {
      rest = &rest[at + keyword.len()..];
      let quoted = rest.trim_start();
      if !quoted.starts_with('"') {
        continue;
      }
      let quoted = &quoted[1..];
      let Some(end) = quoted.find('"') else { break };
      let specifier = &quoted[..end];
      if !specifier.is_empty() && !found.contains(&specifier) {
        found.push(specifier);
      }
    }
  }
  found
}

/// The import map entries for every module [`ROUTE`] answers: `@snapfire/fsr/client` for `index.js`, `@snapfire/fsr/client/<name>` for `<name>.js`, `@snapfire/fsr` for `native.js`, `@snapfire/fsr/template` for `template.js` and [`DESKTOP_SPECIFIER`] for [`DESKTOP_FILE`].
pub fn import_entries() -> Vec<(String, String)> {
  let mut entries = vec![("@snapfire/fsr/client".to_owned(), format!("{ROUTE}/index.js")), ("@snapfire/fsr".to_owned(), format!("{ROUTE}/native.js")), ("@snapfire/fsr/template".to_owned(), format!("{ROUTE}/template.js"))];
  for (name, _) in FILES {
    if let Some(stem) = name.strip_suffix(".js") {
      entries.push((format!("@snapfire/fsr/client/{stem}"), format!("{ROUTE}/{name}")));
    }
  }
  entries.push(desktop_entry());
  entries
}

/// What a page in an `fsr --desktop` window imports to reach the window's process. Kept out of [`FILES`], whose modules are named under `@snapfire/fsr/client`; outside a window its calls reject.
pub const DESKTOP_SPECIFIER: &str = "@snapfire/fsr/desktop";

/// The file [`DESKTOP_SPECIFIER`] is answered with under [`ROUTE`].
pub const DESKTOP_FILE: &str = "desktop.js";

#[cfg(feature = "client_readable")]
const DESKTOP: &str = include_str!("../embedded/client/desktop.js");
#[cfg(not(feature = "client_readable"))]
const DESKTOP: &str = include_str!("../embedded/client/desktop.min.js");
const DESKTOP_MINIFIED: &str = include_str!("../embedded/client/desktop.min.js");

/// The declarations of [`DESKTOP_SPECIFIER`], written into an application's `types/` beside the client's.
pub const DESKTOP_TYPES: &str = include_str!("../embedded/client/desktop.d.ts");

#[cfg(feature = "client_source_maps")]
const DESKTOP_MAPS: &[(&str, &str)] = &[
  ("desktop.js.map", include_str!("../embedded/client/desktop.js.map")),
  ("desktop.min.js.map", include_str!("../embedded/client/desktop.min.js.map")),
];

/// The desktop module or its source map, as [`get`] and [`map`] answer the client's.
pub fn desktop(name: &str, minified: bool) -> Option<&'static str> {
  match name {
    "desktop.min.js" => Some(unmapped(DESKTOP_MINIFIED)),
    DESKTOP_FILE if minified => Some(unmapped(DESKTOP_MINIFIED)),
    DESKTOP_FILE => Some(unmapped(DESKTOP)),
    #[cfg(feature = "client_source_maps")]
    _ => DESKTOP_MAPS.iter().find(|(file, _)| *file == name).map(|(_, body)| *body),
    #[cfg(not(feature = "client_source_maps"))]
    _ => None,
  }
}

/// The import map entry for [`DESKTOP_SPECIFIER`].
pub fn desktop_entry() -> (String, String) {
  (DESKTOP_SPECIFIER.to_owned(), format!("{ROUTE}/{DESKTOP_FILE}"))
}

/// `text`, an import map, with [`import_entries`] under `imports` wherever it has no entry of its own. Text that is not a JSON object is returned as it is.
pub fn with_import_entries(text: &str) -> String {
  with_entries(text, |_| true)
}

/// [`with_import_entries`] for the entries whose specifier is in `imported`, the bare specifiers a bundle carries.
pub fn with_imported_entries(text: &str, imported: &[String]) -> String {
  with_entries(text, |specifier| imported.iter().any(|named| named == specifier))
}

fn with_entries(text: &str, wanted: impl Fn(&str) -> bool) -> String {
  let Ok(serde_json::Value::Object(mut map)) = serde_json::from_str::<serde_json::Value>(text) else { return text.to_owned() };
  let imports = map.entry("imports").or_insert_with(|| serde_json::Value::Object(Default::default()));
  let Some(imports) = imports.as_object_mut() else { return text.to_owned() };
  for (specifier, url) in import_entries().into_iter().filter(|(specifier, _)| wanted(specifier)) {
    imports.entry(specifier).or_insert(serde_json::Value::String(url));
  }
  serde_json::to_string(&serde_json::Value::Object(map)).unwrap_or_else(|_| text.to_owned())
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn an_import_map_gains_every_client_module_and_keeps_its_own_entries() {
    let served: serde_json::Value = serde_json::from_str(&with_import_entries(r#"{"imports":{"react":"/v/react.js","@snapfire/fsr/client/react":"/mine/react.js"}}"#)).unwrap();
    let imports = &served["imports"];
    assert_eq!(imports["@snapfire/fsr/client/intl"], "/static/js/fsr/intl.js");
    assert_eq!(imports["@snapfire/fsr/client"], "/static/js/fsr/index.js");
    assert_eq!(imports["@snapfire/fsr"], "/static/js/fsr/native.js");
    assert_eq!(imports["@snapfire/fsr/client/react"], "/mine/react.js");
    assert_eq!(imports["react"], "/v/react.js");
  }

  #[cfg(feature = "client_source_maps")]
  #[test]
  fn with_source_maps_every_module_names_a_map_the_host_carries_holding_its_typescript() {
    for (name, _) in MINIFIED {
      for spelling in [name.to_string(), name.replace(".js", ".min.js")] {
        let body = get(&spelling, false).expect("a module");
        let named = body.trim_end().rsplit_once("//# sourceMappingURL=").map(|(_, named)| named.trim()).unwrap_or_else(|| panic!("{spelling} names no map"));
        let map: serde_json::Value = serde_json::from_str(super::map(named).unwrap_or_else(|| panic!("{spelling} names {named}, which is not carried"))).unwrap();
        let sources = map["sourcesContent"].as_array().unwrap_or_else(|| panic!("{named} has no sourcesContent"));
        assert!(sources.iter().all(|source| source.as_str().is_some_and(|text| !text.is_empty())), "{named} inlines no TypeScript");
      }
    }
    let dir = std::env::temp_dir().join(format!("fsr-client-with-maps-{}", std::process::id()));
    let written = write_to(&dir, true).unwrap();
    assert!(written.iter().any(|path| path.extension().is_some_and(|e| e == "map")), "a tree written for a web server carries the maps");
  }

  #[cfg(not(feature = "client_source_maps"))]
  #[test]
  fn no_module_the_host_answers_or_writes_names_a_source_map_it_does_not_carry() {
    for (name, _) in FILES {
      let min = name.replace(".js", ".min.js");
      for (spelling, minified) in [(*name, false), (*name, true), (min.as_str(), false)] {
        if let Some(body) = get(spelling, minified) {
          assert!(!body.contains("sourceMappingURL"), "{spelling} (minified {minified}) names a map");
        }
      }
    }
    let dir = std::env::temp_dir().join(format!("fsr-client-maps-{}", std::process::id()));
    for minified in [false, true] {
      for path in write_to(&dir, minified).unwrap() {
        assert!(!std::fs::read_to_string(&path).unwrap().contains("sourceMappingURL"), "{} names a map", path.display());
      }
    }
  }

  #[test]
  fn a_tree_written_for_a_web_server_carries_the_desktop_module() {
    let dir = std::env::temp_dir().join(format!("fsr-client-desktop-{}", std::process::id()));
    for minified in [false, true] {
      let written = write_to(&dir, minified).unwrap();
      assert!(written.contains(&dir.join(DESKTOP_FILE)), "minified {minified}: {written:?}");
      assert!(std::fs::read_to_string(dir.join(DESKTOP_FILE)).unwrap().contains("openFile"));
      assert_eq!(minified, written.contains(&dir.join("desktop.min.js")));
    }
    std::fs::remove_dir_all(&dir).unwrap();
  }

  #[test]
  fn every_module_the_index_re_exports_is_carried() {
    let index = get("index.js", false).expect("the index");
    for line in index.lines() {
      let Some((_, rest)) = line.split_once("from \"./") else { continue };
      let Some((file, _)) = rest.split_once('"') else { continue };
      assert!(get(file, false).is_some(), "index.js re-exports {file}, which is not carried");
    }
  }

  #[test]
  fn every_module_is_declared() {
    for (file, _) in FILES {
      let declared = file.replace(".js", ".d.ts");
      assert!(
        TYPES.iter().any(|(name, _)| *name == declared),
        "{file} is served with no {declared} beside it"
      );
    }
    let (served, declared_only): (Vec<_>, Vec<_>) = TYPES.iter().partition(|(name, _)| !name.contains('/'));
    assert_eq!(FILES.len(), served.len());
    assert!(declared_only.iter().all(|(name, _)| name.starts_with("host/") || name.starts_with("terminal/")), "only the host and terminal sides are declared without being served");
  }

  #[test]
  #[cfg(feature = "client_readable")]
  fn imports_reads_siblings_and_bare_specifiers() {
    let found = imports("react.js", false);
    assert!(found.contains(&"./boot.js"), "{found:?}");
    assert!(found.contains(&"react"), "{found:?}");
    assert!(found.contains(&"react-dom/client"), "{found:?}");
  }

  #[test]
  fn the_minified_graph_names_its_own_siblings() {
    let found = imports("react.js", true);
    assert!(found.contains(&"./boot.min.js"), "{found:?}");
    assert!(found.contains(&"react"), "{found:?}");
  }

  #[test]
  #[cfg(feature = "client_readable")]
  fn a_min_name_is_the_minified_module_either_way() {
    let plain = get("boot.js", false).expect("boot.js");
    let min = get("boot.js", true).expect("minified boot.js");
    assert!(min.len() < plain.len(), "minified is not smaller");
    assert_eq!(get("boot.min.js", false), Some(min));
    assert_eq!(get("boot.min.js", true), Some(min));
  }

  #[test]
  fn without_the_readable_build_every_name_is_minified() {
    if HAS_READABLE {
      return;
    }
    assert_eq!(get("boot.js", false), get("boot.min.js", false));
    assert!(imports("react.js", false).contains(&"./boot.min.js"));
  }

  #[test]
  fn every_module_has_a_minified_twin() {
    for (name, _) in FILES {
      if !name.ends_with(".js") {
        continue;
      }
      assert!(MINIFIED.iter().any(|(m, _)| m == name), "{name} has no minified twin");
    }
    assert_eq!(MINIFIED.len(), FILES.iter().filter(|(n, _)| n.ends_with(".js")).count());
  }

  #[test]
  fn imports_skips_a_dynamic_import() {
    assert!(imports("navigator.js", false).iter().all(|s| !s.is_empty()));
    assert!(imports("nope.js", false).is_empty());
  }

  #[test]
  fn a_path_is_not_a_module_name() {
    assert!(get("../../etc/passwd", false).is_none());
    assert!(get("nested/index.js", true).is_none());
  }
}
