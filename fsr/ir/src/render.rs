//! Renders a lowered component to HTML. The tree is the component's own, so
//! the serialiser is a string builder: elements, escaped text and the three
//! idioms. What the browser hydrates over is exactly this output, so it
//! follows React's server renderer byte for byte: adjacent text nodes are
//! separated by an empty comment, empty text writes nothing, a boolean
//! attribute is `name=""`, a void element closes with `/>`.

use std::collections::HashMap;
use std::sync::Arc;

use futures_util::future::BoxFuture;
use snapfire_fsr_core::{Value, ValueMap};

use crate::ast::{Component, Entry, Stmt, Tmpl};
use crate::interp::{Env, Fail, Interpreter, stringify, truthy};

/// Every lowered component by module id, so one may render another.
pub type Components = HashMap<String, Arc<Component>>;

const VOID: &[&str] = &["area", "base", "br", "col", "embed", "hr", "img", "input", "link", "meta", "source", "track", "wbr"];

/// Attributes that are present or absent rather than valued.
const BOOLEAN: &[&str] = &["disabled", "checked", "selected", "readonly", "required", "hidden", "multiple", "open", "autofocus", "autoplay", "controls", "loop", "muted", "novalidate", "defer", "async"];

fn escape_text(input: &str, out: &mut String) {
  for c in input.chars() {
    match c {
      '&' => out.push_str("&amp;"),
      '<' => out.push_str("&lt;"),
      '>' => out.push_str("&gt;"),
      c => out.push(c),
    }
  }
}

fn escape_attr(input: &str, out: &mut String) {
  for c in input.chars() {
    match c {
      '&' => out.push_str("&amp;"),
      '<' => out.push_str("&lt;"),
      '>' => out.push_str("&gt;"),
      '"' => out.push_str("&quot;"),
      c => out.push(c),
    }
  }
}

/// The output and whether the last thing written was text, which decides
/// whether the next text needs React's `<!-- -->` between them.
#[derive(Default)]
struct Out {
  html: String,
  text_open: bool,
}

impl Out {
  fn text(&mut self, text: &str) {
    if text.is_empty() {
      return;
    }
    if self.text_open {
      self.html.push_str("<!-- -->");
    }
    escape_text(text, &mut self.html);
    self.text_open = true;
  }

  fn markup(&mut self, html: &str) {
    self.html.push_str(html);
    self.text_open = false;
  }
}

/// A caller's children and the scope they read, rendered wherever the callee places its `Slot`.
struct Slot {
  children: Vec<Tmpl>,
  scope: Vec<(String, Value)>,
}

impl Interpreter {
  /// Renders `component` with `props` bound as `$props`.
  pub async fn render(&self, component: &Component, props: &ValueMap, library: &Components) -> Result<String, Fail> {
    let mut env = Env::detached(self.clock(), vec![("$props".to_owned(), Value::Map(props.clone()))]);
    let mut out = Out::default();
    let mut slots = Vec::new();
    render_component(&mut env, component, library, &mut slots, &mut out).await?;
    Ok(out.html)
  }
}

fn render_component<'a>(env: &'a mut Env, component: &'a Component, library: &'a Components, slots: &'a mut Vec<Slot>, out: &'a mut Out) -> BoxFuture<'a, Result<(), Fail>> {
  Box::pin(async move {
    let depth = env.scope.len();
    for stmt in &component.body {
      let Stmt::Let { name, expr } = stmt else {
        return Err(Fail::internal("a component body is `let`s only"));
      };
      let value = env.eval(expr).await?;
      env.scope.push((name.clone(), value));
    }
    render(env, &component.render, library, slots, out).await?;
    env.scope.truncate(depth);
    Ok(())
  })
}

