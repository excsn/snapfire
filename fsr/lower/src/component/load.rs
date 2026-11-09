use super::*;

impl ComponentSet {
  /// `file` parsed, a described foreign component's script for one. None when it does not parse.
  pub(super) fn script_of(&mut self, file: &str) -> Option<Rc<Cached>> {
    match self.described.get(file).and_then(|described| described.script.as_ref()).or_else(|| self.scripts_only.get(file)) {
      Some(script) => Cached::new(|arena| parse_with(arena, file, &script.content, false)).ok().map(Rc::new),
      None => {
        self.load(file).ok()?;
        Some(self.parsed(file))
      }
    }
  }

  /// `file` parsed into `arena`, with what its names mean once the set has read them. The file has loaded.
  pub(super) fn parsed(&self, file: &str) -> Rc<Cached> {
    self.parsed_if(file).unwrap_or_else(|| panic!("`{file}` is read only once it has loaded"))
  }

  /// `file` parsed into `arena`, `None` when it has not loaded.
  pub(super) fn parsed_if(&self, file: &str) -> Option<Rc<Cached>> {
    let known = self.parses.borrow().get(file).cloned();
    let cached = match known {
      Some(cached) => cached,
      None => {
        let source = self.sources.get(file)?;
        let cached = Rc::new(Cached::new(|arena| source.parse(arena)).ok()?);
        self.parses.borrow_mut().insert(file.to_owned(), cached.clone());
        cached
      }
    };
    let parsed = cached.get();
    if parsed.meanings.get().is_none() {
      if let Some(meanings) = self.meanings.get(file) {
        let _ = parsed.meanings.set(Rc::new(meanings.with_scope(parsed.scope.clone())));
      }
    }
    Some(cached)
  }

  /// Holds `source` as what `file` is parsed from once it parses, and registers the classes it declares.
  pub(super) fn keep(&mut self, file: &str, source: Source) -> Result<(), LowerError> {
    source.parse(&Allocator::default())?;
    self.sources.insert(file.to_owned(), Rc::new(source));
    self.register_classes(file);
    Ok(())
  }

  /// Lowers route module `module`, and when its own body holds what the
  /// lowerer cannot read, moves the part that holds it into an island beside
  /// it and lowers the page again. The island is then a component that does
  /// not lower, which the browser renders alone.
  /// Drops what the set holds for `file` so it is read again: its parse, its
  /// failure and every file's meanings, which copy what `file` exports.
  pub(super) fn forget(&mut self, file: &str) {
    self.sources.remove(file);
    self.failed.remove(file);
    self.meanings.clear();
    self.parses.borrow_mut().clear();
  }

  pub(super) fn load(&mut self, file: &str) -> Result<(), LowerError> {
    if self.sources.contains_key(file) {
      return Ok(());
    }
    if let Some(error) = self.failed.get(file) {
      return Err(error.clone());
    }
    if let Some((base, overrides)) = self.variants.get(file).cloned() {
      let source = match self.provided.get(&base) {
        Some(source) => Ok(source.clone()),
        None => std::fs::read_to_string(self.app.join(&base)).map_err(|e| LowerError::Parse { file: base.clone(), message: e.to_string() }),
      };
      let kept = source.and_then(|source| self.transformed(&base, &source, &overrides)).and_then(|source| self.keep(file, source));
      if let Err(error) = &kept {
        self.failed.insert(file.to_owned(), error.clone());
      }
      return kept;
    }
    let kept = match self.provided.get(file) {
      Some(source) => Ok(source.clone()),
      None => std::fs::read_to_string(self.app.join(file)).map_err(|e| LowerError::Parse { file: file.to_owned(), message: e.to_string() }),
    }
    .and_then(|source: String| match is_transformed(file) {
      true => self.transformed(file, &source, &[]),
      false if file.ends_with(".json") => Ok(Source { name: file.to_owned(), text: format!("export default {source};"), tsx: false, origin: None, vue_this: false }),
      false => Ok(Source { name: file.to_owned(), text: source, tsx: crate::jsx_parses(file), origin: None, vue_this: false }),
    })
    .and_then(|source| self.keep(file, source));
    if let Err(error) = &kept {
      self.failed.insert(file.to_owned(), error.clone());
    }
    kept
  }

  /// Every `import(`prefix${x}suffix`)` in `file`: by where its call starts,
  /// the files under the app it can load, each with the value of `x` that
  /// names it, and as globals every value each of those files exports, keyed
  /// `file#export`, so a destructured `meta` picks among them.
  pub(super) fn template_imports(&mut self, file: &str) -> Result<(HashMap<u32, Vec<(String, String)>>, Vec<(String, Expr)>), LowerError> {
    let parsed_cached = self.parsed(file);
    let parsed = parsed_cached.get();
    let mut imports = HashMap::new();
    let mut globals = Vec::new();
    for (lo, prefix, suffix) in template_import_calls(&parsed.module()) {
      let choices = self.import_choices(file, &prefix, &suffix);
      for (_, target) in &choices {
        self.load(target)?;
        let exports: Vec<String> = self.meanings(target).exports.keys().filter(|name| *name != "default").cloned().collect();
        for export in exports {
          let key = format!("{target}#{export}");
          if globals.iter().any(|(k, _)| *k == key) {
            continue;
          }
          if let Ok(Some((expr, named))) = self.global(target, &export) {
            let expr = self.name_if_large(named, expr);
            globals.push((key, expr));
          }
        }
      }
      imports.insert(lo, choices);
    }
    Ok((imports, globals))
  }

