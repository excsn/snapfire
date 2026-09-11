//! Where a deploy tree puts what it ships.
//!
//! A destination is derived from what a file is, never from where it sat in
//! the project: configuration under `config/`, everything the application
//! reads under `app/`, every static root under `serve/<route>/`. Nothing a
//! configuration says is joined onto the tree root, so no setting can place a
//! file outside the directory the bundle owns and a tree is the same shape
//! whatever layout the project it came from happened to use.
//!
//! The paths that moved are named back in a generated `config/bundle.toml`,
//! the last layer `config_paths` loads. It carries the resolved settings
//! rather than the written ones, so a tree never re-infers from a directory
//! layout that is no longer the one inference was written for.
//!
//! Applying the layout to a tree yields the tree: its configuration already
//! names tree paths, so every destination equals its source. That is what
//! lets one function serve a project about to be bundled and an artifact
//! being verified.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Component, Path, PathBuf};

use snapfire_fsr_host::config::Config;

/// The configuration directory, the one place a tree keeps settings.
pub const CONFIG: &str = "config";
/// The application directory, which `[app] dir` names as this in every tree.
pub const APP: &str = "app";
/// The one directory a web server is pointed at. Nothing outside it is
/// reachable from the network.
pub const SERVE: &str = "serve";
/// The generated configuration layer, loaded after every other.
pub const LAYER: &str = "bundle.toml";
/// What the build wrote, under the application directory.
pub const GENERATED: &str = "generated";

#[derive(Debug, thiserror::Error)]
pub enum LayoutError {
  #[error("`{0}` would place a file outside the tree")]
  Escapes(String),
  #[error("{0} is placed twice")]
  Collides(String),
  #[error("the configuration was not read from a file, so there is nothing for a tree to carry")]
  NoConfig,
  #[error("{0}: {1}")]
  Io(PathBuf, #[source] std::io::Error),
  #[error("the configuration layer does not serialize: {0}")]
  Layer(String),
}

/// What a placed file is made of: something in the project or something the
/// layout writes itself.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Source {
  Path(PathBuf),
  Text(String),
}

/// One thing that ships: where it goes in the tree and what it is made of. A
/// `Path` source is a file or a directory and a directory ships whole.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Placement {
  pub to: String,
  pub from: Source,
  /// Whether the host refuses to start without it. A contracts directory, a
  /// prerender cache and a message catalog are each read as whatever is
  /// there, so their absence is a quieter application rather than one that
  /// does not come up.
  pub required: bool,
}

impl Placement {
  /// The file or directory this places, when it comes from the project.
  pub fn path(&self) -> Option<&Path> {
    match &self.from {
      Source::Path(path) => Some(path),
      Source::Text(_) => None,
    }
  }

  /// Whether what this places is there to be placed. Generated text always
  /// is.
  pub fn exists(&self) -> bool {
    self.path().is_none_or(Path::exists)
  }
}

/// Every placement a configuration implies, in destination order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Layout {
  pub places: Vec<Placement>,
}

/// One file of a tree, resolved out of the placement that carries it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Row {
  pub path: String,
  pub from: Source,
}

impl Row {
  pub fn bytes(&self) -> Result<Vec<u8>, LayoutError> {
    match &self.from {
      Source::Path(path) => std::fs::read(path).map_err(|e| LayoutError::Io(path.clone(), e)),
      Source::Text(text) => Ok(text.clone().into_bytes()),
    }
  }
}

impl Layout {
  /// Every file the tree holds, in path order, with what each comes from. A
  /// placement whose source is absent contributes nothing, the way a part
  /// that was never built ships as nothing rather than as a failure.
  pub fn rows(&self) -> Result<Vec<Row>, LayoutError> {
    let mut rows = Vec::new();
    for place in &self.places {
      match &place.from {
        Source::Text(text) => rows.push(Row {
          path: place.to.clone(),
          from: Source::Text(text.clone()),
        }),
        Source::Path(path) if path.is_dir() => walk(path, &place.to, &mut rows)?,
        Source::Path(path) if path.is_file() => rows.push(Row {
          path: place.to.clone(),
          from: Source::Path(path.clone()),
        }),
        Source::Path(_) => {}
      }
    }
    rows.sort_by(|a, b| a.path.cmp(&b.path));
    rows.dedup_by(|a, b| a.path == b.path);
    Ok(rows)
  }

  /// The destinations, which is what an artifact holds.
  pub fn parts(&self) -> Vec<String> {
    self.places.iter().map(|p| p.to.clone()).collect()
  }
}

fn walk(dir: &Path, prefix: &str, out: &mut Vec<Row>) -> Result<(), LayoutError> {
  let read = std::fs::read_dir(dir).map_err(|e| LayoutError::Io(dir.to_path_buf(), e))?;
  let mut paths: Vec<PathBuf> = Vec::new();
  for found in read {
    paths.push(found.map_err(|e| LayoutError::Io(dir.to_path_buf(), e))?.path());
  }
  paths.sort();
  for path in paths {
    let Some(name) = path.file_name().map(|n| n.to_string_lossy().into_owned()) else {
      continue;
    };
    let to = format!("{prefix}/{name}");
    if path.is_dir() {
      walk(&path, &to, out)?;
    } else if path.is_file() {
      inside(&to)?;
      out.push(Row {
        path: to,
        from: Source::Path(path),
      });
    }
  }
  Ok(())
}

