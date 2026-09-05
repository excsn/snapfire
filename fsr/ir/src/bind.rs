use std::sync::Arc;

use futures_util::future::BoxFuture;
use futures_util::stream;
use snapfire_fsr_core::{Data, ModuleId, Node, SlotName, Value};
use snapfire_fsr_runtime::{ActionError, ActionHandler, Chunk, DataSource, EvalError, Evaluator, LoadError, Meta, Metadata, NodeChunks, RequestCtx};

use crate::ast::{Body, Component};
use crate::interp::Interpreter;
use crate::render::{Components, Rendered, ISLAND_MARK, SLOT_MARK};

/// The nodes a rendered component's markup makes: raw pieces, `Node::Slot`
/// where a root slot sits and, where an island sits, its region: an
/// `<sf-s data-sf-island>` wrapper the outer root adopts and never
/// reconciles, holding a nested client node whose body is the island's own
/// markup, islands inside it included. `when` rides on the wrapper as
/// `data-sf-when`, which the boot runtime reads over the registry's timing.
pub fn rendered_nodes(rendered: &Rendered) -> Vec<Node> {
  let mut out = Vec::new();
  let mut rest = rendered.html.as_str();
  while let Some(start) = rest.find('\u{0}') {
    let (before, marked) = rest.split_at(start);
    if !before.is_empty() {
      out.push(Node::raw(before));
    }
    let end = marked[1..].find('\u{0}').map(|e| e + 1).expect("a marker is closed by a NUL");
    let marker = &marked[..=end];
    rest = &marked[end + 1..];
    if let Some(name) = marker.strip_prefix(SLOT_MARK) {
      out.push(Node::Slot(SlotName(name[..name.len() - 1].to_owned())));
      continue;
    }
    let index: usize = marker[ISLAND_MARK.len()..marker.len() - 1].parse().expect("an island marker carries its index");
    let island = &rendered.islands[index];
    let module: ModuleId = island.module.parse().unwrap_or_else(|_| ModuleId::new(island.module.clone(), "default"));
    let open = match &island.when {
      Some(when) => format!("<sf-s data-sf-island data-sf-when=\"{when}\">"),
      None => "<sf-s data-sf-island>".to_owned(),
    };
    out.push(Node::raw(open));
    out.push(Node::Client { module, props: island.props.clone(), children: Vec::new(), ssr: Some(Box::new(island_body(&island.body))) });
    out.push(Node::raw("</sf-s>"));
  }
  if !rest.is_empty() {
    out.push(Node::raw(rest));
  }
  out
}

fn island_body(body: &Rendered) -> Node {
  if body.islands.is_empty() {
    return Node::raw(body.html.clone());
  }
  let mut nodes = rendered_nodes(body);
  if nodes.len() == 1 { nodes.pop().unwrap() } else { Node::Seq(nodes) }
}

/// A lowered loader answering a data source id. The body must return an
/// object; its fields are the source's data.
pub struct IrSource {
  id: String,
  body: Arc<Body>,
  interpreter: Interpreter,
}

impl IrSource {
  pub fn new(id: impl Into<String>, body: Body) -> Self {
    Self { id: id.into(), body: Arc::new(body), interpreter: Interpreter::default() }
  }

  pub fn with_interpreter(mut self, interpreter: Interpreter) -> Self {
    self.interpreter = interpreter;
    self
  }
}

impl DataSource for IrSource {
  fn load(&self, ctx: &RequestCtx) -> BoxFuture<'static, Result<Data, LoadError>> {
    let id = self.id.clone();
    let body = self.body.clone();
    let interpreter = self.interpreter.clone();
    let ctx = ctx.clone();
    Box::pin(async move {
      let outcome = interpreter
        .run(&body, &ctx, None)
        .await
        .map_err(|fail| LoadError { source_id: id.clone(), message: fail.message })?;
      match outcome.value {
        Value::Map(data) => Ok(data),
        other => Err(LoadError {
          source_id: id,
          message: format!("a loader must return an object, got {}", kind_name(&other)),
        }),
      }
    })
  }
}

/// A lowered `meta` describing the document from its loader's data, which
/// the body reads as its input. It must return an object; `title` and
/// `description` are read from it and anything else is ignored.
pub struct IrMeta {
  source_id: String,
  body: Arc<Body>,
  interpreter: Interpreter,
}

impl IrMeta {
  pub fn new(source_id: impl Into<String>, body: Body) -> Self {
    Self { source_id: source_id.into(), body: Arc::new(body), interpreter: Interpreter::default() }
  }
}

