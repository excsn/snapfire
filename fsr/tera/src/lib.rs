use base64::engine::general_purpose::STANDARD as B64;
use base64::Engine as _;
use futures_util::stream;
use snapfire_fsr_core::{Data, ModuleId, Node, SlotName, Value};
use snapfire_fsr_payload::{json_to_value, value_to_json};
use snapfire_fsr_runtime::{Chunk, EvalError, Evaluator, NodeChunks};
use tera::{Kwargs, State, Tera};

/// Delimits marker tokens in rendered output. Private-use codepoint, so it
/// cannot collide with template content and survives HTML escaping untouched.
pub const MARKER: char = '\u{F8FF}';

fn marker(token: &str) -> String {
  format!("{MARKER}{token}{MARKER}")
}

fn valid_slot_name(name: &str) -> bool {
  !name.is_empty() && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
}

/// Registers `island`, `slot` and `head` on a Tera instance. The functions emit
/// marker tokens; the payload inside them is base64 so no escaping can touch it.
pub fn register_markers(tera: &mut Tera) {
  tera.register_function("island", |kwargs: Kwargs, _: &State| -> tera::TeraResult<String> {
    let module = kwargs.must_get::<String>("module")?;
    let props = match kwargs.get::<tera::Value>("props")? {
      Some(v) => serde_json::to_value(&v)
        .map_err(|e| tera::Error::message(format!("island props are not serializable: {e}")))?,
      None => serde_json::Value::Object(serde_json::Map::new()),
    };
    let payload = serde_json::json!({ "m": module, "p": props });
    Ok(marker(&format!("island:{}", B64.encode(payload.to_string()))))
  });

  tera.register_function("slot", |kwargs: Kwargs, _: &State| -> tera::TeraResult<String> {
    let name = kwargs.must_get::<String>("name")?;
    if !valid_slot_name(&name) {
      return Err(tera::Error::message(format!("invalid slot name `{name}`")));
    }
    Ok(marker(&format!("slot:{name}")))
  });

  tera.register_function("head", |_: Kwargs, _: &State| -> tera::TeraResult<String> {
    Ok(marker("slot:head"))
  });
}

pub struct TeraEvaluator {
  tera: Tera,
}

impl TeraEvaluator {
  /// Tera validates function names when a template is added, so call
  /// [`register_markers`] on the instance before loading any template that
  /// uses `island`, `slot` or `head`. The registration here only covers
  /// templates added later.
  pub fn new(mut tera: Tera) -> Self {
    register_markers(&mut tera);
    Self { tera }
  }
}

fn eval_err(module: &ModuleId, message: impl Into<String>) -> EvalError {
  EvalError { module: module.to_string(), message: message.into() }
}

fn parse_island(module: &ModuleId, token: &str) -> Result<Chunk, EvalError> {
  let raw = B64
    .decode(token)
    .map_err(|_| eval_err(module, "island marker holds invalid base64"))?;
  let payload: serde_json::Value = serde_json::from_slice(&raw)
    .map_err(|e| eval_err(module, format!("island marker holds invalid json: {e}")))?;
  let island_module: ModuleId = payload
    .get("m")
    .and_then(serde_json::Value::as_str)
    .ok_or_else(|| eval_err(module, "island marker missing module"))?
    .parse()
    .map_err(|e| eval_err(module, format!("island module id: {e}")))?;
  let props = match json_to_value(payload.get("p").unwrap_or(&serde_json::Value::Null))
    .map_err(|e| eval_err(module, format!("island props: {e}")))?
  {
    Value::Map(map) => map,
    Value::Null => Default::default(),
    _ => return Err(eval_err(module, "island props must be a map")),
  };
  Ok(Chunk::Node(Node::Client {
    module: island_module,
    props,
    children: Vec::new(),
    ssr: None,
  }))
}

fn split_output(module: &ModuleId, rendered: &str) -> Result<Vec<Chunk>, EvalError> {
  let parts: Vec<&str> = rendered.split(MARKER).collect();
  if parts.len() % 2 == 0 {
    return Err(eval_err(module, "unbalanced marker delimiters in rendered output"));
  }
  let mut chunks = Vec::new();
  for (i, part) in parts.iter().enumerate() {
    if i % 2 == 0 {
      if !part.is_empty() {
        chunks.push(Chunk::Node(Node::raw(*part)));
      }
    } else if let Some(token) = part.strip_prefix("island:") {
      chunks.push(parse_island(module, token)?);
    } else if let Some(name) = part.strip_prefix("slot:") {
      chunks.push(Chunk::Slot(SlotName(name.to_owned())));
    } else {
      return Err(eval_err(module, format!("unknown marker token `{part}`")));
    }
  }
  Ok(chunks)
}

impl Evaluator for TeraEvaluator {
  fn evaluate(&self, module: &ModuleId, props: &Data) -> NodeChunks {
    let mut context = tera::Context::new();
    for (key, value) in props {
      context.insert(key.clone(), &value_to_json(value));
    }
    let result = self
      .tera
      .render(&module.path, &context)
      .map_err(|e| eval_err(module, e.to_string()))
      .and_then(|rendered| split_output(module, &rendered));
    match result {
      Ok(chunks) => Box::pin(stream::iter(chunks.into_iter().map(Ok))),
      Err(e) => Box::pin(stream::iter([Err(e)])),
    }
  }
}
