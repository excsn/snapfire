use super::*;

pub(crate) fn discover(root: &Path, dir: &Path, out: &mut Vec<Route>, handlers: &mut Vec<Route>) -> Result<(), BuildError> {
  let mut children: Vec<PathBuf> = std::fs::read_dir(dir)
    .map_err(|e| BuildError::Io(dir.to_path_buf(), e))?
    .filter_map(|e| e.ok().map(|e| e.path()))
    .filter(|p| p.is_dir())
    .collect();
  children.sort();

  let page = page_file(dir)?.is_some();
  let handler = dir.join("route.ts").is_file();
  if page && handler {
    return Err(BuildError::PageAndRoute(dir.to_path_buf()));
  }
  if !page && dir.join("actions.ts").is_file() {
    return Err(BuildError::ActionsWithoutPage(dir.to_path_buf()));
  }
  if page || handler {
    let (pattern, id) = pattern_of(root, dir)?;
    let route = Route { pattern, dir: dir.to_path_buf(), id };
    if page {
      out.push(route);
    } else {
      handlers.push(route);
    }
  }
  for child in children {
    if child.file_name().is_some_and(|n| n == "slots") {
      if layout_file(dir)?.is_none() {
        return Err(BuildError::SlotsWithoutLayout(child));
      }
      continue;
    }
    discover(root, &child, out, handlers)?;
  }
  Ok(())
}

/// The image files a crawler's card reads beside a route, by stem: Open Graph's and Twitter's.
pub(crate) const SOCIAL_IMAGES: [(&str, &str); 2] = [("opengraph-image", "og"), ("twitter-image", "twitter")];

/// What a social card's crawler decodes.
pub(crate) const SOCIAL_IMAGE_TYPES: [(&str, &str); 5] = [("png", "image/png"), ("jpg", "image/jpeg"), ("jpeg", "image/jpeg"), ("gif", "image/gif"), ("webp", "image/webp")];

/// Whether a card image file is a component that draws it rather than the image itself.
pub(crate) fn is_generated_image(path: &Path) -> bool {
  path.extension().is_some_and(|e| e == "tsx" || e == "ts")
}

/// The size a generated card image is drawn at when its module exports none,
/// the one Open Graph recommends.
pub(crate) const GENERATED_IMAGE_SIZE: (u32, u32) = (1200, 630);

