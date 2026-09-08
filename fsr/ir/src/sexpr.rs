//! The s-expression form of a plan artifact: the text `plan.sexp` carries.
//!
//! [`Sx`] is what the text parses to. The typed layer turns an [`Sx`] into the
//! IR and back, collapsing the shapes a plan is mostly made of, so a literal
//! attribute is `(class "wide")` rather than a field wrapping a literal
//! wrapping a string.

use crate::ast::{ArithOp, Body, Builtin, CompareOp, Component, Entry, Expr, Handler, Lit, LogicOp, Stmt, Tmpl};

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{0}")]
pub struct SexprError(String);

impl SexprError {
  pub fn new(msg: impl std::fmt::Display) -> Self {
    Self(msg.to_string())
  }

  fn at(line: usize, msg: impl std::fmt::Display) -> Self {
    Self(format!("line {line}: {msg}"))
  }

  fn shape(msg: impl std::fmt::Display) -> Self {
    Self(msg.to_string())
  }
}

type Res<T> = Result<T, SexprError>;

/// One node of the syntax. A symbol and a string are distinct terms: `nil` is
/// the null literal and `"nil"` is the string, so the two never collide.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Sx {
  Sym(String),
  Str(String),
  List(Vec<Sx>),
  /// `{expr}`, an expression interpolated where a template child is expected.
  Interp(Box<Sx>),
}

impl Sx {
  fn sym(s: impl Into<String>) -> Self {
    Sx::Sym(s.into())
  }

  fn list(items: Vec<Sx>) -> Self {
    Sx::List(items)
  }

  fn as_sym(&self) -> Res<&str> {
    match self {
      Sx::Sym(s) => Ok(s),
      other => Err(SexprError::shape(format!("expected a symbol, found {}", other.kind()))),
    }
  }

  fn as_list(&self) -> Res<&[Sx]> {
    match self {
      Sx::List(items) => Ok(items),
      other => Err(SexprError::shape(format!("expected a list, found {}", other.kind()))),
    }
  }

  fn kind(&self) -> &'static str {
    match self {
      Sx::Sym(_) => "a symbol",
      Sx::Str(_) => "a string",
      Sx::List(_) => "a list",
      Sx::Interp(_) => "an interpolation",
    }
  }

  /// The head symbol of a list, for dispatching on a form.
  fn head(&self) -> Option<&str> {
    match self {
      Sx::List(items) => match items.first() {
        Some(Sx::Sym(s)) => Some(s),
        _ => None,
      },
      _ => None,
    }
  }
}

const DELIMS: &[u8] = b"()\"|;{}";

fn plain_symbol(s: &str) -> bool {
  !s.is_empty() && !s.bytes().any(|b| b.is_ascii_whitespace() || DELIMS.contains(&b) || b == b'\\')
}

fn write_escaped(out: &mut String, s: &str, quote: char) {
  out.push(quote);
  for c in s.chars() {
    match c {
      '\\' => out.push_str("\\\\"),
      '\n' => out.push_str("\\n"),
      '\r' => out.push_str("\\r"),
      '\t' => out.push_str("\\t"),
      c if c == quote => {
        out.push('\\');
        out.push(c);
      }
      c => out.push(c),
    }
  }
  out.push(quote);
}

// ---------------------------------------------------------------- printing

/// The column a list is allowed to reach before its items break onto their own
/// lines.
const WIDTH: usize = 100;

/// Every form, one per top-level line, wrapped at [`WIDTH`].
pub fn print(forms: &[Sx]) -> String {
  let mut out = String::new();
  for form in forms {
    write_form(&mut out, form, 0);
    out.push('\n');
  }
  out
}

fn inline(form: &Sx) -> String {
  let mut out = String::new();
  write_inline(&mut out, form);
  out
}

fn write_inline(out: &mut String, form: &Sx) {
  match form {
    Sx::Sym(s) => {
      if plain_symbol(s) {
        out.push_str(s);
      } else {
        write_escaped(out, s, '|');
      }
    }
    Sx::Str(s) => write_escaped(out, s, '"'),
    Sx::Interp(inner) => {
      out.push('{');
      write_inline(out, inner);
      out.push('}');
    }
    Sx::List(items) => {
      out.push('(');
      for (i, item) in items.iter().enumerate() {
        if i > 0 {
          out.push(' ');
        }
        write_inline(out, item);
      }
      out.push(')');
    }
  }
}

fn write_form(out: &mut String, form: &Sx, indent: usize) {
  let flat = inline(form);
  if indent + flat.len() <= WIDTH || matches!(form, Sx::Sym(_) | Sx::Str(_)) {
    out.push_str(&flat);
    return;
  }
  match form {
    Sx::Interp(inner) => {
      out.push('{');
      write_form(out, inner, indent + 1);
      out.push('}');
    }
    Sx::List(items) => {
      out.push('(');
      // The head and, where it is short, the item after it stay on the opening
      // line: `(el div` reads as one thing and breaking it helps nobody.
      let mut first = 1;
      if let Some(head) = items.first() {
        write_inline(out, head);
        if let Some(next) = items.get(1) {
          let flat = inline(next);
          if flat.len() <= 32 && !matches!(next, Sx::List(_)) {
            out.push(' ');
            out.push_str(&flat);
            first = 2;
          }
        }
      }
      let pad = indent + 2;
      for item in &items[first.min(items.len())..] {
        out.push('\n');
        for _ in 0..pad {
          out.push(' ');
        }
        write_form(out, item, pad);
      }
      out.push(')');
    }
    _ => out.push_str(&flat),
  }
}

