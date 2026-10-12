//! Prerendering: the routes that read nothing, the files `fsr prerender` writes and the warm loads a boot reads back.

use super::*;

/// What a warm pass writes into the prerender directory and a boot reads back:
/// every memoizable source's data under the key a request composes for it.
pub const LOADS_FILE: &str = "loads.json";

/// Beside the prerendered documents: every file the last run wrote, relative
/// to the directory, which the next run removes before writing its own.
pub const WRITTEN_FILE: &str = "prerendered.json";

/// Beside the prerendered documents: the build they were rendered from, as
/// `build_of` names it. A host booting another build ignores the directory.
pub const BUILD_FILE: &str = "prerender.build";

/// What a prerendered page is rendered from: the plan and every mounted
/// site's, the templates the host reads at boot and each assets manifest,
/// hashed. The configuration and the import map are left out, since a
/// directory prerendered under development's is served under production's,
/// whose import map names the minified client.
pub(crate) fn build_of(plans: &[&str], apps: &[&Path]) -> Result<String, HostError> {
  use sha2::Digest;
  let mut hash = sha2::Sha256::new();
  for plan in plans {
    hash.update(b"plan\0");
    hash.update(plan.as_bytes());
  }
  for app in apps {
    for file in crate::template_files(app)? {
      hash.update(file.strip_prefix(app).unwrap_or(&file).to_string_lossy().as_bytes());
      hash.update(b"\0");
      hash.update(std::fs::read(&file).map_err(|e| HostError::Io(file.clone(), e))?);
    }
    if let Ok(assets) = std::fs::read(app.join(crate::assets::ASSETS_FILE)) {
      hash.update(b"assets\0");
      hash.update(assets);
    }
  }
  Ok(hash.finalize().iter().take(8).map(|b| format!("{b:02x}")).collect())
}

/// Whether `dir` holds a prerender written by a build other than `build`:
/// one that listed its files and named another build or none.
pub(crate) fn written_by_another(dir: &Path, build: &str) -> bool {
  dir.join(WRITTEN_FILE).is_file() && std::fs::read_to_string(dir.join(BUILD_FILE)).ok().as_deref().map(str::trim) != Some(build)
}

/// Beside `loads.json`: every subtree a build rendered ahead of a request,
/// under the memo key a request composes for it, which a boot reads into the
/// render memo in front of whatever `[cache]` configured.
pub const RENDERS_FILE: &str = "renders.json";

pub(crate) fn renders_to_json(entries: &HashMap<String, CacheEntry>) -> String {
  let mut rows: Vec<(&String, &CacheEntry)> = entries.iter().collect();
  rows.sort_by(|a, b| a.0.cmp(b.0));
  let obj: serde_json::Map<String, serde_json::Value> = rows
    .into_iter()
    .map(|(key, entry)| {
      (
        key.clone(),
        serde_json::json!({
          "node": snapfire_fsr_payload::node_to_row_json(&entry.node),
          "segments": entry.segments.iter().map(snapfire_fsr_runtime::segments_to_json).collect::<Vec<_>>(),
          "digest": format!("{:016x}", entry.digest),
        }),
      )
    })
    .collect();
  serde_json::to_string(&serde_json::Value::Object(obj)).expect("a render map serializes")
}

