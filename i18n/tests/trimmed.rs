//! Data trimmed to a list of locales formats those locales as the whole data does and holds no other locale.

use snapfire_i18n::{Data, Display, Error, NameKind, Plural, Sign, Unit, Width, ZoneRules};

struct Eastern;

impl ZoneRules for Eastern {
  fn observed(&self, _: &str, _: f64) -> Option<(i32, i32)> {
    Some((-14_400, 3_600))
  }

  fn saving_in_year(&self, _: &str, _: f64) -> Option<i32> {
    Some(3_600)
  }
}

const NOW: f64 = 1_790_000_000_000.0;

fn trimmed(locales: &[&str]) -> (Data, usize) {
  let bytes = Data::embedded().trimmed(locales);
  let len = bytes.len();
  (Data::from_static(Box::leak(bytes.into_boxed_slice())).unwrap(), len)
}

/// Everything the crate writes for `tag`, one line each.
fn written(data: &Data, tag: &str) -> Vec<String> {
  let mut out = vec![data.cldr_version().to_owned()];
  for code in ["USD", "EUR", "JPY", "GBP", "CAD", "BHD", "XXX"] {
    for display in [Display::Code, Display::Symbol, Display::NarrowSymbol] {
      let currency = data.currency(tag, code, display).unwrap();
      out.push(format!("{} {} {}", currency.format("1234.5", Sign::Minus), currency.shown(), currency.digits()));
    }
  }
  out.push(data.percent(tag).format("12", Sign::Plus));
  for unit in [Unit::Second, Unit::Day, Unit::Quarter, Unit::Year] {
    for width in [Width::Long, Width::Short, Width::Narrow] {
      let relative = data.relative(tag, unit, width);
      out.push(format!("{} {} {:?} {:?}", relative.format("3", false, Plural::Other, None), relative.format("1", true, Plural::One, Some(1)), relative.literal(-1), relative.literal(0)));
    }
  }
  for (kind, code) in [(NameKind::Region, "US"), (NameKind::Region, "419"), (NameKind::Language, "en-GB"), (NameKind::Language, "zh-Hant-TW"), (NameKind::Script, "Cyrl"), (NameKind::Currency, "JPY"), (NameKind::Calendar, "gregory"), (NameKind::DateTimeField, "weekOfYear")] {
    for style in ["long", "short", "narrow"] {
      out.push(format!("{:?}", data.display_name(tag, kind, code, style, false)));
      out.push(format!("{:?}", data.display_name(tag, kind, code, style, true)));
    }
  }
  for zone in ["America/New_York", "Europe/Paris", "Asia/Kolkata", "Etc/Unknown", "No/Such_Zone", "US/Eastern"] {
    out.push(data.zone_name(tag, zone, true, NOW, &Eastern));
    out.push(data.zone_name(tag, zone, false, NOW, &Eastern));
    out.push(data.short_zone_name(tag, zone, NOW, &Eastern));
    out.push(data.in_daylight(zone, NOW, &Eastern).to_string());
  }
  out.push(format!("{:?} {:?}", data.numeric_date(tag, 2024, 9, 10), data.numeric_date_parts(tag, -5, 1, 2)));
  out.push(data.clock(tag, 19, 37, 20));
  out.push(data.date_with_time(tag, "D", "T"));
  for width in ["long", "short", "narrow"] {
    out.push(format!("{:?} {:?}", data.duration_unit(tag, width, "hour", "one"), data.duration_unit(tag, width, "day", "few")));
  }
  out.push(format!("{} {} {} {} {:?}", data.time_separator(tag), data.supported(tag), data.numbering_system(tag), data.hour_cycle(tag), data.calendar(tag)));
  out.push(format!("{:?} {:?}", data.time_widths(tag, "h12", true, true), data.time_widths(tag, "h23", true, false)));
  out.push(format!("{:?} {:?}", data.preferred_calendars("TH"), data.region_zones("AU")));
  out
}

const LISTED: [&str; 6] = ["en-GB", "fr-CA", "zh-Hant", "ar-EG", "fa", "th"];

#[test]
fn trimmed_data_writes_each_listed_locale_as_the_whole_data_does() {
  let (data, _) = trimmed(&LISTED);
  for tag in LISTED.into_iter().chain(["zh-TW", "fr-CA-u-ca-buddhist", "en-GB-u-hc-h23"]) {
    assert_eq!(written(&data, tag), written(&Data::embedded(), tag), "{tag}");
  }
}

#[test]
fn trimmed_data_writes_a_listed_locales_parents_as_the_whole_data_does() {
  let (data, _) = trimmed(&["en-GB", "fr-CA"]);
  for tag in ["en", "fr", "en-001"] {
    assert_eq!(written(&data, tag), written(&Data::embedded(), tag), "{tag}");
  }
}

#[test]
fn trimmed_data_holds_no_locale_outside_the_list_and_its_parents() {
  let (data, len) = trimmed(&["en-GB"]);
  assert!(data.supported("en-GB"));
  assert!(data.supported("en"));
  for tag in ["de", "fr", "ja"] {
    assert_eq!(written(&data, tag), written(&data, "und"), "{tag}");
  }
  assert!(!data.supported("de"));
  assert_eq!(data.display_name("de", NameKind::Region, "US", "long", false), None);
  let whole = Data::embedded().trimmed(&["en-GB", "de", "fr", "ja", "es", "pt", "ru", "zh", "ar", "hi"]).len();
  assert!(len * 3 < whole, "one locale is {len} bytes and ten are {whole}");
}

#[test]
fn trimmed_data_trims_again_to_the_same_bytes() {
  let (data, _) = trimmed(&["en-GB", "fr-CA"]);
  assert_eq!(data.trimmed(&["en-GB", "fr-CA"]), Data::embedded().trimmed(&["en-GB", "fr-CA"]));
  assert_eq!(data.trimmed(&["fr-CA"]), Data::embedded().trimmed(&["fr-CA"]));
}

#[test]
fn bytes_that_are_not_data_are_refused() {
  assert_eq!(Data::from_static(b"").unwrap_err(), Error::NotData);
  assert_eq!(Data::from_static(b"not a table at all").unwrap_err(), Error::NotData);
  let cut: &'static [u8] = Box::leak(Data::embedded().trimmed(&["en"]).into_boxed_slice());
  assert_eq!(Data::from_static(&cut[..cut.len() - 1]).unwrap_err(), Error::NotData);
}
