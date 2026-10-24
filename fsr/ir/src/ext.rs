//! Extensions: the named synchronous functions an `Expr::Ext` calls, the
//! standard library and whatever a host registers beside it. Each carries a
//! reach: `render` runs on both sides of a render and must agree byte for
//! byte with its browser half; `body` runs on the server only; `terminal` runs
//! in a terminal page only.

use std::collections::BTreeMap;
use std::sync::Arc;

use snapfire_fsr_core::Value;
pub use snapfire_fsr_core::ext::{number, option, text, text_opt, Ambient, Field, Reach, Signature, Ty};

use crate::interp::Fail;

pub type ExtFn = dyn Fn(&Ambient, &[Value]) -> Result<Value, Fail> + Send + Sync;

#[derive(Clone)]
#[non_exhaustive]
pub struct Extension {
  pub reach: Reach,
  f: Arc<ExtFn>,
}

impl Extension {
  pub fn call(&self, ambient: &Ambient, args: &[Value]) -> Result<Value, Fail> {
    (self.f)(ambient, args)
  }
}

/// A standard member: its module, its name, its reach and, for one with no TypeScript in the client to declare it, its signatures, which a terminal app's declarations are written from.
#[derive(Debug, Clone, Copy)]
pub struct Member {
  pub module: &'static str,
  pub name: &'static str,
  pub reach: Reach,
  pub signatures: &'static [Signature],
}

const fn member(module: &'static str, name: &'static str, reach: Reach) -> Member {
  Member { module, name, reach, signatures: &[] }
}

/// The standard library: the table the lowerer checks a call against and the registry fills.
pub const STANDARD: &[Member] = &[
  member("intl", "number", Reach::Render),
  member("intl", "currency", Reach::Render),
  member("intl", "date", Reach::Render),
  member("intl", "plural", Reach::Render),
  member("intl", "list", Reach::Render),
  member("intl", "compare", Reach::Render),
  member("text", "slug", Reach::Render),
  member("text", "truncate", Reach::Render),
  member("text", "frontMatter", Reach::Body),
  member("text", "withFrontMatter", Reach::Body),
  member("time", "format", Reach::Render),
  member("time", "add", Reach::Render),
  member("time", "diff", Reach::Render),
  member("time", "parse", Reach::Render),
  member("time", "now", Reach::Body),
  member("time", "monotonic", Reach::Body),
  member("crypto", "hash", Reach::Render),
  member("crypto", "verify", Reach::Render),
  member("crypto", "random", Reach::Body),
  member("crypto", "uuid", Reach::Body),
  member("crypto", "digest", Reach::Render),
  member("id", "new", Reach::Body),
  member("i18n", "t", Reach::Render),
  member("intl", "localTime", Reach::Body),
  member("time", "localOffset", Reach::Body),
  Member { module: "terminal", name: "ms", reach: Reach::Terminal, signatures: &[
    Signature { doc: "Milliseconds for a duration such as `\"2 days\"` or `\"1h\"`; `undefined` for one it cannot read.", generics: &[], params: &[Field::required("value", Ty::String)], returns: Ty::Union(&[Ty::Number, Ty::Undefined]) },
    Signature { doc: "A duration in milliseconds written short as `\"2d\"` or long as `\"2 days\"`.", generics: &[], params: &[Field::required("value", Ty::Number), Field::optional("options", Ty::Object(&[Field::optional("long", Ty::Boolean)]))], returns: Ty::String },
  ] },
  Member { module: "terminal", name: "stripAnsi", reach: Reach::Terminal, signatures: &[Signature { doc: "`text` with its ANSI escape codes removed.", generics: &[], params: &[Field::required("text", Ty::String)], returns: Ty::String }] },
  Member { module: "terminal", name: "stringWidth", reach: Reach::Terminal, signatures: &[Signature { doc: "The columns `text` takes in a terminal, its escape codes left out and wide characters counted twice.", generics: &[], params: &[Field::required("text", Ty::String)], returns: Ty::Number }] },
  Member { module: "terminal", name: "wrapAnsi", reach: Reach::Terminal, signatures: &[Signature {
    doc: "`text` wrapped to `columns`, its escape codes kept where they stand.",
    generics: &[],
    params: &[Field::required("text", Ty::String), Field::required("columns", Ty::Number), Field::optional("options", Ty::Object(&[Field::optional("hard", Ty::Boolean), Field::optional("wordWrap", Ty::Boolean), Field::optional("trim", Ty::Boolean)]))],
    returns: Ty::String,
  }] },
  Member { module: "terminal", name: "bigText", reach: Reach::Terminal, signatures: &[Signature {
    doc: "`text` drawn in `font`, a font file of cfonts' format, as rows joined by newlines with the font's colours as escape codes. A character the font lacks is left out and `|` starts a row.",
    generics: &[],
    params: &[
      Field::required("font", Ty::Unknown),
      Field::required("text", Ty::String),
      Field::optional("options", Ty::Object(&[
        Field::optional("align", Ty::String),
        Field::optional("colors", Ty::Unknown),
        Field::optional("background", Ty::String),
        Field::optional("letterSpacing", Ty::Number),
        Field::optional("lineHeight", Ty::Number),
        Field::optional("space", Ty::Boolean),
        Field::optional("maxLength", Ty::Number),
        Field::optional("width", Ty::Number),
      ])),
    ],
    returns: Ty::String,
  }] },
  member("path", "join", Reach::Render),
  member("path", "resolve", Reach::Body),
  member("path", "normalize", Reach::Render),
  member("path", "dirname", Reach::Render),
  member("path", "basename", Reach::Render),
  member("path", "extname", Reach::Render),
  member("path", "relative", Reach::Body),
  member("path", "isAbsolute", Reach::Render),
  member("system", "env", Reach::Process),
  member("system", "home", Reach::Process),
  member("system", "platform", Reach::Process),
  member("system", "tempDir", Reach::Process),
  member("system", "lineEnd", Reach::Process),
  member("system", "cwd", Reach::Process),
  member("system", "memory", Reach::Process),
  member("log", "trace", Reach::Render),
  member("log", "debug", Reach::Render),
  member("log", "info", Reach::Render),
  member("log", "warn", Reach::Render),
  member("log", "error", Reach::Render),
  member("log", "fatal", Reach::Render),
  member("file", "read", Reach::Process),
  member("file", "write", Reach::Process),
  member("file", "exists", Reach::Process),
  member("file", "list", Reach::Process),
  member("file", "makeDir", Reach::Process),
  member("file", "stat", Reach::Process),
  member("clipboard", "write", Reach::Process),
  member("clipboard", "read", Reach::Process),
];

