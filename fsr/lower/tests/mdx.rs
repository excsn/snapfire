//! MDX and markdown modules lowered through the module the transform writes:
//! a page, an imported file, a template `import()` choosing among files and a
//! residue that names the line of the `.mdx` the author wrote.

use std::rc::Rc;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;

use snapfire_fsr_core::{Value, ValueMap};
use snapfire_fsr_ir::render::{prepare, Components};
use snapfire_fsr_ir::Interpreter;
use snapfire_fsr_lower::component::{ComponentSet, Override, Transformed};
use snapfire_fsr_lower::{Origin, OriginSegment};
use snapfire_fsr_mdx::{Format, Mapped, Options, Target};

static NEXT: AtomicU32 = AtomicU32::new(0);

fn app(files: &[(&str, &str)]) -> std::path::PathBuf {
  let n = NEXT.fetch_add(1, Ordering::Relaxed);
  let dir = std::env::temp_dir().join(format!("fsr_mdx_{}_{n}", std::process::id()));
  let _ = std::fs::remove_dir_all(&dir);
  for (name, source) in files {
    let path = dir.join(name);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, source).unwrap();
  }
  dir
}

fn transform(mapped: Vec<Mapped>) -> Rc<snapfire_fsr_lower::component::Transform> {
  Rc::new(move |file: &str, source: &str, overrides: &[Override]| {
    let mut options = Options::new(Format::of(file).expect("an mdx or md file"), Target::Lowered);
    options.highlight = true;
    options.frontmatter = true;
    options.mapped = mapped.clone();
    for o in overrides {
      options.mapped.retain(|m| m.key != o.key);
      options.mapped.push(Mapped { key: o.key.clone(), specifier: o.specifier.clone(), export: o.export.clone() });
    }
    let converted = snapfire_fsr_mdx::convert(source, &options).map_err(|e| e.to_string())?;
    let segments = converted.segments.iter().map(|s| OriginSegment { generated: s.generated, len: s.len, original: s.original, verbatim: s.verbatim }).collect();
    Ok(Transformed { source: converted.tsx, origin: Some(Origin { source: source.to_owned(), segments }) })
  })
}

fn set(files: &[(&str, &str)], mapped: Vec<Mapped>) -> ComponentSet {
  ComponentSet::new(&app(files)).with_transform(transform(mapped))
}

fn render(set: &ComponentSet, module: &str, props: &[(&str, Value)]) -> String {
  let library: Components = set.components.iter().map(|(module, component)| (module.clone(), Arc::new(prepare(component)))).collect();
  let props: ValueMap = props.iter().map(|(name, value)| ((*name).to_owned(), value.clone())).collect();
  Interpreter::default().render_module(module, &library[module], &props, &library).map(|rendered| rendered.html).unwrap_or_else(|fail| panic!("{module}: {}", fail.message))
}

const CARD: &str = "export function Card({ tone, children }: { tone: string; children?: unknown }) {\n  return <section className={`card card-${tone}`}>{children}</section>;\n}\n";

#[test]
fn an_mdx_page_lowers_markdown_jsx_and_expressions() {
  let page = "import { Card } from \"../src/card\"\nexport const meta = { title: \"Hello\" }\n\n# {meta.title}\n\nSome *text* with `code`.\n\n<Card tone=\"ok\">\n  Inside **the card**, {1 + 2}.\n</Card>\n";
  let mut set = set(&[("routes/page.mdx", page), ("src/card.tsx", CARD)], Vec::new());
  set.lower("routes/page.mdx#default").expect("lowers");
  assert_eq!(
    render(&set, "routes/page.mdx#default", &[]),
    "<h1>Hello</h1><p>Some <em>text</em> with <code>code</code>.</p><section class=\"card card-ok\"><p>Inside <strong>the card</strong>, 3.</p></section>"
  );
}

