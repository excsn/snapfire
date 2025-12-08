mod compiler;
mod config;
mod transforms;

use anyhow::{Context, Result, bail};
use browserslist::{Opts, execute};
use clap::Parser;
use compiler::Compiler;
use config::TsConfig;
use std::fs;
use std::path::{Path, PathBuf};
use walkdir::WalkDir;

#[derive(Parser, Debug)]
#[command(author, version, about = "A bespoke build tool for typescript web libraries", long_about = None)]
struct Args {
  /// Root directory to operate in
  #[arg(long, default_value = ".")]
  root: PathBuf,

  /// Watch for file changes
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
}

fn main() -> Result<()> {
  let args = Args::parse();

  // Change working directory if root is specified
  if args.root != Path::new(".") {
    std::env::set_current_dir(&args.root).context(format!("Failed to set working directory to {:?}", args.root))?;
  }

  // Load tsconfig
  let tsconfig = TsConfig::load(&args.config)?;

  // Determine output directory
  let out_dir = args
    .out_dir
    .or(tsconfig.compiler_options.and_then(|o| o.out_dir))
    .unwrap_or_else(|| PathBuf::from("dist"));

  // Determine source directories (Default to "src" if include is missing)
  let include_dirs = tsconfig.include.unwrap_or_else(|| vec!["src".to_string()]);

  // Use `execute` to find and resolve the browserslist config.
  let opts = Opts {
    path: Some(format!("{:?}", args.root)),
    ..Default::default()
  };
  let distribs = execute(&opts).context("Failed to execute browserslist")?;

  // Convert the resolved distributions back into a single query string for lightningcss.
  let browserslist_query = distribs
    .iter()
    .map(|d| format!("{} {}", d.name(), d.version()))
    .collect::<Vec<_>>()
    .join(", ");

  println!("🔥 snapfirec started");
  println!("   Root:   {:?}", args.root);
  println!("   Config: {:?}", args.config);
  println!("   Output: {:?}", out_dir);
  println!("   Sources: {:?}", include_dirs);
  println!("   Browser Targets: '{}'", browserslist_query);

  let compiler = Compiler::new(&browserslist_query);

  if !out_dir.exists() {
    fs::create_dir_all(&out_dir)?;
  }

  // Get absolute path of output directory for robust filtering
  let abs_out_dir = fs::canonicalize(&out_dir).context("Failed to resolve absolute path of output directory")?;

  let mut has_error = false;

  for source in include_dirs {
    let source_path = Path::new(&source);
    if !source_path.exists() {
      eprintln!("⚠️  Source directory not found: {:?}", source_path);
      continue;
    }

    let abs_source =
      fs::canonicalize(source_path).context(format!("Failed to resolve absolute source path: {:?}", source_path))?;

    let walker = WalkDir::new(&abs_source).into_iter().filter_entry(|e| {
      let p = e.path();

      if p == abs_out_dir {
        return false;
      }

      if let Some(name) = p.file_name() {
        if name == "node_modules" || name == ".git" {
          return false;
        }
      }

      true
    });

    for entry in walker {
      let entry = match entry {
        Ok(e) => e,
        Err(e) => {
          eprintln!("⚠️  Error accessing path: {}", e);
          has_error = true;
          continue;
        }
      };

      let path = entry.path();

      if path.is_file() {
        if let Some(ext) = path.extension() {
          let relative_path = path.strip_prefix(&abs_source).unwrap_or(path);
          let mut dest_path = out_dir.join(relative_path);

          if let Some(parent) = dest_path.parent() {
            fs::create_dir_all(parent)?;
          }

          if ext == "ts" || ext == "tsx" {
            dest_path.set_extension("js");
            println!("   Compiling TS: {:?}", relative_path);
            match compiler.compile_ts(path, args.strip_log, args.strip_debug) {
              Ok(js) => {
                if let Err(e) = fs::write(&dest_path, js).with_context(|| format!("Failed to write {:?}", dest_path)) {
                  eprintln!("❌ Error writing output: {:?}", e);
                  has_error = true;
                }
              }
              Err(e) => {
                eprintln!("❌ Error compiling TS {:?}: {:?}", path, e);
                has_error = true;
              }
            }
          } else if ext == "css" {
            println!("   Compiling CSS: {:?}", relative_path);
            match compiler.compile_css(path) {
              Ok(css) => {
                if let Err(e) = fs::write(&dest_path, css).with_context(|| format!("Failed to write {:?}", dest_path)) {
                  eprintln!("❌ Error writing output: {:?}", e);
                  has_error = true;
                }
              }
              Err(e) => {
                eprintln!("❌ Error compiling CSS {:?}: {:?}", path, e);
                has_error = true;
              }
            }
          }
        }
      }
    }
  }

  // --- Fail if errors occurred ---
  if has_error {
    bail!("Build failed due to compilation errors.");
  }

  Ok(())
}
