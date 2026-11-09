use super::*;

impl Bundle {
  /// Each module and stylesheet output against its name carrying the first eight hex digits of its
  /// content's SHA-256, `src/main.3f2a91c0.js` for `src/main.js`. Read from `dir` on the first
  /// call; an output that cannot be read is left out.
  pub fn hashed(&self) -> &BTreeMap<String, String> {
    self.hashed.get_or_init(|| {
      use sha2::Digest;
      self
        .outputs
        .iter()
        .filter_map(|output| {
          let (stem, ext) = output.rsplit_once('.').filter(|(_, ext)| *ext == "css" || is_module_extension(ext))?;
          let bytes = std::fs::read(self.dir.join(output)).ok()?;
          let hash: String = sha2::Sha256::digest(&bytes).iter().take(4).map(|b| format!("{b:02x}")).collect();
          Some((output.clone(), format!("{stem}.{hash}.{ext}")))
        })
        .collect()
    })
  }

  /// `url` as a host outside development names it: its minified twin when there is one, then its
  /// hashed name.
  pub fn served_url(&self, url: &str) -> String {
    let twin = self.minified_url(url);
    let Some(relative) = twin.strip_prefix(&format!("{}/", self.route)) else { return twin };
    let (path, rest) = match relative.find(['?', '#']) {
      Some(at) => relative.split_at(at),
      None => (relative, ""),
    };
    match self.hashed().get(path) {
      Some(hashed) => format!("{}/{hashed}{rest}", self.route),
      None => twin,
    }
  }

  /// Every module the host serves by its hashed name, its own URL against that name's, for the
  /// import map: a module importing a sibling by relative path resolves to the plain URL and the
  /// map turns it into the hashed one.
  pub fn hashed_modules(&self) -> Vec<(String, String)> {
    self
      .hashed()
      .iter()
      .filter(|(output, _)| output.rsplit_once('.').is_some_and(|(_, ext)| is_module_extension(ext)))
      .map(|(output, hashed)| (format!("{}/{output}", self.route), format!("{}/{hashed}", self.route)))
      .filter(|(plain, _)| self.minified_url(plain) == *plain)
      .collect()
  }

  /// `url`'s minified twin, `/static/js/app/src/main.min.js` for `/static/js/app/src/main.js`, when
  /// `url` is under this bundle and the compiler wrote the twin; `url` as given otherwise. A
  /// minified module imports its siblings by their twins' names, so a page that loads one module of
  /// the bundle by its plain name holds that module twice.
  pub fn minified_url(&self, url: &str) -> String {
    let Some(suffix) = &self.minified else { return url.to_owned() };
    let Some(relative) = url.strip_prefix(&format!("{}/", self.route)) else { return url.to_owned() };
    let (path, rest) = match relative.find(['?', '#']) {
      Some(at) => relative.split_at(at),
      None => (relative, ""),
    };
    let (dir, file) = match path.rsplit_once('/') {
      Some((dir, file)) => (Some(dir), file),
      None => (None, path),
    };
    let Some((stem, ext)) = file.rsplit_once('.') else { return url.to_owned() };
    if stem.ends_with(suffix.as_str()) {
      return url.to_owned();
    }
    let twin = match dir {
      Some(dir) => format!("{dir}/{stem}{suffix}.{ext}"),
      None => format!("{stem}{suffix}.{ext}"),
    };
    match self.outputs.contains(&twin) {
      true => format!("{}/{twin}{rest}", self.route),
      false => url.to_owned(),
    }
  }
}

/// The compiler's facts file, from `dist/` or from whichever static root a
/// configuration points at its bundle: an application that writes its own
/// `[[static]]` for the browser tree, which is what a project rendering
/// through Tera does, keeps the bundle outside `dist/` and its component
/// stylesheets would otherwise never reach the head.
pub(super) fn build_facts(app: &Path, statics: &[StaticRoot]) -> Option<(String, BuildFacts)> {
  let dirs = std::iter::once("dist".to_owned()).chain(statics.iter().map(|root| root.dir.clone()));
  for dir in dirs {
    let path = app.join(&dir).join(".snapfire-build.json");
    let Ok(text) = std::fs::read_to_string(&path) else { continue };
    if let Ok(facts) = serde_json::from_str::<BuildFacts>(&text) {
      return Some((dir, facts));
    }
  }
  None
}

pub(super) fn to_json(value: &c5store::value::C5DataValue) -> serde_json::Value {
  use c5store::value::C5DataValue as V;
  match value {
    V::Null => serde_json::Value::Null,
    V::Boolean(b) => serde_json::Value::Bool(*b),
    V::Integer(i) => serde_json::Value::from(*i),
    V::UInteger(u) => serde_json::Value::from(*u),
    V::Float(f) => serde_json::Value::from(*f),
    V::String(s) => serde_json::Value::String(s.clone()),
    V::Bytes(b) => serde_json::Value::String(String::from_utf8_lossy(b).into_owned()),
    V::Array(items) => serde_json::Value::Array(items.iter().map(to_json).collect()),
    V::Map(map) => serde_json::Value::Object(map.iter().map(|(k, v)| (k.clone(), to_json(v))).collect()),
  }
}
