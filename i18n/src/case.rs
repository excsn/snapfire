//! Case mapping as `toLocaleUpperCase` and `toLocaleLowerCase` write it. Unicode's default mapping applies, with two kinds of exception. Turkish, Azerbaijani and Lithuanian follow the language rules of `SpecialCasing.txt`. Greek uppercase follows ICU, which drops the accents.
//!
//! `data/ucd.txt` is written by `datagen --ucd` from a Unicode Character Database release. One record per line, fields separated by tabs and code points in hex, tagged by the first field:
//!
//! - `V version`: the Unicode release.
//! - `A first last`: combining class 230, the marks above.
//! - `M first last`: any other combining class but 0.
//! - `C first last`: `Cased`.
//! - `I first last`: `Case_Ignorable`.
//! - `D first last`: `Soft_Dotted`.
//! - `G letter upper flags`: a Greek letter and its capital without diacritics. The flags are `v` for a vowel and `a`, `d` and `y` for an accent, a dialytika and a ypogegrammeni written into the letter.

use std::collections::HashMap;
use std::sync::OnceLock;

const TEXT: &str = include_str!("../data/ucd.txt");

const ACCENT: u8 = 1;
const DIALYTIKA: u8 = 2;
const COMBINING_DIALYTIKA: u8 = 4;
const YPOGEGRAMMENI: u8 = 8;
const VOWEL: u8 = 16;

#[derive(Default)]
struct Data {
  version: &'static str,
  above: Vec<(u32, u32)>,
  other_marks: Vec<(u32, u32)>,
  cased: Vec<(u32, u32)>,
  ignorable: Vec<(u32, u32)>,
  soft_dotted: Vec<(u32, u32)>,
  greek: HashMap<char, (char, u8)>,
}

fn point(text: &str) -> u32 {
  u32::from_str_radix(text, 16).unwrap_or_else(|_| panic!("snapfire_i18n case data: `{text}` is not a code point"))
}

fn letter(text: &str) -> char {
  char::from_u32(point(text)).unwrap_or_else(|| panic!("snapfire_i18n case data: `{text}` is not a character"))
}

fn data() -> &'static Data {
  static DATA: OnceLock<Data> = OnceLock::new();
  DATA.get_or_init(|| {
    let mut data = Data::default();
    for line in TEXT.lines() {
      let f: Vec<&'static str> = line.split('\t').collect();
      let range = || (point(f[1]), point(f[2]));
      match f[0] {
        "V" => data.version = f[1],
        "A" => data.above.push(range()),
        "M" => data.other_marks.push(range()),
        "C" => data.cased.push(range()),
        "I" => data.ignorable.push(range()),
        "D" => data.soft_dotted.push(range()),
        "G" => {
          let flags = f[3].chars().fold(0, |bits, flag| {
            bits
              | match flag {
                'v' => VOWEL,
                'a' => ACCENT,
                'd' => DIALYTIKA,
                'y' => YPOGEGRAMMENI,
                other => panic!("snapfire_i18n case data: unknown flag `{other}`"),
              }
          });
          data.greek.insert(letter(f[1]), (letter(f[2]), flags));
        }
        other => panic!("snapfire_i18n case data: unknown record `{other}`"),
      }
    }
    data
  })
}

/// The Unicode release the embedded case data was generated from.
pub fn unicode_version() -> &'static str {
  data().version
}

fn within(ranges: &[(u32, u32)], c: char) -> bool {
  let c = c as u32;
  let at = ranges.partition_point(|&(_, last)| last < c);
  ranges.get(at).is_some_and(|&(first, _)| first <= c)
}

#[derive(PartialEq)]
enum Class {
  Base,
  Above,
  Other,
}

fn class(c: char) -> Class {
  let data = data();
  if within(&data.above, c) {
    Class::Above
  } else if within(&data.other_marks, c) {
    Class::Other
  } else {
    Class::Base
  }
}

/// Whether the closest character before `at` of combining class 0 or 230 is one `found` accepts, as `After_I` and `After_Soft_Dotted` read it.
fn after(chars: &[char], at: usize, found: impl Fn(char) -> bool) -> bool {
  for &c in chars[..at].iter().rev() {
    if found(c) {
      return true;
    }
    if class(c) != Class::Other {
      return false;
    }
  }
  false
}

/// Whether a combining dot above follows `at` with no mark of class 0 or 230 between, as `Before_Dot` reads it.
fn before_dot(chars: &[char], at: usize) -> bool {
  for &c in &chars[at + 1..] {
    if c == '\u{307}' {
      return true;
    }
    if class(c) != Class::Other {
      return false;
    }
  }
  false
}

/// Whether a mark of class 230 follows `at` with no character of class 0 between, as `More_Above` reads it.
fn more_above(chars: &[char], at: usize) -> bool {
  for &c in &chars[at + 1..] {
    match class(c) {
      Class::Above => return true,
      Class::Base => return false,
      Class::Other => {}
    }
  }
  false
}

enum Language {
  Root,
  Turkic,
  Lithuanian,
  Greek,
}

fn language(locale: &str) -> Language {
  match crate::locale::subtags(locale).0.as_str() {
    "tr" | "az" => Language::Turkic,
    "lt" => Language::Lithuanian,
    "el" => Language::Greek,
    _ => Language::Root,
  }
}

