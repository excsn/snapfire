//! `fsr use`: gives an existing application a direction. A client direction's
//! adapter line goes into the import map, the framework it pins is vendored and
//! the declarations and generated files follow, so a bare application takes on
//! React, Vue, custom elements or htmx at any point rather than at `fsr new`.
//! Tailwind adds a stylesheet beside the application's own.

use std::path::{Path, PathBuf};

use crate::vendor::{self, Spec, VendorManifest};
use crate::xwpm::Layout;
use crate::{types, Adapter, BuildError};

/// The React runtime a direction vendors, pinned the way the examples pin it.
pub const REACT: &str = "19.3.1";
pub const VUE: &str = "3.5.44";
pub const SVELTE: &str = "5.57.2";
pub const HTMX: &str = "4.0.0";

#[non_exhaustive]
pub struct Direction {
  pub name: &'static str,
  /// Specifier and the file under the client route the import map names;
  /// a direction with no browser half maps nothing.
  pub maps: &'static [(&'static str, &'static str)],
  pub(crate) adapter: Option<&'static Adapter>,
  /// Package, version and subpath of every module the direction vendors.
  pub vendors: &'static [(&'static str, &'static str, Option<&'static str>)],
  /// Vendors the package as published, its module tree kept, rather than one
  /// bundle per subpath: Svelte's subpaths share one runtime by relative path.
  pub published: bool,
  /// File and line an existing application edits by hand, printed rather
  /// than written: the file is the application's and its structure is not
  /// the command's to guess. The scaffold writes them into its own files.
  pub edits: &'static [(&'static str, &'static str)],
}

pub const DIRECTIONS: &[Direction] = &[
  Direction {
    name: "react",
    maps: &[("@snapfire/fsr/client/react", "react.js"), ("@snapfire/fsr/template", "template.js")],
    adapter: Some(&crate::REACT),
    vendors: &[("react", REACT, None), ("react", REACT, Some("jsx-runtime")), ("react-dom", REACT, Some("client"))],
    published: false,
   
    edits: &[],
  },
  Direction { name: "vue", maps: &[("@snapfire/fsr/client/vue", "vue.js")], adapter: Some(&crate::VUE), vendors: &[("vue", VUE, None)], published: false, edits: &[] },
  Direction {
    name: "svelte",
    maps: &[("@snapfire/fsr/client/svelte", "svelte.js")],
    adapter: Some(&crate::SVELTE),
    vendors: &[
      ("svelte", SVELTE, None),
      ("svelte", SVELTE, Some("internal/client")),
      ("svelte", SVELTE, Some("internal/disclose-version")),
      ("svelte", SVELTE, Some("internal/flags/legacy")),
      ("svelte", SVELTE, Some("store")),
      ("svelte", SVELTE, Some("reactivity")),
      ("svelte", SVELTE, Some("legacy")),
      ("svelte", SVELTE, Some("transition")),
      ("svelte", SVELTE, Some("animate")),
      ("svelte", SVELTE, Some("easing")),
      ("svelte", SVELTE, Some("motion")),
      ("svelte", SVELTE, Some("events")),
      ("svelte", SVELTE, Some("attachments")),
    ],
    published: true,
    edits: &[],
  },
  Direction { name: "elements", maps: &[("@snapfire/fsr/client/elements", "elements.js")], adapter: None, vendors: &[], published: false, edits: &[] },
  Direction {
    name: "htmx",
    maps: &[("@snapfire/fsr/client/htmx", "htmx.js")],
    adapter: None,
    vendors: &[("htmx.org", HTMX, None)],
    published: false,
   
    edits: &[("src/main.ts", "import htmx from \"htmx.org\";"), ("src/main.ts", "import { bindHtmx } from \"@snapfire/fsr/client/htmx\";"), ("src/main.ts", "bindHtmx(htmx); after enableNavigation()")],
  },
  Direction { name: "tailwind", maps: &[], adapter: None, vendors: &[], published: false, edits: &[] },
  #[cfg(feature = "tera")]
  Direction { name: "tera", maps: &[], adapter: None, vendors: &[], published: false, edits: &[] },
  #[cfg(feature = "mdx")]
  Direction { name: "mdx", maps: &[], adapter: None, vendors: &[], published: false, edits: &[] },
];

