use std::path::{Path, PathBuf};
use std::process::ExitCode;

use snapfire_fsr_cli::dev::DevOptions;
use snapfire_fsr_cli::doctor;
use snapfire_fsr_cli::new::{NewOptions, SiteScaffold};
use snapfire_fsr_cli::serve::ServeOptions;
use snapfire_fsr_cli::bundle;
use snapfire_fsr_cli::typecheck::{self, Typecheck};
use snapfire_fsr_cli::vendor::Spec;
use snapfire_fsr_cli::{build, dev, emit, new, serve, sites, test, types, vendor, Options};

const USAGE: &str = "usage: fsr new   <project dir> [--no-fetch] [--shell | --site --at <path> [--name <name>] [--into <shell dir>]]\n       fsr dev   <app dir> [--shell <module id>] [--slot <name>] [--public-path <prefix>] [--snapfirec <path>] [--typecheck flags]\n       fsr test  <app dir> [<name filter>]\n       fsr serve <app dir> [--listen <addr>]\n       fsr prerender <app dir> [--out <dir>]\n       fsr bundle <app dir> [--out <dir>]\n       fsr build <app dir> [--shell <module id>] [--slot <name>] [--public-path <prefix>] [--snapfirec <path>] [--typecheck flags]\n       fsr doctor <app dir>\n       fsr check <app dir> [--shell <module id>] [--slot <name>] [--typecheck flags]\n       fsr add   <app dir> <name@version[/subpath]>... [--external <name,...>]\n       fsr types <app dir> [--refresh]\n       fsr sites list   <shell dir>\n       fsr sites hash   <site dir> [--files]\n       fsr sites pack   <site dir> --version <version> [-o <file>]\n       fsr sites install <shell dir> <archive> [--as <name>] [--keep <n>]\n       fsr sites link   <shell dir> <site dir> --at <path> [--name <name>]\n       fsr sites unlink <shell dir> <name> [--keep-site]\n\ntypecheck flags: [--no-typecheck] [--tsc <path>] [--tsc-version <version>] [--snapfiretc <path>]";

fn usage() -> ExitCode {
  eprintln!("{USAGE}");
  ExitCode::from(2)
}

