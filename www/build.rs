//! Reads `fsr/docs/guide/*.md` into `app/src/docs/guide.ts`, then builds the
//! app. The guide is the source; the site never holds a second copy of it.

use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use pulldown_cmark::{CodeBlockKind, Event, HeadingLevel, Options, Parser, Tag, TagEnd};
use snapfire_fsr_lower::component::ComponentSet;

type Cells = Vec<Vec<Run>>;

fn main() {
  let root = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap());
  let guide = root.join("../fsr/docs/guide");
  let out = root.join("app/src/docs/guide.ts");

  println!("cargo:rerun-if-changed={}", guide.display());
  let app = root.join("app");
  for watched in ["routes", "src", "schemas", "clients", "importmap.json", "types"] {
    println!("cargo:rerun-if-changed={}", app.join(watched).display());
  }

  let samples = root.join("samples");
  println!("cargo:rerun-if-changed={}", samples.display());
  std::fs::write(root.join("app/src/docs/samples.ts"), emit_samples(&samples)).unwrap();

  let chapters = read_guide(&guide);
  assert!(!chapters.is_empty(), "no chapters under {}", guide.display());
  std::fs::create_dir_all(out.parent().unwrap()).unwrap();
  std::fs::write(&out, emit(&chapters)).unwrap_or_else(|e| panic!("{}: {e}", out.display()));

  let mut options = snapfire_fsr_cli::DevOptions::beside(&app);
  options.snapfirec = snapfirec(&root);
  snapfire_fsr_cli::emit(&app, options).unwrap_or_else(|e| panic!("fsr build app: {e}"));
}

/// The compiler that bundles the app; `$SNAPFIREC` overrides it and `None`
/// falls back to `PATH`.
fn snapfirec(root: &Path) -> Option<PathBuf> {
  if let Some(path) = std::env::var_os("SNAPFIREC") {
    return Some(path.into());
  }
  ["target/debug/snapfirec", "target/release/snapfirec"].iter().map(|p| root.join("..").join(p)).find(|p| p.is_file())
}

struct Chapter {
  slug: String,
  number: String,
  title: String,
  section: &'static str,
  audience: String,
  blocks: Vec<Block>,
}

enum Block {
  Heading { level: u8, runs: Vec<Run> },
  Para(Vec<Run>),
  Code { lang: String, code: String },
  List { ordered: bool, items: Vec<Vec<Run>> },
  Table { head: Cells, rows: Vec<Cells> },
  Quote(Vec<Run>),
}

struct Run {
  kind: &'static str,
  text: String,
  href: String,
}

/// `000` through `003` are foundations, `1xx` the application, `2xx` the host,
/// `3xx` tooling and `9xx` reference, which is how the guide's own README
/// groups them.
fn section_of(number: &str) -> &'static str {
  match number.as_bytes().first() {
    Some(b'0') => "Foundations",
    Some(b'1') => "The application",
    Some(b'2') => "The host",
    Some(b'3') => "Tooling",
    _ => "Reference",
  }
}

fn read_guide(dir: &Path) -> Vec<Chapter> {
  let mut files: Vec<PathBuf> = std::fs::read_dir(dir)
    .unwrap_or_else(|e| panic!("{}: {e}", dir.display()))
    .filter_map(|e| e.ok().map(|e| e.path()))
    .filter(|p| p.extension().is_some_and(|x| x == "md") && p.file_stem().is_some_and(|s| s != "README"))
    .collect();
  files.sort();
  files.iter().map(|path| chapter(path)).collect()
}

