//! What FSR derives from an image or a font. `fsr build` calls these at build
//! time; a host answering images at request time calls the same functions.

pub mod font;
pub mod hash;
pub mod image;
pub mod policy;

pub use font::{Face, Fallback, Metrics, Style};
pub use image::{Source, variant_name};
pub use policy::{Format, VariantPolicy};

#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum Error {
  #[error("reading {0}: {1}")]
  Io(std::path::PathBuf, std::io::Error),
  #[error("{0} is not an image this build decodes: {1}")]
  Decode(std::path::PathBuf, String),
  #[error("encoding {0}: {1}")]
  Encode(&'static str, String),
  #[error("{0} is not a font this build reads: {1}")]
  Font(std::path::PathBuf, String),
}
