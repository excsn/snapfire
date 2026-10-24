//! `[pwa]`: the web app manifest, the service worker and the head rows that name them.

use std::collections::BTreeMap;

use bytes::Bytes;
use snapfire_fsr_runtime::HeadEl;

use crate::config::{Config, PwaSection};
use crate::{client, HostError};

pub(crate) const MANIFEST_ROUTE: &str = "/manifest.webmanifest";
pub(crate) const WORKER_ROUTE: &str = "/sw.js";

/// What the host answers for `[pwa]`: the manifest always, the worker only outside development.
pub(crate) struct Pwa {
  pub(crate) manifest: Bytes,
  pub(crate) worker: Option<Bytes>,
}

/// The head rows `[pwa]` adds: the manifest link in place of any the icons directory gave, the theme colour when no row sets one and, with a worker, the meta the client registers it from.
pub(crate) fn head_rows(rows: &mut Vec<HeadEl>, section: &PwaSection, worker: bool) {
  let attr = |row: &HeadEl, name: &str| row.attrs.iter().find(|(k, _)| k == name).map(|(_, v)| v.clone());
  rows.retain(|row| !(row.tag == "link" && attr(row, "rel").is_some_and(|rel| rel.split_whitespace().any(|r| r == "manifest"))));
  rows.push(HeadEl::new("link".to_owned(), vec![("rel".to_owned(), "manifest".to_owned()), ("href".to_owned(), MANIFEST_ROUTE.to_owned())], None));
  if let Some(color) = &section.theme_color {
    if !rows.iter().any(|row| row.tag == "meta" && attr(row, "name").as_deref() == Some("theme-color")) {
      rows.push(HeadEl::new("meta".to_owned(), vec![("name".to_owned(), "theme-color".to_owned()), ("content".to_owned(), color.clone())], None));
    }
  }
  if worker {
    rows.push(HeadEl::new("meta".to_owned(), vec![("name".to_owned(), "sf:worker".to_owned()), ("content".to_owned(), WORKER_ROUTE.to_owned())], None));
  }
}

/// The manifest and, outside development, the worker. `import_map` and `styles` are what the document names; the worker warms each local URL among them.
pub(crate) fn serve(config: &Config, section: &PwaSection, import_map: Option<&str>, styles: &[String], client_minified: bool) -> Result<Pwa, HostError> {
  let manifest = match &section.file {
    Some(file) => read(config, file)?,
    None => Bytes::from(manifest(config, section)),
  };
  let worker = match (config.dev(), &section.worker) {
    (true, _) => None,
    (false, Some(file)) => Some(read(config, file)?),
    (false, None) => Some(Bytes::from(worker(config, section, import_map, styles, client_minified))),
  };
  Ok(Pwa { manifest, worker })
}

fn read(config: &Config, file: &str) -> Result<Bytes, HostError> {
  let path = config.resolve(file);
  std::fs::read(&path).map(Bytes::from).map_err(|e| HostError::Io(path, e))
}

fn manifest(config: &Config, section: &PwaSection) -> String {
  let mut out = serde_json::Map::new();
  let mut put = |key: &str, value: Option<&String>| {
    if let Some(value) = value {
      out.insert(key.to_owned(), serde_json::Value::String(value.clone()));
    }
  };
  put("name", section.name.as_ref());
  put("short_name", section.short_name.as_ref());
  put("description", section.description.as_ref());
  put("start_url", Some(&section.start_url.clone().unwrap_or_else(|| "/".to_owned())));
  put("display", Some(&section.display.clone().unwrap_or_else(|| "standalone".to_owned())));
  put("theme_color", section.theme_color.as_ref());
  put("background_color", section.background_color.as_ref());
  out.insert("icons".to_owned(), serde_json::Value::Array(icons(config)));
  serde_json::Value::Object(out).to_string()
}

