//! The manifest as `plan.sexp`: one form per row, in the order a reader wants
//! them, so opening the file shows the routes before the bodies that fill them.

mod node;
mod rows;
mod shape;

use snapfire_fsr_ir::ast::Consts;
use snapfire_fsr_ir::sexpr::{expr_from_sx, expr_to_sx, parse, print, stmt_to_sx, Sx};
use snapfire_fsr_ir::sexpr::component_from_sections;
use snapfire_fsr_ir::sexpr::component_sections;

use node::{node_from_sx, node_to_sx};
use rows::{action_from_sx, action_to_sx, handler_from_sx, handler_to_sx, source_from_sx, source_to_sx};
use shape::*;

use crate::HeadRow;
use crate::{
  ClientEntry, ComponentEntry, DeriveEntry, Manifest, RouteEntry, RowOwner, FORMAT_VERSION, OLDEST_READABLE,
};

/// The manifest as forms: the version, then routes, then the rows that fill
/// them, then the components, in the order a reader wants to meet them.
pub fn manifest_to_sx(manifest: &Manifest) -> Vec<Sx> {
  let mut out = vec![form("plan", vec![sym(manifest.version.to_string())])];
  for (package, version) in &manifest.frameworks {
    out.push(form("framework", vec![sym(package.clone()), sym(version.clone())]));
  }
  for (module, keys) in &manifest.reads {
    let mut terms = vec![sym(module.clone())];
    terms.extend(keys.iter().map(|key| Sx::Str(key.clone())));
    out.push(form("reads", terms));
  }
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
  out.extend(manifest.components.iter().map(component_to_sx));
  out.extend(manifest.derives.iter().map(derive_to_sx));
  for entry in &manifest.clients {
    let mut rest = vec![sym(entry.module.clone()), Sx::Str(entry.at.clone()), Sx::Str(entry.message.clone())];
    if let Some(hint) = &entry.hint {
      rest.push(Sx::Str(hint.clone()));
    }
    out.push(form("client", rest));
  }
  out
}

fn component_to_sx(entry: &ComponentEntry) -> Sx {
  let mut rest = vec![sym(entry.module.clone())];
  rest.extend(component_sections(&entry.body));
  for row in &entry.head {
    let mut terms = vec![sym(row.tag.clone())];
    for (name, value) in &row.attrs {
      terms.push(Sx::Str(name.clone()));
      terms.push(Sx::Str(value.clone()));
    }
    rest.push(form("head", terms));
  }
  form("component", rest)
}

fn derive_to_sx(entry: &DeriveEntry) -> Sx {
  let mut sources = vec![sym("sources".to_owned())];
  sources.extend(entry.sources.iter().map(|key| Sx::Str(key.clone())));
  let mut rest = vec![Sx::Str(entry.key.clone()), Sx::List(sources)];
  match &entry.body {
    Some(body) => rest.push(expr_to_sx(body)),
    None => rest.push(form("browser", vec![Sx::Str(entry.at.clone().unwrap_or_default()), Sx::Str(entry.why.clone().unwrap_or_default())])),
  }
  form("derive", rest)
}

/// The sections a source row can carry, each one export of its loader module.
pub const SOURCE_SECTIONS: [&str; 5] = ["body", "meta", "store", "defaults", "paths"];

/// Why a row or one section of a source row cannot be printed alone.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ViewError {
  #[error("no row is named `{id}`")]
  Unknown { id: String },
  #[error("`{id}` has one body and no sections")]
  OneBody { id: String },
  #[error("a source's sections are body, meta, store, defaults and paths, not `{section}`")]
  NoSuchSection { section: String },
  #[error("`{id}` has no `{section}` section; it carries {present}")]
  Absent { id: String, section: String, present: String },
}