pub(crate) fn draw_image(app: &Path, routes_dir: &Path, dir: &Path, file: &Path, stem: &str, set: &mut ComponentSet, sources: &mut Vec<SourceEntry>) -> Result<DrawnImage, BuildError> {
  let refuse = |why: String| BuildError::SocialImage(file.to_path_buf(), why);
  let rel = file.strip_prefix(app).unwrap_or(file).to_string_lossy().replace('\\', "/");
  let module = format!("{rel}#default");
  set.lower(&module).map_err(|e| refuse(format!("it draws the image on the server and must lower: {e}")))?;
  let Some((_, component)) = set.components.iter().find(|(m, _)| *m == module) else {
    return Err(refuse("it draws the image on the server and must lower".to_owned()));
  };
  if component.owner.hydrates() {
    return Err(refuse("it holds state or a handler, and an image is drawn once with nothing to run in a browser".to_owned()));
  }
  if !island_modules(&component.render).is_empty() {
    return Err(refuse("it places an island, and an image has no browser to mount one in".to_owned()));
  }
  let hash = snapfire_fsr_assets::hash::of(format!("{component:?}").as_bytes());
  let exports = set.image_exports(&rel).map_err(|e| refuse(e.to_string()))?;
  let (width, height) = match (exports.width, exports.height) {
    (Some(w), Some(h)) => (w, h),
    _ => GENERATED_IMAGE_SIZE,
  };
  if width > snapfire_fsr_core::CARD_MAX_SIDE || height > snapfire_fsr_core::CARD_MAX_SIDE {
    return Err(refuse(format!("`size` is {width}x{height}; each side is at most {}", snapfire_fsr_core::CARD_MAX_SIDE)));
  }
  let content_type = exports.content_type.unwrap_or_else(|| "image/png".to_owned());
  let ext = match content_type.as_str() {
    "image/png" => "png",
    "image/jpeg" => "jpg",
    other => return Err(refuse(format!("`contentType` is `{other}`; a generated image is `image/png` or `image/jpeg`"))),
  };
  let alt_file = dir.join(format!("{stem}.alt.txt"));
  let alt = match exports.alt {
    Some(alt) => Some(alt),
    None if alt_file.is_file() => Some(std::fs::read_to_string(&alt_file).map_err(|e| BuildError::Io(alt_file.clone(), e))?.trim().to_owned()),
    None => None,
  };
  let images = set.lower_images(&rel).map_err(|e| refuse(e.to_string()))?;
  let loader = dir.join(format!("{stem}.loader.ts"));
  let source = if loader.is_file() {
    let loader_rel = loader.strip_prefix(app).unwrap_or(&loader).to_string_lossy().replace('\\', "/");
    let body = set.lower_loader(&loader_rel).map_err(|e| BuildError::SocialImage(loader.clone(), format!("an image's loader must lower: {e}")))?;
    let (_, dir_id) = pattern_of(routes_dir, dir)?;
    let id = if dir_id.is_empty() { stem.to_owned() } else { format!("{dir_id}.{stem}") };
    sources.push(SourceEntry::lowered(id.clone(), loader_rel, body));
    Some(id)
  } else {
    None
  };
  Ok(DrawnImage { module, source, width, height, content_type, ext, hash, alt, images })
}

