//! ESLint's configuration as `fsr.lint.toml`: an `.eslintrc` as JSON, or a flat `eslint.config.js` read
//! without running it, where it writes its configuration out literally.

use std::path::Path;

use oxc_allocator::Allocator;
use oxc_ast::ast::{ArrayExpressionElement, Expression, ObjectPropertyKind, PropertyKey, Statement};
use oxc_parser::Parser;
use oxc_span::{GetSpan, SourceType};
use serde_json::Value;

use crate::config::full_id;
use crate::RuleInfo;

/// A translated configuration and what it could not carry over.
#[derive(Debug, Clone, Default)]
#[non_exhaustive]
pub struct Translation {
  /// The `fsr.lint.toml` text.
  pub toml: String,
  /// Each rule, preset or setting with no counterpart, and each part of the source not read.
  pub notes: Vec<String>,
}

/// Presets whose rules `fsr lint` runs by default, so extending one needs nothing written.
const COVERED: &[&str] = &[
  "eslint:recommended",
  "plugin:react/recommended",
  "plugin:react/jsx-runtime",
  "plugin:react-hooks/recommended",
  "plugin:jsx-a11y/recommended",
  "plugin:@typescript-eslint/recommended",
  "plugin:@typescript-eslint/eslint-recommended",
  "plugin:vue/vue3-essential",
  "plugin:svelte/recommended",
  "plugin:markdown/recommended",
];

/// Keys of an ESLint configuration that have no counterpart, and why.
const UNREAD: &[(&str, &str)] = &[
  ("env", "fsr lint sees the builtins, `browser` and `es2026`"),
  ("globals", "fsr lint sees the builtins, `browser` and `es2026`"),
  ("languageOptions", "fsr lint parses every dialect it reads with its own settings"),
  ("parser", "fsr lint parses with oxc"),
  ("parserOptions", "fsr lint parses with oxc"),
  ("processor", "fsr lint reads `.vue`, `.svelte`, `.md` and `.mdx` itself"),
  ("excludedFiles", "an override applies to all of its `files`"),
];

#[derive(Default)]
struct Out {
  known: Vec<&'static str>,
  rules: toml::Table,
  overrides: Vec<toml::Table>,
  ignores: Vec<String>,
  settings: Option<toml::Value>,
  notes: Vec<String>,
}

impl Out {
  fn note(&mut self, note: String) {
    if !self.notes.contains(&note) {
      self.notes.push(note);
    }
  }

  fn knowing(rules: &[RuleInfo]) -> Self {
    Self { known: rules.iter().map(|r| r.id).collect(), ..Self::default() }
  }

  /// `rules` translated: each rule in `known` with its level and options, each other one a note.
  fn rules(&mut self, rules: &Value) -> toml::Table {
    let mut out = toml::Table::new();
    let Some(rules) = rules.as_object() else { return out };
    for (name, setting) in rules {
      let id = full_id(name);
      if !self.known.contains(&id.as_str()) && !(id.starts_with("svelte/") && id != "svelte/*") {
        self.note(format!("`{name}` has no counterpart in fsr lint and is not carried over"));
        continue;
      }
      match toml_of(setting) {
        Some(value) => {
          out.insert(id, value);
        }
        None => self.note(format!("`{name}`'s setting is not a level fsr lint reads")),
      }
    }
    out
  }

  fn unread(&mut self, config: &serde_json::Map<String, Value>) {
    for (key, why) in UNREAD {
      if config.contains_key(*key) {
        self.note(format!("`{key}` is not carried over: {why}"));
      }
    }
  }

  fn finish(self) -> Translation {
    let mut table = toml::Table::new();
    if !self.ignores.is_empty() {
      table.insert("ignores".to_owned(), toml::Value::Array(self.ignores.into_iter().map(toml::Value::String).collect()));
    }
    if let Some(settings) = self.settings {
      table.insert("settings".to_owned(), settings);
    }
    if !self.rules.is_empty() {
      table.insert("rules".to_owned(), toml::Value::Table(self.rules));
    }
    if !self.overrides.is_empty() {
      table.insert("overrides".to_owned(), toml::Value::Array(self.overrides.into_iter().map(toml::Value::Table).collect()));
    }
    Translation { toml: toml::to_string(&table).unwrap_or_default(), notes: self.notes }
  }
}

