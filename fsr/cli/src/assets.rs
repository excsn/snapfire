//! The build's half of images and fonts. The lowerer asks the [`Resolver`]
//! what an imported image is; `derive` writes the variants and the hashed
//! font copies under the bundle's output directory; `fonts` reads the font
//! directory and the `[fonts]` section into the CSS a document inlines.

use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use snapfire_fsr_assets::{font, hash, image, variant_name, Face, Format, Source, VariantPolicy};
use snapfire_fsr_host::assets::{AssetsManifest, Face as FaceOut, Fonts, ImageEntry, ImagePolicy, Remote, Variant, VERSION};
use snapfire_fsr_host::config::{Config, DirsSection, FontsSection, ImagesSection};
use snapfire_fsr_lower::assets::{AssetResolver, ImageFacts, ImageRequest};

use crate::BuildError;

/// The asset sections of the configuration, defaults filled when there is
/// no configuration to read.
#[derive(Clone)]
pub struct Sections {
  pub dirs: DirsSection,
  pub images: ImagesSection,
  pub fonts: FontsSection,
}

impl Sections {
  pub fn of(app: &Path) -> Self {
    let root = crate::serve::project_root(app);
    match Config::load(&root) {
      Ok(config) => Self { dirs: config.dirs, images: config.images, fonts: config.fonts },
      Err(_) => Self { dirs: DirsSection::default(), images: ImagesSection::default(), fonts: FontsSection::default() },
    }
  }

  pub fn policy(&self) -> VariantPolicy {
    policy_of(&self.images)
  }
}

fn policy_of(images: &ImagesSection) -> VariantPolicy {
  let mut policy = VariantPolicy { widths: images.widths.clone(), formats: images.formats.iter().filter_map(|f| Format::parse(f)).collect(), quality: BTreeMap::new() };
  for format in Format::ALL {
    policy.quality.insert(format, images.quality.get(format.extension()).copied().unwrap_or_else(|| format.default_quality()));
  }
  policy
}

/// One image the markup named, with everything the build needs to write its
/// variants after the bundle runs.
#[derive(Debug, Clone)]
struct Seen {
  src: String,
  hash: String,
  width: u32,
  height: u32,
  passthrough: bool,
  /// Relative directory under the app, `src/img`, and the stem and extension.
  dir: String,
  stem: String,
  widths: BTreeSet<u32>,
  quality: BTreeMap<Format, u8>,
}

/// Answers the lowerer from the files, remembering every image asked for.
pub struct Resolver {
  app: PathBuf,
  public_path: String,
  sections: Sections,
  policy: VariantPolicy,
  seen: RefCell<BTreeMap<String, Seen>>,
  /// Files that were asked for and are not images this build decodes, with why.
  pub refused: RefCell<Vec<(String, String)>>,
}

impl Resolver {
  pub fn new(app: &Path, public_path: &str, sections: Sections) -> Self {
    let policy = sections.policy();
    Self { app: app.to_path_buf(), public_path: public_path.trim_end_matches('/').to_owned(), sections, policy, seen: RefCell::new(BTreeMap::new()), refused: RefCell::new(Vec::new()) }
  }

  fn url(&self, base: Option<&str>, relative: &str) -> String {
    format!("{}{}/{relative}", base.unwrap_or(""), self.public_path)
  }

  fn facts_of(&self, seen: &Seen, widths: &[u32]) -> ImageFacts {
    let base = self.sections.images.base.as_deref();
    let sources = if seen.passthrough {
      Vec::new()
    } else {
      self
        .policy
        .formats
        .iter()
        .map(|format| {
          let srcset = widths
            .iter()
            .map(|w| format!("{} {w}w", self.url(base, &format!("{}{}", dir_prefix(&seen.dir), variant_name(&seen.stem, &seen.hash, *w, *format)))))
            .collect::<Vec<_>>()
            .join(", ");
          (format.mime().to_owned(), srcset)
        })
        .collect()
    };
    ImageFacts { src: seen.src.clone(), width: seen.width, height: seen.height, sources }
  }