/// The stylesheet `fsr use <app> mdx` writes for highlighted code.
pub const CODE_SHEET: &str = "code.css";

/// The theme highlighted code takes in a light scheme and in a dark one.
pub const CODE_THEMES: (&str, &str) = ("GitHub", "OneHalfDark");

const MDX_COMPONENTS: &str = "import type { MDXComponents } from \"mdx/types\";\n\n/** A second-level heading with a link to itself. */\nfunction Heading({ id, children }: { id?: string; children?: any }) {\n  return (\n    <h2 id={id}>\n      <a href={`#${id}`} aria-hidden=\"true\">#</a> {children}\n    </h2>\n  );\n}\n\nexport function useMDXComponents(components?: MDXComponents): MDXComponents {\n  return { h2: Heading, ...components };\n}\n";

const MDX_PAGE: &str = "export const meta = { title: \"Notes\", description: \"A page written in MDX\" };\n\n# Notes\n\nThis page is **markdown** with JSX in it, lowered like any other page: {1 + 1} is worked out at build.\n\n## A table\n\n| Feature | Where |\n| --- | --- |\n| Tables, task lists, footnotes | `[mdx] gfm` |\n| Heading ids and `toc` | `[mdx] heading_ids` |\n\n## Some code\n\n```ts title=\"routes/notes/page.mdx\" {1}\nexport const meta = { title: \"Notes\" };\nconst sum = 1 + 1;\n```\n";

/// The stylesheet `fsr use <app> tailwind` adds beside an application's own.
pub const TAILWIND_SHEET: &str = "tailwind.css";

/// Theme and utilities without the preflight reset, so the stylesheets an
/// application already has keep their look.
const TAILWIND_ADDED: &str = "/* Tailwind's theme and utilities. Its preflight reset is left out so the other stylesheets here keep their look; add `@import \"tailwindcss/preflight\" layer(base);` to take it. */\n@import \"tailwindcss/theme\" layer(theme.root);\n@import \"tailwindcss/utilities\" layer(utilities);\n";

const TAILWIND_PAGE: &str = "export default function Styled() {\n  return (\n    <main className=\"mx-auto max-w-xl p-8\">\n      <h1 className=\"text-3xl font-bold tracking-tight\">Styled with Tailwind</h1>\n      <p className=\"mt-2 text-gray-600 dark:text-gray-300\">Each class on this page is a utility the build generated because the page renders it.</p>\n      <a href=\"/\" className=\"mt-6 inline-block rounded-md bg-sky-600 px-4 py-2 text-white hover:bg-sky-700\">Back home</a>\n    </main>\n  );\n}\n";

const TAILWIND_THEME: &str = "@theme {\n  --color-sky-600: #0e7490;\n  --color-sky-700: #155e75;\n}\n";

impl Direction {
  /// Each import map line the direction writes: the specifier and its URL
  /// under the host's client route.
  pub fn lines(&self) -> Vec<(String, String)> {
    self.maps.iter().map(|(specifier, file)| ((*specifier).to_owned(), format!("{}/{file}", snapfire_fsr_host::client::ROUTE))).collect()
  }

  pub fn specs(&self) -> Vec<Spec> {
    self.vendors.iter().map(|(package, version, subpath)| Spec { package: (*package).to_owned(), version: (*version).to_owned(), subpath: subpath.map(str::to_owned) }).collect()
  }