/// A destination is relative and every component is a plain name, so joining
/// it onto the tree root cannot reach outside. The one check standing between
/// a configured path and a write.
fn inside(to: &str) -> Result<(), LayoutError> {
  let ok = !to.is_empty()
    && Path::new(to)
      .components()
      .all(|c| matches!(c, Component::Normal(name) if !name.is_empty()));
  if ok {
    Ok(())
  } else {
    Err(LayoutError::Escapes(to.to_owned()))
  }
}

fn name_of(path: &str) -> Option<String> {
  Path::new(path).file_name().map(|n| n.to_string_lossy().into_owned())
}

/// The deploy tree for `config`, whose project sits at `root`.
pub fn layout(root: &Path, config: &Config) -> Result<Layout, LayoutError> {
  let mut places: Vec<Placement> = Vec::new();
  let mut layer = Layer::default();
  let mut place = |to: String, from: Source, required: bool| -> Result<(), LayoutError> {
    inside(&to)?;
    if places.iter().any(|p: &Placement| p.to == to) {
      return Err(LayoutError::Collides(to));
    }
    places.push(Placement { to, from, required });
    Ok(())
  };

  if config.sources.is_empty() {
    return Err(LayoutError::NoConfig);
  }
  let config_dir = config.config_dir();
  let mut written = BTreeSet::new();
  if config_dir != root && config_dir.is_dir() {
    // The whole directory, not the files this run loaded: a tree deployed
    // under one `RELEASE_ENV` carries the overlays of every other.
    place(CONFIG.to_owned(), Source::Path(config_dir.clone()), true)?;
    if let Ok(entries) = std::fs::read_dir(&config_dir) {
      written.extend(entries.flatten().map(|e| e.file_name().to_string_lossy().into_owned()));
    }
  } else {
    for source in &config.sources {
      let Some(name) = source.file_name().map(|n| n.to_string_lossy().into_owned()) else {
        continue;
      };
      place(format!("{CONFIG}/{name}"), Source::Path(source.clone()), true)?;
      written.insert(name);
    }
  }

  layer.app_dir = APP.to_owned();

  let plan = name_of(&config.server.plan).unwrap_or_else(|| "plan.sexp".to_owned());
  let plan_at = format!("{GENERATED}/{plan}");
  place(
    format!("{APP}/{plan_at}"),
    Source::Path(config.resolve(&config.server.plan)),
    true,
  )?;
  layer.plan = plan_at;

  let contracts = format!("{GENERATED}/contracts");
  place(
    format!("{APP}/{contracts}"),
    Source::Path(config.resolve(&config.server.contracts)),
    false,
  )?;
  layer.contracts = contracts;

  if let Some(prerender) = &config.server.prerender {
    let at = format!("{GENERATED}/prerender");
    place(format!("{APP}/{at}"), Source::Path(config.resolve(prerender)), false)?;
    layer.prerender = Some(at);
  }

  if let Some(map) = &config.document.import_map {
    let name = name_of(map).unwrap_or_else(|| "importmap.json".to_owned());
    place(format!("{APP}/{name}"), Source::Path(config.resolve(map)), true)?;
    layer.import_map = Some(name);
  }
  layer.entry = config.document.entry.clone();
  layer.styles = config.document.styles.clone();
  layer.head = config.document.head.clone();

  // `build_facts` and the leak check read this by name. It also ships inside
  // whichever static root `dist/` serves and both copies are wanted: one
  // answers a request, this one answers the host at boot.
  let facts = config.app.join("dist/.snapfire-build.json");
  if facts.is_file() {
    place(
      format!("{APP}/dist/.snapfire-build.json"),
      Source::Path(facts),
      false,
    )?;
  }

  // `load_catalogs` reads this directory by name rather than through a
  // setting, so nothing else names it and a tree without it serves keys.
  let locales = config.app.join("locales");
  if locales.is_dir() {
    place(format!("{APP}/locales"), Source::Path(locales), false)?;
  }

  for (name, client) in &config.clients {
    let document = client
      .document
      .clone()
      .unwrap_or_else(|| format!("clients/{name}.openapi.json"));
    // The host reads the suffix to choose between a proto import and an
    // OpenAPI one, so the extension survives even though the stem does not.
    let at = if document.ends_with(".proto") {
      format!("clients/{name}.proto")
    } else {
      format!("clients/{name}.openapi.json")
    };
    place(format!("{APP}/{at}"), Source::Path(config.resolve(&document)), true)?;
    let responses = client.is_mock().then(|| {
      let at = format!("clients/{name}.mock.json");
      (at, config.resolve(&client.responses_file(name)))
    });
    if let Some((at, from)) = &responses {
      place(format!("{APP}/{at}"), Source::Path(from.clone()), true)?;
    }
    layer.clients.insert(
      name.clone(),
      (at, responses.map(|(at, _)| at)),
    );
  }

  for served in &config.statics {
    let route = served.route.trim_matches('/');
    let at = if route.is_empty() {
      SERVE.to_owned()
    } else {
      format!("{SERVE}/{route}")
    };
    place(at.clone(), Source::Path(config.resolve(&served.dir)), false)?;
    layer.statics.push((served.route.clone(), format!("../{at}")));
  }

  // The host answers this prefix out of its own binary, so the tree carries it
  // only for a web server that answers `serve/` before a request reaches the
  // host. A static root on the same prefix is an application serving its own
  // client. Nothing here is written then.
  //
  // A site is skipped because the shell answers the prefix on its behalf: a
  // root outside the site's own prefix is dropped at mount, and these rows
  // would otherwise move the hash every pin is checked against.
  use snapfire_fsr_host::client;
  let serves_client = config.site.is_none()
    && !config.statics.iter().any(|s| s.route.trim_end_matches('/') == client::ROUTE);
  if serves_client {
    let under = client::ROUTE.trim_matches('/');
    for (name, body) in client::FILES {
      place(format!("{SERVE}/{under}/{name}"), Source::Text((*body).to_owned()), false)?;
    }
  }

  // A tree already carries a layer and regenerating it would be a second
  // opinion about paths that are already the tree's own.
  if !written.contains(LAYER) {
    let text = layer.render()?;
    place(format!("{CONFIG}/{LAYER}"), Source::Text(text), true)?;
  }

  places.sort_by(|a, b| a.to.cmp(&b.to));
  Ok(Layout { places })
}

