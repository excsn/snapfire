use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Format {
  Avif,
  Webp,
}

impl Format {
  pub const ALL: [Format; 2] = [Format::Avif, Format::Webp];

  pub fn extension(self) -> &'static str {
    match self {
      Format::Avif => "avif",
      Format::Webp => "webp",
    }
  }

  pub fn mime(self) -> &'static str {
    match self {
      Format::Avif => "image/avif",
      Format::Webp => "image/webp",
    }
  }

  pub fn parse(name: &str) -> Option<Format> {
    match name.trim().to_ascii_lowercase().as_str() {
      "avif" => Some(Format::Avif),
      "webp" => Some(Format::Webp),
      _ => None,
    }
  }

  /// The quality the policy uses when the configuration names none. The two
  /// scales differ: AVIF 60 looks like WebP 80.
  pub fn default_quality(self) -> u8 {
    match self {
      Format::Avif => 60,
      Format::Webp => 80,
    }
  }
}

impl std::fmt::Display for Format {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    f.write_str(self.extension())
  }
}

/// Which variants an image gets: the widths, the formats and a quality per
/// format. One policy serves a whole build; a per-image override is the same
/// struct with fields replaced.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VariantPolicy {
  pub widths: Vec<u32>,
  pub formats: Vec<Format>,
  #[serde(default)]
  pub quality: BTreeMap<Format, u8>,
}

impl Default for VariantPolicy {
  fn default() -> Self {
    Self {
      widths: vec![640, 960, 1280, 1920, 2560],
      formats: vec![Format::Avif, Format::Webp],
      quality: Format::ALL.iter().map(|f| (*f, f.default_quality())).collect(),
    }
  }
}

impl VariantPolicy {
  pub fn quality(&self, format: Format) -> u8 {
    self.quality.get(&format).copied().unwrap_or_else(|| format.default_quality())
  }

  /// The widths a source of `source_width` pixels gets: every policy width
  /// below its own, then its own. Nothing is ever upscaled, so a 400px logo
  /// gets one variant and a 4000px hero gets them all plus 4000.
  pub fn widths_for(&self, source_width: u32) -> Vec<u32> {
    let mut widths: Vec<u32> = self.widths.iter().copied().filter(|w| *w < source_width && *w > 0).collect();
    widths.sort_unstable();
    widths.dedup();
    if source_width > 0 {
      widths.push(source_width);
    }
    widths
  }

  pub fn with_widths(mut self, widths: Vec<u32>) -> Self {
    self.widths = widths;
    self
  }

  pub fn with_quality(mut self, format: Format, quality: u8) -> Self {
    self.quality.insert(format, quality.min(100));
    self
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use proptest::prelude::*;

  #[test]
  fn the_defaults_are_the_documented_ones() {
    let policy = VariantPolicy::default();
    assert_eq!(policy.widths, [640, 960, 1280, 1920, 2560]);
    assert_eq!(policy.formats, [Format::Avif, Format::Webp]);
    assert_eq!(policy.quality(Format::Avif), 60);
    assert_eq!(policy.quality(Format::Webp), 80);
  }

  #[test]
  fn a_small_source_gets_its_own_width_only_and_a_large_one_gets_every_width_plus_its_own() {
    let policy = VariantPolicy::default();
    assert_eq!(policy.widths_for(400), [400]);
    assert_eq!(policy.widths_for(640), [640]);
    assert_eq!(policy.widths_for(1000), [640, 960, 1000]);
    assert_eq!(policy.widths_for(4000), [640, 960, 1280, 1920, 2560, 4000]);
    assert_eq!(policy.widths_for(0), Vec::<u32>::new());
  }

  #[test]
  fn an_override_replaces_the_field_it_names_and_keeps_the_rest() {
    let policy = VariantPolicy::default().with_widths(vec![320, 160]).with_quality(Format::Avif, 90);
    assert_eq!(policy.widths_for(500), [160, 320, 500]);
    assert_eq!(policy.quality(Format::Avif), 90);
    assert_eq!(policy.quality(Format::Webp), 80);
    assert_eq!(policy.formats, [Format::Avif, Format::Webp]);
  }

  #[test]
  fn a_format_parses_from_its_extension_in_any_case() {
    assert_eq!(Format::parse("AVIF"), Some(Format::Avif));
    assert_eq!(Format::parse(" webp "), Some(Format::Webp));
    assert_eq!(Format::parse("jpeg"), None);
    let text = serde_json::to_string(&VariantPolicy::default()).unwrap();
    assert!(text.contains("\"avif\""), "{text}");
    let back: VariantPolicy = serde_json::from_str(&text).unwrap();
    assert_eq!(back, VariantPolicy::default());
  }

  proptest! {
    #[test]
    fn no_variant_is_wider_than_its_source_and_the_source_width_is_last(
      widths in proptest::collection::vec(1u32..5000, 0..12),
      source in 1u32..6000,
    ) {
      let policy = VariantPolicy::default().with_widths(widths);
      let got = policy.widths_for(source);
      prop_assert!(got.iter().all(|w| *w <= source));
      prop_assert_eq!(got.last().copied(), Some(source));
      prop_assert!(got.windows(2).all(|w| w[0] < w[1]));
    }
  }
}
