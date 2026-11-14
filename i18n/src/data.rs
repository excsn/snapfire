//! The CLDR data the crate reads and where it comes from: the tables built into the crate or bytes the caller holds.
//!
//! `build.rs` writes the embedded tables from the text files `datagen` writes under `data/`. A key is a record's kind and what it is of, joined by tabs. A value's fields are separated by tabs.
//!
//! - `cV`: the CLDR release.
//! - `cF CODE`: a currency's fraction digits, `*` for the default.
//! - `cL locale`: the ids of a locale's numbers, currency symbols and relative-time fields.
//! - `cA tag`: the locale a tag with no data of its own reads.
//! - `cN id`: number patterns and symbols: the percent pattern, the currency pattern, the `standard-alphaNextToNumber` currency pattern or nothing, the percent sign, the minus sign and the plus sign.
//! - `cS id CODE`: one currency's symbol and narrow symbol, each followed by its letters. An empty symbol means the code, an empty narrow means the symbol. The letters have bit 1 set when the symbol starts with a letter and bit 0 when it ends with one.
//! - `cR id field`: the id of one unit and width of relative time.
//! - `cr id`: the literals, `offset US text` joined by RS, then the future and past patterns, `form US pattern` joined by RS.
//! - `zV`: the CLDR release the zone names come from.
//! - `zE locale`: a locale the zone records hold, under its own name.
//! - `zK name`: the zone a name is of, as CLDR names it.
//! - `zM zone`: the metazones a zone belongs to, `metazone US from US to` joined by RS, both bounds milliseconds since 1970 UTC and empty for none.
//! - `zG metazone`: the zone whose rules stand for the metazone's.
//! - `zR region`: the calendars a region prefers, most preferred first.
//! - `zZ region`: the zones a region holds.
//! - `zP`, `zD`, `zF`, `zY`, `zJ`, `zO`, `zH` and `zT` `locale`, `zN`, `zS` and `zC` `locale key` and `zU locale width unit plural`: the records `zone.rs` describes under the same letters.
//! - `nV`: the CLDR release the display names come from.
//! - `nQ locale` and `nL`, `nR`, `nS`, `nC`, `nK` and `nF` `locale code`: the records `names.rs` describes under the same letters.

use std::collections::BTreeSet;

use crate::table::Table;
use crate::Error;

/// CLDR data to format from. [`Data::embedded`] is what the crate's own functions read. [`Data::from_static`] reads what [`Data::trimmed`] wrote.
#[derive(Clone, Copy)]
pub struct Data {
  core: Table,
  names: Option<Table>,
}

impl std::fmt::Debug for Data {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    f.debug_struct("Data").field("cldr", &self.cldr_version()).field("display_names", &self.has_display_names()).finish()
  }
}

/// A value's fields, an absent one empty.
pub(crate) fn fields<const N: usize>(value: &'static str) -> [&'static str; N] {
  let mut out = [""; N];
  let mut rest = value;
  for slot in &mut out {
    match rest.as_bytes().iter().position(|byte| *byte == b'\t') {
      Some(at) => {
        *slot = &rest[..at];
        rest = &rest[at + 1..];
      }
      None => {
        *slot = rest;
        break;
      }
    }
  }
  out
}

pub(crate) struct Numbers {
  pub percent: &'static str,
  pub currency: &'static str,
  pub currency_alpha: Option<&'static str>,
  pub percent_sign: &'static str,
  pub minus: &'static str,
  pub plus: &'static str,
}

const LOCALE_KINDS: [&str; 21] = ["cL", "zE", "zP", "zD", "zF", "zY", "zJ", "zO", "zH", "zT", "zN", "zS", "zC", "zU", "nQ", "nL", "nR", "nS", "nC", "nK", "nF"];

impl Data {
  /// The data built into the crate, every locale of the CLDR release.
  #[cfg(feature = "embedded")]
  pub fn embedded() -> Data {
    static DATA: std::sync::OnceLock<Data> = std::sync::OnceLock::new();
    *DATA.get_or_init(|| {
      let core = Table::open(include_bytes!(concat!(env!("OUT_DIR"), "/core.table"))).expect("snapfire_i18n data: the embedded table is not one");
      #[cfg(feature = "display_names")]
      let names = Table::open(include_bytes!(concat!(env!("OUT_DIR"), "/names.table")));
      #[cfg(not(feature = "display_names"))]
      let names = None;
      Data { core, names }
    })
  }

  /// The data `bytes` hold, as [`Data::trimmed`] wrote them.
  pub fn from_static(bytes: &'static [u8]) -> Result<Data, Error> {
    let table = Table::open(bytes).ok_or(Error::NotData)?;
    table.get(&["cV"]).ok_or(Error::NotData)?;
    Ok(Data { core: table, names: table.get(&["nV"]).map(|_| table) })
  }

  /// The CLDR release the data was generated from.
  pub fn cldr_version(&self) -> &'static str {
    self.get(&["cV"]).unwrap_or("")
  }

  /// Whether the data holds the display names.
  pub fn has_display_names(&self) -> bool {
    self.names.is_some()
  }

  pub(crate) fn get(&self, parts: &[&str]) -> Option<&'static str> {
    self.core.get(parts)
  }

  pub(crate) fn name(&self, parts: &[&str]) -> Option<&'static str> {
    self.names?.get(parts)
  }

  pub(crate) fn numbers(&self, id: &str) -> Numbers {
    let [percent, currency, alpha, percent_sign, minus, plus] = fields(self.get(&["cN", id]).unwrap_or("#,##0%\t¤\u{a0}#,##0.00\t\t%\t-\t+"));
    Numbers { percent, currency, currency_alpha: (!alpha.is_empty()).then_some(alpha), percent_sign, minus, plus }
  }

  /// The bytes of data that formats `locales` as this data does and holds no other locale's records. Each locale keeps the locales it falls back to. [`Data::from_static`] reads the bytes.
  pub fn trimmed(&self, locales: &[&str]) -> Vec<u8> {
    let mut kept: BTreeSet<String> = BTreeSet::new();
    let mut ids: BTreeSet<(&str, &'static str)> = BTreeSet::new();
    for tag in locales.iter().copied().chain(["und"]) {
      kept.extend(self.locale_chain(tag).into_iter().map(str::to_owned));
      for name in crate::locale::candidates(tag) {
        kept.extend(self.get(&["cA", &name]).map(str::to_owned));
        kept.insert(name);
      }
    }
    for name in &kept {
      let [numbers, symbols, relative] = fields(self.get(&["cL", name]).unwrap_or(""));
      ids.extend([("cN", numbers), ("cS", symbols), ("cR", relative)]);
    }
    let relative: BTreeSet<&'static str> = self.core.records().filter(|(key, _)| key.split('\t').next() == Some("cR") && key.split('\t').nth(1).is_some_and(|id| ids.contains(&("cR", id)))).map(|(_, id)| id).collect();
    let keeps = |key: &str| {
      let mut parts = key.split('\t');
      let (kind, of) = (parts.next().unwrap_or(""), parts.next().unwrap_or(""));
      match kind {
        "cN" | "cS" | "cR" => ids.contains(&(kind, of)),
        "cr" => relative.contains(of),
        kind if LOCALE_KINDS.contains(&kind) => kept.contains(of),
        _ => true,
      }
    };
    let names = self.names.filter(|names| !names.is(&self.core));
    let records = self.core.records().chain(names.into_iter().flat_map(|names| names.records())).filter(|(key, _)| keeps(key));
    crate::table::write(records).expect("snapfire_i18n data: records read from a table fit one")
  }
}
