//! `.mdx` and `.md` in a build: a page route and its `meta`, the component
//! map, the collections module, the browser's copy of each file, highlight
//! classes kept out of Tailwind and what a markdown file leaves out.

use std::path::{Path, PathBuf};

use snapfire_fsr_cli::{build, BuildError, Options};

static NEXT: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);

/// A project with `config/app.toml` naming `app/`, React in the import map
/// and `files` under the app.
fn project(config: &str, files: &[(&str, &str)]) -> PathBuf {
  let n = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
  let root = std::env::temp_dir().join(format!("fsr-cli-mdx-{}-{n}", std::process::id()));
  let _ = std::fs::remove_dir_all(&root);
  let app = root.join("app");
  std::fs::create_dir_all(app.join("routes")).unwrap();
  std::fs::create_dir_all(root.join("config")).unwrap();
  std::fs::write(root.join("config/app.toml"), format!("[app]\ndir = \"app\"\n\n{config}")).unwrap();
  std::fs::write(app.join("importmap.json"), r#"{"imports":{"@snapfire/fsr-client/react":"/r","react":"/r","react-dom/client":"/d"}}"#).unwrap();
  std::fs::create_dir_all(app.join("vendor")).unwrap();
  std::fs::write(app.join("vendor/.fsr-vendor.json"), r#"{"packages":{"react":{"version":"19.1.0"},"react-dom":{"version":"19.1.0"}}}"#).unwrap();
  for (name, source) in files {
    let path = app.join(name);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, source).unwrap();
  }
  app
}

fn file<'a>(built: &'a snapfire_fsr_cli::Built, rel: &str) -> &'a str {
  built.files.iter().find(|(f, _)| f == rel).map(|(_, text)| text.as_str()).unwrap_or_else(|| panic!("{rel} not written; wrote {:?}", built.files.iter().map(|(f, _)| f).collect::<Vec<_>>()))
}

fn modules(built: &snapfire_fsr_cli::Built) -> Vec<String> {
  built.manifest.components.iter().map(|c| c.module.clone()).collect()
}

fn cleanup(app: &Path) {
  let _ = std::fs::remove_dir_all(app.parent().unwrap());
}

const NOTES: &str = "export const meta = { title: \"Notes\" }\n\n# Notes\n\nSome **text** and {1 + 1}.\n";

#[test]
fn a_page_mdx_is_a_route_whose_meta_is_its_export() {
  let app = project("", &[("routes/notes/page.mdx", NOTES), ("routes/plain/page.md", "# Plain\n\nNo meta here.\n")]);
  let built = build(&app, &Options::default()).expect("builds");
  assert!(built.report.routes.iter().any(|(pattern, _)| pattern == "/notes"), "{:?}", built.report.routes);
  assert!(built.report.routes.iter().any(|(pattern, _)| pattern == "/plain"), "{:?}", built.report.routes);
  assert!(modules(&built).contains(&"routes/notes/page.mdx#default".to_owned()), "{:?}", modules(&built));
  let source = built.manifest.sources.iter().find(|s| s.module.as_deref() == Some("routes/notes/page.mdx")).expect("a source row for the page's meta");
  assert!(source.meta.is_some());
  assert!(!built.manifest.sources.iter().any(|s| s.module.as_deref() == Some("routes/plain/page.md")), "a page with no meta has no source row");
  cleanup(&app);
}

#[test]
fn a_loader_and_an_mdx_page_both_setting_meta_is_refused() {
  let loader = "export async function load() {\n  return {};\n}\nexport const meta = () => ({ title: \"From the loader\" });\n";
  let app = project("", &[("routes/notes/page.mdx", NOTES), ("routes/notes/page.loader.ts", loader)]);
  match build(&app, &Options::default()) {
    Err(BuildError::MetaTwice { module, page }) => {
      assert_eq!(module, "routes/notes/page.loader.ts");
      assert_eq!(page, "routes/notes/page.mdx");
    }
    Err(other) => panic!("{other}"),
    Ok(_) => panic!("built"),
  }
  cleanup(&app);
}

