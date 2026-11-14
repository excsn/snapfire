//! The locale data a program formats from: ICU4X's and snapfire_i18n's, read from one run of bytes a build cut for an application's locales. The `intl_compiled` feature builds every locale's into the crate.
//!
//! The bytes are `SFINTL`, a zero byte and the format's version, then three parts, each written as its length in four little-endian bytes followed by its bytes: what the data was cut for, the ICU4X blob and the snapfire_i18n table. The first part is lines of a name, a tab and a value: `icu`, `locales` separated by commas, `default`, `unlisted`, `words` and `source`.

use std::sync::OnceLock;

use icu_locale::fallback::LocaleFallbacker;
use icu_locale_core::Locale;
use icu_provider_adapters::fallback::LocaleFallbackProvider;
use icu_provider_blob::BlobDataProvider;

use crate::interp::Fail;
use crate::locales::{Locales, LocalesError, Refused, Unlisted};

const MAGIC: &[u8; 8] = b"SFINTL\x00\x01";

/// The ICU4X release whose data this crate reads. A blob another release wrote is refused.
pub const ICU: &str = "2.3";

/// Every locale's ICU4X data, the blob [`IntlData::compiled`] reads and a build cuts an application's from.
#[cfg(feature = "intl_compiled")]
pub const COMPILED_ICU: &[u8] = include_bytes!("../data/icu.postcard");

/// What names the data built into the crate: a cut says which data it was taken from by this.
#[cfg(feature = "intl_compiled")]
pub fn compiled_source() -> &'static str {
  static SOURCE: OnceLock<String> = OnceLock::new();
  SOURCE.get_or_init(|| format!("icu4x-{ICU}-{:016x}-cldr-{}", xxhash_rust::xxh3::xxh3_64(COMPILED_ICU), snapfire_i18n::Data::embedded().cldr_version()))
}

/// Whether the plan `text` splits words with `Intl.Segmenter`, which takes dictionaries a cut otherwise leaves out. A plan that does not parse and a granularity the plan computes both count as splitting words.
pub fn splits_words(text: &str) -> bool {
  fn any(form: &crate::sexpr::Sx) -> bool {
    use crate::sexpr::Sx;
    match form {
      Sx::List(items) => {
        let whole = |granularity: &Sx| match granularity {
          Sx::Str(name) => !matches!(name.as_str(), "grapheme" | "sentence"),
          Sx::List(wrapped) => !matches!(wrapped.as_slice(), [Sx::Sym(head), Sx::Str(name)] if head == "string" && matches!(name.as_str(), "grapheme" | "sentence")),
          _ => true,
        };
        let here = matches!(items.first(), Some(Sx::Sym(head)) if head == "segments") && items.get(2).is_none_or(whole);
        here || items.iter().any(any)
      }
      Sx::Interp(inner) => any(inner),
      Sx::Sym(_) | Sx::Str(_) => false,
    }
  }
  crate::sexpr::parse(text).map_or(true, |forms| forms.iter().any(any))
}

/// The locale tags the plan `text` gives an `intl` function as a literal, each once, in the order met: a `locale` field of an options object and the tag `toLocaleString` and its kin are given.
pub fn named_locales(text: &str) -> Vec<String> {
  use crate::sexpr::Sx;
  fn walk(form: &Sx, out: &mut Vec<String>) {
    let Sx::List(items) = form else {
      if let Sx::Interp(inner) = form {
        walk(inner, out);
      }
      return;
    };
    if let [Sx::Sym(head), Sx::Sym(module), Sx::Sym(name), args @ ..] = items.as_slice() {
      if head == "ext" && module == "intl" {
        let mut named = |tag: &Sx| {
          if let Sx::Str(tag) = tag {
            if !out.contains(tag) {
              out.push(tag.clone());
            }
          }
        };
        if matches!(name.as_str(), "numeric" | "numericParts") {
          args.get(1).into_iter().for_each(&mut named);
        }
        for arg in args {
          let Sx::List(object) = arg else { continue };
          if !matches!(object.first(), Some(Sx::Sym(head)) if head == "obj") {
            continue;
          }
          for entry in &object[1..] {
            if let Sx::List(field) = entry {
              if let [Sx::Sym(key), tag] = field.as_slice() {
                if key == "locale" {
                  named(tag);
                }
              }
            }
          }
        }
      }
    }
    items.iter().for_each(|item| walk(item, out));
  }
  let mut out = Vec::new();
  for form in crate::sexpr::parse(text).unwrap_or_default() {
    walk(&form, &mut out);
  }
  out
}

