use super::*;

/// FSR's components as Svelte components, which the build writes under `generated/components` for an app whose `.svelte` files import them.
pub(crate) const SVELTE_COMPONENT_FILES: &[(&str, &str)] = &[
  ("Link", include_str!("../embedded/components/svelte/Link.svelte")),
  ("Picture", include_str!("../embedded/components/svelte/Picture.svelte")),
  ("Mount", include_str!("../embedded/components/svelte/Mount.svelte")),
  ("Embed", include_str!("../embedded/components/svelte/Embed.svelte")),
  ("Place", include_str!("../embedded/components/svelte/Place.svelte")),
];

/// The Svelte components and the module that exports them, by path under the application.
/// The Svelte components the Svelte adapter renders an island's children with, written beside FSR's own and exported by none.
pub(crate) const SVELTE_TREE_FILES: &[(&str, &str)] = &[("Pieces", include_str!("../embedded/components/svelte/Pieces.svelte")), ("Island", include_str!("../embedded/components/svelte/Island.svelte"))];

pub(crate) fn svelte_components_files() -> Vec<(String, String)> {
  let mut files: Vec<(String, String)> = SVELTE_COMPONENT_FILES.iter().chain(SVELTE_TREE_FILES).map(|(name, source)| (format!("generated/components/{name}.svelte"), (*source).to_owned())).collect();
  files.push(("generated/components/index.ts".to_owned(), SVELTE_COMPONENT_FILES.iter().map(|(name, _)| format!("export {{ default as {name} }} from \"./{name}.svelte\";\n")).collect()));
  files
}

/// Every source file that imports FSR's components, the overlay's own among them, with the specifier
/// pointed at the importing file's framework, by path in the bundle overlay:
/// a `.vue` file at Vue's adapter, a `.svelte` file at the Svelte components
/// the build writes under `generated/components` and any other file at the
/// adapter of the first island that reaches it, else React's when the app
/// serves React and the JSX runtime when it does not.
pub(crate) fn components_rewrites(app: &Path, set: &mut ComponentSet, islands: &[String], owners: &HashMap<String, Owner>, react: bool, rewritten: &[(String, String)]) -> Result<Vec<(String, String)>, BuildError> {
  let specifier = format!("\"{}\"", snapfire_fsr_lower::COMPONENTS_MODULE);
  let mut reached: HashMap<String, &'static str> = HashMap::new();
  for module in islands {
    let Some(file) = module.split_once('#').map(|(file, _)| file) else { continue };
    if snapfire_fsr_lower::component::is_foreign(file) {
      continue;
    }
    let adapter = adapter_for(module, owners)?;
    for file in set.local_reach(file) {
      reached.entry(file).or_insert(adapter.module);
    }
  }
  let mut files = Vec::new();
  for dir in types::source_dirs(app) {
    source_files(app, &app.join(dir), &mut files)?;
  }
  for (file, _) in rewritten {
    if !files.contains(file) {
      files.push(file.clone());
    }
  }
  let mut out = Vec::new();
  for file in files {
    let source = match rewritten.iter().find(|(f, _)| *f == file) {
      Some((_, text)) => text.clone(),
      None => {
        let path = app.join(&file);
        std::fs::read_to_string(&path).map_err(|e| BuildError::Io(path, e))?
      }
    };
    if !source.contains(&specifier) {
      continue;
    }
    let ext = Path::new(&file).extension().and_then(|e| e.to_str()).unwrap_or("").to_ascii_lowercase();
    let target = match snapfire_compiler_wire::claimed(&ext) {
      Some("vue") => VUE.module,
      Some("svelte") => SVELTE_COMPONENTS,
      _ => match reached.get(&file) {
        Some(adapter) => adapter,
        None if react => REACT.module,
        None => "@snapfire/fsr/client/jsx-runtime",
      },
    };
    let source = match target == SVELTE_COMPONENTS {
      true => svelte_component_imports(&source),
      false => source,
    };
    out.push((file, source.replace(&specifier, &format!("\"{target}\""))));
  }
  Ok(out)
}

