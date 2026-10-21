use std::ops::Range;

/// Replacements over one text, applied together. Two that overlap keep the
/// one added first.
#[derive(Default)]
pub(crate) struct Edits {
  edits: Vec<(Range<usize>, String)>,
}

impl Edits {
  pub fn replace(&mut self, range: Range<usize>, with: impl Into<String>) {
    self.edits.push((range, with.into()));
  }

  pub fn is_empty(&self) -> bool {
    self.edits.is_empty()
  }

  pub fn apply(mut self, text: &str) -> String {
    let mut kept: Vec<(Range<usize>, String)> = Vec::new();
    for (range, with) in self.edits.drain(..) {
      let overlaps = kept.iter().any(|(r, _)| range.start < r.end && r.start < range.end || range == *r);
      if !overlaps {
        kept.push((range, with));
      }
    }
    kept.sort_by_key(|(r, _)| r.start);
    let mut out = String::with_capacity(text.len());
    let mut at = 0;
    for (range, with) in kept {
      out.push_str(&text[at..range.start]);
      out.push_str(&with);
      at = range.end;
    }
    out.push_str(&text[at..]);
    out
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn replacements_apply_in_position_order_and_an_overlap_keeps_the_first() {
    let mut edits = Edits::default();
    edits.replace(6..11, "there");
    edits.replace(0..5, "hi");
    edits.replace(8..9, "X");
    assert_eq!(edits.apply("hello world"), "hi there");
  }

  #[test]
  fn an_insertion_is_an_empty_range() {
    let mut edits = Edits::default();
    edits.replace(5..5, ",");
    assert_eq!(edits.apply("hello world"), "hello, world");
  }
}