/// Lowers every `opengraph-image.tsx` and `twitter-image.tsx` under
/// `routes/` into a component and writes, for each page it covers, the image
/// row the host answers its URL from and, unless the module lists its images,
/// the head rows that name it on the page. A page takes the nearest image at
/// or above its directory; a static image nearer than a component wins. The
/// component's props are its own loader's data when `<stem>.loader.ts` sits
/// beside it, else the data of the page or layout beside it, with `params`.
/// A page under the image's directory gets the same: the page's loader there
/// runs for the params the two share, and the layout's when no page has one.
pub(crate) fn generated_images(app: &Path, routes_dir: &Path, routes: &[Route], set: &mut ComponentSet, sources: &mut Vec<SourceEntry>) -> Result<(Vec<snapfire_fsr_plan::ImageEntry>, Vec<snapfire_fsr_plan::ModuleHead>), BuildError> {
  let mut drawn: HashMap<PathBuf, DrawnImage> = HashMap::new();
  let mut images = Vec::new();
  let mut heads = Vec::new();
  for route in routes {
    let Some(page) = page_file(&route.dir)? else { continue };
    let rel_dir = route.dir.strip_prefix(app).unwrap_or(&route.dir).to_string_lossy().replace('\\', "/");
    let page_module = format!("{rel_dir}/{page}#default");
    let mut head = Vec::new();
    for (stem, prefix) in SOCIAL_IMAGES {
      let mut found = None;
      for dir in route.dir.ancestors() {
        if !dir.starts_with(routes_dir) {
          break;
        }
        if let Some(file) = ["tsx", "ts"].iter().map(|ext| dir.join(format!("{stem}.{ext}"))).find(|f| f.is_file()) {
          found = Some((dir.to_path_buf(), file));
          break;
        }
        if SOCIAL_IMAGE_TYPES.iter().any(|(ext, _)| dir.join(format!("{stem}.{ext}")).is_file()) {
          break;
        }
      }
      let Some((dir, file)) = found else { continue };
      if !drawn.contains_key(&file) {
        let image = draw_image(app, routes_dir, &dir, &file, stem, set, sources)?;
        drawn.insert(file.clone(), image);
      }
      let image = &drawn[&file];
      let source = match &image.source {
        Some(own) => Some(own.clone()),
        None if dir == route.dir => sources.iter().find(|s| s.id == route.id).map(|s| s.id.clone()),
        None => {
          let (_, dir_id) = pattern_of(routes_dir, &dir)?;
          let layout = if dir == routes_dir { "layout".to_owned() } else { format!("{dir_id}.layout") };
          let beside = routes.iter().find(|r| r.dir == dir).map(|r| r.id.clone());
          sources.iter().find(|s| Some(&s.id) == beside.as_ref()).or_else(|| sources.iter().find(|s| s.id == layout)).map(|s| s.id.clone())
        }
      };
      let base = if route.pattern == "/" { String::new() } else { route.pattern.clone() };
      let pattern = match image.images {
        Some(_) => format!("{base}/{stem}/{{{}}}", snapfire_fsr_plan::IMAGE_PARAM),
        None => format!("{base}/{stem}.{}", image.ext),
      };
      if image.images.is_none() {
        let (key, field) = if prefix == "og" { ("property", "og:image") } else { ("name", "twitter:image") };
        let row = |name: String, content: String| snapfire_fsr_plan::HeadRow { tag: "meta".to_owned(), attrs: vec![(key.to_owned(), name), ("content".to_owned(), content)] };
        head.push(row(field.to_owned(), format!("{}/{stem}.{}?v={}", snapfire_fsr_runtime::PATH_MARK, image.ext, image.hash)));
        head.push(row(format!("{field}:width"), image.width.to_string()));
        head.push(row(format!("{field}:height"), image.height.to_string()));
        head.push(row(format!("{field}:type"), image.content_type.clone()));
        if let Some(alt) = &image.alt {
          head.push(row(format!("{field}:alt"), alt.clone()));
        }
      }
      images.push(snapfire_fsr_plan::ImageEntry {
        pattern,
        module: image.module.clone(),
        source,
        width: image.width,
        height: image.height,
        content_type: image.content_type.clone(),
        page: page_module.clone(),
        card: prefix.to_owned(),
        hash: image.hash.clone(),
        alt: image.alt.clone(),
        images: image.images.clone(),
      });
    }
    if !head.is_empty() {
      heads.push(snapfire_fsr_plan::ModuleHead { module: page_module, head });
    }
  }
  Ok((images, heads))
}

/// The head rows of every `opengraph-image.<ext>` and `twitter-image.<ext>`
/// under `routes/`, covering every route under its directory, a nearer image
/// replacing them. They go on the module of the directory's layout, which
/// every route beneath it renders, or without one on each page beneath it
/// that no nearer image covers. `<stem>.alt.txt` beside one is its alt text.
/// Each image is served as its hashed original.
pub(crate) fn social_images(app: &Path, routes: &Path, resolver: &assets::Resolver) -> Result<Vec<snapfire_fsr_plan::ModuleHead>, BuildError> {
  let mut found = Vec::new();
  image_dirs(app, routes, resolver, &mut found)?;
  let module = |dir: &Path, file: String| format!("{}/{file}#default", dir.strip_prefix(app).unwrap_or(dir).to_string_lossy().replace('\\', "/"));
  let mut out = Vec::new();
  for (dir, head) in &found {
    if let Some(file) = layout_file(dir)? {
      out.push(snapfire_fsr_plan::ModuleHead { module: module(dir, file), head: head.clone() });
      continue;
    }
    let mut pages = Vec::new();
    pages_under(dir, &mut pages)?;
    let covered: Vec<&PathBuf> = pages.iter().filter(|page| !found.iter().any(|(other, _)| other != dir && other.starts_with(dir) && page.starts_with(other))).collect();
    if covered.is_empty() {
      return Err(BuildError::SocialImage(dir.clone(), "a card image covers the routes under its directory, and there is none".to_owned()));
    }
    for page in covered {
      let file = page_file(page)?.expect("a page directory holds a page");
      out.push(snapfire_fsr_plan::ModuleHead { module: module(page, file), head: head.clone() });
    }
  }
  Ok(out)
}