/// `text.toLocaleUpperCase(locale)`.
pub fn upper_case(text: &str, locale: &str) -> String {
  match language(locale) {
    Language::Root => text.to_uppercase(),
    Language::Turkic => text.replace('i', "\u{130}").to_uppercase(),
    Language::Lithuanian => {
      let chars: Vec<char> = text.chars().collect();
      let soft_dotted = |c| within(&data().soft_dotted, c);
      let kept: String = chars.iter().enumerate().filter(|&(at, &c)| c != '\u{307}' || !after(&chars, at, soft_dotted)).map(|(_, &c)| c).collect();
      kept.to_uppercase()
    }
    Language::Greek => greek_upper(text),
  }
}

/// `text.toLocaleLowerCase(locale)`.
pub fn lower_case(text: &str, locale: &str) -> String {
  let chars: Vec<char> = text.chars().collect();
  let mut out = String::with_capacity(text.len());
  match language(locale) {
    Language::Root | Language::Greek => return text.to_lowercase(),
    Language::Turkic => {
      for (at, &c) in chars.iter().enumerate() {
        match c {
          '\u{130}' => out.push('i'),
          '\u{307}' if after(&chars, at, |p| p == 'I') => {}
          'I' if before_dot(&chars, at) => out.push('i'),
          'I' => out.push('\u{131}'),
          _ => out.push(c),
        }
      }
    }
    Language::Lithuanian => {
      for (at, &c) in chars.iter().enumerate() {
        match c {
          'I' | 'J' | '\u{12e}' if more_above(&chars, at) => {
            out.extend(c.to_lowercase());
            out.push('\u{307}');
          }
          '\u{cc}' => out.push_str("i\u{307}\u{300}"),
          '\u{cd}' => out.push_str("i\u{307}\u{301}"),
          '\u{128}' => out.push_str("i\u{307}\u{303}"),
          _ => out.push(c),
        }
      }
    }
  }
  out.to_lowercase()
}

/// What a combining mark after a Greek letter adds to it, `Some(0)` for a breathing or a length mark, which is dropped and adds nothing.
fn greek_diacritic(c: char) -> Option<u8> {
  match c {
    '\u{300}' | '\u{301}' | '\u{342}' | '\u{302}' | '\u{303}' | '\u{311}' => Some(ACCENT),
    '\u{308}' => Some(COMBINING_DIALYTIKA),
    '\u{344}' => Some(COMBINING_DIALYTIKA | ACCENT),
    '\u{345}' => Some(YPOGEGRAMMENI),
    '\u{304}' | '\u{306}' | '\u{313}' | '\u{314}' | '\u{343}' => Some(0),
    _ => None,
  }
}

#[derive(Clone, Copy)]
enum Accented {
  Precomposed,
  Combining,
}

/// ICU's Greek uppercasing (`GreekUpper::toUpper` in ICU4C's `ustrcase.cpp`): each letter loses its accents and breathings. A dialytika is added to an iota or upsilon after an accented vowel, so the two vowels still read apart. A lone accented eta keeps its tonos, as the disjunctive "or".
fn greek_upper(text: &str) -> String {
  let data = data();
  let chars: Vec<char> = text.chars().collect();
  let followed_by_cased = |from: usize| chars[from..].iter().find(|&&c| !within(&data.ignorable, c)).is_some_and(|&c| within(&data.cased, c));
  let mut out = String::with_capacity(text.len());
  let (mut after_cased, mut after_accent) = (false, None);
  let mut at = 0;
  while at < chars.len() {
    let c = chars[at];
    let mut next = at + 1;
    let next_after_cased = if within(&data.ignorable, c) { after_cased } else { within(&data.cased, c) };
    let mut next_accent = None;
    if let Some(&(mut upper, flags)) = data.greek.get(&c) {
      let mut flags = flags;
      let precomposed_accent = flags & ACCENT != 0;
      if flags & VOWEL != 0 && matches!(upper, '\u{399}' | '\u{3a5}') {
        match after_accent {
          Some(Accented::Precomposed) => flags |= DIALYTIKA,
          Some(Accented::Combining) => flags |= COMBINING_DIALYTIKA,
          None => {}
        }
      }
      let mut ypogegrammeni = usize::from(flags & YPOGEGRAMMENI != 0);
      while let Some(mark) = chars.get(next).and_then(|&m| greek_diacritic(m)) {
        flags |= mark;
        ypogegrammeni += usize::from(mark & YPOGEGRAMMENI != 0);
        next += 1;
      }
      if flags & (VOWEL | ACCENT | DIALYTIKA) == VOWEL | ACCENT {
        next_accent = Some(if precomposed_accent { Accented::Precomposed } else { Accented::Combining });
      }
      let mut tonos = false;
      if upper == '\u{397}' && flags & ACCENT != 0 && ypogegrammeni == 0 && !after_cased && !followed_by_cased(next) {
        if precomposed_accent {
          upper = '\u{389}';
        } else {
          tonos = true;
        }
      } else if flags & DIALYTIKA != 0 && matches!(upper, '\u{399}' | '\u{3a5}') {
        upper = if upper == '\u{399}' { '\u{3aa}' } else { '\u{3ab}' };
        flags &= !(DIALYTIKA | COMBINING_DIALYTIKA);
      }
      out.push(upper);
      if flags & (DIALYTIKA | COMBINING_DIALYTIKA) != 0 {
        out.push('\u{308}');
      }
      if tonos {
        out.push('\u{301}');
      }
      out.extend(std::iter::repeat_n('\u{399}', ypogegrammeni));
    } else {
      out.extend(c.to_uppercase());
    }
    after_cased = next_after_cased;
    after_accent = next_accent;
    at = next;
  }
  out
}
