//! A GraphQL API as a contract: `clients/<name>.graphql` holds the schema's
//! SDL and the named operations the application sends. Each operation is a
//! method whose parameters are its variables and whose answer is its
//! selection typed against the schema; a call posts the operation to the one
//! endpoint the API serves.

use std::collections::{BTreeSet, HashMap};
use std::time::Duration;

use apollo_compiler::executable::{ExecutableDocument, Fragment, Operation, OperationType, Selection, SelectionSet};
use apollo_compiler::schema::ExtendedType;
use apollo_compiler::{ast, Node, Schema};
use futures_util::future::BoxFuture;
use indexmap::IndexMap;
use snapfire_fsr_core::Value;
use snapfire_fsr_payload::json_to_value;
use snapfire_fsr_runtime::{FailureKind, ServiceError};

use crate::call::Call;
use crate::contract::{Contract, Field, Method, Service, Type, TypeDef, Variant};
use crate::http::{kind_for_status, replace_lone_surrogates};
use crate::openapi::ImportError;
use crate::transport::Transport;

/// One operation as it goes out: its name and a document holding it and the
/// fragments it spreads.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct GraphqlOperation {
  pub name: String,
  pub document: String,
}

/// What a `.graphql` document lowers to: the contract and, by method, the
/// operation each sends.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct ImportedGraphql {
  pub contract: Contract,
  pub service: String,
  pub operations: IndexMap<String, GraphqlOperation>,
}

fn malformed(at: impl Into<String>, what: impl Into<String>) -> ImportError {
  ImportError::Malformed { at: at.into(), what: what.into() }
}

fn unsupported(at: impl Into<String>, what: impl Into<String>) -> ImportError {
  ImportError::Unsupported { at: at.into(), what: what.into() }
}

/// Reads `source`, the schema and its operations in one document, as the
/// service `service`. Every operation is validated against the schema first.
pub fn import_graphql(source: &str, service: &str) -> Result<ImportedGraphql, ImportError> {
  let (schema, document) = apollo_compiler::parser::Parser::new()
    .parse_mixed_validate(source, format!("{service}.graphql"))
    .map_err(|errors| malformed(format!("{service}.graphql"), errors.to_string()))?;
  if document.operations.anonymous.is_some() {
    return Err(unsupported(format!("{service}.graphql"), "an anonymous operation; name it, since its name is the method's"));
  }
  let mut import = Import { schema: &schema, document: &document, types: IndexMap::new() };
  let mut methods = Service::new();
  let mut operations = IndexMap::new();
  for (name, operation) in &document.operations.named {
    let at = format!("{service}.graphql: {name}");
    let suffix = match operation.operation_type {
      OperationType::Query => "Query",
      OperationType::Mutation => "Mutation",
      OperationType::Subscription => return Err(unsupported(at, "a subscription")),
    };
    let mut params = Vec::new();
    for variable in &operation.variables {
      let ty = import.input(&variable.ty, &at)?;
      let ty = if variable.default_value.is_some() && !matches!(ty, Type::Optional(_)) { Type::optional(ty) } else { ty };
      params.push(Field::new(variable.name.as_str(), ty));
    }
    let answer = format!("{name}{suffix}");
    import.selection(&answer, &operation.selection_set, &at)?;
    let method = lower_first(name.as_str());
    methods = methods.method(method.clone(), Method::new(params, Type::named(answer)));
    operations.insert(method, GraphqlOperation { name: name.to_string(), document: alone(&document, operation) });
  }
  let mut contract = Contract::new().service(service, methods);
  contract.types = import.types;
  Ok(ImportedGraphql { contract, service: service.to_owned(), operations })
}

struct Import<'d> {
  schema: &'d Schema,
  document: &'d ExecutableDocument,
  types: IndexMap<String, TypeDef>,
}