  /// The files `prefix${x}suffix` names from `file`, anywhere under the
  /// directory the prefix ends in, each with the text that stands for `x`,
  /// which holds a `/` for a file in a subdirectory as webpack's context does.
  pub(super) fn import_choices(&self, file: &str, prefix: &str, suffix: &str) -> Vec<(String, String)> {
    let (dir_spec, start) = match prefix.rsplit_once('/') {
      Some((dir, start)) => (dir.to_owned(), start.to_owned()),
      None => (".".to_owned(), prefix.to_owned()),
    };
    let Some(probe) = crate::resolve_specifier(file, &format!("{dir_spec}/x")) else { return Vec::new() };
    let dir = probe.rsplit_once('/').map(|(d, _)| d.to_owned()).unwrap_or_default();
    let mut names = files_under(&self.app.join(&dir));
    for provided in self.provided.keys() {
      if let Some(name) = provided.strip_prefix(&format!("{dir}/")) {
        names.push(name.to_owned());
      }
    }
    names.sort();
    names.dedup();
    names
      .into_iter()
      .filter_map(|name| {
        let middle = name.strip_prefix(start.as_str())?.strip_suffix(suffix)?;
        (!middle.is_empty() && !is_foreign(&name)).then(|| (middle.to_owned(), if dir.is_empty() { name.clone() } else { format!("{dir}/{name}") }))
      })
      .collect()
  }

  /// `source`, the text of `file`, as the module the transform writes from it.
  pub(super) fn transformed(&self, file: &str, source: &str, overrides: &[Override]) -> Result<Source, LowerError> {
    let Some(transform) = &self.transform else {
      return Err(LowerError::Parse { file: file.to_owned(), message: "an MDX or markdown module, which this fsr reads only when built with its `mdx` feature".to_owned() });
    };
    let written = transform(file, source, overrides).map_err(|message| LowerError::Parse { file: file.to_owned(), message })?;
    Ok(Source { name: file.to_owned(), text: written.source, tsx: true, origin: written.origin.map(Rc::new), vue_this: false })
  }

  pub(super) fn resolve_import(&self, from: &str, source: &str) -> Option<String> {
    if self.reaches(Reach::Terminal) {
      if let Some((_, file, _)) = crate::terminal::COMPANIONS.iter().find(|(specifier, _, _)| *specifier == source) {
        return Some((*file).to_owned());
      }
    }
    let joined = crate::resolve_specifier(from, source)?;
    // TypeScript resolves a `.js` or `.jsx` specifier to the source it compiles to that name.
    let compiled: Vec<String> = match (joined.strip_suffix(".js"), joined.strip_suffix(".jsx")) {
      (Some(stem), _) => vec![format!("{stem}.ts"), format!("{stem}.tsx"), format!("{stem}.jsx")],
      (_, Some(stem)) => vec![format!("{stem}.tsx")],
      _ => Vec::new(),
    };
    let named = [joined.clone(), format!("{joined}.tsx"), format!("{joined}.ts"), format!("{joined}.jsx"), format!("{joined}.js")];
    let index = ["index.tsx", "index.ts", "index.jsx", "index.js"].map(|name| format!("{joined}/{name}"));
    for candidate in named.into_iter().chain(index).chain(compiled) {
      if candidate.starts_with("generated/") && candidate != crate::HEAD_MODULE && candidate != crate::CONTENT_MODULE {
        continue;
      }
      if self.provided.contains_key(&candidate) || self.app.join(&candidate).is_file() {
        return Some(candidate);
      }
    }
    None
  }
}

impl Cached {
  pub(super) fn new(parse: impl FnOnce(&'static Allocator) -> Result<Parsed<'static>, LowerError>) -> Result<Self, LowerError> {
    let arena = Box::new(Allocator::default());
    // SAFETY: the box is never moved out of or replaced while `parsed` lives, and `parsed` drops first, so every
    // reference into the arena is dead before the arena is freed. `get` hands out borrows no longer than `&self`.
    let static_arena: &'static Allocator = unsafe { &*(arena.as_ref() as *const Allocator) };
    let parsed = parse(static_arena)?;
    Ok(Cached { parsed, _arena: arena })
  }

  pub(super) fn get<'s>(&'s self) -> &'s Parsed<'s> {
    // SAFETY: shortening the arena lifetime to the borrow of `self`, which owns the arena.
    unsafe { std::mem::transmute::<&'s Parsed<'static>, &'s Parsed<'s>>(&self.parsed) }
  }
}