/// Evaluates attribute or prop entries into one map in order, later entries winning; attributes are keyed by HTML spelling so a spread's `className` and a literal `class` are one key.
async fn entries(env: &mut Env, entries: &[Entry], attrs: bool) -> Result<Vec<(String, Value)>, Fail> {
  let mut out: Vec<(String, Value)> = Vec::new();
  let mut put = |name: String, value: Value| {
    let name = if attrs { html_attr_name(&name).to_owned() } else { name };
    if let Some(slot) = out.iter_mut().find(|(n, _)| *n == name) {
      slot.1 = value;
    } else {
      out.push((name, value));
    }
  };
  for entry in entries {
    match entry {
      Entry::Field(name, expr) => put(name.clone(), env.eval(expr).await?),
      Entry::Spread(expr) => match env.eval(expr).await? {
        Value::Map(map) => {
          for (name, value) in map {
            put(name, value);
          }
        }
        Value::Null => {}
        other => return Err(crate::interp::type_error("spread", "an object", &other)),
      },
      Entry::Computed(key, expr) => {
        let key = stringify(&env.eval(key).await?)?;
        put(key, env.eval(expr).await?);
      }
      Entry::Item(_) => return Err(Fail::internal("an item entry among attributes")),
    }
  }
  Ok(out)
}

fn render<'a>(env: &'a mut Env, tmpl: &'a Tmpl, library: &'a Components, slots: &'a mut Vec<Slot>, out: &'a mut Out) -> BoxFuture<'a, Result<(), Fail>> {
  Box::pin(async move {
    match tmpl {
      Tmpl::Text(text) => out.text(text),
      Tmpl::Expr(expr) => {
        let value = env.eval(expr).await?;
        interpolate(&value, out)?;
      }
      Tmpl::Element { tag, attrs, children } => {
        let mut open = format!("<{tag}");
        for (name, value) in entries(env, attrs, true).await? {
          if skipped_attr(&name) {
            continue;
          }
          attribute(&name, &value, &mut open)?;
        }
        if VOID.contains(&tag.as_str()) {
          open.push_str("/>");
          out.markup(&open);
          return Ok(());
        }
        open.push('>');
        out.markup(&open);
        for child in children {
          render(env, child, library, slots, out).await?;
        }
        out.markup(&format!("</{tag}>"));
      }
      Tmpl::Fragment(children) => {
        for child in children {
          render(env, child, library, slots, out).await?;
        }
      }
      Tmpl::If { cond, then, r#else } => {
        let value = env.eval(cond).await?;
        if truthy(&value) {
          render(env, then, library, slots, out).await?;
        } else if let Some(other) = r#else {
          render(env, other, library, slots, out).await?;
        }
      }
      Tmpl::For { over, params, body } => {
        let items = match env.eval(over).await? {
          Value::Seq(items) => items,
          other => return Err(crate::interp::type_error("map", "an array", &other)),
        };
        let depth = env.scope.len();
        for (i, item) in items.into_iter().enumerate() {
          env.scope.truncate(depth);
          for (param, value) in params.iter().zip([item, Value::F64(i as f64)]) {
            env.scope.push((param.clone(), value));
          }
          render(env, body, library, slots, out).await?;
        }
        env.scope.truncate(depth);
      }
      Tmpl::Let { name, expr, then } => {
        let value = env.eval(expr).await?;
        let depth = env.scope.len();
        env.scope.push((name.clone(), value));
        render(env, then, library, slots, out).await?;
        env.scope.truncate(depth);
      }
      Tmpl::Component { module, props, children } => {
        let component = library.get(module).cloned().ok_or_else(|| Fail::internal(format!("`{module}` is not a lowered component")))?;
        let mut map = ValueMap::new();
        for (name, value) in entries(env, props, false).await? {
          if name != "children" {
            map.insert(name, value);
          }
        }
        let depth = env.scope.len();
        let outer = std::mem::replace(&mut env.scope, vec![("$props".to_owned(), Value::Map(map))]);
        slots.push(Slot { children: children.clone(), scope: outer.clone() });
        let result = render_component(env, &component, library, slots, out).await;
        slots.pop();
        env.scope = outer;
        env.scope.truncate(depth);
        result?;
      }
      Tmpl::Slot => {
        let Some(slot) = slots.pop() else { return Ok(()) };
        let inner = std::mem::replace(&mut env.scope, slot.scope.clone());
        let mut result = Ok(());
        for child in &slot.children {
          result = render(env, child, library, slots, out).await;
          if result.is_err() {
            break;
          }
        }
        env.scope = inner;
        slots.push(slot);
        result?;
      }
    }
    Ok(())
  })
}

