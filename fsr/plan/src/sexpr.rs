//! The manifest as `plan.sexp`: one form per row, in the order a reader wants
//! them, so opening the file shows the routes before the bodies that fill them.

use snapfire_fsr_ir::ast::Consts;
use snapfire_fsr_ir::sexpr::{
  component_from_sections, component_sections, expr_from_sx, expr_to_sx, parse, print,
  stmt_from_sx, stmt_to_sx, SexprError, Sx,
};
use snapfire_fsr_ir::Body;

use crate::{
  ActionEntry, Child, ComponentEntry, HandlerEntry, Manifest, Node, RouteEntry, RowOwner,
  SourceEntry, FORMAT_VERSION, OLDEST_READABLE,
};

type Res<T> = Result<T, SexprError>;

fn err(msg: impl std::fmt::Display) -> SexprError {
  SexprError::new(msg)
}

fn sym(s: impl Into<String>) -> Sx {
  Sx::Sym(s.into())
}

fn form(head: &str, mut rest: Vec<Sx>) -> Sx {
  let mut items = vec![sym(head)];
  items.append(&mut rest);
  Sx::List(items)
}

/// `(name value)` when the value is there, nothing when it is not.
fn opt_form(head: &str, value: &Option<String>) -> Option<Sx> {
  value.as_ref().map(|v| form(head, vec![sym(v.clone())]))
}

fn body_form(head: &str, body: &Option<Body>) -> Option<Sx> {
  body.as_ref().map(|b| form(head, b.iter().map(stmt_to_sx).collect()))
}

fn owner_sym(owner: RowOwner) -> Sx {
  sym(owner.as_str())
}

fn owner_of(sx: &Sx) -> Res<RowOwner> {
  match as_sym(sx)?.as_str() {
    "lowered" => Ok(RowOwner::Lowered),
    "engine" => Ok(RowOwner::Engine),
    "rust" => Ok(RowOwner::Rust),
    other => Err(err(format!("`{other}` is not a row owner"))),
  }
}

fn as_sym(sx: &Sx) -> Res<String> {
  match sx {
    Sx::Sym(s) => Ok(s.clone()),
    Sx::Str(s) => Ok(s.clone()),
    _ => Err(err("expected a name")),
  }
}

fn as_list(sx: &Sx) -> Res<&[Sx]> {
  match sx {
    Sx::List(items) => Ok(items),
    _ => Err(err("expected a list")),
  }
}

fn head_of(sx: &Sx) -> Res<String> {
  match sx {
    Sx::List(items) => match items.first() {
      Some(Sx::Sym(s)) => Ok(s.clone()),
      _ => Err(err("a form starts with a symbol")),
    },
    _ => Err(err("expected a form")),
  }
}

// ---------------------------------------------------------------- the node

fn node_to_sx(node: &Node) -> Sx {
  let mut rest = vec![sym(node.id.to_string()), sym(node.module.clone())];
  rest.extend(opt_form("source", &node.source));
  if node.deferred {
    rest.push(form("deferred", vec![]));
  }
  rest.extend(opt_form("fallback", &node.fallback));
  rest.extend(opt_form("error", &node.error));
  rest.extend(opt_form("cache-key", &node.cache_key));
  if !node.keep.is_empty() {
    rest.push(form("keep", node.keep.iter().map(|k| sym(k.clone())).collect()));
  }
  for child in &node.children {
    rest.push(form("slot", vec![sym(child.slot.clone()), node_to_sx(&child.node)]));
  }
  form("node", rest)
}

fn node_from_sx(sx: &Sx) -> Res<Node> {
  let items = as_list(sx)?;
  if head_of(sx)? != "node" {
    return Err(err("a plan node is `(node ...)`"));
  }
  if items.len() < 3 {
    return Err(err("a plan node needs an id and a module"));
  }
  let mut node = Node {
    id: as_sym(&items[1])?.parse().map_err(|_| err("a node id is a number"))?,
    module: as_sym(&items[2])?,
    source: None,
    deferred: false,
    fallback: None,
    error: None,
    cache_key: None,
    children: Vec::new(),
    keep: Vec::new(),
  };
  for section in &items[3..] {
    let inner = as_list(section)?;
    let one = || -> Res<String> {
      inner.get(1).ok_or_else(|| err("this node section needs a value")).and_then(as_sym)
    };
    match head_of(section)?.as_str() {
      "source" => node.source = Some(one()?),
      "deferred" => node.deferred = true,
      "fallback" => node.fallback = Some(one()?),
      "error" => node.error = Some(one()?),
      "cache-key" => node.cache_key = Some(one()?),
      "keep" => node.keep = inner[1..].iter().map(as_sym).collect::<Res<_>>()?,
      "slot" => {
        if inner.len() != 3 {
          return Err(err("a slot is `(slot name node)`"));
        }
        node.children.push(Child { slot: as_sym(&inner[1])?, node: node_from_sx(&inner[2])? });
      }
      other => return Err(err(format!("`{other}` is not a node section"))),
    }
  }
  Ok(node)
}

