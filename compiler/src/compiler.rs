use crate::config::{MapMode, MapOptions};
use crate::transforms::{Import, ImportRewriter, StripConsole};
use anyhow::{Context, Result, anyhow};
use lightningcss::rules::CssRule;
use lightningcss::stylesheet::{MinifyOptions, ParserFlags, ParserOptions, PrinterOptions, StyleSheet};
use lightningcss::targets::{Browsers, Targets};
use std::cell::RefCell;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use swc_core::common::source_map::SourceMapGenConfig;
use swc_core::common::{FileName, GLOBALS, Globals, Mark, SourceMap, Spanned, sync::Lrc};
use swc_core::ecma::{
  ast::{Decl, EsVersion, ExportSpecifier, ModuleDecl, ModuleItem, ObjectPatProp, Pat, Program},
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

/// Whether the angle-bracket forms in a file are markup or type syntax.
///
/// The extension decides, as it does for `tsc`: in a `.ts` file `<T>(x: T) => x`
/// is a generic arrow, and parsing it as TSX reads the `<T>` as an unclosed tag.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Markup {
  Allowed,
  Denied,
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
  /// Names this module exports under its own roof.
  pub exports: Vec<String>,
  /// Specifiers of `export * from`, which contribute whatever the target exports.
  pub star_sources: Vec<String>,
  /// Set by an `export *` from a bare specifier, whose contribution only the page
  /// that supplies it can know.
  pub open_exports: bool,
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
      exports: Vec::new(),
      star_sources: Vec::new(),
      open_exports: false,
    }
  }
}

/// What a module offers importers: the names it declares, and the modules an
/// `export *` defers to.
#[derive(Default)]
struct Surface {
  names: Vec<String>,
  stars: Vec<String>,
  open: bool,
}

fn exported(program: &Program) -> Surface {
  let mut surface = Surface::default();

  let Some(module) = program.as_module() else {
    return surface;
  };

  for item in &module.body {
    let ModuleItem::ModuleDecl(declaration) = item else {
      continue;
    };

    match declaration {
      ModuleDecl::ExportDecl(export) => bound(&export.decl, &mut surface.names),
      ModuleDecl::ExportDefaultDecl(_) | ModuleDecl::ExportDefaultExpr(_) => {
        surface.names.push("default".to_string());
      }
      ModuleDecl::ExportNamed(export) => {
        for specifier in &export.specifiers {
          match specifier {
            ExportSpecifier::Named(named) => surface
              .names
              .push(crate::transforms::export_name(named.exported.as_ref().unwrap_or(&named.orig))),
            ExportSpecifier::Namespace(namespace) => {
              surface.names.push(crate::transforms::export_name(&namespace.name))
            }
            ExportSpecifier::Default(default) => surface.names.push(default.exported.sym.to_string()),
          }
        }
      }
      ModuleDecl::ExportAll(export) => {
        let source = export.src.value.to_string_lossy().into_owned();

        if source.starts_with('.') {
          surface.stars.push(source);
        } else {
          surface.open = true;
        }
      }
      _ => {}
    }
  }

  surface
}

/// Every name a declaration binds, so `export const { a, b } = x` counts as two.
fn bound(declaration: &Decl, into: &mut Vec<String>) {
  match declaration {
    Decl::Class(class) => into.push(class.ident.sym.to_string()),
    Decl::Fn(function) => into.push(function.ident.sym.to_string()),
    Decl::Var(var) => {
      for declarator in &var.decls {
        pattern(&declarator.name, into);
      }
    }
    _ => {}
  }
}

fn pattern(pat: &Pat, into: &mut Vec<String>) {
  match pat {
    Pat::Ident(ident) => into.push(ident.id.sym.to_string()),
    Pat::Array(array) => array.elems.iter().flatten().for_each(|element| pattern(element, into)),
    Pat::Object(object) => {
      for property in &object.props {
        match property {
          ObjectPatProp::KeyValue(entry) => pattern(&entry.value, into),
          ObjectPatProp::Assign(entry) => into.push(entry.key.sym.to_string()),
          ObjectPatProp::Rest(rest) => pattern(&rest.arg, into),
        }
      }
    }
    Pat::Assign(assign) => pattern(&assign.left, into),
    Pat::Rest(rest) => pattern(&rest.arg, into),
    _ => {}
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
    markup: Markup,
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
      let markup = markup == Markup::Allowed;

      let syntax = if is_ts {
        Syntax::Typescript(TsSyntax {
          tsx: markup,
          decorators: true,
          ..Default::default()
        })
      } else {
        Syntax::Es(EsSyntax {
          jsx: markup,
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

      // Read before compression, which is free to rename anything that is not
      // part of the module's public surface.
      let surface = exported(&program);

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
        exports: surface.names,
        star_sources: surface.stars,
        open_exports: surface.open,
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

      point_imports_at_minified(&mut stylesheet.rules.0);
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
      exports: Vec::new(),
      star_sources: Vec::new(),
      open_exports: false,
    })
  }
}

/// Points a minified stylesheet's `@import`s at the minified siblings.
///
/// The JavaScript graph gets this from `transforms::minified_name` as it folds
/// the module, but a stylesheet never reaches that fold: LightningCSS parses and
/// prints it whole. Without this the minified entry imports the unminified
/// files, so loading `x.min.css` pulls the full graph and the two builds are not
/// separable, which is the same defect `minified_name` exists to prevent for
/// modules.
///
/// Only relative targets are moved. A bare specifier is an external the consumer
/// supplies and has no minified twin here, and an absolute URL is fetched by the
/// browser unaided.
fn point_imports_at_minified(rules: &mut [CssRule<'_>]) {
  for rule in rules {
    let CssRule::Import(import) = rule else {
      continue;
    };

    if let Some(minified) = minified_import(import.url.as_ref()) {
      import.url = minified.into();
    }
  }
}

fn minified_import(specifier: &str) -> Option<String> {
  let relative = specifier.starts_with("./") || specifier.starts_with("../") || specifier.starts_with('/');

  if !relative || specifier.starts_with("//") {
    return None;
  }

  let stem = specifier.strip_suffix(".css")?;

  if stem.ends_with(".min") {
    return None;
  }

  Some(format!("{stem}.min.css"))
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