/// The locales data was cut for and what a tag outside them gets.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct Cut {
  pub locales: Vec<String>,
  pub default: String,
  pub unlisted: Unlisted,
  /// Whether the data holds the dictionaries `Intl.Segmenter` splits words by.
  pub words: bool,
  /// What names the data the cut was taken from, so a cut from other data is told apart.
  pub source: String,
}

impl Cut {
  /// A cut with no word dictionaries from a source with no name.
  pub fn new(locales: Vec<String>, default: String, unlisted: Unlisted) -> Self {
    Self { locales, default, unlisted, words: false, source: String::new() }
  }

  pub fn words(mut self, words: bool) -> Self {
    self.words = words;
    self
  }

  pub fn source(mut self, source: impl Into<String>) -> Self {
    self.source = source.into();
    self
  }
}

/// Why bytes are not locale data this crate formats from.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum IntlDataError {
  #[error("the bytes are not locale data this version reads")]
  NotData,
  #[error("the data is ICU4X {0}'s and this version reads ICU4X {ICU}'s")]
  Icu(String),
  #[error("the ICU4X data does not load: {0}")]
  Blob(String),
  #[error("the snapfire_i18n data does not load: {0}")]
  Tables(String),
  #[error("the locales the data was cut for: {0}")]
  Locales(#[from] LocalesError),
}

/// Locale data to format from.
pub struct IntlData {
  icu: LocaleFallbackProvider<BlobDataProvider>,
  tables: snapfire_i18n::Data,
  cut: Option<(Cut, Locales)>,
  id: u64,
}

impl std::fmt::Debug for IntlData {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    f.debug_struct("IntlData").field("cut", &self.cut()).field("tables", &self.tables).finish()
  }
}

static INSTALLED: std::sync::atomic::AtomicPtr<IntlData> = std::sync::atomic::AtomicPtr::new(std::ptr::null_mut());

fn part<'a>(bytes: &'a [u8], at: &mut usize) -> Option<&'a [u8]> {
  let len = u32::from_le_bytes(bytes.get(*at..*at + 4)?.try_into().ok()?) as usize;
  let part = bytes.get(*at + 4..*at + 4 + len)?;
  *at += 4 + len;
  Some(part)
}

fn provider(blob: &'static [u8]) -> Result<LocaleFallbackProvider<BlobDataProvider>, IntlDataError> {
  let blob = BlobDataProvider::try_new_from_static_blob(blob).map_err(|e| IntlDataError::Blob(e.to_string()))?;
  let fallbacker = LocaleFallbacker::try_new_with_buffer_provider(&blob).map_err(|e| IntlDataError::Blob(e.to_string()))?;
  Ok(LocaleFallbackProvider::new(blob, fallbacker))
}

impl IntlData {
  /// The bytes [`IntlData::from_static`] reads: an ICU4X blob and a snapfire_i18n table, both cut for `cut`.
  pub fn packed(icu: &[u8], tables: &[u8], cut: &Cut) -> Vec<u8> {
    let unlisted = match cut.unlisted {
      Unlisted::Fallback => "fallback",
      Unlisted::Error => "error",
    };
    let described = format!("icu\t{ICU}\nlocales\t{}\ndefault\t{}\nunlisted\t{unlisted}\nwords\t{}\nsource\t{}\n", cut.locales.join(","), cut.default, cut.words, cut.source);
    let mut out = MAGIC.to_vec();
    for part in [described.as_bytes(), icu, tables] {
      out.extend_from_slice(&(part.len() as u32).to_le_bytes());
      out.extend_from_slice(part);
    }
    out
  }

  /// The bytes [`IntlData::from_static`] reads for `cut`, holding every locale's ICU4X data and `cut`'s locales of snapfire_i18n's. A build cuts the ICU4X data as well.
  #[cfg(feature = "intl_compiled")]
  pub fn packed_from_compiled(cut: &Cut) -> Vec<u8> {
    let listed: Vec<&str> = cut.locales.iter().map(String::as_str).collect();
    Self::packed(COMPILED_ICU, &snapfire_i18n::Data::embedded().trimmed(&listed), cut)
  }

  /// What `bytes` say they were cut for, when they are locale data of this version and ICU4X release. The data itself is not read.
  pub fn cut_of(bytes: &[u8]) -> Result<Cut, IntlDataError> {
    Self::parts(bytes).map(|(cut, _, _)| cut)
  }