/// A JSON value as TOML; `null` has no TOML form.
fn toml_of(value: &Value) -> Option<toml::Value> {
  match value {
    Value::Null => None,
    Value::Array(items) => Some(toml::Value::Array(items.iter().filter_map(toml_of).collect())),
    Value::Object(map) => Some(toml::Value::Table(map.iter().filter_map(|(k, v)| toml_of(v).map(|v| (k.clone(), v))).collect())),
    Value::Bool(b) => Some(toml::Value::Boolean(*b)),
    Value::String(s) => Some(toml::Value::String(s.clone())),
    Value::Number(n) => n.as_i64().map(toml::Value::Integer).or_else(|| n.as_f64().map(toml::Value::Float)),
  }
}

/// An ESLint ignore pattern as a glob: a directory pattern covers what is under it.
fn glob_of(pattern: &str) -> String {
  match pattern.strip_suffix('/') {
    Some(dir) => format!("{dir}/**"),
    None => pattern.to_owned(),
  }
}

fn strings(value: &Value) -> Vec<String> {
  match value {
    Value::String(s) => vec![s.clone()],
    Value::Array(items) => items.iter().filter_map(Value::as_str).map(str::to_owned).collect(),
    _ => Vec::new(),
  }
}

/// An `.eslintrc` (JSON) or `package.json`'s `eslintConfig`, its rules carried over when `rules` has them:
/// [`crate::rules`] or a linter's [`crate::Linter::rules`] with its `Check`s among them.
pub fn translate_eslintrc(config: &Value, rules: &[RuleInfo]) -> Translation {
  let mut out = Out::knowing(rules);
  let Some(map) = config.as_object() else {
    out.note("the configuration is not an object".to_owned());
    return out.finish();
  };
  for preset in strings(map.get("extends").unwrap_or(&Value::Null)) {
    if !COVERED.contains(&preset.as_str()) {
      out.note(format!("`{preset}` is a preset fsr lint has no counterpart for; its rules are not carried over"));
    }
  }
  out.unread(map);
  if let Some(rules) = map.get("rules") {
    out.rules = out.rules(rules);
  }
  for block in map.get("overrides").and_then(Value::as_array).into_iter().flatten() {
    let Some(block) = block.as_object() else { continue };
    out.unread(block);
    let mut table = toml::Table::new();
    table.insert("files".to_owned(), toml::Value::Array(strings(block.get("files").unwrap_or(&Value::Null)).into_iter().map(toml::Value::String).collect()));
    let rules = out.rules(block.get("rules").unwrap_or(&Value::Null));
    table.insert("rules".to_owned(), toml::Value::Table(rules));
    out.overrides.push(table);
  }
  out.ignores = strings(map.get("ignorePatterns").unwrap_or(&Value::Null)).iter().map(|p| glob_of(p)).collect();
  out.settings = map.get("settings").and_then(toml_of);
  out.finish()
}

