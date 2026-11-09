use super::*;

impl LayoutInfo {
  pub(crate) fn declares(&self, slot: &str) -> bool {
    self.slots.iter().any(|s| s.name == slot) || self.placed.iter().any(|p| p == slot)
  }

  /// Every slot of this layout, `content` first, that `filled` leaves out.
  pub(crate) fn kept(&self, filled: &[String]) -> Vec<String> {
    let mut kept = vec!["content".to_owned()];
    for slot in self.slots.iter().map(|s| &s.name).chain(&self.placed) {
      if !kept.contains(slot) {
        kept.push(slot.clone());
      }
    }
    kept.retain(|slot| !filled.contains(slot));
    kept
  }

  pub(crate) fn node(&self, children: Vec<Child>, keep: Vec<String>, error: &Boundary) -> Node {
    Node {
      id: 0,
      module: self.module.clone(),
      source: self.source.clone(),
      deferred: false,
      fallback: None,
      error: error.module.clone(),
      error_kinds: error.kinds.clone(),
      cache_key: Some(self.module.clone()),
      children,
      keep,
    }
  }
}

impl SlotInfo {
  pub(crate) fn child(&self, error: &Boundary) -> Child {
    let boundary = self.error.or(error);
    Child {
      slot: self.name.clone(),
      node: Node {
        id: 0,
        module: self.page.clone(),
        source: self.source.clone(),
        deferred: self.loading.is_some(),
        fallback: self.loading.clone(),
        error: boundary.module,
        error_kinds: boundary.kinds,
        cache_key: Some(self.page.clone()),
        children: Vec::new(),
        keep: Vec::new(),
      },
    }
  }
}

/// Every module a plan node and its children render, the node's own first.
pub(crate) fn plan_modules(node: &Node, out: &mut Vec<String>) {
  if !out.contains(&node.module) {
    out.push(node.module.clone());
  }
  for child in &node.children {
    plan_modules(&child.node, out);
  }
}

pub(crate) fn wrap_in_layouts(content: Node, wrapping: &[&LayoutInfo], error: &Boundary) -> Node {
  let mut node = content;
  for layout in wrapping.iter().rev() {
    let mut children = vec![Child { slot: "content".to_owned(), node }];
    children.extend(layout.slots.iter().map(|s| s.child(error)));
    node = layout.node(children, Vec::new(), error);
  }
  node
}

/// The tree a soft navigation renders for a `page.<slot>.tsx`: the layouts
/// down to the one declaring `slot`, which takes `variant` there and keeps
/// its page; every other slot along the way is kept too, so only the one
/// region changes in the browser.
pub(crate) fn intercept_plan(variant: Node, slot: &str, wrapping: &[&LayoutInfo], error: &Boundary) -> Node {
  let (declaring, above) = wrapping.split_last().expect("an intercept sits under the layout declaring its slot");
  let mut node = declaring.node(vec![Child { slot: slot.to_owned(), node: variant }], declaring.kept(&[slot.to_owned()]), error);
  for layout in above.iter().rev() {
    node = layout.node(vec![Child { slot: "content".to_owned(), node }], layout.kept(&["content".to_owned()]), error);
  }
  node
}

pub(crate) fn shell_over(options: &Options, content: Node) -> Node {
  Node {
    id: 0,
    module: options.shell.clone(),
    source: None,
    deferred: false,
    fallback: None,
    error: None,
    error_kinds: Vec::new(),
    cache_key: None,
    children: vec![Child { slot: options.slot.clone(), node: content }],
    keep: Vec::new(),
  }
}

/// Ids in tree order from `next`, so a plan's ids are unique whatever its shape.
pub(crate) fn renumber(node: &mut Node, next: &mut u32) {
  node.id = *next;
  *next += 1;
  for child in &mut node.children {
    renumber(&mut child.node, next);
  }
}

/// `page.<slot>.tsx` files in a route directory (`.ts`, `.jsx` and `.js` as
/// well): the file and the slot it names.
pub(crate) fn variant_files(dir: &Path) -> Result<Vec<(String, String)>, BuildError> {
  let mut out = Vec::new();
  for file in sorted_files(dir, "")? {
    let name = file.file_name().unwrap_or_default().to_string_lossy().to_string();
    let Some(middle) = name.strip_prefix("page.").and_then(|n| COMPONENT_EXTENSIONS.iter().find_map(|ext| n.strip_suffix(&format!(".{ext}")))) else { continue };
    if middle.is_empty() || middle == "loader" || middle.contains('.') {
      continue;
    }
    out.push((name.clone(), middle.to_owned()));
  }
  Ok(out)
}