  /// Records every image under `[dirs] images`, so a template that names one
  /// by its path finds it in the manifest whether or not a component
  /// imported it.
  pub fn scan(&self, dir: &Path, under: &str) {
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    let mut files: Vec<PathBuf> = entries.filter_map(|e| e.ok().map(|e| e.path())).collect();
    files.sort();
    for file in files {
      let name = file.file_name().unwrap_or_default().to_string_lossy().into_owned();
      if file.is_dir() {
        self.scan(&file, &format!("{under}/{name}"));
        continue;
      }
      let ext = file.extension().and_then(|e| e.to_str()).map(|e| e.to_ascii_lowercase()).unwrap_or_default();
      if ["png", "jpg", "jpeg", "gif", "webp", "avif", "svg", "ico", "bmp"].contains(&ext.as_str()) {
        let _ = self.image(&format!("{under}/{name}"), &ImageRequest::default());
      }
    }
  }

  /// Everything the markup named, as the manifest records it.
  pub fn entries(&self) -> Vec<ImageEntry> {
    let base = self.sections.images.base.as_deref();
    self
      .seen
      .borrow()
      .iter()
      .map(|(source, seen)| {
        let widths: Vec<u32> = seen.widths.iter().copied().collect();
        let variants = if seen.passthrough {
          Vec::new()
        } else {
          widths
            .iter()
            .flat_map(|w| {
              self.policy.formats.iter().map(move |format| {
                let path = format!("{}{}", dir_prefix(&seen.dir), variant_name(&seen.stem, &seen.hash, *w, *format));
                Variant { width: *w, format: format.extension().to_owned(), url: self.url(base, &path), path }
              })
            })
            .collect()
        };
        ImageEntry {
          source: source.clone(),
          src: seen.src.clone(),
          hash: seen.hash.clone(),
          width: seen.width,
          height: seen.height,
          passthrough: seen.passthrough,
          widths,
          quality: seen.quality.iter().map(|(f, q)| (f.extension().to_owned(), *q)).collect(),
          variants,
        }
      })
      .collect()
  }

  pub fn policy_out(&self) -> ImagePolicy {
    ImagePolicy {
      widths: self.policy.widths.clone(),
      formats: self.policy.formats.iter().map(|f| f.extension().to_owned()).collect(),
      quality: self.policy.quality.iter().map(|(f, q)| (f.extension().to_owned(), *q)).collect(),
      base: self.sections.images.base.clone(),
      sources: self.sections.images.sources.iter().map(|(name, source)| (name.clone(), source.template.clone())).collect(),
    }
  }
}

fn dir_prefix(dir: &str) -> String {
  if dir.is_empty() { String::new() } else { format!("{dir}/") }
}

impl AssetResolver for Resolver {
  fn image(&self, path: &str, request: &ImageRequest) -> Option<ImageFacts> {
    let file = self.app.join(path);
    if !file.is_file() {
      return None;
    }
    let mut seen = self.seen.borrow_mut();
    if !seen.contains_key(path) {
      let bytes = std::fs::read(&file).ok()?;
      let digest = hash::of(&bytes);
      let (width, height) = match image::dimensions(&file) {
        Ok(size) => size,
        Err(e) => {
          self.refused.borrow_mut().push((path.to_owned(), e.to_string()));
          return None;
        }
      };
      let passthrough = image::passthrough(&file).unwrap_or(true);
      let relative = Path::new(path);
      let dir = relative.parent().map(|d| d.to_string_lossy().replace('\\', "/")).unwrap_or_default();
      let stem = relative.file_stem().unwrap_or_default().to_string_lossy().into_owned();
      let ext = relative.extension().unwrap_or_default().to_string_lossy().to_ascii_lowercase();
      let src = self.url(self.sections.images.base.as_deref(), &format!("{}{}", dir_prefix(&dir), hash::emitted_name(&stem, &digest, &ext)));
      seen.insert(path.to_owned(), Seen { src, hash: digest, width, height, passthrough, dir, stem, widths: BTreeSet::new(), quality: BTreeMap::new() });
    }
    let entry = seen.get_mut(path).expect("just inserted");
    let policy = match &request.widths {
      Some(widths) => self.policy.clone().with_widths(widths.clone()),
      None => self.policy.clone(),
    };
    let widths = policy.widths_for(entry.width);
    entry.widths.extend(widths.iter().copied());
    if let Some(quality) = &request.quality {
      for (format, q) in quality {
        if let Some(format) = Format::parse(format) {
          entry.quality.insert(format, *q);
        }
      }
    }
    let facts = self.facts_of(entry, &widths);
    Some(facts)
  }