  fn parts(bytes: &[u8]) -> Result<(Cut, &[u8], &[u8]), IntlDataError> {
    if bytes.get(..MAGIC.len()) != Some(MAGIC.as_slice()) {
      return Err(IntlDataError::NotData);
    }
    let mut at = MAGIC.len();
    let mut next = || part(bytes, &mut at).ok_or(IntlDataError::NotData);
    let described = std::str::from_utf8(next()?).map_err(|_| IntlDataError::NotData)?;
    let (icu, tables) = (next()?, next()?);
    let value = |name: &str| described.lines().find_map(|line| line.split_once('\t').filter(|(key, _)| *key == name).map(|(_, value)| value)).ok_or(IntlDataError::NotData);
    if value("icu")? != ICU {
      return Err(IntlDataError::Icu(value("icu")?.to_owned()));
    }
    let unlisted = match value("unlisted")? {
      "fallback" => Unlisted::Fallback,
      "error" => Unlisted::Error,
      _ => return Err(IntlDataError::NotData),
    };
    let cut = Cut { locales: value("locales")?.split(',').filter(|tag| !tag.is_empty()).map(str::to_owned).collect(), default: value("default")?.to_owned(), unlisted, words: value("words")? == "true", source: value("source")?.to_owned() };
    Ok((cut, icu, tables))
  }

  /// The data `bytes` hold, as [`IntlData::packed`] wrote them.
  pub fn from_static(bytes: &'static [u8]) -> Result<IntlData, IntlDataError> {
    let (cut, icu, tables) = Self::parts(bytes)?;
    let locales = Locales::new(&cut.locales, &cut.default, cut.unlisted)?;
    let tables = snapfire_i18n::Data::from_static(tables).map_err(|e| IntlDataError::Tables(e.to_string()))?;
    Ok(IntlData { icu: provider(icu)?, tables, cut: Some((cut, locales)), id: xxhash_rust::xxh3::xxh3_64(bytes) })
  }

  /// Every locale's data, built into the crate.
  #[cfg(feature = "intl_compiled")]
  pub fn compiled() -> &'static IntlData {
    static COMPILED: OnceLock<IntlData> = OnceLock::new();
    COMPILED.get_or_init(|| IntlData { icu: provider(COMPILED_ICU).expect("the ICU4X data built into the crate loads"), tables: snapfire_i18n::Data::embedded(), cut: None, id: 0 })
  }

  /// Makes this the data the program formats from, in place of what it installed before. The data is kept for as long as the program runs.
  pub fn install(self) {
    INSTALLED.store(Box::leak(Box::new(self)), std::sync::atomic::Ordering::Release);
  }

  /// Drops what the program installed, so it formats from the data the `intl_compiled` feature builds in.
  pub fn uninstall() {
    INSTALLED.store(std::ptr::null_mut(), std::sync::atomic::Ordering::Release);
  }

  /// The data the program formats from: what it installed last, else what the `intl_compiled` feature builds in.
  pub fn current() -> Result<&'static IntlData, Fail> {
    // SAFETY: the pointer is null or one `install` leaked, which nothing frees.
    if let Some(installed) = unsafe { INSTALLED.load(std::sync::atomic::Ordering::Acquire).as_ref() } {
      return Ok(installed);
    }
    #[cfg(feature = "intl_compiled")]
    return Ok(Self::compiled());
    #[cfg(not(feature = "intl_compiled"))]
    Err(Fail::internal("this program holds no locale data: it installed none and is built without `intl_compiled`"))
  }

  /// What the data was cut for, `None` for data holding every locale.
  pub fn cut(&self) -> Option<&Cut> {
    self.cut.as_ref().map(|(cut, _)| cut)
  }

  /// The locale `tag` formats in: itself when the data holds every locale, else the one of the locales the data was cut for that covers it. `what` names the caller in a refusal.
  pub fn locale(&self, what: &str, tag: &str) -> Result<Locale, Fail> {
    let malformed = || Fail::internal(format!("{what}: `{tag}` is not a locale"));
    match &self.cut {
      None => tag.parse().map_err(|_| malformed()),
      Some((cut, locales)) => locales.resolve(tag).map_err(|refused| match refused {
        Refused::Unlisted => Fail::internal(format!("RangeError: {what}: `{tag}` is not one of the application's locales ({}); add it to `[locales] supported`", cut.locales.join(", "))),
        _ => malformed(),
      }),
    }
  }

  /// Whether the data holds the dictionaries `Intl.Segmenter` splits words by.
  pub fn splits_words(&self) -> bool {
    self.cut().is_none_or(|cut| cut.words)
  }

  /// What tells this data from other data a program could hold.
  pub(crate) fn id(&self) -> u64 {
    self.id
  }

  pub(crate) fn icu(&self) -> &LocaleFallbackProvider<BlobDataProvider> {
    &self.icu
  }

  pub(crate) fn tables(&self) -> &snapfire_i18n::Data {
    &self.tables
  }
}
