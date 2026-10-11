//! The bundle as the document names it: minified twins, client URLs, the preload set, the CSP and the asset head rows.

use super::*;

/// Every module the page fetches before an island can mount, as URLs: the
/// entry's own static imports out of the bundle's graph, the import map's
/// answer for each bare specifier the bundle carries, and whatever the
/// embedded client reaches from there. A module an island pulls in with
/// `import()` is left out, since which islands a document holds is not known
/// until it renders. The entry itself is left out too: it is already a
/// `<script type="module">` on the page.
/// The import map names the build the host serves. A minified module imports its siblings by
/// their `.min.js` names, so a specifier resolved to a plain name would be a second copy of that
/// module, holding its own store.
/// The application's bundle, when the facts say the compiler wrote its minified twins and the host
/// is not a development one.
pub(crate) fn minified_bundle(config: &Config) -> Option<&config::Bundle> {
  minified_under(config.bundle.as_ref(), config.dev())
}

/// A bundle served minified under a host whose `dev` is given: a mounted site's follows the shell.
pub(crate) fn minified_under(bundle: Option<&config::Bundle>, dev: bool) -> Option<&config::Bundle> {
  bundle.filter(|bundle| bundle.minified.is_some() && !dev)
}

pub(crate) fn minified_url(url: &str, bundle: Option<&config::Bundle>) -> String {
  bundle.map_or_else(|| url.to_owned(), |bundle| bundle.minified_url(url))
}

pub(crate) fn minified_urls(urls: Vec<String>, bundle: Option<&config::Bundle>) -> Vec<String> {
  urls.into_iter().map(|url| minified_url(&url, bundle)).collect()
}

/// The import map with every URL under `bundle` naming its minified twin, for a specifier the
/// application maps to one of its own modules.
pub(crate) fn bundle_urls(text: &str, bundle: Option<&config::Bundle>) -> String {
  let Some(bundle) = bundle else { return text.to_owned() };
  let Ok(mut value) = serde_json::from_str::<serde_json::Value>(text) else {
    return text.to_owned();
  };
  let mut changed = false;
  minify_bundle_urls(&mut value, bundle, &mut changed);
  match changed {
    true => serde_json::to_string(&value).unwrap_or_else(|_| text.to_owned()),
    false => text.to_owned(),
  }
}

pub(crate) fn minify_bundle_urls(value: &mut serde_json::Value, bundle: &config::Bundle, changed: &mut bool) {
  match value {
    serde_json::Value::Object(map) => {
      for (_, held) in map.iter_mut() {
        minify_bundle_urls(held, bundle, changed);
      }
    }
    serde_json::Value::Array(items) => {
      for held in items {
        minify_bundle_urls(held, bundle, changed);
      }
    }
    serde_json::Value::String(url) => {
      let twin = bundle.minified_url(url);
      if twin != *url {
        *url = twin;
        *changed = true;
      }
    }
    _ => {}
  }
}

pub(crate) fn client_urls(text: &str, minified: bool) -> String {
  if !minified {
    return text.to_owned();
  }
  let Ok(mut value) = serde_json::from_str::<serde_json::Value>(text) else {
    return text.to_owned();
  };
  let mut changed = false;
  minify_client_urls(&mut value, &format!("{}/", client::ROUTE), &mut changed);
  match changed {
    true => serde_json::to_string(&value).unwrap_or_else(|_| text.to_owned()),
    false => text.to_owned(),
  }
}

pub(crate) fn minify_client_urls(value: &mut serde_json::Value, prefix: &str, changed: &mut bool) {
  match value {
    serde_json::Value::Object(map) => {
      for (_, held) in map.iter_mut() {
        minify_client_urls(held, prefix, changed);
      }
    }
    serde_json::Value::Array(items) => {
      for held in items {
        minify_client_urls(held, prefix, changed);
      }
    }
    serde_json::Value::String(url) => {
      let Some(stem) = url.strip_prefix(prefix).and_then(|rest| rest.strip_suffix(".js")) else {
        return;
      };
      if !stem.ends_with(".min") {
        *url = format!("{prefix}{stem}.min.js");
        *changed = true;
      }
    }
    _ => {}
  }
}

pub(crate) fn preload_set(config: &Config, import_map: Option<&str>, twins: Option<&config::Bundle>) -> Vec<String> {
  let Some(bundle) = &config.bundle else { return Vec::new() };
  // A `[[static]]` root on the client prefix is an application serving its own
  // copy, which the host has not read, so the names the import map gives are
  // the only ones it knows are there.
  let serves_client = !config.statics.iter().any(|s| s.route == client::ROUTE);
  let minified = serves_client && config.document.client.minified(config.dev());

  let map: std::collections::BTreeMap<String, String> = import_map
    .and_then(|text| serde_json::from_str::<serde_json::Value>(text).ok())
    .and_then(|value| value.get("imports").cloned())
    .and_then(|imports| serde_json::from_value(imports).ok())
    .unwrap_or_default();

  let entry = config.document.entry.as_deref().unwrap_or_default();
  let relative = entry.strip_prefix(&bundle.route).unwrap_or(entry).trim_start_matches('/');

  let mut urls: Vec<String> = Vec::new();
  let push = |urls: &mut Vec<String>, url: String| {
    if url != entry && !urls.contains(&url) {
      urls.push(url);
    }
  };

  for module in bundle.graph.get(relative).into_iter().flatten() {
    push(&mut urls, minified_url(&format!("{}/{module}", bundle.route), twins));
  }

  let prefix = format!("{}/", client::ROUTE);
  let mut specifiers: Vec<String> = bundle.externals.clone();
  let mut modules: Vec<String> = Vec::new();
  let mut seen: Vec<String> = Vec::new();
  let mut walked: Vec<String> = Vec::new();

  while !specifiers.is_empty() || !modules.is_empty() {
    while let Some(specifier) = specifiers.pop() {
      if seen.contains(&specifier) {
        continue;
      }
      seen.push(specifier.clone());
      let Some(url) = map.get(&specifier) else { continue };
      push(&mut urls, url.clone());
      if let Some(name) = url.strip_prefix(&prefix) {
        modules.push(name.to_owned());
      }
    }
    while let Some(name) = modules.pop() {
      if walked.contains(&name) {
        continue;
      }
      walked.push(name.clone());
      push(&mut urls, format!("{prefix}{name}"));
      for import in client::imports(&name, minified) {
        match import.strip_prefix("./") {
          Some(sibling) => modules.push(sibling.to_owned()),
          None => specifiers.push(import.to_owned()),
        }
      }
    }
  }
  urls
}

