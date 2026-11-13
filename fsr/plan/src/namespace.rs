//! A manifest made a site's artifact, by walking its types.

use std::sync::Arc;

use snapfire_fsr_ir::ast::{Expr, Tmpl};

use crate::{under, Manifest, Node};

impl Manifest {
  /// The manifest as a site's artifact: every id prefixed `<name>:` and every
  /// pattern under `at`, so two sites can carry the same files. The document
  /// module `shell` stays the host's. Inputs name types and are prefixed the
  /// way `Contract::namespaced` prefixes them.
  pub fn namespaced(&self, name: &str, at: &str, shell: &str) -> Manifest {
    let mut manifest = self.clone();
    manifest.namespace(name, at, shell);
    manifest
  }

  /// [`Manifest::namespaced`] in place.
  pub fn namespace(&mut self, name: &str, at: &str, shell: &str) {
    let prefix = format!("{name}:");
    let prefix = prefix.as_str();
    for entry in self.routes.iter_mut().chain(self.intercepts.iter_mut()) {
      entry.pattern = under(at, &entry.pattern);
      node(&mut entry.plan, prefix, shell);
    }
    if let Some(not_found) = &mut self.not_found {
      node(not_found, prefix, shell);
    }
    for row in &mut self.images {
      prefix_string(&mut row.module, prefix);
      prefix_option(&mut row.source, prefix);
      prefix_string(&mut row.page, prefix);
      row.pattern = under(at, &row.pattern);
    }
    for row in &mut self.sources {
      prefix_string(&mut row.id, prefix);
      prefix_option(&mut row.module, prefix);
    }
    for row in &mut self.actions {
      prefix_string(&mut row.id, prefix);
      prefix_option(&mut row.module, prefix);
      prefix_option(&mut row.input, prefix);
    }
    for row in &mut self.handlers {
      prefix_string(&mut row.id, prefix);
      prefix_option(&mut row.module, prefix);
      prefix_option(&mut row.input, prefix);
      row.pattern = under(at, &row.pattern);
    }
    for module in self.components.iter_mut().map(|row| &mut row.module)
      .chain(self.clients.iter_mut().map(|row| &mut row.module))
      .chain(self.heads.iter_mut().map(|row| &mut row.module))
      .chain(self.sheets.iter_mut().map(|row| &mut row.module))
      .chain(self.themes.iter_mut().map(|row| &mut row.module))
    {
      prefix_string(module, prefix);
    }
    for row in &mut self.components {
      placements(&mut row.body.render, prefix);
    }
    self.visit_exprs_mut(&mut |expr| match expr {
      Expr::Const(name) => prefix_string(name, prefix),
      Expr::Call { service, .. } => prefix_string(service, prefix),
      Expr::Markup(tmpl) => placements(Arc::make_mut(tmpl), prefix),
      _ => {}
    });
    self.reads = std::mem::take(&mut self.reads).into_iter().map(|(mut module, keys)| {
      prefix_string(&mut module, prefix);
      (module, keys)
    }).collect();
    self.consts = std::mem::take(&mut self.consts).into_iter().map(|(mut name, value)| {
      prefix_string(&mut name, prefix);
      (name, value)
    }).collect();
  }
}

fn prefix_string(value: &mut String, prefix: &str) {
  if !value.starts_with(prefix) {
    value.insert_str(0, prefix);
  }
}

fn prefix_option(value: &mut Option<String>, prefix: &str) {
  if let Some(value) = value {
    prefix_string(value, prefix);
  }
}

fn node(node: &mut Node, prefix: &str, shell: &str) {
  if node.module != shell {
    prefix_string(&mut node.module, prefix);
  }
  for value in [&mut node.source, &mut node.fallback, &mut node.error, &mut node.cache_key] {
    prefix_option(value, prefix);
  }
  for (_, module) in &mut node.error_kinds {
    prefix_string(module, prefix);
  }
  for child in &mut node.children {
    self::node(&mut child.node, prefix, shell);
  }
}

/// The module of every component and island `tmpl` places, outside the
/// expressions it holds, which the caller's walk over expressions reaches.
fn placements(tmpl: &mut Tmpl, prefix: &str) {
  match tmpl {
    Tmpl::Component { module, children, .. } | Tmpl::Island { module, children, .. } => {
      prefix_string(module, prefix);
      children.iter_mut().for_each(|child| placements(child, prefix));
    }
    Tmpl::Element { children, .. } | Tmpl::Fragment(children) | Tmpl::Baked { children, .. } | Tmpl::Place { children, .. } => children.iter_mut().for_each(|child| placements(child, prefix)),
    Tmpl::If { then, r#else, .. } => {
      placements(then, prefix);
      if let Some(other) = r#else {
        placements(other, prefix);
      }
    }
    Tmpl::For { body, .. } => placements(body, prefix),
    Tmpl::Let { then, .. } => placements(then, prefix),
    Tmpl::Text(_) | Tmpl::Expr(_) | Tmpl::Html(_) | Tmpl::Slot(_) | Tmpl::Marker(_) | Tmpl::Outlet { .. } => {}
  }
}
