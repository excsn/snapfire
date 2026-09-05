use indexmap::IndexMap;
use serde_json::{Map, Value as Json};

use crate::contract::{Contract, Field, Method, Service, Type, TypeDef, Variant, Freshness};
use crate::http::Route;

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ImportError {
  #[error("{at}: {what}")]
  Malformed { at: String, what: String },
  #[error("{at}: {what} is not supported")]
  Unsupported { at: String, what: String },
}

fn malformed(at: impl Into<String>, what: impl Into<String>) -> ImportError {
  ImportError::Malformed { at: at.into(), what: what.into() }
}

fn unsupported(at: impl Into<String>, what: impl Into<String>) -> ImportError {
  ImportError::Unsupported { at: at.into(), what: what.into() }
}

/// What one document lowers to: the neutral contract, the transport shape the
/// contract deliberately does not carry, and the server the document names.
#[derive(Debug, Clone)]
pub struct Imported {
  pub contract: Contract,
  pub routes: Vec<(String, Route)>,
  pub base_url: Option<String>,
}

struct Import<'d> {
  components: &'d Map<String, Json>,
  types: IndexMap<String, TypeDef>,
  aliases: IndexMap<String, Type>,
  taken: Vec<String>,
}

fn pascal(input: &str) -> String {
  let mut out = String::new();
  let mut upper = true;
  for c in input.chars() {
    if c.is_alphanumeric() {
      if upper {
        out.extend(c.to_uppercase());
        upper = false;
      } else {
        out.push(c);
      }
    } else {
      upper = true;
    }
  }
  out
}

impl<'d> Import<'d> {
  fn name_for(&mut self, hint: &str) -> String {
    let base = pascal(hint);
    let base = if base.is_empty() { "Anonymous".to_owned() } else { base };
    if !self.taken.contains(&base) {
      self.taken.push(base.clone());
      return base;
    }
    for n in 2.. {
      let candidate = format!("{base}{n}");
      if !self.taken.contains(&candidate) {
        self.taken.push(candidate.clone());
        return candidate;
      }
    }
    unreachable!()
  }

  fn object(&mut self, schema: &Map<String, Json>, at: &str) -> Result<Vec<Field>, ImportError> {
    let required: Vec<&str> = schema
      .get("required")
      .and_then(Json::as_array)
      .map(|r| r.iter().filter_map(Json::as_str).collect())
      .unwrap_or_default();

    let mut fields = Vec::new();
    if let Some(properties) = schema.get("properties").and_then(Json::as_object) {
      for (name, property) in properties {
        let at = format!("{at}/properties/{name}");
        let ty = self.ty(property, &at, name)?;
        let ty = if required.contains(&name.as_str()) { ty } else { Type::optional(ty) };
        fields.push(Field::new(name.clone(), ty));
      }
    }
    Ok(fields)
  }

  /// `allOf` is flattened: every branch must be an object or a reference to
  /// one, and their fields merge with later branches winning.
  fn all_of(&mut self, branches: &[Json], at: &str, hint: &str) -> Result<Vec<Field>, ImportError> {
    let mut merged: IndexMap<String, Field> = IndexMap::new();
    for (i, branch) in branches.iter().enumerate() {
      let at = format!("{at}/allOf/{i}");
      let resolved = self.resolve(branch, &at)?;
      let fields = match resolved.get("allOf").and_then(Json::as_array) {
        Some(inner) => self.all_of(inner, &at, hint)?,
        None => self.object(&resolved, &at)?,
      };
      for field in fields {
        merged.insert(field.name.clone(), field);
      }
    }
    Ok(merged.into_values().collect())
  }

  fn resolve(&self, schema: &Json, at: &str) -> Result<Map<String, Json>, ImportError> {
    let object = schema
      .as_object()
      .ok_or_else(|| malformed(at, "a schema must be an object"))?;
    let Some(reference) = object.get("$ref").and_then(Json::as_str) else {
      return Ok(object.clone());
    };
    let name = reference
      .strip_prefix("#/components/schemas/")
      .ok_or_else(|| unsupported(at, format!("the reference `{reference}`")))?;
    let target = self
      .components
      .get(name)
      .ok_or_else(|| malformed(at, format!("`{reference}` names no schema")))?;
    target
      .as_object()
      .cloned()
      .ok_or_else(|| malformed(at, format!("`{reference}` is not an object")))
  }