#[test]
fn a_page_places_an_imported_mdx_file_and_reads_its_exports() {
  let post = "export const meta = { title: \"First post\" }\n\n## Body\n\nText.\n";
  let page = "import Post, { meta, toc } from \"../content/first.mdx\";\nexport default function Page() {\n  return <article><h1>{meta.title}</h1><nav>{toc.map((e) => <a key={e.id} href={`#${e.id}`}>{e.text}</a>)}</nav><Post /></article>;\n}\n";
  let mut set = set(&[("routes/page.tsx", page), ("content/first.mdx", post)], Vec::new());
  set.lower("routes/page.tsx#default").expect("lowers");
  assert_eq!(render(&set, "routes/page.tsx#default", &[]), "<article><h1>First post</h1><nav><a href=\"#body\">Body</a></nav><h2 id=\"body\">Body</h2><p>Text.</p></article>");
}

#[test]
fn a_template_import_places_the_file_its_key_names() {
  let page = "export default async function Page({ slug }: { slug: string }) {\n  const { default: Body, meta } = await import(`../content/posts/${slug}.mdx`);\n  return <article><h1>{meta.title}</h1><Body /></article>;\n}\n";
  let files = [
    ("routes/page.tsx", page),
    ("content/posts/alpha.mdx", "export const meta = { title: \"Alpha\" }\n\nThe first.\n"),
    ("content/posts/beta.mdx", "export const meta = { title: \"Beta\" }\n\nThe second.\n"),
    ("content/posts/notes.txt", "not a post"),
  ];
  let mut set = set(&files, Vec::new());
  set.lower("routes/page.tsx#default").expect("lowers");
  assert_eq!(render(&set, "routes/page.tsx#default", &[("slug", Value::from("alpha"))]), "<article><h1>Alpha</h1><p>The first.</p></article>");
  assert_eq!(render(&set, "routes/page.tsx#default", &[("slug", Value::from("beta"))]), "<article><h1>Beta</h1><p>The second.</p></article>");
  assert_eq!(render(&set, "routes/page.tsx#default", &[("slug", Value::from("gamma"))]), "<article><h1></h1></article>");
}

#[test]
fn an_async_page_awaiting_anything_but_an_import_is_still_refused() {
  let page = "export default async function Page() {\n  const data = await fetch(\"/x\");\n  return <p>{String(data)}</p>;\n}\n";
  let mut set = set(&[("routes/page.tsx", page)], Vec::new());
  let refused = match set.lower("routes/page.tsx#default") {
    Err(e) => e.to_string(),
    Ok(()) => set.browser_only.iter().map(|(_, r)| r.message.clone()).collect(),
  };
  assert!(refused.contains("an `async` component"), "{refused}");
}

#[test]
fn a_residue_in_mdx_names_the_line_the_author_wrote() {
  let page = "# Title\n\nSome text.\n\nNow {window.location.href} here.\n";
  let mut set = set(&[("routes/page.mdx", page)], Vec::new());
  let residue = match set.lower("routes/page.mdx#default") {
    Err(snapfire_fsr_lower::LowerError::Residue(r)) => r,
    Err(other) => panic!("{other}"),
    Ok(()) => set.browser_only.iter().find(|(m, _)| m == "routes/page.mdx#default").map(|(_, r)| r.clone()).expect("a residue"),
  };
  assert_eq!((residue.file.as_str(), residue.line, residue.column), ("routes/page.mdx", 5, 6), "{residue:?}");
}

#[test]
fn a_parse_error_in_mdx_names_its_line() {
  let mut set = set(&[("routes/page.mdx", "# Title\n\n<Card>\n")], Vec::new());
  let error = set.lower("routes/page.mdx#default").expect_err("refused").to_string();
  assert!(error.contains("routes/page.mdx") && error.contains("3:"), "{error}");
}

#[test]
fn a_static_meta_object_lowers_as_the_routes_meta() {
  let mut set = set(&[("routes/page.mdx", "export const meta = { title: \"Static\", description: `a ${\"b\"}` }\n\n# x\n")], Vec::new());
  let meta = set.lower_meta("routes/page.mdx").expect("lowers").expect("a meta");
  let [snapfire_fsr_ir::ast::Stmt::Return(expr)] = meta.as_slice() else { panic!("one return: {meta:?}") };
  let value = Interpreter::default().compute(&snapfire_fsr_runtime::RequestCtx::anonymous(Default::default()), expr, &ValueMap::default()).expect("computes");
  let shown = format!("{value:?}");
  assert!(shown.contains("Static") && shown.contains("a b"), "{shown}");
}

