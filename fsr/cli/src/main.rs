use std::path::PathBuf;
use std::process::ExitCode;

use snapfire_fsr_cli::{build, write, Options};

const USAGE: &str = "usage: fsr build <app dir> [--shell <module id>] [--slot <name>]\n       fsr check <app dir> [--shell <module id>] [--slot <name>]";

fn main() -> ExitCode {
  let args: Vec<String> = std::env::args().skip(1).collect();
  let Some(command) = args.first() else {
    eprintln!("{USAGE}");
    return ExitCode::from(2);
  };
  let Some(app) = args.get(1).map(PathBuf::from) else {
    eprintln!("{USAGE}");
    return ExitCode::from(2);
  };

  let mut options = Options::default();
  let mut rest = args[2..].iter();
  while let Some(flag) = rest.next() {
    match (flag.as_str(), rest.next()) {
      ("--shell", Some(value)) => options.shell = value.clone(),
      ("--slot", Some(value)) => options.slot = value.clone(),
      _ => {
        eprintln!("{USAGE}");
        return ExitCode::from(2);
      }
    }
  }

  let built = match build(&app, &options) {
    Ok(built) => built,
    Err(e) => {
      eprintln!("{e}");
      return ExitCode::from(1);
    }
  };

  match command.as_str() {
    "build" => match write(&app, &built) {
      Ok(paths) => {
        print!("{}", built.report);
        for path in paths {
          println!("wrote {}", path.display());
        }
        ExitCode::SUCCESS
      }
      Err(e) => {
        eprintln!("{e}");
        ExitCode::from(1)
      }
    },
    "check" => {
      print!("{}", built.report);
      ExitCode::SUCCESS
    }
    _ => {
      eprintln!("{USAGE}");
      ExitCode::from(2)
    }
  }
}
