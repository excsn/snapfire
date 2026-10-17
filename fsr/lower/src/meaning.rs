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

/// A library construct the build lowers, under the name its library exports it as.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Construct {
  /// A hook, by React's name for it: `useState`, `useContext`, `use`.
  Hook(&'static str),
  /// `createContext`; `react` when it is React's, whose providers the browser mounts.
  CreateContext { react: bool },
  /// A placement the build lowers itself: `Island`, `island`, `Link`, `Slot`, `Picture`, `Mount`, `Embed` or the retired `tree`.
  Placement(&'static str),
  /// The store's `key`, which names a store key by its string.
  StoreKey,
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
  /// React's `Children`, of which the build reads the count of a component's own children.
  Children,
  /// React's `cloneElement`.
  CloneElement,
  /// React's `isValidElement`.
  IsValidElement,
  /// React's `Suspense` or `Activity`, which the server writes as a boundary around its children.
  Boundary(&'static str),
}

/// Where the placements are exported: the template dialect and each client adapter, which offer the same ones.
const PLACEMENT_MODULES: [&str; 4] = ["@snapfire/fsr-authoring/template", "@snapfire/fsr-client/react", "@snapfire/fsr-client/vue", "@snapfire/fsr-client/jsx-runtime"];
const PLACEMENTS: [&str; 8] = ["Island", "island", "Link", "Slot", "Picture", "Mount", "Embed", "tree"];

fn rows() -> &'static [(&'static str, &'static str, Construct)] {
  static ROWS: std::sync::OnceLock<Vec<(&'static str, &'static str, Construct)>> = std::sync::OnceLock::new();
  ROWS.get_or_init(|| {
    let mut rows: Vec<(&'static str, &'static str, Construct)> = LIBRARY.to_vec();
    for module in PLACEMENT_MODULES {
      rows.extend(PLACEMENTS.iter().map(|name| (module, *name, Construct::Placement(name))));
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
  ("@snapfire/fsr-client/react", "useHoisted", Construct::Hook("useHoisted")),
  ("@snapfire/fsr-authoring/template", "createContext", Construct::CreateContext { react: false }),
  ("@snapfire/fsr-authoring/template", "use", Construct::Hook("use")),
  ("@snapfire/fsr-client/vue", "useStore", Construct::Hook("useStore")),
  ("@snapfire/fsr-client/vue", "useLocale", Construct::Hook("useLocale")),
  ("@snapfire/fsr-client/store", "key", Construct::StoreKey),
  ("@snapfire/fsr-client", "key", Construct::StoreKey),
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
  ("@snapfire/fsr-client/std", "t", Construct::Std("t")),
  ("@snapfire/fsr-client/std", "native", Construct::Std("native")),
  ("@snapfire/fsr-client/std", "localeTag", Construct::Std("localeTag")),
  ("@snapfire/fsr-client/std", "intl", Construct::Std("intl")),
  ("@snapfire/fsr-client/std", "text", Construct::Std("text")),
  ("@snapfire/fsr-client/std", "time", Construct::Std("time")),
  ("@snapfire/fsr-client/std", "crypto", Construct::Std("crypto")),
  ("@snapfire/fsr-client/std", "id", Construct::Std("id")),
  ("vue", "ref", Construct::Vue("ref")),
  ("vue", "shallowRef", Construct::Vue("shallowRef")),
  ("vue", "computed", Construct::Vue("computed")),
  ("vue", "reactive", Construct::Vue("reactive")),
  ("vue", "shallowReactive", Construct::Vue("shallowReactive")),
  ("vue", "readonly", Construct::Vue("readonly")),
  ("vue", "provide", Construct::Vue("provide")),
  ("vue", "inject", Construct::Vue("inject")),
  ("vue", "defineAsyncComponent", Construct::AsyncComponent),
  ("react", "Children", Construct::Children),
  ("react", "cloneElement", Construct::CloneElement),
  ("react", "isValidElement", Construct::IsValidElement),
  ("react", "Suspense", Construct::Boundary("Suspense")),
  ("react", "Activity", Construct::Boundary("Activity")),
  ("clsx", "clsx", Construct::ClassNames),
  ("clsx", "default", Construct::ClassNames),
  ("classnames", "default", Construct::ClassNames),
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
  /// An import from a package the table does not speak for.
  Foreign,
}

impl Meaning {
  /// What `.name` of this means.
  pub(crate) fn member(&self, name: &str) -> Option<Meaning> {
    match self {
      Meaning::Namespace(members) => members.get(name).cloned(),
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
