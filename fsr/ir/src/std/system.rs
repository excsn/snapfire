//! `file` and `system`. `file` is `Body` reach: in a terminal program a path is the working directory's, in a web host the application's and confined to the roots `Ambient::files` names. `system` is the machine a terminal page runs on, `Process` reach.

use crate::ext::{option, text, Ambient, Extensions, Reach};
use crate::interp::Fail;
use snapfire_fsr_core::{Value, ValueMap};

pub fn register(extensions: &mut Extensions) {
  extensions.register("file.read", Reach::Body, |ambient, args| read("file.read", ambient, args));
  extensions.register("file.write", Reach::Body, |ambient, args| write("file.write", ambient, args));
  extensions.register("file.exists", Reach::Body, |ambient, args| Ok(Value::Bool(placed("file.exists", ambient, args)?.exists())));
  extensions.register("file.list", Reach::Body, |ambient, args| list("file.list", ambient, args));
  extensions.register("file.makeDir", Reach::Body, |ambient, args| make_dir("file.makeDir", ambient, args));
  extensions.register("file.stat", Reach::Body, |ambient, args| stat("file.stat", ambient, args));
  extensions.register("system.env", Reach::Process, |_, args| Ok(std::env::var(text("system.env", args, 0)?).map_or(Value::Null, Value::str)));
  extensions.register("system.home", Reach::Process, |_, _| Ok(Value::str(std::env::var("HOME").or_else(|_| std::env::var("USERPROFILE")).unwrap_or_default())));
  extensions.register("system.platform", Reach::Process, |_, _| Ok(Value::str(std::env::consts::OS)));
  extensions.register("system.tempDir", Reach::Process, |_, _| Ok(Value::str(std::env::temp_dir().to_string_lossy().trim_end_matches('/').to_owned())));
  extensions.register("system.lineEnd", Reach::Process, |_, _| Ok(Value::str(if cfg!(windows) { "\r\n" } else { "\n" })));
  extensions.register("system.cwd", Reach::Process, |_, _| Ok(Value::str(std::env::current_dir().map_err(|e| Fail::internal(format!("system.cwd: {e}")))?.to_string_lossy().into_owned())));
  extensions.register("system.memory", Reach::Process, |_, _| Ok(Value::Int(resident_bytes() as i128)));
}

#[cfg(target_os = "macos")]
fn resident_bytes() -> u64 {
  let mut info: libc::proc_taskinfo = unsafe { std::mem::zeroed() };
  let size = std::mem::size_of::<libc::proc_taskinfo>() as libc::c_int;
  let written = unsafe { libc::proc_pidinfo(std::process::id() as libc::c_int, libc::PROC_PIDTASKINFO, 0, &mut info as *mut libc::proc_taskinfo as *mut libc::c_void, size) };
  if written == size { info.pti_resident_size } else { 0 }
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

/// A file system failure as a throwable `{ name, message, code, path }`, `code` the POSIX error name, so a `catch` can tell a missing file from the rest.
fn io_fail(what: &str, path: &str, e: std::io::Error) -> Fail {
  let code = match e.kind() {
    std::io::ErrorKind::NotFound => "ENOENT",
    std::io::ErrorKind::PermissionDenied => "EACCES",
    std::io::ErrorKind::AlreadyExists => "EEXIST",
    _ => "EIO",
  };
  let message = format!("{what} '{path}': {e} ({code})");
  let mut thrown = ValueMap::default();
  thrown.insert("name".to_owned(), Value::str("Error"));
  thrown.insert("message".to_owned(), Value::str(message.clone()));
  thrown.insert("code".to_owned(), Value::str(code));
  thrown.insert("path".to_owned(), Value::str(path));
  Fail::internal(message).with_thrown(Value::Map(thrown))
}

/// `file.read(path)`: the file as text, bytes that are not UTF-8 as U+FFFD.
/// The first argument as a path: as written in a terminal program, placed under the roots in a web host.
fn placed(what: &str, ambient: &Ambient, args: &[Value]) -> Result<std::path::PathBuf, Fail> {
  let path = text(what, args, 0)?;
  match &ambient.files {
    Some(roots) => roots.place(what, path),
    None => Ok(std::path::PathBuf::from(path)),
  }
}

fn read(what: &str, ambient: &Ambient, args: &[Value]) -> Result<Value, Fail> {
  let path = text(what, args, 0)?;
  let bytes = std::fs::read(placed(what, ambient, args)?).map_err(|e| io_fail(what, path, e))?;
  Ok(Value::str(String::from_utf8_lossy(&bytes).into_owned()))
}

/// `file.write(path, data)`: `data` as text, a value that is not text as JSON.
fn write(what: &str, ambient: &Ambient, args: &[Value]) -> Result<Value, Fail> {
  let path = text(what, args, 0)?;
  let target = placed(what, ambient, args)?;
  let data = match args.get(1) {
    Some(Value::Str(s)) => s.to_string(),
    Some(other) => crate::interp::stringify(other).unwrap_or_default(),
    None => String::new(),
  };
  std::fs::write(target, data).map_err(|e| io_fail(what, path, e))?;
  Ok(Value::Null)
}

/// `file.makeDir(path, { recursive }?)`: the directory, its parents too with `recursive`.
fn make_dir(what: &str, ambient: &Ambient, args: &[Value]) -> Result<Value, Fail> {
  let path = text(what, args, 0)?;
  let target = placed(what, ambient, args)?;
  let recursive = option(what, args, 1, "recursive")?.is_some_and(crate::interp::truthy);
  let made = if recursive { std::fs::create_dir_all(target) } else { std::fs::create_dir(target) };
  made.map_err(|e| io_fail(what, path, e))?;
  Ok(Value::Null)
}

/// `file.list(path)`: the entries' names, sorted.
fn list(what: &str, ambient: &Ambient, args: &[Value]) -> Result<Value, Fail> {
  let path = text(what, args, 0)?;
  let entries = std::fs::read_dir(placed(what, ambient, args)?).map_err(|e| io_fail(what, path, e))?;
  let mut names: Vec<String> = entries.filter_map(|e| e.ok()).map(|e| e.file_name().to_string_lossy().into_owned()).collect();
  names.sort();
  Ok(Value::Seq(names.into_iter().map(Value::str).collect()))
}

/// `file.stat(path)`: `{ isFile, isDirectory, size, mtimeMs }`.
fn stat(what: &str, ambient: &Ambient, args: &[Value]) -> Result<Value, Fail> {
  let path = text(what, args, 0)?;
  let meta = std::fs::metadata(placed(what, ambient, args)?).map_err(|e| io_fail(what, path, e))?;
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
  fn a_missing_file_fails_with_a_throwable_carrying_enoent() {
    let fail = read("file.read", &Ambient::default(), &[Value::str("/nowhere/at/all.txt")]).unwrap_err();
    let Some(thrown) = fail.thrown else { panic!("a thrown value") };
    let Value::Map(map) = *thrown else { panic!("a map") };
    assert_eq!(map.get("code"), Some(&Value::str("ENOENT")));
  }
}
