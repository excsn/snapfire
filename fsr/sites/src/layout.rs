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
//! names tree paths, so every destination equals its source. One function can
//! therefore serve both a project about to be bundled and an artifact being
//! verified.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Component, Path, PathBuf};

use snapfire_fsr_host::config::{Config, Deployment, Loader};

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
#[non_exhaustive]
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
  #[error("`{0}` is placed from two files: {1} names one and {2} another")]
  Disagree(String, String, String),
  #[error("config/{0} does not load, so the tree cannot carry it: {1}")]
  Overlay(String, String),
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
#[non_exhaustive]
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
#[non_exhaustive]
pub struct Layout {
  pub places: Vec<Placement>,
}

/// One file of a tree, resolved out of the placement that carries it.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
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

/// Placements so far, each with the configuration file that asked for it.
#[derive(Default)]
struct Placer {
  places: Vec<Placement>,
  origins: Vec<String>,
}

impl Placer {
  /// Places `from` at `to`. The same file asked for twice is placed once; two
  /// files sent to one path are refused, naming the layers when they differ.
  fn place(&mut self, to: String, from: Source, required: bool, origin: &str) -> Result<(), LayoutError> {
    inside(&to)?;
    if let Some(at) = self.places.iter().position(|p| p.to == to) {
      if self.places[at].from == from {
        return Ok(());
      }
      if self.origins[at] == origin {
        return Err(LayoutError::Collides(to));
      }
      return Err(LayoutError::Disagree(to, self.origins[at].clone(), origin.to_owned()));
    }
    self.places.push(Placement { to, from, required });
    self.origins.push(origin.to_owned());
    Ok(())
  }
}

/// The configuration files in `dir` other than `app` and the tree's own
/// layer: the overlays a deployment may load, each of which can name files.
fn overlays(dir: &Path) -> Result<Vec<PathBuf>, LayoutError> {
  let mut found = Vec::new();
  for entry in std::fs::read_dir(dir).map_err(|e| LayoutError::Io(dir.to_path_buf(), e))? {
    let path = entry.map_err(|e| LayoutError::Io(dir.to_path_buf(), e))?.path();
    let is_config = path.extension().is_some_and(|ext| ext == "toml" || ext == "yaml");
    let stem = path.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
    if path.is_file() && is_config && stem != "app" && stem != "bundle" {
      found.push(path);
    }
  }
  found.sort();
  Ok(found)
}

/// Places every file `config` names and records where each went in the
/// layer. `base` is the configuration the tree was asked for; an overlay
/// adds what it names and fills in what the base left unsaid.
fn place_named(config: &Config, origin: &str, base: bool, placer: &mut Placer, layer: &mut Layer) -> Result<(), LayoutError> {
  let plan = name_of(&config.server.plan).unwrap_or_else(|| "plan.sexp".to_owned());
  let plan_at = format!("{GENERATED}/{plan}");
  placer.place(format!("{APP}/{plan_at}"), Source::Path(config.resolve(&config.server.plan)), true, origin)?;
  if base {
    layer.plan = plan_at;
  }

  let contracts = format!("{GENERATED}/contracts");
  placer.place(format!("{APP}/{contracts}"), Source::Path(config.resolve(&config.server.contracts)), false, origin)?;
  if base {
    layer.contracts = contracts;
  }

  if let Some(prerender) = &config.server.prerender {
    let at = format!("{GENERATED}/prerender");
    placer.place(format!("{APP}/{at}"), Source::Path(config.resolve(prerender)), false, origin)?;
    layer.prerender.get_or_insert(at);
  }

  if let Some(map) = &config.document.import_map {
    let name = name_of(map).unwrap_or_else(|| "importmap.json".to_owned());
    placer.place(format!("{APP}/{name}"), Source::Path(config.resolve(map)), true, origin)?;
    layer.import_map.get_or_insert(name);
  }
  if base {
    layer.entry = config.document.entry.clone();
    layer.styles = config.document.styles.clone();
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
    placer.place(format!("{APP}/{at}"), Source::Path(config.resolve(&document)), true, origin)?;
    let responses = client.is_mock().then(|| {
      let at = format!("clients/{name}.mock.json");
      (at, config.resolve(&client.responses_file(name)))
    });
    if let Some((at, from)) = &responses {
      placer.place(format!("{APP}/{at}"), Source::Path(from.clone()), true, origin)?;
    }
    let responses = responses.map(|(at, _)| at);
    match layer.clients.get_mut(name) {
      Some((held, held_responses)) => {
        if *held != at {
          return Err(LayoutError::Disagree(format!("{APP}/{at}"), origin.to_owned(), format!("another layer, as {held}")));
        }
        if held_responses.is_none() {
          *held_responses = responses;
        }
      }
      None => {
        layer.clients.insert(name.clone(), (at, responses));
      }
    }
  }

  for served in &config.statics {
    let route = served.route.trim_matches('/');
    let at = if route.is_empty() {
      SERVE.to_owned()
    } else {
      format!("{SERVE}/{route}")
    };
    placer.place(at.clone(), Source::Path(config.resolve(&served.dir)), false, origin)?;
    if !layer.statics.iter().any(|(held, _)| *held == served.route) {
      layer.statics.push((served.route.clone(), format!("../{at}")));
    }
  }
  Ok(())
}

