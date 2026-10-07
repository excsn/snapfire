use std::path::Path;

use snapfire_fsr_tera::TeraEvaluator;

use crate::HostError;

/// Every `.tera` file under `app`, each named by its path relative to `app`
/// with `/` separators, added to one `Tera` with the markers registered.
/// `None` when the app holds no template. A template that fails to parse
/// names itself.
pub fn evaluator(app: &Path, assets: Option<&crate::assets::AssetsManifest>) -> Result<Option<(TeraEvaluator, Vec<String>)>, HostError> {
  let files = crate::template_files(app)?;
  if files.is_empty() {
    return Ok(None);
  }
  let mut tera = tera::Tera::new();
  snapfire_fsr_tera::register_markers(&mut tera);
  register_assets(&mut tera, assets.cloned().unwrap_or_default());
  let mut names = Vec::with_capacity(files.len());
  let mut templates = Vec::with_capacity(files.len());
  for file in &files {
    let name = file.strip_prefix(app).unwrap_or(file).to_string_lossy().replace('\\', "/");
    let text = std::fs::read_to_string(file).map_err(|e| HostError::Io(file.clone(), e))?;
    names.push(name.clone());
    templates.push((name, text));
  }
  tera.add_raw_templates(templates).map_err(|e| HostError::Template(app.to_path_buf(), e.to_string()))?;
  Ok(Some((TeraEvaluator::new(tera), names)))
}

/// `fsr_picture(src="src/img/hero.png", alt="…", sizes="…", priority=true)`
/// writes what the `Picture` tag writes for the same image, and
/// `fsr_fonts()` the inline style and the links a document's head carries
/// for the fonts, for a template that builds its own head. Both resolve
/// against the manifest the build wrote.
fn register_assets(tera: &mut tera::Tera, manifest: crate::assets::AssetsManifest) {
  use tera::{Kwargs, State};
  let entries = std::sync::Arc::new(manifest.clone());
  tera.register_function("fsr_picture", move |kwargs: Kwargs, _: &State| -> tera::TeraResult<tera::Value> {
    let src: String = kwargs.must_get("src")?;
    let entry = entries.image(&src).ok_or_else(|| tera::Error::message(format!("`{src}` is not an image the build saw; import it from a component or name a file under the app the build read")))?;
    let alt: Option<String> = kwargs.get("alt")?;
    let sizes: Option<String> = kwargs.get("sizes")?;
    let priority: Option<bool> = kwargs.get("priority")?;
    let class: Option<String> = kwargs.get("class")?;
    Ok(tera::Value::safe_string(&picture_html(entry, &entries.images, alt.as_deref(), sizes.as_deref(), priority.unwrap_or(false), class.as_deref())))
  });
  let fonts = std::sync::Arc::new(manifest);
  tera.register_function("fsr_fonts", move |_: Kwargs, _: &State| -> tera::TeraResult<tera::Value> {
    let mut out = String::new();
    for row in crate::asset_head_rows(&fonts) {
      if row.tag == "meta" {
        continue;
      }
      row.render(&mut out);
    }
    Ok(tera::Value::safe_string(&out))
  });
}

fn picture_html(entry: &crate::assets::ImageEntry, policy: &crate::assets::ImagePolicy, alt: Option<&str>, sizes: Option<&str>, priority: bool, class: Option<&str>) -> String {
  let escape = crate::shell::escape;
  let mut img = format!("<img src=\"{}\" width=\"{}\" height=\"{}\"", escape(&entry.src), entry.width, entry.height);
  if let Some(alt) = alt {
    img.push_str(&format!(" alt=\"{}\"", escape(alt)));
  }
  if let Some(class) = class {
    img.push_str(&format!(" class=\"{}\"", escape(class)));
  }
  img.push_str(if priority { " loading=\"eager\" decoding=\"async\" fetchpriority=\"high\">" } else { " loading=\"lazy\" decoding=\"async\">" });
  if entry.passthrough || entry.variants.is_empty() {
    return img;
  }
  let sizes = sizes.map(str::to_owned).unwrap_or_else(|| format!("(max-width: {}px) 100vw, {}px", entry.width, entry.width));
  let mut out = String::from("<picture>");
  for format in &policy.formats {
    let srcset: Vec<String> = entry.variants.iter().filter(|v| &v.format == format).map(|v| format!("{} {}w", v.url, v.width)).collect();
    if srcset.is_empty() {
      continue;
    }
    let mime = match format.as_str() {
      "avif" => "image/avif",
      "webp" => "image/webp",
      other => other,
    };
    out.push_str(&format!("<source type=\"{mime}\" srcset=\"{}\" sizes=\"{}\">", escape(&srcset.join(", ")), escape(&sizes)));
  }
  out.push_str(&img);
  out.push_str("</picture>");
  out
}