// ----------------------------------------------------------------- parsing

struct Parser<'a> {
  src: &'a [u8],
  pos: usize,
  line: usize,
}

/// Every top-level form in `src`.
pub fn parse(src: &str) -> Res<Vec<Sx>> {
  let mut p = Parser { src: src.as_bytes(), pos: 0, line: 1 };
  let mut out = Vec::new();
  loop {
    p.skip_trivia();
    if p.pos >= p.src.len() {
      return Ok(out);
    }
    out.push(p.form()?);
  }
}

impl<'a> Parser<'a> {
  fn skip_trivia(&mut self) {
    while self.pos < self.src.len() {
      match self.src[self.pos] {
        b'\n' => {
          self.line += 1;
          self.pos += 1;
        }
        b if b.is_ascii_whitespace() => self.pos += 1,
        b';' => {
          while self.pos < self.src.len() && self.src[self.pos] != b'\n' {
            self.pos += 1;
          }
        }
        _ => return,
      }
    }
  }

  fn form(&mut self) -> Res<Sx> {
    self.skip_trivia();
    let line = self.line;
    match self.src.get(self.pos) {
      None => Err(SexprError::at(line, "the file ends inside a form")),
      Some(b'(') => {
        self.pos += 1;
        let mut items = Vec::new();
        loop {
          self.skip_trivia();
          match self.src.get(self.pos) {
            None => return Err(SexprError::at(line, "unclosed `(`")),
            Some(b')') => {
              self.pos += 1;
              return Ok(Sx::List(items));
            }
            _ => items.push(self.form()?),
          }
        }
      }
      Some(b'{') => {
        self.pos += 1;
        let inner = self.form()?;
        self.skip_trivia();
        match self.src.get(self.pos) {
          Some(b'}') => {
            self.pos += 1;
            Ok(Sx::Interp(Box::new(inner)))
          }
          _ => Err(SexprError::at(line, "unclosed `{`")),
        }
      }
      Some(b')') => Err(SexprError::at(line, "unbalanced `)`")),
      Some(b'}') => Err(SexprError::at(line, "unbalanced `}`")),
      Some(b'"') => self.quoted(b'"').map(Sx::Str),
      Some(b'|') => self.quoted(b'|').map(Sx::Sym),
      _ => {
        let start = self.pos;
        while self.pos < self.src.len() {
          let b = self.src[self.pos];
          if b.is_ascii_whitespace() || DELIMS.contains(&b) {
            break;
          }
          self.pos += 1;
        }
        let raw = std::str::from_utf8(&self.src[start..self.pos])
          .map_err(|_| SexprError::at(line, "a symbol is not valid UTF-8"))?;
        Ok(Sx::Sym(raw.to_owned()))
      }
    }
  }

  fn quoted(&mut self, quote: u8) -> Res<String> {
    let line = self.line;
    self.pos += 1;
    let mut out = String::new();
    loop {
      match self.src.get(self.pos) {
        None => return Err(SexprError::at(line, "the file ends inside a quoted term")),
        Some(&b) if b == quote => {
          self.pos += 1;
          return Ok(out);
        }
        Some(b'\\') => {
          self.pos += 1;
          match self.src.get(self.pos) {
            None => return Err(SexprError::at(line, "the file ends after `\\`")),
            Some(b'n') => out.push('\n'),
            Some(b'r') => out.push('\r'),
            Some(b't') => out.push('\t'),
            Some(&b) => out.push(b as char),
          }
          self.pos += 1;
        }
        Some(_) => {
          let start = self.pos;
          while self.pos < self.src.len() {
            let b = self.src[self.pos];
            if b == quote || b == b'\\' {
              break;
            }
            if b == b'\n' {
              self.line += 1;
            }
            self.pos += 1;
          }
          out.push_str(
            std::str::from_utf8(&self.src[start..self.pos])
              .map_err(|_| SexprError::at(line, "a quoted term is not valid UTF-8"))?,
          );
        }
      }
    }
  }
}

// ------------------------------------------------------------- the IR layer

/// Heads that mean something other than a field name in an entry position.
const ENTRY_MARKERS: [&str; 4] = ["=", ":", "...", "@"];

fn reserved_atom(s: &str) -> bool {
  s.is_empty()
    || s == "nil"
    || s == "#t"
    || s == "#f"
    || s.parse::<i128>().is_ok()
    || s.parse::<f64>().is_ok()
}

