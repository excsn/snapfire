//! The locale data a build writes for the host: ICU4X's and snapfire_i18n's, cut to the locales `[locales]` lists.

extern crate alloc;

use std::path::Path;

use icu::locale::fallback::LocaleFallbacker;
use icu_provider::buf::DeserializingBufferProvider;
use icu_provider::export::ExportableProvider;
use icu_provider::prelude::*;
use icu_provider_blob::BlobDataProvider;
use icu_provider_export::blob_exporter::BlobExporter;
use icu_provider_export::prelude::*;
use snapfire_fsr_host::Config;
use snapfire_fsr_ir::intl_data::{Cut, IntlData, COMPILED_ICU};

use crate::BuildError;

pub use snapfire_fsr_host::locale::DATA_FILE;

/// The dictionaries ICU4X splits words by. The LSTM models are left out, since the host splits by dictionary.
const DICTIONARIES: [&str; 5] = ["cjdict", "burmesedict", "khmerdict", "laodict", "thaidict"];

/// An ICU4X blob as what an export reads from.
struct Source(BlobDataProvider);

impl<M: DataMarker> DataProvider<M> for Source
where
  for<'a> DeserializingBufferProvider<'a, BlobDataProvider>: DataProvider<M>,
{
  fn load(&self, req: DataRequest) -> Result<DataResponse<M>, DataError> {
    self.0.as_deserializing().load(req)
  }
}

impl<M: DataMarker> IterableDataProvider<M> for Source
where
  for<'a> DeserializingBufferProvider<'a, BlobDataProvider>: DataProvider<M>,
{
  fn iter_ids(&self) -> Result<std::collections::BTreeSet<DataIdentifierCow<'_>>, DataError> {
    self.0.iter_ids_for_marker(M::INFO)
  }
}

macro_rules! cb {
  ($($marker_ty:ty:$marker:ident,)+ #[unstable] $($emarker_ty:ty:$emarker:ident,)+) => {
    icu_provider::export::make_exportable_provider!(Source, [ $( $marker_ty, )+ ]);
  }
}
icu_provider_registry::registry!(cb);

/// The locales the plan names as literals that `cut` does not cover, each once.
pub fn unlisted(cut: &Cut, plan: &str) -> Vec<String> {
  let Ok(locales) = snapfire_fsr_ir::locales::Locales::new(&cut.locales, &cut.default, snapfire_fsr_ir::locales::Unlisted::Error) else { return Vec::new() };
  snapfire_fsr_ir::intl_data::named_locales(plan).into_iter().filter(|tag| locales.resolve(tag) == Err(snapfire_fsr_ir::locales::Refused::Unlisted)).collect()
}

/// What the application at `app` formats in, whose plan is `plan`: the cut its `[locales]` and the plan call for, out of the data built into this program. An application whose configuration does not load is cut as one with no `[locales]`.
pub fn cut_of(app: &Path, plan: &str) -> Cut {
  let config = Config::load(&crate::serve::project_root(app)).ok();
  snapfire_fsr_host::locale::cut(config.as_ref().and_then(|config| config.locales.as_ref()), plan).source(snapfire_fsr_ir::intl_data::compiled_source())
}

/// The ICU4X blob holding `cut`'s locales and the locales they fall back to, out of every locale's.
fn icu(cut: &Cut) -> Result<Vec<u8>, DataError> {
  let blob = BlobDataProvider::try_new_from_static_blob(COMPILED_ICU)?;
  let fallbacker = LocaleFallbacker::try_new_with_buffer_provider(&blob)?;
  let source = Source(blob);
  let holds = |marker: &DataMarkerInfo| IterableDynamicDataProvider::<icu_provider::export::ExportMarker>::iter_ids_for_marker(&source, *marker).is_ok_and(|ids| !ids.is_empty());
  let held: Vec<DataMarkerInfo> = source.supported_markers().into_iter().filter(holds).collect();
  let mut families = Vec::new();
  for tag in &cut.locales {
    let locale: icu::locale::Locale = tag.parse().map_err(|_| DataError::custom("a listed locale is not a BCP 47 tag"))?;
    families.push(DataLocaleFamily::without_descendants((&locale).into()));
  }
  let models: Vec<String> = if cut.words { DICTIONARIES.iter().map(|name| (*name).to_owned()).collect() } else { Vec::new() };
  let mut out = Vec::new();
  ExportDriver::new(families, DeduplicationStrategy::Maximal.into(), fallbacker).with_markers(held).with_segmenter_models(models).export(&source, BlobExporter::new_with_sink(Box::new(&mut out)))?;
  Ok(out)
}

/// The bytes of [`DATA_FILE`] for `cut`. `written` is what the last build left there, kept as it is when it was cut for the same.
pub fn data(cut: &Cut, written: Option<Vec<u8>>) -> Result<Vec<u8>, BuildError> {
  if let Some(written) = written.filter(|written| IntlData::cut_of(written).as_ref() == Ok(cut)) {
    return Ok(written);
  }
  let named = || cut.locales.join(", ");
  snapfire_fsr_ir::locales::Locales::new(&cut.locales, &cut.default, cut.unlisted).map_err(|e| BuildError::Configuration(format!("[locales]: {e}")))?;
  let icu = icu(cut).map_err(|e| BuildError::Configuration(format!("[locales]: no ICU4X data is cut for {}: {e}", named())))?;
  let listed: Vec<&str> = cut.locales.iter().map(String::as_str).collect();
  let tables = snapfire_i18n::Data::embedded().trimmed(&listed);
  Ok(IntlData::packed(&icu, &tables, cut))
}