  /// The files `--example` writes, relative to the app, and the edits that
  /// place the example, each a file and a line.
  fn example(&self) -> (Vec<(&'static str, &'static str)>, Vec<(&'static str, &'static str)>) {
    match self.name {
      "react" => (vec![("src/ui/Counter.tsx", REACT_COUNTER)], vec![("a page", "import Counter from \"@src/ui/Counter\";"), ("a page", "import { Island } from \"@snapfire/fsr/client/react\";"), ("a page", "<Island><Counter start={0} /></Island>")]),
      "vue" => (vec![("src/ui/Counter.vue", VUE_COUNTER)], vec![("a page", "import Counter from \"@src/ui/Counter.vue\";"), ("a page", "import { Island } from \"@snapfire/fsr/template\";"), ("a page", "<Island><Counter start={0} /></Island>")]),
      "elements" => (vec![("elements/hello-tag.tsx", ELEMENT_TEMPLATE), ("src/elements/hello-tag.ts", ELEMENT_CLASS)], vec![("src/main.ts", "import \"./elements/hello-tag.js\";"), ("a page", "<hello-tag name=\"world\" />")]),
      "htmx" => (vec![("routes/pulse/page.loader.ts", HTMX_LOADER), ("routes/pulse/page.tsx", HTMX_PAGE)], vec![("a page", "<div hx-get=\"/pulse?__fragment\" hx-trigger=\"every 5s\" hx-swap=\"innerHTML\" />")]),
      "tera" => (vec![("routes/hello/page.loader.ts", TERA_LOADER), ("routes/hello/page.tera", TERA_PAGE)], vec![("routes/layout.tera", "in place of routes/layout.tsx, which puts the shell in templates too")]),
      "tailwind" => (vec![("routes/styled/page.tsx", TAILWIND_PAGE), ("styles/themes/ocean.css", TAILWIND_THEME)], vec![("a page", "<a href=\"/styled\">Styled</a>"), ("routes/layout.tsx", "export const theme = \"ocean\"; to switch the document to styles/themes/ocean.css")]),
      "mdx" => (vec![("routes/notes/page.mdx", MDX_PAGE)], vec![("a page", "<a href=\"/notes\">Notes</a>")]),
      _ => (Vec::new(), Vec::new()),
    }
  }
}

/// `specs` with every module a framework's adapter needs added at the version asked for: `react`
/// or `react-dom` brings both and `react/jsx-runtime`, `svelte` its subpaths. Returns the specs
/// vendored one bundle each and, apart, those a published framework vendors as published.
pub fn with_companions(specs: Vec<Spec>) -> (Vec<Spec>, Vec<Spec>) {
  let mut plain: Vec<Spec> = Vec::new();
  let mut published: Vec<Spec> = Vec::new();
  for spec in specs {
    let framework = DIRECTIONS.iter().find(|d| d.adapter.is_some() && d.vendors.iter().any(|(package, _, _)| *package == spec.package));
    let (out, wanted) = match framework {
      Some(direction) => (
        if direction.published { &mut published } else { &mut plain },
        direction.vendors.iter().map(|(package, _, subpath)| Spec { package: (*package).to_owned(), version: spec.version.clone(), subpath: subpath.map(str::to_owned) }).collect(),
      ),
      None => (&mut plain, vec![spec]),
    };
    for spec in wanted {
      if !out.contains(&spec) {
        out.push(spec);
      }
    }
  }
  (plain, published)
}

pub fn find(name: &str) -> Result<&'static Direction, BuildError> {
  DIRECTIONS.iter().find(|d| d.name == name).ok_or_else(|| BuildError::Direction { name: name.to_owned(), known: DIRECTIONS.iter().map(|d| d.name).collect::<Vec<_>>().join(", ") })
}

/// The direction whose adapter is `module`, for a message that names the command.
pub(crate) fn for_adapter(module: &str) -> Option<&'static Direction> {
  DIRECTIONS.iter().find(|d| d.adapter.is_some_and(|a| a.module == module))
}

#[non_exhaustive]
pub struct UseOptions {
  /// Vendors and fetches declarations, both of which reach the network.
  pub fetch: bool,
  /// Writes one example per direction, refusing a file that is already there.
  pub example: bool,
}

impl UseOptions {
  pub fn fetch(mut self, fetch: bool) -> Self {
    self.fetch = fetch;
    self
  }

  pub fn example(mut self, example: bool) -> Self {
    self.example = example;
    self
  }
}

impl Default for UseOptions {
  fn default() -> Self {
    Self { fetch: true, example: false }
  }
}

