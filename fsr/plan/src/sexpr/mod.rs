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

use crate::{
  ComponentEntry, Manifest, RouteEntry, RowOwner, FORMAT_VERSION, OLDEST_READABLE,
};

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