/// Lowers `module` into the set, recording residue as a client-only component.
pub(crate) fn lower_into(set: &mut ComponentSet, module: &str, report: &mut Report) -> Result<(), BuildError> {
  match set.lower(module) {
    Ok(()) => Ok(()),
    Err(LowerError::Residue(residue)) => {
      let at = format!("{}:{}:{}", residue.file, residue.line, residue.column);
      let chain = residue.chain();
      blame(report, module, at, residue.message, residue.hint, chain);
      Ok(())
    }
    Err(LowerError::Parse { file, message }) => {
      blame(report, module, file, message, None, String::new());
      Ok(())
    }
    Err(e) => Err(e.into()),
  }
}

/// Marks `module` client and files it under the cause at `at`, which several
/// modules reach when they import their way to the same unlowerable line.
/// Whether the file behind `module` places FSR's `<Embed>`. A page's split island reads its page's file.
pub(crate) fn places_embed(app: &Path, module: &str) -> bool {
  let file = module.split('#').next().unwrap_or(module);
  let file = match file.rfind(".island") {
    Some(at) => format!("{}{}", &file[..at], file.rfind('.').map_or("", |dot| &file[dot..])),
    None => file.to_owned(),
  };
  std::fs::read_to_string(app.join(file)).is_ok_and(|text| text.contains("@snapfire/fsr/") && text.contains("Embed"))
}

/// What `[build] strict` refuses in `report`: each cause that leaves a component to render in the browser alone, with the components it leaves there.
pub(crate) fn strict_refusal(report: &Report) -> Option<String> {
  let stills = report.embeds.len() + report.embed_failures.len() + report.browser_embeds.len();
  if report.causes.is_empty() && report.foreign.is_empty() && report.plugins.is_empty() && stills == 0 {
    return None;
  }
  let mut out = String::from("these components render in the browser alone or show no still, which `[build] strict` refuses; make each lower and serve its still or drop `strict`");
  for (module, provider) in &report.embeds {
    out.push_str(&format!("\n  embed    {module}: {provider}, the id is computed"));
  }
  for (provider, id, why) in &report.embed_failures {
    out.push_str(&format!("\n  embed    {provider} {id}: {why}"));
  }
  for module in &report.browser_embeds {
    out.push_str(&format!("\n  embed    {module}: renders in the browser"));
  }
  for (kind, causes) in [("client", &report.causes), ("foreign", &report.foreign)] {
    for cause in causes {
      out.push_str(&format!("\n  {kind:<8} {}: {}", cause.at, cause.message));
      if let Some(hint) = &cause.hint {
        out.push_str(&format!("\n           {hint}"));
      }
      for (module, _) in &cause.pages {
        out.push_str(&format!("\n             {module}"));
      }
    }
  }
  for plugin in &report.plugins {
    out.push_str(&format!("\n  plugins  {plugin}"));
  }
  Some(out)
}

pub(crate) fn blame(report: &mut Report, module: &str, at: String, message: String, hint: Option<String>, chain: String) {
  report.components.push((module.to_owned(), "client".to_owned(), at.clone()));
  let page = (module.to_owned(), chain);
  match report.causes.iter_mut().find(|c| c.at == at && c.message == message) {
    Some(cause) => cause.pages.push(page),
    None => report.causes.push(Cause { at, message, hint, pages: vec![page] }),
  }
}

/// The first handler of `component` reading a name that will not exist when
/// the host runs it, with the handler's index. A step binds `$props`, `$state`
/// and `$event` and re-runs the component's own `let`s; anything else a
/// handler reads was bound by the markup it sits in, a loop's variable for
/// one, which is gone by then.
pub(crate) fn captured_by_handler(component: &snapfire_fsr_ir::Component) -> Option<(String, usize)> {
  let mut bound: Vec<&str> = vec!["$props", "$state", "$event"];
  for stmt in &component.body {
    if let snapfire_fsr_ir::Stmt::Let { name, .. } = stmt {
      bound.push(name);
    }
  }
  component.handlers.iter().enumerate().find_map(|(index, handler)| {
    snapfire_fsr_ir::body_free_vars(&handler.body)
      .into_iter()
      .find(|name| !bound.contains(&name.as_str()))
      .map(|name| (name, index))
  })
}

