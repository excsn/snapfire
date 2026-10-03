//! What FSR derives from an image or a font. `fsr build` calls these at build
//! time; a host answering images at request time calls the same functions.
//! What a file says about itself, a header's size and orientation or a face's
//! metrics, is read through `snapfire_media`, which snapfirec reads through too.

pub mod hash;
pub mod image;
pub mod policy;

/// The face reader and the fallback table, `snapfire_media::font` as it is.
pub mod font {
  pub use snapfire_media::font::*;
}

pub use snapfire_media::{Face, Fallback, Header, Metrics, Style};
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
  #[error(transparent)]
  Media(#[from] snapfire_media::Error),
}
