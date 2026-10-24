//! `path`: POSIX paths as text. `Terminal` reach, since it reads nothing but
//! its arguments, `path.resolve` aside, which reads the working directory.

use crate::ext::{text, text_opt, Extensions, Reach};
use crate::interp::Fail;
use snapfire_fsr_core::Value;

pub fn register(extensions: &mut Extensions) {
  extensions.register("path.join", Reach::Terminal, |_, args| Ok(Value::str(join(&parts("path.join", args)?))));
  extensions.register("path.resolve", Reach::Terminal, |_, args| Ok(Value::str(resolve(&parts("path.resolve", args)?)?)));
  extensions.register("path.normalize", Reach::Terminal, |_, args| Ok(Value::str(normalize(text("path.normalize", args, 0)?))));
  extensions.register("path.dirname", Reach::Terminal, |_, args| Ok(Value::str(dirname(text("path.dirname", args, 0)?))));
  extensions.register("path.basename", Reach::Terminal, |_, args| Ok(Value::str(basename(text("path.basename", args, 0)?, text_opt("path.basename", args, 1)?))));
  extensions.register("path.extname", Reach::Terminal, |_, args| Ok(Value::str(extname(text("path.extname", args, 0)?))));
  extensions.register("path.relative", Reach::Terminal, |_, args| Ok(Value::str(relative(text("path.relative", args, 0)?, text("path.relative", args, 1)?)?)));
  extensions.register("path.isAbsolute", Reach::Terminal, |_, args| Ok(Value::Bool(text("path.isAbsolute", args, 0)?.starts_with('/'))));
}

/// Every argument as text, as `path.join(...parts)` takes them.
fn parts<'a>(what: &str, args: &'a [Value]) -> Result<Vec<&'a str>, Fail> {
  (0..args.len()).map(|i| text(what, args, i)).collect()
}

/// `path.normalize`: `.` and empty segments dropped, `..` resolved against what precedes it, a leading `/` and a trailing `/` kept; `.` for nothing.
fn normalize(path: &str) -> String {
  let absolute = path.starts_with('/');
  let trailing = path.len() > 1 && path.ends_with('/');
  let mut out: Vec<&str> = Vec::new();
  for part in path.split('/') {
    match part {
      "" | "." => {}
      ".." => match out.last() {
        Some(&"..") | None if !absolute => out.push(".."),
        Some(_) => {
          out.pop();
        }
        None => {}
      },
      p => out.push(p),
    }
  }
  let mut joined = out.join("/");
  if absolute {
    joined.insert(0, '/');
  }
  if joined.is_empty() {
    return if absolute { "/".to_owned() } else { ".".to_owned() };
  }
  if trailing && joined != "/" {
    joined.push('/');
  }
  joined
}

fn join(parts: &[&str]) -> String {
  let joined: Vec<&str> = parts.iter().copied().filter(|p| !p.is_empty()).collect();
  if joined.is_empty() {
    return ".".to_owned();
  }
  normalize(&joined.join("/"))
}

/// `path.resolve`: the parts right to left until one is absolute, the working directory before them otherwise; no trailing `/`.
fn resolve(parts: &[&str]) -> Result<String, Fail> {
  let mut resolved = String::new();
  for part in parts.iter().rev().filter(|p| !p.is_empty()) {
    resolved = if resolved.is_empty() { (*part).to_owned() } else { format!("{part}/{resolved}") };
    if part.starts_with('/') {
      break;
    }
  }
  if !resolved.starts_with('/') {
    let cwd = std::env::current_dir().map_err(|e| Fail::internal(format!("path.resolve: {e}")))?;
    resolved = format!("{}/{resolved}", cwd.to_string_lossy());
  }
  let normalized = normalize(&resolved);
  let trimmed = normalized.trim_end_matches('/');
  Ok(if trimmed.is_empty() { "/".to_owned() } else { trimmed.to_owned() })
}

fn dirname(path: &str) -> String {
  let trimmed = path.trim_end_matches('/');
  if trimmed.is_empty() {
    return if path.starts_with('/') { "/".to_owned() } else { ".".to_owned() };
  }
  match trimmed.rfind('/') {
    None => ".".to_owned(),
    Some(0) => "/".to_owned(),
    Some(at) => trimmed[..at].trim_end_matches('/').to_owned(),
  }
}

fn basename(path: &str, ext: Option<&str>) -> String {
  let trimmed = path.trim_end_matches('/');
  let base = trimmed.rsplit('/').next().unwrap_or(trimmed);
  match ext {
    Some(ext) if base != ext && base.ends_with(ext) => base[..base.len() - ext.len()].to_owned(),
    _ => base.to_owned(),
  }
}

fn extname(path: &str) -> String {
  let base = basename(path, None);
  match base.rfind('.') {
    Some(at) if at > 0 => base[at..].to_owned(),
    _ => String::new(),
  }
}

fn relative(from: &str, to: &str) -> Result<String, Fail> {
  let from = resolve(&[from])?;
  let to = resolve(&[to])?;
  let from: Vec<&str> = from.split('/').filter(|p| !p.is_empty()).collect();
  let to: Vec<&str> = to.split('/').filter(|p| !p.is_empty()).collect();
  let common = from.iter().zip(&to).take_while(|(a, b)| a == b).count();
  let mut out: Vec<&str> = vec![".."; from.len() - common];
  out.extend(&to[common..]);
  Ok(out.join("/"))
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn paths_join_normalize_and_split_as_posix_paths_do() {
    assert_eq!(join(&["/a/b", "../c", "./d"]), "/a/c/d");
    assert_eq!(join(&["a", "", "b/"]), "a/b/");
    assert_eq!(join(&[]), ".");
    assert_eq!(normalize("/a//b/./c/.."), "/a/b");
    assert_eq!(normalize("../../x"), "../../x");
    assert_eq!(normalize("/.."), "/");
    assert_eq!(dirname("/a/b/c.txt"), "/a/b");
    assert_eq!(dirname("c.txt"), ".");
    assert_eq!(dirname("/c.txt"), "/");
    assert_eq!(basename("/a/b/c.txt", None), "c.txt");
    assert_eq!(basename("/a/b/c.txt", Some(".txt")), "c");
    assert_eq!(basename("/a/b/", None), "b");
    assert_eq!(extname("c.tar.gz"), ".gz");
    assert_eq!(extname(".bashrc"), "");
    assert_eq!(extname("file."), ".");
    assert_eq!(relative("/a/b/c", "/a/d").unwrap(), "../../d");
    assert_eq!(resolve(&["/a/b", "c", "/x", "y"]).unwrap(), "/x/y");
    assert_eq!(resolve(&["/a/b/"]).unwrap(), "/a/b");
  }
}
