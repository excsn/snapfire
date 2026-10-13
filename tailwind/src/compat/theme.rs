//! `theme(colors.red.500)`: the v3 dotted lookup. Values come from the CSS theme first and
//! from Tailwind's v3 default theme second, merged the way its `resolveConfig` and
//! `createThemeFn` merge them.

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::LazyLock;

use indexmap::IndexMap;
use regex::Regex;

use super::default_theme::default_theme;
use crate::design_system::DesignSystem;
use crate::theme::{Theme, ThemeOptions};
use crate::utilities::with_alpha;
use crate::utils::escape::unescape;
use crate::utils::segment::segment;

/// A value in the default theme as written: literals, `theme('…')` calls and the functions
/// that wrap them.
#[derive(Debug, Clone)]
pub enum E {
  Str(&'static str),
  Arr(Vec<E>),
  Obj(Vec<Entry>),
  Theme(&'static str, Option<Box<E>>),
  Fn(Box<E>),
  Bare,
  Colors,
}

#[derive(Debug, Clone)]
pub enum Entry {
  Prop(&'static str, E),
  Spread(E),
}

/// A JavaScript value, enough of one for theme objects.
#[derive(Debug, Clone)]
enum Js {
  Undefined,
  Null,
  Num(f64),
  Str(String),
  Arr(Vec<Js>),
  Obj(IndexMap<String, Js>),
  Func(Rc<Lazy>),
  Bare,
}

#[derive(Debug)]
enum Lazy {
  Expr(E),
  Merge(Vec<Js>),
}

impl Js {
  fn is_nullish(&self) -> bool {
    matches!(self, Js::Undefined | Js::Null)
  }

  fn is_object_like(&self) -> bool {
    matches!(self, Js::Null | Js::Arr(_) | Js::Obj(_))
  }

  fn or(self, other: Js) -> Js {
    if self.is_nullish() { other } else { self }
  }

  fn num(&self) -> u8 {
    match self {
      Js::Num(n) => *n as u8,
      _ => 0,
    }
  }
}

pub struct LegacyTheme {
  resolved: RefCell<Option<IndexMap<String, Js>>>,
}

impl Default for LegacyTheme {
  fn default() -> Self {
    LegacyTheme {
      resolved: RefCell::new(None),
    }
  }
}

impl std::fmt::Debug for LegacyTheme {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    f.write_str("LegacyTheme")
  }
}

impl LegacyTheme {
  /// What `theme(path)` resolves to, as the compatibility layer's `resolveThemeValue` returns it.
  pub fn resolve(&self, path: &str, ds: &DesignSystem) -> Option<String> {
    if self.resolved.borrow().is_none() {
      let resolved = resolve_config(&ds.theme);
      *self.resolved.borrow_mut() = Some(resolved);
    }
    let resolved = self.resolved.borrow();
    let config = resolved.as_ref().expect("the resolved config");
    let value = theme_fn(&ds.theme, config, path, Js::Undefined, &|v, _| v);
    match value {
      Js::Arr(items) if items.len() == 2 => to_string(&items[0]),
      Js::Arr(items) => Some(items.iter().filter_map(to_string).collect::<Vec<_>>().join(", ")),
      Js::Obj(map) => map.get("DEFAULT").and_then(to_string),
      Js::Str(s) => Some(s),
      _ => None,
    }
  }
}

fn to_string(value: &Js) -> Option<String> {
  match value {
    Js::Str(s) => Some(s.clone()),
    Js::Num(n) => Some(crate::utils::js::number_to_string(*n)),
    Js::Arr(items) => Some(items.iter().filter_map(to_string).collect::<Vec<_>>().join(",")),
    Js::Obj(_) => Some("[object Object]".to_owned()),
    _ => None,
  }
}

type Resolver<'a> = dyn Fn(Js, &IndexMap<String, Js>) -> Js + 'a;

fn eval(e: &E, theme: &Theme, config: &IndexMap<String, Js>, resolve: &Resolver) -> Js {
  match e {
    E::Str(s) => Js::Str((*s).to_owned()),
    E::Arr(items) => Js::Arr(items.iter().map(|i| eval(i, theme, config, resolve)).collect()),
    E::Obj(entries) => {
      let mut map = IndexMap::new();
      for entry in entries {
        match entry {
          Entry::Prop(k, v) => {
            map.insert((*k).to_owned(), eval(v, theme, config, resolve));
          }
          Entry::Spread(v) => spread(&mut map, eval(v, theme, config, resolve)),
        }
      }
      Js::Obj(map)
    }
    E::Theme(path, fallback) => {
      let fallback = match fallback {
        Some(f) => eval(f, theme, config, resolve),
        None => Js::Undefined,
      };
      theme_fn(theme, config, path, fallback, resolve)
    }
    E::Fn(body) => Js::Func(Rc::new(Lazy::Expr((**body).clone()))),
    E::Bare => Js::Bare,
    E::Colors => Js::Obj(IndexMap::new()),
  }
}

fn spread(map: &mut IndexMap<String, Js>, value: Js) {
  match value {
    Js::Obj(source) => map.extend(source),
    Js::Arr(items) => {
      for (i, item) in items.into_iter().enumerate() {
        map.insert(i.to_string(), item);
      }
    }
    Js::Str(s) => {
      for (i, c) in s.chars().enumerate() {
        map.insert(i.to_string(), Js::Str(c.to_string()));
      }
    }
    _ => {}
  }
}

fn call(lazy: &Lazy, theme: &Theme, config: &IndexMap<String, Js>, resolve: &Resolver) -> Js {
  match lazy {
    Lazy::Expr(e) => eval(e, theme, config, resolve),
    Lazy::Merge(values) => {
      let resolved: Vec<Js> = values.iter().map(|v| resolve(v.clone(), config)).collect();
      deep_merge(IndexMap::new(), &resolved, &merge_theme_extension)
    }
  }
}

fn is_plain_object(v: &Js) -> bool {
  matches!(v, Js::Obj(_))
}

fn merge_theme_extension(theme: &Js, extension: &Js) -> Option<Js> {
  if let Js::Arr(items) = theme
    && items.first().is_some_and(is_plain_object)
  {
    let mut out = items.clone();
    match extension {
      Js::Arr(ext) => out.extend(ext.iter().cloned()),
      other => out.push(other.clone()),
    }
    return Some(Js::Arr(out));
  }
  if let Js::Arr(ext) = extension
    && ext.first().is_some_and(is_plain_object)
    && is_plain_object(theme)
  {
    let mut out = vec![theme.clone()];
    out.extend(ext.iter().cloned());
    return Some(Js::Arr(out));
  }
  if let Js::Arr(_) = extension {
    return Some(extension.clone());
  }
  None
}

fn deep_merge(mut target: IndexMap<String, Js>, sources: &[Js], customizer: &dyn Fn(&Js, &Js) -> Option<Js>) -> Js {
  for source in sources {
    let entries: Vec<(String, Js)> = match source {
      Js::Obj(map) => map.iter().map(|(k, v)| (k.clone(), v.clone())).collect(),
      Js::Arr(items) => items
        .iter()
        .enumerate()
        .map(|(i, v)| (i.to_string(), v.clone()))
        .collect(),
      _ => continue,
    };
    for (k, v) in entries {
      let existing = target.get(&k).cloned().unwrap_or(Js::Undefined);
      let merged = match customizer(&existing, &v) {
        Some(m) => m,
        None if !is_plain_object(&existing) || !is_plain_object(&v) => v,
        None => deep_merge(IndexMap::new(), &[existing, v], customizer),
      };
      target.insert(k, merged);
    }
  }
  Js::Obj(target)
}

fn resolve_value(value: Js, theme: &Theme, config: &IndexMap<String, Js>) -> Js {
  match value {
    Js::Func(lazy) => {
      let resolver = |v: Js, c: &IndexMap<String, Js>| resolve_value(v, theme, c);
      call(&lazy, theme, config, &resolver).or(Js::Null)
    }
    other => other.or(Js::Null),
  }
}

/// `createCompatConfig` merged by `resolveConfig`: the default theme with colors from the CSS
/// theme only and the CSS namespaces extended in.
fn resolve_config(theme: &Theme) -> IndexMap<String, Js> {
  let mut config: IndexMap<String, Js> = IndexMap::new();
  let empty = || Js::Obj(IndexMap::new());
  let resolver = |v: Js, c: &IndexMap<String, Js>| resolve_value(v, theme, c);

  for (key, value) in default_theme() {
    config.insert(key.to_owned(), eval(&value, theme, &config.clone(), &resolver));
  }
  config.insert(
    "colors".to_owned(),
    Js::Func(Rc::new(Lazy::Expr(E::Theme(
      "color",
      Some(Box::new(E::Obj(Vec::new()))),
    )))),
  );

  let spread_of = |ns: &'static str| {
    Js::Func(Rc::new(Lazy::Expr(E::Obj(vec![Entry::Spread(E::Theme(
      ns,
      Some(Box::new(E::Obj(Vec::new()))),
    ))]))))
  };
  let default_of = |var: &str| {
    let mut map = IndexMap::new();
    map.insert("DEFAULT".to_owned(), theme.get(&[var]).map(Js::Str).unwrap_or(Js::Null));
    Js::Obj(map)
  };
  let extend: Vec<(&str, Js)> = vec![
    ("fontSize", spread_of("text")),
    ("boxShadow", spread_of("shadow")),
    ("animation", spread_of("animate")),
    ("aspectRatio", spread_of("aspect")),
    ("borderRadius", spread_of("radius")),
    ("screens", spread_of("breakpoint")),
    ("letterSpacing", spread_of("tracking")),
    ("lineHeight", spread_of("leading")),
    ("transitionDuration", default_of("--default-transition-duration")),
    (
      "transitionTimingFunction",
      default_of("--default-transition-timing-function"),
    ),
    ("maxWidth", spread_of("container")),
  ];
  let _ = empty;

  for (key, extension) in extend {
    let base = config.get(key).cloned().unwrap_or(Js::Undefined);
    config.insert(key.to_owned(), Js::Func(Rc::new(Lazy::Merge(vec![base, extension]))));
  }

  let keys: Vec<String> = config.keys().cloned().collect();
  for key in keys {
    let value = config.get(&key).cloned().unwrap_or(Js::Undefined);
    let resolved = resolve_value(value, theme, &config);
    config.insert(key, resolved);
  }

  if let Some(Js::Obj(screens)) = config.get_mut("screens") {
    for value in screens.values_mut() {
      if let Js::Obj(screen) = value
        && !screen.contains_key("raw")
        && !screen.contains_key("max")
        && let Some(min) = screen.get("min").cloned()
      {
        *value = min;
      }
    }
  }

  config
}

fn get(obj: &Js, path: &[String]) -> Js {
  let mut path: Vec<String> = path.to_vec();
  let mut current = obj.clone();
  let mut i = 0;
  while i < path.len() {
    let key = path[i].clone();
    let found = match &current {
      Js::Obj(map) => map.get(&key).cloned(),
      Js::Arr(items) => key.parse::<usize>().ok().and_then(|n| items.get(n).cloned()),
      _ => None,
    };
    match found {
      Some(v) => current = v,
      None => {
        if i + 1 >= path.len() {
          return Js::Undefined;
        }
        path[i + 1] = format!("{key}-{}", path[i + 1]);
        i += 1;
        continue;
      }
    }
    i += 1;
  }
  current
}

fn theme_fn(theme: &Theme, config: &IndexMap<String, Js>, path: &str, fallback: Js, resolve: &Resolver) -> Js {
  let (path, modifier) = match path.rfind('/') {
    Some(i) => (path[..i].trim(), Some(path[i + 1..].trim().to_owned())),
    None => (path, None),
  };

  let keypath = to_key_path(path);
  let (mut css_value, options) = read_from_css(theme, &keypath);
  let mut config_value = resolve(get(&Js::Obj(config.clone()), &keypath).or(Js::Null), config);
  if let Js::Str(s) = &config_value {
    config_value = Js::Str(s.replacen("<alpha-value>", "1", 1));
  }

  let resolved = (|| {
    if !css_value.is_object_like() {
      if !matches!(options, Js::Obj(_) | Js::Arr(_)) && options.num() & ThemeOptions::DEFAULT.0 != 0 {
        return config_value.clone().or(css_value.clone());
      }
      return css_value.clone();
    }

    if let Js::Obj(config_map) = &config_value {
      let mut copy = config_map.clone();
      let css_values = config_map.get("__CSS_VALUES__").cloned();
      if matches!(css_value, Js::Null)
        && let Some(Js::Obj(values)) = &css_values
      {
        let mut local = IndexMap::new();
        for key in values.keys() {
          local.insert(key.clone(), config_map.get(key).cloned().unwrap_or(Js::Undefined));
          copy.shift_remove(key);
        }
        css_value = Js::Obj(local);
      }
      if let Js::Obj(css_map) = &css_value {
        for (key, value) in css_map {
          if key == "__CSS_VALUES__" {
            continue;
          }
          let is_default = match &css_values {
            Some(Js::Obj(values)) => values.get(key).map(Js::num).unwrap_or(0) & ThemeOptions::DEFAULT.0 != 0,
            _ => false,
          };
          let parts: Vec<String> = key.split('-').map(str::to_owned).collect();
          if is_default && !matches!(get(&Js::Obj(copy.clone()), &parts), Js::Undefined) {
            continue;
          }
          copy.insert(unescape(key), value.clone());
        }
      }
      return Js::Obj(copy);
    }

    if let (Js::Arr(css), Js::Arr(opts), Js::Arr(cfg)) = (&css_value, &options, &config_value) {
      let mut base = css.first().cloned().unwrap_or(Js::Undefined);
      let mut extra = match css.get(1) {
        Some(Js::Obj(m)) => m.clone(),
        _ => IndexMap::new(),
      };
      if opts.first().map(Js::num).unwrap_or(0) & ThemeOptions::DEFAULT.0 != 0 {
        base = cfg.first().cloned().unwrap_or(Js::Undefined).or(base);
      }
      let extra_options = match opts.get(1) {
        Some(Js::Obj(m)) => m.clone(),
        _ => IndexMap::new(),
      };
      let cfg_extra = match cfg.get(1) {
        Some(Js::Obj(m)) => m.clone(),
        _ => IndexMap::new(),
      };
      let keys: Vec<String> = extra.keys().cloned().collect();
      for key in keys {
        if extra_options.get(&key).map(Js::num).unwrap_or(0) & ThemeOptions::DEFAULT.0 != 0 {
          let current = extra.get(&key).cloned().unwrap_or(Js::Undefined);
          extra.insert(
            key.clone(),
            cfg_extra.get(&key).cloned().unwrap_or(Js::Undefined).or(current),
          );
        }
      }
      return Js::Arr(vec![base, Js::Obj(extra)]);
    }

    css_value.clone().or(config_value.clone())
  })();

  let resolved = match (&modifier, resolved) {
    (Some(m), Js::Str(s)) if !m.is_empty() => Js::Str(with_alpha(&s, m)),
    (_, other) => other,
  };
  resolved.or(fallback)
}

fn read_from_css(theme: &Theme, path: &[String]) -> (Js, Js) {
  if path.len() == 1 && path[0].starts_with("--") {
    let value = theme.get(&[path[0].as_str()]).map(Js::Str).unwrap_or(Js::Null);
    return (value, Js::Num(theme.get_options(&path[0]).0 as f64));
  }

  let theme_key = key_path_to_css_property(path).unwrap_or_else(|| "null".to_owned());
  let namespace = theme.namespace(&format!("--{theme_key}"));
  if namespace.is_empty() {
    return (Js::Null, Js::Num(0.0));
  }

  let mut map: IndexMap<Option<String>, Js> = IndexMap::new();
  let mut options: IndexMap<Option<String>, Js> = IndexMap::new();
  let mut nested: IndexMap<Option<String>, IndexMap<String, (String, u8)>> = IndexMap::new();

  for (key, value) in &namespace {
    match key {
      Some(k) if k.contains("--") => {
        let idx = k.find("--").unwrap();
        let main = &k[..idx];
        let nested_key = camel_case(&k[idx + 2..]);
        let opts = theme.get_options(&format!("--{theme_key}{k}")).0;
        nested
          .entry(if main.is_empty() { None } else { Some(main.to_owned()) })
          .or_default()
          .insert(nested_key, (value.clone(), opts));
      }
      _ => {
        map.insert(key.clone(), Js::Str(value.clone()));
        let option_key = match key {
          Some(k) if !k.is_empty() => format!("--{theme_key}-{k}"),
          _ => format!("--{theme_key}"),
        };
        options.insert(key.clone(), Js::Num(theme.get_options(&option_key).0 as f64));
      }
    }
  }

  let base = theme.get_options(&format!("--{theme_key}")).0;
  for (key, extra) in nested {
    let Some(Js::Str(value)) = map.get(&key).cloned() else {
      continue;
    };
    let mut extra_obj = IndexMap::new();
    let mut extra_options = IndexMap::new();
    for (k, (v, o)) in extra {
      extra_obj.insert(k.clone(), Js::Str(v));
      extra_options.insert(k, Js::Num(o as f64));
    }
    map.insert(key.clone(), Js::Arr(vec![Js::Str(value), Js::Obj(extra_obj)]));
    options.insert(key, Js::Arr(vec![Js::Num(base as f64), Js::Obj(extra_options)]));
  }

  let mut obj: IndexMap<String, Js> = IndexMap::new();
  for (key, value) in map {
    obj.insert(key.unwrap_or_else(|| "DEFAULT".to_owned()), value);
  }
  let mut options_obj: IndexMap<String, Js> = IndexMap::new();
  for (key, value) in options {
    options_obj.insert(key.unwrap_or_else(|| "DEFAULT".to_owned()), value);
  }

  if path.last().map(String::as_str) == Some("DEFAULT") {
    return (
      obj.get("DEFAULT").cloned().unwrap_or(Js::Null),
      options_obj.get("DEFAULT").cloned().unwrap_or(Js::Num(0.0)),
    );
  }
  if obj.contains_key("DEFAULT") && obj.len() == 1 {
    return (
      obj.get("DEFAULT").cloned().unwrap(),
      options_obj.get("DEFAULT").cloned().unwrap_or(Js::Num(0.0)),
    );
  }

  obj.insert("__CSS_VALUES__".to_owned(), Js::Obj(options_obj.clone()));
  (Js::Obj(obj), Js::Obj(options_obj))
}

fn camel_case(input: &str) -> String {
  let mut out = String::new();
  let mut chars = input.chars().peekable();
  while let Some(c) = chars.next() {
    if c == '-'
      && let Some(next) = chars.peek()
      && next.is_ascii_lowercase()
    {
      out.push(next.to_ascii_uppercase());
      chars.next();
      continue;
    }
    out.push(c);
  }
  out
}

/// `a[b.c].e` as `["a", "b.c", "e"]`.
fn to_key_path(path: &str) -> Vec<String> {
  let mut keypath = Vec::new();
  for part in segment(path, b'.') {
    if !part.contains('[') {
      keypath.push(part.to_owned());
      continue;
    }
    let mut current = 0;
    while let Some(left) = part[current..].find('[').map(|i| i + current) {
      let Some(right) = part[left..].find(']').map(|i| i + left) else {
        break;
      };
      if left > current {
        keypath.push(part[current..left].to_owned());
      }
      keypath.push(part[left + 1..right].to_owned());
      current = right + 1;
    }
    if current < part.len() {
      keypath.push(part[current..].to_owned());
    }
  }
  keypath
}

static IS_VALID_KEY: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^[a-zA-Z0-9\-_%/.]+$").unwrap());
static CAMEL: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"([a-z])([A-Z])").unwrap());

