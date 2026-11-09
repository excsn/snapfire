use std::path::Path;

use anyhow::{Context, Result, anyhow};
use swc_core::common::comments::SingleThreadedComments;
use swc_core::common::{BytePos, FileName, GLOBALS, Globals, Mark, SourceMap, Span, sync::Lrc};
use swc_core::ecma::ast::{EsVersion, ModuleDecl, ModuleItem, Program, Str};
use swc_core::ecma::codegen::text_writer::{BindingStorage, JsWriter, ScopeKind, WriteJs};
use swc_core::ecma::codegen::{Config as CodegenConfig, Emitter};
use swc_core::ecma::parser::{Parser, StringInput, Syntax, TsSyntax, lexer::Lexer};
use swc_core::ecma::transforms::base::resolver;
use swc_core::typescript::fast_dts::{FastDts, FastDtsOptions};

use crate::config::Aliases;
use crate::transforms::{relative_specifier, resolve_specifier};

/// Emits the `.d.ts` for one TypeScript file.
///
/// Isolated declarations, so a file's exports must be annotated well enough to describe themselves
/// without the rest of the program. That is the same per-file bargain the rest of the compiler makes
/// and the reason this needs no `node_modules`, but it does mean an export whose type can only be
/// inferred across files is an error here rather than something to guess at.
pub fn declare(path: &Path, aliases: &Aliases, source_text: &Result<String>) -> Result<String> {
  let source_text = match source_text {
    Ok(text) => text.clone(),
    Err(e) => return Err(anyhow!("{e:#}")),
  };
  let cm: Lrc<SourceMap> = Default::default();
  GLOBALS.set(&Globals::new(), || {
    let fm = cm.new_source_file(Lrc::new(FileName::Real(path.to_path_buf())), source_text);
    let comments = SingleThreadedComments::default();
    let tsx = path.extension().is_some_and(|e| e == "tsx");
    let syntax = Syntax::Typescript(TsSyntax { tsx, decorators: true, ..Default::default() });
    let lexer = Lexer::new(syntax, EsVersion::latest(), StringInput::from(&*fm), Some(&comments));
    let mut parser = Parser::new_from(lexer);
    let parsed = parser.parse_module();
    let recovered = parser.take_errors();
    let module = match parsed {
      Ok(module) if recovered.is_empty() => module,
      Ok(_) => return Err(anyhow!("{}", recovered.iter().map(|e| crate::compiler::describe(&cm, e)).collect::<Vec<_>>().join("\n"))),
      Err(fatal) => return Err(anyhow!("{}", crate::compiler::describe(&cm, &fatal))),
    };

    let unresolved_mark = Mark::new();
    let top_level_mark = Mark::new();
    let mut program = Program::Module(module).apply(&mut resolver(unresolved_mark, top_level_mark, true));
    let internal_annotations = Some(FastDts::get_internal_annotations(&comments));
    let name = std::sync::Arc::new(FileName::Real(path.to_path_buf()));
    let issues = FastDts::new(name, unresolved_mark, FastDtsOptions { internal_annotations }).transform(&mut program);
    if !issues.is_empty() {
      let described: Vec<String> = issues
        .iter()
        .map(|issue| {
          let at = cm.lookup_char_pos(issue.range.span.lo);
          format!("{}:{}:{}: {}", path.display(), at.line, at.col_display + 1, issue.message)
        })
        .collect();
      return Err(anyhow!("{}", described.join("\n")));
    }

    let dir = path.parent().unwrap_or_else(|| Path::new("."));
    if let Program::Module(module) = &mut program {
      for item in &mut module.body {
        let Some(source) = module_source(item) else {
          continue;
        };
        let Some(written) = source.value.as_wtf8().as_str().map(str::to_owned) else {
          continue;
        };
        let relative = if written.starts_with('.') {
          written.clone()
        } else {
          match aliases.expand(&written) {
            Some(target) => relative_specifier(dir, &target),
            None => continue,
          }
        };
        let resolved = resolve_specifier(dir, &relative);
        if resolved == written {
          continue;
        }
        source.value = resolved.as_str().into();
        source.raw = None;
      }
    }

    let mut buf = vec![];
    {
      let mut emitter = Emitter { cfg: CodegenConfig::default(), cm: cm.clone(), comments: Some(&comments), wr: DocCommentLines::new(JsWriter::new(cm.clone(), "\n", &mut buf, None)) };
      emitter.emit_program(&program).context("Failed to emit declarations")?;
    }
    Ok(String::from_utf8(buf)?)
  })
}