/// The deploy tree for `config`, whose project sits at `root`. The tree
/// carries every configuration file under `config/`, so it is laid out for
/// every overlay there and not only for the deployment this run loaded.
pub fn layout(root: &Path, config: &Config) -> Result<Layout, LayoutError> {
  let mut placer = Placer::default();
  let mut layer = Layer::default();

  if config.sources.is_empty() {
    return Err(LayoutError::NoConfig);
  }
  let config_dir = config.config_dir();
  let mut written = BTreeSet::new();
  let base = config.sources.first().and_then(|p| p.file_name()).map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| "app.toml".to_owned());
  let whole_dir = config_dir != root && config_dir.is_dir();
  if whole_dir {
    placer.place(CONFIG.to_owned(), Source::Path(config_dir.clone()), true, &base)?;
    if let Ok(entries) = std::fs::read_dir(&config_dir) {
      written.extend(entries.flatten().map(|e| e.file_name().to_string_lossy().into_owned()));
    }
  } else {
    for source in &config.sources {
      let Some(name) = source.file_name().map(|n| n.to_string_lossy().into_owned()) else {
        continue;
      };
      placer.place(format!("{CONFIG}/{name}"), Source::Path(source.clone()), true, &base)?;
      written.insert(name);
    }
  }

  layer.app_dir = APP.to_owned();
  place_named(config, &base, true, &mut placer, &mut layer)?;
  if whole_dir {
    for file in overlays(&config_dir)? {
      let name = file.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
      let stem = file.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
      let deployment = Deployment::default().release_env(stem.clone()).app_env(stem).region(None);
      let overlay = Loader::at(&config_dir).deployment(deployment).config().map_err(|e| LayoutError::Overlay(name.clone(), e.to_string()))?;
      place_named(&overlay, &name, false, &mut placer, &mut layer)?;
    }
  }

  // `build_facts` and the leak check read this by name. It also ships inside
  // whichever static root `dist/` serves and both copies are wanted: one
  // answers a request, this one answers the host at boot.
  // The host reads it at boot for the image policy, the font CSS and the
  // preloads; the variants and the font copies it names ship inside the
  // static root `dist/` serves.
  let assets = config.app.join(snapfire_fsr_host::assets::ASSETS_FILE);
  if assets.is_file() {
    placer.place(format!("{APP}/{}", snapfire_fsr_host::assets::ASSETS_FILE), Source::Path(assets), false, &base)?;
  }

  let cards = config.app.join(snapfire_fsr_host::CARD_CLASSES_FILE);
  if cards.is_file() {
    placer.place(format!("{APP}/{}", snapfire_fsr_host::CARD_CLASSES_FILE), Source::Path(cards), false, &base)?;
  }

  let facts = config.app.join("dist/.snapfire-build.json");
  if facts.is_file() {
    placer.place(format!("{APP}/dist/.snapfire-build.json"), Source::Path(facts), false, &base)?;
  }

  // `load_catalogs` reads this directory by name rather than through a
  // setting, so nothing else names it and a tree without it serves keys.
  let locales = config.app.join("locales");
  if locales.is_dir() {
    placer.place(format!("{APP}/locales"), Source::Path(locales), false, &base)?;
  }

  let templates = snapfire_fsr_host::template_files(&config.app).map_err(|e| match e {
    snapfire_fsr_host::HostError::Io(path, e) => LayoutError::Io(path, e),
    other => LayoutError::Io(config.app.clone(), std::io::Error::other(other.to_string())),
  })?;
  for file in templates {
    let rel = file.strip_prefix(&config.app).unwrap_or(&file).to_string_lossy().replace('\\', "/");
    placer.place(format!("{APP}/{rel}"), Source::Path(file), true, &base)?;
  }

  // The host answers this prefix out of its own binary, so the tree carries it
  // only for a web server that answers `serve/` before a request reaches the
  // host. A static root on the same prefix is an application serving its own
  // client. Nothing here is written then.
  //
  // A site is skipped because the shell answers the prefix on its behalf: a
  // root outside the site's own prefix is dropped at mount and these rows
  // would otherwise move the hash every pin is checked against.
  use snapfire_fsr_host::client;
  let serves_client = config.site.is_none()
    && !config.statics.iter().any(|s| s.route.trim_end_matches('/') == client::ROUTE);
  if serves_client {
    let under = client::ROUTE.trim_matches('/');
    // `config.dev()` here is the `RELEASE_ENV` of the machine bundling the
    // tree, never the one the tree is deployed under.
    let minified = config.document.client.minified(false);
    for (name, _) in client::FILES {
      let body = client::get(name, minified).unwrap_or_default();
      placer.place(format!("{SERVE}/{under}/{name}"), Source::Text(body.to_owned()), false, &base)?;
    }
    if minified {
      for (name, body) in client::MINIFIED {
        let name = name.replace(".js", ".min.js");
        placer.place(format!("{SERVE}/{under}/{name}"), Source::Text((*body).to_owned()), false, &base)?;
      }
    }
  }

  // A tree already carries a layer and regenerating it would be a second
  // opinion about paths that are already the tree's own.
  if !written.contains(LAYER) {
    let text = layer.render()?;
    placer.place(format!("{CONFIG}/{LAYER}"), Source::Text(text), true, &base)?;
  }

  let mut places = placer.places;
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
    let mut server = vec![("plan", text(&self.plan)), ("contracts", text(&self.contracts)), ("dev", Value::Boolean(false))];
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
