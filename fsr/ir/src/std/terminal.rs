//! `terminal`: the text functions of the npm packages Ink applications import, `ms`, `strip-ansi`, `string-width` and `wrap-ansi`, run natively. Terminal reach: a terminal page calls them where it runs and there is no browser half.

use crate::ext::{number, option, text, Ambient, Extensions, Reach};
use crate::interp::Fail;
use snapfire_fsr_core::Value;

pub fn register(extensions: &mut Extensions) {
  extensions.register("terminal.ms", Reach::Terminal, ms);
  extensions.register("terminal.stripAnsi", Reach::Terminal, |_, args| Ok(Value::str(strip(text("terminal.stripAnsi", args, 0)?))));
  extensions.register("terminal.stringWidth", Reach::Terminal, |_, args| Ok(Value::F64(width(text("terminal.stringWidth", args, 0)?) as f64)));
  extensions.register("terminal.wrapAnsi", Reach::Terminal, wrap_ansi);
  extensions.register("terminal.env", Reach::Process, |_, args| Ok(std::env::var(text("terminal.env", args, 0)?).map_or(Value::Null, Value::str)));
}

/// `ms(x, { long })`: a number of milliseconds as `1m` or `1 minute`, a string such as `2 days` as milliseconds, null for one it cannot read.
fn ms(_: &Ambient, args: &[Value]) -> Result<Value, Fail> {
  let what = "terminal.ms";
  let long = option(what, args, 1, "long")?.is_some_and(crate::interp::truthy);
  match args.first() {
    Some(Value::Str(s)) if !s.is_empty() => Ok(parse_ms(s).map_or(Value::Null, Value::F64)),
    Some(Value::Int(_) | Value::UInt(_) | Value::F64(_) | Value::F32(_)) if number(what, args, 0)?.is_finite() => Ok(Value::str(format_ms(number(what, args, 0)?, long))),
    other => Err(Fail::internal(format!("{what}: val is not a non-empty string or a valid number. val={}", other.map(|v| crate::interp::stringify(v).unwrap_or_default()).unwrap_or_default()))),
  }
}

/// `wrapAnsi(s, columns, { hard, trim, wordWrap })`.
fn wrap_ansi(_: &Ambient, args: &[Value]) -> Result<Value, Fail> {
  let what = "terminal.wrapAnsi";
  let flag = |key: &str, default: bool| -> Result<bool, Fail> { Ok(option(what, args, 2, key)?.map_or(default, crate::interp::truthy)) };
  let columns = number(what, args, 1)?.max(0.0) as usize;
  Ok(Value::str(wrap(text(what, args, 0)?, columns, flag("hard", false)?, flag("trim", true)?, flag("wordWrap", true)?)))
}


use std::sync::OnceLock;

use unicode_normalization::UnicodeNormalization;
use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;

/// `ansi-regex` 6.1's pattern: CSI and OSC sequences, terminated by BEL, `ESC \` or ST.
const ANSI: &str = r"[\u001B\u009B][[\]()#;?]*(?:(?:(?:(?:;[-a-zA-Z\d\/#&.:=?%@~_]+)*|[a-zA-Z\d]+(?:;[-a-zA-Z\d\/#&.:=?%@~_]*)*)?(?:\u0007|\u001B\|\u009C))|(?:(?:\d{1,4}(?:;\d{0,4})*)?[\dA-PR-TZcf-nq-uy=><~]))";

fn ansi() -> &'static regress::Regex {
  static RE: OnceLock<regress::Regex> = OnceLock::new();
  RE.get_or_init(|| regress::Regex::new(ANSI).expect("ansi-regex compiles"))
}

/// `stripAnsi(s)`.
pub fn strip(s: &str) -> String {
  let mut out = String::with_capacity(s.len());
  let mut at = 0;
  for found in ansi().find_iter(s) {
    out.push_str(&s[at..found.start()]);
    at = found.end();
  }
  out.push_str(&s[at..]);
  out
}

/// `stringWidth(s)`: the columns `s` takes once its escape codes are gone, each grapheme cluster as wide as its widest form up to two; a control character or a cluster of marks alone takes none.
pub fn width(s: &str) -> usize {
  let plain = strip(s);
  plain
    .graphemes(true)
    .map(|cluster| match cluster.chars().next() {
      Some(c) if c.is_control() => 0,
      _ => cluster.width().min(2),
    })
    .sum()
}

/// `wrapAnsi(s, columns, { hard, trim, wordWrap })` as `wrap-ansi` 9 lays out the rows. Escape codes stay where they stand; a style is not reopened on the next row as `wrap-ansi` does.
pub fn wrap(s: &str, columns: usize, hard: bool, trim: bool, word_wrap: bool) -> String {
  let s: String = s.nfc().collect();
  s.replace("\r\n", "\n").split('\n').map(|line| wrap_line(line, columns, hard, trim, word_wrap)).collect::<Vec<_>>().join("\n")
}

