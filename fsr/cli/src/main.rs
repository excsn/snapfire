use std::path::PathBuf;
use std::process::ExitCode;

use snapfire_fsr_cli::dev::DevOptions;
use snapfire_fsr_cli::vendor::Spec;
use snapfire_fsr_cli::{build, dev, test, types, vendor, write, Options};

const USAGE: &str = "usage: fsr dev   <app dir> [--shell <module id>] [--slot <name>] [--public-path <prefix>] [--snapfirec <path>]\n       fsr test  <app dir> [<name filter>]\n       fsr build <app dir> [--shell <module id>] [--slot <name>]\n       fsr check <app dir> [--shell <module id>] [--slot <name>]\n       fsr add   <app dir> <name@version[/subpath]>... [--external <name,...>]\n       fsr types <app dir> [--refresh]";

fn usage() -> ExitCode {
  eprintln!("{USAGE}");
  ExitCode::from(2)
}

fn main() -> ExitCode {
  let args: Vec<String> = std::env::args().skip(1).collect();
  let (Some(command), Some(app)) = (args.first(), args.get(1).map(PathBuf::from)) else {
    return usage();
  };
  let rest = &args[2..];

  match command.as_str() {
    "test" => {
      let filter = match rest {
        [] => None,
        [one] => Some(one.as_str()),
        _ => return usage(),
      };
      match test::run(&app, &Options::default(), filter) {
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
    "dev" => {
      let mut options = DevOptions::default();
      let mut rest = rest.iter();
      while let Some(flag) = rest.next() {
        match (flag.as_str(), rest.next()) {
          ("--shell", Some(value)) => options.build.shell = value.clone(),
          ("--slot", Some(value)) => options.build.slot = value.clone(),
          ("--public-path", Some(value)) => options.public_path = value.clone(),
          ("--snapfirec", Some(value)) => options.snapfirec = Some(PathBuf::from(value)),
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
    "build" | "check" => {
      let mut options = Options::default();
      let mut rest = rest.iter();
      while let Some(flag) = rest.next() {
        match (flag.as_str(), rest.next()) {
          ("--shell", Some(value)) => options.shell = value.clone(),
          ("--slot", Some(value)) => options.slot = value.clone(),
          _ => return usage(),
        }
      }
      let built = match build(&app, &options) {
        Ok(built) => built,
        Err(e) => {
          eprintln!("{e}");
          return ExitCode::from(1);
        }
      };
      print!("{}", built.report);
      if command == "check" {
        return ExitCode::SUCCESS;
      }
      match write(&app, &built) {
        Ok(paths) => {
          for path in paths {
            println!("wrote {}", path.display());
          }
          ExitCode::SUCCESS
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