/// Every directory under `dir` holding a card image, with the rows it writes.
pub(crate) fn image_dirs(app: &Path, dir: &Path, resolver: &assets::Resolver, out: &mut Vec<(PathBuf, Vec<snapfire_fsr_plan::HeadRow>)>) -> Result<(), BuildError> {
  let mut children: Vec<PathBuf> = std::fs::read_dir(dir).map_err(|e| BuildError::Io(dir.to_path_buf(), e))?.filter_map(|e| e.ok().map(|e| e.path())).collect();
  children.sort();
  let mut head = Vec::new();
  for (stem, prefix) in SOCIAL_IMAGES {
    let found: Vec<&PathBuf> = children.iter().filter(|p| p.is_file() && p.file_stem().is_some_and(|s| s == stem) && !is_generated_image(p)).collect();
    let generated = children.iter().find(|p| p.file_stem().is_some_and(|s| s == stem) && is_generated_image(p));
    if let (Some(file), Some(_)) = (found.first(), generated) {
      return Err(BuildError::SocialImage(dir.to_path_buf(), format!("`{}` and `{stem}.tsx` both draw its {stem}; keep one", file.display())));
    }
    if let Some(file) = generated {
      let mut pages = Vec::new();
      pages_under(dir, &mut pages)?;
      if pages.is_empty() {
        return Err(BuildError::SocialImage(file.clone(), "a generated card image covers the pages under its directory, and there is none".to_owned()));
      }
    }
    let file = match found.as_slice() {
      [] => continue,
      [one] => *one,
      [first, second, ..] => return Err(BuildError::SocialImage(dir.to_path_buf(), format!("`{}` and `{}` are both its {stem}; keep one", first.display(), second.display()))),
    };
    let ext = file.extension().and_then(|e| e.to_str()).map(str::to_ascii_lowercase).unwrap_or_default();
    let Some((_, mime)) = SOCIAL_IMAGE_TYPES.iter().find(|(e, _)| *e == ext) else {
      return Err(BuildError::SocialImage(file.clone(), format!("a card image is png, jpg, gif or webp, which a crawler decodes, not `{ext}`")));
    };
    let rel = file.strip_prefix(app).unwrap_or(file).to_string_lossy().replace('\\', "/");
    let Some(facts) = resolver.original(&rel) else {
      let why = resolver.refused.borrow().iter().rev().find(|(p, _)| *p == rel).map(|(_, why)| why.clone()).unwrap_or_else(|| "not an image this build reads".to_owned());
      return Err(BuildError::SocialImage(file.clone(), why));
    };
    let (key, field) = if prefix == "og" { ("property", "og:image") } else { ("name", "twitter:image") };
    let row = |name: String, content: String| snapfire_fsr_plan::HeadRow { tag: "meta".to_owned(), attrs: vec![(key.to_owned(), name), ("content".to_owned(), content)] };
    head.push(row(field.to_owned(), facts.src));
    head.push(row(format!("{field}:width"), facts.width.to_string()));
    head.push(row(format!("{field}:height"), facts.height.to_string()));
    head.push(row(format!("{field}:type"), (*mime).to_owned()));
    let alt = dir.join(format!("{stem}.alt.txt"));
    if alt.is_file() {
      let text = std::fs::read_to_string(&alt).map_err(|e| BuildError::Io(alt.clone(), e))?;
      head.push(row(format!("{field}:alt"), text.trim().to_owned()));
    }
  }
  if !head.is_empty() {
    out.push((dir.to_path_buf(), head));
  }
  for child in children.iter().filter(|p| p.is_dir() && p.file_name().is_some_and(|n| n != "slots")) {
    image_dirs(app, child, resolver, out)?;
  }
  Ok(())
}

