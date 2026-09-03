//! Renders a lowered component to HTML. The tree is the component's own, so
//! the serialiser is a string builder: elements, escaped text and the three
//! idioms. What the browser hydrates over is exactly this output, so it
//! follows React's server renderer where hydration can tell: adjacent text
//! nodes are separated by an empty comment, empty text writes nothing.

use std::collections::HashMap;
use std::sync::Arc;

use futures_util::future::BoxFuture;
use snapfire_fsr_core::{Value, ValueMap};

use crate::ast::{Component, Stmt, Tmpl};
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

impl Interpreter {
  /// Renders `component` with `props` bound as `$props`.
  pub async fn render(&self, component: &Component, props: &ValueMap, library: &Components) -> Result<String, Fail> {
    let mut env = Env::detached(self.clock(), vec![("$props".to_owned(), Value::Map(props.clone()))]);
    let mut out = Out::default();
    render_component(&mut env, component, library, &mut out).await?;
    Ok(out.html)
  }
}

fn render_component<'a>(env: &'a mut Env, component: &'a Component, library: &'a Components, out: &'a mut Out) -> BoxFuture<'a, Result<(), Fail>> {
  Box::pin(async move {
    let depth = env.scope.len();
    for stmt in &component.body {
      let Stmt::Let { name, expr } = stmt else {
        return Err(Fail::internal("a component body is `let`s only"));
      };
      let value = env.eval(expr).await?;
      env.scope.push((name.clone(), value));
    }
    render(env, &component.render, library, out).await?;
    env.scope.truncate(depth);
    Ok(())
  })
}

fn render<'a>(env: &'a mut Env, tmpl: &'a Tmpl, library: &'a Components, out: &'a mut Out) -> BoxFuture<'a, Result<(), Fail>> {
  Box::pin(async move {
    match tmpl {
      Tmpl::Text(text) => out.text(text),
      Tmpl::Expr(expr) => {
        let value = env.eval(expr).await?;
        interpolate(&value, out)?;
      }
      Tmpl::Element { tag, attrs, children } => {
        let mut open = format!("<{tag}");
        for (name, expr) in attrs {
          let value = env.eval(expr).await?;
          attribute(name, &value, &mut open)?;
        }
        open.push('>');
        out.markup(&open);
        if VOID.contains(&tag.as_str()) {
          return Ok(());
        }
        for child in children {
          render(env, child, library, out).await?;
        }
        out.markup(&format!("</{tag}>"));
      }
      Tmpl::Fragment(children) => {
        for child in children {
          render(env, child, library, out).await?;
        }
      }
      Tmpl::If { cond, then, r#else } => {
        let value = env.eval(cond).await?;
        if truthy(&value) {
          render(env, then, library, out).await?;
        } else if let Some(other) = r#else {
          render(env, other, library, out).await?;
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
          render(env, body, library, out).await?;
        }
        env.scope.truncate(depth);
      }
      Tmpl::Let { name, expr, then } => {
        let value = env.eval(expr).await?;
        let depth = env.scope.len();
        env.scope.push((name.clone(), value));
        render(env, then, library, out).await?;
        env.scope.truncate(depth);
      }
      Tmpl::Component { module, props } => {
        let component = library.get(module).cloned().ok_or_else(|| Fail::internal(format!("`{module}` is not a lowered component")))?;
        let mut map = ValueMap::new();
        for (name, expr) in props {
          let value = env.eval(expr).await?;
          map.insert(name.clone(), value);
        }
        let depth = env.scope.len();
        let outer = std::mem::replace(&mut env.scope, vec![("$props".to_owned(), Value::Map(map))]);
        let result = render_component(env, &component, library, out).await;
        env.scope = outer;
        env.scope.truncate(depth);
        result?;
      }
    }
    Ok(())
  })
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

/// An attribute the way React prints one: `null`, `undefined` and `false`
/// omit it, `true` on a boolean attribute writes the bare name, anything else
/// is stringified.
fn attribute(name: &str, value: &Value, out: &mut String) -> Result<(), Fail> {
  if matches!(value, Value::Null | Value::Bool(false)) {
    return Ok(());
  }
  if BOOLEAN.contains(&name) {
    if truthy(value) {
      out.push(' ');
      out.push_str(name);
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
        attrs: vec![("class".to_owned(), Expr::lit_str("count")), ("hidden".to_owned(), Expr::Lit(Lit::Bool(false))), ("title".to_owned(), Expr::Lit(Lit::Null))],
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
              attrs: vec![("data-i".to_owned(), Expr::var("i"))],
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
          attrs: vec![("title".to_owned(), Expr::Template(vec![Expr::Builtin { name: Builtin::ToFixed, args: vec![p("rating"), Expr::Lit(Lit::Float(1.0))] }, Expr::lit_str(" out of 5")]))],
          children: vec![Tmpl::Expr(Expr::Arith(crate::ast::ArithOp::Add, Box::new(Expr::Builtin { name: Builtin::Repeat, args: vec![Expr::lit_str("★"), Expr::var("full")] }), Box::new(Expr::Builtin { name: Builtin::Repeat, args: vec![Expr::lit_str("☆"), Expr::Arith(crate::ast::ArithOp::Sub, Box::new(Expr::Lit(Lit::Float(5.0))), Box::new(Expr::var("full")))] })))],
        },
      }),
    );
    let page = Component {
      body: Vec::new(),
      render: Tmpl::Fragment(vec![Tmpl::Component { module: "src/ui/Stars.tsx#Stars".to_owned(), props: vec![("rating".to_owned(), p("product").field("rating"))] }, Tmpl::Expr(p("product").field("name"))]),
    };
    let product = Value::Map(props(&[("rating", Value::F64(4.5)), ("name", Value::str("Filament"))]));
    let html = block_on(Interpreter::default().render(&page, &props(&[("product", product)]), &library)).unwrap();
    assert_eq!(html, "<span title=\"4.5 out of 5\">★★★★★</span>Filament");
  }

  #[test]
  fn void_elements_boolean_attributes_and_escaping() {
    let component = Component {
      body: Vec::new(),
      render: Tmpl::Fragment(vec![
        Tmpl::Element { tag: "input".to_owned(), attrs: vec![("value".to_owned(), Expr::lit_str("a \"b\" & c")), ("disabled".to_owned(), Expr::Lit(Lit::Bool(true))), ("aria-hidden".to_owned(), Expr::Lit(Lit::Bool(true)))], children: Vec::new() },
        Tmpl::Element { tag: "br".to_owned(), attrs: Vec::new(), children: Vec::new() },
        Tmpl::Expr(Expr::Lit(Lit::Bool(true))),
        Tmpl::Expr(Expr::Lit(Lit::Null)),
      ]),
    };
    let html = block_on(Interpreter::default().render(&component, &ValueMap::new(), &Components::new())).unwrap();
    assert_eq!(html, "<input value=\"a &quot;b&quot; &amp; c\" disabled aria-hidden=\"true\"><br>");
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