// ----------------------------------------------------------------- the rows

fn source_to_sx(row: &SourceEntry) -> Sx {
  let mut rest = vec![sym(row.id.clone()), owner_sym(row.owner)];
  rest.extend(opt_form("module", &row.module));
  rest.extend(opt_form("export", &row.export));
  rest.extend(opt_form("reason", &row.reason));
  rest.extend(body_form("body", &row.body));
  rest.extend(body_form("meta", &row.meta));
  rest.extend(body_form("store", &row.store));
  form("source", rest)
}

fn action_to_sx(row: &ActionEntry) -> Sx {
  let mut rest = vec![sym(row.id.clone()), owner_sym(row.owner)];
  rest.extend(opt_form("module", &row.module));
  rest.extend(opt_form("export", &row.export));
  rest.extend(opt_form("input", &row.input));
  rest.extend(opt_form("reason", &row.reason));
  rest.extend(body_form("body", &row.body));
  form("action", rest)
}

fn handler_to_sx(row: &HandlerEntry) -> Sx {
  let mut rest = vec![
    sym(row.id.clone()),
    sym(row.method.clone()),
    sym(row.pattern.clone()),
    owner_sym(row.owner),
  ];
  rest.extend(opt_form("module", &row.module));
  rest.extend(opt_form("input", &row.input));
  rest.extend(opt_form("reason", &row.reason));
  rest.extend(body_form("body", &row.body));
  form("handler", rest)
}

/// The named sections of a row, each `(name ...)`, so a row reads whatever
/// order they were written in and an absent one stays absent.
fn sections(items: &[Sx], from: usize) -> Res<Vec<(String, Vec<Sx>)>> {
  items[from..]
    .iter()
    .map(|section| {
      let inner = as_list(section)?;
      Ok((head_of(section)?, inner[1..].to_vec()))
    })
    .collect()
}

fn one_of(values: &[Sx], head: &str) -> Res<String> {
  match values.first() {
    Some(value) => as_sym(value),
    None => Err(err(format!("`{head}` needs a value"))),
  }
}

fn body_of(values: &[Sx]) -> Res<Body> {
  values.iter().map(stmt_from_sx).collect()
}

fn source_from_sx(items: &[Sx]) -> Res<SourceEntry> {
  if items.len() < 3 {
    return Err(err("a source row needs an id and an owner"));
  }
  let mut row = SourceEntry {
    id: as_sym(&items[1])?,
    owner: owner_of(&items[2])?,
    module: None,
    export: None,
    reason: None,
    body: None,
    meta: None,
    store: None,
  };
  for (head, values) in sections(items, 3)? {
    match head.as_str() {
      "module" => row.module = Some(one_of(&values, "module")?),
      "export" => row.export = Some(one_of(&values, "export")?),
      "reason" => row.reason = Some(one_of(&values, "reason")?),
      "body" => row.body = Some(body_of(&values)?),
      "meta" => row.meta = Some(body_of(&values)?),
      "store" => row.store = Some(body_of(&values)?),
      other => return Err(err(format!("`{other}` is not a source section"))),
    }
  }
  Ok(row)
}

fn action_from_sx(items: &[Sx]) -> Res<ActionEntry> {
  if items.len() < 3 {
    return Err(err("an action row needs an id and an owner"));
  }
  let mut row = ActionEntry {
    id: as_sym(&items[1])?,
    owner: owner_of(&items[2])?,
    module: None,
    export: None,
    input: None,
    reason: None,
    body: None,
  };
  for (head, values) in sections(items, 3)? {
    match head.as_str() {
      "module" => row.module = Some(one_of(&values, "module")?),
      "export" => row.export = Some(one_of(&values, "export")?),
      "input" => row.input = Some(one_of(&values, "input")?),
      "reason" => row.reason = Some(one_of(&values, "reason")?),
      "body" => row.body = Some(body_of(&values)?),
      other => return Err(err(format!("`{other}` is not an action section"))),
    }
  }
  Ok(row)
}

fn handler_from_sx(items: &[Sx]) -> Res<HandlerEntry> {
  if items.len() < 5 {
    return Err(err("a handler row needs an id, a method, a pattern and an owner"));
  }
  let mut row = HandlerEntry {
    id: as_sym(&items[1])?,
    method: as_sym(&items[2])?,
    pattern: as_sym(&items[3])?,
    owner: owner_of(&items[4])?,
    module: None,
    input: None,
    reason: None,
    body: None,
  };
  for (head, values) in sections(items, 5)? {
    match head.as_str() {
      "module" => row.module = Some(one_of(&values, "module")?),
      "input" => row.input = Some(one_of(&values, "input")?),
      "reason" => row.reason = Some(one_of(&values, "reason")?),
      "body" => row.body = Some(body_of(&values)?),
      other => return Err(err(format!("`{other}` is not a handler section"))),
    }
  }
  Ok(row)
}

// ------------------------------------------------------------- the manifest

