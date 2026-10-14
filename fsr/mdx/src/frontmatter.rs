//! YAML or TOML frontmatter as the JSON that `export const meta` is written as,
//! the way `remark-mdx-frontmatter` makes it.

use serde_json::{Map, Number, Value};
use yaml_rust2::{Yaml, YamlLoader};

pub(crate) fn yaml(text: &str) -> Result<Value, String> {
  let docs = YamlLoader::load_from_str(text).map_err(|e| e.to_string())?;
  match docs.into_iter().next() {
    None => Ok(Value::Object(Map::new())),
    Some(doc) => from_yaml(doc),
  }
}

fn from_yaml(yaml: Yaml) -> Result<Value, String> {
  Ok(match yaml {
    Yaml::Null | Yaml::BadValue => Value::Null,
    Yaml::Boolean(b) => Value::Bool(b),
    Yaml::Integer(i) => Value::Number(i.into()),
    Yaml::Real(text) => match text.parse::<f64>().ok().and_then(Number::from_f64) {
      Some(n) => Value::Number(n),
      None => Value::String(text),
    },
    Yaml::String(s) => Value::String(s),
    Yaml::Array(items) => Value::Array(items.into_iter().map(from_yaml).collect::<Result<_, _>>()?),
    Yaml::Hash(hash) => {
      let mut out = Map::new();
      for (key, value) in hash {
        let key = match key {
          Yaml::String(s) => s,
          Yaml::Integer(i) => i.to_string(),
          Yaml::Boolean(b) => b.to_string(),
          Yaml::Real(r) => r,
          other => return Err(format!("a frontmatter key is {other:?}; write a string")),
        };
        out.insert(key, from_yaml(value)?);
      }
      Value::Object(out)
    }
    Yaml::Alias(_) => return Err("an alias in frontmatter; write the value out".to_owned()),
  })
}

pub(crate) fn toml(text: &str) -> Result<Value, String> {
  let table: toml::Table = text.parse().map_err(|e: toml::de::Error| e.message().to_owned())?;
  serde_json::to_value(table).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn yaml_becomes_json() {
    let value = yaml("title: Hello\ntags: [a, b]\ncount: 3\nratio: 0.5\ndraft: false\nnested:\n  key: v\n").expect("yaml");
    assert_eq!(value, serde_json::json!({ "title": "Hello", "tags": ["a", "b"], "count": 3, "ratio": 0.5, "draft": false, "nested": { "key": "v" } }));
    assert_eq!(yaml("").expect("empty"), serde_json::json!({}));
    assert!(yaml("a: [1, 2").is_err());
  }

  #[test]
  fn toml_becomes_json() {
    let value = toml("title = \"Hello\"\ntags = [\"a\"]\n").expect("toml");
    assert_eq!(value, serde_json::json!({ "title": "Hello", "tags": ["a"] }));
  }
}