/// The specifier a module item imports from, when it has one.
fn module_source(item: &mut ModuleItem) -> Option<&mut Str> {
  match item {
    ModuleItem::ModuleDecl(ModuleDecl::Import(import)) => Some(&mut import.src),
    ModuleItem::ModuleDecl(ModuleDecl::ExportAll(export)) => Some(&mut export.src),
    ModuleItem::ModuleDecl(ModuleDecl::ExportNamed(export)) => export.src.as_deref_mut(),
    _ => None,
  }
}

/// A writer that ends the line after a `/** … */` doc comment where swc's emitter writes a space, so the comment stands above the declaration it documents.
struct DocCommentLines<W> {
  inner: W,
  opened: bool,
  in_doc: bool,
  after_doc: bool,
}

impl<W> DocCommentLines<W> {
  fn new(inner: W) -> Self {
    Self { inner, opened: false, in_doc: false, after_doc: false }
  }
}

impl<W: WriteJs> WriteJs for DocCommentLines<W> {
  fn increase_indent(&mut self) -> std::io::Result<()> {
    self.after_doc = false;
    self.inner.increase_indent()
  }
  fn decrease_indent(&mut self) -> std::io::Result<()> {
    self.after_doc = false;
    self.inner.decrease_indent()
  }
  fn write_semi(&mut self, span: Option<Span>) -> std::io::Result<()> {
    self.after_doc = false;
    self.inner.write_semi(span)
  }
  fn write_space(&mut self) -> std::io::Result<()> {
    if std::mem::take(&mut self.after_doc) {
      return self.inner.write_line();
    }
    self.inner.write_space()
  }
  fn write_keyword(&mut self, span: Option<Span>, s: &'static str) -> std::io::Result<()> {
    self.after_doc = false;
    self.inner.write_keyword(span, s)
  }
  fn write_operator(&mut self, span: Option<Span>, s: &str) -> std::io::Result<()> {
    self.after_doc = false;
    self.inner.write_operator(span, s)
  }
  fn write_param(&mut self, s: &str) -> std::io::Result<()> {
    self.after_doc = false;
    self.inner.write_param(s)
  }
  fn write_property(&mut self, s: &str) -> std::io::Result<()> {
    self.after_doc = false;
    self.inner.write_property(s)
  }
  fn write_line(&mut self) -> std::io::Result<()> {
    self.after_doc = false;
    self.inner.write_line()
  }
  fn write_lit(&mut self, span: Span, s: &str) -> std::io::Result<()> {
    self.after_doc = false;
    self.inner.write_lit(span, s)
  }
  fn write_comment(&mut self, s: &str) -> std::io::Result<()> {
    self.after_doc = false;
    if s == "/*" {
      self.opened = true;
    } else if s == "*/" {
      self.after_doc = std::mem::take(&mut self.in_doc);
    } else if std::mem::take(&mut self.opened) {
      self.in_doc = s.starts_with('*');
    }
    self.inner.write_comment(s)
  }
  fn write_str_lit(&mut self, span: Span, s: &str) -> std::io::Result<()> {
    self.after_doc = false;
    self.inner.write_str_lit(span, s)
  }
  fn write_str(&mut self, s: &str) -> std::io::Result<()> {
    self.after_doc = false;
    self.inner.write_str(s)
  }
  fn write_symbol(&mut self, span: Span, s: &str) -> std::io::Result<()> {
    self.after_doc = false;
    self.inner.write_symbol(span, s)
  }
  fn write_punct(&mut self, span: Option<Span>, s: &'static str, commit_pending_semi: bool) -> std::io::Result<()> {
    self.after_doc = false;
    self.inner.write_punct(span, s, commit_pending_semi)
  }
  fn care_about_srcmap(&self) -> bool {
    self.inner.care_about_srcmap()
  }
  fn add_srcmap(&mut self, pos: BytePos) -> std::io::Result<()> {
    self.inner.add_srcmap(pos)
  }
  fn commit_pending_semi(&mut self) -> std::io::Result<()> {
    self.inner.commit_pending_semi()
  }
  fn can_ignore_invalid_unicodes(&mut self) -> bool {
    self.inner.can_ignore_invalid_unicodes()
  }
  fn has_scope_tracking(&self) -> bool {
    self.inner.has_scope_tracking()
  }
  fn start_scope(&mut self, name: Option<&str>, kind: ScopeKind, is_stack_frame: bool, is_hidden: bool, original_span: Option<Span>) -> std::io::Result<()> {
    self.inner.start_scope(name, kind, is_stack_frame, is_hidden, original_span)
  }
  fn end_scope(&mut self) -> std::io::Result<()> {
    self.inner.end_scope()
  }
  fn add_scope_variable(&mut self, name: &str, expression: Option<&str>, storage: BindingStorage) -> std::io::Result<()> {
    self.inner.add_scope_variable(name, expression, storage)
  }
}