  fn font(&self, path: &str) -> Option<String> {
    let file = self.app.join(path);
    let bytes = std::fs::read(&file).ok()?;
    let digest = hash::of(&bytes);
    let relative = Path::new(path);
    let dir = relative.parent().map(|d| d.to_string_lossy().replace('\\', "/")).unwrap_or_default();
    let stem = relative.file_stem().unwrap_or_default().to_string_lossy().into_owned();
    let ext = relative.extension().unwrap_or_default().to_string_lossy().to_ascii_lowercase();
    Some(self.url(self.sections.fonts.base.as_deref(), &format!("{}{}", dir_prefix(&dir), hash::emitted_name(&stem, &digest, &ext))))
  }

  fn source(&self, name: &str) -> Option<String> {
    self.sections.images.sources.get(name).map(|s| s.template.clone())
  }

  fn widths(&self) -> Vec<u32> {
    self.policy.widths.clone()
  }
}

/// The manifest the build writes, from what the resolver saw and what the
/// font directory holds.
pub fn manifest(resolver: &Resolver, fonts: Fonts) -> AssetsManifest {
  AssetsManifest { version: VERSION, images: resolver.policy_out(), entries: resolver.entries(), fonts }
}

/// What `derive` wrote and what it found already there.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Derived {
  pub written: Vec<PathBuf>,
  pub kept: usize,
}

/// Writes every variant and hashed font copy the manifest names that is not
/// under `out` already. A name carries the content's hash, so a file that is
/// there is the same bytes and is kept.
pub fn derive(app: &Path, out: &Path, manifest: &AssetsManifest) -> Result<Derived, BuildError> {
  let mut derived = Derived::default();
  let mut policy = VariantPolicy::default();
  policy.formats = manifest.images.formats.iter().filter_map(|f| Format::parse(f)).collect();
  for (format, q) in &manifest.images.quality {
    if let Some(format) = Format::parse(format) {
      policy.quality.insert(format, *q);
    }
  }
  for entry in &manifest.entries {
    let missing: Vec<&Variant> = entry.variants.iter().filter(|v| !out.join(&v.path).is_file()).collect();
    derived.kept += entry.variants.len() - missing.len();
    if missing.is_empty() {
      continue;
    }
    let source = Source::open(&app.join(&entry.source)).map_err(|e| BuildError::Assets(e.to_string()))?;
    for variant in missing {
      let Some(format) = Format::parse(&variant.format) else { continue };
      let quality = entry.quality.get(&variant.format).copied().unwrap_or_else(|| policy.quality(format));
      let bytes = source.variant(variant.width, format, quality).map_err(|e| BuildError::Assets(e.to_string()))?;
      let path = out.join(&variant.path);
      if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| BuildError::Io(parent.to_path_buf(), e))?;
      }
      std::fs::write(&path, bytes).map_err(|e| BuildError::Io(path.clone(), e))?;
      derived.written.push(path);
    }
  }
  for face in &manifest.fonts.faces {
    let path = out.join(&face.path);
    if path.is_file() {
      derived.kept += 1;
      continue;
    }
    if let Some(parent) = path.parent() {
      std::fs::create_dir_all(parent).map_err(|e| BuildError::Io(parent.to_path_buf(), e))?;
    }
    std::fs::copy(app.join(&face.source), &path).map_err(|e| BuildError::Io(path.clone(), e))?;
    derived.written.push(path);
  }
  Ok(derived)
}

/// One face the directory holds, read for the CSS.
struct Held {
  file: String,
  face: Face,
  hash: String,
  unicode_range: Option<String>,
}

