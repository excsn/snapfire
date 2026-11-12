//! `snapfire_i18n_datagen <cldr-json> <out>`: reads the `cldr-core`, `cldr-numbers-full` and `cldr-dates-full` packages of a cldr-json release and writes the data file `snapfire_i18n` embeds. The line format is described in the crate's `src/data.rs`. `snapfire_i18n_datagen --ucd <ucd-dir> <out>` writes its case data instead, from a Unicode Character Database release. `snapfire_i18n_datagen --zones <cldr-json> <zone.tab> <out>` writes its time zone names and each region's calendars from cldr-json and each region's zones from a tz database `zone.tab`. `--names` writes its display names.

mod names;
mod ucd;
mod zones;

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use serde_json::Value;

const UNITS: [&str; 8] = ["second", "minute", "hour", "day", "week", "month", "quarter", "year"];
const WIDTHS: [&str; 3] = ["", "-short", "-narrow"];
const PLURALS: [(&str, &str); 8] = [("zero", "zero"), ("one", "one"), ("two", "two"), ("few", "few"), ("many", "many"), ("other", "other"), ("0", "=0"), ("1", "=1")];

fn main() {
  let mut args = std::env::args().skip(1);
  let mut first = args.next();
  let mode = match first.as_deref() {
    Some(flag @ ("--ucd" | "--zones" | "--names")) => {
      let flag = flag.to_owned();
      first = args.next();
      Some(flag)
    }
    _ => None,
  };
  let tab = if mode.as_deref() == Some("--zones") { args.next() } else { None };
  let (Some(root), Some(out)) = (first, args.next()) else {
    eprintln!("usage: snapfire_i18n_datagen <cldr-json> <out>\n       snapfire_i18n_datagen --ucd <ucd-dir> <out>\n       snapfire_i18n_datagen --zones <cldr-json> <zone.tab> <out>\n       snapfire_i18n_datagen --names <cldr-json> <out>");
    std::process::exit(2);
  };
  let made = match mode.as_deref() {
    Some("--ucd") => ucd::generate(Path::new(&root)),
    Some("--names") => names::generate(Path::new(&root)),
    Some(_) => zones::generate(Path::new(&root), Path::new(&tab.unwrap_or_default())),
    None => generate(Path::new(&root)),
  };
  let text = made.unwrap_or_else(|e| {
    eprintln!("snapfire_i18n_datagen: {e}");
    std::process::exit(1);
  });
  std::fs::write(&out, text).unwrap_or_else(|e| {
    eprintln!("snapfire_i18n_datagen: write {out}: {e}");
    std::process::exit(1);
  });
}

fn read(path: &Path) -> Result<Value, String> {
  let text = std::fs::read_to_string(path).map_err(|e| format!("read {}: {e}", path.display()))?;
  serde_json::from_str(&text).map_err(|e| format!("parse {}: {e}", path.display()))
}

fn str_at<'a>(value: &'a Value, keys: &[&str]) -> Option<&'a str> {
  keys.iter().try_fold(value, |v, k| v.get(*k)).and_then(Value::as_str)
}

/// A field of the data file. A tab, a newline or a separator inside CLDR text would break the line format, so one is refused.
fn field(text: &str) -> Result<&str, String> {
  match text.contains(['\t', '\n', '\u{1e}', '\u{1f}']) {
    true => Err(format!("`{text}` holds a character the data file uses as a separator")),
    false => Ok(text),
  }
}

fn locales(dir: &Path) -> Result<Vec<String>, String> {
  let mut names: Vec<String> = std::fs::read_dir(dir)
    .map_err(|e| format!("read {}: {e}", dir.display()))?
    .filter_map(|entry| entry.ok().map(|entry| entry.file_name().to_string_lossy().into_owned()))
    .collect();
  names.sort();
  Ok(names)
}

/// Bit 1 when the symbol starts with a letter, bit 0 when it ends with one: which side of the number an alpha-next-to-number pattern applies to.
fn letters(symbol: &str) -> u8 {
  let starts = symbol.chars().next().is_some_and(char::is_alphabetic) as u8;
  let ends = symbol.chars().next_back().is_some_and(char::is_alphabetic) as u8;
  starts << 1 | ends
}

/// The id of `record` in `table`, adding it when no equal record is there yet.
fn intern(table: &mut Vec<String>, index: &mut HashMap<String, usize>, record: String) -> usize {
  *index.entry(record.clone()).or_insert_with(|| {
    table.push(record);
    table.len() - 1
  })
}

