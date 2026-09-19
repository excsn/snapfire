use std::path::{Path, PathBuf};

use snapfire_fsr_tera::TeraEvaluator;

use crate::HostError;

/// Directories under the app no template is read from.
const SKIPPED: &[&str] = &["vendor", "dist", "generated", "node_modules", "tests", "types"];

/// Every `.tera` file under `app`, each named by its path relative to `app`
/// with `/` separators, added to one `Tera` with the markers registered.
/// `None` when the app holds no template. A template that fails to parse
/// names itself.
pub fn evaluator(app: &Path) -> Result<Option<(TeraEvaluator, Vec<String>)>, HostError> {
  let mut files = Vec::new();
  collect(app, app, &mut files)?;
  if files.is_empty() {
    return Ok(None);
  }
  files.sort();
  let mut tera = tera::Tera::new();
  snapfire_fsr_tera::register_markers(&mut tera);
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

fn collect(app: &Path, dir: &Path, out: &mut Vec<PathBuf>) -> Result<(), HostError> {
  let mut entries: Vec<PathBuf> = std::fs::read_dir(dir).map_err(|e| HostError::Io(dir.to_path_buf(), e))?.filter_map(|e| e.ok().map(|e| e.path())).collect();
  entries.sort();
  for path in entries {
    let name = path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
    if path.is_dir() {
      if dir == app && SKIPPED.contains(&name.as_str()) || name.starts_with('.') {
        continue;
      }
      collect(app, &path, out)?;
    } else if name.ends_with(".tera") {
      out.push(path);
    }
  }
  Ok(())
}