/// Each PNG, WebP or SVG in the icons directory whose name carries its size, `android-chrome-192x192.png`, smallest first; an SVG without one is `any`.
fn icons(config: &Config) -> Vec<serde_json::Value> {
  let Some(root) = config.statics.iter().find(|s| s.route.ends_with("/static/icons")) else { return Vec::new() };
  let Ok(entries) = std::fs::read_dir(config.resolve(&root.dir)) else { return Vec::new() };
  let mut found: Vec<(u32, String, String, &'static str)> = Vec::new();
  for entry in entries.flatten() {
    let name = entry.file_name().to_string_lossy().into_owned();
    let Some((stem, ext)) = name.rsplit_once('.') else { continue };
    let kind = match ext {
      "png" => "image/png",
      "webp" => "image/webp",
      "svg" => "image/svg+xml",
      _ => continue,
    };
    let size = stem.split(['-', '_', '.']).find_map(|part| {
      let (w, h) = part.split_once('x')?;
      Some((w.parse::<u32>().ok()?, h.parse::<u32>().ok()?))
    });
    match (size, kind) {
      (Some((w, h)), _) => found.push((w, format!("{w}x{h}"), name, kind)),
      (None, "image/svg+xml") => found.push((u32::MAX, "any".to_owned(), name, kind)),
      _ => {}
    }
  }
  found.sort();
  found.into_iter().map(|(_, sizes, name, kind)| serde_json::json!({ "src": format!("{}/{name}", root.route), "sizes": sizes, "type": kind })).collect()
}

/// The worker: the build's hashed modules and stylesheets precached as one set per build and answered from it first; the client, the vendor modules, the document's other stylesheets, the manifest and the offline route warmed at install; everything else from the network first, kept when the response is not `private` or `no-store` and answered from what was kept when the network fails. A payload a client navigation kept also keeps its page's document, so a reload offline finds the page. On taking control the worker keeps the document of every window it now controls, since those loads came before it.
fn worker(config: &Config, section: &PwaSection, import_map: Option<&str>, styles: &[String], client_minified: bool) -> String {
  let bundle = config.bundle.as_ref();
  let build: Vec<String> = bundle
    .map(|bundle| {
      bundle
        .hashed()
        .iter()
        .map(|(output, hashed)| (format!("{}/{output}", bundle.route), format!("{}/{hashed}", bundle.route)))
        .filter(|(plain, _)| bundle.minified_url(plain) == *plain)
        .map(|(_, hashed)| hashed)
        .collect()
    })
    .unwrap_or_default();
  let mut warm: Vec<String> = Vec::new();
  let mut add = |url: String, warm: &mut Vec<String>| {
    if url.starts_with('/') && !url.starts_with("//") && !build.contains(&url) && !warm.contains(&url) {
      warm.push(url);
    }
  };
  let map: BTreeMap<String, String> = import_map
    .and_then(|text| serde_json::from_str::<serde_json::Value>(text).ok())
    .and_then(|value| value.get("imports").cloned())
    .and_then(|imports| serde_json::from_value(imports).ok())
    .unwrap_or_default();
  let prefix = format!("{}/", client::ROUTE);
  let mut names: Vec<String> = Vec::new();
  for url in map.values() {
    match url.strip_prefix(&prefix) {
      Some(name) => names.push(name.to_owned()),
      None => add(url.clone(), &mut warm),
    }
  }
  let mut walked: Vec<String> = Vec::new();
  while let Some(name) = names.pop() {
    if walked.contains(&name) {
      continue;
    }
    for import in client::imports(&name, client_minified) {
      if let Some(sibling) = import.strip_prefix("./") {
        names.push(sibling.to_owned());
      }
    }
    walked.push(name);
  }
  walked.sort();
  for name in &walked {
    add(format!("{prefix}{name}"), &mut warm);
  }
  for href in styles {
    add(href.clone(), &mut warm);
  }
  add(MANIFEST_ROUTE.to_owned(), &mut warm);
  if let Some(offline) = &section.offline {
    add(offline.clone(), &mut warm);
  }
  let version = {
    use sha2::Digest;
    let mut hash = sha2::Sha256::new();
    for url in build.iter().chain(&warm) {
      hash.update(url.as_bytes());
      hash.update([0]);
    }
    for name in &walked {
      hash.update(client::get(name, client_minified).unwrap_or_default().as_bytes());
    }
    hash.finalize().iter().take(6).map(|b| format!("{b:02x}")).collect::<String>()
  };
  WORKER
    .replace("__VERSION__", &json(&version))
    .replace("__BUILD__", &json(&build))
    .replace("__WARM__", &json(&warm))
    .replace("__BUNDLE__", &json(&bundle.map(|b| format!("{}/", b.route))))
    .replace("__OFFLINE__", &json(&section.offline))
}

fn json<T: serde::Serialize>(value: &T) -> String {
  serde_json::to_string(value).unwrap_or_else(|_| "null".to_owned())
}

const WORKER: &str = r#"// Written by the FSR host from [pwa].
const VERSION = __VERSION__;
const BUILD = __BUILD__;
const WARM = __WARM__;
const BUNDLE = __BUNDLE__;
const OFFLINE = __OFFLINE__;
const BUILD_CACHE = "sf-build-" + VERSION;
const RUNTIME_CACHE = "sf-runtime";

function kept(response) {
  return response.ok && !/private|no-store/.test(response.headers.get("cache-control") || "");
}

function keepDocument(href) {
  return fetch(href, { credentials: "same-origin" })
    .then((document) => (kept(document) ? caches.open(RUNTIME_CACHE).then((cache) => cache.put(href, document)) : undefined))
    .catch(() => undefined);
}

self.addEventListener("install", (event) => {
  event.waitUntil(
    Promise.all([
      caches.open(BUILD_CACHE).then((cache) => cache.addAll(BUILD)),
      caches.open(RUNTIME_CACHE).then((cache) =>
        Promise.all(WARM.map((url) => fetch(url, { credentials: "omit" }).then((response) => (kept(response) ? cache.put(url, response) : undefined)).catch(() => undefined))),
      ),
    ]).then(() => self.skipWaiting()),
  );
});

self.addEventListener("activate", (event) => {
  event.waitUntil(
    caches
      .keys()
      .then((names) => {
        const older = names.filter((name) => name.startsWith("sf-build-") && name !== BUILD_CACHE);
        return Promise.all(older.slice(0, -1).map((name) => caches.delete(name)));
      })
      .then(() => self.clients.claim())
      .then(() => self.clients.matchAll({ type: "window" }))
      .then((windows) => Promise.all(windows.map((client) => keepDocument(client.url)))),
  );
});

self.addEventListener("fetch", (event) => {
  const request = event.request;
  const url = new URL(request.url);
  if (request.method !== "GET" || url.origin !== self.location.origin || url.pathname.startsWith("/_sf/") || url.pathname.startsWith("/__fsr/") || url.pathname === "/sw.js") {
    return;
  }
  if (BUNDLE !== null && url.pathname.startsWith(BUNDLE)) {
    event.respondWith(caches.match(request).then((hit) => hit || fetch(request)));
    return;
  }
  event.respondWith(
    fetch(request)
      .then((response) => {
        if (kept(response)) {
          const copy = response.clone();
          event.waitUntil(caches.open(RUNTIME_CACHE).then((cache) => cache.put(request, copy)));
          if (url.searchParams.has("__payload")) {
            const page = new URL(url);
            page.searchParams.delete("__payload");
            event.waitUntil(keepDocument(page.href));
          }
        }
        return response;
      })
      .catch(() =>
        caches
          .match(request, { ignoreVary: true })
          .then((hit) => hit || (request.mode === "navigate" && OFFLINE !== null ? caches.match(OFFLINE, { ignoreVary: true }) : undefined))
          .then((hit) => hit || Response.error()),
      ),
  );
});
"#;