/// The settings a tree states for itself, because its own layout rather than
/// the project's decides them.
#[derive(Debug, Default)]
struct Layer {
  app_dir: String,
  plan: String,
  contracts: String,
  prerender: Option<String>,
  import_map: Option<String>,
  entry: Option<String>,
  styles: Option<Vec<String>>,
  head: Vec<BTreeMap<String, String>>,
  /// Client name to its document and its recorded responses when it mocks.
  clients: BTreeMap<String, (String, Option<String>)>,
  /// Route and the directory serving it, relative to the application.
  statics: Vec<(String, String)>,
}

impl Layer {
  fn render(&self) -> Result<String, LayoutError> {
    use toml::Value;
    let table = |rows: Vec<(&str, Value)>| Value::Table(rows.into_iter().map(|(k, v)| (k.to_owned(), v)).collect());
    let text = |s: &str| Value::String(s.to_owned());

    let mut app = vec![("dir", text(&self.app_dir))];
    let mut server = vec![("plan", text(&self.plan)), ("contracts", text(&self.contracts))];
    if let Some(prerender) = &self.prerender {
      server.push(("prerender", text(prerender)));
    }
    let mut document = Vec::new();
    if let Some(map) = &self.import_map {
      document.push(("import_map", text(map)));
    }
    if let Some(entry) = &self.entry {
      document.push(("entry", text(entry)));
    }
    if let Some(styles) = &self.styles {
      document.push((
        "styles",
        Value::Array(styles.iter().map(|s| text(s)).collect()),
      ));
    }
    if !self.head.is_empty() {
      document.push((
        "head",
        Value::Array(
          self
            .head
            .iter()
            .map(|el| Value::Table(el.iter().map(|(k, v)| (k.clone(), text(v))).collect()))
            .collect(),
        ),
      ));
    }

    let mut root: toml::map::Map<String, Value> = toml::map::Map::new();
    app.sort_by_key(|(k, _)| *k);
    root.insert("app".to_owned(), table(app));
    root.insert("server".to_owned(), table(server));
    if !document.is_empty() {
      root.insert("document".to_owned(), table(document));
    }
    if !self.clients.is_empty() {
      let mut clients: toml::map::Map<String, Value> = toml::map::Map::new();
      for (name, (document, responses)) in &self.clients {
        let mut rows = vec![("document", text(document))];
        if let Some(responses) = responses {
          rows.push(("responses", text(responses)));
        }
        clients.insert(name.clone(), table(rows));
      }
      root.insert("clients".to_owned(), Value::Table(clients));
    }
    root.insert(
      "static".to_owned(),
      Value::Array(
        self
          .statics
          .iter()
          .map(|(route, dir)| table(vec![("route", text(route)), ("dir", text(dir))]))
          .collect(),
      ),
    );

    let body = toml::to_string_pretty(&Value::Table(root)).map_err(|e| LayoutError::Layer(e.to_string()))?;
    Ok(format!(
      "# Written by `fsr bundle`. The tree's own layout, layered over the\n\
       # configuration beside it; `fsr bundle` writes it again from scratch.\n\n{body}"
    ))
  }
}