/// The typecheck rows of a report, and the exit code the diagnostics call for.
fn types_row(checked: Option<&typecheck::Checked>) -> ExitCode {
  let Some(checked) = checked else { return ExitCode::SUCCESS };
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
      let (mut site, mut at, mut name, mut into) = (false, None, None, None);
      let mut flags = rest.iter();
      while let Some(flag) = flags.next() {
        match flag.as_str() {
          "--no-fetch" => options.fetch = false,
          "--shell" => options.shell = true,
          "--site" => site = true,
          "--at" => at = flags.next().cloned(),
          "--name" => name = flags.next().cloned(),
          "--into" => into = flags.next().map(PathBuf::from),
          _ => return usage(),
        }
      }
      if site {
        let Some(at) = at else {
          eprintln!("fsr new --site needs --at <path>, the prefix a shell mounts it under");
          return ExitCode::from(2);
        };
        options.site = Some(SiteScaffold { at, name, into });
      } else if at.is_some() || name.is_some() || into.is_some() {
        eprintln!("--at, --name and --into belong to --site");
        return ExitCode::from(2);
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
          if let Some(linked) = &created.linked {
            println!("wrote     [site] {} at {} in {}", linked.name, linked.at, linked.site_config.display());
            println!("wrote     [sites.{}] in {}", linked.name, linked.shell_config.display());
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
    "bundle" => {
      let mut out: Option<PathBuf> = None;
      let mut rest = rest.iter();
      while let Some(flag) = rest.next() {
        match (flag.as_str(), rest.next()) {
          ("--out", Some(value)) => out = Some(PathBuf::from(value)),
          _ => return usage(),
        }
      }
      let out = out.unwrap_or_else(|| snapfire_fsr_cli::serve::project_root(&app).join("dist"));
      match snapfire_fsr_cli::bundle::run(&app, &out) {
        Ok(bundled) => {
          for (route, from) in &bundled.served {
            println!("{:<24} {}", format!("{}/{}", bundle::SERVE, route.trim_start_matches('/')), from.display());
          }
          for path in &bundled.read {
            println!("{:<24} read by the host", path.strip_prefix(&bundled.out).unwrap_or(path).display());
          }
          println!("\nplace beside it: {}", bundled.beside.join(", "));
          ExitCode::SUCCESS
        }
        Err(e) => {
          eprintln!("{e}");
          ExitCode::from(1)
        }
      }
    }
    "doctor" => {
      if !rest.is_empty() {
        return usage();
      }
      match doctor::run(&app) {
        Ok(report) => {
          print!("{report}");
          if report.is_clean() { ExitCode::SUCCESS } else { ExitCode::from(1) }
        }
        Err(e) => {
          eprintln!("{e}");
          ExitCode::from(2)
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
            println!("nothing to prerender or warm: every source reads the request");
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
            Ok(checked) => types_row(checked.as_ref()),
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
      match emit(&app, options) {
        Ok(emitted) => {
          print!("{}", emitted.built.report);
          for path in emitted.written {
            println!("wrote {}", path.display());
          }
          types_row(emitted.checked.as_ref())
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
          match sites::cached(&PathBuf::from(shell)) {
            Ok(cached) => {
              for (name, versions) in &cached {
                println!("cached    {:<20} {}", name, versions.join(" "));
              }
            }
            Err(e) => eprintln!("{e}"),
          }
          ExitCode::SUCCESS
        }
        Err(e) => {
          eprintln!("{e}");
          ExitCode::from(1)
        }
      }
    }
    "hash" => {
      let Some(site) = args.get(1) else { return usage() };
      let mut files = false;
      for flag in &args[2..] {
        match flag.as_str() {
          "--files" => files = true,
          _ => return usage(),
        }
      }
      match sites::hash(&PathBuf::from(site)) {
        Ok(hashed) => {
          println!("site      {} at {}", hashed.name, hashed.at);
          println!("hash      {}", hashed.hash);
          println!("ships     {} files, {}", hashed.files.len(), bytes(hashed.bytes));
          for part in &hashed.parts {
            println!("          {part}");
          }
          if files {
            for file in &hashed.files {
              println!("file      {:<12} {:<64} {}", file.size, file.sha256, file.path);
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
    "pack" => {
      let Some(site) = args.get(1) else { return usage() };
      let mut version = None;
      let mut out = None;
      let mut rest = args[2..].iter();
      while let Some(flag) = rest.next() {
        match flag.as_str() {
          "--version" => version = rest.next().cloned(),
          "-o" | "--out" => out = rest.next().cloned(),
          _ => return usage(),
        }
      }
      let Some(version) = version else {
        eprintln!("fsr sites pack needs --version <version>, the release this artifact is");
        return ExitCode::from(2);
      };
      match sites::pack(&PathBuf::from(site), &version, out.as_deref().map(Path::new)) {
        Ok(packed) => {
          println!("packed    {} {}", packed.manifest.name, packed.manifest.version);
          println!("hash      {}", packed.manifest.hash);
          println!("wrote     {} ({} of {} in {} files)", packed.out.display(), bytes(packed.bytes), bytes(packed.unpacked), packed.manifest.files.len());
          ExitCode::SUCCESS
        }
        Err(e) => {
          eprintln!("{e}");
          ExitCode::from(1)
        }
      }
    }
    "install" => {
      let (Some(shell), Some(archive)) = (args.get(1), args.get(2)) else { return usage() };
      let mut name = None;
      let mut keep = None;
      let mut rest = args[3..].iter();
      while let Some(flag) = rest.next() {
        match flag.as_str() {
          "--as" => name = rest.next().cloned(),
          "--keep" => keep = rest.next().and_then(|n| n.parse::<usize>().ok()),
          _ => return usage(),
        }
      }
      match sites::install(&PathBuf::from(shell), Path::new(archive), name.as_deref(), keep) {
        Ok(installed) => {
          if installed.held {
            println!("held      {} {} already at {}", installed.name, installed.version, installed.path.display());
          } else {
            println!("installed {} {}", installed.name, installed.version);
            println!("hash      {}", installed.hash);
            println!("at        {}", installed.path.display());
          }
          for version in &installed.swept {
            println!("removed   {} {version}", installed.name);
          }
          println!("next      artifact = \"{}@{}\" in [sites.{}]", installed.name, installed.version, installed.name);
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

/// A byte count for a report line, three significant figures and a unit.
fn bytes(count: u64) -> String {
  const UNITS: &[&str] = &["B", "KiB", "MiB", "GiB"];
  let mut size = count as f64;
  let mut unit = 0;
  while size >= 1024.0 && unit + 1 < UNITS.len() {
    size /= 1024.0;
    unit += 1;
  }
  if unit == 0 {
    format!("{count} B")
  } else {
    format!("{size:.1} {}", UNITS[unit])
  }
}
