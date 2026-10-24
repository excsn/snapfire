use serde::{Deserialize, Serialize};

/// A body is a statement list. A loader body ends in a `return` of an object;
/// an action body may return anything or nothing.
pub type Body = Vec<Stmt>;

/// The module-level constants a plan names, by `<file>#<name>`. A body reads
/// one with `Expr::Const` rather than carrying its own copy.
pub type Consts = std::collections::BTreeMap<String, Expr>;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Stmt {
  Let { name: String, expr: Expr },
  /// `name = expr` where `name` is already bound: the nearest binding takes
  /// the value, so a write inside a branch or a loop outlives it.
  Set { name: String, expr: Expr },
  If { cond: Expr, then: Body, #[serde(default, skip_serializing_if = "Vec::is_empty")] r#else: Body },
  ForOf { name: String, over: Expr, body: Body },
  Return(Expr),
  /// `if (cond) fail(kind, message)`. The kind is a `FailureKind` name; the
  /// message is any expression, evaluated only when the guard fires.
  Guard { cond: Expr, kind: String, message: Expr },
  SessionSet { key: String, #[serde(default, skip_serializing_if = "Vec::is_empty")] path: Vec<Expr>, value: Expr },
  SessionDelete { key: String, #[serde(default, skip_serializing_if = "Vec::is_empty")] path: Vec<Expr> },
  /// `session.extend(seconds)`: the session ends `seconds` from now once the
  /// body commits. An action, a route handler or middleware holds one; the
  /// lowerer refuses it in a loader.
  SessionExtend { seconds: Expr },
  /// `void save(input)` in a handler, `save` an `action("id")`: the host
  /// dispatches the action when the handler runs in server mode. Only a
  /// handler holds one; a body cannot dispatch an action.
  Act { action: String, input: Expr },
  /// `try { body } catch (name) { handler } finally { finally }`. `handler` runs when `body` fails, whatever failed: a `throw`, a `fail`, a call or the interpreter. `name` binds what was thrown; for a failure nothing threw it binds `{ name, kind, message }`. `None` is a `try` with no `catch`. `finally` runs on every way out of the two; when it returns or fails, that replaces how they ended.
  Try { body: Body, #[serde(default, skip_serializing_if = "Option::is_none")] name: Option<String>, #[serde(default, skip_serializing_if = "Option::is_none")] handler: Option<Body>, #[serde(default, skip_serializing_if = "Vec::is_empty")] finally: Body },
  Expr(Expr),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Expr {
  Param(String),
  Query(String),
  /// The route's params as an object, for a body that passes them whole.
  ParamsAll,
  /// The request's query as an object, for a body that passes it whole.
  QueryAll,
  Session(String),
  /// `useContext(c)`: the value the nearest provider of the context `c`, `file#name`, holds around the render point, null under none.
  Context(String),
  /// A store key, read from the seed the route's `store` exports settled on.
  /// Ambient in a render: a nested component reads it without a prop.
  Store(String),
  Identity(Vec<String>),
  /// The request's locale as the application spells it. Ambient in a render,
  /// the way a store key is.
  Locale,
  /// The locales the application serves, `ctx.locales`, in the order `[locales]` declares them.
  /// The same on every request.
  Locales,
  /// The path the request matched, without its query. A route that is
  /// prerendered has one, so this is fixed rather than dynamic.
  Path,
  /// The path of the page the document is showing: the origin of an
  /// intercepted navigation, else the path the request matched. Fixed the
  /// way `Path` is.
  Document,
  /// The host the request named, `null` unless `[server] hosts` lists it.
  /// Several configured hosts differ between requests, so this is dynamic
  /// where `Path` is not.
  Host,
  /// `document.origin`, the scheme and host this deployment is reached over,
  /// `null` when it names none. One value for the whole deployment, so this
  /// is constant the way `Config` is rather than per-request like `Host`.
  Origin,
  /// `ctx.address`: the navigation's own request on an intercepted render,
  /// a map of `path`, `params` and `query` or null.
  Address,
  /// One `[public]` value, `ctx.config.<key>`. Fixed for the deployment, so
  /// it is constant the way `Path` is rather than per-request the way `Host` is.
  Config(String),
  Input,
  Now,
  Var(String),
  /// A literal or an object, array or template made only of literals, built
  /// once by `render::prepare` and cloned on each read. Never in a plan.
  #[serde(skip)]
  Folded(snapfire_fsr_core::value::Value),
  /// A module-level `const` the build named rather than inlined, `<file>#<name>`.
  /// The plan's `consts` table holds it once however many bodies read it.
  Const(String),
  Lit(Lit),
  Object(Vec<Entry>),
  Array(Vec<Entry>),
  Field(Box<Expr>, String),
  Index(Box<Expr>, Box<Expr>),
  Arith(ArithOp, Box<Expr>, Box<Expr>),
  Compare(CompareOp, Box<Expr>, Box<Expr>),
  Logic(LogicOp, Box<Expr>, Box<Expr>),
  Not(Box<Expr>),
  Coalesce(Box<Expr>, Box<Expr>),
  Ternary(Box<Expr>, Box<Expr>, Box<Expr>),
  Template(Vec<Expr>),
  /// `await services.<service>.<method>(args)`. An argument whose value is
  /// `null` is omitted, the way an absent optional argument is in TypeScript.
  Call { service: String, method: String, #[serde(default)] args: Vec<(String, Expr)> },
  /// `await native.<module>.<method>(args)`: the application's own Rust,
  /// in this process. Shaped like `Call` and resolved through a different
  /// handle, since nothing crosses a wire and there is no contract to check
  /// it against.
  NativeCall { module: String, method: String, #[serde(default)] args: Vec<(String, Expr)>, #[serde(default, skip_serializing_if = "std::ops::Not::not")] sync: bool },
  /// Middleware's `next(request)`: the rest of the chain run for `request`, `undefined` for the request as it came, answering the route's `Response`.
  Next(Box<Expr>),
  /// `after(fn)`: `fn`, a lambda, queued on the request's `after` to run once the response is written, over a copy of the scope it was called in. `undefined`.
  After(Box<Expr>),
  Lambda { params: Vec<String>, body: Box<Expr> },
  /// A function value that sees itself under `name` while it runs, so it may call itself; `f` is the `Lambda`. A call nested `interp::MAX_APPLIES` deep fails the step.
  Fix { name: String, f: Box<Expr> },
  /// A lambda applied to arguments: a module-level helper a component calls.
  Apply { f: Box<Expr>, args: Vec<Expr> },
  /// [`Expr::Apply`] with the arguments as one array: `f(a, ...rest)`.
  ApplyOf(Box<Expr>, Box<Expr>),
  /// Statements run in the enclosing scope: the value the first `return` yields, null when none. A lambda whose body is one may write the names around it, which a lambda with an expression body cannot; hoisting reads those writes as taint and the interpreter runs it on the async path.
  Block(Body),
  /// A fixed pure function; the list is `Builtin`.
  Builtin { name: Builtin, args: Vec<Expr> },
  /// An extension call, `intl.number(n)`: a member of a module the
  /// interpreter's registry holds, the standard library or a native pair the
  /// host registered. The locale is ambient.
  Ext { module: String, name: String, args: Vec<Expr> },
  Map(Box<Expr>, Box<Expr>),
  Filter(Box<Expr>, Box<Expr>),
  Reduce(Box<Expr>, Box<Expr>, Box<Expr>),
  Find(Box<Expr>, Box<Expr>),
  FindIndex(Box<Expr>, Box<Expr>),
  Some(Box<Expr>, Box<Expr>),
  Every(Box<Expr>, Box<Expr>),
  /// A sorted copy, `toSorted(f)`: `f` is the comparator lambda; `null` is
  /// JavaScript's default order by `String(x)` in UTF-16 code units. Stable.
  Sort(Box<Expr>, Box<Expr>),
  /// `flatMap(f)`: each result an array spread one level, anything else kept.
  FlatMap(Box<Expr>, Box<Expr>),
  /// `s.replace(pattern, f)`: each match replaced by `String(f(match, ...groups, offset, s))`, every match for a regular expression with `g`, the first otherwise.
  ReplaceWith(Box<Expr>, Box<Expr>, Box<Expr>),
  Entries(Box<Expr>),
  Keys(Box<Expr>),
  Values(Box<Expr>),
  Length(Box<Expr>),
  Str(Box<Expr>),
  Num(Box<Expr>),
  BigInt(Box<Expr>),
  /// A render-path expression whose inputs are props only: the renderer
  /// records its value under `id` and the enclosing loop indices; the
  /// browser reads that record instead of computing it. `id` is unique
  /// within the component's module.
  Hoist { id: u32, expr: Box<Expr> },
  /// A `useId` call: the string `Hoists::id` spells for `id` under the current module and path, so a server render and the browser's render of the same component agree on it without a wire entry. `id` is numbered with the hoists of the component's module.
  HoistId(u32),
  /// The component's handler at this index as a value, which a prop holding a function carries. It evaluates to a reference to the handler of the component being rendered, which an `on*` attribute binds and an island's props leave out.
  Handler(u32),
  /// A cell for one render: a fresh one holding `init`, evaluating to a handle that `ReadCell` and `WriteCell` take and a closure captures like any value. A body that makes one makes a fresh one each time it runs, which is a cell per component instance.
  Cell(Box<Expr>),
  /// The render's cell named by the string `key` evaluates to, made from `init` the first time it is reached in the render and the same cell every time after: module state, shared by the instances of one render (its islands among them, as the browser shares a module across a page's islands) and fresh in the next.
  ModuleCell { key: Box<Expr>, init: Box<Expr> },
  /// What the cell a handle names holds.
  ReadCell(Box<Expr>),
  /// Writes `value` into the cell a handle names and evaluates to `value`.
  WriteCell(Box<Expr>, Box<Expr>),
  /// Writes `value` to the store key and evaluates to `value`: the store's `set` where a function body calls it, which a step returns with the writes its handler's patch made.
  WriteStore(String, Box<Expr>),
  /// Markup written where a value goes, `body={<p />}` or `const x = <p />`. The build moves it into the tree where it can; one left here evaluates to a markup value that renders where it is interpolated, in the scope it was written in.
  Markup(std::sync::Arc<Tmpl>),
}

/// The pure functions a component may call by name. Each is one JavaScript
/// method or global with the same semantics for the value model.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Builtin {
  Round,
  Floor,
  Ceil,
  Abs,
  Min,
  Max,
  ToFixed,
  Repeat,
  Join,
  Trim,
  Upper,
  Lower,
  Includes,
  StartsWith,
  EndsWith,
  /// `s.split(sep, limit?)`, at most `limit` pieces, read as `ToUint32`. An empty separator splits into characters and fails on one
  /// outside the Basic Multilingual Plane: JavaScript splits that into UTF-16 code units, which a Rust `String` cannot hold a half of.
  Split,
  /// `s.replace(from, to)`, the first occurrence only, with `$` in `to`
  /// carrying JavaScript's substitutions.
  Replace,
  EncodeUriComponent,
  LocaleNumber,
  /// `Array.from({ length: n })`: the integers `0..n`.
  Range,
  /// An object without the named keys: the rest of a destructuring.
  Omit,
  /// `x.slice(start, end)` of an array or a string; a string's indices count UTF-16 code units.
  Slice,
  /// `x.at(i)`, a negative index counting from the end.
  At,
  /// `x.indexOf(v, from)` of an array by strict equality and of a string by its text.
  IndexOf,
  /// `a.concat(...items)`: an array item spread one level, anything else appended; for a string, the items as text.
  Concat,
  /// A reversed copy, `toReversed()`.
  Reverse,
  /// `s.padStart(length, fill)`.
  PadStart,
  /// `s.padEnd(length, fill)`.
  PadEnd,
  /// `s.substring(start, end)`.
  Substring,
  /// `JSON.stringify(v, null, indent)`.
  Json,
  /// `JSON.parse(s)`.
  JsonParse,
  /// `Math.pow(a, b)`.
  Pow,
  /// `Math.sqrt(n)`.
  Sqrt,
  /// `Math.trunc(n)`.
  Trunc,
  /// `Math.sign(n)`.
  Sign,
  /// `Math.min(...items)` over one array; `Infinity` when it is empty.
  MinOf,
  /// `Math.max(...items)` over one array; `-Infinity` when it is empty.
  MaxOf,
  /// `new Date(x)` as milliseconds since the epoch: a number as it is, an ISO 8601 string parsed, anything else NaN.
  DateMs,
  /// A UTC part of a date's milliseconds, named by the second argument: `year`, `month` (from 0), `date`, `day` (0 is Sunday), `hours`, `minutes`, `seconds` or `milliseconds`.
  DatePart,
  /// `date.toISOString()` of milliseconds since the epoch.
  IsoString,
  /// `Object.fromEntries(pairs)` and `new Map(pairs)`: each key as `String(key)`.
  FromEntries,
  /// `new Set(items)`: the items without repeats, first occurrence kept.
  Unique,
  /// `map.has(key)` over an object a `Map` lowered to and `key in x`: an object's own key, an array's index.
  HasKey,
  /// `clsx(...)` and `classnames(...)` over its arguments as one array: each non-empty string and non-zero number, an array's items in turn and an object's keys whose values are truthy, joined by a space.
  ClassNames,
  /// `typeof x`: `"string"`, `"number"`, `"boolean"`, `"function"` for a handler or an action and `"object"` for null, an object or an array and `"undefined"` for undefined.
  TypeOf,
  /// `Array.isArray(x)`.
  IsArray,
  /// `x instanceof Error` or one of its subclasses, `(x, name)`: whether `x` is an object whose `name` is an error constructor's name and whose `message` is a string; with `name` other than `Error`, whether `x.name` is that name.
  IsError,
  /// `([value, last, ...], [index, ...])`: what a function that writes fields of its parameters answers, its value then each written parameter's last value, and those parameters' positions. The call that applies it answers `value` and writes each `last` to the argument at `index` when that argument is a variable or a path of fields and indexes from one, as JavaScript's caller sees an object it passed changed.
  WriteBack,
  /// `console.log(...)` and its kin on a render path: the first argument names the method, `log`, `info`, `debug`, `warn` or `error`, and the rest are written as `console.log` writes them, as a `tracing` event under the target `fsr::console` at that level; the value is null.
  Log,
  /// [`Builtin::Log`] with the values to write as one array in the second argument: `console.log(...items)`.
  LogOf,
  /// `throw Error(message)` on a render path: the render fails with `String(message)`, as a component that throws fails React's server render. A second argument is what a `catch` around it binds, the `Error` itself.
  Throw,
  /// A component named as a value, `icon={Star}`: its module, which a tag read from the value renders.
  ComponentRef,
  /// `isValidElement(x)`: whether `x` is a markup value.
  IsElement,
  /// `cloneElement(element, props)`.
  CloneElement,
  /// `new URLSearchParams(x).toString()`: an object's entries or a query string, form-encoded.
  FormEncode,
  /// `new URLSearchParams(x).get(key)` over the string `FormEncode` wrote: the first value under `key`, null for none.
  FormGet,
  /// `.has(key)` over the same string: whether any value sits under `key`.
  FormHas,
  /// `.getAll(key)` over the same string: every value under `key`, in order.
  FormGetAll,
  /// `.entries()` over the same string: each pair as `[key, value]`, in order.
  FormEntries,
  /// The class an instance of a class the build lowered was made by, `file#Class`, from its `$c`; null for any other value. A method call dispatches on it.
  ClassOf,
  /// `x.m(args)` with `x` an instance: the method `m` of the nearest class on `x`'s chain that declares it, called with `x` and then `args`. The arguments are `x`, the name and `args`.
  CallMethod,
  /// `x.g` with `x` an instance: the getter `g` of the nearest class on `x`'s chain that declares one, called with `x`, else `x`'s own field `g`.
  GetMember,
  /// `.set(key, value)` over the same string: the string with the first value under `key` replaced and the rest removed; the pair appended when there is none.
  FormSet,
  /// `.append(key, value)` over the same string: the string with the pair added at the end.
  FormAppend,
  /// `.delete(key)` over the same string: the string without any value under `key`.
  FormDelete,
  /// `new URL(url, base)`: the href, parsed against `base` when given and normalised as the URL standard does; an invalid URL fails as `new URL` throws.
  UrlParse,
  /// `new Response(body, init)`: a response of `body`, a string, bytes or nothing, with the `status` and `headers` `init` gives.
  Response,
  /// `Response.json(value, init)`: `value` written as JSON, typed `application/json`.
  ResponseJson,
  /// `Response.redirect(url, status)`: a redirect to `url`, a path or an absolute URL, with `status` one of 301, 302, 303, 307 and 308, 302 when absent.
  ResponseRedirect,
  /// `redirect(to)` from `@snapfire/fsr`: a 303 to `to`, which must be a path on this origin.
  RedirectTo,
  /// `new Request(url)`: a request for `url`, which middleware hands to `next`.
  RequestNew,
  /// A member of a response: `status` or `ok`, by the name in the second argument.
  ResponseMember,
  /// `response.headers.get(name)` or, with `"has"` as the third argument, `.has(name)`.
  ResponseHeaderGet,
  /// `response.headers.set(name, value)`, `.append(name, value)` or `.delete(name)`: the response with its headers changed, the operation in the second argument.
  ResponseHeaderSet,
  /// `new Uint8Array(source)`: the bytes of an array of numbers, each taken modulo 256; for a length, that many zeros.
  Uint8Array,
  /// `url.pathname` and the other parts of a href: `href`, `origin`, `protocol`, `username`, `password`, `host`, `hostname`, `port`, `pathname`, `search` and `hash`, each as `URL` reads it.
  UrlPart,
  /// `url.pathname = value` and the other settable parts: the href with that part replaced as `URL`'s setter replaces it.
  UrlSet,
  /// `re.test(s)`.
  RegexTest,
  /// `s.match(re)`: the first match and its groups, every match with `g`, null for none.
  Match,
  /// `s.matchAll(re)`: each match with its groups.
  MatchAll,
  /// `s.search(re)`: the UTF-16 index of the first match, -1 for none.
  Search,
  /// `s.replaceAll(from, to)` with a string `from`.
  ReplaceAll,
  /// A checkbox's `v-model`: whether an array model holds the box's value by Vue's loose equality, else whether the model is truthy.
  Checked,
  /// A radio's or an option's `v-model`: whether an array model holds the value, else whether the two are equal as text.
  LooseMatch,
  /// The global `isNaN(x)`: whether `Number(x)` is NaN.
  IsNaN,
  /// `x == null`: whether `x` is null or undefined.
  IsNullish,
  /// `parseInt(s, radix)` over `String(s)`: a radix of 0 or none reads 10 (16 after `0x`); one outside 2 to 36 is NaN.
  ParseInt,
  /// `x.lastIndexOf(v, from)` of an array by strict equality and of a string by its text.
  LastIndexOf,
  /// `s.charAt(i)`: the UTF-16 code unit at `i` as a string, empty past either end.
  CharAt,
  /// `s.charCodeAt(i)`: the UTF-16 code unit at `i`, NaN past either end.
  CharCodeAt,
  /// A `Math` function V8 computes in its own code: the first argument names it (`sin`, `cos`, `tan`, `asin`, `acos`, `atan`, `atan2`, `sinh`, `cosh`, `asinh`, `acosh`, `atanh`, `exp`, `expm1`, `log`, `log2`, `log10`, `log1p` or `cbrt`) and the rest are its arguments, computed as `jsmath` does.
  Math,
  /// `s.codePointAt(i)`: the code point starting at UTF-16 index `i`, a lone surrogate as its unit, null past either end.
  CodePointAt,
  /// `x.toString(radix)`: a number's digits in a radix from 2 to 36, anything else as `String(x)`. In a radix other than 10 a number with a fraction is an error. So is one past 2^53 in a radix that is not a power of 2, since engines print their digits differently.
  Radix,
  /// `new Intl.Segmenter(locale, { granularity: "grapheme" }).segment(s)`: each extended grapheme cluster of `s` as `{ segment, index, input }`, `index` a UTF-16 offset. With an index as the second argument, `.containing(i)`: the cluster holding UTF-16 index `i`, null past either end.
  Graphemes,
  /// `x[key] = value` on a local, as a copy: an array with the index set (its length appends) or an object with `String(key)` set. A key on an array that is not an index or lies past its length is an error, since the value model holds no sparse array.
  SetAt,
  /// `emit(kind, value)`: hands `value` to the step's host as an emitted value of `kind`, with the path of the component whose render or handler evaluated it; null. Only a step in server mode, a terminal's, emits; anywhere else it does nothing.
  Emit,
}

/// A component's render tree. Elements and text are literal; `Expr` is
/// interpolated as text; `If`, `For` and `Let` are the JSX idioms for a
/// ternary, `.map` and a block body; `Component` is another lowered component
/// applied to props.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Tmpl {
  Text(String),
  Expr(Expr),
  /// Attributes are `Entry::Field` in HTML spelling or `Entry::Spread` of a map whose keys are React's spelling, later entries winning.
  Element { tag: String, #[serde(default, skip_serializing_if = "Vec::is_empty")] attrs: Vec<Entry>, #[serde(default, skip_serializing_if = "Vec::is_empty")] children: Vec<Tmpl> },
  Fragment(Vec<Tmpl>),
  If { cond: Expr, then: Box<Tmpl>, #[serde(default, skip_serializing_if = "Option::is_none")] r#else: Option<Box<Tmpl>> },
  For { over: Expr, params: Vec<String>, body: Box<Tmpl> },
  Let { name: String, expr: Expr, then: Box<Tmpl> },
  /// Props are `Entry::Field` or `Entry::Spread`; `children` render in the caller's scope wherever the callee places its `Slot`.
  /// `id` is the placement's index among the enclosing component's hoist
  /// candidates, kept so a placement that turns out to be an island carries it.
  /// `keyed` says the placed component keys a hoist or a region somewhere
  /// below it, so the placement extends the key path by `id` on both halves:
  /// two placements of one component then key what it holds apart.
  Component { module: String, #[serde(default, skip_serializing_if = "Vec::is_empty")] props: Vec<Entry>, #[serde(default, skip_serializing_if = "Vec::is_empty")] children: Vec<Tmpl>, #[serde(default, skip_serializing_if = "is_zero")] id: u32, #[serde(default, skip_serializing_if = "is_false")] keyed: bool },
  /// A component placed as its own island: rendered like `Component`, then
  /// wrapped as a nested client node the browser mounts in its own root,
  /// `when` its hydration timing. `id` is the placement's index among the
  /// enclosing component's hoist candidates, which with the module and the
  /// loop path names the region the browser mounts it in.
  /// `define` marks an island whose module is an element definition rather
  /// than a component: the server renders `children` inside the marker as
  /// ordinary markup, the browser imports the module at the island's timing
  /// and the element upgrades itself.
  Island { module: String, #[serde(default, skip_serializing_if = "Vec::is_empty")] props: Vec<Entry>, #[serde(default, skip_serializing_if = "Vec::is_empty")] children: Vec<Tmpl>, #[serde(default, skip_serializing_if = "Option::is_none")] when: Option<String>, #[serde(default, skip_serializing_if = "Option::is_none")] mode: Option<String>, #[serde(default, skip_serializing_if = "is_zero")] id: u32, #[serde(default, skip_serializing_if = "is_false")] define: bool },
  /// The caller's children where the callee places `{children}`, named
  /// `content`; at a layout's root, the plan child of that name, so a
  /// `<Slot name="modal" />` names a second segment beside the page.
  Slot(String),
  /// Markup a framework's hydration reads, written as given: a comment such
  /// as `<!--[0-->` that the lowerer placed where that framework's own server
  /// renderer writes it. The markers depend on the template's structure and
  /// never on data, so a lowering bakes them in as the framework's compiler does.
  Marker(String),
  /// `{@html value}`: the value written unescaped, `null` as nothing and
  /// anything else as the markup's dialect writes it as text.
  Html(Expr),
  /// An element whose open tag is entirely literal, baked into one slice when
  /// the component was prepared. The plan never carries this: it is what
  /// `snapfire_fsr_ir::render::prepare` turns an `Element` into, so the
  /// renderer copies the tag instead of evaluating it. `tag` is `None` for a
  /// void element, whose `open` already closes itself and which has no
  /// children.
  Baked { open: String, tag: Option<String>, children: Vec<Tmpl> },
  /// `children` written at `target` rather than in place, where the
  /// evaluator carries them as a `Node::Placed`: a Vue `<Teleport>`. The
  /// target is a `Place` spelling, `body` for the end of the body.
  Place { target: String, #[serde(default, skip_serializing_if = "Vec::is_empty")] children: Vec<Tmpl> },
  /// Where the content a `Place` targeting `#name` holds is written: inside the element around it, whose id is `name`. Rendered in markup a framework hydrates it fails the render, unless `hydrated` says that framework's hydration keeps such content: a `<Place>` (each framework's client renders it so hydration leaves the element's children alone) or a Vue `<Teleport>` into an element of its own component. Never static.
  Outlet { name: String, #[serde(default, skip_serializing_if = "is_false")] hydrated: bool },
}

fn is_false(b: &bool) -> bool {
  !*b
}

fn is_zero(n: &u32) -> bool {
  *n == 0
}

/// A lowered component: `let`s run once with `$props` bound, then the tree.
/// `state` names the `let`s the browser can change, `useState` and `useStore`
/// bindings in order, and `stores` the `useStore` bindings among them with the
/// store key each reads, which a server-mode island reads and writes through
/// the store rather than holding; `handlers` are its event handlers as bodies, for an
/// island in server mode, each returning the state it sets. `owner` is what
/// renders the component in the browser, if anything does. `shadow` is the
/// shadow root an element template declares with its root
/// `<template shadowrootmode>`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(try_from = "ComponentRepr")]
pub struct Component {
  #[serde(default, skip_serializing_if = "Vec::is_empty")]
  pub body: Body,
  pub render: Tmpl,
  #[serde(default, skip_serializing_if = "Vec::is_empty")]
  pub state: Vec<String>,
  #[serde(default, skip_serializing_if = "Vec::is_empty")]
  pub stores: Vec<(String, String)>,
  #[serde(default, skip_serializing_if = "Vec::is_empty")]
  pub handlers: Vec<Handler>,
  pub owner: Owner,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub shadow: Option<ShadowRoot>,
  /// The scoped style a Vue component carries, when it has one.
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub scope: Option<ScopedStyle>,
}

/// A Vue component's scoped style: the attribute its rules select on and whether any rule is `:slotted()`, which Vue answers by stamping `<id>-s` on the content its slots render.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ScopedStyle {
  pub id: String,
  #[serde(default, skip_serializing_if = "is_false")]
  pub slotted: bool,
}

impl ScopedStyle {
  /// The attribute Vue stamps on slot content under a `:slotted()` rule.
  pub fn slotted_attr(&self) -> String {
    format!("{}-s", self.id)
  }
}

/// A component as a JSON plan spells it: `owner`, or in a plan written before
/// owners the `hydrate` flag, `false` for nothing, `true` for React and
/// `"tree"` or `"vue"`, left out for React.
#[derive(Deserialize)]
struct ComponentRepr {
  #[serde(default)]
  body: Body,
  render: Tmpl,
  #[serde(default)]
  state: Vec<String>,
  #[serde(default)]
  stores: Vec<(String, String)>,
  #[serde(default)]
  handlers: Vec<Handler>,
  #[serde(default)]
  owner: Option<Owner>,
  #[serde(default)]
  hydrate: Option<serde_json::Value>,
  #[serde(default)]
  shadow: Option<ShadowRoot>,
  #[serde(default)]
  scope: Option<ScopedStyle>,
}

impl TryFrom<ComponentRepr> for Component {
  type Error = String;

  fn try_from(repr: ComponentRepr) -> Result<Self, String> {
    let owner = match (repr.owner, repr.hydrate) {
      (Some(owner), _) => owner,
      (None, Some(serde_json::Value::Bool(true))) => Owner::React,
      (None, Some(serde_json::Value::Bool(false))) => Owner::Fsr,
      (None, Some(serde_json::Value::String(word))) if word == "tree" => Owner::React,
      (None, Some(serde_json::Value::String(word))) => Owner::of(&word).ok_or_else(|| format!("`hydrate` is a boolean, \"tree\" or \"vue\", not \"{word}\""))?,
      (None, Some(other)) => return Err(format!("`hydrate` is a boolean or a word, not {other}")),
      (None, None) => Owner::React,
    };
    Ok(Component { body: repr.body, render: repr.render, state: repr.state, stores: repr.stores, handlers: repr.handlers, owner, shadow: repr.shadow, scope: repr.scope })
  }
}

/// The declarative shadow root the server writes around an element template.
/// The default is an open root with no options.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ShadowRoot {
  pub mode: ShadowMode,
  #[serde(default, skip_serializing_if = "is_false")]
  pub delegates_focus: bool,
  #[serde(default, skip_serializing_if = "is_false")]
  pub clonable: bool,
  #[serde(default, skip_serializing_if = "is_false")]
  pub serializable: bool,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ShadowMode {
  #[default]
  Open,
  Closed,
}

impl ShadowMode {
  pub fn of(mode: &str) -> Option<Self> {
    match mode {
      "open" => Some(Self::Open),
      "closed" => Some(Self::Closed),
      _ => None,
    }
  }

  pub fn as_str(self) -> &'static str {
    match self {
      Self::Open => "open",
      Self::Closed => "closed",
    }
  }
}

/// What renders a component in the browser. `Fsr` is composition: the server
/// renders it and nothing renders it again. `React`, `Vue` and `Svelte`
/// hydrate the server's markup through their adapters.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Owner {
  Fsr,
  React,
  Vue,
  Svelte,
}

impl Owner {
  /// The word the plan's `(owner ...)` section and the JSON `owner` field use.
  pub fn as_str(self) -> &'static str {
    match self {
      Self::Fsr => "fsr",
      Self::React => "react",
      Self::Vue => "vue",
      Self::Svelte => "svelte",
    }
  }

  pub fn of(word: &str) -> Option<Self> {
    match word {
      "fsr" => Some(Self::Fsr),
      "react" => Some(Self::React),
      "vue" => Some(Self::Vue),
      "svelte" => Some(Self::Svelte),
      _ => None,
    }
  }

  /// Whether a framework mounts the component over the server's markup.
  pub fn hydrates(self) -> bool {
    self != Self::Fsr
  }
}

/// One event handler of a component, lowered: runs with `$props`, `$state`
/// and `$event` bound and returns an object whose keys are the state names
/// it sets. `element` is the attribute marker's id, so the renderer can bind
/// it to its element.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Handler {
  pub event: String,
  pub body: Body,
}

impl Component {
  pub fn new(owner: Owner, body: Body, render: Tmpl) -> Self {
    Self { body, render, state: Vec::new(), stores: Vec::new(), handlers: Vec::new(), owner, shadow: None, scope: None }
  }

  /// The store key `name` reads when it is a `useStore` binding.
  pub fn store_key(&self, name: &str) -> Option<&str> {
    self.stores.iter().find(|(binding, _)| binding == name).map(|(_, key)| key.as_str())
  }
}

impl Component {
  /// Calls `f` on every expression in the body and the tree.
  pub fn visit(&self, f: &mut dyn FnMut(&Expr)) {
    body_visit(&self.body, f);
    self.render.visit(f);
  }

  /// True when the component reads `$props.<name>` or uses `$props` whole,
  /// which is how a spread or a rest carries every prop along.
  pub fn reads_prop(&self, name: &str) -> bool {
    let mut aliases = vec!["$props".to_owned()];
    for stmt in &self.body {
      if let Stmt::Let { name, expr: Expr::Var(var) } = stmt {
        if var == "$props" {
          aliases.push(name.clone());
        }
      }
    }
    let mut found = false;
    let mut bare = 0usize;
    let mut wrapped = 0usize;
    self.visit(&mut |expr| match expr {
      Expr::Field(inner, field) if matches!(&**inner, Expr::Var(v) if aliases.contains(v)) => {
        wrapped += 1;
        if field == name {
          found = true;
        }
      }
      Expr::Var(v) if aliases.contains(v) => bare += 1,
      _ => {}
    });
    let defined = aliases.len() - 1;
    found || bare > wrapped + defined
  }
}

/// Calls `f` on every expression a body holds, in statement order, to change it.
pub fn body_visit_mut(body: &mut Body, f: &mut dyn FnMut(&mut Expr)) {
  for stmt in body {
    match stmt {
      Stmt::Let { expr, .. } | Stmt::Set { expr, .. } | Stmt::Return(expr) | Stmt::Expr(expr) => expr.visit_mut(f),
      Stmt::If { cond, then, r#else } => {
        cond.visit_mut(f);
        body_visit_mut(then, f);
        body_visit_mut(r#else, f);
      }
      Stmt::ForOf { over, body, .. } => {
        over.visit_mut(f);
        body_visit_mut(body, f);
      }
      Stmt::Guard { cond, message, .. } => {
        cond.visit_mut(f);
        message.visit_mut(f);
      }
      Stmt::SessionSet { path, value, .. } => {
        path.iter_mut().for_each(|p| p.visit_mut(f));
        value.visit_mut(f);
      }
      Stmt::SessionDelete { path, .. } => path.iter_mut().for_each(|p| p.visit_mut(f)),
      Stmt::SessionExtend { seconds } => seconds.visit_mut(f),
      Stmt::Act { input, .. } => input.visit_mut(f),
      Stmt::Try { body, handler, finally, .. } => {
        body_visit_mut(body, f);
        handler.iter_mut().for_each(|handler| body_visit_mut(handler, f));
        body_visit_mut(finally, f);
      }
    }
  }
}

/// Calls `f` on every expression a body holds, in statement order.
pub fn body_visit(body: &Body, f: &mut dyn FnMut(&Expr)) {
  for stmt in body {
    match stmt {
      Stmt::Let { expr, .. } | Stmt::Set { expr, .. } | Stmt::Return(expr) | Stmt::Expr(expr) => expr.visit(f),
      Stmt::If { cond, then, r#else } => {
        cond.visit(f);
        body_visit(then, f);
        body_visit(r#else, f);
      }
      Stmt::ForOf { over, body, .. } => {
        over.visit(f);
        body_visit(body, f);
      }
      Stmt::Guard { cond, message, .. } => {
        cond.visit(f);
        message.visit(f);
      }
      Stmt::SessionSet { path, value, .. } => {
        path.iter().for_each(|p| p.visit(f));
        value.visit(f);
      }
      Stmt::SessionDelete { path, .. } => path.iter().for_each(|p| p.visit(f)),
      Stmt::SessionExtend { seconds } => seconds.visit(f),
      Stmt::Act { input, .. } => input.visit(f),
      Stmt::Try { body, handler, finally, .. } => {
        body_visit(body, f);
        handler.iter().for_each(|handler| body_visit(handler, f));
        body_visit(finally, f);
      }
    }
  }
}

impl Tmpl {
  /// Calls `f` on every expression in the tree, in tree order.
  pub fn visit(&self, f: &mut dyn FnMut(&Expr)) {
    let entries = |entries: &[Entry], f: &mut dyn FnMut(&Expr)| {
      for entry in entries {
        match entry {
          Entry::Field(_, e) | Entry::Item(e) | Entry::Spread(e) => e.visit(f),
          Entry::Baked(_) => {}
          Entry::Computed(k, v) => {
            k.visit(f);
            v.visit(f);
          }
        }
      }
    };
    match self {
      Tmpl::Text(_) | Tmpl::Slot(_) | Tmpl::Marker(_) | Tmpl::Outlet { .. } => {}
      Tmpl::Expr(e) | Tmpl::Html(e) => e.visit(f),
      Tmpl::Element { attrs, children, .. } => {
        entries(attrs, f);
        children.iter().for_each(|c| c.visit(f));
      }
      Tmpl::Fragment(children) => children.iter().for_each(|c| c.visit(f)),
      Tmpl::If { cond, then, r#else } => {
        cond.visit(f);
        then.visit(f);
        if let Some(other) = r#else {
          other.visit(f);
        }
      }
      Tmpl::For { over, body, .. } => {
        over.visit(f);
        body.visit(f);
      }
      Tmpl::Let { expr, then, .. } => {
        expr.visit(f);
        then.visit(f);
      }
      Tmpl::Component { props, children, .. } | Tmpl::Island { props, children, .. } => {
        entries(props, f);
        children.iter().for_each(|c| c.visit(f));
      }
      Tmpl::Baked { children, .. } | Tmpl::Place { children, .. } => children.iter().for_each(|c| c.visit(f)),
    }
  }

  /// `visit` with each expression open to change: `f` sees an expression before what is beneath it.
  pub fn visit_mut(&mut self, f: &mut dyn FnMut(&mut Expr)) {
    match self {
      Tmpl::Text(_) | Tmpl::Slot(_) | Tmpl::Marker(_) | Tmpl::Outlet { .. } => {}
      Tmpl::Expr(e) | Tmpl::Html(e) => e.visit_mut(f),
      Tmpl::Element { attrs, children, .. } => {
        attrs.iter_mut().for_each(|entry| entry.visit_mut(f));
        children.iter_mut().for_each(|c| c.visit_mut(f));
      }
      Tmpl::Fragment(children) | Tmpl::Baked { children, .. } | Tmpl::Place { children, .. } => children.iter_mut().for_each(|c| c.visit_mut(f)),
      Tmpl::If { cond, then, r#else } => {
        cond.visit_mut(f);
        then.visit_mut(f);
        if let Some(other) = r#else {
          other.visit_mut(f);
        }
      }
      Tmpl::For { over, body, .. } => {
        over.visit_mut(f);
        body.visit_mut(f);
      }
      Tmpl::Let { expr, then, .. } => {
        expr.visit_mut(f);
        then.visit_mut(f);
      }
      Tmpl::Component { props, children, .. } | Tmpl::Island { props, children, .. } => {
        props.iter_mut().for_each(|entry| entry.visit_mut(f));
        children.iter_mut().for_each(|c| c.visit_mut(f));
      }
    }
  }
}

impl Entry {
  fn visit_mut(&mut self, f: &mut dyn FnMut(&mut Expr)) {
    match self {
      Entry::Field(_, e) | Entry::Item(e) | Entry::Spread(e) => e.visit_mut(f),
      Entry::Baked(_) => {}
      Entry::Computed(k, v) => {
        k.visit_mut(f);
        v.visit_mut(f);
      }
    }
  }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Entry {
  Field(String, Expr),
  /// Attributes already printed, ` name="value"` text written as it stands: what a prepared element carries for the literal attributes every markup prints the same way and what a lowerer writes for an attribute its framework prints verbatim. The plan spells it `(! "…")`.
  Baked(String),
  /// `{ [key]: value }`; the key must evaluate to a string.
  Computed(Expr, Expr),
  Item(Expr),
  Spread(Expr),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Lit {
  Null,
  Undefined,
  Bool(bool),
  Int(i128),
  Float(f64),
  Str(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ArithOp {
  Add,
  Sub,
  Mul,
  /// `/` between numbers: an integer that divides evenly stays one, anything else is a fraction.
  Div,
  Rem,
  /// `**`.
  Pow,
  /// `/` between `bigint`s, which truncates toward zero and refuses zero.
  Quot,
  /// `&`, `|`, `^`, `<<` and `>>` between numbers, each side taken through `ToInt32`.
  BitAnd,
  BitOr,
  BitXor,
  Shl,
  Shr,
  /// `>>>`, the left side taken through `ToUint32`.
  UShr,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CompareOp {
  Eq,
  Ne,
  Lt,
  Le,
  Gt,
  Ge,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LogicOp {
  And,
  Or,
}

#[derive(Debug, thiserror::Error)]
#[error("malformed IR: {0}")]
pub struct ParseError(#[from] serde_json::Error);

pub fn from_json(text: &str) -> Result<Body, ParseError> {
  Ok(serde_json::from_str(text)?)
}

pub fn to_json(body: &Body) -> String {
  serde_json::to_string_pretty(body).expect("the IR serialises")
}

impl Expr {
  pub fn lit_str(s: impl Into<String>) -> Self {
    Expr::Lit(Lit::Str(s.into()))
  }

  pub fn lit_int(n: impl Into<i128>) -> Self {
    Expr::Lit(Lit::Int(n.into()))
  }

  pub fn var(name: impl Into<String>) -> Self {
    Expr::Var(name.into())
  }

  pub fn field(self, name: impl Into<String>) -> Self {
    Expr::Field(Box::new(self), name.into())
  }

  pub fn index(self, key: Expr) -> Self {
    Expr::Index(Box::new(self), Box::new(key))
  }

  pub fn call(service: impl Into<String>, method: impl Into<String>, args: Vec<(&str, Expr)>) -> Self {
    Expr::Call {
      service: service.into(),
      method: method.into(),
      args: args.into_iter().map(|(k, v)| (k.to_owned(), v)).collect(),
    }
  }

  pub fn native_call(module: impl Into<String>, method: impl Into<String>, args: Vec<(&str, Expr)>, sync: bool) -> Self {
    Expr::NativeCall {
      module: module.into(),
      method: method.into(),
      args: args.into_iter().map(|(k, v)| (k.to_owned(), v)).collect(),
      sync,
    }
  }

  pub fn ext(module: impl Into<String>, name: impl Into<String>, args: Vec<Expr>) -> Self {
    Expr::Ext { module: module.into(), name: name.into(), args }
  }

  pub fn lambda(params: &[&str], body: Expr) -> Self {
    Expr::Lambda { params: params.iter().map(|p| (*p).to_owned()).collect(), body: Box::new(body) }
  }

  pub fn object(entries: Vec<(&str, Expr)>) -> Self {
    Expr::Object(entries.into_iter().map(|(k, v)| Entry::Field(k.to_owned(), v)).collect())
  }

  /// Every `Var` name the expression reads that it does not bind itself.
  pub fn free_vars(&self, out: &mut Vec<String>) {
    match self {
      Expr::Markup(tmpl) => tmpl.visit(&mut |e| e.free_vars(out)),
      // A name the block writes is read by whoever holds it afterwards, so it counts with the reads.
      Expr::Block(body) => {
        for name in body_free_vars(body).into_iter().chain(set_targets(body)) {
          if !out.contains(&name) {
            out.push(name);
          }
        }
      }
      Expr::Const(_) => {}
      Expr::Var(name) => {
        if !out.contains(name) {
          out.push(name.clone());
        }
      }
      Expr::Folded(_) | Expr::Param(_) | Expr::Query(_) | Expr::ParamsAll | Expr::QueryAll | Expr::Session(_) | Expr::Store(_) | Expr::Context(_) | Expr::Identity(_) | Expr::Locale | Expr::Locales | Expr::Path | Expr::Document | Expr::Host | Expr::Origin | Expr::Address | Expr::Config(_) | Expr::Input | Expr::Now | Expr::Lit(_) | Expr::Handler(_) | Expr::HoistId(_) => {}
      Expr::Object(entries) | Expr::Array(entries) => {
        for entry in entries {
          match entry {
            Entry::Field(_, e) | Entry::Item(e) | Entry::Spread(e) => e.free_vars(out),
            Entry::Baked(_) => {}
            Entry::Computed(k, v) => {
              k.free_vars(out);
              v.free_vars(out);
            }
          }
        }
      }
      Expr::Field(e, _) | Expr::Not(e) | Expr::Entries(e) | Expr::Keys(e) | Expr::Values(e)
      | Expr::Length(e) | Expr::Str(e) | Expr::Num(e) | Expr::BigInt(e) => e.free_vars(out),
      Expr::Index(a, b) | Expr::Arith(_, a, b) | Expr::Compare(_, a, b) | Expr::Logic(_, a, b)
      | Expr::Coalesce(a, b) | Expr::Map(a, b) | Expr::Filter(a, b) | Expr::Find(a, b) | Expr::FindIndex(a, b)
      | Expr::Some(a, b) | Expr::Every(a, b) | Expr::Sort(a, b) | Expr::FlatMap(a, b) => {
        a.free_vars(out);
        b.free_vars(out);
      }
      Expr::Ternary(a, b, c) | Expr::Reduce(a, b, c) | Expr::ReplaceWith(a, b, c) => {
        a.free_vars(out);
        b.free_vars(out);
        c.free_vars(out);
      }
      Expr::Template(parts) => parts.iter().for_each(|p| p.free_vars(out)),
      Expr::Call { args, .. } | Expr::NativeCall { args, .. } => args.iter().for_each(|(_, e)| e.free_vars(out)),
      Expr::Next(e) | Expr::After(e) => e.free_vars(out),
      Expr::Builtin { args, .. } | Expr::Ext { args, .. } => args.iter().for_each(|e| e.free_vars(out)),
      Expr::Apply { f, args } => {
        f.free_vars(out);
        args.iter().for_each(|e| e.free_vars(out));
      }
      Expr::ApplyOf(f, args) => {
        f.free_vars(out);
        args.free_vars(out);
      }
      Expr::Lambda { params, body } => {
        let mut inner = Vec::new();
        body.free_vars(&mut inner);
        for name in inner {
          if !params.contains(&name) && !out.contains(&name) {
            out.push(name);
          }
        }
      }
      Expr::Fix { name, f } => {
        let mut inner = Vec::new();
        f.free_vars(&mut inner);
        for free in inner {
          if free != *name && !out.contains(&free) {
            out.push(free);
          }
        }
      }
      Expr::Hoist { expr, .. } => expr.free_vars(out),
      Expr::Cell(e) | Expr::ReadCell(e) | Expr::WriteStore(_, e) => e.free_vars(out),
      Expr::WriteCell(a, b) | Expr::ModuleCell { key: a, init: b } => {
        a.free_vars(out);
        b.free_vars(out);
      }
    }
  }

  /// Calls `f` on this expression and every expression beneath it, in tree order.
  pub fn visit(&self, f: &mut dyn FnMut(&Expr)) {
    f(self);
    match self {
      Expr::Markup(tmpl) => tmpl.visit(f),
      Expr::Folded(_) | Expr::Param(_) | Expr::Query(_) | Expr::ParamsAll | Expr::QueryAll | Expr::Session(_) | Expr::Store(_) | Expr::Context(_) | Expr::Identity(_) | Expr::Locale | Expr::Locales | Expr::Path | Expr::Document | Expr::Host | Expr::Origin | Expr::Address | Expr::Config(_) | Expr::Input | Expr::Now | Expr::Var(_) | Expr::Const(_) | Expr::Lit(_) | Expr::Handler(_) | Expr::HoistId(_) => {}
      Expr::Call { args, .. } | Expr::NativeCall { args, .. } => args.iter().for_each(|(_, e)| e.visit(f)),
      Expr::Next(e) | Expr::After(e) => e.visit(f),
      Expr::Object(entries) | Expr::Array(entries) => entries.iter().for_each(|entry| match entry {
        Entry::Field(_, e) | Entry::Item(e) | Entry::Spread(e) => e.visit(f),
          Entry::Baked(_) => {}
        Entry::Computed(k, v) => {
          k.visit(f);
          v.visit(f);
        }
      }),
      Expr::Field(e, _) | Expr::Not(e) | Expr::Entries(e) | Expr::Keys(e) | Expr::Values(e)
      | Expr::Length(e) | Expr::Str(e) | Expr::Num(e) | Expr::BigInt(e) => e.visit(f),
      Expr::Index(a, b) | Expr::Arith(_, a, b) | Expr::Compare(_, a, b) | Expr::Logic(_, a, b)
      | Expr::Coalesce(a, b) | Expr::Map(a, b) | Expr::Filter(a, b) | Expr::Find(a, b) | Expr::FindIndex(a, b)
      | Expr::Some(a, b) | Expr::Every(a, b) | Expr::Sort(a, b) | Expr::FlatMap(a, b) => {
        a.visit(f);
        b.visit(f);
      }
      Expr::Ternary(a, b, c) | Expr::Reduce(a, b, c) | Expr::ReplaceWith(a, b, c) => {
        a.visit(f);
        b.visit(f);
        c.visit(f);
      }
      Expr::Template(parts) => parts.iter().for_each(|e| e.visit(f)),
      Expr::Builtin { args, .. } | Expr::Ext { args, .. } => args.iter().for_each(|e| e.visit(f)),
      Expr::Apply { f: callee, args } => {
        callee.visit(f);
        args.iter().for_each(|e| e.visit(f));
      }
      Expr::ApplyOf(callee, args) => {
        callee.visit(f);
        args.visit(f);
      }
      Expr::Lambda { body, .. } => body.visit(f),
      Expr::Fix { f: inner, .. } => inner.visit(f),
      Expr::Block(body) => body_visit(body, f),
      Expr::Hoist { expr, .. } => expr.visit(f),
      Expr::Cell(e) | Expr::ReadCell(e) | Expr::WriteStore(_, e) => e.visit(f),
      Expr::WriteCell(a, b) | Expr::ModuleCell { key: a, init: b } => {
        a.visit(f);
        b.visit(f);
      }
    }
  }

  /// `visit` with each expression open to change: `f` sees an expression before what is beneath it, a markup value's template included.
  pub fn visit_mut(&mut self, f: &mut dyn FnMut(&mut Expr)) {
    f(self);
    match self {
      Expr::Markup(tmpl) => std::sync::Arc::make_mut(tmpl).visit_mut(f),
      Expr::Folded(_) | Expr::Param(_) | Expr::Query(_) | Expr::ParamsAll | Expr::QueryAll | Expr::Session(_) | Expr::Store(_) | Expr::Context(_) | Expr::Identity(_) | Expr::Locale | Expr::Locales | Expr::Path | Expr::Document | Expr::Host | Expr::Origin | Expr::Address | Expr::Config(_) | Expr::Input | Expr::Now | Expr::Var(_) | Expr::Const(_) | Expr::Lit(_) | Expr::Handler(_) | Expr::HoistId(_) => {}
      Expr::Call { args, .. } | Expr::NativeCall { args, .. } => args.iter_mut().for_each(|(_, e)| e.visit_mut(f)),
      Expr::Next(e) | Expr::After(e) => e.visit_mut(f),
      Expr::Object(entries) | Expr::Array(entries) => entries.iter_mut().for_each(|entry| entry.visit_mut(f)),
      Expr::Field(e, _) | Expr::Not(e) | Expr::Entries(e) | Expr::Keys(e) | Expr::Values(e)
      | Expr::Length(e) | Expr::Str(e) | Expr::Num(e) | Expr::BigInt(e) => e.visit_mut(f),
      Expr::Index(a, b) | Expr::Arith(_, a, b) | Expr::Compare(_, a, b) | Expr::Logic(_, a, b)
      | Expr::Coalesce(a, b) | Expr::Map(a, b) | Expr::Filter(a, b) | Expr::Find(a, b) | Expr::FindIndex(a, b)
      | Expr::Some(a, b) | Expr::Every(a, b) | Expr::Sort(a, b) | Expr::FlatMap(a, b) => {
        a.visit_mut(f);
        b.visit_mut(f);
      }
      Expr::Ternary(a, b, c) | Expr::Reduce(a, b, c) | Expr::ReplaceWith(a, b, c) => {
        a.visit_mut(f);
        b.visit_mut(f);
        c.visit_mut(f);
      }
      Expr::Template(parts) => parts.iter_mut().for_each(|e| e.visit_mut(f)),
      Expr::Builtin { args, .. } | Expr::Ext { args, .. } => args.iter_mut().for_each(|e| e.visit_mut(f)),
      Expr::Apply { f: callee, args } => {
        callee.visit_mut(f);
        args.iter_mut().for_each(|e| e.visit_mut(f));
      }
      Expr::ApplyOf(callee, args) => {
        callee.visit_mut(f);
        args.visit_mut(f);
      }
      Expr::Lambda { body, .. } => body.visit_mut(f),
      Expr::Fix { f: inner, .. } => inner.visit_mut(f),
      Expr::Block(body) => body_visit_mut(body, f),
      Expr::Hoist { expr, .. } => expr.visit_mut(f),
      Expr::Cell(e) | Expr::ReadCell(e) | Expr::WriteStore(_, e) => e.visit_mut(f),
      Expr::WriteCell(a, b) | Expr::ModuleCell { key: a, init: b } => {
        a.visit_mut(f);
        b.visit_mut(f);
      }
    }
  }

  /// True when the expression reads anything that differs between requests:
  /// a parameter, the query, the session, the identity, the input or the clock.
  /// The locale is not counted: a route reading only it renders once per
  /// configured locale, which is what prerendering does with it. Neither is
  /// the path, which a prerendered route has one of.
  pub fn reads_request(&self) -> bool {
    match self {
      Expr::Markup(tmpl) => {
        let mut reads = false;
        tmpl.visit(&mut |e| reads |= e.reads_request());
        reads
      }
      Expr::Folded(_) | Expr::Param(_) | Expr::Query(_) | Expr::ParamsAll | Expr::QueryAll | Expr::Session(_) | Expr::Store(_) | Expr::Context(_) | Expr::Identity(_) | Expr::Input | Expr::Now | Expr::Host | Expr::Address => true,
      Expr::Locale | Expr::Locales | Expr::Path | Expr::Document | Expr::Origin | Expr::Config(_) | Expr::Handler(_) | Expr::HoistId(_) => false,
      Expr::Call { args, .. } | Expr::NativeCall { args, .. } => args.iter().any(|(_, e)| e.reads_request()),
      Expr::Next(_) | Expr::After(_) => true,
      Expr::Var(_) | Expr::Const(_) | Expr::Lit(_) => false,
      Expr::Object(entries) | Expr::Array(entries) => entries.iter().any(|entry| match entry {
        Entry::Field(_, e) | Entry::Item(e) | Entry::Spread(e) => e.reads_request(),
        Entry::Baked(_) => false,
        Entry::Computed(k, v) => k.reads_request() || v.reads_request(),
      }),
      Expr::Field(e, _) | Expr::Not(e) | Expr::Entries(e) | Expr::Keys(e) | Expr::Values(e)
      | Expr::Length(e) | Expr::Str(e) | Expr::Num(e) | Expr::BigInt(e) => e.reads_request(),
      Expr::Index(a, b) | Expr::Arith(_, a, b) | Expr::Compare(_, a, b) | Expr::Logic(_, a, b)
      | Expr::Coalesce(a, b) | Expr::Map(a, b) | Expr::Filter(a, b) | Expr::Find(a, b) | Expr::FindIndex(a, b)
      | Expr::Some(a, b) | Expr::Every(a, b) | Expr::Sort(a, b) | Expr::FlatMap(a, b) => a.reads_request() || b.reads_request(),
      Expr::Ternary(a, b, c) | Expr::Reduce(a, b, c) | Expr::ReplaceWith(a, b, c) => a.reads_request() || b.reads_request() || c.reads_request(),
      Expr::Template(parts) => parts.iter().any(Expr::reads_request),
      Expr::Ext { module, name, args } => crate::ext::standard_reach(module, name).is_some_and(|reach| reach != crate::ext::Reach::Render) || args.iter().any(Expr::reads_request),
      Expr::Builtin { args, .. } => args.iter().any(Expr::reads_request),
      Expr::Apply { f, args } => f.reads_request() || args.iter().any(Expr::reads_request),
      Expr::ApplyOf(f, args) => f.reads_request() || args.reads_request(),
      Expr::Lambda { body, .. } => body.reads_request(),
      Expr::Fix { f, .. } => f.reads_request(),
      Expr::Block(body) => body_reads_request(body),
      Expr::Hoist { expr, .. } => expr.reads_request(),
      Expr::Cell(e) | Expr::ReadCell(e) | Expr::WriteStore(_, e) => e.reads_request(),
      Expr::WriteCell(a, b) | Expr::ModuleCell { key: a, init: b } => a.reads_request() || b.reads_request(),
    }
  }

  pub fn has_call(&self) -> bool {
    match self {
      Expr::Markup(tmpl) => {
        let mut calls = false;
        tmpl.visit(&mut |e| calls |= e.has_call());
        calls
      }
      Expr::Call { .. } | Expr::NativeCall { .. } | Expr::Next(_) => true,
      Expr::After(_) => false,
      Expr::Folded(_) | Expr::Var(_) | Expr::Param(_) | Expr::Query(_) | Expr::ParamsAll | Expr::QueryAll | Expr::Session(_) | Expr::Store(_) | Expr::Context(_) | Expr::Identity(_) | Expr::Locale | Expr::Locales | Expr::Path | Expr::Document | Expr::Host | Expr::Origin | Expr::Address | Expr::Config(_) | Expr::Input | Expr::Now | Expr::Const(_) | Expr::Lit(_) | Expr::Handler(_) | Expr::HoistId(_) => false,
      Expr::Object(entries) | Expr::Array(entries) => entries.iter().any(|entry| match entry {
        Entry::Field(_, e) | Entry::Item(e) | Entry::Spread(e) => e.has_call(),
        Entry::Baked(_) => false,
        Entry::Computed(k, v) => k.has_call() || v.has_call(),
      }),
      Expr::Field(e, _) | Expr::Not(e) | Expr::Entries(e) | Expr::Keys(e) | Expr::Values(e)
      | Expr::Length(e) | Expr::Str(e) | Expr::Num(e) | Expr::BigInt(e) => e.has_call(),
      Expr::Index(a, b) | Expr::Arith(_, a, b) | Expr::Compare(_, a, b) | Expr::Logic(_, a, b)
      | Expr::Coalesce(a, b) | Expr::Map(a, b) | Expr::Filter(a, b) | Expr::Find(a, b) | Expr::FindIndex(a, b)
      | Expr::Some(a, b) | Expr::Every(a, b) | Expr::Sort(a, b) | Expr::FlatMap(a, b) => a.has_call() || b.has_call(),
      Expr::Ternary(a, b, c) | Expr::Reduce(a, b, c) | Expr::ReplaceWith(a, b, c) => a.has_call() || b.has_call() || c.has_call(),
      Expr::Template(parts) => parts.iter().any(Expr::has_call),
      Expr::Builtin { args, .. } | Expr::Ext { args, .. } => args.iter().any(Expr::has_call),
      Expr::Apply { f, args } => f.has_call() || args.iter().any(Expr::has_call),
      Expr::ApplyOf(..) => true,
      Expr::Lambda { body, .. } => body.has_call(),
      Expr::Fix { f, .. } => f.has_call(),
      Expr::Block(body) => {
        let mut any = false;
        body_visit(body, &mut |e| any |= matches!(e, Expr::Call { .. } | Expr::NativeCall { .. } | Expr::Next(_)));
        any
      }
      Expr::Hoist { expr, .. } => expr.has_call(),
      Expr::Cell(e) | Expr::ReadCell(e) | Expr::WriteStore(_, e) => e.has_call(),
      Expr::WriteCell(a, b) | Expr::ModuleCell { key: a, init: b } => a.has_call() || b.has_call(),
    }
  }
}

/// The names a body reads that it does not bind itself, in first-read order.
/// A `let` binds for the statements after it and a `for-of` for its own body,
/// the way the interpreter scopes them.
/// The names a body's `set` statements write, at any depth.
pub fn set_targets(body: &Body) -> Vec<String> {
  let mut out = Vec::new();
  fn walk(body: &Body, out: &mut Vec<String>) {
    for stmt in body {
      match stmt {
        Stmt::Set { name, .. } => {
          if !out.contains(name) {
            out.push(name.clone());
          }
        }
        Stmt::If { then, r#else, .. } => {
          walk(then, out);
          walk(r#else, out);
        }
        Stmt::ForOf { body, .. } => walk(body, out),
        _ => {}
      }
    }
  }
  walk(body, &mut out);
  out
}

pub fn body_free_vars(body: &Body) -> Vec<String> {
  fn walk(body: &Body, bound: &mut Vec<String>, out: &mut Vec<String>) {
    let depth = bound.len();
    for stmt in body {
      let mut exprs: Vec<&Expr> = Vec::new();
      match stmt {
        Stmt::Let { expr, .. } | Stmt::Set { expr, .. } | Stmt::Return(expr) | Stmt::Expr(expr) => exprs.push(expr),
        Stmt::Guard { cond, message, .. } => {
          exprs.push(cond);
          exprs.push(message);
        }
        Stmt::Act { input, .. } => exprs.push(input),
        Stmt::SessionSet { path, value, .. } => {
          exprs.extend(path.iter());
          exprs.push(value);
        }
        Stmt::SessionDelete { path, .. } => exprs.extend(path.iter()),
        Stmt::SessionExtend { seconds } => exprs.push(seconds),
        Stmt::If { cond, .. } | Stmt::ForOf { over: cond, .. } => exprs.push(cond),
        Stmt::Try { .. } => {}
      }
      for expr in exprs {
        let mut free = Vec::new();
        expr.free_vars(&mut free);
        for name in free {
          if !bound.contains(&name) && !out.contains(&name) {
            out.push(name);
          }
        }
      }
      match stmt {
        Stmt::Let { name, .. } => bound.push(name.clone()),
        Stmt::Set { name, .. } => {
          if !bound.contains(name) && !out.contains(name) {
            out.push(name.clone());
          }
        }
        Stmt::If { then, r#else, .. } => {
          walk(then, bound, out);
          walk(r#else, bound, out);
        }
        Stmt::ForOf { name, body, .. } => {
          bound.push(name.clone());
          walk(body, bound, out);
          bound.truncate(bound.len() - 1);
        }
        Stmt::Try { body, name, handler, finally } => {
          walk(body, bound, out);
          bound.extend(name.iter().cloned());
          handler.iter().for_each(|handler| walk(handler, bound, out));
          bound.truncate(bound.len() - name.iter().count());
          walk(finally, bound, out);
        }
        _ => {}
      }
    }
    bound.truncate(depth);
  }
  let mut out = Vec::new();
  walk(body, &mut Vec::new(), &mut out);
  out
}

/// True when any statement of the body reads the request or writes the
/// session, so the body's result is not the same for every request.
pub fn body_reads_request(body: &Body) -> bool {
  body.iter().any(|stmt| match stmt {
    Stmt::Let { expr, .. } | Stmt::Set { expr, .. } | Stmt::Return(expr) | Stmt::Expr(expr) => expr.reads_request(),
    Stmt::If { cond, then, r#else } => cond.reads_request() || body_reads_request(then) || body_reads_request(r#else),
    Stmt::ForOf { over, body, .. } => over.reads_request() || body_reads_request(body),
    Stmt::Guard { cond, message, .. } => cond.reads_request() || message.reads_request(),
    Stmt::Try { body, handler, finally, .. } => body_reads_request(body) || handler.as_ref().is_some_and(|handler| body_reads_request(handler)) || body_reads_request(finally),
    Stmt::SessionSet { .. } | Stmt::SessionDelete { .. } | Stmt::SessionExtend { .. } | Stmt::Act { .. } => true,
  })
}

/// True when a body reads anything of the request other than its input: a
/// parameter, the query, the session, the identity or the clock. A `meta`
/// body's input is its loader's data, which is not the request and the
/// locale is left out for the reason `reads_request` gives.
pub fn body_reads_ambient(body: &Body) -> bool {
  let mut found = false;
  for expr in body_exprs(body) {
    expr.visit(&mut |e| {
      if matches!(e, Expr::Param(_) | Expr::Query(_) | Expr::ParamsAll | Expr::QueryAll | Expr::Session(_) | Expr::Store(_) | Expr::Context(_) | Expr::Identity(_) | Expr::Now) {
        found = true;
      }
    });
  }
  found
}

/// Every expression a body holds, in order, branches and loops included.
fn body_exprs(body: &Body) -> Vec<&Expr> {
  fn exprs<'a>(body: &'a Body, into: &mut Vec<&'a Expr>) {
    for stmt in body {
      match stmt {
        Stmt::Let { expr, .. } | Stmt::Set { expr, .. } | Stmt::Return(expr) | Stmt::Expr(expr) => into.push(expr),
        Stmt::If { cond, then, r#else } => {
          into.push(cond);
          exprs(then, into);
          exprs(r#else, into);
        }
        Stmt::ForOf { over, body, .. } => {
          into.push(over);
          exprs(body, into);
        }
        Stmt::Guard { cond, message, .. } => {
          into.push(cond);
          into.push(message);
        }
        Stmt::SessionSet { path, value, .. } => {
          into.extend(path.iter());
          into.push(value);
        }
        Stmt::SessionDelete { path, .. } => into.extend(path.iter()),
        Stmt::SessionExtend { seconds } => into.push(seconds),
        Stmt::Act { input, .. } => into.push(input),
        Stmt::Try { body, handler, finally, .. } => {
          exprs(body, into);
          handler.iter().for_each(|handler| exprs(handler, into));
          exprs(finally, into);
        }
      }
    }
  }
  let mut all = Vec::new();
  exprs(body, &mut all);
  all
}

/// The route parameters a body reads, by name, without duplicates.
pub fn body_params_read(body: &Body) -> Vec<String> {
  let mut out = Vec::new();
  for expr in body_exprs(body) {
    expr.visit(&mut |e| {
      if let Expr::Param(name) = e {
        if !out.contains(name) {
          out.push(name.clone());
        }
      }
    });
  }
  out
}

/// A `{ tag: "script" }` object a `meta` body puts in its head: its `src`
/// when the build can read it as a string and whether it carries inline code.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HeadScript {
  pub src: Option<String>,
  pub inline: bool,
}

/// The `src` of every `video`, `audio`, `source` and `track` in `tmpl` the
/// build can read as a string or the start of a template, in the order written.
pub fn media_sources(tmpl: &Tmpl) -> Vec<String> {
  literal_attrs(tmpl, &|tag| matches!(tag, "video" | "audio" | "source" | "track"), "src")
}

/// The `src` of every `img` in `tmpl` the build can read as a string or the start of a template, in the order written.
pub fn image_sources(tmpl: &Tmpl) -> Vec<String> {
  literal_attrs(tmpl, &|tag| tag == "img", "src")
}

/// The player URL of every embed placeholder in `tmpl`, its `data-sf-embed`. For a computed id it is the URL's start.
pub fn embed_players(tmpl: &Tmpl) -> Vec<String> {
  literal_attrs(tmpl, &|_| true, "data-sf-embed")
}

/// The start of the player URL of every embed placeholder in `tmpl` whose id the render computes.
pub fn computed_embed_players(tmpl: &Tmpl) -> Vec<String> {
  attr_values(tmpl, &|_| true, "data-sf-embed").into_iter().filter_map(|(text, computed)| computed.then_some(text)).collect()
}

/// The string value of `name` on every element `tag_matches` takes, in the order written.
/// A template whose first part is a string gives that part, which holds the URL's origin.
fn literal_attrs(tmpl: &Tmpl, tag_matches: &dyn Fn(&str) -> bool, name: &str) -> Vec<String> {
  attr_values(tmpl, tag_matches, name).into_iter().map(|(text, _)| text).collect()
}

/// `literal_attrs` with whether each value is a template the render fills.
fn attr_values(tmpl: &Tmpl, tag_matches: &dyn Fn(&str) -> bool, name: &str) -> Vec<(String, bool)> {
  fn walk(tmpl: &Tmpl, tag_matches: &dyn Fn(&str) -> bool, name: &str, out: &mut Vec<(String, bool)>) {
    match tmpl {
      Tmpl::Element { tag, attrs, children } => {
        if tag_matches(tag) {
          for entry in attrs {
            if let Entry::Field(field, expr) = entry {
              let text = match expr {
                Expr::Lit(Lit::Str(s)) => Some((s.clone(), false)),
                Expr::Folded(snapfire_fsr_core::value::Value::Str(s)) => Some((s.to_string(), false)),
                Expr::Template(parts) => match parts.first() {
                  Some(Expr::Lit(Lit::Str(s))) => Some((s.clone(), true)),
                  _ => None,
                },
                _ => None,
              };
              if let Some(text) = text.filter(|_| field == name) {
                out.push(text);
              }
            }
          }
        }
        children.iter().for_each(|child| walk(child, tag_matches, name, out));
      }
      Tmpl::Fragment(children) | Tmpl::Component { children, .. } | Tmpl::Island { children, .. } | Tmpl::Baked { children, .. } | Tmpl::Place { children, .. } => children.iter().for_each(|child| walk(child, tag_matches, name, out)),
      Tmpl::If { then, r#else, .. } => {
        walk(then, tag_matches, name, out);
        if let Some(other) = r#else {
          walk(other, tag_matches, name, out);
        }
      }
      Tmpl::For { body, .. } => walk(body, tag_matches, name, out),
      Tmpl::Let { then, .. } => walk(then, tag_matches, name, out),
      Tmpl::Text(_) | Tmpl::Expr(_) | Tmpl::Html(_) | Tmpl::Slot(_) | Tmpl::Marker(_) | Tmpl::Outlet { .. } => {}
    }
  }
  let mut out = Vec::new();
  walk(tmpl, tag_matches, name, &mut out);
  out
}

/// Every script row in `body`. A helper the lowerer inlined,
/// `scriptTag("/x.js")`, is a lambda applied to its arguments, so a
/// parameter is read as the literal it was given. A data block, `jsonLd`'s
/// `application/ld+json` among them, runs nothing and is not a script row; a
/// `type` the build cannot read is taken as code.
pub fn head_scripts(body: &Body) -> Vec<HeadScript> {
  fn text(e: &Expr, bound: &[(String, Option<String>)]) -> Option<String> {
    match e {
      Expr::Lit(Lit::Str(s)) => Some(s.clone()),
      Expr::Folded(snapfire_fsr_core::value::Value::Str(s)) => Some(s.to_string()),
      Expr::Var(name) => bound.iter().rev().find(|(n, _)| n == name).and_then(|(_, v)| v.clone()),
      _ => None,
    }
  }
  fn field<'a>(entries: &'a [Entry], name: &str) -> Option<&'a Expr> {
    entries.iter().find_map(|entry| match entry {
      Entry::Field(key, e) if key == name => Some(e),
      _ => None,
    })
  }
  fn script(e: &Expr, bound: &[(String, Option<String>)]) -> Option<HeadScript> {
    let Expr::Object(entries) = e else { return None };
    if field(entries, "tag").and_then(|t| text(t, bound)).as_deref() != Some("script") {
      return None;
    }
    if field(entries, "type").and_then(|t| text(t, bound)).is_some_and(|t| snapfire_fsr_runtime::is_data_block(Some(&t))) {
      return None;
    }
    let inline = field(entries, "children").is_some_and(|c| !matches!(c, Expr::Lit(Lit::Null)));
    Some(HeadScript { src: field(entries, "src").and_then(|src| text(src, bound)), inline })
  }
  let mut found: Vec<(*const Expr, HeadScript)> = Vec::new();
  let mut record = |at: *const Expr, row: HeadScript| match found.iter_mut().find(|(p, _)| *p == at) {
    Some(held) if held.1.src.is_none() => held.1 = row,
    Some(_) => {}
    None => found.push((at, row)),
  };
  body_visit(body, &mut |e| {
    if let Some(row) = script(e, &[]) {
      record(e as *const Expr, row);
    }
    if let Expr::Apply { f, args } = e {
      if let Expr::Lambda { params, body } = &**f {
        let bound: Vec<(String, Option<String>)> = params.iter().cloned().zip(args.iter().map(|a| text(a, &[])).chain(std::iter::repeat(None))).collect();
        body.visit(&mut |inner| {
          if let Some(row) = script(inner, &bound) {
            record(inner as *const Expr, row);
          }
        });
      }
    }
  });
  found.into_iter().map(|(_, row)| row).collect()
}
