use super::*;

/// Removes the bundle overlay, so a file no longer rewritten does not shadow its source.
pub(crate) fn clear_overlay(app: &Path) -> Result<(), BuildError> {
  let overlay = app.join(dev::BUNDLE_OVERLAY);
  if overlay.is_dir() {
    std::fs::remove_dir_all(&overlay).map_err(|e| BuildError::Io(overlay, e))?;
  }
  Ok(())
}

/// The shell contract of this build: every store key a loader's `store`
/// export seeds, typed the way the browser reads it, plus the import map and
/// the frameworks this build vendors.
/// The keys a `store` body returns and their types, inferred from the data its
/// loader returns; `None` when the return is not a record the build can read.
pub(crate) fn seeded(contract: &Contract, session: Option<&str>, consts: &Consts, config: &[(String, infer::Ts)], loader: &snapfire_fsr_ir::Body, store: &snapfire_fsr_ir::Body) -> Option<Vec<(String, infer::Ts)>> {
  let data = infer::Inferer { contract, session, input: None, input_type: None, consts, config, natives: &[] }.returns(loader);
  match (infer::Inferer { contract, session, input: None, input_type: Some(data), consts, config, natives: &[] }.returns(store)) {
    infer::Ts::Record(fields) => Some(fields.into_iter().collect()),
    _ => None,
  }
}

pub(crate) fn shell_contract(app: &Path, contract: &Contract, session: Option<&str>, sources: &[SourceEntry], consts: &Consts, config: &[(String, infer::Ts)], frameworks: &std::collections::BTreeMap<String, String>) -> Result<ShellContract, BuildError> {
  let mut store = std::collections::BTreeMap::new();
  for source in sources {
    let Some(loader) = &source.body else { continue };
    for body in [&source.store, &source.defaults].into_iter().flatten() {
      for (key, ty) in seeded(contract, session, consts, config, loader, body).unwrap_or_default() {
        store.insert(key, ty.print(Flavour::Client));
      }
    }
  }
  let imports = std::fs::read_to_string(app.join("importmap.json"))
    .ok()
    .and_then(|text| serde_json::from_str::<serde_json::Value>(&text).ok())
    .and_then(|json| json.get("imports").cloned())
    .and_then(|imports| serde_json::from_value(imports).ok())
    .unwrap_or_default();
  Ok(ShellContract { version: SHELL_CONTRACT_VERSION, store, imports, frameworks: frameworks.clone(), fsr: env!("CARGO_PKG_VERSION").to_owned(), themes: Vec::new(), tokens: Vec::new() })
}
