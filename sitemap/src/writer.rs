/// Escapes the five entities the sitemap protocol requires in element text.
pub(crate) fn escape(value: &str) -> String {
  let mut out = String::with_capacity(value.len());

  for ch in value.chars() {
    match ch {
      '&' => out.push_str("&amp;"),
      '<' => out.push_str("&lt;"),
      '>' => out.push_str("&gt;"),
      '"' => out.push_str("&quot;"),
      '\'' => out.push_str("&apos;"),
      _ => out.push(ch),
    }
  }

  out
}

pub(crate) fn indent(out: &mut String, depth: usize) {
  for _ in 0..depth {
    out.push_str("  ");
  }
}

pub(crate) fn tag(out: &mut String, depth: usize, name: &str, text: &str) {
  indent(out, depth);
  out.push('<');
  out.push_str(name);
  out.push('>');
  out.push_str(&escape(text));
  out.push_str("</");
  out.push_str(name);
  out.push_str(">\n");
}

pub(crate) fn tag_with(out: &mut String, depth: usize, name: &str, attributes: &[(&str, &str)], text: &str) {
  indent(out, depth);
  out.push('<');
  out.push_str(name);
  push_attributes(out, attributes);
  out.push('>');
  out.push_str(&escape(text));
  out.push_str("</");
  out.push_str(name);
  out.push_str(">\n");
}

pub(crate) fn empty_tag(out: &mut String, depth: usize, name: &str, attributes: &[(&str, &str)]) {
  indent(out, depth);
  out.push('<');
  out.push_str(name);
  push_attributes(out, attributes);
  out.push_str(" />\n");
}

pub(crate) fn open(out: &mut String, depth: usize, name: &str) {
  indent(out, depth);
  out.push('<');
  out.push_str(name);
  out.push_str(">\n");
}

pub(crate) fn close(out: &mut String, depth: usize, name: &str) {
  indent(out, depth);
  out.push_str("</");
  out.push_str(name);
  out.push_str(">\n");
}

pub(crate) fn yes_no(value: bool) -> &'static str {
  if value { "yes" } else { "no" }
}

fn push_attributes(out: &mut String, attributes: &[(&str, &str)]) {
  for (name, value) in attributes {
    out.push(' ');
    out.push_str(name);
    out.push_str("=\"");
    out.push_str(&escape(value));
    out.push('"');
  }
}
