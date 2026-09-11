//! The browser half of FSR, carried by the binary.
//!
//! `@snapfire/fsr-client` is an import map entry rather than a package
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

/// Every module of the client, by the file name its URL ends in.
pub const FILES: &[(&str, &str)] = &[
  ("actions.js", include_str!("../embedded/client/actions.js")),
  ("boot.js", include_str!("../embedded/client/boot.js")),
  ("index.js", include_str!("../embedded/client/index.js")),
  ("live.js", include_str!("../embedded/client/live.js")),
  ("locale.js", include_str!("../embedded/client/locale.js")),
  ("navigator.js", include_str!("../embedded/client/navigator.js")),
  ("react.js", include_str!("../embedded/client/react.js")),
  ("reader.js", include_str!("../embedded/client/reader.js")),
  ("render.js", include_str!("../embedded/client/render.js")),
  ("server.js", include_str!("../embedded/client/server.js")),
  ("socket.js", include_str!("../embedded/client/socket.js")),
  ("std.js", include_str!("../embedded/client/std.js")),
  ("store.js", include_str!("../embedded/client/store.js")),
  ("testing.js", include_str!("../embedded/client/testing.js")),
  ("values.js", include_str!("../embedded/client/values.js")),
];

/// The declarations for every module, written into an application's `types/`
/// by `fsr types` rather than served. They are read by an editor and by
/// `tsc --noEmit`, so they pair with [`FILES`] and an application cannot
/// typecheck against a client the host does not serve.
pub const TYPES: &[(&str, &str)] = &[
  ("actions.d.ts", include_str!("../embedded/client/actions.d.ts")),
  ("boot.d.ts", include_str!("../embedded/client/boot.d.ts")),
  ("index.d.ts", include_str!("../embedded/client/index.d.ts")),
  ("live.d.ts", include_str!("../embedded/client/live.d.ts")),
  ("locale.d.ts", include_str!("../embedded/client/locale.d.ts")),
  ("navigator.d.ts", include_str!("../embedded/client/navigator.d.ts")),
  ("react.d.ts", include_str!("../embedded/client/react.d.ts")),
  ("reader.d.ts", include_str!("../embedded/client/reader.d.ts")),
  ("render.d.ts", include_str!("../embedded/client/render.d.ts")),
  ("server.d.ts", include_str!("../embedded/client/server.d.ts")),
  ("socket.d.ts", include_str!("../embedded/client/socket.d.ts")),
  ("std.d.ts", include_str!("../embedded/client/std.d.ts")),
  ("store.d.ts", include_str!("../embedded/client/store.d.ts")),
  ("testing.d.ts", include_str!("../embedded/client/testing.d.ts")),
  ("values.d.ts", include_str!("../embedded/client/values.d.ts")),
];

/// The module `name` names. `None` for anything the client does not carry.
/// A name with a path separator in it matches nothing, so the prefix is the
/// whole of what this answers.
pub fn get(name: &str) -> Option<&'static str> {
  if name.contains('/') || name.contains('\\') {
    return None;
  }
  FILES.iter().find(|(file, _)| *file == name).map(|(_, body)| *body)
}

/// What the modules come to, for a report that says how much the binary is
/// answering out of itself.
pub fn bytes() -> usize {
  FILES.iter().map(|(_, body)| body.len()).sum()
}

/// Writes the client into `dir`, for a deployment whose web server answers
/// [`ROUTE`] from disk before a request reaches the host. Returns what it
/// wrote.
pub fn write_to(dir: &Path) -> std::io::Result<Vec<PathBuf>> {
  std::fs::create_dir_all(dir)?;
  let mut written = Vec::new();
  for (name, body) in FILES {
    let path = dir.join(name);
    std::fs::write(&path, body)?;
    written.push(path);
  }
  Ok(written)
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn every_module_the_index_re_exports_is_carried() {
    let index = get("index.js").expect("the index");
    for line in index.lines() {
      let Some((_, rest)) = line.split_once("from \"./") else { continue };
      let Some((file, _)) = rest.split_once('"') else { continue };
      assert!(get(file).is_some(), "index.js re-exports {file}, which is not carried");
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
    assert_eq!(FILES.len(), TYPES.len());
  }

  #[test]
  fn a_path_is_not_a_module_name() {
    assert!(get("../../etc/passwd").is_none());
    assert!(get("nested/index.js").is_none());
  }
}