pub fn manifest_from_sx(forms: &[Sx]) -> Res<Manifest> {
  let mut manifest = Manifest {
    version: 0,
    routes: Vec::new(),
    sources: Vec::new(),
    actions: Vec::new(),
    components: Vec::new(),
    clients: Vec::new(),
    consts: Consts::new(),
    not_found: None,
    handlers: Vec::new(),
    middleware: None,
    intercepts: Vec::new(),
    frameworks: Default::default(),
    reads: Default::default(),
    derives: Vec::new(),
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
      "framework" => {
        if items.len() != 3 {
          return Err(err("a framework is `(framework package version)`"));
        }
        manifest.frameworks.insert(as_sym(&items[1])?, as_sym(&items[2])?);
      }
      "reads" => {
        if items.len() < 2 {
          return Err(err("reads are `(reads module key ...)`"));
        }
        manifest.reads.insert(as_sym(&items[1])?, items[2..].iter().map(as_sym).collect::<Res<Vec<_>>>()?);
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
        let (heads, sections): (Vec<&Sx>, Vec<&Sx>) = items[2..].iter().partition(|s| head_of(s).ok().as_deref() == Some("head"));
        let mut sections: Vec<Sx> = sections.into_iter().cloned().collect();
        let named = sections.iter().any(|s| matches!(head_of(s).ok().as_deref(), Some("owner" | "static" | "tree" | "vue")));
        if !named && manifest.version < crate::OWNERS_NAMED {
          sections.push(form("owner", vec![sym("react".to_owned())]));
        }
        let mut head = Vec::new();
        for row in heads {
          let terms = as_list(row)?;
          if terms.len() < 2 || terms.len() % 2 != 0 {
            return Err(err("a head row is `(head tag \"name\" \"value\" ...)`"));
          }
          let tag = as_sym(&terms[1])?;
          let attrs = terms[2..].chunks(2).map(|pair| Ok((as_sym(&pair[0])?, as_sym(&pair[1])?))).collect::<Result<Vec<_>, _>>()?;
          head.push(HeadRow { tag, attrs });
        }
        manifest.components.push(ComponentEntry {
          module: as_sym(&items[1])?,
          body: component_from_sections(&sections)?,
          head,
        });
      }
      "derive" => {
        if items.len() != 4 {
          return Err(err("a derive is `(derive \"key\" (sources \"key\" ...) expr)` or ends in `(browser \"at\" \"why\")`"));
        }
        let sources = as_list(&items[2])?;
        if head_of(&items[2])? != "sources" {
          return Err(err("a derive's second term is `(sources \"key\" ...)`"));
        }
        let sources = sources[1..].iter().map(as_sym).collect::<Res<Vec<_>>>()?;
        let (body, at, why) = match head_of(&items[3]).ok().as_deref() {
          Some("browser") => {
            let terms = as_list(&items[3])?;
            if terms.len() != 3 {
              return Err(err("a derive the build did not lower ends in `(browser \"at\" \"why\")`"));
            }
            (None, Some(as_sym(&terms[1])?), Some(as_sym(&terms[2])?))
          }
          _ => (Some(expr_from_sx(&items[3])?), None, None),
        };
        manifest.derives.push(crate::DeriveEntry { key: as_sym(&items[1])?, sources, body, at, why });
      }
      "client" => {
        if !(4..=5).contains(&items.len()) {
          return Err(err("a client is `(client module \"at\" \"message\")` with an optional hint"));
        }
        manifest.clients.push(ClientEntry {
          module: as_sym(&items[1])?,
          at: as_sym(&items[2])?,
          message: as_sym(&items[3])?,
          hint: items.get(4).map(as_sym).transpose()?,
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
  /// One row as `plan.sexp` writes it: a source, action or handler by its id,
  /// a component by its module id or a derive by its key. `section` names one
  /// export of a source row and prints that section alone.
  pub fn view(&self, id: &str, section: Option<&str>) -> Result<String, ViewError> {
    let unknown = || ViewError::Unknown { id: id.to_owned() };
    if let Some(row) = self.sources.iter().find(|row| row.id == id) {
      let Some(section) = section else { return Ok(print(&[source_to_sx(row)])) };
      if !SOURCE_SECTIONS.contains(&section) {
        return Err(ViewError::NoSuchSection { section: section.to_owned() });
      }
      let body = match section {
        "body" => &row.body,
        "meta" => &row.meta,
        "store" => &row.store,
        "defaults" => &row.defaults,
        _ => &row.paths,
      };
      return match body_form(section, body) {
        Some(sx) => Ok(print(&[sx])),
        None => {
          let present: Vec<&str> = SOURCE_SECTIONS
            .iter()
            .zip([&row.body, &row.meta, &row.store, &row.defaults, &row.paths])
            .filter(|(_, body)| body.is_some())
            .map(|(name, _)| *name)
            .collect();
          let present = if present.is_empty() { "no sections".to_owned() } else { present.join(", ") };
          Err(ViewError::Absent { id: id.to_owned(), section: section.to_owned(), present })
        }
      };
    }
    let sx = if let Some(row) = self.actions.iter().find(|row| row.id == id) {
      action_to_sx(row)
    } else if let Some(row) = self.handlers.iter().find(|row| row.id == id) {
      handler_to_sx(row)
    } else if let Some(row) = self.components.iter().find(|row| row.module == id) {
      component_to_sx(row)
    } else if let Some(row) = self.derives.iter().find(|row| row.key == id) {
      derive_to_sx(row)
    } else {
      return Err(unknown());
    };
    if section.is_some() {
      return Err(ViewError::OneBody { id: id.to_owned() });
    }
    Ok(print(&[sx]))
  }

  /// Every id [`Manifest::view`] answers, in file order.
  pub fn row_ids(&self) -> Vec<String> {
    let mut ids: Vec<String> = self.sources.iter().map(|row| row.id.clone()).collect();
    ids.extend(self.actions.iter().map(|row| row.id.clone()));
    ids.extend(self.handlers.iter().map(|row| row.id.clone()));
    ids.extend(self.components.iter().map(|row| row.module.clone()));
    ids.extend(self.derives.iter().map(|row| row.key.clone()));
    ids
  }

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