#[derive(Debug, Default)]
#[non_exhaustive]
pub struct Adopted {
  /// Specifier and the URL written into the import map.
  pub mapped: Vec<(String, String)>,
  /// Specifiers the map already carried at that URL.
  pub present: Vec<String>,
  pub vendored: Vec<(String, String, usize)>,
  /// Specifiers the vendor manifest already records at the pinned version.
  pub kept: Vec<String>,
  pub from_shell: Vec<(String, String)>,
  pub delegated: Vec<String>,
  pub typed: Vec<(String, String, String)>,
  /// The configuration file and the TypeScript version pinned in it.
  pub pinned: Option<(std::path::PathBuf, String)>,
  pub written: Vec<PathBuf>,
  /// What the command could not settle, each a line the caller prints.
  pub notes: Vec<String>,
  /// File and line the application edits by hand, in order.
  pub edits: Vec<(String, String)>,
  /// What to type next, in order.
  pub next: Vec<String>,
}

/// Adopts every direction in `names`, in order. Running it again on the same
/// application changes nothing: a map line already there is reported as
/// present and a package already recorded at the pinned version is kept.
pub fn adopt(app: &Path, names: &[String], options: UseOptions) -> Result<Adopted, BuildError> {
  let mut directions: Vec<&'static Direction> = Vec::new();
  // A direction named `svelte@5.60.0` vendors that version in place of its own.
  let mut versions: std::collections::HashMap<&'static str, String> = std::collections::HashMap::new();
  for name in names {
    let (bare, version) = match name.split_once('@') {
      Some((bare, version)) if types::range::Version::parse(version).is_some() => (bare, Some(version.to_owned())),
      Some(_) => return Err(BuildError::Spec(name.clone())),
      None => (name.as_str(), None),
    };
    let direction = find(bare)?;
    if let Some(version) = version {
      versions.insert(direction.name, version);
    }
    if !directions.iter().any(|d| d.name == direction.name) {
      directions.push(direction);
    }
  }
  let layout = Layout::of(app)?;
  let mut adopted = Adopted::default();

  if options.example {
    for direction in &directions {
      for (path, _) in direction.example().0 {
        if app.join(path).exists() {
          return Err(BuildError::ExampleExists(app.join(path)));
        }
      }
    }
  }

  let manifest = VendorManifest::read(app, &layout)?;
  let shell = match crate::site_beside(app).and_then(|site| site.shell) {
    Some(path) => Some(crate::ShellContract::read(&path)?),
    None => None,
  };
  let mut wanted: Vec<Spec> = Vec::new();
  let mut published: Vec<Spec> = Vec::new();
  let mut served: Vec<Spec> = Vec::new();
  for direction in &directions {
    write_map(app, &layout, direction, &mut adopted)?;
    for mut spec in direction.specs() {
      if let Some(version) = versions.get(direction.name) {
        spec.version = version.clone();
      }
      match manifest.packages.get(&spec.package) {
        Some(recorded) if recorded.version == spec.version => adopted.kept.push(spec.specifier()),
        Some(recorded) => {
          return Err(BuildError::DirectionPinned { direction: direction.name.to_owned(), package: spec.package.clone(), recorded: recorded.version.clone(), wanted: spec.version.clone(), manifest: format!("`{}`", app.join(&layout.vendor).join(vendor::VENDOR_MANIFEST).display()) });
        }
        None if shell.as_ref().is_some_and(|contract| contract.imports.contains_key(&spec.specifier())) => served.push(spec),
        None if direction.published => published.push(spec),
        None => wanted.push(spec),
      }
    }
  }
  // A package the site's shell serves needs no fetch: `add` writes the
  // shell's URL into the map from the contract alone.
  if !served.is_empty() {
    let report = vendor::add(app, &served, &[])?;
    adopted.from_shell = report.from_shell;
  }

  if options.example {
    for direction in &directions {
      let (files, placement) = direction.example();
      for (path, contents) in files {
        let path = app.join(path);
        if let Some(parent) = path.parent() {
          std::fs::create_dir_all(parent).map_err(|e| BuildError::Io(parent.to_path_buf(), e))?;
        }
        std::fs::write(&path, contents).map_err(|e| BuildError::Io(path.clone(), e))?;
        adopted.written.push(path);
      }
      adopted.edits.extend(placement.iter().map(|(file, line)| ((*file).to_owned(), (*line).to_owned())));
    }
  }

  let line = |flag: &str, specs: &[Spec]| match specs.is_empty() {
    true => None,
    false => Some(format!("fsr add {}{flag} {}", app.display(), specs.iter().map(|s| format!("{}@{}{}", s.package, s.version, s.subpath.as_ref().map(|sub| format!("/{sub}")).unwrap_or_default())).collect::<Vec<_>>().join(" "))),
  };
  let add = line("", &wanted);
  let add_published = line(" --published", &published);
  if options.fetch {
    if !wanted.is_empty() {
      match vendor::add(app, &wanted, &[]) {
        Ok(report) => {
          adopted.vendored = report.added;
          adopted.from_shell.extend(report.from_shell);
          adopted.delegated = report.delegated;
        }
        Err(e) => adopted.notes.push(format!("vendoring failed ({e}); run `{}`", add.as_deref().unwrap_or_default())),
      }
    }
    if !published.is_empty() {
      match crate::published::add_published(app, &published) {
        Ok(report) => {
          adopted.vendored.extend(report.added);
          adopted.delegated.extend(report.delegated);
        }
        Err(e) => adopted.notes.push(format!("vendoring failed ({e}); run `{}`", add_published.as_deref().unwrap_or_default())),
      }
    }
    match types::fetch(app, false) {
      Ok(report) => {
        adopted.typed = report.fetched;
        adopted.pinned = report.pinned;
        match crate::new::generate(app) {
          Ok(written) => adopted.written.extend(written),
          Err(e) => adopted.notes.push(format!("writing the generated modules failed ({e}); run `fsr build {}`", app.display())),
        }
      }
      Err(e) => adopted.notes.push(format!("fetching types failed ({e}); run `fsr types {}`", app.display())),
    }
  } else {
    adopted.next.extend(add);
    adopted.next.extend(add_published);
    adopted.next.push(format!("fsr types {}", app.display()));
    adopted.next.push(format!("fsr build {}", app.display()));
  }

  for direction in &directions {
    adopted.edits.extend(direction.edits.iter().map(|(file, line)| ((*file).to_owned(), (*line).to_owned())));
    if direction.name == "tailwind" {
      adopt_tailwind(app, &mut adopted)?;
    }
    if direction.name == "mdx" {
      adopt_mdx(app, &mut adopted)?;
    }
  }
  Ok(adopted)
}

