use std::path::{Path, PathBuf};
use std::process::ExitCode;

/// The handbook's `[build] before`: reads the `title` and `order` of every page under
/// `routes/(docs)/` and writes `src/chapters.ts`, the list the masthead and the docs layout link
/// from. A page's title is the part before ` · `.
fn main() -> ExitCode {
  let app = std::env::var_os("FSR_APP").map(PathBuf::from).unwrap_or_else(|| Path::new(env!("CARGO_MANIFEST_DIR")).join("../app"));
  match chapters(&app.join("routes/(docs)")).and_then(|chapters| write(&app.join("src/chapters.ts"), &chapters)) {
    Ok(()) => ExitCode::SUCCESS,
    Err(why) => {
      eprintln!("contents: {why}");
      ExitCode::FAILURE
    }
  }
}

struct Chapter {
  order: u32,
  href: String,
  title: String,
}

fn chapters(docs: &Path) -> Result<Vec<Chapter>, String> {
  let mut chapters = Vec::new();
  let dirs = std::fs::read_dir(docs).map_err(|e| format!("{}: {e}", docs.display()))?;
  for dir in dirs.map_while(Result::ok).filter(|entry| entry.path().is_dir()) {
    let Some(page) = ["page.md", "page.mdx"].iter().map(|name| dir.path().join(name)).find(|page| page.is_file()) else { continue };
    let source = std::fs::read_to_string(&page).map_err(|e| format!("{}: {e}", page.display()))?;
    let meta = metadata(&source).ok_or_else(|| format!("{} has no frontmatter and no `export const meta`", page.display()))?;
    let title = field(meta, "title").ok_or_else(|| format!("{} has no title", page.display()))?;
    let order = field(meta, "order").and_then(|order| order.parse().ok()).ok_or_else(|| format!("{} has no numeric order", page.display()))?;
    let title = title.split(" · ").next().unwrap_or(title).to_owned();
    chapters.push(Chapter { order, href: format!("/{}", dir.file_name().to_string_lossy()), title });
  }
  chapters.sort_by(|a, b| a.order.cmp(&b.order).then_with(|| a.href.cmp(&b.href)));
  Ok(chapters)
}

/// The frontmatter of a markdown page or the `export const meta` line of an MDX page.
fn metadata(source: &str) -> Option<&str> {
  if let Some(rest) = source.strip_prefix("---\n") {
    return rest.find("\n---").map(|end| &rest[..end]);
  }
  source.lines().find(|line| line.starts_with("export const meta = {"))
}

/// The value written after `name:`, up to a comma, a brace or the line's end, without quotes.
fn field<'a>(meta: &'a str, name: &str) -> Option<&'a str> {
  let at = meta.find(&format!("{name}:"))? + name.len() + 1;
  let rest = &meta[at..];
  let end = rest.find([',', '}', '\n']).unwrap_or(rest.len());
  Some(rest[..end].trim().trim_matches('"')).filter(|value| !value.is_empty())
}

/// Writes the module when what it holds changed, so an unchanged list is no change to the watcher.
fn write(to: &Path, chapters: &[Chapter]) -> Result<(), String> {
  let mut module = String::from("// Written by contents/, the handbook's `[build] before`, from each page's title and order.\n\nexport type Chapter = { href: string; title: string };\n\nexport const chapters: Chapter[] = [\n");
  for chapter in chapters {
    module.push_str(&format!("  {{ href: \"{}\", title: \"{}\" }},\n", chapter.href, chapter.title));
  }
  module.push_str("];\n");
  if std::fs::read_to_string(to).ok().as_deref() == Some(module.as_str()) {
    return Ok(());
  }
  std::fs::write(to, &module).map_err(|e| format!("{}: {e}", to.display()))?;
  println!("wrote     {}, {} chapters", to.display(), chapters.len());
  Ok(())
}
