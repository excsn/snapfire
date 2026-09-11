//! `fsr new`: writes the smallest application the stock host can serve, in the
//! shape [DX.md] section 1 describes. No Cargo project and no `package.json`;
//! `fsr dev` on the app directory is what runs it.

use std::path::{Path, PathBuf};

use crate::vendor::{self, Spec};
use crate::{types, BuildError};

/// The React runtime a scaffolded app vendors, pinned the way the examples pin it.
pub const REACT: &str = "18.3.1";

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
  /// Scaffolds a shell: the configuration gains the `[sites]` table sites mount into.
  pub shell: bool,
  /// Scaffolds a site, mounted at a path and optionally linked into a shell.
  pub site: Option<SiteScaffold>,
}

/// What `fsr new --site` writes beside the app, and the shell it links into.
pub struct SiteScaffold {
  /// The prefix the shell mounts the site under.
  pub at: String,
  /// The site's name; the project directory's own name when absent.
  pub name: Option<String>,
  /// A shell to link into once the project is written, which writes both
  /// halves rather than the `[site]` section alone.
  pub into: Option<PathBuf>,
}

impl Default for NewOptions {
  fn default() -> Self {
    Self { fetch: true, shell: false, site: None }
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
  /// The shell this project was linked into, when `--into` named one.
  pub linked: Option<crate::sites::Linked>,
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
  if options.shell && options.site.is_some() {
    return Err(BuildError::Dev("a site cannot mount sites; pass --shell or --site, not both".to_owned()));
  }
  let name = root.file_name().and_then(|n| n.to_str()).unwrap_or("app").to_owned();
  let app = root.join("app");

  // Nothing is written until the section the flags ask for is known to be one
  // the host would accept, so a refused name leaves no half-made project.
  let site_section = match &options.site {
    Some(site) => {
      let site_name = match &site.name {
        Some(given) => given.clone(),
        None => crate::sites::name_from(&name)?,
      };
      crate::sites::check(&site_name, &site.at)?;
      // `--into` writes the whole `[site]`, shell path and all, once the
      // project exists; writing a partial one here would only be replaced.
      match site.into {
        Some(_) => String::new(),
        None => format!("\n[site]\nname = \"{site_name}\"\nat = \"{}\"\n", site.at),
      }
    }
    None if options.shell => "\n[sites]\npoll = \"5s\"\n".to_owned(),
    None => String::new(),
  };

  let mut created = Created::default();

  for (path, contents) in TEMPLATE {
    let contents = contents.replace("{{name}}", &name).replace("{{site}}", &site_section);
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
      Ok(report) => {
        created.typed = report.fetched;
        // The scaffold's own routes import `@snapfire/fsr` and `@generated/*`,
        // which are written by a build rather than by the template. The
        // `paths` that resolve every other import live in the `tsconfig.json`
        // a build writes. Without this an editor opened on a fresh project
        // reports four unresolved imports that are not wrong.
        match generate(&app) {
          Ok(written) => created.written.extend(written),
          Err(e) => created.notes.push(format!("writing the generated modules failed ({e}); run `fsr build {}`", app.display())),
        }
      }
      Err(e) => created.notes.push(format!("fetching types failed ({e}); run `fsr types {}`", app.display())),
    }
  } else {
    created.next.push(add);
    created.next.push(format!("fsr types {}", app.display()));
    created.next.push(format!("fsr build {}", app.display()));
  }
  if let Some(site) = &options.site {
    if let Some(shell) = &site.into {
      created.linked = Some(crate::sites::link(shell, root, &site.at, site.name.as_deref())?);
    }
  }
  created.next.push(format!("fsr dev {}", app.display()));

  Ok(created)
}

/// `generated/`, `tsconfig.json` and `tsconfig.build.json`, which is what the
/// editor resolves imports through. The bundler is not run: `dist/` is what
/// serving needs and `fsr dev` writes it.
fn generate(app: &Path) -> Result<Vec<PathBuf>, BuildError> {
  let built = crate::build(app, &crate::Options::beside(app))?;
  crate::write(app, &built)
}

