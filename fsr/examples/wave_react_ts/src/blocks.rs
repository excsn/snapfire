//! A blip's body as blocks: one per top-level markdown block, each with an id
//! a reply anchors to and an editor holds, kept while its text changes.

use pulldown_cmark::{Event, Parser};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Block {
  pub id: String,
  pub text: String,
}

/// `body`'s top-level blocks, each as its source. A block runs from the start
/// of its first line to the start of the next block's, so what renders nothing
/// between two blocks (a link reference definition among it) stays with the
/// one before it.
pub fn split(body: &str) -> Vec<String> {
  let mut starts = Vec::new();
  let mut depth = 0usize;
  for (event, range) in Parser::new(body).into_offset_iter() {
    match event {
      Event::Start(_) => {
        if depth == 0 {
          starts.push(line_start(body, range.start));
        }
        depth += 1;
      }
      Event::End(_) => depth = depth.saturating_sub(1),
      _ if depth == 0 => starts.push(line_start(body, range.start)),
      _ => {}
    }
  }
  if starts.is_empty() {
    let text = body.trim();
    return if text.is_empty() { Vec::new() } else { vec![text.to_owned()] };
  }
  let mut out = Vec::with_capacity(starts.len());
  for (i, start) in starts.iter().enumerate() {
    let from = if i == 0 { 0 } else { *start };
    let to = starts.get(i + 1).copied().unwrap_or(body.len());
    let text = body[from..to].trim_start_matches(['\n', '\r']).trim_end();
    if !text.is_empty() {
      out.push(text.to_owned());
    }
  }
  out
}

fn line_start(body: &str, at: usize) -> usize {
  body[..at].rfind('\n').map(|newline| newline + 1).unwrap_or(0)
}

/// A block with a new id. `made` counts the ids a blip has handed out, so an
/// id is never given to a second block.
pub fn fresh(made: &mut u64, text: String) -> Block {
  *made += 1;
  Block { id: format!("b{made}"), text }
}

/// `texts` as blocks replacing `old`. A block whose text is unchanged keeps its
/// id wherever it moved to. Between two of those, the changed blocks pair up in
/// order, so a block that was edited keeps its id as well. A text left over is
/// a new block and a block left over is gone.
pub fn assign(old: &[Block], texts: Vec<String>, made: &mut u64) -> Vec<Block> {
  let (n, m) = (old.len(), texts.len());
  let mut common = vec![vec![0usize; m + 1]; n + 1];
  for i in (0..n).rev() {
    for j in (0..m).rev() {
      common[i][j] = if old[i].text == texts[j] { common[i + 1][j + 1] + 1 } else { common[i + 1][j].max(common[i][j + 1]) };
    }
  }
  let mut kept = Vec::new();
  let (mut i, mut j) = (0, 0);
  while i < n && j < m {
    if old[i].text == texts[j] {
      kept.push((i, j));
      i += 1;
      j += 1;
    } else if common[i + 1][j] >= common[i][j + 1] {
      i += 1;
    } else {
      j += 1;
    }
  }
  kept.push((n, m));
  let mut texts: Vec<Option<String>> = texts.into_iter().map(Some).collect();
  let mut out = Vec::with_capacity(m);
  let (mut from_old, mut from_new) = (0, 0);
  for (to_old, to_new) in kept {
    for (k, text) in texts[from_new..to_new].iter_mut().enumerate() {
      let text = text.take().unwrap_or_default();
      out.push(match old.get(from_old + k).filter(|_| from_old + k < to_old) {
        Some(edited) => Block { id: edited.id.clone(), text },
        None => fresh(made, text),
      });
    }
    if to_old < n {
      out.push(Block { id: old[to_old].id.clone(), text: texts[to_new].take().unwrap_or_default() });
    }
    (from_old, from_new) = (to_old + 1, to_new + 1);
  }
  out
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn a_body_splits_into_its_top_level_blocks() {
    assert_eq!(split("one\n\n    code\n\n- a\n- b\n\n> quoted"), ["one", "    code", "- a\n- b", "> quoted"]);
    assert_eq!(split("see [it][r]\n\n[r]: https://example.com\n\nnext"), ["see [it][r]\n\n[r]: https://example.com", "next"], "a definition stays with the block before it");
    assert!(split("  \n\n").is_empty());
  }

  #[test]
  fn a_rewrite_keeps_the_ids_of_blocks_it_kept_or_edited() {
    let mut made = 0;
    let old = assign(&[], vec!["a".into(), "b".into(), "c".into()], &mut made);
    let ids = |blocks: &[Block]| blocks.iter().map(|block| block.id.clone()).collect::<Vec<_>>();
    assert_eq!(ids(&old), ["b1", "b2", "b3"]);
    let new = assign(&old, vec!["a, edited".into(), "b".into(), "inserted".into(), "c".into()], &mut made);
    assert_eq!(ids(&new), ["b1", "b2", "b4", "b3"], "an edited block keeps its id and an inserted one takes a new id");
    let fewer = assign(&new, vec!["b".into(), "c".into()], &mut made);
    assert_eq!(ids(&fewer), ["b2", "b3"], "a block left out is gone");
  }
}
