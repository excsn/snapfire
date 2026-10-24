//! `text.frontMatter` and `text.withFrontMatter`: a YAML block between two
//! delimiter lines at the top of a text, read and written as gray-matter
//! does. `Terminal` reach, since the browser half has no YAML.

use snapfire_fsr_core::ext::type_error;
use snapfire_fsr_core::{Value, ValueMap};
use yaml_rust2::yaml::Hash;
use yaml_rust2::{Yaml, YamlEmitter, YamlLoader};

use crate::ext::{option, text, Ambient, Extensions, Reach};
use crate::interp::Fail;

pub fn register(extensions: &mut Extensions) {
  extensions.register("text.frontMatter", Reach::Terminal, front_matter);
  extensions.register("text.withFrontMatter", Reach::Terminal, with_front_matter);
}

/// The `delimiters` option: one string for both lines or `[open, close]`; `---` when absent.
fn delimiters<'a>(what: &str, args: &'a [Value], i: usize) -> Result<(&'a str, &'a str), Fail> {
  match option(what, args, i, "delimiters")? {
    None => Ok(("---", "---")),
    Some(Value::Str(both)) => Ok((both, both)),
    Some(Value::Seq(items)) => match items.as_slice() {
      [Value::Str(both)] => Ok((both, both)),
      [Value::Str(open), Value::Str(close)] => Ok((open, close)),
      _ => Err(Fail::internal(format!("{what} wants `delimiters` as a string or two strings"))),
    },
    Some(other) => Err(type_error(what, "`delimiters` as a string or two strings", other)),
  }
}

/// `text.frontMatter(source, { delimiters }?)`: `{ data, content }`. A text that does not open with the delimiter or opens with a longer run of its last character, is all content; an unclosed block runs to the end; the line break after the closing delimiter is dropped.
fn front_matter(_: &Ambient, args: &[Value]) -> Result<Value, Fail> {
  let what = "text.frontMatter";
  let source = text(what, args, 0)?;
  let (open, close) = delimiters(what, args, 1)?;
  let whole = |content: &str| parsed(Value::Map(ValueMap::default()), content);
  let Some(rest) = source.strip_prefix(open) else { return Ok(whole(source)) };
  if open.chars().last().is_some_and(|last| rest.starts_with(last)) {
    return Ok(whole(source));
  }
  let line = rest.find('\n').unwrap_or(rest.len());
  let language = rest[..line].trim();
  if !language.is_empty() && language != "yaml" {
    return Err(Fail::internal(format!("{what}: front matter in `{language}`; write YAML")));
  }
  let rest = if language.is_empty() { rest } else { &rest[line..] };
  let close = format!("\n{close}");
  let (block, content) = match rest.find(&close) {
    None => (rest, ""),
    Some(at) => {
      let after = &rest[at + close.len()..];
      let after = after.strip_prefix('\r').unwrap_or(after);
      (&rest[..at], after.strip_prefix('\n').unwrap_or(after))
    }
  };
  let commented = block.lines().all(|l| l.trim().is_empty() || l.trim_start().starts_with('#'));
  let data = if commented { Value::Map(ValueMap::default()) } else { read(what, block)? };
  Ok(parsed(data, content))
}

fn parsed(data: Value, content: &str) -> Value {
  let mut out = ValueMap::default();
  out.insert("data".to_owned(), data);
  out.insert("content".to_owned(), Value::str(content));
  Value::Map(out)
}

fn read(what: &str, block: &str) -> Result<Value, Fail> {
  let docs = YamlLoader::load_from_str(block).map_err(|e| Fail::internal(format!("{what}: {e}")))?;
  match docs.into_iter().next() {
    None | Some(Yaml::Null) => Ok(Value::Map(ValueMap::default())),
    Some(doc) => from_yaml(what, doc),
  }
}