fn lit_to_sx(lit: &Lit) -> Sx {
  match lit {
    Lit::Null => Sx::sym("nil"),
    Lit::Bool(true) => Sx::sym("#t"),
    Lit::Bool(false) => Sx::sym("#f"),
    Lit::Int(n) => Sx::Sym(n.to_string()),
    Lit::Float(f) => Sx::Sym(format!("{f:?}")),
    Lit::Str(s) => Sx::Str(s.clone()),
  }
}

fn arith_sym(op: ArithOp) -> &'static str {
  match op {
    ArithOp::Add => "+",
    ArithOp::Sub => "-",
    ArithOp::Mul => "*",
    ArithOp::Div => "/",
    ArithOp::Rem => "%",
  }
}

fn arith_of(s: &str) -> Option<ArithOp> {
  Some(match s {
    "+" => ArithOp::Add,
    "-" => ArithOp::Sub,
    "*" => ArithOp::Mul,
    "/" => ArithOp::Div,
    "%" => ArithOp::Rem,
    _ => return None,
  })
}

fn compare_sym(op: CompareOp) -> &'static str {
  match op {
    CompareOp::Eq => "==",
    CompareOp::Ne => "!=",
    CompareOp::Lt => "<",
    CompareOp::Le => "<=",
    CompareOp::Gt => ">",
    CompareOp::Ge => ">=",
  }
}

fn compare_of(s: &str) -> Option<CompareOp> {
  Some(match s {
    "==" => CompareOp::Eq,
    "!=" => CompareOp::Ne,
    "<" => CompareOp::Lt,
    "<=" => CompareOp::Le,
    ">" => CompareOp::Gt,
    ">=" => CompareOp::Ge,
    _ => return None,
  })
}

fn builtin_sym(b: Builtin) -> &'static str {
  match b {
    Builtin::Round => "round",
    Builtin::Floor => "floor",
    Builtin::Ceil => "ceil",
    Builtin::Abs => "abs",
    Builtin::Min => "min",
    Builtin::Max => "max",
    Builtin::ToFixed => "to-fixed",
    Builtin::Repeat => "repeat",
    Builtin::Join => "join",
    Builtin::Trim => "trim",
    Builtin::Upper => "upper",
    Builtin::Lower => "lower",
    Builtin::Includes => "includes",
    Builtin::EncodeUriComponent => "encode-uri",
    Builtin::LocaleNumber => "locale-number",
    Builtin::Range => "range",
    Builtin::Omit => "omit",
  }
}

fn builtin_of(s: &str) -> Option<Builtin> {
  Some(match s {
    "round" => Builtin::Round,
    "floor" => Builtin::Floor,
    "ceil" => Builtin::Ceil,
    "abs" => Builtin::Abs,
    "min" => Builtin::Min,
    "max" => Builtin::Max,
    "to-fixed" => Builtin::ToFixed,
    "repeat" => Builtin::Repeat,
    "join" => Builtin::Join,
    "trim" => Builtin::Trim,
    "upper" => Builtin::Upper,
    "lower" => Builtin::Lower,
    "includes" => Builtin::Includes,
    "encode-uri" => Builtin::EncodeUriComponent,
    "locale-number" => Builtin::LocaleNumber,
    "range" => Builtin::Range,
    "omit" => Builtin::Omit,
    _ => return None,
  })
}

fn form(head: &str, mut rest: Vec<Sx>) -> Sx {
  let mut items = vec![Sx::sym(head)];
  items.append(&mut rest);
  Sx::List(items)
}

fn named_args(args: &[(String, Expr)]) -> Vec<Sx> {
  args.iter().map(|(name, value)| Sx::list(vec![Sx::Sym(name.clone()), expr_to_sx(value)])).collect()
}

