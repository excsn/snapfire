use std::path::PathBuf;
use std::process::ExitCode;

use snapfire_fsr_cli::dev::DevOptions;
use snapfire_fsr_cli::new::NewOptions;
use snapfire_fsr_cli::serve::ServeOptions;
use snapfire_fsr_cli::typecheck::{self, Typecheck};
use snapfire_fsr_cli::vendor::Spec;
use snapfire_fsr_cli::{build, dev, emit, new, serve, sites, test, types, vendor, Options};

const USAGE: &str = "usage: fsr new   <project dir> [--no-fetch]\n       fsr dev   <app dir> [--shell <module id>] [--slot <name>] [--public-path <prefix>] [--snapfirec <path>] [--typecheck flags]\n       fsr test  <app dir> [<name filter>]\n       fsr serve <app dir> [--listen <addr>]\n       fsr prerender <app dir> [--out <dir>]\n       fsr build <app dir> [--shell <module id>] [--slot <name>] [--public-path <prefix>] [--snapfirec <path>] [--typecheck flags]\n       fsr check <app dir> [--shell <module id>] [--slot <name>] [--typecheck flags]\n       fsr add   <app dir> <name@version[/subpath]>... [--external <name,...>]\n       fsr types <app dir> [--refresh]\n       fsr sites list   <shell dir>\n       fsr sites link   <shell dir> <site dir> --at <path> [--name <name>]\n       fsr sites unlink <shell dir> <name> [--keep-site]\n\ntypecheck flags: [--no-typecheck] [--tsc <path>] [--tsc-version <version>] [--snapfiretc <path>]";

fn usage() -> ExitCode {
  eprintln!("{USAGE}");
  ExitCode::from(2)
}

/// The typecheck rows of a report, and the exit code the diagnostics call for.
fn types_row(checked: Option<&typecheck::Checked>, enabled: bool) -> ExitCode {
  let Some(checked) = checked else {
    if enabled {
      eprintln!("note      types are not checked: no `{}` beside fsr or on PATH", typecheck::CHECKER);
    }
    return ExitCode::SUCCESS;
  };
  for diagnostic in &checked.diagnostics {
    println!("{diagnostic}");
  }
  println!("typecheck {}", checked.row());
  if let Some(path) = &checked.recorded {
    println!("recorded  typecheck.version = \"{}\" in {}", checked.version, path.display());
  }
  if checked.errors() > 0 { ExitCode::from(1) } else { ExitCode::SUCCESS }
}

