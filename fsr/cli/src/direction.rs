//! `fsr use`: gives an existing application a client direction. The adapter's
//! line goes into the import map, the framework the direction pins is vendored
//! and the declarations and generated files follow, so a bare application takes
//! on React, Vue, custom elements or htmx at any point rather than at `fsr new`.

use std::path::{Path, PathBuf};

use crate::vendor::{self, Spec, VendorManifest};
use crate::xwpm::Layout;
use crate::{types, Adapter, BuildError};

/// The React runtime a direction vendors, pinned the way the examples pin it.
pub const REACT: &str = "18.3.1";
pub const VUE: &str = "3.5.13";
pub const HTMX: &str = "2.0.10";

pub struct Direction {
  pub name: &'static str,
  /// Specifier and the file under the client route the import map names;
  /// a direction with no browser half maps nothing.
  pub maps: &'static [(&'static str, &'static str)],
  pub(crate) adapter: Option<&'static Adapter>,
  /// Package, version and subpath of every module the direction vendors.
  pub vendors: &'static [(&'static str, &'static str, Option<&'static str>)],
  /// File and line an existing application edits by hand, printed rather
  /// than written: the file is the application's and its structure is not
  /// the command's to guess. The scaffold writes them into its own files.
  pub edits: &'static [(&'static str, &'static str)],
}

pub const DIRECTIONS: &[Direction] = &[
  Direction {
    name: "react",
    maps: &[("@snapfire/fsr-client/react", "react.js"), ("@snapfire/fsr-authoring/template", "template.js")],
    adapter: Some(&crate::REACT),
    vendors: &[("react", REACT, None), ("react", REACT, Some("jsx-runtime")), ("react-dom", REACT, Some("client"))],
    edits: &[],
  },
  Direction { name: "vue", maps: &[("@snapfire/fsr-client/vue", "vue.js")], adapter: Some(&crate::VUE), vendors: &[("vue", VUE, None)], edits: &[] },
  Direction { name: "elements", maps: &[("@snapfire/fsr-client/elements", "elements.js")], adapter: None, vendors: &[], edits: &[] },
  Direction {
    name: "htmx",
    maps: &[("@snapfire/fsr-client/htmx", "htmx.js")],
    adapter: None,
    vendors: &[("htmx.org", HTMX, None)],
    edits: &[("src/main.ts", "import htmx from \"htmx.org\";"), ("src/main.ts", "import { bindHtmx } from \"@snapfire/fsr-client/htmx\";"), ("src/main.ts", "bindHtmx(htmx); after enableNavigation()")],
  },
  #[cfg(feature = "tera")]
  Direction { name: "tera", maps: &[], adapter: None, vendors: &[], edits: &[] },
];

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
      "react" => (vec![("src/ui/Counter.tsx", REACT_COUNTER)], vec![("a page", "import Counter from \"@src/ui/Counter\";"), ("a page", "import { Island } from \"@snapfire/fsr-client/react\";"), ("a page", "<Island><Counter start={0} /></Island>")]),
      "vue" => (vec![("src/ui/Counter.vue", VUE_COUNTER)], vec![("a page", "import Counter from \"@src/ui/Counter.vue\";"), ("a page", "import { Island } from \"@snapfire/fsr-authoring/template\";"), ("a page", "<Island><Counter start={0} /></Island>")]),
      "elements" => (vec![("elements/hello-tag.tsx", ELEMENT_TEMPLATE), ("src/elements/hello-tag.ts", ELEMENT_CLASS)], vec![("src/main.ts", "import \"./elements/hello-tag.js\";"), ("a page", "<hello-tag name=\"world\" />")]),
      "htmx" => (vec![("routes/pulse/page.loader.ts", HTMX_LOADER), ("routes/pulse/page.tsx", HTMX_PAGE)], vec![("a page", "<div hx-get=\"/pulse?__fragment\" hx-trigger=\"every 5s\" hx-swap=\"innerHTML\" />")]),
      "tera" => (vec![("routes/hello/page.loader.ts", TERA_LOADER), ("routes/hello/page.tera", TERA_PAGE)], vec![("routes/layout.tera", "in place of routes/layout.tsx, which puts the shell in templates too")]),
      _ => (Vec::new(), Vec::new()),
    }
  }
}