pub fn expr_to_sx(expr: &Expr) -> Sx {
  let one = |head: &str, e: &Expr| form(head, vec![expr_to_sx(e)]);
  let two = |head: &str, a: &Expr, b: &Expr| form(head, vec![expr_to_sx(a), expr_to_sx(b)]);
  match expr {
    Expr::Lit(lit) => lit_to_sx(lit),
    Expr::Var(name) => {
      if reserved_atom(name) {
        form("var", vec![Sx::Sym(name.clone())])
      } else {
        Sx::Sym(name.clone())
      }
    }
    Expr::Param(n) => form("param", vec![Sx::Sym(n.clone())]),
    Expr::Query(n) => form("query", vec![Sx::Sym(n.clone())]),
    Expr::Session(n) => form("session", vec![Sx::Sym(n.clone())]),
    Expr::Store(n) => form("store", vec![Sx::Sym(n.clone())]),
    Expr::Const(n) => form("const", vec![Sx::Sym(n.clone())]),
    Expr::Identity(path) => form("identity", path.iter().map(|p| Sx::Sym(p.clone())).collect()),
    Expr::Locale => form("locale", vec![]),
    Expr::Path => form("path", vec![]),
    Expr::Input => form("input", vec![]),
    Expr::Now => form("now", vec![]),
    Expr::Object(entries) => form("obj", entries.iter().map(entry_to_sx).collect()),
    Expr::Array(entries) => form("arr", entries.iter().map(entry_to_sx).collect()),
    Expr::Field(base, name) => form(".", vec![expr_to_sx(base), Sx::Sym(name.clone())]),
    Expr::Index(base, index) => two("idx", base, index),
    Expr::Arith(op, a, b) => two(arith_sym(*op), a, b),
    Expr::Compare(op, a, b) => two(compare_sym(*op), a, b),
    Expr::Logic(LogicOp::And, a, b) => two("and", a, b),
    Expr::Logic(LogicOp::Or, a, b) => two("or", a, b),
    Expr::Not(e) => one("!", e),
    Expr::Coalesce(a, b) => two("??", a, b),
    Expr::Ternary(c, a, b) => form("if", vec![expr_to_sx(c), expr_to_sx(a), expr_to_sx(b)]),
    Expr::Template(parts) => form("concat", parts.iter().map(expr_to_sx).collect()),
    Expr::Call { service, method, args } => {
      let mut rest = vec![Sx::Sym(service.clone()), Sx::Sym(method.clone())];
      rest.extend(named_args(args));
      form("call", rest)
    }
    Expr::NativeCall { module, method, args, sync } => {
      let mut rest = vec![Sx::Sym(module.clone()), Sx::Sym(method.clone())];
      rest.extend(named_args(args));
      form(if *sync { "native-sync" } else { "native" }, rest)
    }
    Expr::Lambda { params, body } => form(
      "fn",
      vec![Sx::list(params.iter().map(|p| Sx::Sym(p.clone())).collect()), expr_to_sx(body)],
    ),
    Expr::Apply { f, args } => {
      let mut rest = vec![expr_to_sx(f)];
      rest.extend(args.iter().map(expr_to_sx));
      form("apply", rest)
    }
    Expr::Builtin { name, args } => form(builtin_sym(*name), args.iter().map(expr_to_sx).collect()),
    Expr::Ext { module, name, args } => {
      let mut rest = vec![Sx::Sym(module.clone()), Sx::Sym(name.clone())];
      rest.extend(args.iter().map(expr_to_sx));
      form("ext", rest)
    }
    Expr::Map(a, b) => two("map", a, b),
    Expr::Filter(a, b) => two("filter", a, b),
    Expr::Reduce(a, b, c) => form("reduce", vec![expr_to_sx(a), expr_to_sx(b), expr_to_sx(c)]),
    Expr::Find(a, b) => two("find", a, b),
    Expr::FindIndex(a, b) => two("find-index", a, b),
    Expr::Some(a, b) => two("some", a, b),
    Expr::Every(a, b) => two("every", a, b),
    Expr::Entries(e) => one("entries", e),
    Expr::Keys(e) => one("keys", e),
    Expr::Values(e) => one("values", e),
    Expr::Length(e) => one("length", e),
    Expr::Str(e) => one("string", e),
    Expr::Num(e) => one("number", e),
    Expr::BigInt(e) => one("bigint", e),
    Expr::Hoist { id, expr } => form("hoist", vec![Sx::Sym(id.to_string()), expr_to_sx(expr)]),
  }
}

fn entry_to_sx(entry: &Entry) -> Sx {
  match entry {
    Entry::Field(name, value) => {
      if ENTRY_MARKERS.contains(&name.as_str()) {
        form("=", vec![Sx::Sym(name.clone()), expr_to_sx(value)])
      } else {
        Sx::list(vec![Sx::Sym(name.clone()), expr_to_sx(value)])
      }
    }
    Entry::Item(value) => form(":", vec![expr_to_sx(value)]),
    Entry::Spread(value) => form("...", vec![expr_to_sx(value)]),
    Entry::Computed(key, value) => form("@", vec![expr_to_sx(key), expr_to_sx(value)]),
  }
}

fn opt_sx(value: &Option<String>) -> Sx {
  match value {
    Some(s) => Sx::Str(s.clone()),
    None => Sx::sym("nil"),
  }
}

pub fn tmpl_to_sx(tmpl: &Tmpl) -> Sx {
  match tmpl {
    Tmpl::Text(text) => Sx::Str(text.clone()),
    Tmpl::Expr(expr) => Sx::Interp(Box::new(expr_to_sx(expr))),
    Tmpl::Element { tag, attrs, children } => {
      let mut rest = vec![Sx::Sym(tag.clone()), Sx::list(attrs.iter().map(entry_to_sx).collect())];
      rest.extend(children.iter().map(tmpl_to_sx));
      form("el", rest)
    }
    Tmpl::Fragment(children) => form("<>", children.iter().map(tmpl_to_sx).collect()),
    Tmpl::If { cond, then, r#else } => {
      let mut rest = vec![expr_to_sx(cond), tmpl_to_sx(then)];
      if let Some(other) = r#else {
        rest.push(tmpl_to_sx(other));
      }
      form("if", rest)
    }
    Tmpl::For { over, params, body } => form(
      "for",
      vec![
        Sx::list(params.iter().map(|p| Sx::Sym(p.clone())).collect()),
        expr_to_sx(over),
        tmpl_to_sx(body),
      ],
    ),
    Tmpl::Let { name, expr, then } => {
      form("let", vec![Sx::Sym(name.clone()), expr_to_sx(expr), tmpl_to_sx(then)])
    }
    Tmpl::Component { module, props, children, id } => {
      let mut rest = vec![
        Sx::Sym(module.clone()),
        Sx::Sym(id.to_string()),
        Sx::list(props.iter().map(entry_to_sx).collect()),
      ];
      rest.extend(children.iter().map(tmpl_to_sx));
      form("comp", rest)
    }
    Tmpl::Island { module, props, children, when, mode, id } => {
      let mut rest = vec![
        Sx::Sym(module.clone()),
        Sx::Sym(id.to_string()),
        opt_sx(when),
        opt_sx(mode),
        Sx::list(props.iter().map(entry_to_sx).collect()),
      ];
      rest.extend(children.iter().map(tmpl_to_sx));
      form("island", rest)
    }
    Tmpl::Slot(name) => form("slot", vec![Sx::Sym(name.clone())]),
    Tmpl::Baked { open, tag, children } => {
      let mut rest = vec![Sx::Str(open.clone()), opt_sx(tag)];
      rest.extend(children.iter().map(tmpl_to_sx));
      form("baked", rest)
    }
  }
}