  fn scalar(&self, schema: &Map<String, Json>, kind: &str, at: &str) -> Result<Type, ImportError> {
    let format = schema.get("format").and_then(Json::as_str);
    Ok(match (kind, format) {
      ("string", Some("byte" | "binary")) => Type::Bytes,
      ("string", _) => Type::Str,
      ("boolean", _) => Type::Bool,
      ("integer", Some("int32")) => Type::I32,
      ("integer", Some("uint32")) => Type::U32,
      ("integer", Some("uint64")) => Type::U64,
      ("integer", _) => Type::I64,
      ("number", Some("float")) => Type::F32,
      ("number", _) => Type::F64,
      ("null", _) => Type::Null,
      _ => return Err(unsupported(at, format!("the type `{kind}`"))),
    })
  }

  fn ty(&mut self, schema: &Json, at: &str, hint: &str) -> Result<Type, ImportError> {
    if let Some(reference) = schema.get("$ref").and_then(Json::as_str) {
      let name = reference
        .strip_prefix("#/components/schemas/")
        .ok_or_else(|| unsupported(at, format!("the reference `{reference}`")))?;
      if self.types.contains_key(name) {
        return Ok(Type::named(name));
      }
      if let Some(alias) = self.aliases.get(name) {
        return Ok(alias.clone());
      }
      let target = self.resolve(schema, at)?;
      return self.register(name, &target, &format!("#/components/schemas/{name}"));
    }

    let object = schema
      .as_object()
      .ok_or_else(|| malformed(at, "a schema must be an object"))?;

    // 3.1 writes a nullable type as a union with null; 3.0 writes a flag.
    if object.get("nullable").and_then(Json::as_bool) == Some(true) {
      let mut inner = object.clone();
      inner.remove("nullable");
      return Ok(Type::optional(self.ty(&Json::Object(inner), at, hint)?));
    }
    if let Some(types) = object.get("type").and_then(Json::as_array) {
      let named: Vec<&str> = types.iter().filter_map(Json::as_str).collect();
      let rest: Vec<&&str> = named.iter().filter(|t| **t != "null").collect();
      if named.contains(&"null") && rest.len() == 1 {
        let mut inner = object.clone();
        inner.insert("type".to_owned(), Json::String((*rest[0]).to_owned()));
        return Ok(Type::optional(self.ty(&Json::Object(inner), at, hint)?));
      }
      return Err(unsupported(at, "a union of more than one non-null type"));
    }

    if object.contains_key("allOf") || object.contains_key("oneOf") || object.contains_key("anyOf")
      || object.get("enum").is_some()
      || object.get("type").and_then(Json::as_str) == Some("object")
    {
      let name = self.name_for(hint);
      return self.register(&name, object, at);
    }

    if let Some(items) = object.get("items") {
      let at = format!("{at}/items");
      return Ok(Type::list(self.ty(items, &at, &format!("{hint}Item"))?));
    }

    let Some(kind) = object.get("type").and_then(Json::as_str) else {
      return Err(unsupported(at, "a schema with no `type`, `$ref` or composition"));
    };
    if kind == "array" {
      return Err(malformed(at, "an array schema needs `items`"));
    }
    self.scalar(object, kind, at)
  }

