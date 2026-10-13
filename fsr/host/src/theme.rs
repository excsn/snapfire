//! The document's theme for one request: the route's own from the `theme` its layouts and page
//! export, then the shell's `theme.ts` and the mounted site's, each seeing the last one's choice
//! as `ctx.theme`. The result is written onto `<html>` of every document, prerendered or not, and
//! into the payload's eager head row.

use super::*;
use snapfire_fsr_runtime::DocumentTheme;

/// The theme a route sets: down the `content` chain from the document to the page, the deepest
/// level that names one, unless a level above it forces its own. Of two forced levels the outer
/// one wins.
pub(crate) fn static_theme(plan: &PlanNode, themes: &HashMap<String, (String, bool)>) -> (Option<String>, bool) {
  let mut chosen: (Option<String>, bool) = (None, false);
  let mut node = plan;
  loop {
    if !chosen.1
      && let Some((name, force)) = themes.get(&node.module.to_string())
    {
      chosen = (Some(name.clone()), *force);
    }
    match node.children.iter().find(|(slot, _)| slot.0 == "content") {
      Some((_, child)) => node = child,
      None => return chosen,
    }
  }
}

/// `<html …>` with `data-sf-theme` and `data-sf-scheme` set to `theme`, an attribute already
/// there replaced and one the theme leaves out removed. Text without an `<html` tag comes back as
/// it is.
pub(crate) fn html_with_theme(text: &str, theme: &DocumentTheme) -> String {
  let Some(start) = text.find("<html") else { return text.to_owned() };
  let Some(len) = text[start..].find('>') else { return text.to_owned() };
  let end = start + len;
  let mut tag = text[start..end].to_owned();
  for name in ["data-sf-theme", "data-sf-scheme"] {
    tag = without_attr(&tag, name);
  }
  if let Some(name) = &theme.name {
    tag.push_str(&format!(" data-sf-theme=\"{}\"", shell::escape(name)));
  }
  if let Some(scheme) = &theme.scheme {
    tag.push_str(&format!(" data-sf-scheme=\"{}\"", shell::escape(scheme)));
  }
  format!("{}{tag}{}", &text[..start], &text[end..])
}

fn without_attr(tag: &str, name: &str) -> String {
  let needle = format!(" {name}=\"");
  let Some(at) = tag.find(&needle) else { return tag.to_owned() };
  let value = at + needle.len();
  match tag[value..].find('"') {
    Some(close) => format!("{}{}", &tag[..at], &tag[value + close + 1..]),
    None => tag.to_owned(),
  }
}

/// A prerendered payload with its eager `H` row carrying `theme`, the row added before the
/// segments row when the file has none.
pub(crate) fn payload_with_theme(text: &str, theme: &DocumentTheme) -> String {
  let mut out = String::with_capacity(text.len() + 48);
  let mut done = false;
  for line in text.split_inclusive('\n') {
    if !done && let Some(json) = line.strip_prefix("H ") {
      if let Ok(serde_json::Value::Object(mut fields)) = serde_json::from_str::<serde_json::Value>(json.trim_end()) {
        fields.insert("theme".to_owned(), serde_json::json!(theme.name));
        fields.insert("scheme".to_owned(), serde_json::json!(theme.scheme));
        out.push_str(&format!("H {}\n", serde_json::Value::Object(fields)));
        done = true;
        continue;
      }
    }
    if !done && line.starts_with("G ") {
      out.push_str(&format!("H {}\n", serde_json::json!({ "theme": theme.name, "scheme": theme.scheme })));
      done = true;
    }
    out.push_str(line);
  }
  out
}

/// What a `theme.ts` body answered: `name` and `scheme`, each a string or null.
fn choice_of(value: &Value) -> Result<(Option<String>, Option<String>), String> {
  let text = |map: &ValueMap, key: &str| match map.get(key) {
    None | Some(Value::Null) => Ok(None),
    Some(Value::Str(s)) => Ok(Some(s.to_string())),
    Some(other) => Err(format!("`theme` answered `{key}` as {}; expected a string or null", actions::kind_of(other))),
  };
  match value {
    Value::Null => Ok((None, None)),
    Value::Map(map) => Ok((text(map, "name")?, text(map, "scheme")?)),
    other => Err(format!("`theme` answered {}; expected an object", actions::kind_of(other))),
  }
}

impl Tables {
  /// Whether anything in these tables decides a theme, so a request without themes pays nothing.
  pub(crate) fn themed(&self) -> bool {
    self.theme.is_some() || !self.themes.is_empty() || self.sites.iter().any(|site| site.theme.is_some())
  }
}

