//! Every react.dev example's first HTML held against React's `renderToString` in `first_html.txt`: `same`, `equivalent` once FSR's own markers are taken out, `differs`, or why one side did not render. One test per example, so a runner such as nextest runs them in parallel; `--list` and `<name> --exact` are what it calls. `CORPUS_BLESS=1` writes each example's line from this run. `CORPUS_DUMP=<dir>` writes both renders there.

use std::collections::BTreeMap;
use std::fs::File;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use corpus::{app_of, copy};
use snapfire_fsr_cli::{build, Options};
use snapfire_fsr_ir::render::{prepare, Components};

fn root() -> &'static Path {
  Path::new(env!("CARGO_MANIFEST_DIR"))
}

fn examples() -> Vec<String> {
  let mut names: Vec<String> = std::fs::read_dir(root().join("react_dev")).unwrap().flatten().filter(|e| e.path().is_dir()).map(|e| e.file_name().to_string_lossy().into_owned()).collect();
  names.sort();
  names
}

fn held() -> BTreeMap<String, String> {
  std::fs::read_to_string(root().join("first_html.txt")).unwrap_or_default().lines().filter_map(|line| line.split_once('\t')).map(|(name, verdict)| (name.to_owned(), verdict.to_owned())).collect()
}

/// `html` without the `<link rel="preload">` hints React 19 writes before the element they serve, which a document carries in its head.
fn without_preloads(html: &str) -> String {
  let mut out = String::with_capacity(html.len());
  let mut rest = html;
  while let Some(at) = rest.find("<link rel=\"preload\"") {
    out.push_str(&rest[..at]);
    rest = rest[at..].split_once('>').map_or("", |(_, after)| after);
  }
  out.push_str(rest);
  out
}

/// React's engine for `app`; the first one made fetches `react-dom/server` into `cache` while the others wait for it.
fn react_engine(app: &Path, built: &snapfire_fsr_cli::Built, cache: &Path, rt: &tokio::runtime::Runtime) -> Result<snapfire_fsr_engine::Engine, String> {
  let ready = cache.with_extension("ready");
  if !ready.exists() {
    std::fs::create_dir_all(cache.parent().unwrap()).map_err(|e| e.to_string())?;
    let lock = File::create(cache.with_extension("lock")).map_err(|e| e.to_string())?;
    lock.lock().map_err(|e| e.to_string())?;
    if !ready.exists() {
      let engine = patterns::react::react_engine(app, built, cache, rt)?;
      std::fs::write(&ready, "").map_err(|e| e.to_string())?;
      return Ok(engine);
    }
  }
  patterns::react::react_engine(app, built, cache, rt)
}

/// How long one example may take; the engine's clock stands still, so a render that waits for it never returns.
const BUDGET: std::time::Duration = std::time::Duration::from_secs(20);

fn first_html(name: &str) -> String {
  let scratch = std::env::temp_dir().join(format!("fsr-corpus-html-{}", std::process::id()));
  let app = app_of(root(), &root().join("react_dev").join(name), &scratch);
  let (send, receive) = std::sync::mpsc::channel();
  let worker = app.clone();
  std::thread::spawn(move || send.send(verdict(&worker)));
  let mut verdict = receive.recv_timeout(BUDGET).unwrap_or_else(|_| "times out".to_owned());
  // The temp directory differs per machine and per run, an error names it resolved through `/private` on macOS and the engine cuts a long message inside the path.
  let temp = std::env::temp_dir();
  for (spelled, with) in [(scratch.canonicalize().ok(), "<scratch>"), (Some(scratch.clone()), "<scratch>"), (temp.canonicalize().ok(), "<temp>"), (Some(temp), "<temp>")] {
    if let Some(spelled) = spelled {
      verdict = verdict.replace(spelled.to_string_lossy().trim_end_matches('/'), with);
    }
  }
  let _ = std::fs::remove_dir_all(&scratch);
  verdict
}

