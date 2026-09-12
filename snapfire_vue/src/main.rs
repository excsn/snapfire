fn main() -> std::process::ExitCode {
  let compiler = match snapfire_vue::Compiler::new() {
    Ok(compiler) => compiler,
    Err(e) => {
      eprintln!("{e}");
      return std::process::ExitCode::FAILURE;
    }
  };
  match compiler.version() {
    Ok(version) => {
      println!("snapfirec-vue {} (@vue/compiler-sfc {version})", env!("CARGO_PKG_VERSION"));
      std::process::ExitCode::SUCCESS
    }
    Err(e) => {
      eprintln!("{e}");
      std::process::ExitCode::FAILURE
    }
  }
}