/// MDX for an application: `mdx-components.tsx` at its root unless it has
/// one, and with highlighting built in, `code.css` beside its stylesheets
/// colouring code in a light and a dark theme by the document's scheme.
fn adopt_mdx(app: &Path, adopted: &mut Adopted) -> Result<(), BuildError> {
  let components = app.join(crate::mdx::COMPONENTS);
  match components.is_file() {
    true => adopted.present.push(crate::mdx::COMPONENTS.to_owned()),
    false => {
      std::fs::write(&components, MDX_COMPONENTS).map_err(|e| BuildError::Io(components.clone(), e))?;
      adopted.written.push(components);
    }
  }
  if let Some(css) = code_sheet() {
    let styles = crate::assets::Sections::of(app).dirs.styles;
    let dir = app.join(&styles);
    let path = dir.join(CODE_SHEET);
    match path.is_file() {
      true => adopted.present.push(format!("{styles}/{CODE_SHEET}")),
      false => {
        std::fs::create_dir_all(&dir).map_err(|e| BuildError::Io(dir.clone(), e))?;
        std::fs::write(&path, css).map_err(|e| BuildError::Io(path.clone(), e))?;
        adopted.written.push(path);
        let listed = crate::config_beside(app).is_some_and(|config| config.document.styles.is_some() && !config.inferred.iter().any(|row| row.starts_with("document.styles from")));
        if listed {
          adopted.edits.push(("config/app.toml".to_owned(), format!("[document] styles gains the compiled {styles}/{CODE_SHEET}")));
        }
      }
    }
  }
  Ok(())
}