/// An expression's value when it is written out literally.
fn literal(expression: &Expression<'_>) -> Option<Value> {
  Some(match expression {
    Expression::StringLiteral(s) => Value::String(s.value.as_str()?.to_owned()),
    Expression::NumericLiteral(n) => serde_json::Number::from_f64(n.value).map(|n| match n.as_f64() {
      Some(f) if f.fract() == 0.0 => Value::Number((f as i64).into()),
      _ => Value::Number(n),
    })?,
    Expression::BooleanLiteral(b) => Value::Bool(b.value),
    Expression::NullLiteral(_) => Value::Null,
    Expression::TemplateLiteral(t) if t.expressions.is_empty() => Value::String(t.quasis.iter().map(|q| q.value.raw.as_str()).collect()),
    Expression::ArrayExpression(array) => Value::Array(array.elements.iter().map(|e| e.as_expression().and_then(literal)).collect::<Option<_>>()?),
    Expression::ObjectExpression(object) => {
      let mut map = serde_json::Map::new();
      for property in &object.properties {
        let ObjectPropertyKind::ObjectProperty(property) = property else { return None };
        let key = match &property.key {
          PropertyKey::StaticIdentifier(id) => id.name.to_string(),
          PropertyKey::StringLiteral(s) => s.value.as_str()?.to_owned(),
          _ => return None,
        };
        map.insert(key, literal(&property.value)?);
      }
      Value::Object(map)
    }
    Expression::ParenthesizedExpression(p) => literal(&p.expression)?,
    Expression::TSAsExpression(e) => literal(&e.expression)?,
    Expression::TSSatisfiesExpression(e) => literal(&e.expression)?,
    _ => return None,
  })
}

/// Whether the expression names a recommended preset whose rules `fsr lint` runs by default.
fn covered_preset(written: &str) -> bool {
  written.contains("configs.recommended") || written.contains("configs[\"recommended\"]") || written.contains("flat/recommended") || written.contains("recommended-latest") || written.contains("configs.flat.recommended") || written.contains("configs[\"flat/recommended\"]")
}

/// An `eslint.config.js`, `.mjs` or `.ts`, read without running it, its rules carried over when `rules` has them.
pub fn translate_flat(source: &str, path: &Path, rules: &[RuleInfo]) -> Translation {
  let mut out = Out::knowing(rules);
  let allocator = Allocator::default();
  let source_type = SourceType::from_path(path).unwrap_or_default();
  let parsed = Parser::new(&allocator, source, source_type).parse();
  let name = path.display().to_string();
  let default = parsed.program.body.iter().find_map(|statement| match statement {
    Statement::ExportDefaultDeclaration(export) => export.declaration.as_expression(),
    _ => None,
  });
  let Some(default) = default else {
    out.note(format!("{name} has no `export default` fsr lint can read"));
    return out.finish();
  };
  let text = |span: oxc_span::Span| source[span.start as usize..span.end as usize].to_owned();
  let array = match default {
    Expression::ArrayExpression(array) => Some(array),
    Expression::CallExpression(call) => match call.arguments.first().and_then(|a| a.as_expression()) {
      Some(Expression::ArrayExpression(array)) if call.arguments.len() == 1 => Some(array),
      _ => None,
    },
    _ => None,
  };
  let Some(array) = array else {
    out.note(format!("`{}` in {name} is not a list fsr lint can read without running it", text(default.span())));
    return out.finish();
  };
  let mut scoped = false;
  for element in &array.elements {
    let written = text(element.span());
    let object = match element {
      ArrayExpressionElement::SpreadElement(_) => None,
      other => other.as_expression().and_then(literal),
    };
    let Some(Value::Object(object)) = object else {
      if !covered_preset(&written) {
        out.note(format!("`{written}` in {name} is not carried over: fsr lint reads only what is written out literally"));
      }
      continue;
    };
    out.unread(&object);
    let files = strings(object.get("files").unwrap_or(&Value::Null));
    if let Some(settings) = object.get("settings").and_then(toml_of) {
      out.settings = Some(settings);
    }
    if object.len() == 1 && object.contains_key("ignores") {
      out.ignores.extend(strings(&object["ignores"]).iter().map(|p| glob_of(p)));
      continue;
    }
    let rules = out.rules(object.get("rules").unwrap_or(&Value::Null));
    if rules.is_empty() {
      continue;
    }
    if files.is_empty() && !scoped {
      out.rules.extend(rules);
      continue;
    }
    scoped = true;
    let mut table = toml::Table::new();
    let files = if files.is_empty() { vec!["**".to_owned()] } else { files };
    table.insert("files".to_owned(), toml::Value::Array(files.into_iter().map(toml::Value::String).collect()));
    table.insert("rules".to_owned(), toml::Value::Table(rules));
    out.overrides.push(table);
  }
  out.finish()
}
