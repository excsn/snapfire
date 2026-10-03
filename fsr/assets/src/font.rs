use crate::Error;
use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Style {
  Normal,
  Italic,
}

impl Style {
  pub fn as_css(self) -> &'static str {
    match self {
      Style::Normal => "normal",
      Style::Italic => "italic",
    }
  }
}

/// The vertical metrics and the average advance a fallback is matched to, in
/// the face's own units.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Metrics {
  pub units_per_em: u16,
  pub ascender: i16,
  pub descender: i16,
  pub line_gap: i16,
  /// The advance of `a` to `z` weighted by English letter frequency, in font
  /// units, which is what `size-adjust` scales by. `OS/2`'s `xAvgCharWidth`
  /// averages every glyph in the font and a face with a wide symbol set
  /// overstates its text width by half.
  pub avg_char_width: i16,
  pub cap_height: Option<i16>,
  pub x_height: Option<i16>,
}

impl Metrics {
  fn avg_width(&self) -> f64 {
    f64::from(self.avg_char_width) / f64::from(self.units_per_em)
  }
}

/// One face as its tables describe it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Face {
  pub family: String,
  pub weight: u16,
  pub style: Style,
  pub metrics: Metrics,
}

impl Face {
  pub fn read(path: &Path) -> Result<Self, Error> {
    let bytes = std::fs::read(path).map_err(|e| Error::Io(path.to_path_buf(), e))?;
    let ext = path.extension().and_then(|e| e.to_str()).map(|e| e.to_ascii_lowercase()).unwrap_or_default();
    Self::from_bytes(path, &bytes, &ext)
  }

  /// `ext` says what the bytes are: `woff2` and `woff` are unpacked to the
  /// sfnt inside, `ttf` and `otf` are read as they are. `path` names the file
  /// in an error only.
  pub fn from_bytes(path: &Path, bytes: &[u8], ext: &str) -> Result<Self, Error> {
    let sfnt: std::borrow::Cow<[u8]> = match ext {
      "woff2" => wuff::decompress_woff2(bytes).map_err(|e| Error::Font(path.to_path_buf(), format!("{e:?}")))?.into(),
      "woff" => wuff::decompress_woff1(bytes).map_err(|e| Error::Font(path.to_path_buf(), format!("{e:?}")))?.into(),
      _ => bytes.into(),
    };
    let face = ttf_parser::Face::parse(&sfnt, 0).map_err(|e| Error::Font(path.to_path_buf(), e.to_string()))?;

    let family = name(&face, ttf_parser::name_id::TYPOGRAPHIC_FAMILY)
      .or_else(|| name(&face, ttf_parser::name_id::FAMILY))
      .ok_or_else(|| Error::Font(path.to_path_buf(), "no family name".to_owned()))?;

    let avg_char_width = weighted_advance(&face).ok_or_else(|| Error::Font(path.to_path_buf(), "no a to z glyphs".to_owned()))?;

    Ok(Face {
      family,
      weight: face.weight().to_number(),
      style: if face.is_italic() || face.is_oblique() { Style::Italic } else { Style::Normal },
      metrics: Metrics {
        units_per_em: face.units_per_em(),
        ascender: face.ascender(),
        descender: face.descender(),
        line_gap: face.line_gap(),
        avg_char_width,
        cap_height: face.capital_height(),
        x_height: face.x_height(),
      },
    })
  }

  /// The `@font-face` for a fallback drawn with `fallback` so its text takes
  /// the same room as this face's: `size-adjust` matches the average advance
  /// and the three overrides match the vertical metrics after that scale.
  /// `family` is the name the fallback face is declared under.
  pub fn fallback_face(&self, family: &str, fallback: &Fallback) -> String {
    let adjust = self.metrics.avg_width() / fallback.metrics.avg_width();
    let em = f64::from(self.metrics.units_per_em);
    let pct = |value: f64| format!("{:.2}%", value * 100.0);
    format!(
      "@font-face {{ font-family: \"{}\"; src: local(\"{}\"); size-adjust: {}; ascent-override: {}; descent-override: {}; line-gap-override: {}; }}",
      family,
      fallback.local,
      pct(adjust),
      pct(f64::from(self.metrics.ascender) / em / adjust),
      pct(f64::from(self.metrics.descender).abs() / em / adjust),
      pct(f64::from(self.metrics.line_gap) / em / adjust),
    )
  }
}

/// A name record, a Unicode one when the font has it and the Macintosh
/// Roman one otherwise, which is all some system fonts carry.
fn name(face: &ttf_parser::Face<'_>, id: u16) -> Option<String> {
  let records: Vec<_> = face.names().into_iter().filter(|n| n.name_id == id).collect();
  records
    .iter()
    .filter(|n| n.is_unicode())
    .find_map(|n| n.to_string())
    .or_else(|| {
      records
        .iter()
        .find(|n| n.platform_id == ttf_parser::PlatformId::Macintosh && n.name.is_ascii())
        .map(|n| String::from_utf8_lossy(n.name).into_owned())
    })
}

/// Relative frequency of each letter in English text, `a` first, in
/// thousandths. The weights Capsize and `next/font` size a fallback with.
const LETTER_WEIGHTS: [u32; 26] = [
  8167, 1492, 2782, 4253, 12702, 2228, 2015, 6094, 6966, 153, 772, 4025, 2406, 6749, 7507, 1929, 95, 5987, 6327, 9056,
  2758, 978, 2360, 150, 1974, 74,
];

