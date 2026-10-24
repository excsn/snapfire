//! `path`, `os`, `process` and `fs`: Node's host API for a terminal page,
//! which runs where it is viewed. `path` works on text alone and is
//! `Terminal` reach; the rest read the machine and are `Process` reach.
//! Paths are POSIX, as Node's `path.posix`.

use crate::ext::{option, text, text_opt, Extensions, Reach};
use crate::interp::Fail;
use snapfire_fsr_core::{Value, ValueMap};

pub fn register(extensions: &mut Extensions) {
  extensions.register("path.join", Reach::Terminal, |_, args| Ok(Value::str(join(&parts("path.join", args)?))));
  extensions.register("path.resolve", Reach::Terminal, |_, args| Ok(Value::str(resolve(&parts("path.resolve", args)?)?)));
  extensions.register("path.normalize", Reach::Terminal, |_, args| Ok(Value::str(normalize(text("path.normalize", args, 0)?))));
  extensions.register("path.dirname", Reach::Terminal, |_, args| Ok(Value::str(dirname(text("path.dirname", args, 0)?))));
  extensions.register("path.basename", Reach::Terminal, |_, args| Ok(Value::str(basename(text("path.basename", args, 0)?, text_opt("path.basename", args, 1)?))));
  extensions.register("path.extname", Reach::Terminal, |_, args| Ok(Value::str(extname(text("path.extname", args, 0)?))));
  extensions.register("path.relative", Reach::Terminal, |_, args| Ok(Value::str(relative(text("path.relative", args, 0)?, text("path.relative", args, 1)?)?)));
  extensions.register("path.isAbsolute", Reach::Terminal, |_, args| Ok(Value::Bool(text("path.isAbsolute", args, 0)?.starts_with('/'))));
  extensions.register("os.homedir", Reach::Process, |_, _| Ok(Value::str(std::env::var("HOME").or_else(|_| std::env::var("USERPROFILE")).unwrap_or_default())));
  extensions.register("os.platform", Reach::Process, |_, _| Ok(Value::str(platform())));
  extensions.register("os.EOL", Reach::Process, |_, _| Ok(Value::str(if cfg!(windows) { "\r\n" } else { "\n" })));
  extensions.register("os.tmpdir", Reach::Process, |_, _| Ok(Value::str(std::env::temp_dir().to_string_lossy().trim_end_matches('/').to_owned())));
  extensions.register("process.cwd", Reach::Process, |_, _| Ok(Value::str(std::env::current_dir().map_err(|e| Fail::internal(format!("process.cwd: {e}")))?.to_string_lossy().into_owned())));
  extensions.register("process.memoryUsage", Reach::Process, |_, _| Ok(memory_usage()));
  extensions.register("fs.existsSync", Reach::Process, |_, args| Ok(Value::Bool(std::path::Path::new(text("fs.existsSync", args, 0)?).exists())));
  extensions.register("fs.readFileSync", Reach::Process, |_, args| read_file("fs.readFileSync", args));
  extensions.register("fs.readFile", Reach::Process, |_, args| read_file("fs.readFile", args));
  extensions.register("fs.writeFileSync", Reach::Process, |_, args| write_file("fs.writeFileSync", args));
  extensions.register("fs.writeFile", Reach::Process, |_, args| write_file("fs.writeFile", args));
  extensions.register("fs.mkdirSync", Reach::Process, |_, args| mkdir("fs.mkdirSync", args));
  extensions.register("fs.mkdir", Reach::Process, |_, args| mkdir("fs.mkdir", args));
  extensions.register("fs.readdirSync", Reach::Process, |_, args| readdir("fs.readdirSync", args));
  extensions.register("fs.readdir", Reach::Process, |_, args| readdir("fs.readdir", args));
  extensions.register("fs.statSync", Reach::Process, |_, args| stat("fs.statSync", args));
  extensions.register("fs.stat", Reach::Process, |_, args| stat("fs.stat", args));
}

/// Every argument as text, as `path.join(...parts)` takes them.
fn parts<'a>(what: &str, args: &'a [Value]) -> Result<Vec<&'a str>, Fail> {
  (0..args.len()).map(|i| text(what, args, i)).collect()
}

/// Node's `platform()`: `darwin`, `linux`, `win32`, `freebsd` or whatever Rust calls the rest.
fn platform() -> &'static str {
  match std::env::consts::OS {
    "macos" => "darwin",
    "windows" => "win32",
    other => other,
  }
}

/// Node's `process.memoryUsage()`: `rss` is the process's resident set in bytes; `heapTotal`, `heapUsed`, `external` and `arrayBuffers` are 0, since there is no JavaScript heap.
fn memory_usage() -> Value {
  let mut usage = ValueMap::default();
  usage.insert("rss".to_owned(), Value::Int(resident_bytes() as i128));
  for field in ["heapTotal", "heapUsed", "external", "arrayBuffers"] {
    usage.insert(field.to_owned(), Value::Int(0));
  }
  Value::Map(usage)
}