/// Reads `<app>/<dirs.fonts>` and the `[fonts]` section into the faces, the
/// CSS and the preloads a document carries. A face no key names gets the key
/// its family slugs to.
pub fn fonts(app: &Path, public_path: &str, sections: &Sections) -> Result<(Fonts, Vec<String>), BuildError> {
  let dir = app.join(&sections.dirs.fonts);
  let mut held: Vec<Held> = Vec::new();
  if dir.is_dir() {
    let mut files: Vec<PathBuf> = std::fs::read_dir(&dir).map_err(|e| BuildError::Io(dir.clone(), e))?.filter_map(|e| e.ok().map(|e| e.path())).collect();
    files.sort();
    for file in files {
      let ext = file.extension().and_then(|e| e.to_str()).map(|e| e.to_ascii_lowercase()).unwrap_or_default();
      if !["woff2", "woff", "ttf", "otf"].contains(&ext.as_str()) {
        continue;
      }
      let face = Face::read(&file).map_err(|e| BuildError::Assets(e.to_string()))?;
      let bytes = std::fs::read(&file).map_err(|e| BuildError::Io(file.clone(), e))?;
      let range = std::fs::read_to_string(file.with_extension(format!("{ext}.range"))).ok().map(|r| r.trim().to_owned()).filter(|r| !r.is_empty());
      let name = file.file_name().unwrap_or_default().to_string_lossy().into_owned();
      held.push(Held { file: format!("{}/{name}", sections.dirs.fonts), face, hash: hash::of(&bytes), unicode_range: range });
    }
  }

  let public_path = public_path.trim_end_matches('/');
  let base = sections.fonts.base.as_deref().unwrap_or("");
  let mut fonts = Fonts { base: sections.fonts.base.clone(), ..Fonts::default() };
  let mut css = String::new();
  let mut lines: Vec<String> = Vec::new();

  // Which key each held face belongs to: a key's `files`, then a key's
  // family, then the family's own slug.
  let mut keyed: Vec<(String, usize)> = Vec::new();
  let mut taken = vec![false; held.len()];
  for (key, entry) in &sections.fonts.faces {
    if entry.remote.is_some() {
      continue;
    }
    if !entry.files.is_empty() {
      for file in &entry.files {
        let want = format!("{}/{file}", sections.dirs.fonts);
        match held.iter().position(|h| h.file == want) {
          Some(i) => {
            taken[i] = true;
            keyed.push((key.clone(), i));
          }
          None => return Err(BuildError::Assets(format!("fonts.{key}.files names `{file}`, which is not under {}/", sections.dirs.fonts))),
        }
      }
      continue;
    }
    let Some(family) = &entry.family else {
      return Err(BuildError::Assets(format!("fonts.{key} names neither `family` nor `files`, so nothing says which faces it covers")));
    };
    let mut any = false;
    for (i, h) in held.iter().enumerate() {
      if !taken[i] && h.face.family.eq_ignore_ascii_case(family) {
        taken[i] = true;
        keyed.push((key.clone(), i));
        any = true;
      }
    }
    if !any {
      return Err(BuildError::Assets(format!("fonts.{key}: no file under {}/ is the family `{family}`", sections.dirs.fonts)));
    }
  }
  for (i, h) in held.iter().enumerate() {
    if !taken[i] {
      keyed.push((slug(&h.face.family), i));
    }
  }

  let mut keys: Vec<String> = keyed.iter().map(|(k, _)| k.clone()).collect();
  keys.sort();
  keys.dedup();
  for key in &keys {
    let entry = sections.fonts.faces.get(key).cloned().unwrap_or_default();
    let indices: Vec<usize> = keyed.iter().filter(|(k, _)| k == key).map(|(_, i)| *i).collect();
    let family = entry.family.clone().unwrap_or_else(|| held[indices[0]].face.family.clone());
    let display = entry.display.clone().unwrap_or_else(|| "swap".to_owned());
    let fallback_name = entry.fallback.clone().unwrap_or_else(|| "Arial".to_owned());
    let Some(fallback) = font::fallback(&fallback_name) else {
      let known: Vec<&str> = font::fallbacks().collect();
      return Err(BuildError::Assets(format!("fonts.{key}.fallback `{fallback_name}` is not a face this build knows; one of {}", known.join(", "))));
    };
    let regular = indices.iter().copied().min_by_key(|i| (held[*i].face.weight.abs_diff(400), held[*i].face.style != snapfire_fsr_assets::Style::Normal)).expect("a key has a face");
    for i in &indices {
      let h = &held[*i];
      let path = Path::new(&h.file);
      let stem = path.file_stem().unwrap_or_default().to_string_lossy().into_owned();
      let ext = path.extension().unwrap_or_default().to_string_lossy().to_ascii_lowercase();
      let out_path = format!("{}/{}", sections.dirs.fonts, hash::emitted_name(&stem, &h.hash, &ext));
      let url = format!("{base}{public_path}/{out_path}");
      let preload = match entry.preload {
        Some(on) => on,
        None => *i == regular,
      };
      let format = match ext.as_str() {
        "woff2" => "woff2",
        "woff" => "woff",
        "ttf" => "truetype",
        _ => "opentype",
      };
      css.push_str(&format!(
        "@font-face{{font-family:\"{family}\";font-style:{};font-weight:{};font-display:{display};src:url({url}) format(\"{format}\");",
        h.face.style.as_css(),
        h.face.weight
      ));
      if let Some(range) = &h.unicode_range {
        css.push_str(&format!("unicode-range:{range};"));
      }
      css.push('}');
      if preload {
        fonts.preload.push(url.clone());
      }
      fonts.faces.push(FaceOut { key: key.clone(), family: family.clone(), weight: h.face.weight, style: h.face.style.as_css().to_owned(), source: h.file.clone(), url, path: out_path, unicode_range: h.unicode_range.clone(), preload });
      lines.push(format!("{family} {} {} from {}", h.face.weight, h.face.style.as_css(), h.file));
    }
    let fallback_family = format!("{family} Fallback");
    css.push_str(&held[regular].face.fallback_face(&fallback_family, fallback));
    let variable = entry.variable.clone().unwrap_or_else(|| format!("--font-{key}"));
    let value = format!("\"{family}\", \"{fallback_family}\", {}", generic_of(&fallback_name));
    fonts.variables.insert(variable, value);
  }

  for (key, entry) in &sections.fonts.faces {
    let Some(href) = &entry.remote else { continue };
    let Some(family) = &entry.family else {
      return Err(BuildError::Assets(format!("fonts.{key}.remote needs `family`, since there is no file to read the name from")));
    };
    let mut preconnect = Vec::new();
    if let Some(origin) = origin_of(href) {
      preconnect.push(origin.clone());
      if origin == "https://fonts.googleapis.com" {
        preconnect.push("https://fonts.gstatic.com".to_owned());
      }
    }
    let variable = entry.variable.clone().unwrap_or_else(|| format!("--font-{key}"));
    let fallback_name = entry.fallback.clone().unwrap_or_else(|| "Arial".to_owned());
    fonts.variables.insert(variable, format!("\"{family}\", {}", generic_of(&fallback_name)));
    fonts.remote.push(Remote { key: key.clone(), family: family.clone(), href: href.clone(), preconnect });
    lines.push(format!("{family} from {href}"));
  }

  if !fonts.variables.is_empty() {
    css.push_str(":root{");
    for (name, value) in &fonts.variables {
      css.push_str(&format!("{name}:{value};"));
    }
    css.push('}');
  }
  fonts.css = css;
  Ok((fonts, lines))
}