fn numbers(numbers: &Value) -> Result<String, String> {
  let system = str_at(numbers, &["defaultNumberingSystem"]).unwrap_or("latn");
  let pick = |kind: &str| numbers.get(format!("{kind}-numberSystem-{system}")).or_else(|| numbers.get(format!("{kind}-numberSystem-latn")));
  let symbols = pick("symbols").ok_or("no number symbols")?;
  let percent = pick("percentFormats").and_then(|p| str_at(p, &["standard"])).ok_or("no percent pattern")?;
  let currency = pick("currencyFormats").ok_or("no currency patterns")?;
  let standard = str_at(currency, &["standard"]).ok_or("no currency pattern")?;
  let alpha = str_at(currency, &["standard-alphaNextToNumber"]).unwrap_or("");
  let symbol = |name: &str| str_at(symbols, &[name]).ok_or(format!("no `{name}` symbol"));
  Ok(format!(
    "{}\t{}\t{}\t{}\t{}\t{}",
    field(percent)?,
    field(standard)?,
    field(alpha)?,
    field(symbol("percentSign")?)?,
    field(symbol("minusSign")?)?,
    field(symbol("plusSign")?)?
  ))
}

/// One line per currency the locale writes other than as its code: `CODE`, symbol and its letter bits, narrow symbol and its letter bits. An absent symbol is empty.
fn symbols(currencies: &Value) -> Result<String, String> {
  let mut lines = Vec::new();
  let Some(map) = currencies.as_object() else { return Err("currencies is not an object".to_owned()) };
  for (code, names) in map {
    let symbol = str_at(names, &["symbol"]).filter(|s| *s != code).unwrap_or("");
    let narrow = str_at(names, &["symbol-alt-narrow"]).filter(|s| *s != code && *s != symbol).unwrap_or("");
    if symbol.is_empty() && narrow.is_empty() {
      continue;
    }
    lines.push(format!("{}\t{}\t{}\t{}\t{}", field(code)?, field(symbol)?, letters(symbol), field(narrow)?, letters(narrow)));
  }
  Ok(lines.join("\n"))
}

fn plurals(block: Option<&Value>) -> Result<String, String> {
  let mut forms = Vec::new();
  for (cldr, key) in PLURALS {
    if let Some(pattern) = block.and_then(|b| str_at(b, &[&format!("relativeTimePattern-count-{cldr}")])) {
      forms.push(format!("{key}\u{1f}{}", field(pattern)?));
    }
  }
  Ok(forms.join("\u{1e}"))
}

/// One line per unit and width: the field name, the `numeric: "auto"` literals by offset, then the future and past patterns by plural form.
fn relative(fields: &Value) -> Result<String, String> {
  let mut lines = Vec::new();
  for unit in UNITS {
    for width in WIDTHS {
      let name = format!("{unit}{width}");
      let Some(field_data) = fields.get(&name) else { return Err(format!("no `{name}` field")) };
      let mut literals = Vec::new();
      for offset in -2..=2 {
        if let Some(text) = str_at(field_data, &[&format!("relative-type-{offset}")]) {
          literals.push(format!("{offset}\u{1f}{}", field(text)?));
        }
      }
      let future = plurals(field_data.get("relativeTime-type-future"))?;
      let past = plurals(field_data.get("relativeTime-type-past"))?;
      lines.push(format!("{name}\t{}\t{future}\t{past}", literals.join("\u{1e}")));
    }
  }
  Ok(lines.join("\n"))
}

/// Writes each distinct line of `records` once under `line`, then each record under `table` as the comma-separated ids of its lines.
fn tables(out: &mut String, line: char, table: char, records: &[String]) {
  let (mut lines, mut index) = (Vec::new(), HashMap::new());
  let ids: Vec<Vec<usize>> = records.iter().map(|record| record.lines().map(|l| intern(&mut lines, &mut index, l.to_owned())).collect()).collect();
  for (id, text) in lines.iter().enumerate() {
    writeln!(out, "{line}\t{id}\t{text}").unwrap();
  }
  for (id, members) in ids.iter().enumerate() {
    let members: Vec<String> = members.iter().map(usize::to_string).collect();
    writeln!(out, "{table}\t{id}\t{}", members.join(",")).unwrap();
  }
}

