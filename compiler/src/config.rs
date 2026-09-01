use anyhow::{Context, Result, bail};
use jsonc_parser::ParseOptions;
use serde::Deserialize;
use std::fs;
use std::path::{Path, PathBuf};

/// The lowest target snapfirec can honour. Output is ES modules, and no engine that supports
/// `<script type="module">` predates ES2017, so anything below this is unsatisfiable rather than
/// merely unimplemented.
const MIN_TARGET: u32 = 2017;

#[derive(Debug, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct CompilerOptions {
  pub out_dir: Option<PathBuf>,
  pub root_dir: Option<PathBuf>,
  pub target: Option<String>,
  pub source_map: Option<bool>,
  pub inline_source_map: Option<bool>,
  pub inline_sources: Option<bool>,
  pub declaration: Option<bool>,
  pub jsx: Option<String>,
  pub jsx_import_source: Option<String>,
}

/// How JSX is lowered. `Preserve` writes the markup through untouched, which is
/// only useful as input to another tool.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum Jsx {
  Preserve,
  Automatic { import_source: String, development: bool },
}

impl Jsx {
  /// tsc's `jsx` key, restricted to what snapfirec emits. The classic runtime
  /// is rejected rather than silently preserved, since its output needs a
  /// `React` binding this compiler does not inject.
  pub fn resolve(options: &CompilerOptions) -> Result<Self> {
    let import_source = || {
      options
        .jsx_import_source
        .clone()
        .unwrap_or_else(|| "react".to_owned())
    };

    match options.jsx.as_deref() {
      None | Some("preserve") => Ok(Jsx::Preserve),
      Some("react-jsx") => Ok(Jsx::Automatic { import_source: import_source(), development: false }),
      Some("react-jsxdev") => Ok(Jsx::Automatic { import_source: import_source(), development: true }),
      Some(mode @ ("react" | "react-native")) => bail!(
        "'jsx': {:?} is not supported. snapfirec lowers JSX through the automatic runtime only: set 'jsx' to \"react-jsx\".",
        mode
      ),
      Some(mode) => bail!("'jsx': {:?} is not a recognised mode.", mode),
    }
  }

  /// The module the runtime is imported from, which an import map has to name.
  pub fn runtime_specifier(&self) -> Option<String> {
    match self {
      Jsx::Preserve => None,
      Jsx::Automatic { import_source, development } => Some(format!(
        "{import_source}/jsx-{}runtime",
        if *development { "dev-" } else { "" }
      )),
    }
  }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum MapMode {
  Off,
  External,
  Inline,
}

#[derive(Clone, Copy)]
pub struct MapOptions {
  pub mode: MapMode,
  pub inline_sources: bool,
}

impl MapOptions {
  pub fn resolve(options: &CompilerOptions, external_flag: bool, inline_flag: bool) -> Result<Self> {
    let external = external_flag || options.source_map.unwrap_or(false);
    let inline = inline_flag || options.inline_source_map.unwrap_or(false);

    if external && inline {
      bail!("'sourceMap' and 'inlineSourceMap' cannot both be set.");
    }

    let mode = match (external, inline) {
      (_, true) => MapMode::Inline,
      (true, _) => MapMode::External,
      _ => MapMode::Off,
    };

    Ok(Self {
      mode,
      inline_sources: options.inline_sources.unwrap_or(false),
    })
  }
}

#[derive(Debug, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct TsConfig {
  pub compiler_options: Option<CompilerOptions>,
  pub files: Option<Vec<String>>,
  pub include: Option<Vec<String>>,
  pub exclude: Option<Vec<String>>,
}

impl TsConfig {
  pub fn load(path: &Path) -> Result<Self> {
    if !path.exists() {
      return Ok(TsConfig::default());
    }

    let content = fs::read_to_string(path).with_context(|| format!("Failed to read {:?}", path))?;

    // tsconfig.json is JSONC: tsc accepts comments and trailing commas.
    jsonc_parser::parse_to_serde_value::<TsConfig>(&content, &ParseOptions::default())
      .with_context(|| format!("Failed to parse {:?}", path))
  }
}

/// Rejects a target snapfirec cannot deliver.
///
/// Anything at or above the floor passes without comment. `target` is tsc's key and tsc genuinely
/// uses it, so a project is right to set it; warning on every build about a correct config would
/// only teach people to ignore warnings. That snapfirec does not downlevel is a documented property
/// of the tool rather than something to restate per build.
pub fn check_target(target: Option<&str>) -> Result<()> {
  let Some(target) = target else {
    return Ok(());
  };

  let Some(year) = target_year(target) else {
    eprintln!("⚠️  'target': {:?} is not recognised and has no effect.", target);
    return Ok(());
  };

  if year < MIN_TARGET {
    bail!(
      "'target': {:?} cannot be honoured. snapfirec emits ES modules, which no pre-ES2017 engine can load.",
      target
    );
  }

  Ok(())
}

fn target_year(target: &str) -> Option<u32> {
  match target.to_ascii_lowercase().as_str() {
    "es3" => Some(3),
    "es5" => Some(5),
    "es6" | "es2015" => Some(2015),
    "esnext" | "latest" => Some(u32::MAX),
    // A four-digit edition year and nothing else, so that a typo like `es2O22` or `es20200` is
    // reported rather than silently read as a target far in the future.
    other => other
      .strip_prefix("es")
      .filter(|year| year.len() == 4)
      .and_then(|year| year.parse().ok())
      .filter(|year| (2015..=2100).contains(year)),
  }
}
