//! The browser half of each `render` member that computes in JavaScript, the client's built `std.js` run in QuickJS, against the Rust half on the same arguments: `path` over generated paths, `text`, `time` and `crypto` over fixed cases. The `Intl` forms are checked against Chrome's own output in `ir/tests/intl_browser.rs`.

use proptest::prelude::*;
use rquickjs::loader::{ImportAttributes, Loader, Resolver};
use rquickjs::{Context, Ctx, Module, Object, Persistent, Runtime};
use snapfire_fsr_core::Value;
use snapfire_fsr_ir::{Ambient, Extensions};

/// An absolute name as given, a relative one against the importing file's directory.
struct Files;

impl Resolver for Files {
  fn resolve<'js>(&mut self, _ctx: &Ctx<'js>, base: &str, name: &str, _attributes: Option<ImportAttributes<'js>>) -> rquickjs::Result<String> {
    if name.starts_with('/') {
      return Ok(name.to_owned());
    }
    let dir = std::path::Path::new(base).parent().unwrap_or(std::path::Path::new("/"));
    Ok(dir.join(name.trim_start_matches("./")).to_string_lossy().into_owned())
  }
}

impl Loader for Files {
  fn load<'js>(&mut self, ctx: &Ctx<'js>, name: &str, _attributes: Option<ImportAttributes<'js>>) -> rquickjs::Result<Module<'js>> {
    let source = std::fs::read_to_string(name).unwrap_or_else(|e| panic!("{name}: {e}"));
    Module::declare(ctx.clone(), name, source)
  }
}

/// An argument both halves take.
#[derive(Clone, Copy)]
enum Arg<'a> {
  Str(&'a str),
  Num(f64),
}

impl Arg<'_> {
  fn js(&self) -> String {
    match self {
      Arg::Str(s) => format!("\"{}\"", s.replace('\\', "\\\\").replace('"', "\\\"").replace('\n', "\\n")),
      Arg::Num(n) => format!("{n:?}"),
    }
  }

  fn value(&self) -> Value {
    match self {
      Arg::Str(s) => Value::str(*s),
      Arg::Num(n) => Value::F64(*n),
    }
  }
}

/// QuickJS with the embedded client's `std.js` imported and held as `globalThis.__std`.
struct Browser {
  context: Context,
  /// Dropped last: QuickJS frees the context and every handle into it before its runtime.
  runtime: Runtime,
}

impl Browser {
  fn new() -> Self {
    let std_js = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../host/embedded/client/std.js").canonicalize().unwrap();
    let runtime = Runtime::new().unwrap();
    runtime.set_loader(Files, Files);
    let context = Context::full(&runtime).unwrap();
    // Bare QuickJS has no TextEncoder; this one writes the UTF-8 bytes a browser's does.
    context.with(|ctx| ctx.eval::<(), _>("globalThis.TextEncoder = class { encode(s) { return Uint8Array.from(unescape(encodeURIComponent(s)), (c) => c.charCodeAt(0)); } };").unwrap());
    let promise = context.with(|ctx| Persistent::save(&ctx, Module::import(&ctx, std_js.to_str().unwrap()).unwrap()));
    while runtime.is_job_pending() {
      runtime.execute_pending_job().unwrap();
    }
    context.with(|ctx| {
      let namespace: Object = promise.restore(&ctx).unwrap().result::<Object>().expect("settled").unwrap_or_else(|_| panic!("importing std.js: {:?}", ctx.catch()));
      ctx.globals().set("__std", namespace).unwrap();
    });
    Self { context, runtime }
  }

  /// `module.name(args)` as text: a string as itself, `null` as `null`, anything else through `String`.
  fn call(&self, module: &str, name: &str, args: &[Arg]) -> String {
    let _ = &self.runtime;
    let list = args.iter().map(Arg::js).collect::<Vec<_>>().join(", ");
    let source = format!("(() => {{ const v = __std.{module}.{name}({list}); return v === null ? \"null\" : String(v); }})()");
    self.context.with(|ctx| ctx.eval::<String, _>(source.as_str()).unwrap_or_else(|_| panic!("{module}.{name}({list}): {:?}", ctx.catch())))
  }
}

fn rust(module: &str, name: &str, args: &[Arg]) -> String {
  let values: Vec<Value> = args.iter().map(Arg::value).collect();
  match Extensions::standard().call(&format!("{module}.{name}"), &Ambient::default(), &values).unwrap_or_else(|e| panic!("{module}.{name}: {}", e.message)) {
    Value::Bool(b) => b.to_string(),
    Value::Str(s) => s.to_string(),
    Value::Null => "null".to_owned(),
    Value::Int(n) => n.to_string(),
    Value::F64(f) if f.fract() == 0.0 && f.abs() < 1e21 => format!("{}", f as i64),
    Value::F64(f) => f.to_string(),
    other => panic!("{other:?}"),
  }
}