/// The reach of a standard member or `None` when no such member exists.
pub fn standard_reach(module: &str, name: &str) -> Option<Reach> {
  STANDARD.iter().find(|member| member.module == module && member.name == name).map(|member| member.reach)
}

/// The extensions an interpreter answers, by `module.name`.
#[derive(Clone, Default)]
pub struct Extensions {
  map: BTreeMap<String, Extension>,
}

impl Extensions {
  /// No extensions at all, not even the standard library.
  pub fn empty() -> Self {
    Self::default()
  }

  /// The standard library.
  pub fn standard() -> Self {
    let mut extensions = Self::empty();
    crate::std::register(&mut extensions);
    extensions
  }

  /// Registers `f` under `name`, `module.member`, replacing what the name held.
  pub fn register<F>(&mut self, name: impl Into<String>, reach: Reach, f: F)
  where
    F: Fn(&Ambient, &[Value]) -> Result<Value, Fail> + Send + Sync + 'static,
  {
    self.map.insert(name.into(), Extension { reach, f: Arc::new(f) });
  }

  pub fn get(&self, name: &str) -> Option<&Extension> {
    self.map.get(name)
  }

  pub fn contains(&self, name: &str) -> bool {
    self.map.contains_key(name)
  }

  /// Every registered name, sorted.
  pub fn names(&self) -> Vec<String> {
    self.map.keys().cloned().collect()
  }

  pub fn call(&self, name: &str, ambient: &Ambient, args: &[Value]) -> Result<Value, Fail> {
    let extension = self.map.get(name).ok_or_else(|| Fail::internal(format!("extension `{name}` is not registered")))?;
    extension.call(ambient, args)
  }
}

impl std::fmt::Debug for Extensions {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    f.debug_list().entries(self.map.iter().map(|(name, e)| format!("{name} ({})", e.reach.as_str()))).finish()
  }
}