fn verdict(app: &Path) -> String {
  let Ok(built) = build(app, &Options::default()) else { return "refused".to_owned() };
  let components: Arc<Components> = Arc::new(built.manifest.components.iter().map(|c| (c.module.clone(), Arc::new(prepare(&c.body)))).collect());
  let Some(page) = components.get(patterns::PAGE).cloned() else { return "no page".to_owned() };
  let ir = match snapfire_fsr_ir::Interpreter::default().with_consts(Some(Arc::new(built.manifest.consts.clone()))).render(&page, &snapfire_fsr_core::ValueMap::default(), &components) {
    Ok(rendered) => patterns::flatten(&rendered),
    Err(e) => return format!("ir fails: {}", e.message.lines().next().unwrap_or_default()),
  };
  for package in ["react", "react-dom"] {
    copy(&patterns::root().join("template/app/vendor").join(package), &app.join("vendor").join(package));
  }
  let cache = Path::new(env!("CARGO_TARGET_TMPDIR")).join("react-dom-server");
  let rt = tokio::runtime::Builder::new_current_thread().build().unwrap();
  let react = match react_engine(app, &built, &cache, &rt).and_then(|engine| engine.eval_string("__render({})").map_err(|e| format!("{e}"))) {
    Ok(html) => without_preloads(&html),
    Err(e) => return format!("react fails: {}", e.lines().next().unwrap_or_default()),
  };
  if let Some(dir) = std::env::var_os("CORPUS_DUMP") {
    let name = app.parent().and_then(|p| p.file_name()).unwrap_or_default().to_string_lossy().into_owned();
    let _ = std::fs::create_dir_all(&dir);
    let _ = std::fs::write(Path::new(&dir).join(format!("{name}.ir.html")), &ir);
    let _ = std::fs::write(Path::new(&dir).join(format!("{name}.react.html")), &react);
  }
  if patterns::normalised(&ir) == patterns::normalised(&react) {
    "same".to_owned()
  } else if patterns::normalised(&patterns::without_fsr_markers(&ir)) == patterns::normalised(&react) {
    "equivalent".to_owned()
  } else {
    "differs".to_owned()
  }
}

/// `name`'s line in `first_html.txt` set to `verdict`, under a lock since every example writes the one file.
fn bless(name: &str, verdict: &str) {
  let path: PathBuf = root().join("first_html.txt");
  let lock = File::create(Path::new(env!("CARGO_TARGET_TMPDIR")).join("first_html.lock")).unwrap();
  lock.lock().unwrap();
  let mut lines = held();
  lines.insert(name.to_owned(), verdict.to_owned());
  let text: String = lines.iter().map(|(name, verdict)| format!("{name}\t{verdict}\n")).collect();
  std::fs::write(path, text).unwrap();
}

fn main() {
  let args: Vec<String> = std::env::args().skip(1).collect();
  if args.iter().any(|a| a == "--list") {
    if !args.iter().any(|a| a == "--ignored") {
      for name in examples() {
        println!("{name}: test");
      }
    }
    return;
  }
  let exact = args.iter().any(|a| a == "--exact");
  let filter = args.iter().find(|a| !a.starts_with('-'));
  let chosen: Vec<String> = examples().into_iter().filter(|name| filter.is_none_or(|f| if exact { name == f } else { name.contains(f.as_str()) })).collect();
  let blessing = std::env::var_os("CORPUS_BLESS").is_some_and(|v| v == "1");
  let held = held();
  let mut failed = 0;
  for name in &chosen {
    let now = first_html(name);
    if blessing {
      bless(name, &now);
      println!("test {name} ... ok");
      continue;
    }
    match held.get(name) {
      Some(was) if *was == now => println!("test {name} ... ok"),
      was => {
        failed += 1;
        println!("test {name} ... FAILED\n  was {}\n  now {now}\n  CORPUS_BLESS=1 takes this run", was.map_or("(not held)", String::as_str));
      }
    }
  }
  println!("test result: {}. {} passed; {failed} failed", if failed == 0 { "ok" } else { "FAILED" }, chosen.len() - failed);
  if failed > 0 {
    std::process::exit(101);
  }
}