fn body_to_sx(body: &Body) -> Vec<Sx> {
  body.iter().map(stmt_to_sx).collect()
}

pub fn stmt_to_sx(stmt: &Stmt) -> Sx {
  match stmt {
    Stmt::Let { name, expr } => form("let", vec![Sx::Sym(name.clone()), expr_to_sx(expr)]),
    Stmt::If { cond, then, r#else } => {
      let mut rest = vec![expr_to_sx(cond), Sx::list(body_to_sx(then))];
      if !r#else.is_empty() {
        rest.push(Sx::list(body_to_sx(r#else)));
      }
      form("if", rest)
    }
    Stmt::ForOf { name, over, body } => {
      let mut rest = vec![Sx::Sym(name.clone()), expr_to_sx(over)];
      rest.extend(body_to_sx(body));
      form("for-of", rest)
    }
    Stmt::Return(expr) => form("ret", vec![expr_to_sx(expr)]),
    Stmt::Guard { cond, kind, message } => form(
      "guard",
      vec![expr_to_sx(cond), Sx::Sym(kind.clone()), Sx::Str(message.clone())],
    ),
    Stmt::SessionSet { key, path, value } => form(
      "session-set",
      vec![
        Sx::Sym(key.clone()),
        Sx::list(path.iter().map(expr_to_sx).collect()),
        expr_to_sx(value),
      ],
    ),
    Stmt::SessionDelete { key, path } => form(
      "session-del",
      vec![Sx::Sym(key.clone()), Sx::list(path.iter().map(expr_to_sx).collect())],
    ),
    Stmt::Expr(expr) => form("do", vec![expr_to_sx(expr)]),
  }
}

/// A component as the sections it has, each tagged, so an absent section is an
/// absent form rather than a hole to count past. The plan puts the module id
/// between the head and these.
pub fn component_sections(component: &Component) -> Vec<Sx> {
  let mut rest = Vec::new();
  if !component.state.is_empty() {
    rest.push(form("state", component.state.iter().map(|s| Sx::Sym(s.clone())).collect()));
  }
  if !component.body.is_empty() {
    rest.push(form("body", body_to_sx(&component.body)));
  }
  rest.push(form("render", vec![tmpl_to_sx(&component.render)]));
  for handler in &component.handlers {
    let mut on = vec![Sx::Sym(handler.event.clone())];
    on.extend(body_to_sx(&handler.body));
    rest.push(form("on", on));
  }
  rest
}

pub fn component_to_sx(component: &Component) -> Sx {
  form("component", component_sections(component))
}

// ------------------------------------------------------------------ reading

fn args<'a>(items: &'a [Sx], head: &str, n: usize) -> Res<&'a [Sx]> {
  let rest = &items[1..];
  if rest.len() != n {
    return Err(SexprError::shape(format!("`{head}` takes {n} terms, found {}", rest.len())));
  }
  Ok(rest)
}

fn at_least<'a>(items: &'a [Sx], head: &str, n: usize) -> Res<&'a [Sx]> {
  let rest = &items[1..];
  if rest.len() < n {
    return Err(SexprError::shape(format!("`{head}` takes at least {n} terms, found {}", rest.len())));
  }
  Ok(rest)
}

fn sym_of(sx: &Sx) -> Res<String> {
  sx.as_sym().map(str::to_owned)
}

fn str_of(sx: &Sx) -> Res<String> {
  match sx {
    Sx::Str(s) => Ok(s.clone()),
    other => Err(SexprError::shape(format!("expected a string, found {}", other.kind()))),
  }
}

fn opt_of(sx: &Sx) -> Res<Option<String>> {
  match sx {
    Sx::Sym(s) if s == "nil" => Ok(None),
    Sx::Str(s) => Ok(Some(s.clone())),
    other => Err(SexprError::shape(format!("expected a string or `nil`, found {}", other.kind()))),
  }
}

fn u32_of(sx: &Sx) -> Res<u32> {
  sx.as_sym()?.parse().map_err(|_| SexprError::shape("expected a number"))
}

