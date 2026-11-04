//! Card images on a host built without the `share_cards` feature: an application whose plan draws them is refused at boot, naming the feature.

use std::path::Path;

use http::{Response, StatusCode};
use snapfire_fsr_core::Params;
use snapfire_fsr_runtime::Locale;

use crate::assets::AssetsManifest;
use crate::edge::text_response;
use crate::{Body, Host, Tables};

/// Never made: `new` refuses.
pub(crate) struct Cards(std::convert::Infallible);

impl Cards {
  pub(crate) fn new(_app: &Path, _out: Option<&Path>, _manifest: Option<&AssetsManifest>, _scripts: &dyn Fn(&str) -> Vec<String>) -> Result<Cards, String> {
    Err("this application draws card images and the host is built without the `share_cards` feature of snapfire_fsr_host".to_owned())
  }

  pub(crate) fn with_sites(self, _apps: &[(String, std::path::PathBuf)]) -> Cards {
    self
  }
}

impl Host {
  pub(crate) async fn card_image(&self, _t: &Tables, _entry: &snapfire_fsr_plan::ImageEntry, _params: Params, _path: &str, _incoming: crate::edge::Incoming, _locale: &Locale) -> Response<Body> {
    text_response(StatusCode::NOT_FOUND, "card images are not drawn by this host".to_owned())
  }

  pub(crate) async fn prerender_images(&self, _t: &Tables, _path: &str, _dir: &Path, _locale: &Locale, _written: &mut Vec<(String, std::path::PathBuf)>, _held: &mut Vec<std::path::PathBuf>) -> Result<(), crate::HostError> {
    Ok(())
  }
}
