//! What a name in a module is, resolved once per file: a construct a library
//! exports, a context the application created, that context's provider, the
//! module's own binding or an import the build does not read. A site that
//! recognises a construct asks what an expression means, so every spelling
//! TypeScript allows for reaching it (an alias, a namespace, a re-export, a
//! module-level `const`) is one meaning, and a local that shadows an import is
//! none.

use std::collections::HashMap;
use std::rc::Rc;

use snapfire_fsr_ir::ast::Expr;
use swc_core::common::SyntaxContext;
use swc_core::ecma::ast as js;

/// Which of FSR's standard-library roots a module object or member was imported from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Side {
  /// `@snapfire/fsr/client`: what a server render and the browser both run.
  Client,
  /// `@snapfire/fsr/host`: what every host body runs.
  Host,
  /// `@snapfire/fsr/terminal`: what only a terminal program runs.
  Terminal,
}

impl Side {
  pub(crate) fn root(self) -> &'static str {
    match self {
      Side::Client => CLIENT_ROOT,
      Side::Host => HOST_ROOT,
      Side::Terminal => TERMINAL_ROOT,
    }
  }

  /// Whether a standard member of `module` with `reach` is under this side.
  pub(crate) fn holds(self, module: &str, reach: snapfire_fsr_ir::Reach) -> bool {
    use snapfire_fsr_ir::Reach;
    let terminal_module = TERMINAL_STD_MODULES.contains(&module);
    match self {
      Side::Client => !terminal_module && module != "terminal" && reach == Reach::Render,
      Side::Host => !terminal_module && module != "terminal" && matches!(reach, Reach::Render | Reach::Body),
      Side::Terminal => terminal_module,
    }
  }
}

pub const CLIENT_ROOT: &str = "@snapfire/fsr/client";
pub const HOST_ROOT: &str = "@snapfire/fsr/host";
pub const TERMINAL_ROOT: &str = "@snapfire/fsr/terminal";
/// The standard modules only a terminal program has, under [`TERMINAL_ROOT`].
const TERMINAL_STD_MODULES: [&str; 2] = ["system", "clipboard"];
/// Members a side's module has that are not extensions: `system.spawn` is an act the terminal runs.
const SIDE_ACTS: [(Side, &str, &str); 1] = [(Side::Terminal, "system", "spawn")];

/// Whether `module.name` is under `side`.
fn side_has(side: Side, module: &str, name: &str) -> Option<(&'static str, &'static str)> {
  let member = snapfire_fsr_ir::STANDARD.iter().find(|m| m.module == module && m.name == name && side.holds(module, m.reach)).map(|m| (m.module, m.name));
  member.or_else(|| SIDE_ACTS.iter().find(|(s, m, n)| *s == side && *m == module && *n == name).map(|(_, m, n)| (*m, *n)))
}

/// Each standard module under `side`, with the members it holds there.
pub(crate) fn side_modules(side: Side) -> Vec<(&'static str, Vec<&'static str>)> {
  let mut out: Vec<(&'static str, Vec<&'static str>)> = Vec::new();
  for member in snapfire_fsr_ir::STANDARD {
    if !side.holds(member.module, member.reach) {
      continue;
    }
    match out.iter_mut().find(|(m, _)| *m == member.module) {
      Some((_, names)) => names.push(member.name),
      None => out.push((member.module, vec![member.name])),
    }
  }
  for (_, module, name) in SIDE_ACTS.iter().filter(|(s, _, _)| *s == side) {
    if let Some((_, names)) = out.iter_mut().find(|(m, _)| m == module) {
      names.push(name);
    }
  }
  out
}

/// `<root>/<module>` for every side and module, made once.
fn module_paths() -> &'static [(Side, &'static str, &'static str)] {
  static PATHS: std::sync::OnceLock<Vec<(Side, &'static str, &'static str)>> = std::sync::OnceLock::new();
  PATHS.get_or_init(|| {
    let mut paths = Vec::new();
    for side in [Side::Client, Side::Host, Side::Terminal] {
      for (module, _) in side_modules(side) {
        let path: &'static str = Box::leak(format!("{}/{module}", side.root()).into_boxed_str());
        paths.push((side, module, path));
      }
    }
    paths
  })
}