fn boxed(sx: &Sx) -> Res<Box<Expr>> {
  expr_from_sx(sx).map(Box::new)
}

fn named_args_from(items: &[Sx]) -> Res<Vec<(String, Expr)>> {
  items
    .iter()
    .map(|item| {
      let pair = item.as_list()?;
      if pair.len() != 2 {
        return Err(SexprError::shape("a named argument is `(name expr)`"));
      }
      Ok((sym_of(&pair[0])?, expr_from_sx(&pair[1])?))
    })
    .collect()
}

fn atom_to_expr(s: &str) -> Expr {
  match s {
    "nil" => Expr::Lit(Lit::Null),
    "#t" => Expr::Lit(Lit::Bool(true)),
    "#f" => Expr::Lit(Lit::Bool(false)),
    _ => {
      if let Ok(n) = s.parse::<i128>() {
        Expr::Lit(Lit::Int(n))
      } else if let Ok(f) = s.parse::<f64>() {
        Expr::Lit(Lit::Float(f))
      } else {
        Expr::Var(s.to_owned())
      }
    }
  }
}

pub fn expr_from_sx(sx: &Sx) -> Res<Expr> {
  let items = match sx {
    Sx::Str(s) => return Ok(Expr::Lit(Lit::Str(s.clone()))),
    Sx::Sym(s) => return Ok(atom_to_expr(s)),
    Sx::Interp(_) => {
      return Err(SexprError::shape("`{...}` belongs in a template, not an expression"))
    }
    Sx::List(items) => items,
  };
  let head = sx
    .head()
    .ok_or_else(|| SexprError::shape("an expression form starts with a symbol"))?;
  if let Some(op) = arith_of(head) {
    let a = args(items, head, 2)?;
    return Ok(Expr::Arith(op, boxed(&a[0])?, boxed(&a[1])?));
  }
  if let Some(op) = compare_of(head) {
    let a = args(items, head, 2)?;
    return Ok(Expr::Compare(op, boxed(&a[0])?, boxed(&a[1])?));
  }
  let two = |head: &str| -> Res<(Box<Expr>, Box<Expr>)> {
    let a = args(items, head, 2)?;
    Ok((boxed(&a[0])?, boxed(&a[1])?))
  };
  let one = |head: &str| -> Res<Box<Expr>> { boxed(&args(items, head, 1)?[0]) };
  Ok(match head {
    "var" => Expr::Var(sym_of(&args(items, head, 1)?[0])?),
    "param" => Expr::Param(sym_of(&args(items, head, 1)?[0])?),
    "query" => Expr::Query(sym_of(&args(items, head, 1)?[0])?),
    "session" => Expr::Session(sym_of(&args(items, head, 1)?[0])?),
    "store" => Expr::Store(sym_of(&args(items, head, 1)?[0])?),
    "const" => Expr::Const(sym_of(&args(items, head, 1)?[0])?),
    "identity" => Expr::Identity(items[1..].iter().map(sym_of).collect::<Res<_>>()?),
    "locale" => {
      args(items, head, 0)?;
      Expr::Locale
    }
    "path" => {
      args(items, head, 0)?;
      Expr::Path
    }
    "input" => {
      args(items, head, 0)?;
      Expr::Input
    }
    "now" => {
      args(items, head, 0)?;
      Expr::Now
    }
    "obj" => Expr::Object(items[1..].iter().map(entry_from_sx).collect::<Res<_>>()?),
    "arr" => Expr::Array(items[1..].iter().map(entry_from_sx).collect::<Res<_>>()?),
    "." => {
      let a = args(items, head, 2)?;
      Expr::Field(boxed(&a[0])?, sym_of(&a[1])?)
    }
    "idx" => {
      let (a, b) = two(head)?;
      Expr::Index(a, b)
    }
    "and" => {
      let (a, b) = two(head)?;
      Expr::Logic(LogicOp::And, a, b)
    }
    "or" => {
      let (a, b) = two(head)?;
      Expr::Logic(LogicOp::Or, a, b)
    }
    "!" => Expr::Not(one(head)?),
    "??" => {
      let (a, b) = two(head)?;
      Expr::Coalesce(a, b)
    }
    "if" => {
      let a = args(items, head, 3)?;
      Expr::Ternary(boxed(&a[0])?, boxed(&a[1])?, boxed(&a[2])?)
    }
    "concat" => Expr::Template(items[1..].iter().map(expr_from_sx).collect::<Res<_>>()?),
    "call" => {
      let a = at_least(items, head, 2)?;
      Expr::Call { service: sym_of(&a[0])?, method: sym_of(&a[1])?, args: named_args_from(&a[2..])? }
    }
    "native" | "native-sync" => {
      let a = at_least(items, head, 2)?;
      Expr::NativeCall {
        module: sym_of(&a[0])?,
        method: sym_of(&a[1])?,
        args: named_args_from(&a[2..])?,
        sync: head == "native-sync",
      }
    }
    "fn" => {
      let a = args(items, head, 2)?;
      Expr::Lambda {
        params: a[0].as_list()?.iter().map(sym_of).collect::<Res<_>>()?,
        body: boxed(&a[1])?,
      }
    }
    "apply" => {
      let a = at_least(items, head, 1)?;
      Expr::Apply { f: boxed(&a[0])?, args: a[1..].iter().map(expr_from_sx).collect::<Res<_>>()? }
    }
    "ext" => {
      let a = at_least(items, head, 2)?;
      Expr::Ext {
        module: sym_of(&a[0])?,
        name: sym_of(&a[1])?,
        args: a[2..].iter().map(expr_from_sx).collect::<Res<_>>()?,
      }
    }
    "map" => {
      let (a, b) = two(head)?;
      Expr::Map(a, b)
    }
    "filter" => {
      let (a, b) = two(head)?;
      Expr::Filter(a, b)
    }
    "reduce" => {
      let a = args(items, head, 3)?;
      Expr::Reduce(boxed(&a[0])?, boxed(&a[1])?, boxed(&a[2])?)
    }
    "find" => {
      let (a, b) = two(head)?;
      Expr::Find(a, b)
    }
    "find-index" => {
      let (a, b) = two(head)?;
      Expr::FindIndex(a, b)
    }
    "some" => {
      let (a, b) = two(head)?;
      Expr::Some(a, b)
    }
    "every" => {
      let (a, b) = two(head)?;
      Expr::Every(a, b)
    }
    "entries" => Expr::Entries(one(head)?),
    "keys" => Expr::Keys(one(head)?),
    "values" => Expr::Values(one(head)?),
    "length" => Expr::Length(one(head)?),
    "string" => Expr::Str(one(head)?),
    "number" => Expr::Num(one(head)?),
    "bigint" => Expr::BigInt(one(head)?),
    "hoist" => {
      let a = args(items, head, 2)?;
      Expr::Hoist { id: u32_of(&a[0])?, expr: boxed(&a[1])? }
    }
    other => match builtin_of(other) {
      Some(name) => Expr::Builtin {
        name,
        args: items[1..].iter().map(expr_from_sx).collect::<Res<_>>()?,
      },
      None => return Err(SexprError::shape(format!("`{other}` is not an expression form"))),
    },
  })
}

