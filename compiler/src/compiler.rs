use crate::config::{MapMode, MapOptions};
use crate::transforms::{Import, ImportRewriter, StripConsole};
use anyhow::{Context, Result, anyhow};
use lightningcss::stylesheet::{MinifyOptions, ParserFlags, ParserOptions, PrinterOptions, StyleSheet};
use lightningcss::targets::{Browsers, Targets};
use std::cell::RefCell;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use swc_core::common::source_map::SourceMapGenConfig;
use swc_core::common::{FileName, GLOBALS, Globals, Mark, SourceMap, Spanned, sync::Lrc};
use swc_core::ecma::{
  ast::{EsVersion, Program},
  codegen::{Config as CodegenConfig, Emitter, text_writer::JsWriter},
  parser::{EsSyntax, Parser, StringInput, Syntax, TsSyntax, error::Error as ParserError, lexer::Lexer},
  transforms::base::{fixer::fixer, hygiene::hygiene, resolver},
  transforms::typescript::strip,
  visit::fold_pass,
};

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Dialect {
  TypeScript,
  JavaScript,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, clap::ValueEnum)]
pub enum Minify {
  /// Strip whitespace at codegen. Identifiers survive, so stack traces stay readable.
  Compact,
  /// Mangle, inline and drop dead code. Requires the `minify` cargo feature.
  Full,
}

/// Whether the full minifier was compiled in, so a request for it can be refused up front rather
/// than silently downgraded to compaction.
pub const FULL_MINIFIER: bool = cfg!(feature = "minify");

/// A compiled file, its source map when one was asked for, and every relative specifier the source
/// named.
///
/// The caller decides which of those paths are assets it has to deliver; the rewriter only reports
/// what the emitted file points at.
pub struct Output {
  pub code: String,
  pub map: Option<String>,
  pub referenced: Vec<PathBuf>,
  pub externals: Vec<String>,
  pub imports: Vec<Import>,
}

impl Output {
  /// An emit that is only text: no map, and nothing for the caller to deliver or resolve.
  pub fn text(code: String) -> Self {
    Self {
      code,
      map: None,
      referenced: Vec::new(),
      externals: Vec::new(),
      imports: Vec::new(),
    }
  }
}

/// How a map should name the file it came from, relative to where the map itself will be written.
pub struct MapRequest<'a> {
  pub options: MapOptions,
  pub source_name: &'a str,
}

impl MapRequest<'_> {
  fn wanted(&self) -> bool {
    self.options.mode != MapMode::Off
  }
}

pub struct Compiler {
  targets: Option<Browsers>,
}

impl Compiler {
  pub fn new(targets: Option<Browsers>) -> Self {
    Self { targets }
  }

  pub fn compile_script(
    &self,
    path: &Path,
    dialect: Dialect,
    strip_log: bool,
    strip_debug: bool,
    minify: Option<Minify>,
    map: MapRequest,
  ) -> Result<Output> {
    let content = std::fs::read_to_string(path).with_context(|| format!("Failed to read file: {:?}", path))?;

    let cm: Lrc<SourceMap> = Default::default();
    let globals = Globals::new();
    let referenced: Rc<RefCell<Vec<PathBuf>>> = Default::default();
    let externals: Rc<RefCell<Vec<String>>> = Default::default();
    let imports: Rc<RefCell<Vec<Import>>> = Default::default();

    GLOBALS.set(&globals, || {
      let fm = cm.new_source_file(Lrc::new(FileName::Real(path.to_path_buf())), content);

      let is_ts = dialect == Dialect::TypeScript;
      let syntax = if is_ts {
        Syntax::Typescript(TsSyntax {
          tsx: true,
          decorators: true,
          ..Default::default()
        })
      } else {
        Syntax::Es(EsSyntax {
          jsx: true,
          decorators: true,
          ..Default::default()
        })
      };

      let lexer = Lexer::new(syntax, EsVersion::latest(), StringInput::from(&*fm), None);
      let mut parser = Parser::new_from(lexer);

      let parsed = parser.parse_module();
      let recovered = parser.take_errors();

      let module = match parsed {
        Ok(module) => module,
        Err(fatal) => {
          let mut reported = vec![describe(&cm, &fatal)];
          reported.extend(recovered.iter().map(|e| describe(&cm, e)));
          return Err(anyhow!("{}", reported.join("\n")));
        }
      };

      if !recovered.is_empty() {
        let reported: Vec<_> = recovered.iter().map(|e| describe(&cm, e)).collect();
        return Err(anyhow!("{}", reported.join("\n")));
      }

      let program = Program::Module(module);
      let unresolved_mark = Mark::new();
      let top_level_mark = Mark::new();

      let mut passes = (
        resolver(unresolved_mark, top_level_mark, is_ts),
        strip(unresolved_mark, top_level_mark),
        fold_pass(StripConsole { strip_log, strip_debug }),
        fold_pass(ImportRewriter::new(
          path,
          referenced.clone(),
          externals.clone(),
          imports.clone(),
          minify.is_some(),
        )),
        // `fixer` inserts the parentheses the grammar requires; without it the namespace and enum
        // forms `strip` emits are printed as invalid JavaScript.
        hygiene(),
        fixer(None),
      );
      let program = program.apply(&mut passes);
      let program = compress(program, minify, unresolved_mark, top_level_mark);

      let mut buf = vec![];
      let mut mappings = vec![];
      {
        let writer = if map.wanted() {
          JsWriter::new(cm.clone(), "\n", &mut buf, Some(&mut mappings))
        } else {
          JsWriter::new(cm.clone(), "\n", &mut buf, None)
        };

        let mut emitter = Emitter {
          cfg: CodegenConfig::default().with_minify(minify.is_some()),
          cm: cm.clone(),
          comments: None,
          wr: writer,
        };

        emitter.emit_program(&program).context("Failed to emit module")?;
      }

      let serialised = if map.wanted() {
        let built = cm.build_source_map(
          &mappings,
          None,
          NamedSource {
            source_name: map.source_name.to_string(),
            inline_sources: map.options.inline_sources,
          },
        );

        let mut json = vec![];
        built.to_writer(&mut json).context("Failed to serialise the source map")?;
        Some(String::from_utf8(json)?)
      } else {
        None
      };

      Ok(Output {
        code: String::from_utf8(buf)?,
        map: serialised,
        referenced: referenced.borrow().clone(),
        externals: externals.borrow().clone(),
        imports: imports.take(),
      })
    })
  }