/// The modules a template places as islands in server mode.
pub(crate) fn server_islands(tmpl: &snapfire_fsr_ir::Tmpl) -> Vec<String> {
  use snapfire_fsr_ir::Tmpl;
  fn walk(tmpl: &Tmpl, out: &mut Vec<String>) {
    match tmpl {
      Tmpl::Baked { children, .. } => children.iter().for_each(|c| walk(c, out)),
      Tmpl::Island { module, mode, children, .. } => {
        if mode.as_deref() == Some(snapfire_fsr_ir::render::SERVER_MODE) && !out.contains(module) {
          out.push(module.clone());
        }
        children.iter().for_each(|c| walk(c, out));
      }
      Tmpl::Component { children, .. } | Tmpl::Element { children, .. } | Tmpl::Fragment(children) | Tmpl::Place { children, .. } => children.iter().for_each(|c| walk(c, out)),
      Tmpl::If { then, r#else, .. } => {
        walk(then, out);
        if let Some(e) = r#else {
          walk(e, out);
        }
      }
      Tmpl::For { body, .. } => walk(body, out),
      Tmpl::Let { then, .. } => walk(then, out),
      Tmpl::Text(_) | Tmpl::Marker(_) | Tmpl::Outlet { .. } | Tmpl::Html(_) | Tmpl::Expr(_) | Tmpl::Slot(_) => {}
    }
  }
  let mut out = Vec::new();
  walk(tmpl, &mut out);
  out
}

/// The line and reason left on the first element whose handler did not lower.
pub(crate) fn unlowered_handler(tmpl: &snapfire_fsr_ir::Tmpl) -> Option<String> {
  unlowered_handlers(tmpl).into_iter().next()
}

/// The line and reason left on every element whose handler did not lower and
/// on every placement handed a function that did not, in document order.
pub(crate) fn unlowered_handlers(tmpl: &snapfire_fsr_ir::Tmpl) -> Vec<String> {
  use snapfire_fsr_ir::ast::{Entry, Expr, Lit};
  use snapfire_fsr_ir::Tmpl;
  fn walk(tmpl: &Tmpl, out: &mut Vec<String>) {
    match tmpl {
      Tmpl::Element { attrs, children, .. } => {
        out.extend(attrs.iter().filter_map(|e| match e {
          Entry::Field(n, Expr::Lit(Lit::Str(why))) if n == snapfire_fsr_ir::render::UNLOWERED_ATTR => Some(why.clone()),
          _ => None,
        }));
        children.iter().for_each(|c| walk(c, out));
      }
      Tmpl::Component { props, children, .. } => {
        out.extend(props.iter().filter_map(|e| match e {
          Entry::Field(n, Expr::Lit(Lit::Str(why))) if n == snapfire_fsr_ir::render::UNLOWERED_ATTR => Some(why.clone()),
          _ => None,
        }));
        children.iter().for_each(|c| walk(c, out));
      }
      Tmpl::Baked { children, .. } | Tmpl::Island { children, .. } | Tmpl::Fragment(children) | Tmpl::Place { children, .. } => children.iter().for_each(|c| walk(c, out)),
      Tmpl::If { then, r#else, .. } => {
        walk(then, out);
        if let Some(e) = r#else {
          walk(e, out);
        }
      }
      Tmpl::For { body, .. } => walk(body, out),
      Tmpl::Let { then, .. } => walk(then, out),
      Tmpl::Text(_) | Tmpl::Marker(_) | Tmpl::Outlet { .. } | Tmpl::Html(_) | Tmpl::Expr(_) | Tmpl::Slot(_) => {}
    }
  }
  let mut out = Vec::new();
  walk(tmpl, &mut out);
  out
}