/// The stylesheet for highlighted code: [`CODE_THEMES`] light under
/// `data-sf-scheme="light"` and dark under `data-sf-scheme="dark"`, each also
/// under the browser's preference when the document names no scheme. Neither
/// theme is unscoped, so a scope one theme colours and the other does not never
/// keeps the first theme's colour in the second scheme. `None` without the
/// highlighter.
pub fn code_sheet() -> Option<String> {
  #[cfg(feature = "mdx-highlight")]
  {
    use snapfire_fsr_mdx::highlight::theme_css;
    let (light, dark) = CODE_THEMES;
    let mut out = format!("/* Code highlighted at build: {light} in a light scheme, {dark} in a dark one. Each class is a TextMate scope; regenerate with another pair of themes from snapfire_fsr_mdx::highlight::themes(). */\n\n");
    out.push_str(&theme_css(light, "[data-sf-scheme=\"light\"]")?);
    out.push('\n');
    out.push_str(&theme_css(dark, "[data-sf-scheme=\"dark\"]")?);
    for (preference, theme) in [("light", light), ("dark", dark)] {
      out.push_str(&format!("\n@media (prefers-color-scheme: {preference}) {{\n"));
      for line in theme_css(theme, ":root:not([data-sf-scheme])")?.lines() {
        out.push_str("  ");
        out.push_str(line);
        out.push('\n');
      }
      out.push_str("}\n");
    }
    out.push_str("\n[data-rehype-pretty-code-figure] { margin: 1rem 0; }\n[data-rehype-pretty-code-title] { font-family: ui-monospace, monospace; font-size: 0.85em; opacity: 0.8; padding: 0.25rem 0; }\n.hl-code { padding: 0.75rem 1rem; overflow-x: auto; border-radius: 6px; }\n.hl-code code { background: none; padding: 0; border-radius: 0; font-size: inherit; color: inherit; }\n.hl-code .hl-punctuation.hl-definition.hl-prompt { opacity: 0.6; user-select: none; }\n.hl-code .hl-meta.hl-output { opacity: 0.75; }\n.hl-code [data-line] { display: inline-block; min-width: 100%; }\n.hl-code [data-highlighted-line] { background: rgba(127, 127, 127, 0.18); }\n");
    Some(out)
  }
  #[cfg(not(feature = "mdx-highlight"))]
  {
    None
  }
}

/// Tailwind for an application: nothing when a stylesheet under the styles
/// directory already imports it, else `tailwind.css` beside the others, which
/// the document links with them. An application that lists `[document]
/// styles` itself is told to list the new sheet.
fn adopt_tailwind(app: &Path, adopted: &mut Adopted) -> Result<(), BuildError> {
  let dir = app.join(crate::assets::Sections::of(app).dirs.styles);
  let sheets: Vec<PathBuf> = std::fs::read_dir(&dir).into_iter().flatten().flatten().map(|e| e.path()).filter(|p| p.extension().is_some_and(|x| x == "css")).collect();
  if let Some(sheet) = sheets.iter().find(|p| std::fs::read_to_string(p).is_ok_and(|css| imports_tailwind(&css))) {
    adopted.present.push(format!("tailwind in {}", sheet.strip_prefix(app).unwrap_or(sheet).display()));
    return Ok(());
  }
  std::fs::create_dir_all(&dir).map_err(|e| BuildError::Io(dir.clone(), e))?;
  let path = dir.join(TAILWIND_SHEET);
  std::fs::write(&path, TAILWIND_ADDED).map_err(|e| BuildError::Io(path.clone(), e))?;
  adopted.written.push(path);
  let listed = crate::config_beside(app).is_some_and(|config| config.document.styles.is_some() && !config.inferred.iter().any(|row| row.starts_with("document.styles from")));
  if listed {
    adopted.edits.push(("config/app.toml".to_owned(), format!("[document] styles gains the compiled {}/{TAILWIND_SHEET}", crate::assets::Sections::of(app).dirs.styles)));
  }
  Ok(())
}

