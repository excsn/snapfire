//! `fsr migrate`: rewrites an application written for another framework into
//! the layout and idioms of an FSR application. The input and the output are
//! [`Tree`]s, so nothing here touches the disk; what a pass could not rewrite
//! is a [`Note`] naming the line.

mod config;
mod edit;
mod head;
mod items;
mod lift;
mod modules;
mod next;
mod pages;
mod relocate;
mod syntax;

use std::collections::BTreeMap;


/// Files by `/`-separated path relative to the project root.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Tree {
  files: BTreeMap<String, Vec<u8>>,
}

impl Tree {
  pub fn new() -> Self {
    Self::default()
  }

  pub fn insert(&mut self, path: impl Into<String>, bytes: impl Into<Vec<u8>>) {
    self.files.insert(path.into(), bytes.into());
  }

  pub fn get(&self, path: &str) -> Option<&[u8]> {
    self.files.get(path).map(Vec::as_slice)
  }

  /// The file at `path` when it is UTF-8.
  pub fn text(&self, path: &str) -> Option<&str> {
    self.get(path).and_then(|b| std::str::from_utf8(b).ok())
  }

  pub fn contains(&self, path: &str) -> bool {
    self.files.contains_key(path)
  }

  pub fn remove(&mut self, path: &str) -> Option<Vec<u8>> {
    self.files.remove(path)
  }

  pub fn paths(&self) -> impl Iterator<Item = &str> {
    self.files.keys().map(String::as_str)
  }

  pub fn iter(&self) -> impl Iterator<Item = (&str, &[u8])> {
    self.files.iter().map(|(p, b)| (p.as_str(), b.as_slice()))
  }

  pub fn len(&self) -> usize {
    self.files.len()
  }

  pub fn is_empty(&self) -> bool {
    self.files.is_empty()
  }
}

/// The framework an application was written for, read from its
/// `package.json` and its directories.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum Source {
  /// Next.js with an `app/` or `src/app/` directory.
  NextApp,
  /// Next.js with only a `pages/` or `src/pages/` directory.
  NextPages,
  SvelteKit,
  Nuxt,
}

impl Source {
  pub fn name(self) -> &'static str {
    match self {
      Source::NextApp => "Next.js App Router",
      Source::NextPages => "Next.js Pages Router",
      Source::SvelteKit => "SvelteKit",
      Source::Nuxt => "Nuxt",
    }
  }

  /// The framework FSR renders the migrated application's components with.
  pub fn direction(self) -> &'static str {
    match self {
      Source::NextApp | Source::NextPages => "react",
      Source::SvelteKit => "svelte",
      Source::Nuxt => "vue",
    }
  }
}

/// What `tree` was written for; `None` when nothing says.
pub fn detect(tree: &Tree) -> Option<Source> {
  let package = tree.text("package.json").and_then(|t| serde_json::from_str::<serde_json::Value>(t).ok());
  let depends = |name: &str| {
    package.as_ref().is_some_and(|p| ["dependencies", "devDependencies"].iter().any(|k| p.get(k).and_then(|d| d.get(name)).is_some()))
  };
  let has_dir = |dir: &str| tree.paths().any(|p| p.starts_with(&format!("{dir}/")));
  if depends("next") || tree.paths().any(|p| p.starts_with("next.config.")) {
    if has_dir("app") || has_dir("src/app") {
      return Some(Source::NextApp);
    }
    return Some(Source::NextPages);
  }
  if depends("@sveltejs/kit") {
    return Some(Source::SvelteKit);
  }
  if depends("nuxt") || tree.paths().any(|p| p.starts_with("nuxt.config.")) {
    return Some(Source::Nuxt);
  }
  None
}

/// Something a pass left for a person, at a line of the input.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Note {
  /// Relative to the project root, as the pass that wrote the note saw it.
  pub file: String,
  /// One-based; 0 when the note is about the file as a whole.
  pub line: usize,
  pub message: String,
}

impl std::fmt::Display for Note {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    match self.line {
      0 => write!(f, "{}: {}", self.file, self.message),
      line => write!(f, "{}:{line}: {}", self.file, self.message),
    }
  }
}

#[derive(Debug, Clone, Default)]
pub struct Report {
  pub source: Option<Source>,
  /// The passes that ran, in order.
  pub passes: Vec<&'static str>,
  pub notes: Vec<Note>,
  /// Packages the input depends on that the output still imports, by name
  /// and the version range `package.json` asked for.
  pub packages: BTreeMap<String, String>,
  /// Commands to run in the migrated project, in order, each once.
  pub commands: Vec<String>,
  /// TOML tables for `config/app.toml`, each written whole.
  pub config: Vec<String>,
}

impl Report {
  pub fn note(&mut self, file: &str, line: usize, message: impl Into<String>) {
    self.notes.push(Note { file: file.to_owned(), line, message: message.into() });
  }

  pub fn command(&mut self, command: impl Into<String>) {
    let command = command.into();
    if !self.commands.contains(&command) {
      self.commands.push(command);
    }
  }
}

/// One rewrite over the whole tree.
pub trait Pass {
  fn name(&self) -> &'static str;
  fn run(&self, tree: &mut Tree, report: &mut Report);
}

/// The passes for `source`, in the order they run.
pub fn passes(source: Source) -> Vec<Box<dyn Pass>> {
  match source {
    Source::NextApp => next::app_passes().into_iter().chain(modules::passes()).chain(lift::passes()).collect(),
    Source::NextPages => pages::passes().into_iter().chain(modules::passes()).chain(lift::passes()).collect(),
    Source::SvelteKit | Source::Nuxt => Vec::new(),
  }
}

/// `input` rewritten as an FSR project tree: the application under `app/`.
pub fn migrate(input: &Tree) -> (Tree, Report) {
  let mut report = Report { source: detect(input), ..Report::default() };
  let mut tree = input.clone();
  match report.source {
    None => report.note("package.json", 0, "names neither next, @sveltejs/kit nor nuxt, so nothing says what this application was written for"),
    Some(source) => {
      let passes = passes(source);
      if passes.is_empty() {
        report.note("package.json", 0, format!("{} is not migrated yet", source.name()));
      }
      for pass in passes {
        report.passes.push(pass.name());
        pass.run(&mut tree, &mut report);
      }
    }
  }
  (tree, report)
}

/// The passes that work on an FSR application whatever it was written for,
/// run alone on `tree`: what a component awaits while it renders moved to
/// the route's loader.
pub fn lift(tree: &Tree) -> (Tree, Report) {
  let mut report = Report::default();
  let mut tree = tree.clone();
  for pass in lift::passes() {
    report.passes.push(pass.name());
    pass.run(&mut tree, &mut report);
  }
  (tree, report)
}