impl Host {
  /// The document's theme for a request to `path`. A body that fails is logged and leaves the
  /// choice where the levels before it put it.
  pub(crate) async fn document_theme(&self, t: &Tables, path: &str, raw_query: &str, incoming: &Incoming, locale: &Locale) -> DocumentTheme {
    let (mut name, forced) = match self.plan_for(t, path) {
      Some((plan, _)) => static_theme(&plan, &t.themes),
      None => (None, false),
    };
    let mut scheme = None;
    let bodies = [t.theme.as_ref(), t.site_for(path).and_then(|site| site.theme.as_ref())];
    for body in bodies.into_iter().flatten() {
      let ctx = self.ctx(t, incoming.clone(), Params::new(), parse_query(raw_query), path, locale.clone());
      let mut input = ValueMap::default();
      input.insert("name".to_owned(), name.clone().map(Value::str).unwrap_or(Value::Null));
      input.insert("forced".to_owned(), Value::Bool(forced));
      match body.call(ctx, Value::Map(input)).await.map_err(|e| e.message).and_then(|value| choice_of(&value)) {
        Ok((chosen, chosen_scheme)) => {
          name = chosen;
          scheme = chosen_scheme;
        }
        Err(why) => tracing::warn!(target: "fsr::theme", %path, why, "theme.ts failed; the document keeps the theme so far"),
      }
    }
    DocumentTheme { name, scheme }
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use snapfire_fsr_core::{NodeId, SlotName};

  fn theme(name: Option<&str>, scheme: Option<&str>) -> DocumentTheme {
    DocumentTheme { name: name.map(str::to_owned), scheme: scheme.map(str::to_owned) }
  }

  #[test]
  fn the_deepest_theme_wins_unless_one_above_it_forces() {
    let page = PlanNode::new(NodeId(2), "routes/sale/page.tsx#default".parse().unwrap());
    let mut layout = PlanNode::new(NodeId(1), "routes/layout.tsx#default".parse().unwrap());
    layout.children.push((SlotName("content".into()), page));
    let mut document = PlanNode::new(NodeId(0), "shell#document".parse().unwrap());
    document.children.push((SlotName("content".into()), layout));
    let rows = |rows: &[(&str, &str, bool)]| rows.iter().map(|(m, n, f)| (m.to_string(), (n.to_string(), *f))).collect::<HashMap<_, _>>();
    assert_eq!(static_theme(&document, &rows(&[])), (None, false));
    assert_eq!(static_theme(&document, &rows(&[("routes/layout.tsx#default", "acme", false), ("routes/sale/page.tsx#default", "sale", false)])), (Some("sale".to_owned()), false));
    assert_eq!(static_theme(&document, &rows(&[("routes/layout.tsx#default", "acme", true), ("routes/sale/page.tsx#default", "sale", false)])), (Some("acme".to_owned()), true));
    assert_eq!(static_theme(&document, &rows(&[("routes/layout.tsx#default", "acme", true), ("routes/sale/page.tsx#default", "sale", true)])), (Some("acme".to_owned()), true));
  }

  #[test]
  fn the_html_tag_takes_the_theme_and_drops_what_it_replaces() {
    let html = "<!doctype html><html lang=\"en\" data-sf-locale=\"en\"><head>";
    assert_eq!(html_with_theme(html, &theme(Some("ocean"), Some("dark"))), "<!doctype html><html lang=\"en\" data-sf-locale=\"en\" data-sf-theme=\"ocean\" data-sf-scheme=\"dark\"><head>");
    let themed = html_with_theme(html, &theme(Some("ocean"), Some("dark")));
    assert_eq!(html_with_theme(&themed, &theme(Some("forest"), None)), "<!doctype html><html lang=\"en\" data-sf-locale=\"en\" data-sf-theme=\"forest\"><head>");
    assert_eq!(html_with_theme(&themed, &theme(None, None)), html);
    assert_eq!(html_with_theme("<p>no document</p>", &theme(Some("x"), None)), "<p>no document</p>");
  }

  #[test]
  fn a_prerendered_payload_carries_the_theme_in_its_eager_head_row() {
    let with_head = "V {\"fmt\":2}\nN {}\nH {\"title\":\"Posts\"}\nG {}\n";
    assert_eq!(payload_with_theme(with_head, &theme(Some("editorial"), None)), "V {\"fmt\":2}\nN {}\nH {\"title\":\"Posts\",\"theme\":\"editorial\",\"scheme\":null}\nG {}\n");
    let without = "V {\"fmt\":2}\nN {}\nG {}\nS 1 {}\n";
    assert_eq!(payload_with_theme(without, &theme(None, Some("dark"))), "V {\"fmt\":2}\nN {}\nH {\"theme\":null,\"scheme\":\"dark\"}\nG {}\nS 1 {}\n");
  }
}