  /// Registers a named type and returns how to refer to it. A schema that is
  /// not a record or a union becomes an alias instead, since the contract
  /// names only those two.
  fn register(&mut self, name: &str, schema: &Map<String, Json>, at: &str) -> Result<Type, ImportError> {
    if self.types.contains_key(name) {
      return Ok(Type::named(name));
    }
    if let Some(alias) = self.aliases.get(name) {
      return Ok(alias.clone());
    }
    if !self.taken.contains(&name.to_owned()) {
      self.taken.push(name.to_owned());
    }

    if let Some(values) = schema.get("enum").and_then(Json::as_array) {
      let mut variants = Vec::new();
      for value in values {
        let tag = value
          .as_str()
          .ok_or_else(|| unsupported(at, "an enum of values that are not strings"))?;
        variants.push(Variant::unit(tag));
      }
      self.types.insert(name.to_owned(), TypeDef::Union { variants });
      return Ok(Type::named(name));
    }

    for key in ["oneOf", "anyOf"] {
      if let Some(branches) = schema.get(key).and_then(Json::as_array) {
        // Reserve the name before the branches, since a branch may point back.
        self.types.insert(name.to_owned(), TypeDef::Union { variants: Vec::new() });
        let mut variants = Vec::new();
        for (i, branch) in branches.iter().enumerate() {
          let at = format!("{at}/{key}/{i}");
          let tag = branch
            .get("$ref")
            .and_then(Json::as_str)
            .and_then(|r| r.rsplit('/').next())
            .map(str::to_owned)
            .unwrap_or_else(|| format!("case{i}"));
          let payload = self.ty(branch, &at, &format!("{name}{}", pascal(&tag)))?;
          variants.push(Variant::with(tag, payload));
        }
        self.types.insert(name.to_owned(), TypeDef::Union { variants });
        return Ok(Type::named(name));
      }
    }

    if let Some(branches) = schema.get("allOf").and_then(Json::as_array) {
      self.types.insert(name.to_owned(), TypeDef::Record { fields: Vec::new() });
      let fields = self.all_of(branches, at, name)?;
      self.types.insert(name.to_owned(), TypeDef::Record { fields });
      return Ok(Type::named(name));
    }

    let kind = schema.get("type").and_then(Json::as_str);
    if kind == Some("object") || schema.contains_key("properties") {
      if let Some(additional) = schema.get("additionalProperties") {
        if schema.get("properties").is_none() {
          return match additional {
            Json::Bool(_) => Err(unsupported(at, "`additionalProperties: true`, which has no element type")),
            other => {
              let at = format!("{at}/additionalProperties");
              let inner = self.ty(other, &at, &format!("{name}Value"))?;
              let alias = Type::map(inner);
              self.aliases.insert(name.to_owned(), alias.clone());
              Ok(alias)
            }
          };
        }
      }
      // Reserved before the fields, so a self-referential schema resolves.
      self.types.insert(name.to_owned(), TypeDef::Record { fields: Vec::new() });
      let fields = self.object(schema, at)?;
      self.types.insert(name.to_owned(), TypeDef::Record { fields });
      return Ok(Type::named(name));
    }

    let alias = self.ty(&Json::Object(schema.clone()), at, name)?;
    self.aliases.insert(name.to_owned(), alias.clone());
    Ok(alias)
  }
}

fn success_schema<'r>(responses: &'r Map<String, Json>) -> Option<(&'r str, &'r Json)> {
  let mut codes: Vec<&String> = responses.keys().collect();
  codes.sort();
  for code in codes {
    let is_success = code.starts_with('2') || code == "default";
    if !is_success {
      continue;
    }
    let body = responses[code]
      .get("content")
      .and_then(|c| c.get("application/json"))
      .and_then(|j| j.get("schema"));
    if let Some(schema) = body {
      return Some((code.as_str(), schema));
    }
    if code.starts_with('2') {
      return Some((code.as_str(), &Json::Null));
    }
  }
  None
}

fn operation_name(operation: &Map<String, Json>, verb: &str, path: &str) -> String {
  if let Some(id) = operation.get("operationId").and_then(Json::as_str) {
    return id.to_owned();
  }
  let mut name = verb.to_lowercase();
  for segment in path.split('/').filter(|s| !s.is_empty()) {
    name.push_str(&pascal(segment.trim_matches(|c| c == '{' || c == '}')));
  }
  name
}