/// `dir` and every directory under it that holds a page.
pub(crate) fn pages_under(dir: &Path, out: &mut Vec<PathBuf>) -> Result<(), BuildError> {
  if page_file(dir)?.is_some() {
    out.push(dir.to_path_buf());
  }
  let mut children: Vec<PathBuf> = std::fs::read_dir(dir).map_err(|e| BuildError::Io(dir.to_path_buf(), e))?.filter_map(|e| e.ok().map(|e| e.path())).filter(|p| p.is_dir() && p.file_name().is_some_and(|n| n != "slots")).collect();
  children.sort();
  for child in children {
    pages_under(&child, out)?;
  }
  Ok(())
}

/// The one page file in `dir`: `page.tsx`, `page.ts` or a `page.<ext>` for an
/// extension in `TEMPLATE_EXTENSIONS`. Two of them is a refusal naming both,
/// and a template is a refusal when this fsr was built without its feature,
/// so a template never becomes a silent 404.
pub(crate) fn page_file(dir: &Path) -> Result<Option<String>, BuildError> {
  let found = route_file(dir, &["page.tsx", "page.ts", "page.jsx", "page.js", "page.mdx", "page.md"], "page")?;
  if let Some(file) = found.as_deref().filter(|f| snapfire_fsr_lower::component::is_transformed(f)) {
    if !mdx::BUILT {
      return Err(mdx::unbuilt(&dir.join(file)));
    }
  }
  Ok(found)
}

/// The one layout file in `dir`, `layout.tsx` or a `layout.<ext>` for a
/// template extension, under the same rules as `page_file`.
pub(crate) fn layout_file(dir: &Path) -> Result<Option<String>, BuildError> {
  route_file(dir, &["layout.tsx", "layout.ts", "layout.jsx", "layout.js"], "layout")
}

/// The extensions a route's component module may be written with, in the
/// order a lookup prefers them.
pub(crate) const COMPONENT_EXTENSIONS: &[&str] = &["tsx", "ts", "jsx", "js"];

/// `<stem>.<ext>` in `dir` for the first of `COMPONENT_EXTENSIONS` present.
pub(crate) fn component_file(dir: &Path, stem: &str) -> Option<String> {
  COMPONENT_EXTENSIONS.iter().map(|ext| format!("{stem}.{ext}")).find(|f| dir.join(f).is_file())
}

pub(crate) fn route_file(dir: &Path, sources: &[&str], stem: &str) -> Result<Option<String>, BuildError> {
  let templates: Vec<String> = snapfire_fsr_host::TEMPLATE_EXTENSIONS.iter().map(|ext| format!("{stem}.{ext}")).collect();
  let present: Vec<String> = sources.iter().map(|s| (*s).to_owned()).chain(templates.iter().cloned()).filter(|f| dir.join(f).is_file()).collect();
  match present.as_slice() {
    [] => Ok(None),
    [one] => {
      if templates.contains(one) && !cfg!(feature = "tera") {
        return Err(BuildError::TemplateFeature(dir.join(one)));
      }
      Ok(Some(one.clone()))
    }
    [first, second, ..] => Err(BuildError::PageAndTemplate { dir: dir.to_path_buf(), first: first.clone(), second: second.clone() }),
  }
}