/// CSS properties React leaves unitless; every other number gets `px`.
const UNITLESS: &[&str] = &["animation-iteration-count", "aspect-ratio", "border-image-outset", "border-image-slice", "border-image-width", "box-flex", "box-flex-group", "box-ordinal-group", "column-count", "columns", "flex", "flex-grow", "flex-positive", "flex-shrink", "flex-negative", "flex-order", "grid-area", "grid-row", "grid-row-end", "grid-row-span", "grid-row-start", "grid-column", "grid-column-end", "grid-column-span", "grid-column-start", "font-weight", "line-clamp", "line-height", "opacity", "order", "orphans", "scale", "tab-size", "widows", "z-index", "zoom", "fill-opacity", "flood-opacity", "stop-opacity", "stroke-dasharray", "stroke-dashoffset", "stroke-miterlimit", "stroke-opacity", "stroke-width"];

/// A style object the way React's server renderer prints it: `name:value` joined by `;`, null and empty values skipped, a number in `px` unless the property is unitless or the number is zero.
fn style_text(map: &ValueMap) -> Result<String, Fail> {
  let mut out = String::new();
  for (name, value) in map {
    let text = match value {
      Value::Null | Value::Bool(_) => continue,
      Value::Str(s) if s.trim().is_empty() => continue,
      Value::Str(s) => s.trim().to_owned(),
      Value::Int(0) => "0".to_owned(),
      Value::F64(f) if *f == 0.0 => "0".to_owned(),
      Value::Int(_) | Value::F64(_) | Value::F32(_) | Value::UInt(_) => {
        let n = stringify(value)?;
        if UNITLESS.contains(&name.as_str()) || name.starts_with("--") { n } else { format!("{n}px") }
      }
      other => stringify(other)?,
    };
    if !out.is_empty() {
      out.push(';');
    }
    out.push_str(name);
    out.push(':');
    out.push_str(&text);
  }
  Ok(out)
}

/// Attribute keys the browser owns or that name no attribute: handlers, `key`, `ref`, `children` and `dangerouslySetInnerHTML`.
fn skipped_attr(name: &str) -> bool {
  name == "key" || name == "ref" || name == "children" || name == "dangerouslySetInnerHTML" || (name.len() > 2 && name.starts_with("on") && name.as_bytes()[2].is_ascii_uppercase())
}

/// React's attribute spellings to HTML's; an HTML spelling passes through.
pub fn html_attr_name(name: &str) -> &str {
  match name {
    "className" => "class",
    "htmlFor" => "for",
    "readOnly" => "readonly",
    "autoFocus" => "autofocus",
    "autoComplete" => "autocomplete",
    "tabIndex" => "tabindex",
    "defaultValue" => "value",
    "defaultChecked" => "checked",
    "maxLength" => "maxlength",
    "minLength" => "minlength",
    "colSpan" => "colspan",
    "rowSpan" => "rowspan",
    "srcSet" => "srcset",
    "noValidate" => "novalidate",
    "acceptCharset" => "accept-charset",
    "httpEquiv" => "http-equiv",
    "crossOrigin" => "crossorigin",
    "spellCheck" => "spellcheck",
    "encType" => "enctype",
    "formAction" => "formaction",
    other => other,
  }
}

/// A child expression the way React prints one: strings and numbers as text,
/// `null` and booleans as nothing, an array as its items in turn.
fn interpolate(value: &Value, out: &mut Out) -> Result<(), Fail> {
  match value {
    Value::Null | Value::Bool(_) => Ok(()),
    Value::Seq(items) => {
      for item in items {
        interpolate(item, out)?;
      }
      Ok(())
    }
    other => {
      out.text(&stringify(other)?);
      Ok(())
    }
  }
}