fn from_yaml(what: &str, yaml: Yaml) -> Result<Value, Fail> {
  Ok(match yaml {
    Yaml::Null | Yaml::BadValue => Value::Null,
    Yaml::Boolean(b) => Value::Bool(b),
    Yaml::Integer(i) => Value::Int(i.into()),
    Yaml::Real(text) => match text.parse::<f64>() {
      Ok(f) => Value::F64(f),
      Err(_) => Value::str(text),
    },
    Yaml::String(s) => Value::str(s),
    Yaml::Array(items) => Value::seq(items.into_iter().map(|item| from_yaml(what, item)).collect::<Result<Vec<_>, _>>()?),
    Yaml::Hash(hash) => {
      let mut out = ValueMap::default();
      for (key, value) in hash {
        let key = match key {
          Yaml::String(s) | Yaml::Real(s) => s,
          Yaml::Integer(i) => i.to_string(),
          Yaml::Boolean(b) => b.to_string(),
          other => return Err(Fail::internal(format!("{what}: a key is {other:?}; write a string"))),
        };
        out.insert(key, from_yaml(what, value)?);
      }
      Value::Map(out)
    }
    Yaml::Alias(_) => return Err(Fail::internal(format!("{what}: an alias; write the value out"))),
  })
}

/// `text.withFrontMatter(content, data, { delimiters }?)`: `data` as YAML between the delimiter lines, then `content` ending in a line break. Empty `data` writes no block.
fn with_front_matter(_: &Ambient, args: &[Value]) -> Result<Value, Fail> {
  let what = "text.withFrontMatter";
  let content = text(what, args, 0)?;
  let (open, close) = delimiters(what, args, 2)?;
  let mut out = String::new();
  match args.get(1) {
    None | Some(Value::Null | Value::Undefined) => {}
    Some(Value::Map(fields)) if fields.values().all(|v| matches!(v, Value::Undefined)) => {}
    Some(data @ Value::Map(_)) => {
      let mut emitted = String::new();
      let mut emitter = YamlEmitter::new(&mut emitted);
      emitter.multiline_strings(true);
      emitter.dump(&to_yaml(what, data)?).map_err(|e| Fail::internal(format!("{what}: {e}")))?;
      let emitted = emitted.strip_prefix("---").unwrap_or(&emitted).trim();
      out.push_str(open);
      out.push('\n');
      out.push_str(emitted);
      out.push('\n');
      out.push_str(close);
      out.push('\n');
    }
    Some(other) => return Err(type_error(what, "data as an object", other)),
  }
  out.push_str(content);
  if !content.ends_with('\n') {
    out.push('\n');
  }
  Ok(Value::str(out))
}

fn to_yaml(what: &str, value: &Value) -> Result<Yaml, Fail> {
  Ok(match value {
    Value::Null | Value::Undefined => Yaml::Null,
    Value::Bool(b) => Yaml::Boolean(*b),
    Value::Int(i) => match i64::try_from(*i) {
      Ok(i) => Yaml::Integer(i),
      Err(_) => Yaml::Real(i.to_string()),
    },
    Value::UInt(u) => Yaml::Real(u.to_string()),
    Value::F32(f) => number(f64::from(*f)),
    Value::F64(f) => number(*f),
    Value::Str(s) => Yaml::String(s.to_string()),
    Value::Seq(items) => Yaml::Array(items.iter().map(|item| to_yaml(what, item)).collect::<Result<_, _>>()?),
    Value::Map(fields) => {
      let mut hash = Hash::new();
      for (key, field) in fields.iter().filter(|(_, v)| !matches!(v, Value::Undefined)) {
        hash.insert(Yaml::String(key.clone()), to_yaml(what, field)?);
      }
      Yaml::Hash(hash)
    }
    other => return Err(type_error(what, "data YAML can hold", other)),
  })
}

/// An integral float as an integer, as JavaScript writes one.
fn number(f: f64) -> Yaml {
  if f.fract() == 0.0 && f.abs() < 9.0e15 {
    Yaml::Integer(f as i64)
  } else {
    Yaml::Real(f.to_string())
  }
}
