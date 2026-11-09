use super::*;

/// Refuses two rows claiming one id. Ids are derived from directory names, so
/// a collision reaches the host as `Bind(Claimed(..))` naming neither file and
/// the generated module as a type declared twice; this names both.
pub(crate) fn claimed(report: &Report, routes: &[Route], handler_routes: &[Route], layout_ids: &[String]) -> Result<(), BuildError> {
  for (kind, rows) in [("source", &report.sources), ("action", &report.actions), ("handler", &report.handlers)] {
    let mut seen: std::collections::HashMap<&str, &str> = std::collections::HashMap::new();
    for (id, module) in rows {
      if let Some(first) = seen.insert(id, module) {
        return Err(BuildError::ClaimedId { kind: kind.to_owned(), id: id.clone(), first: first.to_owned(), second: module.clone() });
      }
    }
  }
  for (kind, rows) in [("route", routes), ("handler route", handler_routes)] {
    let mut seen: std::collections::HashMap<&str, &Path> = std::collections::HashMap::new();
    for route in rows {
      if let Some(first) = seen.insert(&route.id, &route.dir) {
        return Err(BuildError::ClaimedId {
          kind: kind.to_owned(),
          id: route.id.clone(),
          first: first.display().to_string(),
          second: route.dir.display().to_string(),
        });
      }
    }
  }
  let mut answered: std::collections::HashMap<String, &Path> = std::collections::HashMap::new();
  for route in routes.iter().chain(handler_routes) {
    if let Some(first) = answered.insert(matched_shape(&route.pattern), &route.dir) {
      return Err(BuildError::ClaimedPattern { pattern: route.pattern.clone(), first: first.display().to_string(), second: route.dir.display().to_string() });
    }
  }
  let mut named: std::collections::HashMap<String, &str> = std::collections::HashMap::new();
  for id in routes.iter().map(|r| r.id.as_str()).chain(layout_ids.iter().map(String::as_str)) {
    if let Some(first) = named.insert(props_name(id), id) {
      return Err(BuildError::ClaimedId { kind: "props type".to_owned(), id: props_name(id), first: first.to_owned(), second: id.to_owned() });
    }
  }
  Ok(())
}

/// Refuses a file under the public root whose URL a route, a generated image,
/// `[crawl]`'s `robots.txt` or sitemap or the root icon also answers. A
/// pattern with a parameter is not compared: the file wins it in every mode.
pub(crate) fn public_claimed<'a>(app: &Path, patterns: impl Iterator<Item = &'a str>) -> Result<(), BuildError> {
  let Some(config) = config_beside(app) else { return Ok(()) };
  let Some(root) = config.statics.iter().find(|s| s.route.trim_end_matches('/').is_empty()) else { return Ok(()) };
  let dir = config.resolve(&root.dir);
  let mut answered: Vec<(String, String)> = patterns.filter(|p| !p.contains('{')).map(|p| (p.to_owned(), "a route".to_owned())).collect();
  if let Some(crawl) = &config.crawl {
    answered.extend(crawl.robots.iter().map(|p| (p.clone(), "[crawl] robots".to_owned())));
    answered.extend(crawl.sitemap.iter().map(|p| (p.clone(), "[crawl] sitemap".to_owned())));
  }
  if let Some(icon) = &config.favicon {
    answered.push(("/favicon.ico".to_owned(), format!("the icons root's {icon}")));
  }
  let mut stack = vec![dir.clone()];
  while let Some(at) = stack.pop() {
    let Ok(entries) = std::fs::read_dir(&at) else { continue };
    for entry in entries.filter_map(Result::ok) {
      let path = entry.path();
      if path.is_dir() {
        stack.push(path);
        continue;
      }
      let rel = path.strip_prefix(&dir).unwrap_or(&path).to_string_lossy().replace('\\', "/");
      let url = format!("/{rel}");
      if let Some((_, by)) = answered.iter().find(|(p, _)| p.trim_end_matches('/') == url) {
        return Err(BuildError::PublicClaimed { file: format!("{}/{rel}", root.dir.trim_end_matches('/')), path: url, by: by.clone() });
      }
    }
  }
  Ok(())
}

/// A pattern with its parameters' names left out, which is what two patterns
/// match the same URLs by.
pub(crate) fn matched_shape(pattern: &str) -> String {
  pattern
    .split('/')
    .map(|segment| match segment.strip_prefix('{') {
      Some(param) if param.starts_with('*') => "{*}",
      Some(_) => "{}",
      None => segment,
    })
    .collect::<Vec<_>>()
    .join("/")
}