/// What a build rendered or nothing when the file is absent or unreadable: a
/// render pass is an optimization, so a bad entry costs a render rather than
/// a boot.
pub(crate) fn renders_from_file(path: &Path) -> HashMap<String, CacheEntry> {
  let Ok(text) = std::fs::read_to_string(path) else {
    return HashMap::new();
  };
  let Ok(serde_json::Value::Object(obj)) = serde_json::from_str::<serde_json::Value>(&text) else {
    tracing::warn!(target: "fsr::cache", file = %path.display(), "warmed renders are not an object");
    return HashMap::new();
  };
  let mut out = HashMap::new();
  for (key, json) in obj {
    let read = || -> Result<CacheEntry, snapfire_fsr_payload::DecodeError> {
      let fail = |m: &str| snapfire_fsr_payload::DecodeError(m.to_owned());
      let node = snapfire_fsr_payload::row_json_to_node(json.get("node").ok_or_else(|| fail("a render needs `node`"))?)?;
      let mut segments = Vec::new();
      for item in json.get("segments").and_then(|s| s.as_array()).ok_or_else(|| fail("a render needs `segments`"))? {
        segments.push(snapfire_fsr_runtime::segments_from_json(item)?);
      }
      let digest = json.get("digest").and_then(|d| d.as_str()).and_then(|d| u64::from_str_radix(d, 16).ok()).ok_or_else(|| fail("a render needs a hex `digest`"))?;
      Ok(CacheEntry { node, segments, digest })
    };
    match read() {
      Ok(entry) => {
        out.insert(key, entry);
      }
      Err(e) => tracing::warn!(target: "fsr::cache", key = %key, error = %e, "warmed render is unreadable"),
    }
  }
  out
}

pub(crate) fn warm_to_json(warmed: &HashMap<String, Data>) -> String {
  let mut rows: Vec<(&String, &Data)> = warmed.iter().collect();
  rows.sort_by(|a, b| a.0.cmp(b.0));
  let obj: serde_json::Map<String, serde_json::Value> = rows
    .into_iter()
    .map(|(key, data)| {
      (
        key.clone(),
        snapfire_fsr_payload::value_to_json(&Value::Map(data.clone())),
      )
    })
    .collect();
  serde_json::to_string(&serde_json::Value::Object(obj)).expect("a value map serializes")
}

/// What a build warmed or nothing when the file is absent or unreadable: a
/// warm pass is an optimization, so a bad file costs loads rather than a boot.
pub(crate) fn warm_from_file(path: &Path) -> HashMap<String, Data> {
  let Ok(text) = std::fs::read_to_string(path) else {
    return HashMap::new();
  };
  let Ok(serde_json::Value::Object(obj)) = serde_json::from_str::<serde_json::Value>(&text) else {
    tracing::warn!(target: "fsr::load", file = %path.display(), "warmed loads are not an object");
    return HashMap::new();
  };
  let mut out = HashMap::new();
  for (key, json) in obj {
    match snapfire_fsr_payload::json_to_value(&json) {
      Ok(Value::Map(data)) => {
        out.insert(key, data);
      }
      _ => tracing::warn!(target: "fsr::load", key = %key, "warmed load is not a value map"),
    }
  }
  out
}

/// Refuses a plan node naming a template module nothing renders: one no
/// evaluator covers, or one the stock evaluator covers with no template of
/// that name under the app.
pub(crate) fn templates_answered(plan: &PlanNode, evaluators: &snapfire_fsr_runtime::Evaluators, stock: Option<&[String]>) -> Result<(), HostError> {
  if is_template_module(&plan.module) {
    if !evaluators.covers(&plan.module) {
      return Err(HostError::Uncovered(plan.module.to_string()));
    }
    if let Some(names) = stock {
      if !names.iter().any(|n| *n == plan.module.path) {
        return Err(HostError::TemplateMissing(plan.module.to_string()));
      }
    }
  }
  for (_, child) in &plan.children {
    templates_answered(child, evaluators, stock)?;
  }
  Ok(())
}

/// `pattern` with each `{name}` or `{*name}` replaced by `set[name]`; every
/// parameter must be present and no value may be empty.
pub(crate) fn fill_pattern(pattern: &str, set: &Params) -> Result<String, HostError> {
  let mut out = String::with_capacity(pattern.len());
  let mut rest = pattern;
  while let Some(open) = rest.find('{') {
    out.push_str(&rest[..open]);
    let Some(close) = rest[open..].find('}') else {
      return Err(HostError::Paths(pattern.to_owned(), "an unclosed parameter in the pattern".to_owned()));
    };
    let name = rest[open + 1..open + close].trim_start_matches('*');
    match set.get(name) {
      Some(value) if !value.is_empty() => out.push_str(value),
      Some(_) => return Err(HostError::Paths(pattern.to_owned(), format!("a set gives `{name}` an empty value"))),
      None => return Err(HostError::Paths(pattern.to_owned(), format!("a set names no `{name}`: {}", set.keys().map(|k| format!("`{k}`")).collect::<Vec<_>>().join(", ")))),
    }
    rest = &rest[open + close + 1..];
  }
  out.push_str(rest);
  Ok(out)
}