pub fn find(name: &str) -> Result<&'static Direction, BuildError> {
  DIRECTIONS.iter().find(|d| d.name == name).ok_or_else(|| BuildError::Direction { name: name.to_owned(), known: DIRECTIONS.iter().map(|d| d.name).collect::<Vec<_>>().join(", ") })
}

/// The direction whose adapter is `module`, for a message that names the command.
pub(crate) fn for_adapter(module: &str) -> Option<&'static Direction> {
  DIRECTIONS.iter().find(|d| d.adapter.is_some_and(|a| a.module == module))
}

pub struct UseOptions {
  /// Vendors and fetches declarations, both of which reach the network.
  pub fetch: bool,
  /// Writes one example per direction, refusing a file that is already there.
  pub example: bool,
}

impl Default for UseOptions {
  fn default() -> Self {
    Self { fetch: true, example: false }
  }
}

#[derive(Debug, Default)]
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
  for name in names {
    let direction = find(name)?;
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
  let mut wanted: Vec<Spec> = Vec::new();
  for direction in &directions {
    write_map(app, &layout, direction, &mut adopted)?;
    for spec in direction.specs() {
      match manifest.packages.get(&spec.package) {
        Some(recorded) if recorded.version == spec.version => adopted.kept.push(spec.specifier()),
        Some(recorded) => {
          return Err(BuildError::DirectionPinned { direction: direction.name.to_owned(), package: spec.package.clone(), recorded: recorded.version.clone(), wanted: spec.version.clone(), manifest: format!("`{}`", app.join(&layout.vendor).join(vendor::VENDOR_MANIFEST).display()) });
        }
        None => wanted.push(spec),
      }
    }
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

  let add = match wanted.is_empty() {
    true => None,
    false => Some(format!("fsr add {} {}", app.display(), wanted.iter().map(|s| format!("{}@{}{}", s.package, s.version, s.subpath.as_ref().map(|sub| format!("/{sub}")).unwrap_or_default())).collect::<Vec<_>>().join(" "))),
  };
  if options.fetch {
    if !wanted.is_empty() {
      match vendor::add(app, &wanted, &[]) {
        Ok(report) => {
          adopted.vendored = report.added;
          adopted.from_shell = report.from_shell;
          adopted.delegated = report.delegated;
        }
        Err(e) => adopted.notes.push(format!("vendoring failed ({e}); run `{}`", add.as_deref().unwrap_or_default())),
      }
    }
    match types::fetch(app, false) {
      Ok(report) => {
        adopted.typed = report.fetched;
        match crate::new::generate(app) {
          Ok(written) => adopted.written.extend(written),
          Err(e) => adopted.notes.push(format!("writing the generated modules failed ({e}); run `fsr build {}`", app.display())),
        }
      }
      Err(e) => adopted.notes.push(format!("fetching types failed ({e}); run `fsr types {}`", app.display())),
    }
  } else {
    adopted.next.extend(add);
    adopted.next.push(format!("fsr types {}", app.display()));
    adopted.next.push(format!("fsr build {}", app.display()));
  }

  for direction in &directions {
    adopted.edits.extend(direction.edits.iter().map(|(file, line)| ((*file).to_owned(), (*line).to_owned())));
  }
  Ok(adopted)
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

const ELEMENT_CLASS: &str = r#"import { shadowOf } from "@snapfire/fsr-client/elements";

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

const HTMX_LOADER: &str = r#"import type { Ctx } from "@snapfire/fsr";
import { time } from "@snapfire/fsr-client/std";

export async function load(_ctx: Ctx<"/pulse">) {
  return { at: time.now() };
}
"#;

const HTMX_PAGE: &str = r#"import type { PulseProps } from "@generated/client";

export default function Pulse({ at }: PulseProps) {
  return <p className="pulse">Rendered by the server at tick {`${at}`}.</p>;
}
"#;

const TERA_LOADER: &str = r#"import type { Ctx } from "@snapfire/fsr";

export async function load(_ctx: Ctx<"/hello">) {
  return { greeting: "Hello from a template" };
}
"#;

const TERA_PAGE: &str = r#"<section class="hello">
  <h1>{{ greeting }}</h1>
  <p>This page is a Tera template beside its loader; the loader's return is its context.</p>
</section>
"#;