impl Import<'_> {
  /// A variable's type: a nullable one is optional and may be left out.
  fn input(&mut self, ty: &ast::Type, at: &str) -> Result<Type, ImportError> {
    let inner = match ty {
      ast::Type::Named(name) | ast::Type::NonNullNamed(name) => self.input_named(name.as_str(), at)?,
      ast::Type::List(item) | ast::Type::NonNullList(item) => Type::list(self.input(item, at)?),
    };
    Ok(if ty.is_non_null() { inner } else { Type::optional(inner) })
  }

  fn input_named(&mut self, name: &str, at: &str) -> Result<Type, ImportError> {
    if let Some(scalar) = self.scalar(name) {
      return Ok(scalar);
    }
    match self.schema.types.get(name) {
      Some(ExtendedType::Enum(e)) => {
        self.enumeration(name, e.values.keys().map(|v| v.as_str()));
        Ok(Type::named(name))
      }
      Some(ExtendedType::InputObject(input)) => {
        if !self.types.contains_key(name) {
          self.types.insert(name.to_owned(), TypeDef::Record { fields: Vec::new() });
          let mut fields = Vec::new();
          for (field, def) in &input.fields {
            let ty = self.input(&def.ty, at)?;
            let ty = if def.default_value.is_some() && !matches!(ty, Type::Optional(_)) { Type::optional(ty) } else { ty };
            fields.push(Field::new(field.as_str(), ty));
          }
          self.types.insert(name.to_owned(), TypeDef::Record { fields });
        }
        Ok(Type::named(name))
      }
      _ => Err(malformed(at, format!("`{name}` is not an input type"))),
    }
  }

  /// A built-in scalar's type; a custom scalar travels as its text.
  fn scalar(&self, name: &str) -> Option<Type> {
    match name {
      "Int" => Some(Type::I32),
      "Float" => Some(Type::F64),
      "String" | "ID" => Some(Type::Str),
      "Boolean" => Some(Type::Bool),
      _ => matches!(self.schema.types.get(name), Some(ExtendedType::Scalar(_))).then_some(Type::Str),
    }
  }

  fn enumeration<'v>(&mut self, name: &str, values: impl Iterator<Item = &'v str>) {
    if !self.types.contains_key(name) {
      self.types.insert(name.to_owned(), TypeDef::Union { variants: values.map(Variant::unit).collect() });
    }
  }

  /// The record `name` for `selection`: one present field per response key,
  /// null where the schema allows it.
  fn selection(&mut self, name: &str, selection: &SelectionSet, at: &str) -> Result<(), ImportError> {
    let mut fields: Vec<Field> = Vec::new();
    self.fields_into(name, selection, &mut fields, &mut Vec::new(), at)?;
    self.types.insert(name.to_owned(), TypeDef::Record { fields });
    Ok(())
  }

  fn fields_into(&mut self, name: &str, selection: &SelectionSet, fields: &mut Vec<Field>, spread: &mut Vec<String>, at: &str) -> Result<(), ImportError> {
    for item in &selection.selections {
      match item {
        Selection::Field(field) => {
          let key = field.response_key().as_str();
          if fields.iter().any(|f| f.name == key) {
            continue;
          }
          let ty = if field.name == "__typename" {
            Type::Str
          } else {
            self.output(&format!("{name}{}", upper_first(key)), field.ty(), &field.selection_set, at)?
          };
          fields.push(Field::new(key, ty).present(true));
        }
        Selection::FragmentSpread(spread_of) => {
          let fragment: &Node<Fragment> = self.document.fragments.get(&spread_of.fragment_name).ok_or_else(|| malformed(at, format!("no fragment `{}`", spread_of.fragment_name)))?;
          self.same_type(fragment.type_condition().as_str(), selection.ty.as_str(), at)?;
          if !spread.contains(&fragment.name.to_string()) {
            spread.push(fragment.name.to_string());
            self.fields_into(name, &fragment.selection_set, fields, spread, at)?;
          }
        }
        Selection::InlineFragment(inline) => {
          if let Some(condition) = &inline.type_condition {
            self.same_type(condition.as_str(), selection.ty.as_str(), at)?;
          }
          self.fields_into(name, &inline.selection_set, fields, spread, at)?;
        }
      }
    }
    Ok(())
  }

  /// A fragment on another type than the one it is spread in selects by the
  /// answer's runtime type, which a record cannot say.
  fn same_type(&self, condition: &str, on: &str, at: &str) -> Result<(), ImportError> {
    if condition == on {
      return Ok(());
    }
    Err(unsupported(at, format!("a fragment on `{condition}` inside a selection on `{on}`, which picks fields by the answer's type")))
  }

  fn output(&mut self, hint: &str, ty: &ast::Type, selection: &SelectionSet, at: &str) -> Result<Type, ImportError> {
    let inner = match ty {
      ast::Type::Named(named) | ast::Type::NonNullNamed(named) => {
        let named = named.as_str();
        if let Some(scalar) = self.scalar(named) {
          scalar
        } else {
          match self.schema.types.get(named) {
            Some(ExtendedType::Enum(e)) => {
              self.enumeration(named, e.values.keys().map(|v| v.as_str()));
              Type::named(named)
            }
            Some(ExtendedType::Object(_)) => {
              self.selection(hint, selection, at)?;
              Type::named(hint)
            }
            Some(ExtendedType::Interface(_) | ExtendedType::Union(_)) => {
              let concrete = selection.selections.iter().all(|s| match s {
                Selection::Field(_) => true,
                Selection::FragmentSpread(f) => self.document.fragments.get(&f.fragment_name).is_some_and(|f| f.type_condition() == &selection.ty),
                Selection::InlineFragment(i) => i.type_condition.as_ref().is_none_or(|c| c == &selection.ty),
              });
              if !concrete {
                return Err(unsupported(at, format!("a selection on `{named}` that picks fields by the answer's type")));
              }
              self.selection(hint, selection, at)?;
              Type::named(hint)
            }
            _ => return Err(malformed(at, format!("`{named}` is not an output type"))),
          }
        }
      }
      ast::Type::List(item) | ast::Type::NonNullList(item) => Type::list(self.output(hint, item, selection, at)?),
    };
    Ok(if ty.is_non_null() { inner } else { Type::optional(inner) })
  }
}