thread_local! {
  static BROWSER: Browser = Browser::new();
}

fn browser(module: &str, name: &str, args: &[Arg]) -> String {
  BROWSER.with(|b| b.call(module, name, args))
}

fn a_path() -> impl Strategy<Value = String> {
  prop::collection::vec(prop_oneof![Just(""), Just("."), Just(".."), Just("a"), Just("b.md"), Just(".env"), Just("x.tar.gz")], 0..6)
    .prop_flat_map(|parts| (Just(parts), any::<bool>(), any::<bool>()))
    .prop_map(|(parts, lead, trail)| format!("{}{}{}", if lead { "/" } else { "" }, parts.join("/"), if trail { "/" } else { "" }))
}

fn agree(name: &str, args: &[&str]) -> Result<(), TestCaseError> {
  let args: Vec<Arg> = args.iter().map(|a| Arg::Str(a)).collect();
  prop_assert_eq!(browser("path", name, &args), rust("path", name, &args), "path.{}({:?})", name, args.iter().map(Arg::js).collect::<Vec<_>>());
  Ok(())
}

proptest! {
  #[test]
  fn each_pure_path_member_answers_in_the_browser_as_on_the_server(a in a_path(), b in a_path(), ext in prop_oneof![Just(".md"), Just(".gz"), Just(""), Just("b.md")]) {
    agree("normalize", &[&a])?;
    agree("dirname", &[&a])?;
    agree("basename", &[&a])?;
    agree("basename", &[&a, ext])?;
    agree("extname", &[&a])?;
    agree("isAbsolute", &[&a])?;
    agree("join", &[&a, &b])?;
    agree("join", &[])?;
  }
}

#[test]
fn the_path_edges_agree() {
  for p in ["", "/", "//", "//b", "a//b/", "/a/../..", "../../a", ".", "./", "a/..", "/.hidden", "a.b/c"] {
    for name in ["normalize", "dirname", "basename", "extname", "isAbsolute"] {
      assert_eq!(browser("path", name, &[Arg::Str(p)]), rust("path", name, &[Arg::Str(p)]), "path.{name}({p:?})");
    }
  }
}

#[test]
fn text_time_and_crypto_answer_in_the_browser_as_on_the_server() {
  use Arg::{Num, Str};
  let day = 20_701.0 * 86_400_000.0 + 60_067_250.0;
  let long = "a".repeat(200);
  let cases: Vec<(&str, &str, Vec<Arg>)> = vec![
    ("text", "slug", vec![Str("  Crème Brûlée & Café! ")]),
    ("text", "slug", vec![Str("Ünïcödé--stuff__2026 ß Straße")]),
    ("text", "truncate", vec![Str("héllo wörld 🌍!"), Num(13.0)]),
    ("text", "truncate", vec![Str("short"), Num(10.0)]),
    ("time", "format", vec![Num(day), Str("YYYY-MM-DD HH:mm:ss.SSS")]),
    ("time", "add", vec![Num(day), Num(36.0), Str("h")]),
    ("time", "diff", vec![Num(day), Num(0.0), Str("d")]),
    ("time", "parse", vec![Str("2026-09-05T16:41:07.250Z")]),
    ("time", "parse", vec![Str("2026-09-05T18:41+02:00")]),
    ("time", "parse", vec![Str("2026-09-05")]),
    ("time", "parse", vec![Str("Sep 5 2026")]),
    ("crypto", "hash", vec![Str("hello")]),
    ("crypto", "hash", vec![Str("")]),
    ("crypto", "hash", vec![Str(&long)]),
    ("crypto", "verify", vec![Str("hello"), Str("2CF24DBA5FB0A30E26E83B2AC5B9E29E1B161E5C1FA7425E73043362938B9824")]),
    ("crypto", "verify", vec![Str("hello"), Str("nope")]),
    ("crypto", "digest", vec![Str("sha256"), Str("hex"), Str("abc")]),
    ("crypto", "digest", vec![Str("SHA-256"), Str("base64"), Str("ab"), Str("c")]),
  ];
  let mut wrong = Vec::new();
  for (module, name, args) in &cases {
    let (js, rs) = (browser(module, name, args), rust(module, name, args));
    if js != rs {
      wrong.push(format!("{module}.{name}({}): browser {js:?}, rust {rs:?}", args.iter().map(Arg::js).collect::<Vec<_>>().join(", ")));
    }
  }
  assert!(wrong.is_empty(), "{}", wrong.join("\n"));
}
