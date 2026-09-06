//! `fsr new`: writes the smallest application the stock host can serve, in the
//! shape [DX.md] section 1 describes. No Cargo project and no `package.json`;
//! `fsr dev` on the app directory is what runs it.

use std::path::{Path, PathBuf};

use crate::vendor::{self, Spec};
use crate::{types, BuildError};

/// The React runtime a scaffolded app vendors, pinned the way the examples pin it.
pub const REACT: &str = "18.3.1";

/// How far up the ancestors `client_dist` looks before giving up.
const SEARCH_DEPTH: usize = 8;

const TEMPLATE: &[(&str, &str)] = &[
  (".gitignore", include_str!("../templates/new/gitignore")),
  ("config/app.toml", include_str!("../templates/new/config/app.toml")),
  ("app/importmap.json", include_str!("../templates/new/app/importmap.json")),
  ("app/src/main.ts", include_str!("../templates/new/app/src/main.ts")),
  ("app/routes/layout.tsx", include_str!("../templates/new/app/routes/layout.tsx")),
  ("app/routes/page.loader.ts", include_str!("../templates/new/app/routes/page.loader.ts")),
  ("app/routes/page.tsx", include_str!("../templates/new/app/routes/page.tsx")),
  ("app/routes/not-found.tsx", include_str!("../templates/new/app/routes/not-found.tsx")),
  ("app/routes/error.tsx", include_str!("../templates/new/app/routes/error.tsx")),
  ("app/styles/app.css", include_str!("../templates/new/app/styles/app.css")),
];

pub struct NewOptions {
  /// Vendors React and fetches editor types, both of which reach the network.
  pub fetch: bool,
}

impl Default for NewOptions {
  fn default() -> Self {
    Self { fetch: true }
  }
}

#[derive(Debug, Default)]
pub struct Created {
  pub written: Vec<PathBuf>,
  pub vendored: Vec<(String, String, usize)>,
  pub typed: Vec<(String, String, String)>,
  /// What the scaffold could not settle, each a line the caller prints.
  pub notes: Vec<String>,
  /// What to type next, in order.
  pub next: Vec<String>,
}

/// Writes the project at `root`, whose app directory is `root/app`. Refuses a
/// `root` that already holds `app/` or `config/`, so a second run cannot
/// overwrite an application.
pub fn create(root: &Path, options: NewOptions) -> Result<Created, BuildError> {
  for occupied in ["app", "config"] {
    let path = root.join(occupied);
    if path.exists() {
      return Err(BuildError::Dev(format!("{} already exists; `fsr new` writes a fresh project", path.display())));
    }
  }
  let name = root.file_name().and_then(|n| n.to_str()).unwrap_or("app").to_owned();
  let app = root.join("app");

  let mut created = Created::default();
  let statics = match client_dist(root) {
    Some(dir) => format!("\n[[static]]\nroute = \"/static/js/fsr\"\ndir = \"{dir}\"\n"),
    None => {
      created.notes.push("no fsr client build beside this project; add a `[[static]]` for /static/js/fsr naming the client's dist/".to_owned());
      String::new()
    }
  };

  for (path, contents) in TEMPLATE {
    let contents = contents.replace("{{name}}", &name).replace("{{statics}}", &statics);
    let path = root.join(path);
    if let Some(parent) = path.parent() {
      std::fs::create_dir_all(parent).map_err(|e| BuildError::Io(parent.to_path_buf(), e))?;
    }
    std::fs::write(&path, contents).map_err(|e| BuildError::Io(path.clone(), e))?;
    created.written.push(path);
  }

  let add = format!("fsr add {} react@{REACT} react@{REACT}/jsx-runtime react-dom@{REACT}/client", app.display());
  if options.fetch {
    let specs: Vec<Spec> = [format!("react@{REACT}"), format!("react@{REACT}/jsx-runtime"), format!("react-dom@{REACT}/client")]
      .iter()
      .map(|s| Spec::parse(s))
      .collect::<Result<_, _>>()?;
    match vendor::add(&app, &specs, &[]) {
      Ok(report) => created.vendored = report.added,
      Err(e) => created.notes.push(format!("vendoring React failed ({e}); run `{add}`")),
    }
    match types::fetch(&app, false) {
      Ok(report) => created.typed = report.fetched,
      Err(e) => created.notes.push(format!("fetching types failed ({e}); run `fsr types {}`", app.display())),
    }
  } else {
    created.next.push(add);
    created.next.push(format!("fsr types {}", app.display()));
  }
  created.next.push(format!("fsr dev {}", app.display()));

  Ok(created)
}

/// The client runtime's build, looked for the way an example's `build.rs` looks
/// for snapfirec: up the ancestors of the project being created. The path is
/// written relative to the app directory, which is where `[[static]] dir`
/// resolves.
fn client_dist(root: &Path) -> Option<String> {
  let absolute = std::path::absolute(root).ok()?;
  for (up, ancestor) in absolute.ancestors().skip(1).take(SEARCH_DEPTH).enumerate() {
    if ancestor.join("fsr/client/dist").is_dir() {
      return Some(format!("{}fsr/client/dist", "../".repeat(up + 2)));
    }
  }
  None
}