fn entry_from_sx(sx: &Sx) -> Res<Entry> {
  let items = sx.as_list()?;
  let head = sx.head().ok_or_else(|| SexprError::shape("an entry starts with a symbol"))?;
  Ok(match head {
    "=" => {
      let a = args(items, head, 2)?;
      Entry::Field(sym_of(&a[0])?, expr_from_sx(&a[1])?)
    }
    ":" => Entry::Item(expr_from_sx(&args(items, head, 1)?[0])?),
    "..." => Entry::Spread(expr_from_sx(&args(items, head, 1)?[0])?),
    "@" => {
      let a = args(items, head, 2)?;
      Entry::Computed(expr_from_sx(&a[0])?, expr_from_sx(&a[1])?)
    }
    name => {
      let a = args(items, name, 1)?;
      Entry::Field(name.to_owned(), expr_from_sx(&a[0])?)
    }
  })
}

pub fn tmpl_from_sx(sx: &Sx) -> Res<Tmpl> {
  let items = match sx {
    Sx::Str(s) => return Ok(Tmpl::Text(s.clone())),
    Sx::Interp(inner) => return Ok(Tmpl::Expr(expr_from_sx(inner)?)),
    Sx::Sym(s) => {
      return Err(SexprError::shape(format!("`{s}` is not a template; text is quoted")))
    }
    Sx::List(items) => items,
  };
  let head = sx.head().ok_or_else(|| SexprError::shape("a template form starts with a symbol"))?;
  Ok(match head {
    "el" => {
      let a = at_least(items, head, 2)?;
      Tmpl::Element {
        tag: sym_of(&a[0])?,
        attrs: a[1].as_list()?.iter().map(entry_from_sx).collect::<Res<_>>()?,
        children: a[2..].iter().map(tmpl_from_sx).collect::<Res<_>>()?,
      }
    }
    "<>" => Tmpl::Fragment(items[1..].iter().map(tmpl_from_sx).collect::<Res<_>>()?),
    "if" => {
      let a = at_least(items, head, 2)?;
      if a.len() > 3 {
        return Err(SexprError::shape("a template `if` takes a condition, a branch and at most one else"));
      }
      Tmpl::If {
        cond: expr_from_sx(&a[0])?,
        then: Box::new(tmpl_from_sx(&a[1])?),
        r#else: a.get(2).map(tmpl_from_sx).transpose()?.map(Box::new),
      }
    }
    "for" => {
      let a = args(items, head, 3)?;
      Tmpl::For {
        params: a[0].as_list()?.iter().map(sym_of).collect::<Res<_>>()?,
        over: expr_from_sx(&a[1])?,
        body: Box::new(tmpl_from_sx(&a[2])?),
      }
    }
    "let" => {
      let a = args(items, head, 3)?;
      Tmpl::Let {
        name: sym_of(&a[0])?,
        expr: expr_from_sx(&a[1])?,
        then: Box::new(tmpl_from_sx(&a[2])?),
      }
    }
    "comp" => {
      let a = at_least(items, head, 3)?;
      Tmpl::Component {
        module: sym_of(&a[0])?,
        id: u32_of(&a[1])?,
        props: a[2].as_list()?.iter().map(entry_from_sx).collect::<Res<_>>()?,
        children: a[3..].iter().map(tmpl_from_sx).collect::<Res<_>>()?,
      }
    }
    "island" => {
      let a = at_least(items, head, 5)?;
      Tmpl::Island {
        module: sym_of(&a[0])?,
        id: u32_of(&a[1])?,
        when: opt_of(&a[2])?,
        mode: opt_of(&a[3])?,
        props: a[4].as_list()?.iter().map(entry_from_sx).collect::<Res<_>>()?,
        children: a[5..].iter().map(tmpl_from_sx).collect::<Res<_>>()?,
      }
    }
    "slot" => Tmpl::Slot(sym_of(&args(items, head, 1)?[0])?),
    "baked" => {
      let a = at_least(items, head, 2)?;
      Tmpl::Baked {
        open: str_of(&a[0])?,
        tag: opt_of(&a[1])?,
        children: a[2..].iter().map(tmpl_from_sx).collect::<Res<_>>()?,
      }
    }
    other => return Err(SexprError::shape(format!("`{other}` is not a template form"))),
  })
}

