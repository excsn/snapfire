//! Heading ids the way `github-slugger`, which `rehype-slug` uses, makes them.

use std::collections::HashMap;

#[derive(Default)]
pub(crate) struct Slugger {
  seen: HashMap<String, usize>,
}

impl Slugger {
  /// The id for `text`, `-1`, `-2` and so on appended when an earlier heading took it.
  pub(crate) fn slug(&mut self, text: &str) -> String {
    let base = slug(text);
    let mut candidate = base.clone();
    while self.seen.contains_key(&candidate) {
      let count = self.seen.get_mut(&base).expect("the base was seen first");
      *count += 1;
      candidate = format!("{base}-{count}");
    }
    self.seen.insert(candidate.clone(), 0);
    candidate
  }
}

fn slug(text: &str) -> String {
  text
    .to_lowercase()
    .chars()
    .filter_map(|c| match c {
      ' ' => Some('-'),
      '-' | '_' => Some(c),
      c if c.is_alphanumeric() => Some(c),
      _ => None,
    })
    .collect()
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn a_heading_slugs_like_github() {
    let mut slugger = Slugger::default();
    assert_eq!(slugger.slug("Hello, World!"), "hello-world");
    assert_eq!(slugger.slug("Hello World"), "hello-world-1");
    assert_eq!(slugger.slug("Hello World"), "hello-world-2");
    assert_eq!(slugger.slug("Über `code` 2.0"), "über-code-20");
    assert_eq!(slugger.slug("snake_case and-dash"), "snake_case-and-dash");
  }
}