/// What the last prerender under `out` listed in `prerendered.json`, paths
/// relative to `out`; empty when there is no list.
pub(crate) fn read_written(out: &Path) -> Result<Vec<String>, HostError> {
  let list = out.join(WRITTEN_FILE);
  let Ok(text) = std::fs::read_to_string(&list) else {
    return Ok(Vec::new());
  };
  serde_json::from_str(&text).map_err(|e| HostError::Config(list, e.to_string()))
}

/// Writes `bytes` beside `file` and renames it into place, so a request
/// reading the file while a prerender runs gets the old bytes or the new ones.
fn write_whole(file: &Path, bytes: &[u8]) -> Result<(), HostError> {
  let staged = file.with_file_name(format!(".{}.{}.part", file.file_name().unwrap_or_default().to_string_lossy(), std::process::id()));
  std::fs::write(&staged, bytes).map_err(|e| HostError::Io(staged.clone(), e))?;
  std::fs::rename(&staged, file).map_err(|e| HostError::Io(file.to_path_buf(), e))
}

/// Removes every file of `previous` that `kept` does not hold, then the
/// directories that emptied: a path that dropped out of `paths` is not served
/// from a stale file, and a file nothing recorded is left alone.
fn prune(out: &Path, previous: &[String], kept: &[String]) -> Result<(), HostError> {
  for rel in previous.iter().filter(|rel| !kept.contains(rel)) {
    let file = out.join(rel);
    match std::fs::remove_file(&file) {
      Ok(()) => {}
      Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
      Err(e) => return Err(HostError::Io(file, e)),
    }
    let mut dir = file.parent();
    while let Some(d) = dir {
      if d == out || std::fs::remove_dir(d).is_err() {
        break;
      }
      dir = d.parent();
    }
  }
  Ok(())
}

impl Host {
  /// The patterns one render serves for every request: no parameter, every
  /// source lowered and reading nothing of the request, no page or layout
  /// reading its `identity` or `csrf_token` prop.
  pub fn prerenderable(&self) -> Vec<String> {
    self.tables().app.prerenderable.clone()
  }

  /// The patterns one anonymous render serves for every anonymous request:
  /// their only request reads are the identity, a page or layout's `identity`
  /// prop or a call through a client whose `bearer` is set. `prerender`
  /// writes them too; the file serves a visitor with no identity and a
  /// signed-in one is rendered live.
  pub fn prerenderable_anonymous(&self) -> Vec<String> {
    self.tables().app.prerenderable_anonymous.clone()
  }