/// `$root` is `RootProps`, `product.$id` is `ProductIdProps`, `admin.users` is
/// `AdminUsersProps`, `(shop).cart` is `ShopCartProps`. A parameter's `$`
/// marker and a group's parentheses are not part of the name. A name that
/// would start with a digit, `030-intro`, starts with `Route` instead, since a
/// TypeScript identifier cannot: `Route030IntroProps`.
pub(crate) fn props_name(id: &str) -> String {
  let mut name = String::new();
  for part in id.split(['.', '-', '_']) {
    let mut chars = part.trim_start_matches(['$', '(']).trim_end_matches(')').chars();
    if let Some(first) = chars.next() {
      name.extend(first.to_uppercase());
      name.push_str(chars.as_str());
    }
  }
  // `HostProps` is the type every route's props extend.
  if name == "Host" || name.starts_with(|c: char| c.is_ascii_digit()) {
    name.insert_str(0, "Route");
  }
  name + "Props"
}

/// An object key as written: bare when it is an identifier, quoted otherwise,
/// so `sign-up` and `030-intro` are `"sign-up"` and `"030-intro"`.
pub(crate) fn object_key(name: &str) -> String {
  let identifier = name.starts_with(|c: char| c.is_ascii_alphabetic() || c == '_' || c == '$') && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '$');
  match identifier {
    true => name.to_owned(),
    false => serde_json::to_string(name).expect("a string serializes"),
  }
}

/// Every module a component places as an island, in tree order.
/// Every component a template renders inline, an island's children included.
pub(crate) fn inline_components(tmpl: &snapfire_fsr_ir::Tmpl) -> Vec<String> {
  use snapfire_fsr_ir::Tmpl;
  fn walk(tmpl: &Tmpl, out: &mut Vec<String>) {
    match tmpl {
      Tmpl::Component { module, children, .. } => {
        if !out.contains(module) {
          out.push(module.clone());
        }
        children.iter().for_each(|c| walk(c, out));
      }
      Tmpl::Baked { children, .. } | Tmpl::Island { children, .. } | Tmpl::Element { children, .. } | Tmpl::Fragment(children) | Tmpl::Place { children, .. } => children.iter().for_each(|c| walk(c, out)),
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

pub(crate) fn island_modules(tmpl: &snapfire_fsr_ir::Tmpl) -> Vec<Placement> {
  use snapfire_fsr_ir::Tmpl;
  let mut out = Vec::new();
  fn walk(tmpl: &Tmpl, out: &mut Vec<Placement>) {
    match tmpl {
      Tmpl::Baked { children, .. } => children.iter().for_each(|c| walk(c, out)),
      Tmpl::Island { module, children, define, mode, .. } => {
        out.push(Placement { module: module.clone(), define: *define, server: mode.as_deref() == Some(snapfire_fsr_ir::render::SERVER_MODE) });
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
  walk(tmpl, &mut out);
  out
}

/// Every file an island reaches with the dialect's placements pointed at the
/// island's adapter, by path in the bundle overlay.
pub(crate) fn dialect_rewrites(app: &Path, set: &mut ComponentSet, islands: &[String], static_modules: &[String], defines: &[String], owners: &HashMap<String, Owner>, rewritten: &[(String, String)]) -> Result<Vec<(String, String)>, BuildError> {
  let dialect = format!("\"{TEMPLATE_SPECIFIER}\"");
  let mut out: Vec<(String, String)> = Vec::new();
  let mut seen: Vec<String> = Vec::new();
  for module in islands.iter().filter(|m| !static_modules.contains(m) && !defines.contains(m)) {
    let Some(file) = module.split_once('#').map(|(file, _)| file) else { continue };
    if snapfire_fsr_lower::component::is_foreign(file) {
      continue;
    }
    let island = adapter_for(module, owners)?;
    for reached in set.local_reach(file) {
      if seen.contains(&reached) {
        continue;
      }
      seen.push(reached.clone());
      let source = match rewritten.iter().find(|(f, _)| *f == reached) {
        Some((_, text)) => text.clone(),
        None => {
          let path = app.join(&reached);
          std::fs::read_to_string(&path).map_err(|e| BuildError::Io(path, e))?
        }
      };
      if source.contains(&dialect) {
        out.push((reached, source.replace(&dialect, &format!("\"{}\"", island.module))));
      }
    }
  }
  Ok(out)
}