fn key_path_to_css_property(path: &[String]) -> Option<String> {
  let special = match path.first().map(String::as_str) {
    Some("borderWidth") => Some("border-width"),
    Some("outlineWidth") => Some("outline-width"),
    Some("ringColor") => Some("ring-color"),
    Some("ringWidth") => Some("ring-width"),
    Some("transitionDuration") => Some("transition-duration"),
    Some("transitionTimingFunction") => Some("transition-timing-function"),
    _ => None,
  };
  if let Some(special) = special
    && path.get(1).map(String::as_str) == Some("DEFAULT")
  {
    return Some(format!("default-{special}"));
  }
  if path.first().map(String::as_str) == Some("container") {
    return None;
  }
  if path.iter().any(|p| !IS_VALID_KEY.is_match(p)) {
    return None;
  }

  let mut path: Vec<String> = path.to_vec();
  let renamed = match path[0].as_str() {
    "animation" => Some("animate"),
    "aspectRatio" => Some("aspect"),
    "borderRadius" => Some("radius"),
    "boxShadow" => Some("shadow"),
    "colors" => Some("color"),
    "containers" => Some("container"),
    "fontFamily" => Some("font"),
    "fontSize" => Some("text"),
    "letterSpacing" => Some("tracking"),
    "lineHeight" => Some("leading"),
    "maxWidth" => Some("container"),
    "screens" => Some("breakpoint"),
    "transitionTimingFunction" => Some("ease"),
    _ => None,
  };
  if let Some(ns) = renamed {
    path[0] = ns.to_owned();
  }

  let len = path.len();
  let parts: Vec<String> = path
    .iter()
    .enumerate()
    .map(|(idx, p)| {
      if p == "1" && idx != len - 1 {
        String::new()
      } else {
        p.clone()
      }
    })
    .enumerate()
    .map(|(idx, part)| {
      let part = part.replace('.', "_");
      if idx == 0 || part.starts_with('-') || part == "lineHeight" {
        CAMEL
          .replace_all(&part, |c: &regex::Captures| {
            format!("{}-{}", &c[1], c[2].to_lowercase())
          })
          .into_owned()
      } else {
        part
      }
    })
    .enumerate()
    .filter(|(index, part)| part != "DEFAULT" || *index != len - 1)
    .map(|(_, part)| part)
    .collect();
  Some(parts.join("-"))
}