fn slug(family: &str) -> String {
  let mut out = String::new();
  for c in family.chars() {
    if c.is_ascii_alphanumeric() {
      out.push(c.to_ascii_lowercase());
    } else if !out.ends_with('-') && !out.is_empty() {
      out.push('-');
    }
  }
  out.trim_end_matches('-').to_owned()
}

fn generic_of(fallback: &str) -> &'static str {
  match fallback.to_ascii_lowercase().as_str() {
    "times new roman" | "georgia" => "serif",
    "courier new" => "monospace",
    _ => "sans-serif",
  }
}

fn origin_of(url: &str) -> Option<String> {
  let (scheme, rest) = url.split_once("://")?;
  let host = rest.split('/').next()?;
  Some(format!("{scheme}://{host}"))
}

/// `fsr fonts add google:Inter@400,700`: downloads the faces the provider
/// serves into the font directory, one file per face and subset, each with
/// a `.range` sidecar carrying its `unicode-range`. The build then reads
/// them as local files.
pub fn add(app: &Path, spec: &str) -> Result<Vec<PathBuf>, BuildError> {
  add_from(app, spec, "https://fonts.googleapis.com")
}

/// `add` against a provider at `provider_base`, which a test stands in for.
pub fn add_from(app: &Path, spec: &str, provider_base: &str) -> Result<Vec<PathBuf>, BuildError> {
  let sections = Sections::of(app);
  let (provider, rest) = spec.split_once(':').ok_or_else(|| BuildError::Assets(format!("`{spec}` is not `google:<Family>@<weights>`")))?;
  if provider != "google" {
    return Err(BuildError::Assets(format!("`{provider}` is not a provider this command fetches from; `google` is")));
  }
  let (family, weights) = rest.split_once('@').unwrap_or((rest, "400"));
  let mut normal: Vec<u16> = Vec::new();
  let mut italic: Vec<u16> = Vec::new();
  for weight in weights.split(',') {
    let weight = weight.trim();
    let (weight, is_italic) = match weight.strip_suffix('i') {
      Some(w) => (w, true),
      None => (weight, false),
    };
    let n: u16 = weight.parse().map_err(|_| BuildError::Assets(format!("`{weight}` is not a weight; write `400` or `700i`")))?;
    if is_italic { italic.push(n) } else { normal.push(n) }
  }
  let axes: String = if italic.is_empty() {
    format!("wght@{}", normal.iter().map(|w| w.to_string()).collect::<Vec<_>>().join(";"))
  } else {
    let mut tuples: Vec<String> = normal.iter().map(|w| format!("0,{w}")).collect();
    tuples.extend(italic.iter().map(|w| format!("1,{w}")));
    tuples.sort();
    format!("ital,wght@{}", tuples.join(";"))
  };
  let url = format!("{}/css2?family={}:{axes}&display=swap", provider_base.trim_end_matches('/'), family.replace(' ', "+"));
  let client = reqwest::blocking::Client::builder()
    .user_agent("Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/124.0.0.0 Safari/537.36")
    .build()
    .map_err(|e| BuildError::Http(url.clone(), e.to_string()))?;
  let css = client.get(&url).send().and_then(|r| r.error_for_status()).and_then(|r| r.text()).map_err(|e| BuildError::Http(url.clone(), e.to_string()))?;
  let dir = app.join(&sections.dirs.fonts);
  std::fs::create_dir_all(&dir).map_err(|e| BuildError::Io(dir.clone(), e))?;
  let mut written = Vec::new();
  // The provider writes `/* latin */` before each `@font-face`, so a block's
  // subset is the comment trailing the chunk before it.
  let chunks: Vec<&str> = css.split("@font-face").collect();
  for (i, block) in chunks.iter().enumerate().skip(1) {
    let field = |name: &str| -> Option<String> {
      let at = block.find(name)? + name.len();
      let rest = block[at..].trim_start_matches([':', ' ']);
      Some(rest.split(';').next()?.trim().trim_matches('\'').trim_matches('"').to_owned())
    };
    let subset = chunks[i - 1]
      .trim_end()
      .strip_suffix("*/")
      .and_then(|c| c.rsplit("/*").next())
      .map(|s| s.trim().to_owned())
      .filter(|s| !s.is_empty() && !s.contains('{'))
      .unwrap_or_else(|| "all".to_owned());
    let (Some(weight), Some(style), Some(src)) = (field("font-weight"), field("font-style"), field("src")) else { continue };
    let Some(file_url) = src.split("url(").nth(1).and_then(|u| u.split(')').next()) else { continue };
    let ext = file_url.rsplit('.').next().unwrap_or("woff2").to_owned();
    let name = format!("{}-{weight}{}-{subset}.{ext}", family.replace(' ', ""), if style == "italic" { "-italic" } else { "" });
    let path = dir.join(&name);
    let bytes = client.get(file_url).send().and_then(|r| r.error_for_status()).and_then(|r| r.bytes()).map_err(|e| BuildError::Http(file_url.to_owned(), e.to_string()))?;
    std::fs::write(&path, &bytes).map_err(|e| BuildError::Io(path.clone(), e))?;
    if let Some(range) = field("unicode-range") {
      let sidecar = dir.join(format!("{name}.range"));
      std::fs::write(&sidecar, format!("{range}\n")).map_err(|e| BuildError::Io(sidecar, e))?;
    }
    written.push(path);
  }
  if written.is_empty() {
    return Err(BuildError::Assets(format!("{url} named no face; is `{family}` a family the provider serves?")));
  }
  Ok(written)
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn a_family_slugs_to_a_key() {
    assert_eq!(slug("Inter"), "inter");
    assert_eq!(slug("JetBrains Mono"), "jetbrains-mono");
    assert_eq!(slug("Source Sans 3"), "source-sans-3");
  }

  #[test]
  fn the_origin_of_a_stylesheet_is_its_scheme_and_host() {
    assert_eq!(origin_of("https://fonts.googleapis.com/css2?family=Inter").as_deref(), Some("https://fonts.googleapis.com"));
    assert_eq!(origin_of("not a url"), None);
  }
}
