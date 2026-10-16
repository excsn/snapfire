//! The Lightning CSS pass Tailwind runs on what it generates: nesting flattened, media
//! range syntax lowered for its browser targets and adjacent rules merged.

use lightningcss::printer::PrinterOptions;
use lightningcss::stylesheet::{MinifyOptions, ParserFlags, ParserOptions, StyleSheet};
use lightningcss::targets::{Browsers, Features, Targets};

use crate::Error;

#[derive(Debug, Clone, Default)]
#[non_exhaustive]
pub struct OptimizeOptions {
  pub minify: bool,
  /// The file name messages name.
  pub file: Option<String>,
}

impl OptimizeOptions {
  pub fn minify(mut self, minify: bool) -> Self {
    self.minify = minify;
    self
  }

  pub fn file(mut self, file: Option<String>) -> Self {
    self.file = file;
    self
  }
}

fn targets() -> Targets {
  Targets {
    browsers: Some(Browsers {
      safari: Some((16 << 16) | (4 << 8)),
      ios_saf: Some((16 << 16) | (4 << 8)),
      firefox: Some(128 << 16),
      chrome: Some(111 << 16),
      ..Browsers::default()
    }),
    include: Features::Nesting | Features::MediaQueries,
    exclude: Features::LogicalProperties | Features::DirSelector | Features::LightDark,
  }
}

fn pass(code: &str, options: &OptimizeOptions) -> Result<String, Error> {
  let parser = ParserOptions {
    filename: options.file.clone().unwrap_or_else(|| "input.css".to_owned()),
    error_recovery: true,
    flags: ParserFlags::CUSTOM_MEDIA | ParserFlags::DEEP_SELECTOR_COMBINATOR,
    ..ParserOptions::default()
  };
  let mut sheet = StyleSheet::parse(code, parser).map_err(|e| Error::Message(e.to_string()))?;
  sheet
    .minify(MinifyOptions {
      targets: targets(),
      ..MinifyOptions::default()
    })
    .map_err(|e| Error::Message(e.to_string()))?;
  let printed = sheet
    .to_css(PrinterOptions {
      minify: options.minify,
      targets: targets(),
      ..PrinterOptions::default()
    })
    .map_err(|e| Error::Message(e.to_string()))?;
  Ok(printed.code)
}

/// Runs the pass twice, so rules that only become adjacent once nesting is flattened merge.
pub fn optimize(css: &str, options: &OptimizeOptions) -> Result<String, Error> {
  let first = pass(css, options)?;
  let second = pass(&first, options)?;
  Ok(second.replace("@media not (", "@media not all and ("))
}