/// Whether a stylesheet imports Tailwind or one of its parts.
fn imports_tailwind(css: &str) -> bool {
  css.lines().any(|line| {
    let line = line.trim_start();
    line.starts_with("@import") && (line.contains("\"tailwindcss") || line.contains("'tailwindcss"))
  })
}

/// Writes the direction's lines into the map. A line already at the host's
/// URL is left alone; one pointing anywhere else is refused, since the module
/// the build registers is the one the host serves.
fn write_map(app: &Path, layout: &Layout, direction: &Direction, adopted: &mut Adopted) -> Result<(), BuildError> {
  let lines = direction.lines();
  if lines.is_empty() {
    return Ok(());
  }
  let mut map = vendor::read_import_map(app, layout)?;
  let mut imports = map.get("imports").and_then(|v| v.as_object()).cloned().unwrap_or_default();
  let mut written = false;
  for (specifier, want) in lines {
    match imports.get(&specifier).and_then(|v| v.as_str()) {
      Some(found) if found == want => {
        adopted.present.push(specifier);
        continue;
      }
      Some(found) => {
        return Err(BuildError::AdapterUrl { map: format!("`{}`", app.join(&layout.importmap).display()), specifier, found: found.to_owned(), want });
      }
      None => {}
    }
    imports.insert(specifier.clone(), serde_json::Value::String(want.clone()));
    adopted.mapped.push((specifier, want));
    written = true;
  }
  if written {
    map.insert("imports".to_owned(), serde_json::Value::Object(imports));
    vendor::write_import_map(app, layout, &map)?;
  }
  Ok(())
}

const REACT_COUNTER: &str = r#"import { useState } from "react";

export default function Counter({ start = 0 }: { start?: number }) {
  const [count, setCount] = useState(start);
  return (
    <button type="button" onClick={() => setCount(count + 1)}>
      Clicked {count} times
    </button>
  );
}
"#;

const VUE_COUNTER: &str = r#"<script setup lang="ts">
import { ref } from "vue";

const props = defineProps<{ start?: number }>();
const count = ref(props.start ?? 0);
</script>

<template>
  <button type="button" @click="count++">Clicked {{ count }} times</button>
</template>
"#;

const ELEMENT_TEMPLATE: &str = r#"export default function HelloTag({ name }: { name: string }) {
  return (
    <>
      <style>{":host { display: inline-block; padding: 4px 8px; border: 1px solid currentColor; border-radius: 4px; }"}</style>
      <span>
        Hello, <b>{name}</b>
      </span>
      <button type="button">wave</button>
    </>
  );
}
"#;

const ELEMENT_CLASS: &str = r#"import { shadowOf } from "@snapfire/fsr/client/elements";

class HelloTag extends HTMLElement {
  connectedCallback(): void {
    const button = shadowOf(this)?.querySelector("button");
    button?.addEventListener("click", () => {
      button.textContent = "waved";
    });
  }
}

customElements.define("hello-tag", HelloTag);
"#;

const HTMX_LOADER: &str = r#"import type { Ctx } from "@snapfire/fsr/host";

export async function load(_ctx: Ctx<"/pulse">) {
  return { at: Date.now() };
}
"#;

const HTMX_PAGE: &str = r#"import type { PulseProps } from "@generated/client";

export default function Pulse({ at }: PulseProps) {
  return <p className="pulse">Rendered by the server at tick {`${at}`}.</p>;
}
"#;

const TERA_LOADER: &str = r#"import type { Ctx } from "@snapfire/fsr/host";

export async function load(_ctx: Ctx<"/hello">) {
  return { greeting: "Hello from a template" };
}
"#;

const TERA_PAGE: &str = r#"<section class="hello">
  <h1>{{ greeting }}</h1>
  <p>This page is a Tera template beside its loader; the loader's return is its context.</p>
</section>
"#;