/// `operation` and the fragments it spreads, directly or through another
/// fragment, as the text a server is sent.
fn alone(document: &ExecutableDocument, operation: &Node<Operation>) -> String {
  let mut used: BTreeSet<String> = BTreeSet::new();
  let mut pending: Vec<&SelectionSet> = vec![&operation.selection_set];
  while let Some(selection) = pending.pop() {
    for item in &selection.selections {
      match item {
        Selection::Field(field) => pending.push(&field.selection_set),
        Selection::InlineFragment(inline) => pending.push(&inline.selection_set),
        Selection::FragmentSpread(spread) => {
          if used.insert(spread.fragment_name.to_string()) {
            if let Some(fragment) = document.fragments.get(&spread.fragment_name) {
              pending.push(&fragment.selection_set);
            }
          }
        }
      }
    }
  }
  let mut out = ExecutableDocument::new();
  if let Some(name) = &operation.name {
    out.operations.named.insert(name.clone(), operation.clone());
  }
  for (name, fragment) in &document.fragments {
    if used.contains(name.as_str()) {
      out.fragments.insert(name.clone(), fragment.clone());
    }
  }
  out.to_string()
}

fn lower_first(s: &str) -> String {
  let mut c = s.chars();
  c.next().map(|f| f.to_lowercase().collect::<String>() + c.as_str()).unwrap_or_default()
}

fn upper_first(s: &str) -> String {
  let mut c = s.chars();
  c.next().map(|f| f.to_uppercase().collect::<String>() + c.as_str()).unwrap_or_default()
}

/// Posts each call's operation to the API's one endpoint as
/// `{ query, variables, operationName }` and answers its `data`. An answer
/// carrying `errors` fails the call with the first one's message, its kind
/// from `extensions.code` where the server sets one.
pub struct GraphqlTransport {
  client: reqwest::Client,
  endpoint: String,
  operations: HashMap<String, GraphqlOperation>,
  headers: Vec<(String, String)>,
  query: Vec<(String, String)>,
}

impl GraphqlTransport {
  pub fn new(endpoint: impl Into<String>, imported: &ImportedGraphql) -> Self {
    Self::with_client(endpoint, imported, reqwest::Client::builder().timeout(Duration::from_secs(30)).build().unwrap_or_default())
  }

  pub fn with_client(endpoint: impl Into<String>, imported: &ImportedGraphql, client: reqwest::Client) -> Self {
    let operations = imported.operations.iter().map(|(method, op)| (format!("{}.{method}", imported.service), op.clone())).collect();
    Self { client, endpoint: endpoint.into(), operations, headers: Vec::new(), query: Vec::new() }
  }

  pub fn header(mut self, name: impl Into<String>, value: impl Into<String>) -> Self {
    self.headers.push((name.into(), value.into()));
    self
  }