fn main() -> ExitCode {
  let args: Vec<String> = std::env::args().skip(1).collect();
  let (Some(command), Some(app)) = (args.first(), args.get(1).map(PathBuf::from)) else {
    return usage();
  };
  let rest = &args[2..];

  match command.as_str() {
    "sites" => sites_command(&args[1..]),
    "new" => {
      let mut options = NewOptions::default();
      for flag in rest {
        match flag.as_str() {
          "--no-fetch" => options.fetch = false,
          _ => return usage(),
        }
      }
      match new::create(&app, options) {
        Ok(created) => {
          for path in &created.written {
            println!("wrote     {}", path.display());
          }
          for (specifier, file, bytes) in &created.vendored {
            println!("added     {specifier:<28} {file}  {bytes} bytes");
          }
          for (package, version, from) in &created.typed {
            println!("types     {package:<28} {from} {version}");
          }
          for note in &created.notes {
            eprintln!("note      {note}");
          }
          for step in &created.next {
            println!("next      {step}");
          }
          ExitCode::SUCCESS
        }
        Err(e) => {
          eprintln!("{e}");
          ExitCode::from(1)
        }
      }
    }
    "test" => {
      let filter = match rest {
        [] => None,
        [one] => Some(one.as_str()),
        _ => return usage(),
      };
      match test::run(&app, &Options::beside(&app), filter) {
        Ok(summary) => {
          print!("{summary}");
          if summary.failed == 0 { ExitCode::SUCCESS } else { ExitCode::from(1) }
        }
        Err(e) => {
          eprintln!("{e}");
          ExitCode::from(1)
        }
      }
    }
    "serve" => {
      let mut options = ServeOptions::default();
      let mut rest = rest.iter();
      while let Some(flag) = rest.next() {
        match (flag.as_str(), rest.next()) {
          ("--listen", Some(value)) => options.listen = Some(value.clone()),
          _ => return usage(),
        }
      }
      match serve::run(&app, options) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
          eprintln!("{e}");
          ExitCode::from(1)
        }
      }
    }
    "prerender" => {
      let mut out: Option<PathBuf> = None;
      let mut rest = rest.iter();
      while let Some(flag) = rest.next() {
        match (flag.as_str(), rest.next()) {
          ("--out", Some(value)) => out = Some(PathBuf::from(value)),
          _ => return usage(),
        }
      }
      match serve::prerender(&app, out.as_deref()) {
        Ok(written) => {
          if written.is_empty() {
            println!("nothing to prerender: every route reads the request");
          }
          for (pattern, file) in written {
            println!("{pattern:<22} {}", file.display());
          }
          ExitCode::SUCCESS
        }
        Err(e) => {
          eprintln!("{e}");
          ExitCode::from(1)
        }
      }
    }
    "dev" => {
      let mut options = DevOptions::beside(&app);
      let mut rest = rest.iter();
      while let Some(flag) = rest.next() {
        if flag == "--no-typecheck" {
          options.typecheck.enabled = false;
          continue;
        }
        match (flag.as_str(), rest.next()) {
          ("--shell", Some(value)) => options.build.shell = value.clone(),
          ("--slot", Some(value)) => options.build.slot = value.clone(),
          ("--public-path", Some(value)) => options.public_path = value.clone(),
          ("--snapfirec", Some(value)) => options.snapfirec = Some(PathBuf::from(value)),
          ("--tsc", Some(value)) => options.typecheck.tsc = Some(PathBuf::from(value)),
          ("--tsc-version", Some(value)) => options.typecheck.version = Some(value.clone()),
          ("--snapfiretc", Some(value)) => options.typecheck.checker = Some(PathBuf::from(value)),
          _ => return usage(),
        }
      }
      match dev::run(&app, options) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
          eprintln!("{e}");
          ExitCode::from(1)
        }
      }
    }
    "check" => {
      let mut options = Options::beside(&app);
      let mut typecheck = Typecheck::beside(&app);
      let mut rest = rest.iter();
      while let Some(flag) = rest.next() {
        if flag == "--no-typecheck" {
          typecheck.enabled = false;
          continue;
        }
        match (flag.as_str(), rest.next()) {
          ("--shell", Some(value)) => options.shell = value.clone(),
          ("--slot", Some(value)) => options.slot = value.clone(),
          ("--tsc", Some(value)) => typecheck.tsc = Some(PathBuf::from(value)),
          ("--tsc-version", Some(value)) => typecheck.version = Some(value.clone()),
          ("--snapfiretc", Some(value)) => typecheck.checker = Some(PathBuf::from(value)),
          _ => return usage(),
        }
      }
      match build(&app, &options) {
        Ok(built) => {
          print!("{}", built.report);
          match typecheck::run(&app, &typecheck) {
            Ok(checked) => types_row(checked.as_ref(), typecheck.enabled),
            Err(e) => {
              eprintln!("{e}");
              return ExitCode::from(1);
            }
          }
        }
        Err(e) => {
          eprintln!("{e}");
          ExitCode::from(1)
        }
      }
    }
    "build" => {
      let mut options = DevOptions::beside(&app);
      let mut rest = rest.iter();
      while let Some(flag) = rest.next() {
        if flag == "--no-typecheck" {
          options.typecheck.enabled = false;
          continue;
        }
        match (flag.as_str(), rest.next()) {
          ("--shell", Some(value)) => options.build.shell = value.clone(),
          ("--slot", Some(value)) => options.build.slot = value.clone(),
          ("--public-path", Some(value)) => options.public_path = value.clone(),
          ("--snapfirec", Some(value)) => options.snapfirec = Some(PathBuf::from(value)),
          ("--tsc", Some(value)) => options.typecheck.tsc = Some(PathBuf::from(value)),
          ("--tsc-version", Some(value)) => options.typecheck.version = Some(value.clone()),
          ("--snapfiretc", Some(value)) => options.typecheck.checker = Some(PathBuf::from(value)),
          _ => return usage(),
        }
      }
      let checking = options.typecheck.enabled;
      match emit(&app, options) {
        Ok(emitted) => {
          print!("{}", emitted.built.report);
          for path in emitted.written {
            println!("wrote {}", path.display());
          }
          types_row(emitted.checked.as_ref(), checking)
        }
        Err(e) => {
          eprintln!("{e}");
          ExitCode::from(1)
        }
      }
    }
    "add" => {
      let mut specs = Vec::new();
      let mut externals = Vec::new();
      let mut rest = rest.iter();
      while let Some(arg) = rest.next() {
        if arg == "--external" {
          let Some(value) = rest.next() else { return usage() };
          externals.extend(value.split(',').map(|s| s.trim().to_owned()).filter(|s| !s.is_empty()));
          continue;
        }
        match Spec::parse(arg) {
          Ok(spec) => specs.push(spec),
          Err(e) => {
            eprintln!("{e}");
            return ExitCode::from(2);
          }
        }
      }
      if specs.is_empty() {
        return usage();
      }
      match vendor::add(&app, &specs, &externals) {
        Ok(report) => {
          if !externals.is_empty() && !report.delegated.is_empty() {
            eprintln!("note: xwpm carries dependencies itself; --external was not used");
          }
          for spec in &report.delegated {
            println!("xwpm add  {spec}");
          }
          for (specifier, file, bytes) in report.added {
            println!("added     {specifier:<28} {file}  {bytes} bytes");
          }
          ExitCode::SUCCESS
        }
        Err(e) => {
          eprintln!("{e}");
          ExitCode::from(1)
        }
      }
    }
    "types" => {
      let refresh = match rest {
        [] => false,
        [flag] if flag == "--refresh" => true,
        _ => return usage(),
      };
      match types::fetch(&app, refresh) {
        Ok(report) => {
          for command in &report.delegated {
            println!("ran       {command}");
          }
          for (package, version, from) in &report.fetched {
            println!("types     {package:<28} {from} {version}");
          }
          for package in &report.kept {
            println!("kept      {package}");
          }
          for (package, why) in &report.missing {
            println!("missing   {package:<28} {why}");
          }
          ExitCode::SUCCESS
        }
        Err(e) => {
          eprintln!("{e}");
          ExitCode::from(1)
        }
      }
    }
    _ => usage(),
  }
}