fn module_path(side: Side, module: &'static str) -> &'static str {
  module_paths().iter().find(|(s, m, _)| *s == side && *m == module).map(|(_, _, path)| *path).expect("a module some side holds")
}

/// The side and module a `<root>/<module>` specifier names.
pub(crate) fn side_module_of(source: &str) -> Option<(Side, &'static str)> {
  module_paths().iter().find(|(_, _, path)| *path == source).map(|(side, module, _)| (*side, *module))
}

/// Why `module.name` is not under `side` and where it is, for a refusal.
pub(crate) fn absent(side: Side, module: &str, name: &str) -> (String, Option<String>) {
  let key = format!("{module}.{name}");
  let Some(member) = snapfire_fsr_ir::STANDARD.iter().find(|m| m.module == module && m.name == name) else {
    return (format!("`{key}` is not a member of the standard library"), None);
  };
  let home = [Side::Client, Side::Host, Side::Terminal].into_iter().find(|s| s.holds(module, member.reach));
  match home {
    Some(Side::Host) if side == Side::Client => (
      format!("`{key}` is not in `{CLIENT_ROOT}`: it runs only in a host. Read it in the route's loader `page.loader.ts` from `{HOST_ROOT}` and pass it to the page as a prop"),
      Some(format!("import {{ {module} }} from \"{HOST_ROOT}\";\nexport async function load() {{\n  return {{ value: {key}(/* ... */) }};\n}}")),
    ),
    Some(home) => (format!("`{key}` is not in `{}`; it is in `{}`", side.root(), home.root()), None),
    None => (format!("`{key}` is not in `{}`", side.root()), None),
  }
}

/// A library construct the build lowers, under the name its library exports it as.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Construct {
  /// A standard module as a whole, imported from a side's root (`import { intl } from "@snapfire/fsr/client"`) or as a namespace (`import * as intl from "@snapfire/fsr/client/intl"`).
  StdModule(Side, &'static str),
  /// One standard member, `module.name`, imported by name from a side.
  StdMember(Side, &'static str, &'static str),
  /// A hook, by React's name for it: `useState`, `useContext`, `use`.
  Hook(&'static str),
  /// `createContext`; `react` when it is React's, whose providers the browser mounts.
  CreateContext { react: bool },
  /// A placement the build lowers itself: `Island`, `island`, `Link`, `Slot`, `Picture`, `Mount`, `Embed` or the retired `tree`.
  Placement(&'static str),
  /// The store's `key`, which names a store key by its string.
  StoreKey,
  /// A store key the client exports, by the string it holds: `online` and `updateReady`.
  NamedKey(&'static str),
  /// The store's `derive`, which computes a key from its sources.
  Derive,
  /// A call the store exports that a handler may make: `set`, `clear`, `transaction` or `optimistic`.
  StoreCall(&'static str),
  /// `action` or `fail` from the server tier.
  Server(&'static str),
  /// The client's `action("id")`, a handler's reference to an action by its id.
  ClientAction,
  /// An export of the standard library: `t`, `native`, `localeTag` or a member module such as `text`.
  Std(&'static str),
  /// A Vue composition function the Vue front end lowers: `ref`, `computed`, `provide`, `inject` and the rest.
  Vue(&'static str),
  /// The `actions` object of the client module the build generates, whose property paths are action ids.
  GeneratedActions,
  /// React's `lazy` or Vue's `defineAsyncComponent`: a component the browser loads on demand, which the server renders like any other since the build knows the module.
  AsyncComponent,
  /// `clsx` or `classnames`, which join class names.
  ClassNames,
  /// A package export the build answers with a member of the standard library, `terminal.ms` for `ms`'s default export: a call to it is a call to that member, under the member's reach.
  Extension(&'static str),
  /// The terminal primitive `emit(kind, value)`, `Builtin::Emit`.
  Emit,
  /// The terminal primitive `suspendTerminal(f)`.
  Suspend,
  /// React's `Children`, of which the build reads the count of a component's own children.
  Children,
  /// React's `cloneElement`.
  CloneElement,
  /// React's `isValidElement`.
  IsValidElement,
  /// React's `Suspense` or `Activity`, which the server writes as a boundary around its children.
  Boundary(&'static str),
  /// `svelte/store`'s `writable`, `readable`, `readonly`, `derived` or `get`, `svelte/motion`'s `spring` or `tweened`, `svelte/transition`'s `crossfade` or `svelte`'s `createEventDispatcher`, `getContext` and `setContext`: a store is its value on the server.
  SvelteStore(&'static str),
  /// A class of `svelte/reactivity` or `svelte/motion`, named by what it is on the server: `Set`, `Map`, `Date` and `URLSearchParams` the built-ins Svelte's server entry exports, `Spring` and `Tween` an object holding its value as `current` and `target`.
  SvelteClass(&'static str),
}

/// Where the placements are exported: the template dialect and each client adapter, which offer the same ones.
const PLACEMENT_MODULES: [&str; 4] = ["@snapfire/fsr-authoring/template", "@snapfire/fsr-client/react", "@snapfire/fsr-client/vue", "@snapfire/fsr-client/jsx-runtime"];
const PLACEMENTS: [&str; 9] = ["Island", "island", "Link", "Slot", "Picture", "Mount", "Embed", "Place", "tree"];
/// FSR's own components, which an application imports in every framework; the build points the specifier at the importing file's framework.
pub const COMPONENTS_MODULE: &str = "@snapfire/fsr/components";

/// The module whose `config` and named exports are the deployment's `[public]` values, read anywhere.
pub const CONFIG_MODULE: &str = "@snapfire/fsr/config";
const COMPONENTS: [&str; 5] = ["Link", "Picture", "Mount", "Embed", "Place"];
/// The modules FSR's terminal primitives are imported from: FSR's Ink and the primitives it is written over.
const TERMINAL_MODULES: [&str; 2] = [crate::terminal::INK_MODULE, "@snapfire/fsr/terminal/core"];

use snapfire_fsr_ir::{Field, Signature, Ty};

const ELEMENT: Ty = Ty::Named("ReactElement");

/// The terminal's primitives: what each name lowers to and the signatures it is declared with, the props types `BoxProps`, `TextProps` and `Key` written beside them.
const TERMINAL_PRIMITIVES: &[(&str, Construct, &[Signature])] = &[
  ("Box", Construct::Placement("Box"), &[Signature { doc: "Ink's `Box`: lays its children out with flexbox and draws its border.", generics: &[], params: &[Field::required("props", Ty::Named("BoxProps"))], returns: ELEMENT }]),
  ("Text", Construct::Placement("Text"), &[Signature { doc: "Ink's `Text`: draws its text and nested `Text` in its style.", generics: &[], params: &[Field::required("props", Ty::Named("TextProps"))], returns: ELEMENT }]),
  ("Newline", Construct::Placement("Newline"), &[Signature { doc: "`count` line breaks inside a `Text`, one unless given.", generics: &[], params: &[Field::required("props", Ty::Object(&[Field::optional("count", Ty::Number)]))], returns: ELEMENT }]),
  ("Spacer", Construct::Placement("Spacer"), &[Signature { doc: "A box that grows to fill the space its parent leaves.", generics: &[], params: &[], returns: ELEMENT }]),
  (
    "useInput",
    Construct::Hook("useInput"),
    &[Signature {
      doc: "Runs `handler` on each key the terminal reads while `isActive` holds.",
      generics: &[],
      params: &[
        Field::required("handler", Ty::Function(&[Field::required("input", Ty::String), Field::required("key", Ty::Named("Key"))], &Ty::Void)),
        Field::optional("options", Ty::Object(&[Field::optional("isActive", Ty::Boolean)])),
      ],
      returns: Ty::Void,
    }],
  ),
  ("emit", Construct::Emit, &[Signature { doc: "Hands the terminal `value` under `key` for this render.", generics: &[], params: &[Field::required("key", Ty::String), Field::required("value", Ty::Unknown)], returns: Ty::Void }]),
  (
    "suspendTerminal",
    Construct::Suspend,
    &[Signature { doc: "Gives the terminal up while the promise `run` returns is pending, then takes it back.", generics: &["T"], params: &[Field::required("run", Ty::Function(&[], &Ty::Promise(&Ty::Named("T"))))], returns: Ty::Promise(&Ty::Named("T")) }],
  ),
  (
    "useAnimation",
    Construct::Hook("useAnimation"),
    &[Signature {
      doc: "A frame count that steps every `interval` milliseconds (100 unless given) while `isActive` holds, with the milliseconds it stands for.",
      generics: &[],
      params: &[Field::optional("options", Ty::Object(&[Field::optional("interval", Ty::Number), Field::optional("isActive", Ty::Boolean)]))],
      returns: Ty::Object(&[Field::required("frame", Ty::Number), Field::required("time", Ty::Number)]),
    }],
  ),
];

/// Each terminal primitive's name with the signatures it is declared with.
pub fn terminal_primitives() -> impl Iterator<Item = (&'static str, &'static [Signature])> {
  TERMINAL_PRIMITIVES.iter().map(|(name, _, signatures)| (*name, *signatures))
}

fn rows() -> &'static [(&'static str, &'static str, Construct)] {
  static ROWS: std::sync::OnceLock<Vec<(&'static str, &'static str, Construct)>> = std::sync::OnceLock::new();
  ROWS.get_or_init(|| {
    let mut rows: Vec<(&'static str, &'static str, Construct)> = LIBRARY.to_vec();
    for module in PLACEMENT_MODULES {
      rows.extend(PLACEMENTS.iter().map(|name| (module, *name, Construct::Placement(name))));
    }
    rows.extend(COMPONENTS.iter().map(|name| (COMPONENTS_MODULE, *name, Construct::Placement(name))));
    for module in TERMINAL_MODULES {
      rows.extend(TERMINAL_PRIMITIVES.iter().map(|(name, construct, _)| (module, *name, *construct)));
    }
    for side in [Side::Client, Side::Host, Side::Terminal] {
      for (module, names) in side_modules(side) {
        rows.push((side.root(), module, Construct::StdModule(side, module)));
        let path = module_path(side, module);
        rows.extend(names.iter().map(|name| (path, *name, Construct::StdMember(side, module, name))));
      }
    }
    for side in [Side::Client, Side::Host] {
      rows.push((side.root(), "t", Construct::StdMember(side, "i18n", "t")));
    }
    rows.push((CLIENT_ROOT, "localeTag", Construct::Std("localeTag")));
    rows.push((module_path(Side::Client, "intl"), "localeTag", Construct::Std("localeTag")));
    rows.push(("@snapfire/fsr", "native", Construct::Std("native")));
    for server in ["action", "fail", "redirect", "after"] {
      rows.push((HOST_ROOT, server, Construct::Server(server)));
    }
    let react_hooks: Vec<(&'static str, Construct)> = LIBRARY.iter().filter(|(m, _, c)| *m == "react" && matches!(c, Construct::Hook(_))).map(|(_, e, c)| (*e, *c)).collect();
    for module in ["preact/hooks", "preact/compat"] {
      rows.extend(react_hooks.iter().map(|(export, construct)| (module, *export, *construct)));
    }
    rows
  })
}

/// Every library export the build gives a meaning, by module and export name.
pub(crate) const LIBRARY: &[(&str, &str, Construct)] = &[
  ("react", "createContext", Construct::CreateContext { react: true }),
  ("react", "useState", Construct::Hook("useState")),
  ("react", "useReducer", Construct::Hook("useReducer")),
  ("react", "useMemo", Construct::Hook("useMemo")),
  ("react", "useRef", Construct::Hook("useRef")),
  ("react", "useCallback", Construct::Hook("useCallback")),
  ("react", "useContext", Construct::Hook("useContext")),
  ("react", "use", Construct::Hook("use")),
  ("react", "useEffect", Construct::Hook("useEffect")),
  ("react", "useLayoutEffect", Construct::Hook("useLayoutEffect")),
  ("react", "useInsertionEffect", Construct::Hook("useInsertionEffect")),
  ("react", "useDebugValue", Construct::Hook("useDebugValue")),
  ("react", "useImperativeHandle", Construct::Hook("useImperativeHandle")),
  ("react", "useId", Construct::Hook("useId")),
  ("react", "useTransition", Construct::Hook("useTransition")),
  ("react", "useDeferredValue", Construct::Hook("useDeferredValue")),
  ("react", "useSyncExternalStore", Construct::Hook("useSyncExternalStore")),
  ("react", "useOptimistic", Construct::Hook("useOptimistic")),
  ("react", "useActionState", Construct::Hook("useActionState")),
  ("react", "useEffectEvent", Construct::Hook("useEffectEvent")),
  ("react", "lazy", Construct::AsyncComponent),
  ("react-dom", "useFormStatus", Construct::Hook("useFormStatus")),
  ("@snapfire/fsr-client/react", "useStore", Construct::Hook("useStore")),
  ("@snapfire/fsr-client/react", "useLocale", Construct::Hook("useLocale")),
  ("@snapfire/fsr-client/react", "useLinkStatus", Construct::Hook("useLinkStatus")),
  ("@snapfire/fsr-client/react", "useHoisted", Construct::Hook("useHoisted")),
  ("@snapfire/fsr-authoring/template", "createContext", Construct::CreateContext { react: false }),
  ("@snapfire/fsr-authoring/template", "use", Construct::Hook("use")),
  ("@snapfire/fsr-client/vue", "useStore", Construct::Hook("useStore")),
  ("@snapfire/fsr-client/vue", "useLocale", Construct::Hook("useLocale")),
  ("@snapfire/fsr-client/vue", "useLinkStatus", Construct::Hook("useLinkStatus")),
  ("@snapfire/fsr-client/svelte", "useStore", Construct::Hook("useStore")),
  ("@snapfire/fsr-client/svelte", "useLocale", Construct::Hook("useLocale")),
  ("@snapfire/fsr-client/svelte", "useLinkStatus", Construct::Hook("useLinkStatus")),
  ("@snapfire/fsr-client/svelte", "link", Construct::Placement("link")),
  ("@snapfire/fsr-client/svelte", "picture", Construct::Placement("picture")),
  ("@snapfire/fsr-client/svelte", "embed", Construct::Placement("embed")),
  ("@snapfire/fsr-client/svelte", "island", Construct::Placement("island")),
  ("@snapfire/fsr-client/store", "key", Construct::StoreKey),
  ("@snapfire/fsr-client", "key", Construct::StoreKey),
  ("@snapfire/fsr-client", "online", Construct::NamedKey("sf/online")),
  ("@snapfire/fsr-client", "updateReady", Construct::NamedKey("sf/update-ready")),
  ("@snapfire/fsr-client/store", "derive", Construct::Derive),
  ("@snapfire/fsr-client", "derive", Construct::Derive),
  ("@snapfire/fsr-client/store", "set", Construct::StoreCall("set")),
  ("@snapfire/fsr-client", "set", Construct::StoreCall("set")),
  ("@snapfire/fsr-client/store", "clear", Construct::StoreCall("clear")),
  ("@snapfire/fsr-client", "clear", Construct::StoreCall("clear")),
  ("@snapfire/fsr-client/store", "transaction", Construct::StoreCall("transaction")),
  ("@snapfire/fsr-client", "transaction", Construct::StoreCall("transaction")),
  ("@snapfire/fsr-client/store", "optimistic", Construct::StoreCall("optimistic")),
  ("@snapfire/fsr-client", "optimistic", Construct::StoreCall("optimistic")),
  ("@snapfire/fsr-client", "action", Construct::ClientAction),
  ("@snapfire/fsr", "action", Construct::Server("action")),
  ("@snapfire/fsr", "fail", Construct::Server("fail")),
  ("@snapfire/fsr", "redirect", Construct::Server("redirect")),
  ("@snapfire/fsr", "after", Construct::Server("after")),
  ("@snapfire/fsr-client/std", "t", Construct::Std("t")),
  ("@snapfire/fsr-client/std", "native", Construct::Std("native")),
  ("@snapfire/fsr-client/std", "localeTag", Construct::Std("localeTag")),
  ("@snapfire/fsr-client/std", "intl", Construct::Std("intl")),
  ("@snapfire/fsr-client/std", "text", Construct::Std("text")),
  ("@snapfire/fsr-client/std", "time", Construct::Std("time")),
  ("@snapfire/fsr-client/std", "crypto", Construct::Std("crypto")),
  ("@snapfire/fsr-client/std", "id", Construct::Std("id")),
  ("@snapfire/fsr-client/std", "log", Construct::Std("log")),
  ("@snapfire/fsr-client/std", "file", Construct::Std("file")),
  ("@snapfire/fsr-client/std", "path", Construct::Std("path")),
  ("@snapfire/fsr-client/std", "system", Construct::Std("system")),
  ("@snapfire/fsr-client/std", "clipboard", Construct::Std("clipboard")),
  ("vue", "ref", Construct::Vue("ref")),
  ("vue", "shallowRef", Construct::Vue("shallowRef")),
  ("vue", "computed", Construct::Vue("computed")),
  ("vue", "reactive", Construct::Vue("reactive")),
  ("vue", "shallowReactive", Construct::Vue("shallowReactive")),
  ("vue", "readonly", Construct::Vue("readonly")),
  ("vue", "provide", Construct::Vue("provide")),
  ("vue", "inject", Construct::Vue("inject")),
  ("vue", "defineAsyncComponent", Construct::AsyncComponent),
  ("vue", "defineComponent", Construct::Vue("defineComponent")),
  ("react", "Children", Construct::Children),
  ("react", "cloneElement", Construct::CloneElement),
  ("react", "isValidElement", Construct::IsValidElement),
  ("react", "Suspense", Construct::Boundary("Suspense")),
  ("react", "Activity", Construct::Boundary("Activity")),
  ("clsx", "clsx", Construct::ClassNames),
  ("clsx", "default", Construct::ClassNames),
  ("classnames", "default", Construct::ClassNames),
  ("@snapfire/fsr/terminal", "ms", Construct::Extension("terminal.ms")),
  ("@snapfire/fsr/terminal", "stripAnsi", Construct::Extension("terminal.stripAnsi")),
  ("@snapfire/fsr/terminal", "stringWidth", Construct::Extension("terminal.stringWidth")),
  ("@snapfire/fsr/terminal", "wrapAnsi", Construct::Extension("terminal.wrapAnsi")),
  ("@snapfire/fsr/terminal", "bigText", Construct::Extension("terminal.bigText")),
  ("svelte/store", "writable", Construct::SvelteStore("writable")),
  ("svelte/store", "readable", Construct::SvelteStore("readable")),
  ("svelte/store", "readonly", Construct::SvelteStore("readonly")),
  ("svelte/store", "derived", Construct::SvelteStore("derived")),
  ("svelte/store", "get", Construct::SvelteStore("get")),
  ("svelte", "createEventDispatcher", Construct::SvelteStore("createEventDispatcher")),
  ("svelte", "getContext", Construct::SvelteStore("getContext")),
  ("svelte", "setContext", Construct::SvelteStore("setContext")),
  ("svelte/motion", "spring", Construct::SvelteStore("spring")),
  ("svelte/motion", "tweened", Construct::SvelteStore("tweened")),
  ("svelte/motion", "Spring", Construct::SvelteClass("Spring")),
  ("svelte/motion", "Tween", Construct::SvelteClass("Tween")),
  ("svelte/transition", "crossfade", Construct::SvelteStore("crossfade")),
  ("svelte/reactivity", "SvelteSet", Construct::SvelteClass("Set")),
  ("svelte/reactivity", "SvelteMap", Construct::SvelteClass("Map")),
  ("svelte/reactivity", "SvelteDate", Construct::SvelteClass("Date")),
  ("svelte/reactivity", "SvelteURLSearchParams", Construct::SvelteClass("URLSearchParams")),
];

/// Whether `module` is one the table speaks for, so an export it does not list is a library value the build does not read rather than the application's.
pub(crate) fn is_library(module: &str) -> bool {
  rows().iter().any(|(m, _, _)| *m == module)
}

pub(crate) fn library(module: &str, export: &str) -> Option<Construct> {
  rows().iter().find(|(m, e, _)| *m == module && *e == export).map(|(_, _, c)| *c)
}

/// What a library module's namespace holds: each export the table lists.
pub(crate) fn library_namespace(module: &str) -> Rc<HashMap<String, Meaning>> {
  Rc::new(rows().iter().filter(|(m, _, _)| *m == module).map(|(_, e, c)| ((*e).to_owned(), Meaning::Library(*c))).collect())
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Meaning {
  Library(Construct),
  /// A namespace import, or a library's default import read as one: what each member means.
  Namespace(Rc<HashMap<String, Meaning>>),
  /// A context the application created at `file#name`, with its default lowered and whether React's `createContext` made it.
  Context { id: String, default: Expr, react: bool },
  /// The provider of the context `id`, as `X.Provider` or `const P = X.Provider` names it.
  Provider { id: String, react: bool },
  /// A binding the application declares that is none of the above, a component, a helper or a value, by the file that declares it and the name it is declared under there: `default` for a default export.
  Own { file: String, name: String },
  /// The default import of an image or a font, by the specifier it was imported from.
  Asset { source: String },
  /// An import from [`CONFIG_MODULE`]: `config` whole as `None`, one `[public]` key's value by its name.
  Config(Option<String>),
  /// An import from a package the table does not speak for.
  Foreign,
  /// A standard member a side does not have, by the side it was asked of.
  StdAbsent { side: Side, module: &'static str, name: String },
}

impl Meaning {
  /// What `.name` of this means.
  pub(crate) fn member(&self, name: &str) -> Option<Meaning> {
    match self {
      Meaning::Namespace(members) => members.get(name).cloned(),
      Meaning::Library(Construct::StdModule(side, module)) => Some(
        side_has(*side, module, name)
          .map(|(m, n)| Meaning::Library(Construct::StdMember(*side, m, n)))
          .unwrap_or_else(|| Meaning::StdAbsent { side: *side, module, name: name.to_owned() }),
      ),
      Meaning::Context { id, react, .. } if name == "Provider" => Some(Meaning::Provider { id: id.clone(), react: *react }),
      _ => None,
    }
  }

  /// The provider a tag naming this is: a context's own, React 19's `<Theme value>`, or a provider itself.
  pub(crate) fn as_provider(&self) -> Option<(&str, bool)> {
    match self {
      Meaning::Context { id, react, .. } | Meaning::Provider { id, react } => Some((id, *react)),
      _ => None,
    }
  }

  pub(crate) fn hook(&self) -> Option<&'static str> {
    match self {
      Meaning::Library(Construct::Hook(name)) => Some(name),
      _ => None,
    }
  }

  /// The store's handler call this is: `set`, `clear`, `transaction` or `optimistic`.
  pub(crate) fn store_call(&self) -> Option<&'static str> {
    match self {
      Meaning::Library(Construct::StoreCall(name)) => Some(name),
      _ => None,
    }
  }

  /// The standard member and side this names.
  pub(crate) fn std_member(&self) -> Option<(Side, &'static str, &'static str)> {
    match self {
      Meaning::Library(Construct::StdMember(side, module, name)) => Some((*side, module, name)),
      _ => None,
    }
  }

  pub(crate) fn std(&self) -> Option<&'static str> {
    match self {
      Meaning::Library(Construct::Std(name)) => Some(name),
      _ => None,
    }
  }

  pub(crate) fn vue(&self) -> Option<&'static str> {
    match self {
      Meaning::Library(Construct::Vue(name)) => Some(name),
      _ => None,
    }
  }

  /// Where the application declares this: the file and the name there.
  pub(crate) fn origin(&self) -> Option<(&str, &str)> {
    match self {
      Meaning::Own { file, name } => Some((file, name)),
      _ => None,
    }
  }

  pub(crate) fn placement(&self) -> Option<&'static str> {
    match self {
      Meaning::Library(Construct::Placement(name)) => Some(name),
      _ => None,
    }
  }
}

/// The meanings of one file's module-level names and exports.
#[derive(Debug, Default)]
pub(crate) struct Meanings {
  /// The syntax context of the module's top level: an identifier with another names a local or nothing.
  pub(crate) top: SyntaxContext,
  /// The syntax context of a name nothing in the module binds.
  pub(crate) unresolved: SyntaxContext,
  pub(crate) bindings: HashMap<String, Meaning>,
  pub(crate) exports: HashMap<String, Meaning>,
}

impl Meanings {
  /// Whether `callee` is `want` from the server tier, or the bare name with nothing in the module binding it.
  pub(crate) fn names_server(&self, callee: &js::Callee, want: &str) -> bool {
    let js::Callee::Expr(expr) = callee else { return false };
    if self.of_expr(expr).as_ref().is_some_and(|meant| matches!(meant, Meaning::Library(Construct::Server(name)) if *name == want)) {
      return true;
    }
    matches!(&**expr, js::Expr::Ident(id) if id.ctxt == self.unresolved && id.sym.as_ref() == want)
  }

  /// What `id` names where it is written: a module-level name's meaning, `None` for a local or a name nothing binds.
  pub(crate) fn of_ident(&self, id: &js::Ident) -> Option<&Meaning> {
    (id.ctxt == self.top).then(|| self.bindings.get(id.sym.as_ref())).flatten()
  }

  /// Whether `callee` is the standard `system.spawn`: from `@snapfire/fsr/terminal`, its `system` path or the old standard module.
  pub(crate) fn names_spawn(&self, callee: &js::Expr) -> bool {
    if self.of_expr(callee).and_then(|m| m.std_member()).is_some_and(|(_, module, name)| module == "system" && name == "spawn") {
      return true;
    }
    let js::Expr::Member(member) = bare(callee) else { return false };
    let js::MemberProp::Ident(prop) = &member.prop else { return false };
    prop.sym.as_ref() == "spawn" && self.of_expr(&member.obj) == Some(Meaning::Library(Construct::Std("system")))
  }

  pub(crate) fn of_expr(&self, expr: &js::Expr) -> Option<Meaning> {
    match expr {
      js::Expr::Ident(id) => self.of_ident(id).cloned(),
      js::Expr::Member(member) => {
        let js::MemberProp::Ident(prop) = &member.prop else { return None };
        self.of_expr(&member.obj)?.member(prop.sym.as_ref())
      }
      js::Expr::Paren(p) => self.of_expr(&p.expr),
      js::Expr::TsAs(e) => self.of_expr(&e.expr),
      js::Expr::TsNonNull(e) => self.of_expr(&e.expr),
      js::Expr::TsSatisfies(e) => self.of_expr(&e.expr),
      _ => None,
    }
  }

  /// What a JSX tag names, an identifier or a member chain of any depth.
  pub(crate) fn of_tag(&self, name: &js::JSXElementName) -> Option<Meaning> {
    fn object(m: &Meanings, obj: &js::JSXObject) -> Option<Meaning> {
      match obj {
        js::JSXObject::Ident(id) => m.of_ident(id).cloned(),
        js::JSXObject::JSXMemberExpr(inner) => object(m, &inner.obj)?.member(inner.prop.sym.as_ref()),
      }
    }
    match name {
      js::JSXElementName::Ident(id) => self.of_ident(id).cloned(),
      js::JSXElementName::JSXMemberExpr(m) => object(self, &m.obj)?.member(m.prop.sym.as_ref()),
      js::JSXElementName::JSXNamespacedName(_) => None,
    }
  }
}

/// `expr` without the parentheses and the TypeScript wrappers around it, `(x as T)` and `x satisfies T` read as `x`.
pub(crate) fn bare(expr: &js::Expr) -> &js::Expr {
  match expr {
    js::Expr::Paren(p) => bare(&p.expr),
    js::Expr::TsAs(e) => bare(&e.expr),
    js::Expr::TsNonNull(e) => bare(&e.expr),
    js::Expr::TsSatisfies(e) => bare(&e.expr),
    js::Expr::TsConstAssertion(e) => bare(&e.expr),
    js::Expr::TsTypeAssertion(e) => bare(&e.expr),
    other => other,
  }
}

/// A name a statement binds to a module's export through an awaited `import()`: `const { a, default: B } = await import("m")`, `const m = await import("m")` or `const [{ a }, { default: B }] = await Promise.all([import("m"), import("n")])`, each pattern matched to its import by position.
#[derive(Debug, Clone, PartialEq)]
pub struct ImportBinding {
  /// The local name bound.
  pub local: String,
  /// The module specifier as written.
  pub source: String,
  /// The export bound, `default` for a default import; `None` when the local is the module whole.
  pub imported: Option<String>,
}

/// The import bindings `stmt` makes through awaited dynamic imports; empty for any other statement.
pub fn awaited_imports(stmt: &js::Stmt) -> Vec<ImportBinding> {
  let js::Stmt::Decl(js::Decl::Var(var)) = stmt else { return Vec::new() };
  let mut out = Vec::new();
  for decl in &var.decls {
    let Some(init) = decl.init.as_deref() else { continue };
    let js::Expr::Await(awaited) = bare(init) else { continue };
    match (bare(&awaited.arg), &decl.name) {
      (js::Expr::Call(call), pat) => {
        if let Some(source) = imported_source(call) {
          bind(pat, &source, &mut out);
        } else if let Some(sources) = promise_all(call) {
          if let js::Pat::Array(array) = pat {
            for (element, source) in array.elems.iter().zip(sources) {
              if let Some(pat) = element {
                bind(pat, &source, &mut out);
              }
            }
          }
        }
      }
      _ => {}
    }
  }
  out
}

/// The specifier of `import("m")`; `None` for any other call.
fn imported_source(call: &js::CallExpr) -> Option<String> {
  if !matches!(call.callee, js::Callee::Import(_)) {
    return None;
  }
  match call.args.first().map(|a| bare(&a.expr)) {
    Some(js::Expr::Lit(js::Lit::Str(s))) => Some(s.value.to_string_lossy().into_owned()),
    _ => None,
  }
}

/// The specifiers of `Promise.all([import("m"), import("n")])` in order; `None` for any other call or an element that is not an `import()`.
fn promise_all(call: &js::CallExpr) -> Option<Vec<String>> {
  let js::Callee::Expr(callee) = &call.callee else { return None };
  let js::Expr::Member(member) = bare(callee) else { return None };
  if !matches!(&*member.obj, js::Expr::Ident(id) if id.sym.as_ref() == "Promise") || !matches!(&member.prop, js::MemberProp::Ident(p) if p.sym.as_ref() == "all") {
    return None;
  }
  let js::Expr::Array(array) = bare(&call.args.first()?.expr) else { return None };
  array
    .elems
    .iter()
    .map(|element| match element {
      Some(element) if element.spread.is_none() => match bare(&element.expr) {
        js::Expr::Call(call) => imported_source(call),
        _ => None,
      },
      _ => None,
    })
    .collect()
}

fn bind(pat: &js::Pat, source: &str, out: &mut Vec<ImportBinding>) {
  match pat {
    js::Pat::Ident(id) => out.push(ImportBinding { local: id.id.sym.to_string(), source: source.to_owned(), imported: None }),
    js::Pat::Object(object) => {
      for prop in &object.props {
        match prop {
          js::ObjectPatProp::Assign(a) => out.push(ImportBinding { local: a.key.sym.to_string(), source: source.to_owned(), imported: Some(a.key.sym.to_string()) }),
          js::ObjectPatProp::KeyValue(kv) => {
            let imported = match &kv.key {
              js::PropName::Ident(id) => id.sym.to_string(),
              js::PropName::Str(s) => s.value.to_string_lossy().into_owned(),
              _ => continue,
            };
            if let js::Pat::Ident(local) = &*kv.value {
              out.push(ImportBinding { local: local.id.sym.to_string(), source: source.to_owned(), imported: Some(imported) });
            }
          }
          js::ObjectPatProp::Rest(_) => {}
        }
      }
    }
    _ => {}
  }
}