impl Metadata for IrMeta {
  fn describe(&self, ctx: &RequestCtx, data: &Data) -> BoxFuture<'static, Result<Meta, LoadError>> {
    let id = self.source_id.clone();
    let body = self.body.clone();
    let interpreter = self.interpreter.clone();
    let ctx = ctx.clone();
    let input = Value::Map(data.clone());
    Box::pin(async move {
      let outcome = interpreter.run(&body, &ctx, Some(input)).await.map_err(|fail| LoadError { source_id: id.clone(), message: fail.message })?;
      let Value::Map(map) = outcome.value else {
        return Err(LoadError { source_id: id, message: format!("meta must return an object, got {}", kind_name(&outcome.value)) });
      };
      let text = |key: &str| match map.get(key) {
        Some(Value::Str(s)) => Ok(Some(s.clone())),
        None | Some(Value::Null) => Ok(None),
        Some(other) => Err(LoadError { source_id: id.clone(), message: format!("meta.{key} must be a string, got {}", kind_name(other)) }),
      };
      Ok(Meta { title: text("title")?, description: text("description")? })
    })
  }
}

/// A lowered action answering an action id.
pub struct IrAction {
  body: Arc<Body>,
  interpreter: Interpreter,
}

impl IrAction {
  pub fn new(body: Body) -> Self {
    Self { body: Arc::new(body), interpreter: Interpreter::default() }
  }

  pub fn with_interpreter(mut self, interpreter: Interpreter) -> Self {
    self.interpreter = interpreter;
    self
  }
}

impl ActionHandler for IrAction {
  fn call(&self, ctx: RequestCtx, input: Value) -> BoxFuture<'static, Result<Value, ActionError>> {
    let body = self.body.clone();
    let interpreter = self.interpreter.clone();
    Box::pin(async move {
      interpreter
        .run(&body, &ctx, Some(input))
        .await
        .map(|outcome| outcome.value)
        .map_err(|fail| ActionError::new(fail.kind, fail.message))
    })
  }
}

/// Lowered components rendered in Rust. The output is a `Client` node whose
/// `ssr` holds the markup, so the browser hydrates the module over it; a
/// module the evaluator does not hold is not its business, which is why the
/// registry predicate is `covers`.
pub struct IrEvaluator {
  components: Arc<Components>,
  interpreter: Interpreter,
}

impl IrEvaluator {
  pub fn new(components: impl IntoIterator<Item = (String, Component)>) -> Self {
    let components = components.into_iter().map(|(module, component)| (module, Arc::new(component))).collect();
    Self { components: Arc::new(components), interpreter: Interpreter::default() }
  }

  pub fn with_interpreter(mut self, interpreter: Interpreter) -> Self {
    self.interpreter = interpreter;
    self
  }

  pub fn covers(&self, module: &ModuleId) -> bool {
    self.components.contains_key(&module.to_string())
  }

  pub fn modules(&self) -> Vec<String> {
    let mut modules: Vec<String> = self.components.keys().cloned().collect();
    modules.sort();
    modules
  }
}

impl Evaluator for IrEvaluator {
  fn evaluate(&self, module: &ModuleId, props: &Data) -> NodeChunks {
    let components = self.components.clone();
    let interpreter = self.interpreter.clone();
    let module = module.clone();
    let props = props.clone();
    Box::pin(stream::once(async move {
      let id = module.to_string();
      let component = components.get(&id).cloned().ok_or_else(|| EvalError { module: id.clone(), message: "not a lowered component".to_owned() })?;
      let rendered = interpreter.render(&component, &props, &components).map_err(|fail| EvalError { module: id, message: fail.message })?;
      if rendered.islands.is_empty() && !rendered.html.contains(SLOT_MARK) {
        return Ok(Chunk::Node(Node::Client { module, props, children: Vec::new(), ssr: Some(Box::new(Node::raw(rendered.html))) }));
      }
      Ok(Chunk::Node(Node::Client { module, props, children: rendered_nodes(&rendered), ssr: None }))
    }))
  }
}

pub(crate) fn kind_name(value: &Value) -> &'static str {
  match value {
    Value::Null => "null",
    Value::Bool(_) => "bool",
    Value::Int(_) => "int",
    Value::UInt(_) => "uint",
    Value::F32(_) | Value::F64(_) => "float",
    Value::Str(_) => "string",
    Value::Bytes(_) => "bytes",
    Value::TypedArray(_) => "typed array",
    Value::Seq(_) => "array",
    Value::Map(_) => "object",
    Value::Variant { .. } => "variant",
    Value::Ref { .. } => "ref",
  }
}