fn chapter(path: &Path) -> Chapter {
  let slug = path.file_stem().unwrap().to_string_lossy().into_owned();
  let number = slug.split('-').next().unwrap_or(&slug).to_owned();
  let text = std::fs::read_to_string(path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
  let blocks = parse(&text);

  let title = blocks
    .iter()
    .find_map(|b| match b {
      Block::Heading { level: 1, runs } => Some(plain(runs)),
      _ => None,
    })
    .unwrap_or_else(|| slug.clone());
  let title = title.split_once(". ").map(|(_, rest)| rest.to_owned()).unwrap_or(title);

  let audience = blocks
    .iter()
    .find_map(|b| match b {
      Block::Para(runs) => plain(runs).strip_prefix("For: ").map(|a| a.split('.').next().unwrap_or(a).to_owned()),
      _ => None,
    })
    .unwrap_or_else(|| "everyone".to_owned());

  // The `# 000. Title` and the `**For:** ...` line are the chapter's own
  // front matter; the site renders them as fields, not as body.
  let mut body = Vec::new();
  let mut seen_title = false;
  for block in blocks {
    match &block {
      Block::Heading { level: 1, .. } if !seen_title => seen_title = true,
      Block::Para(runs) if plain(runs).starts_with("For: ") => {}
      _ => body.push(block),
    }
  }

  Chapter { slug, number: number.clone(), title, section: section_of(&number), audience, blocks: body }
}

fn plain(runs: &[Run]) -> String {
  runs.iter().map(|r| r.text.as_str()).collect()
}

fn parse(markdown: &str) -> Vec<Block> {
  let parser = Parser::new_ext(markdown, Options::ENABLE_STRIKETHROUGH | Options::ENABLE_TABLES);
  let mut blocks = Vec::new();
  let mut runs: Vec<Run> = Vec::new();
  let mut kind: &'static str = "text";
  let mut href = String::new();
  let mut code = String::new();
  let mut lang = String::new();
  let mut heading: Option<u8> = None;
  let mut list: Option<(bool, Vec<Vec<Run>>)> = None;
  let mut table: Option<(Cells, Vec<Cells>)> = None;
  let mut row: Cells = Vec::new();
  let mut in_head = false;
  let mut quote = false;
  let mut in_code = false;

  let push = |runs: &mut Vec<Run>, kind: &'static str, text: &str, href: &str| {
    if text.is_empty() {
      return;
    }
    runs.push(Run { kind, text: text.to_owned(), href: href.to_owned() });
  };

  for event in parser {
    match event {
      Event::Start(Tag::Heading { level, .. }) => {
        heading = Some(match level {
          HeadingLevel::H1 => 1,
          HeadingLevel::H2 => 2,
          HeadingLevel::H3 => 3,
          _ => 4,
        })
      }
      Event::End(TagEnd::Heading(_)) => {
        if let Some(level) = heading.take() {
          blocks.push(Block::Heading { level, runs: std::mem::take(&mut runs) });
        }
      }
      Event::Start(Tag::CodeBlock(info)) => {
        in_code = true;
        lang = match info {
          CodeBlockKind::Fenced(l) => l.to_string(),
          CodeBlockKind::Indented => String::new(),
        };
      }
      Event::End(TagEnd::CodeBlock) => {
        in_code = false;
        blocks.push(Block::Code { lang: std::mem::take(&mut lang), code: std::mem::take(&mut code).trim_end().to_owned() });
      }
      Event::Start(Tag::List(first)) => list = Some((first.is_some(), Vec::new())),
      Event::End(TagEnd::List(_)) => {
        if let Some((ordered, items)) = list.take() {
          blocks.push(Block::List { ordered, items });
        }
      }
      Event::End(TagEnd::Item) => {
        if let Some((_, items)) = list.as_mut() {
          items.push(std::mem::take(&mut runs));
        }
      }
      Event::Start(Tag::Table(_)) => table = Some((Vec::new(), Vec::new())),
      Event::End(TagEnd::Table) => {
        if let Some((head, rows)) = table.take() {
          blocks.push(Block::Table { head, rows });
        }
      }
      Event::Start(Tag::TableHead) => in_head = true,
      Event::End(TagEnd::TableHead) => {
        in_head = false;
        if let Some((head, _)) = table.as_mut() {
          *head = std::mem::take(&mut row);
        }
      }
      Event::End(TagEnd::TableCell) => row.push(std::mem::take(&mut runs)),
      Event::End(TagEnd::TableRow) => {
        if !in_head {
          if let Some((_, rows)) = table.as_mut() {
            rows.push(std::mem::take(&mut row));
          }
        }
      }
      Event::Start(Tag::BlockQuote(_)) => quote = true,
      Event::End(TagEnd::BlockQuote(_)) => {
        quote = false;
        blocks.push(Block::Quote(std::mem::take(&mut runs)));
      }
      Event::End(TagEnd::Paragraph) => {
        if list.is_none() && table.is_none() && !quote {
          blocks.push(Block::Para(std::mem::take(&mut runs)));
        }
      }
      Event::Start(Tag::Strong) => kind = "strong",
      Event::End(TagEnd::Strong) => kind = "text",
      Event::Start(Tag::Emphasis) => kind = "em",
      Event::End(TagEnd::Emphasis) => kind = "text",
      Event::Start(Tag::Link { dest_url, .. }) => {
        kind = "link";
        href = link(&dest_url);
      }
      Event::End(TagEnd::Link) => {
        kind = "text";
        href.clear();
      }
      Event::Code(text) => push(&mut runs, "code", &text, ""),
      Event::Text(text) => {
        if in_code {
          code.push_str(&text);
        } else {
          push(&mut runs, kind, &text, &href);
        }
      }
      Event::SoftBreak => {
        if in_code {
          code.push('\n');
        } else {
          push(&mut runs, kind, " ", &href);
        }
      }
      Event::HardBreak => push(&mut runs, kind, " ", &href),
      _ => {}
    }
  }
  blocks
}