/// A payload and a fragment carry no policy: the document they belong to does.
pub(crate) fn set_csp(t: &Tables, mode: &RenderMode, response: &mut Response<Body>) {
  if !matches!(mode, RenderMode::Html) {
    return;
  }
  if let Some(policy) = &t.csp {
    response.headers_mut().insert(header::CONTENT_SECURITY_POLICY, policy.clone());
  }
  if let Some(policy) = &t.csp_report_only {
    response
      .headers_mut()
      .insert(header::CONTENT_SECURITY_POLICY_REPORT_ONLY, policy.clone());
  }
}

/// The nonce a development host puts on its refresh script. Per boot rather
/// than per response, which a prerendered document or the render memo would
/// otherwise replay; neither runs under `dev`.
pub(crate) fn dev_nonce() -> String {
  use base64::Engine;
  use sha2::Digest;
  let seed = std::time::SystemTime::now()
    .duration_since(std::time::UNIX_EPOCH)
    .map(|d| d.as_nanos())
    .unwrap_or_default();
  let digest = sha2::Sha256::digest(format!("{seed}{:?}", std::process::id()).as_bytes());
  base64::engine::general_purpose::STANDARD.encode(&digest[..16])
}

/// Whether a static path names a file the build emitted under a content
/// hash, `hero.0a1b2c3d.png` or `hero.0a1b2c3d.640.avif`, whose bytes never
/// change under that name.
pub(crate) fn is_hashed_name(path: &str) -> bool {
  let name = path.rsplit('/').next().unwrap_or(path);
  let mut parts: Vec<&str> = name.split('.').collect();
  if parts.len() < 3 {
    return false;
  }
  parts.pop();
  if parts.last().is_some_and(|p| !p.is_empty() && p.chars().all(|c| c.is_ascii_digit())) && parts.len() > 2 {
    parts.pop();
  }
  parts.last().is_some_and(|p| p.len() == 8 && p.chars().all(|c| c.is_ascii_hexdigit()))
}

/// The head rows the asset manifest asks for: the policy the page's
/// `Picture` runtime reads, the inline font CSS, one preload per face marked
/// for it and, for a provider's stylesheet, the preconnect and the link.
pub(crate) fn asset_head_rows(manifest: &crate::assets::AssetsManifest) -> Vec<snapfire_fsr_runtime::HeadEl> {
  use snapfire_fsr_runtime::HeadEl;
  let row = |tag: &str, attrs: &[(&str, &str)]| HeadEl { tag: tag.to_owned(), attrs: attrs.iter().map(|(k, v)| ((*k).to_owned(), (*v).to_owned())).collect(), children: None };
  let mut rows = vec![row("meta", &[("name", "sf:images"), ("content", &manifest.policy_json())])];
  for remote in &manifest.fonts.remote {
    for origin in &remote.preconnect {
      rows.push(row("link", &[("rel", "preconnect"), ("href", origin), ("crossorigin", "")]));
    }
  }
  for href in &manifest.fonts.preload {
    rows.push(row("link", &[("rel", "preload"), ("as", "font"), ("href", href), ("crossorigin", "")]));
  }
  if !manifest.fonts.css.is_empty() {
    rows.push(HeadEl { tag: "style".to_owned(), attrs: vec![("data-sf-fonts".to_owned(), String::new())], children: Some(manifest.fonts.css.clone()) });
  }
  for remote in &manifest.fonts.remote {
    rows.push(row("link", &[("rel", "stylesheet"), ("href", &remote.href)]));
  }
  rows
}

/// The CSP sources the asset manifest needs: the origin fonts and images are
/// served from when a base or a remote names one.
pub(crate) fn asset_csp_sources(manifest: &crate::assets::AssetsManifest) -> Vec<(&'static str, String)> {
  let origin = |url: &str| -> Option<String> {
    let (scheme, rest) = url.split_once("://")?;
    Some(format!("{scheme}://{}", rest.split('/').next()?))
  };
  let mut sources = Vec::new();
  if let Some(base) = manifest.fonts.base.as_deref().and_then(origin) {
    sources.push(("font-src", base));
  }
  for remote in &manifest.fonts.remote {
    for pre in &remote.preconnect {
      sources.push(("font-src", pre.clone()));
      sources.push(("style-src", pre.clone()));
    }
  }
  if let Some(base) = manifest.images.base.as_deref().and_then(origin) {
    sources.push(("img-src", base));
  }
  for template in manifest.images.sources.values() {
    if let Some(host) = origin(template) {
      sources.push(("img-src", host));
    }
  }
  sources
}

/// The CSP `script-src` source for the document's inline import map, which is
/// the only executable inline script a page carries.
pub(crate) fn import_map_csp(text: &str) -> String {
  use base64::Engine;
  use sha2::Digest;
  let digest = sha2::Sha256::digest(text.as_bytes());
  format!("'sha256-{}'", base64::engine::general_purpose::STANDARD.encode(digest))
}