fn generate(root: &Path) -> Result<String, String> {
  let numbers_dir: PathBuf = root.join("cldr-numbers-full/main");
  let dates_dir: PathBuf = root.join("cldr-dates-full/main");
  let core = root.join("cldr-core");
  let version = str_at(&read(&core.join("package.json"))?, &["version"]).ok_or("no cldr-core version")?.to_owned();

  let mut out = String::new();
  writeln!(out, "V\t{version}").unwrap();

  let fractions = read(&core.join("supplemental/currencyData.json"))?;
  let fractions = fractions.pointer("/supplemental/currencyData/fractions").and_then(Value::as_object).ok_or("no currency fractions")?;
  let digits = |info: &Value| str_at(info, &["_digits"]).unwrap_or("2").to_owned();
  let default = digits(fractions.get("DEFAULT").ok_or("no DEFAULT fraction")?);
  writeln!(out, "F\t*\t{default}").unwrap();
  for (code, info) in fractions {
    if code != "DEFAULT" && digits(info) != default {
      writeln!(out, "F\t{code}\t{}", digits(info)).unwrap();
    }
  }

  let (mut number_table, mut number_index) = (Vec::new(), HashMap::new());
  let (mut symbol_table, mut symbol_index) = (Vec::new(), HashMap::new());
  let (mut relative_table, mut relative_index) = (Vec::new(), HashMap::new());
  let mut rows = Vec::new();
  let dated: BTreeSet<String> = locales(&dates_dir)?.into_iter().collect();
  let numbered = locales(&numbers_dir)?;
  for locale in &numbered {
    if !dated.contains(locale) {
      continue;
    }
    let at = |e: String| format!("{locale}: {e}");
    let numbers_json = read(&numbers_dir.join(locale).join("numbers.json"))?;
    let currencies_json = read(&numbers_dir.join(locale).join("currencies.json"))?;
    let dates_json = read(&dates_dir.join(locale).join("dateFields.json"))?;
    let number = numbers(numbers_json.pointer(&format!("/main/{locale}/numbers")).ok_or_else(|| at("no numbers".to_owned()))?).map_err(at)?;
    let symbol = symbols(currencies_json.pointer(&format!("/main/{locale}/numbers/currencies")).ok_or_else(|| at("no currencies".to_owned()))?).map_err(at)?;
    let relative = relative(dates_json.pointer(&format!("/main/{locale}/dates/fields")).ok_or_else(|| at("no date fields".to_owned()))?).map_err(at)?;
    let n = intern(&mut number_table, &mut number_index, number);
    let s = intern(&mut symbol_table, &mut symbol_index, symbol);
    let r = intern(&mut relative_table, &mut relative_index, relative);
    rows.push(format!("L\t{locale}\t{n}\t{s}\t{r}"));
  }
  for (id, record) in number_table.iter().enumerate() {
    writeln!(out, "N\t{id}\t{record}").unwrap();
  }
  tables(&mut out, 's', 'S', &symbol_table);
  tables(&mut out, 'r', 'R', &relative_table);
  for row in &rows {
    writeln!(out, "{row}").unwrap();
  }

  let present: BTreeSet<&str> = numbered.iter().map(String::as_str).filter(|l| dated.contains(*l)).collect();
  let mut aliases: BTreeMap<String, String> = BTreeMap::new();
  let likely = read(&core.join("supplemental/likelySubtags.json"))?;
  let likely = likely.pointer("/supplemental/likelySubtags").and_then(Value::as_object).ok_or("no likely subtags")?;
  for (from, to) in likely {
    let parts: Vec<&str> = from.split('-').collect();
    let Some(to) = to.as_str() else { continue };
    let full: Vec<&str> = to.split('-').collect();
    if parts.len() != 2 || full.len() != 3 || present.contains(from.as_str()) {
      continue;
    }
    let (language, script) = (full[0], full[1]);
    let usual = likely.get(language).and_then(Value::as_str).and_then(|t| t.split('-').nth(1));
    if usual == Some(script) {
      continue;
    }
    let target = [to.to_owned(), format!("{language}-{script}")].into_iter().find(|t| present.contains(t.as_str()));
    if let Some(target) = target {
      aliases.insert(from.clone(), target);
    }
  }
  let parents = read(&core.join("supplemental/parentLocales.json"))?;
  let parents = parents.pointer("/supplemental/parentLocales/parentLocale").and_then(Value::as_object).ok_or("no parent locales")?;
  for (child, parent) in parents {
    if present.contains(child.as_str()) || aliases.contains_key(child) {
      continue;
    }
    let mut parent = parent.as_str().map(str::to_owned);
    while let Some(p) = parent.take() {
      if present.contains(p.as_str()) {
        aliases.insert(child.clone(), p);
        break;
      }
      parent = parents.get(&p).and_then(Value::as_str).map(str::to_owned);
    }
  }
  for (from, to) in &aliases {
    writeln!(out, "A\t{from}\t{to}").unwrap();
  }
  Ok(out)
}