/// The modules a template renders as components, islands aside.
pub(crate) fn nested_components(tmpl: &snapfire_fsr_ir::Tmpl) -> Vec<String> {
  use snapfire_fsr_ir::Tmpl;
  fn walk(tmpl: &Tmpl, out: &mut Vec<String>) {
    match tmpl {
      Tmpl::Baked { children, .. } => children.iter().for_each(|c| walk(c, out)),
      Tmpl::Component { module, children, .. } => {
        if !out.contains(module) {
          out.push(module.clone());
        }
        children.iter().for_each(|c| walk(c, out));
      }
      Tmpl::Island { children, .. } | Tmpl::Element { children, .. } | Tmpl::Fragment(children) | Tmpl::Place { children, .. } => children.iter().for_each(|c| walk(c, out)),
      Tmpl::If { then, r#else, .. } => {
        walk(then, out);
        if let Some(e) = r#else {
          walk(e, out);
        }
      }
      Tmpl::For { body, .. } => walk(body, out),
      Tmpl::Let { then, .. } => walk(then, out),
      Tmpl::Text(_) | Tmpl::Marker(_) | Tmpl::Outlet { .. } | Tmpl::Html(_) | Tmpl::Expr(_) | Tmpl::Slot(_) => {}
    }
  }
  let mut out = Vec::new();
  walk(tmpl, &mut out);
  out
}

/// The first slot a template renders, `content` included. A step renders the
/// island with no slot stack, so whatever filled it at first paint is gone
/// from the answer and the patch removes it from the document.
pub(crate) fn first_slot(tmpl: &snapfire_fsr_ir::Tmpl) -> Option<String> {
  use snapfire_fsr_ir::Tmpl;
  fn walk(tmpl: &Tmpl) -> Option<String> {
    match tmpl {
      Tmpl::Slot(name) => Some(name.clone()),
      Tmpl::Baked { children, .. } | Tmpl::Component { children, .. } | Tmpl::Island { children, .. } | Tmpl::Element { children, .. } | Tmpl::Fragment(children) | Tmpl::Place { children, .. } => children.iter().find_map(walk),
      Tmpl::If { then, r#else, .. } => walk(then).or_else(|| r#else.as_ref().and_then(|e| walk(e))),
      Tmpl::For { body, .. } => walk(body),
      Tmpl::Let { then, .. } => walk(then),
      Tmpl::Text(_) | Tmpl::Marker(_) | Tmpl::Outlet { .. } | Tmpl::Html(_) | Tmpl::Expr(_) => None,
    }
  }
  walk(tmpl)
}

/// The named slots a template places, `content` aside, in tree order.
pub(crate) fn slots_placed(tmpl: &snapfire_fsr_ir::Tmpl) -> Vec<String> {
  use snapfire_fsr_ir::Tmpl;
  let mut out = Vec::new();
  fn walk(tmpl: &Tmpl, out: &mut Vec<String>) {
    match tmpl {
      Tmpl::Baked { children, .. } => children.iter().for_each(|c| walk(c, out)),
      Tmpl::Slot(name) if name != "content" && !out.contains(name) => out.push(name.clone()),
      Tmpl::Slot(_) | Tmpl::Text(_) | Tmpl::Marker(_) | Tmpl::Outlet { .. } | Tmpl::Html(_) | Tmpl::Expr(_) => {}
      Tmpl::Component { children, .. } | Tmpl::Island { children, .. } | Tmpl::Element { children, .. } | Tmpl::Fragment(children) | Tmpl::Place { children, .. } => children.iter().for_each(|c| walk(c, out)),
      Tmpl::If { then, r#else, .. } => {
        walk(then, out);
        if let Some(e) = r#else {
          walk(e, out);
        }
      }
      Tmpl::For { body, .. } => walk(body, out),
      Tmpl::Let { then, .. } => walk(then, out),
    }
  }
  walk(tmpl, &mut out);
  out
}

pub(crate) fn sorted_dirs(dir: &Path) -> Result<Vec<PathBuf>, BuildError> {
  if !dir.is_dir() {
    return Ok(Vec::new());
  }
  let mut dirs: Vec<PathBuf> = std::fs::read_dir(dir)
    .map_err(|e| BuildError::Io(dir.to_path_buf(), e))?
    .filter_map(|e| e.ok().map(|e| e.path()))
    .filter(|p| p.is_dir())
    .collect();
  dirs.sort();
  Ok(dirs)
}
