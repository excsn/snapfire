mod build;
mod compiler;
mod config;
mod declarations;
mod graph;
mod importmap;
mod sources;
mod transforms;
mod watch;

use anyhow::{Context, Result, bail};
use clap::Parser;
use compiler::Minify;
use std::path::{Path, PathBuf};

#[derive(Parser, Debug)]
#[command(author, version, about = "A bespoke build tool for typescript web libraries", long_about = None)]
struct Args {
  /// Root directory to operate in
  #[arg(long, default_value = ".")]
  root: PathBuf,

  /// Rebuild whenever a source changes
  #[arg(short, long)]
  watch: bool,

  /// Path to tsconfig.json (relative to root)
  #[arg(short, long, default_value = "tsconfig.json")]
  config: PathBuf,

  /// Output directory (relative to root)
  #[arg(short = 'd', long)]
  out_dir: Option<PathBuf>,

  /// Strip all `console.log` statements
  #[arg(long)]
  strip_log: bool,

  /// Strip all `console.debug` statements
  #[arg(long)]
  strip_debug: bool,

  /// Copy every selected file the compiler does not compile
  #[arg(long)]
  copy_assets: bool,

  /// Emit a .map alongside each output
  #[arg(long)]
  source_map: bool,

  /// Embed the source map in each output as a data URI
  #[arg(long)]
  inline_source_map: bool,

  /// Emit a .d.ts beside each TypeScript output
  #[arg(long)]
  declaration: bool,

  /// Additionally emit a minified `.min` graph
  #[arg(long, value_enum, num_args = 0..=1, default_missing_value = "compact")]
  minify: Option<Minify>,

  /// URL prefix the output will be served under, used for the preload manifest
  #[arg(long)]
  public_path: Option<String>,

  /// Directory whose files stand in for the root's at the same relative path
  #[arg(long)]
  overlay: Option<PathBuf>,

  /// Import map to check every external against
  #[arg(long)]
  import_map: Option<PathBuf>,
}

fn main() -> Result<()> {
  let args = Args::parse();

  if args.root != Path::new(".") {
    std::env::set_current_dir(&args.root).context(format!("Failed to set working directory to {:?}", args.root))?;
  }

  if args.minify == Some(Minify::Full) && !compiler::FULL_MINIFIER {
    bail!("'--minify=full' needs a binary built with the 'minify' feature: cargo install snapfire_compiler --features minify");
  }

  let options = build::Options {
    root: std::env::current_dir().context("Failed to resolve the working directory")?,
    config_path: args.config,
    out_dir_flag: args.out_dir,
    strip_log: args.strip_log,
    strip_debug: args.strip_debug,
    copy_assets: args.copy_assets,
    source_map: args.source_map,
    inline_source_map: args.inline_source_map,
    minify: args.minify,
    declaration: args.declaration,
    public_path: args.public_path.map(|p| if p.ends_with('/') { p } else { format!("{p}/") }),
    import_map: args.import_map,
    overlay: args.overlay,
  };

  let outcome = build::full(&options, true)?;

  if args.watch {
    return watch::run(&options, outcome);
  }

  if outcome.emitted == 0 && !outcome.has_error {
    bail!(
      "No inputs were found in {:?}. Specified 'include' paths were {:?}.",
      options.config_path,
      outcome.include_patterns
    );
  }

  if outcome.has_error {
    bail!("Build failed. See the errors above.");
  }

  Ok(())
}
