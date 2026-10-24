//! The browser half of each pure `path` member, the client's built `std.js` run in QuickJS, against the Rust half on the same paths.

use proptest::prelude::*;
use rquickjs::loader::{ImportAttributes, Loader, Resolver};
use rquickjs::{Context, Ctx, Function, Module, Object, Persistent, Runtime};
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

/// QuickJS with the embedded client's `std.js` imported and its `path` object kept.
struct Browser {
  path: Persistent<Object<'static>>,
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
    let promise = context.with(|ctx| Persistent::save(&ctx, Module::import(&ctx, std_js.to_str().unwrap()).unwrap()));
    while runtime.is_job_pending() {
      runtime.execute_pending_job().unwrap();
    }
    let path = context.with(|ctx| {
      let namespace: Object = promise.restore(&ctx).unwrap().result::<Object>().expect("settled").unwrap_or_else(|_| panic!("importing std.js: {:?}", ctx.catch()));
      Persistent::save(&ctx, namespace.get::<_, Object>("path").unwrap())
    });
    Self { path, context, runtime }
  }

  fn call(&self, name: &str, args: &[&str]) -> String {
    let _ = &self.runtime;
    self.context.with(|ctx| {
      let path = self.path.clone().restore(&ctx).unwrap();
      let f: Function = path.get(name).unwrap();
      let args: Vec<rquickjs::Value> = args.iter().map(|a| rquickjs::String::from_str(ctx.clone(), a).unwrap().into_value()).collect();
      let mut call = rquickjs::function::Args::new(ctx.clone(), args.len());
      call.this(path.clone()).unwrap();
      for a in args {
        call.push_arg(a).unwrap();
      }
      let out: rquickjs::Value = f.call_arg(call).unwrap();
      match out.as_bool() {
        Some(b) => b.to_string(),
        None => out.as_string().unwrap().to_string().unwrap(),
      }
    })
  }
}

fn rust(name: &str, args: &[&str]) -> String {
  let values: Vec<Value> = args.iter().map(|a| Value::str(*a)).collect();
  match Extensions::standard().call(&format!("path.{name}"), &Ambient::default(), &values).unwrap() {
    Value::Bool(b) => b.to_string(),
    Value::Str(s) => s.to_string(),
    other => panic!("{other:?}"),
  }
}

fn a_path() -> impl Strategy<Value = String> {
  prop::collection::vec(prop_oneof![Just(""), Just("."), Just(".."), Just("a"), Just("b.md"), Just(".env"), Just("x.tar.gz")], 0..6)
    .prop_flat_map(|parts| (Just(parts), any::<bool>(), any::<bool>()))
    .prop_map(|(parts, lead, trail)| format!("{}{}{}", if lead { "/" } else { "" }, parts.join("/"), if trail { "/" } else { "" }))
}

thread_local! {
  static BROWSER: Browser = Browser::new();
}

fn agree(name: &str, args: &[&str]) -> Result<(), TestCaseError> {
  let browser = BROWSER.with(|b| b.call(name, args));
  prop_assert_eq!(browser, rust(name, args), "path.{}({:?})", name, args);
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
fn the_edges_agree() {
  for p in ["", "/", "//", "//b", "a//b/", "/a/../..", "../../a", ".", "./", "a/..", "/.hidden", "a.b/c"] {
    for name in ["normalize", "dirname", "basename", "extname", "isAbsolute"] {
      assert_eq!(BROWSER.with(|b| b.call(name, &[p])), rust(name, &[p]), "path.{name}({p:?})");
    }
  }
}
