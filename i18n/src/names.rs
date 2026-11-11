//! Display names as `Intl.DisplayNames` writes them, from CLDR's locale display names, currency names and date field names.
//!
//! `data/names.txt` is written by `datagen --names` from a cldr-json release and embedded with the `display_names` feature. One record per line, fields separated by tabs, tagged by the first field:
//!
//! - `V version`: the CLDR release.
//! - `Q locale pattern separator`: how a language's name takes its script and region, `{0} ({1})`. The separator joins those two, `{0}, {1}`.
//! - `L`, `R`, `S`, `C`, `K` or `F` `locale code name`: the name of a language, a region, a script, a currency, a calendar or a date field. A code ending `-alt-short` is the short form.
//!
//! A locale falls back through the parents `data/zones.txt` records.

use std::collections::HashMap;
use std::sync::OnceLock;

#[cfg(feature = "display_names")]
const TEXT: &str = include_str!("../data/names.txt");
#[cfg(not(feature = "display_names"))]
const TEXT: &str = "";

#[derive(Default)]
struct Names {
  names: HashMap<(char, &'static str), &'static str>,
  pattern: Option<(&'static str, &'static str)>,
}

fn names() -> &'static HashMap<&'static str, Names> {
  static NAMES: OnceLock<HashMap<&'static str, Names>> = OnceLock::new();
  NAMES.get_or_init(|| {
    let mut out: HashMap<&'static str, Names> = HashMap::new();
    for line in TEXT.lines() {
      let f: Vec<&'static str> = line.split('\t').collect();
      match f[0] {
        "V" => {}
        "Q" => out.entry(f[1]).or_default().pattern = Some((f[2], f[3])),
        kind => {
          let kind = kind.chars().next().unwrap_or(' ');
          out.entry(f[1]).or_default().names.insert((kind, f[2]), f[3]);
        }
      }
    }
    out
  })
}

/// Whether this build holds the display names, the `display_names` feature.
pub fn has_display_names() -> bool {
  cfg!(feature = "display_names")
}

/// What `Intl.DisplayNames` is asked to name.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NameKind {
  Language,
  Region,
  Script,
  Currency,
  Calendar,
  DateTimeField,
}

impl NameKind {
  /// The kind `type` names, `None` for one `Intl.DisplayNames` does not take.
  pub fn from_type(name: &str) -> Option<NameKind> {
    Some(match name {
      "language" => NameKind::Language,
      "region" => NameKind::Region,
      "script" => NameKind::Script,
      "currency" => NameKind::Currency,
      "calendar" => NameKind::Calendar,
      "dateTimeField" => NameKind::DateTimeField,
      _ => return None,
    })
  }

  fn tag(self) -> char {
    match self {
      NameKind::Language => 'L',
      NameKind::Region => 'R',
      NameKind::Script => 'S',
      NameKind::Currency => 'C',
      NameKind::Calendar => 'K',
      NameKind::DateTimeField => 'F',
    }
  }
}

fn lookup(chain: &[&'static str], kind: char, code: &str) -> Option<&'static str> {
  let all = names();
  chain.iter().find_map(|locale| all.get(locale).and_then(|n| n.names.get(&(kind, code)).copied()))
}

/// The name `Intl.DisplayNames` gives `code` of `kind` in `tag`, `None` for a code the data does not name. `style` is `long`, `short` or `narrow`. A short or narrow style takes CLDR's short form where it has one. A date field takes its short or narrow name. A language reads as one dialect name when CLDR has its whole tag, `American English`, else as the language's name with its script and region in the locale's pattern, `Chinese (Simplified, China)`; `standard` always composes it.
pub fn display_name(tag: &str, kind: NameKind, code: &str, style: &str, standard: bool) -> Option<String> {
  let short = style != "long";
  let chain = crate::zone::locale_chain(tag);
  let find = |kind: char, code: &str| -> Option<&'static str> {
    if short {
      if let Some(name) = lookup(&chain, kind, &format!("{code}-alt-short")) {
        return Some(name);
      }
    }
    lookup(&chain, kind, code)
  };
  match kind {
    NameKind::Language => {
      let (language, script, region) = crate::locale::subtags(code);
      let mut base = language.clone();
      if let Some(script) = &script {
        base.push('-');
        base.push_str(script);
      }
      if let Some(region) = &region {
        base.push('-');
        base.push_str(region);
      }
      if !standard {
        if let Some(name) = find('L', &base) {
          return Some(name.to_owned());
        }
      }
      let name = find('L', &language)?;
      let mut qualifiers = Vec::new();
      if let Some(script) = &script {
        qualifiers.push(find('S', script).map_or_else(|| script.clone(), str::to_owned));
      }
      if let Some(region) = &region {
        qualifiers.push(find('R', region).map_or_else(|| region.clone(), str::to_owned));
      }
      if qualifiers.is_empty() {
        return Some(name.to_owned());
      }
      let all = names();
      let (pattern, separator) = chain.iter().find_map(|l| all.get(l).and_then(|n| n.pattern)).unwrap_or(("{0} ({1})", "{0}, {1}"));
      let joined = qualifiers.into_iter().reduce(|a, b| separator.replace("{0}", &a).replace("{1}", &b)).unwrap_or_default();
      Some(pattern.replace("{0}", name).replace("{1}", &joined))
    }
    NameKind::Calendar => {
      let cldr = match code {
        "gregory" => "gregorian",
        "ethioaa" => "ethiopic-amete-alem",
        "islamicc" => "islamic-civil",
        other => other,
      };
      find('K', cldr).map(str::to_owned)
    }
    NameKind::DateTimeField => {
      let field = match code {
        "weekOfYear" => "week",
        "dayPeriod" => "dayperiod",
        "timeZoneName" => "zone",
        other => other,
      };
      let sized = if short { lookup(&chain, 'F', &format!("{field}-{style}")) } else { None };
      sized.or_else(|| lookup(&chain, 'F', field)).map(str::to_owned)
    }
    other => find(other.tag(), code).map(str::to_owned),
  }
}