/// The manifest as forms: the version, then routes, then the rows that fill
/// them, then the components, in the order a reader wants to meet them.
pub fn manifest_to_sx(manifest: &Manifest) -> Vec<Sx> {
  let mut out = vec![form("plan", vec![sym(manifest.version.to_string())])];
  for route in &manifest.routes {
    out.push(form("route", vec![sym(route.pattern.clone()), node_to_sx(&route.plan)]));
  }
  for route in &manifest.intercepts {
    out.push(form("intercept", vec![sym(route.pattern.clone()), node_to_sx(&route.plan)]));
  }
  if let Some(node) = &manifest.not_found {
    out.push(form("not-found", vec![node_to_sx(node)]));
  }
  if let Some(body) = &manifest.middleware {
    out.push(form("middleware", body.iter().map(stmt_to_sx).collect()));
  }
  out.extend(manifest.sources.iter().map(source_to_sx));
  out.extend(manifest.actions.iter().map(action_to_sx));
  out.extend(manifest.handlers.iter().map(handler_to_sx));
  for (name, expr) in &manifest.consts {
    out.push(form("const", vec![sym(name.clone()), expr_to_sx(expr)]));
  }
  for entry in &manifest.components {
    let mut rest = vec![sym(entry.module.clone())];
    rest.extend(component_sections(&entry.body));
    out.push(form("component", rest));
  }
  out
}

pub fn manifest_from_sx(forms: &[Sx]) -> Res<Manifest> {
  let mut manifest = Manifest {
    version: 0,
    routes: Vec::new(),
    sources: Vec::new(),
    actions: Vec::new(),
    components: Vec::new(),
    consts: Consts::new(),
    not_found: None,
    handlers: Vec::new(),
    middleware: None,
    intercepts: Vec::new(),
  };
  let mut versioned = false;
  for sx in forms {
    let items = as_list(sx)?;
    match head_of(sx)?.as_str() {
      "plan" => {
        manifest.version = one_of(&items[1..], "plan")?
          .parse()
          .map_err(|_| err("a plan version is a number"))?;
        versioned = true;
      }
      "route" | "intercept" => {
        if items.len() != 3 {
          return Err(err("a route is `(route pattern node)`"));
        }
        let entry = RouteEntry { pattern: as_sym(&items[1])?, plan: node_from_sx(&items[2])? };
        if head_of(sx)? == "route" {
          manifest.routes.push(entry);
        } else {
          manifest.intercepts.push(entry);
        }
      }
      "not-found" => {
        if items.len() != 2 {
          return Err(err("`not-found` takes one node"));
        }
        manifest.not_found = Some(node_from_sx(&items[1])?);
      }
      "middleware" => manifest.middleware = Some(body_of(&items[1..])?),
      "source" => manifest.sources.push(source_from_sx(items)?),
      "action" => manifest.actions.push(action_from_sx(items)?),
      "handler" => manifest.handlers.push(handler_from_sx(items)?),
      "const" => {
        if items.len() != 3 {
          return Err(err("a const is `(const name expr)`"));
        }
        manifest.consts.insert(as_sym(&items[1])?, expr_from_sx(&items[2])?);
      }
      "component" => {
        if items.len() < 2 {
          return Err(err("a component needs a module id"));
        }
        manifest.components.push(ComponentEntry {
          module: as_sym(&items[1])?,
          body: component_from_sections(&items[2..])?,
        });
      }
      other => return Err(err(format!("`{other}` is not a plan form"))),
    }
  }
  if !versioned {
    return Err(err("the plan file has no `(plan <version>)` line"));
  }
  Ok(manifest)
}

impl Manifest {
  /// Reads a plan file of either form, told apart by its first term: a plan in
  /// s-expressions opens with `(`, the JSON it replaced with `{`.
  pub fn from_text(source: &str) -> Result<Self, crate::PlanError> {
    match source.trim_start().as_bytes().first() {
      Some(b'{') => Self::from_json(source),
      _ => Self::from_sexpr(source),
    }
  }

  /// The manifest as the text of a `plan.sexp`.
  pub fn to_sexpr(&self) -> String {
    print(&manifest_to_sx(self))
  }

  /// Reads a `plan.sexp`. The version window is the one [`Manifest::from_json`]
  /// enforces, so a file from a newer build is refused rather than half read.
  pub fn from_sexpr(source: &str) -> Result<Self, crate::PlanError> {
    let forms = parse(source).map_err(|e| crate::PlanError::Malformed(e.to_string()))?;
    let manifest =
      manifest_from_sx(&forms).map_err(|e| crate::PlanError::Malformed(e.to_string()))?;
    if !(OLDEST_READABLE..=FORMAT_VERSION).contains(&manifest.version) {
      return Err(crate::PlanError::Version { found: manifest.version });
    }
    for row in &manifest.sources {
      if row.owner == RowOwner::Lowered && row.body.is_none() {
        return Err(crate::PlanError::NoBody { id: row.id.clone() });
      }
    }
    for row in &manifest.actions {
      if row.owner == RowOwner::Lowered && row.body.is_none() {
        return Err(crate::PlanError::NoBody { id: row.id.clone() });
      }
    }
    Ok(manifest)
  }
}