fn body_from_sx(items: &[Sx]) -> Res<Body> {
  items.iter().map(stmt_from_sx).collect()
}

pub fn stmt_from_sx(sx: &Sx) -> Res<Stmt> {
  let items = sx.as_list()?;
  let head = sx.head().ok_or_else(|| SexprError::shape("a statement starts with a symbol"))?;
  Ok(match head {
    "let" => {
      let a = args(items, head, 2)?;
      Stmt::Let { name: sym_of(&a[0])?, expr: expr_from_sx(&a[1])? }
    }
    "if" => {
      let a = at_least(items, head, 2)?;
      if a.len() > 3 {
        return Err(SexprError::shape("a statement `if` takes a condition, a body and at most one else"));
      }
      Stmt::If {
        cond: expr_from_sx(&a[0])?,
        then: body_from_sx(a[1].as_list()?)?,
        r#else: match a.get(2) {
          Some(other) => body_from_sx(other.as_list()?)?,
          None => Vec::new(),
        },
      }
    }
    "for-of" => {
      let a = at_least(items, head, 2)?;
      Stmt::ForOf { name: sym_of(&a[0])?, over: expr_from_sx(&a[1])?, body: body_from_sx(&a[2..])? }
    }
    "ret" => Stmt::Return(expr_from_sx(&args(items, head, 1)?[0])?),
    "guard" => {
      let a = args(items, head, 3)?;
      Stmt::Guard { cond: expr_from_sx(&a[0])?, kind: sym_of(&a[1])?, message: str_of(&a[2])? }
    }
    "session-set" => {
      let a = args(items, head, 3)?;
      Stmt::SessionSet {
        key: sym_of(&a[0])?,
        path: a[1].as_list()?.iter().map(expr_from_sx).collect::<Res<_>>()?,
        value: expr_from_sx(&a[2])?,
      }
    }
    "session-del" => {
      let a = args(items, head, 2)?;
      Stmt::SessionDelete {
        key: sym_of(&a[0])?,
        path: a[1].as_list()?.iter().map(expr_from_sx).collect::<Res<_>>()?,
      }
    }
    "do" => Stmt::Expr(expr_from_sx(&args(items, head, 1)?[0])?),
    other => return Err(SexprError::shape(format!("`{other}` is not a statement form"))),
  })
}

pub fn component_from_sx(sx: &Sx) -> Res<Component> {
  let items = sx.as_list()?;
  if sx.head() != Some("component") {
    return Err(SexprError::shape("a component is `(component ...)`"));
  }
  component_from_sections(&items[1..])
}

pub fn component_from_sections(items: &[Sx]) -> Res<Component> {
  let mut out = Component { body: Vec::new(), render: Tmpl::Fragment(Vec::new()), state: Vec::new(), handlers: Vec::new() };
  let mut rendered = false;
  for section in items {
    let inner = section.as_list()?;
    match section.head() {
      Some("state") => out.state = inner[1..].iter().map(sym_of).collect::<Res<_>>()?,
      Some("body") => out.body = body_from_sx(&inner[1..])?,
      Some("render") => {
        out.render = tmpl_from_sx(&args(inner, "render", 1)?[0])?;
        rendered = true;
      }
      Some("on") => {
        let a = at_least(inner, "on", 1)?;
        out.handlers.push(Handler { event: sym_of(&a[0])?, body: body_from_sx(&a[1..])? });
      }
      Some(other) => return Err(SexprError::shape(format!("`{other}` is not a component section"))),
      None => return Err(SexprError::shape("a component section starts with a symbol")),
    }
  }
  if !rendered {
    return Err(SexprError::shape("a component needs a `(render ...)` section"));
  }
  Ok(out)
}