#[test]
fn a_mapped_element_is_placed_as_its_component() {
  let components = "export function __mdx_h2({ children, id }: { children?: unknown; id?: string }) {\n  return <h2 className=\"fancy\" id={id}><a href={`#${id}`}>#</a> {children}</h2>;\n}\n";
  let mapped = vec![Mapped { key: "h2".into(), specifier: "../mdx-components".into(), export: "__mdx_h2".into() }];
  let mut set = set(&[("routes/page.mdx", "## Setup\n\nText.\n"), ("mdx-components.tsx", components)], mapped);
  set.lower("routes/page.mdx#default").expect("lowers");
  assert_eq!(render(&set, "routes/page.mdx#default", &[]), "<h2 class=\"fancy\" id=\"setup\"><a href=\"#setup\">#</a> Setup</h2><p>Text.</p>");
}

#[test]
fn a_markdown_file_lowers_as_mdx_does() {
  let mut set = set(&[("routes/page.md", "---\ntitle: Notes\n---\n\n# Notes\n\n| a | b |\n| - | - |\n| 1 | 2 |\n")], Vec::new());
  set.lower("routes/page.md#default").expect("lowers");
  assert_eq!(render(&set, "routes/page.md#default", &[]), "<h1 id=\"notes\">Notes</h1><table><thead><tr><th>a</th><th>b</th></tr></thead><tbody><tr><td>1</td><td>2</td></tr></tbody></table>");
  assert!(set.lower_meta("routes/page.md").expect("lowers").is_some());
}

#[test]
fn without_a_transform_an_mdx_module_names_the_feature() {
  let mut set = ComponentSet::new(&app(&[("routes/page.mdx", "# x\n")]));
  let error = set.lower("routes/page.mdx#default").expect_err("refused").to_string();
  assert!(error.contains("`mdx` feature"), "{error}");
}

#[test]
fn a_components_prop_places_the_file_with_those_elements_replaced() {
  let post = "## Setup\n\nText with [a link](/x).\n";
  let page = "import Post from \"../content/post.mdx\";\nfunction Big({ children, id }: { children?: unknown; id?: string }) {\n  return <h2 className=\"big\" id={id}>{children}</h2>;\n}\nexport default function Page() {\n  return <main><Post components={{ h2: Big }} /><Post /></main>;\n}\n";
  let mut set = set(&[("routes/page.tsx", page), ("content/post.mdx", post)], Vec::new());
  set.lower("routes/page.tsx#default").expect("lowers");
  assert_eq!(render(&set, "routes/page.tsx#default", &[]), "<main><h2 class=\"big\" id=\"setup\">Setup</h2><p>Text with <a href=\"/x\">a link</a>.</p><h2 id=\"setup\">Setup</h2><p>Text with <a href=\"/x\">a link</a>.</p></main>");
}

#[test]
fn a_components_prop_applies_to_every_file_a_template_import_can_place() {
  let page = "import { Quote } from \"../src/quote\";\nexport default async function Page({ slug }: { slug: string }) {\n  const { default: Body } = await import(`../content/${slug}.mdx`);\n  return <Body components={{ blockquote: Quote }} />;\n}\n";
  let quote = "export function Quote({ children }: { children?: unknown }) {\n  return <aside>{children}</aside>;\n}\n";
  let mut set = set(&[("routes/page.tsx", page), ("src/quote.tsx", quote), ("content/a.mdx", "> said a\n"), ("content/b.mdx", "> said b\n")], Vec::new());
  set.lower("routes/page.tsx#default").expect("lowers");
  assert_eq!(render(&set, "routes/page.tsx#default", &[("slug", Value::from("b"))]), "<aside><p>said b</p></aside>");
}

#[test]
fn a_components_entry_the_build_cannot_follow_is_named() {
  let page = "import Post from \"../content/post.mdx\";\nexport default function Page({ pick }: { pick: unknown }) {\n  return <Post components={{ h2: pick }} />;\n}\n";
  let mut set = set(&[("routes/page.tsx", page), ("content/post.mdx", "## x\n")], Vec::new());
  let error = match set.lower("routes/page.tsx#default") {
    Err(e) => e.to_string(),
    Ok(()) => set.browser_only.iter().map(|(_, r)| r.message.clone()).collect(),
  };
  assert!(error.contains("`pick` is not a component this file declares or imports"), "{error}");
}