#[cfg(target_os = "macos")]
fn resident_bytes() -> u64 {
  let mut info: libc::mach_task_basic_info = unsafe { std::mem::zeroed() };
  let mut count = libc::MACH_TASK_BASIC_INFO_COUNT;
  let result = unsafe { libc::task_info(libc::mach_task_self(), libc::MACH_TASK_BASIC_INFO, &mut info as *mut libc::mach_task_basic_info as libc::task_info_t, &mut count) };
  if result == libc::KERN_SUCCESS { info.resident_size } else { 0 }
}

#[cfg(target_os = "linux")]
fn resident_bytes() -> u64 {
  let statm = std::fs::read_to_string("/proc/self/statm").unwrap_or_default();
  let pages: u64 = statm.split_whitespace().nth(1).and_then(|p| p.parse().ok()).unwrap_or(0);
  pages * unsafe { libc::sysconf(libc::_SC_PAGESIZE) }.max(0) as u64
}

#[cfg(not(any(target_os = "macos", target_os = "linux")))]
fn resident_bytes() -> u64 {
  0
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
  Ok(normalized.trim_end_matches('/').to_owned()).map(|s: String| if s.is_empty() { "/".to_owned() } else { s })
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

/// A file system failure as a throwable `{ name, message, code }`, so a `catch` reads `code` as it does under Node.
fn io_fail(what: &str, path: &str, e: std::io::Error) -> Fail {
  let code = match e.kind() {
    std::io::ErrorKind::NotFound => "ENOENT",
    std::io::ErrorKind::PermissionDenied => "EACCES",
    std::io::ErrorKind::AlreadyExists => "EEXIST",
    _ => "EIO",
  };
  let message = format!("{code}: {e}, {what} '{path}'");
  let mut thrown = ValueMap::default();
  thrown.insert("name".to_owned(), Value::str("Error"));
  thrown.insert("message".to_owned(), Value::str(message.clone()));
  thrown.insert("code".to_owned(), Value::str(code));
  thrown.insert("path".to_owned(), Value::str(path));
  Fail::internal(format!("{what}: {message}")).with_thrown(Value::Map(thrown))
}

/// `readFileSync(path, encoding?)`: the file as text. Bytes that are not UTF-8 come back as U+FFFD, as `utf8` decoding does under Node.
fn read_file(what: &str, args: &[Value]) -> Result<Value, Fail> {
  let path = text(what, args, 0)?;
  let bytes = std::fs::read(path).map_err(|e| io_fail(what, path, e))?;
  Ok(Value::str(String::from_utf8_lossy(&bytes).into_owned()))
}

fn write_file(what: &str, args: &[Value]) -> Result<Value, Fail> {
  let path = text(what, args, 0)?;
  let data = match args.get(1) {
    Some(Value::Str(s)) => s.to_string(),
    Some(other) => crate::interp::stringify(other).unwrap_or_default(),
    None => String::new(),
  };
  std::fs::write(path, data).map_err(|e| io_fail(what, path, e))?;
  Ok(Value::Null)
}

fn mkdir(what: &str, args: &[Value]) -> Result<Value, Fail> {
  let path = text(what, args, 0)?;
  let recursive = option(what, args, 1, "recursive")?.is_some_and(crate::interp::truthy);
  let made = if recursive { std::fs::create_dir_all(path) } else { std::fs::create_dir(path) };
  made.map_err(|e| io_fail(what, path, e))?;
  Ok(Value::Null)
}

/// `readdirSync(path)`: the entries' names, sorted, as Node lists them on most file systems.
fn readdir(what: &str, args: &[Value]) -> Result<Value, Fail> {
  let path = text(what, args, 0)?;
  let entries = std::fs::read_dir(path).map_err(|e| io_fail(what, path, e))?;
  let mut names: Vec<String> = entries.filter_map(|e| e.ok()).map(|e| e.file_name().to_string_lossy().into_owned()).collect();
  names.sort();
  Ok(Value::Seq(names.into_iter().map(Value::str).collect()))
}

/// `statSync(path)`: `{ isFile, isDirectory, size, mtimeMs }`. Node's `isFile()` and `isDirectory()` are fields here, since a value holds no function; `fsr migrate` rewrites the calls.
fn stat(what: &str, args: &[Value]) -> Result<Value, Fail> {
  let path = text(what, args, 0)?;
  let meta = std::fs::metadata(path).map_err(|e| io_fail(what, path, e))?;
  let modified = meta.modified().ok().and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok()).map_or(0.0, |d| d.as_secs_f64() * 1000.0);
  let mut out = ValueMap::default();
  out.insert("isFile".to_owned(), Value::Bool(meta.is_file()));
  out.insert("isDirectory".to_owned(), Value::Bool(meta.is_dir()));
  out.insert("size".to_owned(), Value::Int(i128::from(meta.len())));
  out.insert("mtimeMs".to_owned(), Value::F64(modified));
  Ok(Value::Map(out))
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

  #[test]
  fn a_missing_file_fails_with_a_throwable_carrying_enoent() {
    let fail = read_file("fs.readFileSync", &[Value::str("/nowhere/at/all.txt")]).unwrap_err();
    let Some(thrown) = fail.thrown else { panic!("a thrown value") };
    let Value::Map(map) = *thrown else { panic!("a map") };
    assert_eq!(map.get("code"), Some(&Value::str("ENOENT")));
  }
}