/// Lowers one OpenAPI document into a contract plus the routes its transport
/// needs. Operations are grouped by their first tag; an operation with no tag
/// joins `default_service`.
pub fn import(document: &str, default_service: &str) -> Result<Imported, ImportError> {
  let doc: Json = serde_json::from_str(document)
    .map_err(|e| malformed("#", format!("the document is not JSON: {e}")))?;
  let doc = doc.as_object().ok_or_else(|| malformed("#", "the document is not an object"))?;

  let empty = Map::new();
  let components = doc
    .get("components")
    .and_then(|c| c.get("schemas"))
    .and_then(Json::as_object)
    .unwrap_or(&empty);

  let mut import = Import {
    components,
    types: IndexMap::new(),
    aliases: IndexMap::new(),
    taken: components.keys().cloned().collect(),
  };

  let base_url = doc
    .get("servers")
    .and_then(Json::as_array)
    .and_then(|s| s.first())
    .and_then(|s| s.get("url"))
    .and_then(Json::as_str)
    .map(str::to_owned);

  let paths = doc
    .get("paths")
    .and_then(Json::as_object)
    .ok_or_else(|| malformed("#", "the document has no `paths`"))?;

  let mut services: IndexMap<String, IndexMap<String, Method>> = IndexMap::new();
  let mut routes = Vec::new();

  for (path, item) in paths {
    let item = item
      .as_object()
      .ok_or_else(|| malformed(format!("#/paths/{path}"), "a path item must be an object"))?;
    let shared: Vec<Json> = item
      .get("parameters")
      .and_then(Json::as_array)
      .cloned()
      .unwrap_or_default();

    for verb in ["get", "put", "post", "delete", "patch"] {
      let Some(operation) = item.get(verb).and_then(Json::as_object) else { continue };
      let at = format!("#/paths/{path}/{verb}");
      let name = operation_name(operation, verb, path);
      let service = operation
        .get("tags")
        .and_then(Json::as_array)
        .and_then(|t| t.first())
        .and_then(Json::as_str)
        .unwrap_or(default_service)
        .to_owned();

      let mut params: Vec<Field> = Vec::new();
      let declared: Vec<Json> = shared
        .iter()
        .cloned()
        .chain(operation.get("parameters").and_then(Json::as_array).cloned().unwrap_or_default())
        .collect();

      for parameter in &declared {
        let parameter = parameter
          .as_object()
          .ok_or_else(|| malformed(&at, "a parameter must be an object"))?;
        let pname = parameter
          .get("name")
          .and_then(Json::as_str)
          .ok_or_else(|| malformed(&at, "a parameter needs a `name`"))?;
        let location = parameter.get("in").and_then(Json::as_str).unwrap_or("query");
        if !matches!(location, "path" | "query") {
          return Err(unsupported(&at, format!("a `{location}` parameter")));
        }
        let schema = parameter
          .get("schema")
          .ok_or_else(|| unsupported(&at, format!("the parameter `{pname}` without a `schema`")))?;
        let ty = import.ty(schema, &format!("{at}/parameters/{pname}"), pname)?;
        let required = parameter.get("required").and_then(Json::as_bool).unwrap_or(false)
          || location == "path";
        params.push(Field::new(pname, if required { ty } else { Type::optional(ty) }));
      }

      if let Some(body) = operation.get("requestBody") {
        let at = format!("{at}/requestBody");
        let schema = body
          .get("content")
          .and_then(|c| c.get("application/json"))
          .and_then(|j| j.get("schema"))
          .ok_or_else(|| unsupported(&at, "a request body that is not `application/json`"))?;
        let resolved = import.resolve(schema, &at)?;
        let body_required = body.get("required").and_then(Json::as_bool).unwrap_or(false);
        let is_object = resolved.get("type").and_then(Json::as_str) == Some("object")
          || resolved.contains_key("properties");
        if is_object && !resolved.contains_key("additionalProperties") {
          // A JSON object body spreads into named arguments, so a call reads
          // as arguments rather than one opaque envelope.
          for field in import.object(&resolved, &at)? {
            let ty = if body_required { field.ty } else { Type::optional(field.ty) };
            params.push(Field::new(field.name, ty));
          }
        } else {
          let ty = import.ty(schema, &at, &format!("{name}Body"))?;
          params.push(Field::new("body", if body_required { ty } else { Type::optional(ty) }));
        }
      }

      let responses = operation
        .get("responses")
        .and_then(Json::as_object)
        .ok_or_else(|| malformed(&at, "an operation needs `responses`"))?;
      let returns = match success_schema(responses) {
        Some((_, Json::Null)) | None => Type::Null,
        Some((code, schema)) => import.ty(schema, &format!("{at}/responses/{code}"), &format!("{name}Result"))?,
      };

      let mut method = Method::new(params, returns);
      if let Some(cache) = operation.get("x-sf-cache") {
        let freshness: Freshness = serde_json::from_value(cache.clone()).map_err(|e| malformed(&format!("{at}/x-sf-cache"), e.to_string()))?;
        method = method.cached(freshness);
      }
      if let Some(writes) = operation.get("x-sf-writes") {
        let tags: Vec<String> = serde_json::from_value(writes.clone()).map_err(|e| malformed(&format!("{at}/x-sf-writes"), e.to_string()))?;
        method = method.writes(tags);
      }
      services.entry(service.clone()).or_default().insert(name.clone(), method);
      routes.push((format!("{service}.{name}"), Route::new(verb.to_uppercase(), path.clone())));
    }
  }

  let mut contract = Contract::new();
  for (name, def) in import.types {
    contract = contract.define(name, def);
  }
  for (name, methods) in services {
    contract = contract.service(name, Service { methods });
  }
  contract.validate()?;

  Ok(Imported { contract, routes, base_url })
}

impl From<crate::check::ContractError> for ImportError {
  fn from(error: crate::check::ContractError) -> Self {
    malformed("#", error.to_string())
  }
}
