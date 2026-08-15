# Migration: snapfire 0.4 → 0.5

snapfire 0.5 upgrades from Tera 1 to Tera 2. Tera 2 is a rewrite: both the Rust API and the template language changed.

Upstream guide: <https://github.com/Keats/tera/blob/master/MIGRATION.md>

## 1. Cargo.toml

```toml
[dependencies]
snapfire = "0.5"
tera = "2"
```

snapfire enables `tera/glob_fs` and `tera/fast`. Do not disable them; `TeraWeb::builder` and live reload require `glob_fs`.

## 2. snapfire API

| 0.4 | 0.5 |
| --- | --- |
| `SnapFireError::Tera(tera::Error)` wraps `tera` 1's error | wraps `tera` 2's error |
| `configure_tera` closure runs **after** templates are loaded | runs **before** templates are loaded |
| `TeraWeb: Debug` via derive | `TeraWeb: Debug` via manual impl, template contents not printed |

`TeraWeb::builder`, `add_global`, `render`, `watch_static`, `ws_path`, `auto_inject_script`, `configure_routes` and `build` are unchanged.

### configure_tera ordering

Tera 2 resolves filters, functions, tests and components at parse time and errors on an unknown name. Every registration must reach the instance before the glob is loaded. snapfire calls the `configure_tera` closure first, so a template referencing a custom filter now fails at `build()` instead of at render time when the closure is missing.

## 3. Custom filters, functions and tests

```rust
// 0.4
fn upcase(value: &tera::Value, _: &HashMap<String, tera::Value>) -> tera::Result<tera::Value> {
  let s = tera::from_value::<String>(value.clone())?;
  Ok(tera::to_value(s.to_uppercase()).unwrap())
}

// 0.5
fn upcase(value: &str, _: tera::Kwargs, _: &tera::State) -> String {
  value.to_uppercase()
}
```

| 0.4 | 0.5 |
| --- | --- |
| `tera::Result<T>` | `tera::TeraResult<T>` |
| `tera::Value` (re-export of `serde_json::Value`) | `tera::Value` (Tera's own enum) |
| `tera::from_value::<T>(v)` | `T::deserialize(v)`, or take a typed argument |
| `tera::to_value(v)` | `tera::Value::from_serialize(v)`, or return the value directly |
| `&HashMap<String, Value>` args | `tera::Kwargs` |
| no context access | `&tera::State` third argument |
| return `tera::Result<Value>` | return any `T: Into<Value>` or `TeraResult<T>` |

Argument types are converted for you: `&str`, `String`, `Cow<str>`, `i64`, `f64`, `bool`, `Vec<T>`, `&[Value]`, `Map`, `&Map`, `Value` and `&Value` are all accepted directly.

Tests always take kwargs now.

## 4. Template syntax

| 0.4 | 0.5 |
| --- | --- |
| `{% macro %}` / `{% import %}` | removed, use components |
| `my_vec.0` | `my_vec[0]` |
| `{{ undefined }}` renders empty | errors |
| `{{ existing.undefined }}` renders empty | errors |
| `{% if undefined.field %}` | errors |
| `{% if undefined %}` | still allowed, one level of undefined only |

## 5. Builtin filters, tests and functions

Renamed:

| 0.4 | 0.5 | Kind |
| --- | --- | --- |
| `as_str` | `str` | filter |
| `escape` | `escape_html` | filter |
| `linebreaksbr` | `newlines_to_br` | filter |
| `divisibleby` | `divisible_by` | test |
| `object` | `map` | test |

Removed, with no replacement:

| Name | Kind |
| --- | --- |
| `addslashes`, `concat`, `date`, `filesizeformat`, `filter`, `json_encode`, `map`, `slice`, `slugify`, `spaceless`, `striptags`, `urlencode`, `urlencode_strict` | filter |
| `matching` | test |
| `get_env`, `get_random`, `now` | function |

Changed:

| Name | Change |
| --- | --- |
| `trim_start_matches`, `trim_end_matches` | merged into `trim`, `trim_start`, `trim_end` with an optional `pat` argument |
| `int`, `float` | no default value |
| `round` | no `common` method |
| `indent` | takes `width` instead of `prefix` |
| `truncate` | `length` is required, no longer defaults to 255 |
| `unique` | takes no arguments |
| `first`, `last`, `nth` | return `None` on an empty array instead of an empty string |

New in Tera 2: `keys`, `values`, `pairs` filters; `array`, `bool`, `float`, `integer`, `none` tests.