/// `fsr sites <subcommand>`: the one command group whose second argument names
/// a subcommand rather than a directory.
fn sites_command(args: &[String]) -> ExitCode {
  let Some(sub) = args.first() else { return usage() };
  match sub.as_str() {
    "list" => {
      let [_, shell] = args else { return usage() };
      match sites::list(&PathBuf::from(shell)) {
        Ok(rows) => {
          if rows.is_empty() {
            println!("no sites mounted");
          }
          for row in &rows {
            let at = row.at.as_deref().unwrap_or("-");
            println!("site      {:<20} {:<24} {:<8} {}", row.name, at, row.version, row.hash);
            println!("          {}", row.artifact);
            if let Some(note) = &row.note {
              println!("          {note}");
            }
          }
          ExitCode::SUCCESS
        }
        Err(e) => {
          eprintln!("{e}");
          ExitCode::from(1)
        }
      }
    }
    "link" => {
      let (Some(shell), Some(site)) = (args.get(1), args.get(2)) else { return usage() };
      let mut at = None;
      let mut name = None;
      let mut rest = args[3..].iter();
      while let Some(flag) = rest.next() {
        match flag.as_str() {
          "--at" => at = rest.next().cloned(),
          "--name" => name = rest.next().cloned(),
          _ => return usage(),
        }
      }
      let Some(at) = at else {
        eprintln!("fsr sites link needs --at <path>, the prefix the shell mounts the site under");
        return ExitCode::from(2);
      };
      match sites::link(&PathBuf::from(shell), &PathBuf::from(site), &at, name.as_deref()) {
        Ok(linked) => {
          if linked.site_kept {
            println!("kept      [site] {} at {} in {}", linked.name, linked.at, linked.site_config.display());
          } else {
            println!("wrote     [site] {} at {} in {}", linked.name, linked.at, linked.site_config.display());
            println!("          shell = {}", linked.shell_json);
          }
          println!("wrote     [sites.{}] in {}", linked.name, linked.shell_config.display());
          println!("          artifact = {}", linked.artifact);
          for command in &linked.next {
            println!("next      {command}");
          }
          ExitCode::SUCCESS
        }
        Err(e) => {
          eprintln!("{e}");
          ExitCode::from(1)
        }
      }
    }
    "unlink" => {
      let (Some(shell), Some(name)) = (args.get(1), args.get(2)) else { return usage() };
      let mut keep_site = false;
      for flag in &args[3..] {
        match flag.as_str() {
          "--keep-site" => keep_site = true,
          _ => return usage(),
        }
      }
      match sites::unlink(&PathBuf::from(shell), name, keep_site) {
        Ok(unlinked) => {
          println!("removed   [sites.{}] from {}", unlinked.name, unlinked.shell_config.display());
          match &unlinked.site_config {
            Some(path) => println!("removed   [site] from {}", path.display()),
            None => println!("kept      the site's own [site]"),
          }
          ExitCode::SUCCESS
        }
        Err(e) => {
          eprintln!("{e}");
          ExitCode::from(1)
        }
      }
    }
    _ => usage(),
  }
}