/// An attribute the way React's server renderer prints one: `null`,
/// `undefined` and `false` omit it, `true` on a boolean attribute writes
/// `name=""`, a `style` with nothing in it is omitted, anything else is
/// stringified.
fn attribute(name: &str, value: &Value, out: &mut String) -> Result<(), Fail> {
  if matches!(value, Value::Null | Value::Bool(false)) {
    return Ok(());
  }
  if name == "style" {
    let css = match value {
      Value::Map(map) => style_text(map)?,
      other => stringify(other)?,
    };
    if css.is_empty() {
      return Ok(());
    }
    out.push_str(" style=\"");
    escape_attr(&css, out);
    out.push('"');
    return Ok(());
  }
  if BOOLEAN.contains(&name) {
    if truthy(value) {
      out.push(' ');
      out.push_str(name);
      out.push_str("=\"\"");
    }
    return Ok(());
  }
  out.push(' ');
  out.push_str(name);
  out.push_str("=\"");
  escape_attr(&stringify(value)?, out);
  out.push('"');
  Ok(())
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::ast::{Builtin, Expr, Lit, Stmt};
  fn block_on<F: std::future::Future>(f: F) -> F::Output {
    tokio::runtime::Builder::new_current_thread().build().unwrap().block_on(f)
  }

  fn props(entries: &[(&str, Value)]) -> ValueMap {
    entries.iter().map(|(k, v)| ((*k).to_owned(), v.clone())).collect()
  }

  fn p(name: &str) -> Expr {
    Expr::var("$props").field(name)
  }

  #[test]
  fn elements_text_and_interpolation_print_like_react() {
    let component = Component {
      body: vec![Stmt::Let { name: "n".to_owned(), expr: Expr::Length(Box::new(p("items"))) }],
      render: Tmpl::Element {
        tag: "p".to_owned(),
        attrs: vec![Entry::Field("class".to_owned(), Expr::lit_str("count")), Entry::Field("hidden".to_owned(), Expr::Lit(Lit::Bool(false))), Entry::Field("title".to_owned(), Expr::Lit(Lit::Null))],
        children: vec![Tmpl::Expr(Expr::var("n")), Tmpl::Text(" result".to_owned()), Tmpl::Expr(Expr::Ternary(Box::new(Expr::Compare(crate::ast::CompareOp::Eq, Box::new(Expr::var("n")), Box::new(Expr::Lit(Lit::Float(1.0))))), Box::new(Expr::lit_str("")), Box::new(Expr::lit_str("s")))), Tmpl::Text(" <3".to_owned())],
      },
    };
    let html = block_on(Interpreter::default().render(&component, &props(&[("items", Value::Seq(vec![Value::Null, Value::Null]))]), &Components::new())).unwrap();
    assert_eq!(html, "<p class=\"count\">2<!-- --> result<!-- -->s<!-- --> &lt;3</p>");
  }

  #[test]
  fn for_if_and_let_are_the_jsx_idioms() {
    let component = Component {
      body: Vec::new(),
      render: Tmpl::Element {
        tag: "ul".to_owned(),
        attrs: Vec::new(),
        children: vec![Tmpl::For {
          over: p("lines"),
          params: vec!["l".to_owned(), "i".to_owned()],
          body: Box::new(Tmpl::Let {
            name: "qty".to_owned(),
            expr: Expr::Num(Box::new(Expr::var("l").field("quantity"))),
            then: Box::new(Tmpl::Element {
              tag: "li".to_owned(),
              attrs: vec![Entry::Field("data-i".to_owned(), Expr::var("i"))],
              children: vec![Tmpl::Expr(Expr::var("qty")), Tmpl::If { cond: Expr::Compare(crate::ast::CompareOp::Gt, Box::new(Expr::var("qty")), Box::new(Expr::Lit(Lit::Float(1.0)))), then: Box::new(Tmpl::Element { tag: "b".to_owned(), attrs: Vec::new(), children: vec![Tmpl::Text("many".to_owned())] }), r#else: None }],
            }),
          }),
        }],
      },
    };
    let lines = Value::Seq(vec![Value::Map(props(&[("quantity", Value::Int(1))])), Value::Map(props(&[("quantity", Value::Int(3))]))]);
    let html = block_on(Interpreter::default().render(&component, &props(&[("lines", lines)]), &Components::new())).unwrap();
    assert_eq!(html, "<ul><li data-i=\"0\">1</li><li data-i=\"1\">3<b>many</b></li></ul>");
  }

  #[test]
  fn a_component_renders_another_with_its_own_props() {
    let mut library = Components::new();
    library.insert(
      "src/ui/Stars.tsx#Stars".to_owned(),
      Arc::new(Component {
        body: vec![Stmt::Let { name: "full".to_owned(), expr: Expr::Builtin { name: Builtin::Round, args: vec![p("rating")] } }],
        render: Tmpl::Element {
          tag: "span".to_owned(),
          attrs: vec![Entry::Field("title".to_owned(), Expr::Template(vec![Expr::Builtin { name: Builtin::ToFixed, args: vec![p("rating"), Expr::Lit(Lit::Float(1.0))] }, Expr::lit_str(" out of 5")]))],
          children: vec![Tmpl::Expr(Expr::Arith(crate::ast::ArithOp::Add, Box::new(Expr::Builtin { name: Builtin::Repeat, args: vec![Expr::lit_str("★"), Expr::var("full")] }), Box::new(Expr::Builtin { name: Builtin::Repeat, args: vec![Expr::lit_str("☆"), Expr::Arith(crate::ast::ArithOp::Sub, Box::new(Expr::Lit(Lit::Float(5.0))), Box::new(Expr::var("full")))] })))],
        },
      }),
    );
    let page = Component {
      body: Vec::new(),
      render: Tmpl::Fragment(vec![Tmpl::Component { module: "src/ui/Stars.tsx#Stars".to_owned(), props: vec![Entry::Field("rating".to_owned(), p("product").field("rating"))], children: Vec::new() }, Tmpl::Expr(p("product").field("name"))]),
    };
    let product = Value::Map(props(&[("rating", Value::F64(4.5)), ("name", Value::str("Filament"))]));
    let html = block_on(Interpreter::default().render(&page, &props(&[("product", product)]), &library)).unwrap();
    assert_eq!(html, "<span title=\"4.5 out of 5\">★★★★★</span>Filament");
  }

  #[test]
  fn children_render_in_the_callers_scope_and_spreads_merge_in_order() {
    let mut library = Components::new();
    library.insert(
      "src/ui/Page.tsx#Page".to_owned(),
      Arc::new(Component {
        body: Vec::new(),
        render: Tmpl::Element {
          tag: "main".to_owned(),
          attrs: vec![Entry::Field("class".to_owned(), p("className"))],
          children: vec![Tmpl::Element { tag: "h1".to_owned(), attrs: Vec::new(), children: vec![Tmpl::Expr(p("title"))] }, Tmpl::Component { module: "src/ui/Card.tsx#Card".to_owned(), props: Vec::new(), children: vec![Tmpl::Slot] }],
        },
      }),
    );
    library.insert(
      "src/ui/Card.tsx#Card".to_owned(),
      Arc::new(Component { body: Vec::new(), render: Tmpl::Element { tag: "div".to_owned(), attrs: vec![Entry::Field("class".to_owned(), Expr::lit_str("card"))], children: vec![Tmpl::Slot, Tmpl::Slot] } }),
    );
    let page = Component {
      body: vec![Stmt::Let { name: "header".to_owned(), expr: Expr::Object(vec![Entry::Field("title".to_owned(), Expr::lit_str("Picks")), Entry::Field("className".to_owned(), Expr::lit_str("wrong"))]) }],
      render: Tmpl::Component {
        module: "src/ui/Page.tsx#Page".to_owned(),
        props: vec![Entry::Spread(Expr::var("header")), Entry::Field("className".to_owned(), Expr::lit_str("catalog"))],
        children: vec![Tmpl::For {
          over: p("items"),
          params: vec!["it".to_owned()],
          body: Box::new(Tmpl::Element { tag: "p".to_owned(), attrs: vec![Entry::Spread(Expr::var("it").field("attrs")), Entry::Field("class".to_owned(), Expr::lit_str("item"))], children: vec![Tmpl::Expr(Expr::var("it").field("name")), Tmpl::Text(" for ".to_owned()), Tmpl::Expr(p("title"))] }),
        }],
      },
    };
    let mut attrs = ValueMap::new();
    attrs.insert("className".to_owned(), Value::str("ignored"));
    attrs.insert("dataId".to_owned(), Value::Int(7));
    attrs.insert("onClick".to_owned(), Value::str("handler"));
    attrs.insert("hidden".to_owned(), Value::Bool(true));
    let items = Value::Seq(vec![Value::Map(props(&[("name", Value::str("A")), ("attrs", Value::Map(attrs))])), Value::Map(props(&[("name", Value::str("B")), ("attrs", Value::Null)]))]);
    let html = block_on(Interpreter::default().render(&page, &props(&[("items", items), ("title", Value::str("outer"))]), &library)).unwrap();
    assert_eq!(html, "<main class=\"catalog\"><h1>Picks</h1><div class=\"card\"><p class=\"item\" dataId=\"7\" hidden=\"\">A<!-- --> for <!-- -->outer</p><p class=\"item\">B<!-- --> for <!-- -->outer</p><p class=\"item\" dataId=\"7\" hidden=\"\">A<!-- --> for <!-- -->outer</p><p class=\"item\">B<!-- --> for <!-- -->outer</p></div></main>");
  }

  #[test]
  fn void_elements_boolean_attributes_and_escaping() {
    let component = Component {
      body: Vec::new(),
      render: Tmpl::Fragment(vec![
        Tmpl::Element { tag: "input".to_owned(), attrs: vec![Entry::Field("value".to_owned(), Expr::lit_str("a \"b\" & c")), Entry::Field("disabled".to_owned(), Expr::Lit(Lit::Bool(true))), Entry::Field("aria-hidden".to_owned(), Expr::Lit(Lit::Bool(true)))], children: Vec::new() },
        Tmpl::Element { tag: "br".to_owned(), attrs: Vec::new(), children: Vec::new() },
        Tmpl::Expr(Expr::Lit(Lit::Bool(true))),
        Tmpl::Expr(Expr::Lit(Lit::Null)),
      ]),
    };
    let html = block_on(Interpreter::default().render(&component, &ValueMap::new(), &Components::new())).unwrap();
    assert_eq!(html, "<input value=\"a &quot;b&quot; &amp; c\" disabled=\"\" aria-hidden=\"true\"/><br/>");
  }

  #[test]
  fn builtins_follow_javascript() {
    let cases: Vec<(Expr, &str)> = vec![
      (Expr::Builtin { name: Builtin::ToFixed, args: vec![Expr::Lit(Lit::Float(24.0)), Expr::Lit(Lit::Float(2.0))] }, "24.00"),
      (Expr::Builtin { name: Builtin::ToFixed, args: vec![Expr::Lit(Lit::Float(2.345)), Expr::Lit(Lit::Float(2.0))] }, "2.35"),
      (Expr::Builtin { name: Builtin::Round, args: vec![Expr::Lit(Lit::Float(2.5))] }, "3"),
      (Expr::Builtin { name: Builtin::Round, args: vec![Expr::Lit(Lit::Float(-2.5))] }, "-2"),
      (Expr::Builtin { name: Builtin::LocaleNumber, args: vec![Expr::Lit(Lit::Float(1234567.0))] }, "1,234,567"),
      (Expr::Builtin { name: Builtin::LocaleNumber, args: vec![Expr::Lit(Lit::Int(1834))] }, "1,834"),
      (Expr::Builtin { name: Builtin::Join, args: vec![Expr::Array(vec![crate::ast::Entry::Item(Expr::lit_str("A1")), crate::ast::Entry::Item(Expr::lit_str("B2"))]), Expr::lit_str(", ")] }, "A1, B2"),
      (Expr::Builtin { name: Builtin::EncodeUriComponent, args: vec![Expr::lit_str("a b&c/é")] }, "a%20b%26c%2F%C3%A9"),
      (Expr::Builtin { name: Builtin::Min, args: vec![Expr::Lit(Lit::Int(12)), Expr::Lit(Lit::Float(10.0))] }, "10"),
      (Expr::Map(Box::new(Expr::Builtin { name: Builtin::Range, args: vec![Expr::Lit(Lit::Float(3.0))] }), Box::new(Expr::lambda(&["_", "i"], Expr::Arith(crate::ast::ArithOp::Add, Box::new(Expr::var("i")), Box::new(Expr::Lit(Lit::Float(1.0))))))), "1<!-- -->2<!-- -->3"),
    ];
    for (expr, expected) in cases {
      let component = Component { body: Vec::new(), render: Tmpl::Expr(expr.clone()) };
      let html = block_on(Interpreter::default().render(&component, &ValueMap::new(), &Components::new())).unwrap();
      assert_eq!(html, expected, "{expr:?}");
    }
  }
}