  pub fn query(mut self, name: impl Into<String>, value: impl Into<String>) -> Self {
    self.query.push((name.into(), value.into()));
    self
  }
}

/// A variable as GraphQL reads it: an enum value by its name.
fn variable_json(value: &Value) -> serde_json::Value {
  match value {
    Value::Variant { tag, payload: None } => serde_json::Value::String(tag.clone()),
    Value::Variant { payload: Some(payload), .. } => variable_json(payload),
    Value::Seq(items) => serde_json::Value::Array(items.iter().map(variable_json).collect()),
    Value::Map(map) => serde_json::Value::Object(map.iter().map(|(k, v)| (k.clone(), variable_json(v))).collect()),
    other => snapfire_fsr_payload::value_to_json(other),
  }
}

/// The kind an error's `extensions.code` names, by the codes Apollo Server
/// and most others use; `Internal` without one.
fn kind_for_code(code: Option<&str>) -> FailureKind {
  match code {
    Some("NOT_FOUND") => FailureKind::NotFound,
    Some("UNAUTHENTICATED") => FailureKind::Unauthorized,
    Some("FORBIDDEN") => FailureKind::Forbidden,
    Some("BAD_USER_INPUT" | "GRAPHQL_VALIDATION_FAILED" | "GRAPHQL_PARSE_FAILED" | "BAD_REQUEST") => FailureKind::Invalid,
    _ => FailureKind::Internal,
  }
}

impl Transport for GraphqlTransport {
  fn call(&self, call: Call) -> BoxFuture<'static, Result<Value, ServiceError>> {
    let service = call.service.clone();
    let name = call.method.clone();
    let fail = move |kind: FailureKind, message: String| ServiceError::new(kind, service.clone(), name.clone(), message);
    let Some(operation) = self.operations.get(&format!("{}.{}", call.service, call.method)) else {
      let why = format!("no operation answers `{}.{}`", call.service, call.method);
      return Box::pin(async move { Err(fail(FailureKind::Internal, why)) });
    };
    let variables: serde_json::Map<String, serde_json::Value> = call.args.iter().map(|(k, v)| (k.clone(), variable_json(v))).collect();
    let body = serde_json::json!({ "query": operation.document, "variables": variables, "operationName": operation.name });
    let mut request = self.client.post(&self.endpoint).json(&body);
    for (key, value) in &self.headers {
      request = request.header(key.as_str(), value.as_str());
    }
    for (key, value) in &call.metadata {
      let Value::Str(value) = value else {
        let why = format!("metadata `{key}` is not a string, so it cannot be a header");
        return Box::pin(async move { Err(fail(FailureKind::Internal, why)) });
      };
      request = request.header(key.as_str(), value.as_str());
    }
    if !self.query.is_empty() {
      request = request.query(&self.query);
    }
    Box::pin(async move {
      let response = request.send().await.map_err(|e| {
        tracing::warn!(target: "fsr::service", error = %e, "backend request failed");
        match e.is_timeout() {
          true => fail(FailureKind::Timeout, "the backend timed out".to_owned()),
          false => fail(FailureKind::Unavailable, "the backend did not answer".to_owned()),
        }
      })?;
      let status = response.status().as_u16();
      let text = response.text().await.map_err(|e| fail(FailureKind::Internal, e.to_string()))?;
      let json: Option<serde_json::Value> = serde_json::from_str(&replace_lone_surrogates(&text)).ok();
      if let Some(first) = json.as_ref().and_then(|j| j.get("errors")).and_then(|e| e.as_array()).and_then(|e| e.first()) {
        let message = first.get("message").and_then(|m| m.as_str()).unwrap_or("the API answered an error").to_owned();
        let code = first.get("extensions").and_then(|e| e.get("code")).and_then(|c| c.as_str());
        return Err(fail(kind_for_code(code), message));
      }
      if !(200..300).contains(&status) {
        tracing::warn!(target: "fsr::service", status, body = %text.trim(), "backend answered with a failure");
        return Err(fail(kind_for_status(status), status.to_string()));
      }
      let Some(data) = json.as_ref().and_then(|j| j.get("data")) else {
        return Err(fail(FailureKind::Internal, "the answer carries no `data`".to_owned()));
      };
      json_to_value(data).map_err(|e| fail(FailureKind::Internal, e.to_string()))
    })
  }
}