/// The modules a template places as islands, `island(module="...")` with a
/// string literal, so they are bundled, registered and, for server mode,
/// lowered the way a TSX placement's are. An `island(` whose `module` is not
/// a literal is refused naming the line, since the build cannot bundle a
/// module it cannot name.
pub(crate) fn template_islands(file: &Path) -> Result<Vec<String>, BuildError> {
  let text = std::fs::read_to_string(file).map_err(|e| BuildError::Io(file.to_path_buf(), e))?;
  let call = regex::Regex::new(r#"\bisland\s*\("#).expect("a literal pattern");
  let literal = regex::Regex::new(r#"\bmodule\s*=\s*(?:"([^"]+)"|'([^']+)')"#).expect("a literal pattern");
  let mut out = Vec::new();
  for found in call.find_iter(&text) {
    let rest = &text[found.end()..];
    let args = &rest[..rest.find(')').unwrap_or(rest.len())];
    match literal.captures(args).and_then(|c| c.get(1).or_else(|| c.get(2))) {
      Some(m) => out.push(m.as_str().to_owned()),
      None => {
        let line = text[..found.start()].matches('\n').count() + 1;
        return Err(BuildError::TemplateIsland { file: file.to_path_buf(), line });
      }
    }
  }
  Ok(out)
}

/// Whether a module id names a template file rather than a component.
pub(crate) fn is_template(module: &str) -> bool {
  let path = module.split('#').next().unwrap_or(module);
  snapfire_fsr_host::TEMPLATE_EXTENSIONS.iter().any(|ext| path.ends_with(&format!(".{ext}")))
}

/// `routes/index` is `/`; `routes/product/[id]` is `/product/{id}`;
/// `routes/docs/[...rest]` is `/docs/{*rest}`. A directory holding a
/// `page.tsx` is a route at its own path, so `routes/` itself is `/` and
/// `routes/index/` is `/index`; no directory name is special. The id is every
/// segment joined with `.`, `$root` for the root, a parameter contributing
/// `$<name>`. The marker is what makes the id injective: a directory name is
/// alphanumerics, `_`, `-` and `.` only, so no static segment can produce a `$`
/// part and `routes/a/x` can never share an id with `routes/a/[x]`. A `.` in a
/// name, `robots.txt` or `.well-known`, is `~` in the id, so `routes/a.b` and
/// `routes/a/b` stay apart.
pub(crate) fn pattern_of(root: &Path, dir: &Path) -> Result<(String, String), BuildError> {
  let rel = dir.strip_prefix(root).unwrap_or(dir);
  let mut segments = Vec::new();
  let mut id_parts = Vec::new();
  for component in rel.components() {
    let name = component.as_os_str().to_string_lossy().to_string();
    if let Some(group) = name.strip_prefix('(').and_then(|n| n.strip_suffix(')')) {
      if group.is_empty() || !group.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-') {
        return Err(BuildError::Segment { path: dir.to_path_buf(), name });
      }
      id_parts.push(name);
      continue;
    }
    if let Some(inner) = name.strip_prefix('[').and_then(|n| n.strip_suffix(']')) {
      if let Some(rest) = inner.strip_prefix("...") {
        segments.push(format!("{{*{rest}}}"));
        id_parts.push(format!("${rest}"));
      } else {
        segments.push(format!("{{{inner}}}"));
        id_parts.push(format!("${inner}"));
      }
      continue;
    }
    if name.is_empty() || name.chars().all(|c| c == '.') || !name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-' || c == '.') {
      return Err(BuildError::Segment { path: dir.to_path_buf(), name });
    }
    segments.push(name.clone());
    id_parts.push(name.replace('.', "~"));
  }
  let pattern = if segments.is_empty() { "/".to_owned() } else { format!("/{}", segments.join("/")) };
  let id = if id_parts.is_empty() { "$root".to_owned() } else { id_parts.join(".") };
  Ok((pattern, id))
}

/// A head script with inline code is refused: a navigation inserts head rows
/// as data the client loads by `src`, so the code would run on a document load
/// and never after a navigation.
pub(crate) fn refuse_inline_scripts(module: &str, meta: Option<&snapfire_fsr_ir::Body>) -> Result<(), BuildError> {
  match meta.is_some_and(|body| snapfire_fsr_ir::ast::head_scripts(body).iter().any(|script| script.inline)) {
    true => Err(BuildError::InlineScript(module.to_owned())),
    false => Ok(()),
  }
}