/// A link between chapters becomes a site path; anything else is left alone.
fn link(dest: &str) -> String {
  match dest.strip_suffix(".md") {
    Some(name) if !name.contains("://") => {
      let name = name.rsplit('/').next().unwrap_or(name);
      if name == "README" {
        "/fsr/docs".to_owned()
      } else {
        format!("/fsr/docs/{name}")
      }
    }
    _ => dest.to_owned(),
  }
}

fn emit(chapters: &[Chapter]) -> String {
  let mut out = String::new();
  out.push_str("// Generated by build.rs from fsr/docs/guide; edit the markdown, not this file.\n\n");
  out.push_str("export interface Run {\n  kind: string;\n  text: string;\n  href: string;\n}\n\n");
  out.push_str("export interface Block {\n  kind: string;\n  level: number;\n  lang: string;\n  code: string;\n  ordered: boolean;\n  runs: Run[];\n  items: Run[][];\n  rows: Run[][][];\n}\n\n");
  out.push_str("export interface Chapter {\n  slug: string;\n  number: string;\n  title: string;\n  section: string;\n  audience: string;\n  blocks: Block[];\n}\n\n");
  out.push_str("export const CHAPTERS: Chapter[] = [\n");
  for chapter in chapters {
    let _ = write!(
      out,
      "  {{\n    slug: {},\n    number: {},\n    title: {},\n    section: {},\n    audience: {},\n    blocks: [\n",
      quote(&chapter.slug),
      quote(&chapter.number),
      quote(&chapter.title),
      quote(chapter.section),
      quote(&chapter.audience),
    );
    for block in &chapter.blocks {
      out.push_str("      ");
      out.push_str(&block_literal(block));
      out.push_str(",\n");
    }
    out.push_str("    ],\n  },\n");
  }
  out.push_str("];\n");
  out
}

fn block_literal(block: &Block) -> String {
  let (kind, level, lang, code, ordered, runs, items) = match block {
    Block::Heading { level, runs } => ("heading", *level, "", "", false, runs_literal(runs), "[]".to_owned()),
    Block::Para(runs) => ("para", 0, "", "", false, runs_literal(runs), "[]".to_owned()),
    Block::Quote(runs) => ("quote", 0, "", "", false, runs_literal(runs), "[]".to_owned()),
    Block::Code { lang, code } => ("code", 0, lang.as_str(), code.as_str(), false, "[]".to_owned(), "[]".to_owned()),
    Block::List { ordered, items } => {
      ("list", 0, "", "", *ordered, "[]".to_owned(), cells_literal(items))
    }
    Block::Table { head, rows } => {
      let rows = rows.iter().map(|r| cells_literal(r)).collect::<Vec<_>>().join(", ");
      return format!(
        "{{ kind: \"table\", level: 0, lang: \"\", code: \"\", ordered: false, runs: [], items: {}, rows: [{rows}] }}",
        cells_literal(head)
      );
    }
  };
  format!(
    "{{ kind: {}, level: {level}, lang: {}, code: {}, ordered: {ordered}, runs: {runs}, items: {items}, rows: [] }}",
    quote(kind),
    quote(lang),
    quote(code)
  )
}