  pub fn compile_css(&self, path: &Path, minify: bool, map: MapRequest) -> Result<Output> {
    let content = std::fs::read_to_string(path).with_context(|| format!("Failed to read file: {:?}", path))?;

    let parser_options = ParserOptions {
      filename: map.source_name.to_string(),
      flags: ParserFlags::NESTING,
      ..Default::default()
    };

    let mut stylesheet =
      StyleSheet::parse(&content, parser_options).map_err(|e| anyhow!("Failed to parse CSS: {}", e))?;

    let targets: Targets = self.targets.into();

    if minify {
      stylesheet
        .minify(MinifyOptions {
          targets,
          ..Default::default()
        })
        .map_err(|e| anyhow!("Failed to minify CSS: {}", e))?;
    }

    let mut built = parcel_sourcemap::SourceMap::new("/");
    built.add_source(map.source_name);

    if map.options.inline_sources {
      built
        .set_source_content(0, &content)
        .map_err(|e| anyhow!("Failed to embed CSS source: {}", e))?;
    }

    let res = stylesheet
      .to_css(PrinterOptions {
        minify,
        targets,
        source_map: map.wanted().then_some(&mut built),
        ..Default::default()
      })
      .map_err(|e| anyhow!("Failed to generate CSS: {}", e))?;

    let serialised = if map.wanted() {
      Some(
        built
          .to_json(None)
          .map_err(|e| anyhow!("Failed to serialise the source map: {}", e))?,
      )
    } else {
      None
    };

    Ok(Output {
      code: res.code,
      map: serialised,
      referenced: Vec::new(),
      externals: Vec::new(),
      imports: Vec::new(),
    })
  }
}

#[cfg(feature = "minify")]
fn compress(program: Program, minify: Option<Minify>, unresolved_mark: Mark, top_level_mark: Mark) -> Program {
  use swc_core::ecma::minifier::optimize;
  use swc_core::ecma::minifier::option::{ExtraOptions, MangleOptions, MinifyOptions as Compress};

  if minify != Some(Minify::Full) {
    return program;
  }

  optimize(
    program,
    Default::default(),
    None,
    None,
    &Compress {
      compress: Some(Default::default()),
      mangle: Some(MangleOptions::default()),
      ..Default::default()
    },
    &ExtraOptions {
      unresolved_mark,
      top_level_mark,
      mangle_name_cache: None,
    },
  )
}

#[cfg(not(feature = "minify"))]
fn compress(program: Program, _: Option<Minify>, _: Mark, _: Mark) -> Program {
  program
}

/// Names the one source a map covers, since the path swc knows is absolute and the map has to point
/// at something reachable from where it is written.
struct NamedSource {
  source_name: String,
  inline_sources: bool,
}

impl SourceMapGenConfig for NamedSource {
  fn file_name_to_source(&self, _: &FileName) -> String {
    self.source_name.clone()
  }

  fn inline_sources_content(&self, _: &FileName) -> bool {
    self.inline_sources
  }
}

fn describe(cm: &SourceMap, error: &ParserError) -> String {
  let loc = cm.lookup_char_pos(error.span().lo);
  format!(
    "{}:{}:{}: {}",
    loc.file.name,
    loc.line,
    loc.col_display + 1,
    error.kind().msg()
  )
}