  /// Warms every memoizable load, then renders every prerenderable route once
  /// per locale, anonymously, writing the document as `<out>/<path>/index.html`
  /// and the payload beside it as `index.payload`; `/` lands at the top of
  /// `out`. A locale other than the default lands under its tag,
  /// `<out>/fr_FR/about/index.html`. A route with a parameter is rendered once
  /// per set its `paths` returns for the locale, at the path the set fills the
  /// pattern to. Returns what was written, each path with its prefix.
  ///
  /// The warm pass runs first and its result replaces whatever an earlier one
  /// left, so a rerun writes documents from the loads it just took rather than
  /// from the file it booted with.
  ///
  /// It may run while the host serves, as often as the application calls it:
  /// each file is renamed into place whole, a page whose loader fails this run
  /// keeps the file the last run wrote, and only once every page is written
  /// are the files of a path that dropped out of `paths` removed, from the list
  /// the last run left in `prerendered.json`. A file nothing recorded is left alone.
  pub async fn prerender(&self, out: &Path) -> Result<Vec<(String, PathBuf)>, HostError> {
    let t = self.tables();
    let mut written = Vec::new();
    let previous = read_written(out)?;
    let mut held: Vec<PathBuf> = Vec::new();
    let warmed = self.warm(&t).await?;
    t.warm.replace(warmed.clone());
    if !warmed.is_empty() {
      std::fs::create_dir_all(out).map_err(|e| HostError::Io(out.to_path_buf(), e))?;
      let file = out.join(LOADS_FILE);
      write_whole(&file, warm_to_json(&warmed).as_bytes())?;
      written.push((LOADS_FILE.to_owned(), file));
    }
    for pattern in t
      .app
      .prerenderable
      .iter()
      .chain(t.app.prerenderable_anonymous.iter())
      .cloned()
      .collect::<Vec<_>>()
    {
      for tag in t.locales.supported.clone() {
        let locale = t.locales.locale(&tag);
        let root = if locale.is_default {
          out.to_path_buf()
        } else {
          out.join(&tag)
        };
        let paths = match t.app.paths.get(&pattern) {
          Some(enumerated) => {
            let ctx = self.ctx(&t, Incoming::anonymous(SessionCell::default()), Params::new(), Params::new(), "/", locale.clone());
            let sets = enumerated.paths(&ctx).await.map_err(|e| HostError::Paths(pattern.clone(), e.message))?;
            sets.iter().map(|set| fill_pattern(&pattern, set)).collect::<Result<Vec<_>, _>>()?
          }
          None => vec![pattern.clone()],
        };
        for path in paths {
          let dir = root.join(path.trim_matches('/'));
          std::fs::create_dir_all(&dir).map_err(|e| HostError::Io(dir.clone(), e))?;
          let served = if locale.is_default {
            path.clone()
          } else {
            format!("/{tag}{}", path.trim_end_matches('/'))
          };
          for (mode, name) in [(RenderMode::Html, "index.html"), (RenderMode::Payload, "index.payload")] {
            let (plan, params) = self
              .plan_for(&t, &path)
              .ok_or_else(|| HostError::NotFound(path.clone()))?;
            let rendered = self
              .render_plan_with(
                &t,
                &plan,
                params,
                Params::new(),
                mode,
                Incoming::anonymous(SessionCell::default()),
                &served,
                &locale,
                t.head_for(&locale.tag).as_ref().unwrap_or(&t.head),
              )
              .await?;
            if let Some(kind) = rendered.failed {
              tracing::warn!(target: "fsr::host", path = %served, kind = kind.as_str(), "prerender: the page's loader failed, so no new file is written for it and the last run's stays");
              held.extend(["index.html", "index.payload"].map(|name| dir.join(name)));
              break;
            }
            let text: String = rendered.chunks.collect::<Vec<_>>().await.concat();
            let file = dir.join(name);
            write_whole(&file, text.as_bytes())?;
            written.push((served.clone(), file));
          }
        }
      }
    }
    if !t.app.renderable.is_empty() {
      t.renders.replace(HashMap::new());
      t.renders.record(true);
      for (pattern, subtree) in &t.app.renderable {
        for tag in t.locales.supported.clone() {
          let locale = t.locales.locale(&tag);
          let paths = match t.app.paths.get(pattern) {
            Some(enumerated) => {
              let ctx = self.ctx(&t, Incoming::anonymous(SessionCell::default()), Params::new(), Params::new(), "/", locale.clone());
              let sets = enumerated.paths(&ctx).await.map_err(|e| HostError::Paths(pattern.clone(), e.message))?;
              sets.iter().map(|set| fill_pattern(pattern, set)).collect::<Result<Vec<_>, _>>()?
            }
            None => vec![pattern.clone()],
          };
          for path in paths {
            let Some((_, params)) = self.plan_for(&t, &path) else { continue };
            let ctx = self.ctx(&t, Incoming::anonymous(SessionCell::default()), params, Params::new(), &path, locale.clone());
            if let Err(e) = assemble(&t.app.runtime, subtree, &ctx, &t.head).await {
              tracing::warn!(target: "fsr::cache", pattern = %pattern, module = %subtree.module, error = %e, "warm render failed");
            }
          }
        }
      }
      t.renders.record(false);
      let entries = t.renders.entries();
      if !entries.is_empty() {
        std::fs::create_dir_all(out).map_err(|e| HostError::Io(out.to_path_buf(), e))?;
        let file = out.join(RENDERS_FILE);
        write_whole(&file, renders_to_json(&entries).as_bytes())?;
        written.push((RENDERS_FILE.to_owned(), file));
      }
    }
    // The documents' import map names the client under `client::ROUTE`, which
    // this host answers from the binary; a directory served by anything else
    // needs the files, so they go beside the documents whenever a document was written.
    if t.client && written.iter().any(|(_, file)| file.extension().is_some_and(|e| e == "html")) {
      let dir = out.join(client::ROUTE.trim_start_matches('/'));
      std::fs::create_dir_all(&dir).map_err(|e| HostError::Io(dir.clone(), e))?;
      for (name, _) in client::FILES {
        let Some(body) = client::get(name, t.client_minified) else { continue };
        let file = dir.join(name);
        write_whole(&file, body.as_bytes())?;
        written.push((format!("{}/{name}", client::ROUTE), file));
      }
    }
    let relative = |file: &Path| file.strip_prefix(out).ok().map(|rel| rel.to_string_lossy().replace('\\', "/"));
    let mut listed: Vec<String> = written.iter().filter_map(|(_, file)| relative(file)).collect();
    listed.extend(held.iter().filter_map(|file| relative(file)).filter(|rel| previous.contains(rel)));
    prune(out, &previous, &listed)?;
    let build = out.join(BUILD_FILE);
    write_whole(&build, t.build.as_bytes())?;
    written.push((BUILD_FILE.to_owned(), build));
    if t.prerendered.as_deref() == Some(out) {
      t.prerender_current.store(true, std::sync::atomic::Ordering::Release);
    }
    let file = out.join(WRITTEN_FILE);
    if listed.is_empty() {
      match std::fs::remove_file(&file) {
        Ok(()) => {}
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => return Err(HostError::Io(file, e)),
      }
    } else {
      write_whole(&file, serde_json::to_string_pretty(&listed).expect("a list of paths serializes").as_bytes())?;
      written.push((WRITTEN_FILE.to_owned(), file));
    }
    Ok(written)
  }

