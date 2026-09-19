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

/// The prop the browser reads a placement's region key from, as the IR writes it.
const REGION_KEY: &str = "$k";

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
    let when = match kwargs.get::<String>("when")? {
      Some(when) if !matches!(when.as_str(), "load" | "visible" | "idle") => {
        return Err(tera::Error::message(format!("`when` is \"load\", \"visible\" or \"idle\", not `{when}`")));
      }
      other => other,
    };
    let mode = match kwargs.get::<String>("mode")? {
      Some(mode) if !matches!(mode.as_str(), "browser" | "server") => {
        return Err(tera::Error::message(format!("`mode` is \"browser\" or \"server\", not `{mode}`")));
      }
      Some(mode) if mode == "browser" => None,
      other => other,
    };
    let key = kwargs.get::<String>("key")?;
    // The state rides in the props under the key the browser already reads a
    // server-mode island's state from, so a template island and a lowered one
    // are mounted by the same code.
    let mut props = props;
    if let Some(state) = kwargs.get::<tera::Value>("state")? {
      let state = serde_json::to_value(&state)
        .map_err(|e| tera::Error::message(format!("island state is not serializable: {e}")))?;
      match props.as_object_mut() {
        Some(map) => {
          map.insert(STATE_PROP.to_owned(), state);
        }
        None => return Err(tera::Error::message("island props must be an object to carry state")),
      }
    }
    let payload = serde_json::json!({ "m": module, "p": props, "w": when, "d": mode, "k": key });
    Ok(marker(&format!("island:{}", B64.encode(payload.to_string()))))
  });

  tera.register_function("on", |kwargs: Kwargs, _: &State| -> tera::TeraResult<tera::Value> {
    let mut bound: Vec<String> = Vec::new();
    for (event, value) in kwargs.iter() {
      let Some(name) = value.as_str() else {
        return Err(tera::Error::message(format!("the handler for `{event}` is a name, written out")));
      };
      if name.is_empty() || !name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-' || c == '.') {
        return Err(tera::Error::message(format!("invalid handler name `{name}`")));
      }
      bound.push(format!("{event}:{name}"));
    }
    if bound.is_empty() {
      return Err(tera::Error::message("`on` takes an event, `on(click=\"increment\")`"));
    }
    // Safe rather than a plain string: the attribute carries quotes and an
    // escaped one is not an attribute.
    Ok(tera::Value::safe_string(&format!("data-sf-on=\"{}\"", bound.join(" "))))
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

/// The props key a server-mode island's state rides under, the same one the
/// renderer uses for a lowered component.
use snapfire_fsr_runtime::islands::STATE_PROP;

/// The island mode whose events round-trip to the server.
const SERVER_MODE: &str = "server";

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

  /// Whether a template of that name was added.
  pub fn has(&self, name: &str) -> bool {
    self.tera.contains_template(name)
  }

  /// The chunks one `island(...)` becomes: the `<sf-s data-sf-island>` region
  /// around it when the placement asked for a timing or for server mode, which
  /// is where the browser reads both, else the client node alone.
  fn parse_island(&self, module: &ModuleId, token: &str, fill: bool) -> Result<Vec<Chunk>, EvalError> {
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
    let when = payload.get("w").and_then(serde_json::Value::as_str);
    let mode = payload.get("d").and_then(serde_json::Value::as_str);
    let key = payload.get("k").and_then(serde_json::Value::as_str);
    let mut props = props;
    // The region a revalidation patches rather than replaces: the browser reads
    // it from the markup to find the region and from the props to find what to
    // patch it with, so a keyed placement carries both.
    if let Some(key) = key {
      props.insert(REGION_KEY.to_owned(), Value::str(key));
    }
    // A server-mode placement whose module is a template this instance holds is
    // rendered here, so the first paint carries it and a reader with no
    // JavaScript sees the island's markup rather than an empty marker. One level
    // only: an island inside an island's body is the browser's to ask for.
    let ssr = match mode == Some(SERVER_MODE) && fill {
      true => self.render_island(&island_module, &props)?,
      false => None,
    };
    let client = Chunk::Node(Node::Client {
      module: island_module,
      props,
      children: Vec::new(),
      ssr: ssr.map(Box::new),
    });
    if when.is_none() && mode.is_none() && key.is_none() {
      return Ok(vec![client]);
    }
    let mut open = String::from("<sf-s data-sf-island");
    if let Some(key) = key {
      open.push_str(&format!(" data-sf-region=\"{key}\""));
    }
    if let Some(when) = when {
      open.push_str(&format!(" data-sf-when=\"{when}\""));
    }
    if let Some(mode) = mode {
      open.push_str(&format!(" data-sf-mode=\"{mode}\""));
    }
    open.push('>');
    Ok(vec![Chunk::Node(Node::raw(open)), client, Chunk::Node(Node::raw("</sf-s>"))])
  }

  /// The island's own markup, when its module is a template this instance
  /// holds. `None` for a module it does not, which is every component.
  fn render_island(&self, island: &ModuleId, props: &snapfire_fsr_core::ValueMap) -> Result<Option<Node>, EvalError> {
    if !self.tera.get_template_names().any(|name| name == island.path) {
      return Ok(None);
    }
    let state = props.get(snapfire_fsr_runtime::islands::STATE_PROP).cloned().unwrap_or(Value::Null);
    let data = snapfire_fsr_runtime::island_data(props, &state);
    let mut context = tera::Context::new();
    for (key, value) in &data {
      context.insert(key.clone(), &value_to_json(value));
    }
    let rendered = self
      .tera
      .render(&island.path, &context)
      .map_err(|e| eval_err(island, e.to_string()))?;
    let mut nodes = Vec::new();
    for chunk in self.split_output(island, &rendered, false)? {
      match chunk {
        Chunk::Node(node) => nodes.push(node),
        Chunk::Slot(slot) => return Err(eval_err(island, format!("the slot `{}` inside an island, which has no child to fill", slot.0))),
      }
    }
    Ok(Some(Node::Seq(nodes)))
  }

  fn split_output(&self, module: &ModuleId, rendered: &str, fill: bool) -> Result<Vec<Chunk>, EvalError> {
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
        chunks.extend(self.parse_island(module, token, fill)?);
      } else if let Some(name) = part.strip_prefix("slot:") {
        chunks.push(Chunk::Slot(SlotName(name.to_owned())));
      } else {
        return Err(eval_err(module, format!("unknown marker token `{part}`")));
      }
    }
    Ok(chunks)
  }
}

fn eval_err(module: &ModuleId, message: impl Into<String>) -> EvalError {
  EvalError { module: module.to_string(), message: message.into() }
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
      .and_then(|rendered| self.split_output(module, &rendered, true));
    match result {
      Ok(chunks) => Box::pin(stream::iter(chunks.into_iter().map(Ok))),
      Err(e) => Box::pin(stream::iter([Err(e)])),
    }
  }
}