#[test]
fn mdx_components_places_its_components_for_markdown_elements() {
  let components = "function Heading({ id, children }: { id?: string; children?: any }) {\n  return <h2 className=\"heading\" id={id}>{children}</h2>;\n}\nexport function useMDXComponents(components?: Record<string, unknown>) {\n  return { h2: Heading, blockquote: ({ children }: { children?: any }) => <aside>{children}</aside>, ...components };\n}\n";
  let app = project("", &[("routes/page.mdx", "## Mapped\n\n> quoted\n"), ("mdx-components.tsx", components)]);
  let built = build(&app, &Options::default()).expect("builds");
  let modules = modules(&built);
  assert!(modules.contains(&"mdx-components.tsx#Heading".to_owned()), "{modules:?}");
  assert!(modules.contains(&"mdx-components.tsx#__mdx_blockquote".to_owned()), "{modules:?}");
  let copy = file(&built, ".fsr-bundle/routes/page.mdx");
  assert!(copy.contains("import { useMDXComponents as _provideComponents } from \"@app/mdx-components\";"), "{copy}");
  assert!(copy.contains("<_components.h2 id={\"mapped\"}>"), "{copy}");
  cleanup(&app);
}

#[test]
fn a_mdx_components_without_use_mdx_components_is_refused() {
  let app = project("", &[("routes/page.mdx", "# x\n"), ("mdx-components.tsx", "export const components = {};\n")]);
  let error = build(&app, &Options::default()).err().expect("refused").to_string();
  assert!(error.contains("useMDXComponents"), "{error}");
  cleanup(&app);
}

#[test]
fn collections_list_every_content_file_with_its_meta_and_toc() {
  let loader = "import { posts } from \"@generated/content\";\nexport async function load() {\n  return { titles: posts.map((p) => p.meta.title), slugs: posts.map((p) => p.slug) };\n}\n";
  let page = "import type { RootProps } from \"@generated/client\";\nexport default function Index({ titles }: RootProps) {\n  return <ul>{titles.map((t) => <li key={t}>{t}</li>)}</ul>;\n}\n";
  let files = [
    ("routes/page.tsx", page),
    ("routes/page.loader.ts", loader),
    ("content/posts/b-second.mdx", "export const meta = { title: \"Second\" }\n\n## Part\n\nThe second\none, in *short*.\n\nMore.\n"),
    ("content/posts/a-first.md", "# First\n"),
    ("content/posts/notes.txt", "not content"),
  ];
  let app = project("[mdx.collections]\nposts = \"content/posts\"\n", &files);
  let built = build(&app, &Options::default()).expect("builds");
  let content = file(&built, "generated/content.ts");
  assert!(content.contains("import { toc as toc0 } from \"../content/posts/a-first.md\";"), "{content}");
  assert!(content.contains("import { meta as meta1, toc as toc1 } from \"../content/posts/b-second.mdx\";"), "{content}");
  let first = content.find("slug: \"a-first\"").expect("the first entry");
  let second = content.find("slug: \"b-second\"").expect("the second entry");
  assert!(first < second, "{content}");
  assert!(content.contains("{ slug: \"a-first\", file: \"a-first.md\", meta: {}, toc: toc0, excerpt: null }"), "{content}");
  assert!(content.contains("toc: toc1, excerpt: \"The second one, in short.\" }"), "the first paragraph is the excerpt: {content}");
  assert!(!content.contains("notes"), "{content}");
  assert!(built.manifest.sources.iter().any(|s| s.module.as_deref() == Some("routes/page.loader.ts") && s.body.is_some()), "the loader reading the collection lowered");
  assert!(file(&built, "generated/mdx.d.ts").contains("declare module \"*.mdx\""));
  cleanup(&app);
}