  /// Runs every warmable source once per locale with nothing of a request
  /// behind it, keyed the way a request will key it. A source that fails is
  /// left out rather than written as a failure: a request loads it live and
  /// degrades to its error node the way it does without a warm pass.
  pub(crate) async fn warm(&self, t: &Tables) -> Result<HashMap<String, Data>, HostError> {
    let mut warmed = HashMap::new();
    for name in &t.app.warmable {
      let id = snapfire_fsr_core::DataSourceId(name.clone());
      let Some(source) = t.app.runtime.sources.get(&id) else {
        continue;
      };
      for tag in t.locales.supported.clone() {
        let locale = t.locales.locale(&tag);
        let ctx = self.ctx(
          t,
          Incoming::anonymous(SessionCell::default()),
          Params::new(),
          Params::new(),
          "/",
          locale,
        );
        let Some(key) = t.app.runtime.load_keyer.key(&id, &ctx) else {
          continue;
        };
        if warmed.contains_key(&key) {
          continue;
        }
        match source.load(&ctx).await {
          Ok(data) => {
            warmed.insert(key, data);
          }
          Err(e) => tracing::warn!(target: "fsr::load", source = %name, error = %e, "warm load failed"),
        }
      }
    }
    Ok(warmed)
  }

  /// What `prerender` would write for `served` now: the page rendered the way
  /// a run renders it, anonymously with the plain head, after its loaders run.
  /// `path` is `served` without its locale prefix. Development answers a
  /// `?__prerendered` request with it, so a page is previewed whether or not
  /// a run has written it. `Err` says why the route has no prerendered page.
  pub(crate) async fn preview(&self, t: &Tables, path: &str, served: &str, mode: RenderMode, locale: &Locale) -> Result<String, String> {
    let matched = t.app.matcher.match_path(path).or_else(|| t.app.matcher.match_path(path.trim_end_matches('/')));
    let Some(pattern) = matched.and_then(|m| t.app.patterns.get(m.entry.0 as usize)).cloned() else {
      return Err(format!("no route: {served}"));
    };
    if !t.app.prerenderable.contains(&pattern) && !t.app.prerenderable_anonymous.contains(&pattern) {
      return Err(format!("`{pattern}` is not prerendered: its loaders read the request, so it renders live for every visitor"));
    }
    if let Some(enumerated) = t.app.paths.get(&pattern) {
      let ctx = self.ctx(t, Incoming::anonymous(SessionCell::default()), Params::new(), Params::new(), "/", locale.clone());
      let sets = enumerated.paths(&ctx).await.map_err(|e| format!("the paths of `{pattern}` failed: {}", e.message))?;
      let listed = sets.iter().map(|set| fill_pattern(&pattern, set)).collect::<Result<Vec<_>, _>>().map_err(|e| e.to_string())?;
      if !listed.iter().any(|p| p == path || p.trim_end_matches('/') == path.trim_end_matches('/')) {
        return Err(format!("`{path}` is not one of the paths `{pattern}` exports, so it renders live"));
      }
    }
    let (plan, params) = self.plan_for(t, path).ok_or_else(|| format!("no route: {served}"))?;
    let rendered = self
      .render_plan_with(t, &plan, params, Params::new(), mode, Incoming::anonymous(SessionCell::default()), served, locale, t.head_for(&locale.tag).as_ref().unwrap_or(&t.head))
      .await
      .map_err(|e| e.to_string())?;
    if let Some(kind) = rendered.failed {
      return Err(format!("the page's loader failed ({}), so a prerender writes no file for it", kind.as_str()));
    }
    Ok(rendered.chunks.collect::<Vec<_>>().await.concat())
  }