/// `source` with each `import { Link, Picture as Shot } from "@snapfire/fsr/components"` written as one default import per component from its own file under `generated/components`, so a `.svelte` file loads only the components it uses. A type import is left for the index, as is one naming anything else.
pub(crate) fn svelte_component_imports(source: &str) -> String {
  let pattern = regex::Regex::new(&format!(r#"import\s*\{{([^}}]*)\}}\s*from\s*["']{}["'];?"#, regex::escape(snapfire_fsr_lower::COMPONENTS_MODULE))).expect("the pattern compiles");
  pattern
    .replace_all(source, |caps: &regex::Captures| {
      let mut imports = Vec::new();
      for name in caps[1].split(',').map(str::trim).filter(|n| !n.is_empty()) {
        let (imported, local) = name.split_once(" as ").map(|(i, l)| (i.trim(), l.trim())).unwrap_or((name, name));
        if !SVELTE_COMPONENT_FILES.iter().any(|(component, _)| *component == imported) {
          return caps[0].to_owned();
        }
        imports.push(format!("import {local} from \"{SVELTE_COMPONENTS}/{imported}.svelte\";"));
      }
      imports.join("\n  ")
    })
    .into_owned()
}

/// The script and component files under `dir`, relative to `app`.
pub(crate) fn source_files(app: &Path, dir: &Path, out: &mut Vec<String>) -> Result<(), BuildError> {
  let mut entries: Vec<PathBuf> = std::fs::read_dir(dir).map_err(|e| BuildError::Io(dir.to_path_buf(), e))?.flatten().map(|e| e.path()).collect();
  entries.sort();
  for path in entries {
    if path.is_dir() {
      source_files(app, &path, out)?;
      continue;
    }
    let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("").to_ascii_lowercase();
    if matches!(ext.as_str(), "ts" | "tsx" | "js" | "jsx" | "mts" | "mjs") || snapfire_compiler_wire::claimed(&ext).is_some() {
      if let Ok(relative) = path.strip_prefix(app) {
        out.push(relative.to_string_lossy().replace('\\', "/"));
      }
    }
  }
  Ok(())
}

/// Every JSX file under `app` that imports a component another framework
/// mounts, with each such import replaced by a `foreign` component of React's
/// adapter, by path in the bundle overlay. React's `jsx` takes no other
/// framework's component, so `<Chart />` from `Chart.vue` places it by id.
pub(crate) fn foreign_rewrites(app: &Path, set: &mut ComponentSet, rewritten: &[(String, String)]) -> Result<Vec<(String, String)>, BuildError> {
  let mut files = Vec::new();
  jsx_files(app, app, &mut files)?;
  let mut out = Vec::new();
  for file in files {
    let imports = set.foreign_imports(&file);
    if imports.is_empty() {
      continue;
    }
    let mut source = match rewritten.iter().find(|(f, _)| *f == file) {
      Some((_, text)) => text.clone(),
      None => {
        let path = app.join(&file);
        std::fs::read_to_string(&path).map_err(|e| BuildError::Io(path, e))?
      }
    };
    for import in imports {
      let consts: String = import.bindings.iter().map(|(local, module)| format!("const {local} = __sfForeign({module:?});")).collect::<Vec<_>>().join(" ");
      source = source.replacen(&import.text, &consts, 1);
    }
    out.push((file, format!("import {{ foreign as __sfForeign }} from \"{}\";\n{source}", REACT.module)));
  }
  Ok(out)
}

/// The `.tsx` and `.jsx` files under `dir`, relative to `app`, leaving out
/// what the build and the vendor step write.
pub(crate) fn jsx_files(app: &Path, dir: &Path, out: &mut Vec<String>) -> Result<(), BuildError> {
  let mut entries: Vec<PathBuf> = std::fs::read_dir(dir).map_err(|e| BuildError::Io(dir.to_path_buf(), e))?.flatten().map(|e| e.path()).collect();
  entries.sort();
  for path in entries {
    let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    if path.is_dir() {
      if name.starts_with('.') || (path.parent() == Some(app) && ["generated", "dist", "types", "vendor", "node_modules"].contains(&name.as_str())) {
        continue;
      }
      jsx_files(app, &path, out)?;
    } else if name.ends_with(".tsx") || name.ends_with(".jsx") {
      if let Ok(relative) = path.strip_prefix(app) {
        out.push(relative.to_string_lossy().replace('\\', "/"));
      }
    }
  }
  Ok(())
}

pub(crate) const ADAPTERS: &[&Adapter] = &[&REACT, &VUE, &SVELTE];

/// The packages a client adapter imports at runtime. A shell records the
/// version it vendors of each, so a site built against it renders under the
/// same ones and its specs fetch matching development builds.
pub(crate) fn framework_packages() -> Vec<String> {
  let mut packages: Vec<String> = ADAPTERS.iter().flat_map(|adapter| adapter.needs.iter()).map(|need| crate::vendor::package_of(need)).collect();
  packages.sort();
  packages.dedup();
  packages
}

/// The specifiers a client adapter imports from one framework package.
pub(crate) fn framework_needs(package: &str) -> Vec<&'static str> {
  ADAPTERS.iter().flat_map(|adapter| adapter.needs.iter().copied()).filter(|need| crate::vendor::package_of(need) == package).collect()
}

/// The version `fsr add` is suggested with for a package nothing records.
pub(crate) fn suggested_version(package: &str) -> &'static str {
  match package {
    "react" | "react-dom" => crate::direction::REACT,
    "svelte" => crate::direction::SVELTE,
    _ => "<version>",
  }
}

/// The adapter a module is registered with. A source the lowerer reads is
/// React's; a foreign one belongs to the framework whose plugin claims its
/// extension, when the client has an adapter for that framework.
pub(crate) fn adapter_for(module: &str, owners: &HashMap<String, Owner>) -> Result<&'static Adapter, BuildError> {
  match owners.get(module) {
    Some(Owner::Vue) => return Ok(&VUE),
    Some(Owner::Svelte) => return Ok(&SVELTE),
    Some(Owner::React | Owner::Fsr) => return Ok(&REACT),
    None => {}
  }
  let file = module.split_once('#').map(|(file, _)| file).unwrap_or(module);
  if !snapfire_fsr_lower::component::is_foreign(file) {
    return Ok(&REACT);
  }
  let ext = Path::new(file).extension().and_then(|e| e.to_str()).unwrap_or("").to_ascii_lowercase();
  match snapfire_compiler_wire::claimed(&ext) {
    Some("vue") => Ok(&VUE),
    Some("svelte") => Ok(&SVELTE),
    Some(ext) => Err(BuildError::NoAdapter { module: module.to_owned(), ext: ext.to_owned() }),
    None => Err(BuildError::UnknownComponent { module: module.to_owned() }),
  }
}
