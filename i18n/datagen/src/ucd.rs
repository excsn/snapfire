//! `--ucd <ucd-dir> <out>`: reads `UnicodeData.txt`, `DerivedCoreProperties.txt`, `PropList.txt` and `Scripts.txt` of a Unicode Character Database release and writes the case data `snapfire_i18n` embeds. The line format is described in the crate's `src/case.rs`.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;
use std::path::Path;

struct Char {
  category: String,
  class: u8,
  decomposition: Vec<u32>,
  upper: Option<u32>,
}

fn hex(text: &str) -> Result<u32, String> {
  u32::from_str_radix(text.trim(), 16).map_err(|_| format!("`{text}` is not a code point"))
}

fn read(dir: &Path, name: &str) -> Result<String, String> {
  let path = dir.join(name);
  std::fs::read_to_string(&path).map_err(|e| format!("read {}: {e}", path.display()))
}

/// `UnicodeData.txt` by code point, with each `First>`/`Last>` range filled in.
fn characters(text: &str) -> Result<BTreeMap<u32, Char>, String> {
  let mut out = BTreeMap::new();
  let mut first: Option<u32> = None;
  for line in text.lines().filter(|l| !l.is_empty()) {
    let f: Vec<&str> = line.split(';').collect();
    if f.len() < 15 {
      return Err(format!("UnicodeData.txt: `{line}` has {} fields", f.len()));
    }
    let cp = hex(f[0])?;
    let class = f[3].parse().map_err(|_| format!("UnicodeData.txt: `{}` is not a combining class", f[3]))?;
    let decomposition = if f[5].is_empty() || f[5].starts_with('<') { Vec::new() } else { f[5].split(' ').map(hex).collect::<Result<_, _>>()? };
    let upper = if f[12].is_empty() { None } else { Some(hex(f[12])?) };
    let make = || Char { category: f[2].to_owned(), class, decomposition: decomposition.clone(), upper };
    if f[1].ends_with(", First>") {
      first = Some(cp);
    } else if f[1].ends_with(", Last>") {
      let start = first.take().ok_or_else(|| format!("UnicodeData.txt: `{line}` closes no range"))?;
      for c in start..=cp {
        out.insert(c, make());
      }
    } else {
      out.insert(cp, make());
    }
  }
  Ok(out)
}

/// The code points a property file gives `property`.
fn property(text: &str, property: &str) -> Result<BTreeSet<u32>, String> {
  let mut out = BTreeSet::new();
  for line in text.lines() {
    let data = line.split('#').next().unwrap_or_default();
    let Some((range, name)) = data.split_once(';') else { continue };
    if name.trim() != property {
      continue;
    }
    let (start, end) = match range.trim().split_once("..") {
      Some((a, b)) => (hex(a)?, hex(b)?),
      None => (hex(range)?, hex(range)?),
    };
    out.extend(start..=end);
  }
  if out.is_empty() {
    return Err(format!("no code point has `{property}`"));
  }
  Ok(out)
}

fn version(text: &str) -> Result<&str, String> {
  let first = text.lines().next().unwrap_or_default();
  first.strip_prefix("# DerivedCoreProperties-").and_then(|v| v.strip_suffix(".txt")).ok_or_else(|| format!("DerivedCoreProperties.txt: `{first}` names no version"))
}

fn ranges(out: &mut String, tag: char, set: &BTreeSet<u32>) {
  let mut iter = set.iter().copied().peekable();
  while let Some(start) = iter.next() {
    let mut end = start;
    while iter.peek() == Some(&(end + 1)) {
      end = iter.next().unwrap_or(end);
    }
    writeln!(out, "{tag}\t{start:X}\t{end:X}").unwrap();
  }
}

fn decomposed(chars: &BTreeMap<u32, Char>, cp: u32, out: &mut Vec<u32>) {
  match chars.get(&cp) {
    Some(c) if !c.decomposition.is_empty() => {
      for &part in &c.decomposition {
        decomposed(chars, part, out);
      }
    }
    _ => out.push(cp),
  }
}

const VOWELS: [u32; 8] = [0x391, 0x395, 0x397, 0x399, 0x39F, 0x3A5, 0x3A9, 0x3D2];

pub fn generate(dir: &Path) -> Result<String, String> {
  let chars = characters(&read(dir, "UnicodeData.txt")?)?;
  let derived = read(dir, "DerivedCoreProperties.txt")?;
  let props = read(dir, "PropList.txt")?;
  let greek = property(&read(dir, "Scripts.txt")?, "Greek")?;
  let mut out = String::new();
  writeln!(out, "V\t{}", version(&derived)?).unwrap();
  let above: BTreeSet<u32> = chars.iter().filter(|(_, c)| c.class == 230).map(|(&cp, _)| cp).collect();
  let other: BTreeSet<u32> = chars.iter().filter(|(_, c)| c.class != 0 && c.class != 230).map(|(&cp, _)| cp).collect();
  ranges(&mut out, 'A', &above);
  ranges(&mut out, 'M', &other);
  ranges(&mut out, 'C', &property(&derived, "Cased")?);
  ranges(&mut out, 'I', &property(&derived, "Case_Ignorable")?);
  ranges(&mut out, 'D', &property(&props, "Soft_Dotted")?);
  for (&cp, c) in &chars {
    let in_blocks = (0x370..=0x3FF).contains(&cp) || (0x1F00..=0x1FFF).contains(&cp) || cp == 0x2126;
    if !in_blocks || !greek.contains(&cp) || !c.category.starts_with('L') {
      continue;
    }
    let mut parts = Vec::new();
    decomposed(&chars, cp, &mut parts);
    let base = parts[0];
    let upper = chars.get(&base).and_then(|b| b.upper).unwrap_or(base);
    let mut flags = String::new();
    if VOWELS.contains(&upper) {
      flags.push('v');
    }
    if parts[1..].iter().any(|m| matches!(m, 0x300 | 0x301 | 0x342)) {
      flags.push('a');
    }
    if parts[1..].contains(&0x308) {
      flags.push('d');
    }
    if parts[1..].contains(&0x345) {
      flags.push('y');
    }
    writeln!(out, "G\t{cp:X}\t{upper:X}\t{flags}").unwrap();
  }
  Ok(out)
}