  /// The prerendered text for `path` in `mode`, when the prerender directory
  /// holds one. `path` may carry a locale prefix. The query string is
  /// ignored: a prerenderable route reads none.
  pub fn prerendered(&self, path: &str, mode: RenderMode) -> Option<String> {
    let t = self.tables();
    let path = path.split_once('?').map(|(p, _)| p).unwrap_or(path);
    let visit = t.locales.resolve(path, None, None);
    self.prerendered_in(&t, &visit.path, &mode, &visit.locale, true)
  }

  /// `anonymous` says the request carries no identity; a route prerendered
  /// for anonymous visitors only serves its file then. The route is found by
  /// matching, since a pattern with `paths` covers many files.
  pub(crate) fn prerendered_in(
    &self,
    t: &Tables,
    path: &str,
    mode: &RenderMode,
    locale: &Locale,
    anonymous: bool,
  ) -> Option<String> {
    let dir = t.prerendered.as_ref()?;
    if !t.prerender_current.load(std::sync::atomic::Ordering::Acquire) {
      return None;
    }
    if !anonymous {
      let matched = t.app.matcher.match_path(path).or_else(|| t.app.matcher.match_path(path.trim_end_matches('/')));
      let pattern = matched.and_then(|m| t.app.patterns.get(m.entry.0 as usize));
      if pattern.is_some_and(|pattern| t.app.prerenderable_anonymous.contains(pattern)) {
        return None;
      }
    }
    let root = if locale.is_default {
      dir.clone()
    } else {
      dir.join(&locale.tag)
    };
    let name = match mode {
      RenderMode::Html => "index.html",
      RenderMode::Payload => "index.payload",
      RenderMode::Fragment(_) => return None,
    };
    std::fs::read_to_string(root.join(path.trim_matches('/')).join(name)).ok()
  }
}
