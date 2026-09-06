use std::path::{Path, PathBuf};
use std::process::ExitCode;

use snapfire_typecheck::{cached, check, is_pinned, platform, resolve, Diagnostic, Options, Resolved, DEFAULT_VERSION};

const USAGE: &str = "usage: snapfiretc --root <dir> --config <tsconfig> [--format text|json] [--tsc <path>] [--tsc-version <version>] [--cache <dir>] [--registry <url>] [--expect <sha512>] [--offline]\n       snapfiretc --which [--tsc <path>] [--tsc-version <version>] [--cache <dir>] [--offline]";

fn usage() -> ExitCode {
  eprintln!("{USAGE}");
  ExitCode::from(2)
}

struct Args {
  root: PathBuf,
  config: PathBuf,
  json: bool,
  which: bool,
  options: Options,
}

fn parse() -> Option<Args> {
  let mut args = Args { root: PathBuf::from("."), config: PathBuf::from("tsconfig.json"), json: false, which: false, options: Options::default() };
  let raw: Vec<String> = std::env::args().skip(1).collect();
  let mut rest = raw.iter();
  while let Some(flag) = rest.next() {
    match flag.as_str() {
      "--offline" => args.options.offline = true,
      "--which" => args.which = true,
      "--version" => {
        println!("snapfiretc {} (TypeScript {DEFAULT_VERSION} by default)", env!("CARGO_PKG_VERSION"));
        std::process::exit(0);
      }
      "--help" | "-h" => {
        println!("{USAGE}");
        std::process::exit(0);
      }
      _ => {
        let value = rest.next()?;
        match flag.as_str() {
          "--root" => args.root = PathBuf::from(value),
          "--config" => args.config = PathBuf::from(value),
          "--format" => args.json = match value.as_str() {
            "json" => true,
            "text" => false,
            _ => return None,
          },
          "--tsc" => args.options.tsc = Some(PathBuf::from(value)),
          "--tsc-version" => args.options.version = value.clone(),
          "--cache" => args.options.cache = Some(PathBuf::from(value)),
          "--registry" => args.options.registry = value.clone(),
          "--expect" => args.options.expect = Some(value.clone()),
          _ => return None,
        }
      }
    }
  }
  Some(args)
}

fn report(resolved: &Resolved, diagnostics: &[Diagnostic], json: bool) {
  if json {
    let value = serde_json::json!({
      "tsc": resolved.tsc,
      "version": resolved.version,
      "source": resolved.source,
      "sha512": resolved.sha512,
      "pinned": platform().map(|p| is_pinned(&resolved.version, &p)).unwrap_or(false),
      "diagnostics": diagnostics,
    });
    println!("{}", serde_json::to_string(&value).expect("a report serializes"));
    return;
  }
  for diagnostic in diagnostics {
    println!("{diagnostic}");
  }
  eprintln!("tsc {} from {}", resolved.version, resolved.source);
}

fn main() -> ExitCode {
  let Some(args) = parse() else { return usage() };
  if args.options.tsc.is_none() && !args.options.offline && !matches!(cached(args.options.cache.as_deref(), &args.options.version), Ok(Some(_))) && !args.json {
    eprintln!("tsc {} is not in the cache; taking it from PATH when it reports that version, else fetching it", args.options.version);
  }
  let resolved = match resolve(&args.options) {
    Ok(resolved) => resolved,
    Err(e) => {
      eprintln!("{e}");
      return ExitCode::from(2);
    }
  };
  if args.which {
    report(&resolved, &[], args.json);
    return ExitCode::SUCCESS;
  }
  if !args.root.is_dir() {
    eprintln!("{}: not a directory", args.root.display());
    return ExitCode::from(2);
  }
  if !config_path(&args.root, &args.config).is_file() {
    eprintln!("{}: no such tsconfig", config_path(&args.root, &args.config).display());
    return ExitCode::from(2);
  }
  match check(&resolved.tsc, &args.root, &args.config) {
    Ok(diagnostics) => {
      let failed = diagnostics.iter().any(Diagnostic::is_error);
      report(&resolved, &diagnostics, args.json);
      if failed { ExitCode::from(1) } else { ExitCode::SUCCESS }
    }
    Err(e) => {
      eprintln!("{e}");
      ExitCode::from(2)
    }
  }
}

fn config_path(root: &Path, config: &Path) -> PathBuf {
  if config.is_absolute() { config.to_path_buf() } else { root.join(config) }
}