fn cells_literal(cells: &[Vec<Run>]) -> String {
  format!("[{}]", cells.iter().map(|c| runs_literal(c)).collect::<Vec<_>>().join(", "))
}

fn runs_literal(runs: &[Run]) -> String {
  let inner = runs
    .iter()
    .map(|r| format!("{{ kind: {}, text: {}, href: {} }}", quote(r.kind), quote(&r.text), quote(&r.href)))
    .collect::<Vec<_>>()
    .join(", ");
  format!("[{inner}]")
}

fn quote(text: &str) -> String {
  let mut out = String::with_capacity(text.len() + 2);
  out.push('"');
  for c in text.chars() {
    match c {
      '"' => out.push_str("\\\""),
      '\\' => out.push_str("\\\\"),
      '\n' => out.push_str("\\n"),
      '\r' => {}
      '\t' => out.push_str("\\t"),
      c => out.push(c),
    }
  }
  out.push('"');
  out
}


/// The inspector's samples: real sources under `samples/`, lowered here so the
/// page shows what the compiler produced rather than a transcription of it.
fn emit_samples(dir: &Path) -> String {
  let read = |name: &str| std::fs::read_to_string(dir.join(name)).unwrap_or_else(|e| panic!("{}: {e}", dir.join(name).display()));
  let json = |value: &serde_json::Value| serde_json::to_string_pretty(value).unwrap();

  let loader = read("cart.loader.ts");
  let body = snapfire_fsr_lower::lower_loader("cart.loader.ts", &loader).unwrap_or_else(|e| panic!("cart.loader.ts: {e}"));
  let counter = json(&serde_json::to_value(&body[0]).unwrap());

  let actions = read("cart.actions.ts");
  let lowered = snapfire_fsr_lower::lower_actions("cart.actions.ts", &actions).unwrap_or_else(|e| panic!("cart.actions.ts: {e}"));
  let guard = json(&serde_json::to_value(&lowered[0].body[0]).unwrap());

  let component = read("Disclosure.tsx");
  let mut set = ComponentSet::new(dir);
  set.lower("Disclosure.tsx#Disclosure").unwrap_or_else(|e| panic!("Disclosure.tsx: {e}"));
  let handler = json(&serde_json::to_value(&set.components[0].1.handlers[0]).unwrap());

  let refused = read("residue.loader.ts");
  let diagnostic = match snapfire_fsr_lower::lower_loader("routes/cart/page.loader.ts", &refused) {
    Ok(_) => panic!("residue.loader.ts lowered; it is the sample that must not"),
    Err(e) => e.to_string(),
  };

  let mut out = String::from("// Generated by build.rs: every `ir` below is what the lowerer produced from the\n// source beside it under samples/. Edit the samples, not this file.\n\n");
  out.push_str("export interface Sample {\n  id: string;\n  name: string;\n  ts: string;\n  ir: string;\n  explanation: string;\n}\n\n");
  out.push_str("export const SAMPLES: Sample[] = [\n");
  for (id, name, ts, ir, explanation) in [
    ("reduce", "Cart counter", &loader, &counter, "TypeScript array methods lower to deterministic nodes the interpreter evaluates in Rust, with no JavaScript heap involved."),
    ("guard", "Action guard", &actions, &guard, "A guard that reads nothing external runs before any service socket is opened, so an invalid call never leaves the server."),
    ("island", "Server island handler", &component, &handler, "A server island's click lowers to pure data: the browser ships no component code and the host answers the round trip with a patch."),
  ] {
    let _ = write!(out, "  {{ id: {}, name: {}, ts: {}, ir: {}, explanation: {} }},\n", quote(id), quote(name), quote(ts.trim_end()), quote(ir), quote(explanation));
  }
  out.push_str("];\n\n");
  let _ = write!(out, "export const REFUSED = {{ ts: {}, diagnostic: {} }};\n", quote(refused.trim_end()), quote(&diagnostic));
  out
}