#[test]
fn a_collection_lists_the_files_in_its_subdirectories_by_their_path() {
  let files = [
    ("content/plaza/tutorials/1-foundations/010-join.md", "# Join\n"),
    ("content/plaza/tutorials/2-rooms/010-rooms.mdx", "export const meta = { title: \"Rooms\" }\n\nText.\n"),
    ("content/intro.md", "# Intro\n"),
    ("content/.drafts/hidden.md", "# Hidden\n"),
  ];
  let app = project("[mdx.collections]\ntutorials = \"content\"\n", &files);
  let built = build(&app, &Options::default()).expect("builds");
  let content = file(&built, "generated/content.ts");
  assert!(content.contains("{ slug: \"intro\", file: \"intro.md\","), "{content}");
  assert!(content.contains("{ slug: \"plaza/tutorials/1-foundations/010-join\", file: \"plaza/tutorials/1-foundations/010-join.md\","), "{content}");
  assert!(content.contains("from \"../content/plaza/tutorials/2-rooms/010-rooms.mdx\";"), "{content}");
  let join = content.find("1-foundations/010-join\"").expect("the join entry");
  let rooms = content.find("slug: \"plaza/tutorials/2-rooms/010-rooms\"").expect("the rooms entry");
  assert!(join < rooms, "{content}");
  assert!(!content.contains("hidden"), "a dot directory is left out: {content}");
  cleanup(&app);
}

#[test]
fn a_template_import_page_builds_with_one_component_per_file() {
  let loader = "import { posts } from \"@generated/content\";\nimport type { Ctx } from \"@snapfire/fsr\";\nexport async function load({ params }: Ctx<\"/post/{slug}\">) {\n  return { slug: params.slug };\n}\nexport const paths = () => posts.map((p) => ({ slug: p.slug }));\n";
  let page = "import type { PostSlugProps } from \"@generated/client\";\nexport default async function Post({ slug }: PostSlugProps) {\n  const { default: Body, meta } = await import(`../../../content/posts/${slug}.mdx`);\n  return <article><h1>{meta.title}</h1><Body /></article>;\n}\n";
  let files = [
    ("routes/post/[slug]/page.tsx", page),
    ("routes/post/[slug]/page.loader.ts", loader),
    ("content/posts/one.mdx", "export const meta = { title: \"One\" }\n\nFirst.\n"),
    ("content/posts/two.mdx", "export const meta = { title: \"Two\" }\n\nSecond.\n"),
  ];
  let app = project("[mdx.collections]\nposts = \"content/posts\"\n", &files);
  let built = build(&app, &Options::default()).expect("builds");
  let modules = modules(&built);
  for module in ["routes/post/[slug]/page.tsx#default", "content/posts/one.mdx#default", "content/posts/two.mdx#default"] {
    assert!(modules.contains(&module.to_owned()), "{module} in {modules:?}");
  }
  cleanup(&app);
}

#[test]
fn highlight_classes_stay_out_of_the_tailwind_list() {
  let page = "# Code\n\n```ts\nconst a = 1;\n```\n\n<p className=\"mt-4\">tail</p>\n";
  let app = project("", &[("routes/page.mdx", page), ("styles/app.css", "@import \"tailwindcss\";\n")]);
  let built = build(&app, &Options::default()).expect("builds");
  let classes = file(&built, "generated/tailwind.classes");
  assert!(classes.contains("mt-4\t"), "{classes}");
  assert!(!classes.contains("hl-"), "{classes}");
  assert!(built.manifest.to_sexpr().contains("hl-storage"), "the highlighted spans are in the plan");
  cleanup(&app);
}

#[test]
fn turning_highlighting_off_writes_plain_code() {
  let app = project("[mdx]\nhighlight = false\n", &[("routes/page.mdx", "```ts\nconst a = 1;\n```\n")]);
  let built = build(&app, &Options::default()).expect("builds");
  let plan = built.manifest.to_sexpr();
  assert!(plan.contains("language-ts") && !plan.contains("hl-storage"), "{plan}");
  cleanup(&app);
}

#[test]
fn frontmatter_is_read_only_when_configured() {
  let page = "---\ntitle: From frontmatter\n---\n\n# Body\n";
  let app = project("[mdx]\nfrontmatter = true\n", &[("routes/page.md", page)]);
  let built = build(&app, &Options::default()).expect("builds");
  assert!(built.manifest.sources.iter().any(|s| s.module.as_deref() == Some("routes/page.md") && s.meta.is_some()));
  cleanup(&app);
  let app = project("", &[("routes/page.md", page)]);
  let built = build(&app, &Options::default()).expect("builds");
  assert!(!built.manifest.sources.iter().any(|s| s.module.as_deref() == Some("routes/page.md")), "without the setting the dashes are a rule");
  cleanup(&app);
}