fn weighted_advance(face: &ttf_parser::Face<'_>) -> Option<i16> {
  let mut total = 0f64;
  let mut weight = 0f64;
  for (letter, w) in ('a'..='z').zip(LETTER_WEIGHTS) {
    let Some(glyph) = face.glyph_index(letter) else { continue };
    let Some(advance) = face.glyph_hor_advance(glyph) else { continue };
    total += f64::from(advance) * f64::from(w);
    weight += f64::from(w);
  }
  (weight > 0.0).then(|| (total / weight).round() as i16)
}

/// A face a visitor already has, with the metrics a fallback is computed
/// against. The numbers were read with `Face::read` from the fonts macOS
/// ships, which are the same files Windows ships for these families.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Fallback {
  /// The name `local()` finds it under.
  pub local: &'static str,
  pub metrics: Metrics,
}

macro_rules! fallback {
  ($local:literal, $upm:literal, $asc:literal, $desc:literal, $gap:literal, $avg:literal) => {
    Fallback {
      local: $local,
      metrics: Metrics {
        units_per_em: $upm,
        ascender: $asc,
        descender: $desc,
        line_gap: $gap,
        avg_char_width: $avg,
        cap_height: None,
        x_height: None,
      },
    }
  };
}

const FALLBACKS: &[Fallback] = &[
  fallback!("Arial", 2048, 1854, -434, 67, 978),
  fallback!("Arial Black", 2048, 2254, -634, 0, 1227),
  fallback!("Helvetica", 2048, 1577, -471, 0, 978),
  fallback!("Verdana", 2048, 2059, -430, 0, 1112),
  fallback!("Tahoma", 2048, 2049, -423, 0, 972),
  fallback!("Trebuchet MS", 2048, 1923, -455, 0, 998),
  fallback!("Georgia", 2048, 1878, -449, 0, 988),
  fallback!("Times New Roman", 2048, 1825, -443, 87, 887),
  fallback!("Courier New", 2048, 1705, -615, 0, 1229),
];

/// The fallback declared for `name`, matched without regard to case.
pub fn fallback(name: &str) -> Option<&'static Fallback> {
  FALLBACKS.iter().find(|f| f.local.eq_ignore_ascii_case(name.trim()))
}

/// Every fallback this crate knows, for a configuration check to name.
pub fn fallbacks() -> impl Iterator<Item = &'static str> {
  FALLBACKS.iter().map(|f| f.local)
}

#[cfg(test)]
mod tests {
  use super::*;

  fn fixture(name: &str) -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures").join(name)
  }

  #[test]
  fn a_woff2_is_read_for_its_family_weight_style_and_metrics() {
    let regular = Face::read(&fixture("Inter-Regular.woff2")).unwrap();
    assert_eq!(regular.family, "Inter");
    assert_eq!(regular.weight, 400);
    assert_eq!(regular.style, Style::Normal);
    assert!(regular.metrics.units_per_em >= 1000);
    assert!(regular.metrics.ascender > 0 && regular.metrics.descender < 0);
    assert!(regular.metrics.avg_char_width > 0);
    assert!(regular.metrics.avg_char_width < regular.metrics.units_per_em as i16 * 3 / 4);

    let bold = Face::read(&fixture("Inter-Bold.woff2")).unwrap();
    assert_eq!(bold.family, "Inter");
    assert_eq!(bold.weight, 700);
    assert_eq!(bold.metrics.units_per_em, regular.metrics.units_per_em);
  }

  #[test]
  fn the_fallback_face_scales_arial_to_inters_advance_and_overrides_the_vertical_metrics() {
    let inter = Face::read(&fixture("Inter-Regular.woff2")).unwrap();
    let css = inter.fallback_face("Inter Fallback", fallback("arial").unwrap());
    assert!(css.starts_with("@font-face { font-family: \"Inter Fallback\"; src: local(\"Arial\"); size-adjust: "), "{css}");
    let number = |key: &str| -> f64 {
      let start = css.find(key).unwrap() + key.len();
      css[start..].split('%').next().unwrap().trim().parse().unwrap()
    };
    let adjust = number("size-adjust: ");
    assert!((105.0..112.0).contains(&adjust), "{css}");
    let ascent = number("ascent-override: ");
    assert!((80.0..110.0).contains(&ascent), "{css}");
    let descent = number("descent-override: ");
    assert!((15.0..35.0).contains(&descent), "{css}");
    assert!(css.contains("line-gap-override: "), "{css}");
  }

  #[test]
  fn an_unknown_fallback_is_none_and_the_known_ones_are_listed() {
    assert!(fallback("Comic Sans MS").is_none());
    assert_eq!(fallback("ARIAL").unwrap().local, "Arial");
    assert!(fallbacks().any(|f| f == "Georgia"));
  }

  #[test]
  fn bytes_that_are_not_a_font_are_refused_with_the_path() {
    let err = Face::from_bytes(Path::new("x.ttf"), b"nope", "ttf").unwrap_err();
    assert!(err.to_string().contains("x.ttf"), "{err}");
  }
}
