//! `fsr ir view`: one lowered body, printed as the plan file holds it.

use std::path::{Path, PathBuf};

use snapfire_fsr_plan::sexpr::ViewError;
use snapfire_fsr_plan::Manifest;

use crate::PLAN_FILE;

#[derive(Debug, thiserror::Error)]
pub enum IrError {
  #[error("{0} is missing; run `fsr build` first")]
  NoPlan(PathBuf),
  #[error("{0}: {1}")]
  Plan(PathBuf, String),
  #[error("{why}; the rows are:\n{}", rows.join("\n"))]
  Unknown { why: ViewError, rows: Vec<String> },
  #[error("{0}")]
  View(ViewError),
}

/// The row named `id` in `app`'s plan, or one section of it, as `plan.sexp`
/// writes it. Reads the plan the last build wrote and builds nothing.
pub fn view(app: &Path, id: &str, section: Option<&str>) -> Result<String, IrError> {
  let path = app.join(PLAN_FILE);
  let text = std::fs::read_to_string(&path).map_err(|_| IrError::NoPlan(path.clone()))?;
  let manifest = Manifest::from_text(&text).map_err(|e| IrError::Plan(path, e.to_string()))?;
  manifest.view(id, section).map_err(|e| match e {
    ViewError::Unknown { .. } => IrError::Unknown { why: e, rows: manifest.row_ids() },
    other => IrError::View(other),
  })
}
