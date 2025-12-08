use crate::transforms::{ImportRewriter, StripConsole};
use anyhow::{Context, Result};
use lightningcss::stylesheet::{MinifyOptions, ParserFlags, ParserOptions, PrinterOptions, StyleSheet};
use lightningcss::targets::Browsers;
use std::path::Path;
use swc_core::common::{FileName, GLOBALS, Globals, Mark, SourceMap, sync::Lrc};
use swc_core::ecma::{
  ast::{EsVersion, Pass, Program},
  codegen::{Config as CodegenConfig, Emitter, text_writer::JsWriter},
  parser::{Parser, StringInput, Syntax, TsSyntax, lexer::Lexer},
  transforms::typescript::strip,
  visit::VisitMutWith,
  visit::fold_pass,
}; // Import our custom rewriter

pub struct Compiler {
  cm: Lrc<SourceMap>,
  globals: Globals,
  targets: Option<Browsers>,
}

impl Compiler {
  pub fn new(browserslist_query: &str) -> Self {
    Self {
      cm: Default::default(),
      globals: Globals::new(),
      targets: Browsers::from_browserslist([browserslist_query]).ok().flatten(),
    }
  }

  pub fn compile_ts(&self, path: &Path, strip_log: bool, strip_debug: bool) -> Result<String> {
    let content = std::fs::read_to_string(path).with_context(|| format!("Failed to read file: {:?}", path))?;

    GLOBALS.set(&self.globals, || {
      let fm = self
        .cm
        .new_source_file(Lrc::new(FileName::Real(path.to_path_buf())), content);

      let syntax = Syntax::Typescript(TsSyntax {
        tsx: true,
        decorators: true,
        ..Default::default()
      });

      let lexer = Lexer::new(syntax, EsVersion::latest(), StringInput::from(&*fm), None);

      let mut parser = Parser::new_from(lexer);

      let module = parser.parse_module().map_err(|e| {
        eprintln!("Parser Error: {:?}", e);
        anyhow::anyhow!("Parse failed")
      })?;

      let program = Program::Module(module);
      let unresolved_mark = Mark::new();
      let top_level_mark = Mark::new();

      // Chain all passes together. Order matters.
      // 1. Strip Types
      // 2. Strip Console
      // 3. Rewrite Imports
      let mut passes = (
        strip(unresolved_mark, top_level_mark),
        fold_pass(StripConsole { strip_log, strip_debug }),
        fold_pass(ImportRewriter),
      );
      let program = program.apply(&mut passes);

      let mut buf = vec![];
      {
        let mut emitter = Emitter {
          cfg: CodegenConfig::default(),
          cm: self.cm.clone(),
          comments: None,
          wr: JsWriter::new(self.cm.clone(), "\n", &mut buf, None),
        };

        emitter.emit_program(&program).context("Failed to emit module")?;
      }

      Ok(String::from_utf8(buf)?)
    })
  }

  pub fn compile_css(&self, path: &Path) -> Result<String> {
    let content = std::fs::read_to_string(path).with_context(|| format!("Failed to read file: {:?}", path))?;

    // Create ParserOptions and enable CSS Nesting.
    let flags = ParserFlags::NESTING;
    let parser_options = ParserOptions {
      flags,
      ..Default::default()
    };

    let stylesheet =
      StyleSheet::parse(&content, parser_options).map_err(|e| anyhow::anyhow!("Failed to parse CSS: {:?}", e))?;

    let res = stylesheet
      .to_css(PrinterOptions {
        minify: true,
        // --- THIS IS THE FIX ---
        // Convert the `Browsers` struct into the `Targets` enum.
        targets: self.targets.clone().into(),
        // -----------------------
        ..Default::default()
      })
      .map_err(|e| anyhow::anyhow!("Failed to generate CSS: {:?}", e))?;

    Ok(res.code)
  }
}