fn wrap_line(line: &str, columns: usize, hard: bool, trim: bool, word_wrap: bool) -> String {
  if trim && line.trim_matches(is_js_space).is_empty() {
    return String::new();
  }
  let words: Vec<&str> = line.split(' ').collect();
  let lengths: Vec<usize> = words.iter().map(|w| width(w)).collect();
  let mut rows = vec![String::new()];
  for (index, word) in words.iter().enumerate() {
    if trim {
      let last = rows.last_mut().expect("a row");
      *last = last.trim_start_matches(is_js_space).to_owned();
    }
    let mut row_length = width(rows.last().expect("a row"));
    if index != 0 {
      if row_length >= columns && (!word_wrap || !trim) {
        rows.push(String::new());
        row_length = 0;
      }
      if row_length > 0 || !trim {
        rows.last_mut().expect("a row").push(' ');
        row_length += 1;
      }
    }
    let length = lengths[index];
    if hard && length > columns {
      let (length, columns_i, row) = (length as i64, columns.max(1) as i64, row_length as i64);
      let remaining = columns_i - row;
      let starting_here = 1 + (length - remaining - 1).div_euclid(columns_i);
      let starting_next = (length - 1).div_euclid(columns_i);
      if starting_next < starting_here {
        rows.push(String::new());
      }
      wrap_word(&mut rows, word, columns);
      continue;
    }
    if row_length + length > columns && row_length > 0 && length > 0 {
      if !word_wrap && row_length < columns {
        wrap_word(&mut rows, word, columns);
        continue;
      }
      rows.push(String::new());
    }
    if row_length + length > columns && !word_wrap {
      wrap_word(&mut rows, word, columns);
      continue;
    }
    rows.last_mut().expect("a row").push_str(word);
  }
  if trim {
    rows = rows.iter().map(|row| trim_spaces_right(row)).collect();
  }
  rows.join("\n")
}

/// `wrap-ansi`'s `wrapWord`: `word` cut into the rows by code point, an escape sequence taking no column.
fn wrap_word(rows: &mut Vec<String>, word: &str, columns: usize) {
  let characters: Vec<char> = word.chars().collect();
  let mut inside_escape = false;
  let mut inside_link = false;
  let mut visible = width(rows.last().expect("a row"));
  for (index, &character) in characters.iter().enumerate() {
    let length = width(character.encode_utf8(&mut [0; 4]));
    if visible + length <= columns {
      rows.last_mut().expect("a row").push(character);
    } else {
      rows.push(character.to_string());
      visible = 0;
    }
    if character == '\u{1b}' || character == '\u{9b}' {
      inside_escape = true;
      inside_link = characters.get(index + 1..index + 5).is_some_and(|next| next == [']', '8', ';', ';']);
    }
    if inside_escape {
      if inside_link {
        if character == '\u{7}' {
          inside_escape = false;
          inside_link = false;
        }
      } else if character == 'm' {
        inside_escape = false;
      }
      continue;
    }
    visible += length;
    if visible == columns && index + 1 < characters.len() {
      rows.push(String::new());
      visible = 0;
    }
  }
  if visible == 0 && rows.len() > 1 && !rows.last().expect("a row").is_empty() {
    let last = rows.pop().expect("a row");
    rows.last_mut().expect("a row").push_str(&last);
  }
}

/// The row without its trailing spaces, an escape sequence after them kept.
fn trim_spaces_right(row: &str) -> String {
  let words: Vec<&str> = row.split(' ').collect();
  let mut last = words.len();
  while last > 0 && width(words[last - 1]) == 0 {
    last -= 1;
  }
  if last == words.len() {
    return row.to_owned();
  }
  words[..last].join(" ") + &words[last..].concat()
}

/// JavaScript's `trim` whitespace.
fn is_js_space(c: char) -> bool {
  c.is_whitespace() || c == '\u{feff}'
}

/// The `ms` package's `parse`: a number with an optional unit, `2 days`, `-.5h`, `100`.
fn parse_ms(s: &str) -> Option<f64> {
  if s.encode_utf16().count() > 100 {
    return None;
  }
  let lower = s.to_ascii_lowercase();
  let digits = lower.find(|c: char| !(c.is_ascii_digit() || c == '.' || c == '-')).unwrap_or(lower.len());
  let (value, rest) = lower.split_at(digits);
  let number_shape = {
    let unsigned = value.strip_prefix('-').unwrap_or(value);
    let (whole, fraction) = unsigned.split_once('.').map_or((unsigned, None), |(w, f)| (w, Some(f)));
    whole.chars().all(|c| c.is_ascii_digit()) && match fraction {
      Some(f) => !f.is_empty() && f.chars().all(|c| c.is_ascii_digit()),
      None => !whole.is_empty(),
    }
  };
  if !number_shape {
    return None;
  }
  let n: f64 = value.parse().ok()?;
  const S: f64 = 1000.0;
  const M: f64 = S * 60.0;
  const H: f64 = M * 60.0;
  const D: f64 = H * 24.0;
  let unit = match rest.trim_start_matches(' ') {
    "years" | "year" | "yrs" | "yr" | "y" => D * 365.25,
    "weeks" | "week" | "w" => D * 7.0,
    "days" | "day" | "d" => D,
    "hours" | "hour" | "hrs" | "hr" | "h" => H,
    "minutes" | "minute" | "mins" | "min" | "m" => M,
    "seconds" | "second" | "secs" | "sec" | "s" => S,
    "milliseconds" | "millisecond" | "msecs" | "msec" | "ms" | "" => 1.0,
    _ => return None,
  };
  Some(n * unit)
}

/// The `ms` package's `format`: the largest unit the magnitude reaches, rounded as `Math.round` rounds.
fn format_ms(ms: f64, long: bool) -> String {
  const UNITS: [(f64, &str, &str); 4] = [(86_400_000.0, "d", "day"), (3_600_000.0, "h", "hour"), (60_000.0, "m", "minute"), (1000.0, "s", "second")];
  let round = |x: f64| (x + 0.5).floor();
  for (size, short, name) in UNITS {
    if ms.abs() >= size {
      let n = crate::interp::number_text(round(ms / size));
      return match long {
        true => format!("{n} {name}{}", if ms.abs() >= size * 1.5 { "s" } else { "" }),
        false => format!("{n}{short}"),
      };
    }
  }
  match long {
    true => format!("{} ms", crate::interp::number_text(ms)),
    false => format!("{}ms", crate::interp::number_text(ms)),
  }
}

