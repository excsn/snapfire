//! The embedded CLDR data and the tables read out of it on first use.
//!
//! `data/cldr.txt` is written by `datagen` from a cldr-json release. One record per line, fields separated by tabs, tagged by the first field:
//!
//! - `V version`: the CLDR release.
//! - `F CODE digits`: a currency's fraction digits, `*` for the default.
//! - `N id percent currency alpha percentSign minusSign plusSign`: a locale's number patterns and symbols in its default numbering system. `alpha` is the `standard-alphaNextToNumber` currency pattern or empty.
//! - `s id CODE symbol letters narrow letters`: one currency's symbols. An empty symbol means the code, an empty narrow means the symbol. `letters` has bit 1 set when the symbol starts with a letter and bit 0 when it ends with one.
//! - `S id ids`: a locale's currency symbols, as the comma-separated ids of `s` lines.
//! - `r id field literals future past`: one unit and width of relative time. `literals` are `offset US text` joined by RS, the patterns `form US pattern` joined by RS.
//! - `R id ids`: a locale's relative-time fields, as the comma-separated ids of `r` lines.
//! - `L locale N S R`: a locale's ids.
//! - `A from to`: a tag with no data of its own that reads another locale's.

use std::collections::HashMap;
use std::sync::OnceLock;

const TEXT: &str = include_str!("../data/cldr.txt");

pub(crate) struct Numbers {
  pub percent: &'static str,
  pub currency: &'static str,
  pub currency_alpha: Option<&'static str>,
  pub percent_sign: &'static str,
  pub minus: &'static str,
  pub plus: &'static str,
}

pub(crate) struct Symbol {
  pub symbol: Option<(&'static str, u8)>,
  pub narrow: Option<(&'static str, u8)>,
}

#[derive(Debug)]
pub(crate) struct Field {
  pub literals: Vec<(i8, &'static str)>,
  pub future: Vec<(&'static str, &'static str)>,
  pub past: Vec<(&'static str, &'static str)>,
}

pub(crate) struct Locale {
  pub numbers: usize,
  pub symbols: usize,
  pub relative: usize,
}

pub(crate) struct Data {
  pub version: &'static str,
  pub default_digits: u8,
  pub digits: HashMap<&'static str, u8>,
  pub numbers: Vec<Numbers>,
  pub symbol_lines: Vec<Symbol>,
  pub symbols: Vec<HashMap<&'static str, usize>>,
  pub field_lines: Vec<Field>,
  pub relative: Vec<HashMap<&'static str, usize>>,
  pub locales: HashMap<&'static str, Locale>,
  pub aliases: HashMap<&'static str, &'static str>,
}

pub(crate) fn data() -> &'static Data {
  static DATA: OnceLock<Data> = OnceLock::new();
  DATA.get_or_init(|| parse(TEXT))
}

fn number(text: &str) -> usize {
  text.parse().unwrap_or_else(|_| panic!("snapfire_i18n data: `{text}` is not an id"))
}

fn ids(text: &str) -> Vec<usize> {
  text.split(',').filter(|id| !id.is_empty()).map(number).collect()
}

fn pairs(text: &'static str) -> Vec<(&'static str, &'static str)> {
  text.split('\u{1e}').filter(|pair| !pair.is_empty()).filter_map(|pair| pair.split_once('\u{1f}')).collect()
}

fn symbol(text: &'static str, letters: &str) -> Option<(&'static str, u8)> {
  (!text.is_empty()).then(|| (text, letters.parse().unwrap_or(0)))
}

fn parse(text: &'static str) -> Data {
  let mut data = Data {
    version: "",
    default_digits: 2,
    digits: HashMap::new(),
    numbers: Vec::new(),
    symbol_lines: Vec::new(),
    symbols: Vec::new(),
    field_lines: Vec::new(),
    relative: Vec::new(),
    locales: HashMap::new(),
    aliases: HashMap::new(),
  };
  let mut symbol_keys: Vec<&'static str> = Vec::new();
  let mut field_keys: Vec<&'static str> = Vec::new();
  let mut symbol_tables: Vec<Vec<usize>> = Vec::new();
  let mut field_tables: Vec<Vec<usize>> = Vec::new();
  for line in text.lines() {
    let f: Vec<&'static str> = line.split('\t').collect();
    match f[0] {
      "V" => data.version = f[1],
      "F" if f[1] == "*" => data.default_digits = f[2].parse().unwrap_or(2),
      "F" => {
        data.digits.insert(f[1], f[2].parse().unwrap_or(2));
      }
      "N" => data.numbers.push(Numbers {
        percent: f[2],
        currency: f[3],
        currency_alpha: (!f[4].is_empty()).then_some(f[4]),
        percent_sign: f[5],
        minus: f[6],
        plus: f[7],
      }),
      "s" => {
        symbol_keys.push(f[2]);
        data.symbol_lines.push(Symbol { symbol: symbol(f[3], f[4]), narrow: symbol(f[5], f[6]) });
      }
      "S" => symbol_tables.push(ids(f[2])),
      "r" => {
        field_keys.push(f[2]);
        data.field_lines.push(Field {
          literals: pairs(f[3]).into_iter().filter_map(|(offset, text)| Some((offset.parse().ok()?, text))).collect(),
          future: pairs(f[4]),
          past: pairs(f[5]),
        });
      }
      "R" => field_tables.push(ids(f[2])),
      "L" => {
        data.locales.insert(f[1], Locale { numbers: number(f[2]), symbols: number(f[3]), relative: number(f[4]) });
      }
      "A" => {
        data.aliases.insert(f[1], f[2]);
      }
      other => panic!("snapfire_i18n data: unknown record `{other}`"),
    }
  }
  data.symbols = symbol_tables.iter().map(|table| table.iter().map(|&id| (symbol_keys[id], id)).collect()).collect();
  data.relative = field_tables.iter().map(|table| table.iter().map(|&id| (field_keys[id], id)).collect()).collect();
  data
}