#[test]
fn raw_html_in_markdown_is_reported() {
  let app = project("", &[("routes/page.md", "# A\n\n<div>raw</div>\n")]);
  let built = build(&app, &Options::default()).expect("builds");
  let report = built.report.to_string();
  assert!(report.contains("warning   routes/page.md:3:1: raw HTML in markdown is not rendered"), "{report}");
  cleanup(&app);
}

#[test]
fn an_unknown_mdx_key_is_refused() {
  let app = project("[mdx]\nplugins = [\"remark-gfm\"]\n", &[("routes/page.mdx", "# x\n")]);
  let error = build(&app, &Options::default()).err().expect("refused").to_string();
  assert!(error.contains("mdx.plugins is not a key"), "{error}");
  cleanup(&app);
}

#[test]
fn a_residue_in_an_mdx_page_names_its_line() {
  let app = project("[build]\nstrict = true\n", &[("routes/page.mdx", "# Title\n\nNow {window.location.href}.\n")]);
  let error = build(&app, &Options::beside(&app)).err().expect("refused").to_string();
  assert!(error.contains("routes/page.mdx:3:6"), "{error}");
  cleanup(&app);
}

#[test]
fn fsr_use_mdx_writes_the_component_map_and_the_code_sheet() {
  let app = project("", &[("routes/page.tsx", "export default function Page() {\n  return <p>x</p>;\n}\n")]);
  let adopted = snapfire_fsr_cli::direction::adopt(&app, &["mdx".to_owned()], snapfire_fsr_cli::direction::UseOptions::default().fetch(false).example(true)).expect("adopts");
  let written: Vec<String> = adopted.written.iter().map(|p| p.strip_prefix(&app).unwrap().display().to_string()).collect();
  assert!(written.contains(&"mdx-components.tsx".to_owned()), "{written:?}");
  assert!(written.contains(&"styles/code.css".to_owned()), "{written:?}");
  assert!(written.contains(&"routes/notes/page.mdx".to_owned()), "{written:?}");
  let css = std::fs::read_to_string(app.join("styles/code.css")).unwrap();
  assert!(css.contains("[data-sf-scheme=\"dark\"] .hl-code"), "{css}");
  assert!(css.contains(":root:not([data-sf-scheme]) .hl-code"), "{css}");
  assert!(css.contains(".hl-code code { background: none;"), "an application's own code style stops at a highlighted block: {css}");
  assert!(css.contains(".hl-code .hl-meta.hl-output {") && css.contains("hl-prompt { opacity: 0.6; user-select: none; }"), "a session's output and prompts: {css}");
  let built = build(&app, &Options::default()).expect("the example builds");
  assert!(built.report.routes.iter().any(|(pattern, _)| pattern == "/notes"));
  let again = snapfire_fsr_cli::direction::adopt(&app, &["mdx".to_owned()], snapfire_fsr_cli::direction::UseOptions::default().fetch(false).example(false)).expect("adopts again");
  assert!(again.written.is_empty(), "{:?}", again.written);
  assert!(again.present.contains(&"mdx-components.tsx".to_owned()));
  cleanup(&app);
}

#[test]
fn every_coloured_rule_in_the_code_sheet_names_its_scheme() {
  let mut css = snapfire_fsr_cli::direction::code_sheet().expect("built with mdx-highlight");
  while let Some(start) = css.find("/*") {
    let end = css[start..].find("*/").map(|e| start + e + 2).unwrap_or(css.len());
    css.replace_range(start..end, "");
  }
  let scoped = ["[data-sf-scheme=\"light\"] ", "[data-sf-scheme=\"dark\"] ", ":root:not([data-sf-scheme]) "];
  let mut checked = 0;
  for block in css.split('}') {
    let Some((selectors, body)) = block.split_once('{') else { continue };
    if !body.replace("color: inherit", "").contains("color:") || !selectors.contains(".hl-") {
      continue;
    }
    for selector in selectors.split(',') {
      let selector = selector.trim();
      assert!(scoped.iter().any(|s| selector.starts_with(s)), "`{selector}` colours code in every scheme");
      checked += 1;
    }
  }
  assert!(checked > 100, "{checked} selectors checked");
}
