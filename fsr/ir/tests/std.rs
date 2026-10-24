use std::sync::Arc;

use snapfire_fsr_core::Value;
use snapfire_fsr_ir::{standard_reach, Ambient, Expr, Extensions, Interpreter, Reach, STANDARD};

fn call(locale: &str, name: &str, args: Vec<Value>) -> Result<Value, String> {
  let (module, member) = name.split_once('.').unwrap();
  Extensions::standard().call(name, &Ambient::default().locale(locale.to_owned()).now(1_757_090_467_250).catalogs(None), &args).map(|v| {
    let _ = (module, member);
    v
  }).map_err(|e| e.message)
}

fn s(text: &str) -> Value {
  Value::str(text.to_owned())
}

fn n(f: f64) -> Value {
  Value::F64(f)
}

#[test]
fn the_standard_table_and_the_registry_agree() {
  let registry = Extensions::standard();
  for member in STANDARD {
    let key = format!("{}.{}", member.module, member.name);
    let held = registry.get(&key).unwrap_or_else(|| panic!("{key} is registered"));
    assert_eq!(held.reach, member.reach, "{key}");
    assert_eq!(standard_reach(member.module, member.name), Some(member.reach));
  }
  assert_eq!(registry.names().len(), STANDARD.len());
  assert_eq!(standard_reach("intl", "money"), None);
  assert_eq!(Reach::Body.as_str(), "body");
}

#[test]
fn numbers_group_and_round_under_the_locale() {
  assert_eq!(call("en_US", "intl.number", vec![n(1234567.891)]), Ok(s("1,234,567.891")));
  assert_eq!(call("fr_FR", "intl.number", vec![n(1234567.891)]), Ok(s("1\u{202f}234\u{202f}567,891")));
  assert_eq!(call("de_DE", "intl.number", vec![n(1234567.891)]), Ok(s("1.234.567,891")));
  assert_eq!(call("en_US", "intl.number", vec![n(1.23456)]), Ok(s("1.235")));
  assert_eq!(call("en_US", "intl.number", vec![n(5.0)]), Ok(s("5")));
  assert_eq!(call("en_US", "intl.number", vec![n(-0.5)]), Ok(s("-0.5")));
  assert_eq!(call("en_US", "intl.number", vec![Value::Int(12345678901234567890)]), Ok(s("12,345,678,901,234,567,890")));
  assert_eq!(call("", "intl.number", vec![n(1000.0)]), Ok(s("1,000")), "no locale is `en`");
  let options = |min: f64, max: f64| {
    let mut map = snapfire_fsr_core::ValueMap::default();
    map.insert("minimumFractionDigits".to_owned(), n(min));
    map.insert("maximumFractionDigits".to_owned(), n(max));
    Value::Map(map)
  };
  assert_eq!(call("en_US", "intl.number", vec![n(2.0), options(2.0, 2.0)]), Ok(s("2.00")));
  assert_eq!(call("en_US", "intl.number", vec![n(2.345), options(0.0, 2.0)]), Ok(s("2.35")));
  assert_eq!(call("en_US", "intl.number", vec![n(f64::NAN)]), Ok(s("NaN")));
  assert_eq!(call("en_US", "intl.number", vec![n(f64::INFINITY)]), Ok(s("∞")));
  assert!(call("en_US", "intl.number", vec![s("x")]).unwrap_err().contains("a number"));
}

#[test]
fn currency_spells_the_code_with_its_own_digits() {
  assert_eq!(call("en_US", "intl.currency", vec![n(1234.5), s("USD")]), Ok(s("USD\u{a0}1,234.50")));
  assert_eq!(call("de_DE", "intl.currency", vec![n(1234.5), s("EUR")]), Ok(s("1.234,50\u{a0}EUR")));
  assert_eq!(call("ja_JP", "intl.currency", vec![n(1234.5), s("JPY")]), Ok(s("JPY\u{a0}1,235")));
  assert!(call("en_US", "intl.currency", vec![n(1.0), s("dollars")]).unwrap_err().contains("not a currency code"));
}

#[test]
fn dates_take_a_style_in_utc() {
  let day = 20_701.0 * 86_400_000.0 + 82_800_000.0;
  assert_eq!(call("en_US", "intl.date", vec![n(day)]), Ok(s("Sep 5, 2026")));
  assert_eq!(call("en_US", "intl.date", vec![n(day), s("short")]), Ok(s("9/5/26")));
  assert_eq!(call("fr_FR", "intl.date", vec![n(day), s("long")]), Ok(s("5 septembre 2026")));
  assert_eq!(call("de_DE", "intl.date", vec![n(day), s("full")]), Ok(s("Samstag, 5. September 2026")));
  assert_eq!(call("en_US", "intl.date", vec![s("2026-09-05T23:00:00Z"), s("medium")]), Ok(s("Sep 5, 2026")));
  assert!(call("en_US", "intl.date", vec![n(day), s("tiny")]).unwrap_err().contains("not a style"));
  assert!(call("en_US", "intl.date", vec![s("Sep 5")]).unwrap_err().contains("ISO 8601"));
}

#[test]
fn plural_categories_follow_the_locale() {
  assert_eq!(call("en_US", "intl.plural", vec![n(1.0)]), Ok(s("one")));
  assert_eq!(call("en_US", "intl.plural", vec![n(2.0)]), Ok(s("other")));
  assert_eq!(call("fr_FR", "intl.plural", vec![n(0.0)]), Ok(s("one")));
  assert_eq!(call("ar_EG", "intl.plural", vec![n(2.0)]), Ok(s("two")));
  assert_eq!(call("ar_EG", "intl.plural", vec![Value::Int(21)]), Ok(s("many")));
}

#[test]
fn lists_join_as_the_locale_joins_them() {
  let items = |xs: &[&str]| Value::seq(xs.iter().map(|x| s(x)).collect::<Vec<_>>());
  let options = |pairs: &[(&str, &str)]| Value::Map(pairs.iter().map(|(k, v)| ((*k).to_owned(), s(v))).collect());
  assert_eq!(call("en_GB", "intl.list", vec![items(&["a", "b", "c"])]), Ok(s("a, b and c")));
  assert_eq!(call("en_US", "intl.list", vec![items(&["a", "b"])]), Ok(s("a and b")));
  assert_eq!(call("en_US", "intl.list", vec![items(&["a", "b"]), options(&[("type", "disjunction")])]), Ok(s("a or b")));
  assert_eq!(call("en_US", "intl.list", vec![items(&["a"])]), Ok(s("a")));
  assert_eq!(call("en_US", "intl.list", vec![items(&[])]), Ok(s("")));
  assert_eq!(call("en_US", "intl.list", vec![items(&["3 ft", "7 in"]), options(&[("type", "unit"), ("style", "narrow")])]), Ok(s("3 ft 7 in")));
  assert_eq!(call("fr_FR", "intl.list", vec![items(&["a", "b", "c"])]), Ok(s("a, b et c")));
  assert_eq!(call("fr_FR", "intl.list", vec![items(&["a", "b", "c"]), options(&[("locale", "en-GB")])]), Ok(s("a, b and c")));
  assert!(call("en_US", "intl.list", vec![Value::seq(vec![n(1.0)])]).unwrap_err().contains("string"));
  assert!(call("en_US", "intl.list", vec![items(&["a"]), options(&[("type", "both")])]).unwrap_err().contains("type"));
}

#[test]
fn text_slugs_and_truncates_by_code_point() {
  assert_eq!(call("", "text.slug", vec![s("  Crème Brûlée & Café! ")]), Ok(s("creme-brulee-cafe")));
  assert_eq!(call("", "text.slug", vec![s("Ünïcödé--stuff__2026")]), Ok(s("unicode-stuff-2026")));
  assert_eq!(call("", "text.truncate", vec![s("héllo wörld"), n(5.0)]), Ok(s("héllo…")));
  assert_eq!(call("", "text.truncate", vec![s("hi"), n(5.0)]), Ok(s("hi")));
  assert_eq!(call("", "text.truncate", vec![s("hello"), n(3.0), s("...")]), Ok(s("hel...")));
}

#[test]
fn time_is_utc_milliseconds() {
  let ms = 20_701.0 * 86_400_000.0 + 60_067_250.0;
  assert_eq!(call("", "time.format", vec![n(ms), s("YYYY-MM-DD HH:mm:ss")]), Ok(s("2026-09-05 16:41:07")));
  assert_eq!(call("", "time.add", vec![n(ms), n(2.0), s("d")]), Ok(n(ms + 172_800_000.0)));
  assert_eq!(call("", "time.diff", vec![n(ms + 90_000.0), n(ms), s("m")]), Ok(n(1.5)));
  assert_eq!(call("", "time.parse", vec![s("2026-09-05T16:41:07.250Z")]), Ok(n(ms)));
  assert_eq!(call("", "time.parse", vec![s("yesterday")]), Ok(Value::Null));
  assert_eq!(call("", "time.now", vec![]), Ok(Value::Int(1_757_090_467_250)));
  assert!(call("", "time.add", vec![n(ms), n(1.0), s("weeks")]).unwrap_err().contains("not a unit"));
}

#[test]
fn crypto_hashes_verifies_and_draws() {
  let hash = "2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824";
  assert_eq!(call("", "crypto.hash", vec![s("hello")]), Ok(s(hash)));
  assert_eq!(call("", "crypto.verify", vec![s("hello"), s(&hash.to_uppercase())]), Ok(Value::Bool(true)));
  assert_eq!(call("", "crypto.verify", vec![s("hello!"), s(hash)]), Ok(Value::Bool(false)));
  let Ok(Value::Str(random)) = call("", "crypto.random", vec![n(8.0)]) else { panic!() };
  assert_eq!(random.len(), 16);
  assert_ne!(call("", "crypto.random", vec![n(8.0)]), Ok(s(&random)));
}

#[test]
fn ids_are_uuid_v7() {
  let Ok(Value::Str(id)) = call("", "id.new", vec![]) else { panic!() };
  assert_eq!(id.len(), 36);
  assert_eq!(&id[14..15], "7");
  assert_ne!(call("", "id.new", vec![]), Ok(s(&id)));
}

#[tokio::test]
async fn an_ext_expression_runs_through_the_interpreter_under_its_locale() {
  let interpreter = Interpreter::default();
  let expr = Expr::ext("intl", "number", vec![Expr::Lit(snapfire_fsr_ir::Lit::Float(1234.5))]);
  assert_eq!(interpreter.evaluate(&expr, Vec::new()).await.unwrap(), s("1,234.5"), "a detached evaluation has no locale, which is `en`");

  let missing = Expr::ext("fmt", "pretty", vec![]);
  let err = interpreter.evaluate(&missing, Vec::new()).await.unwrap_err();
  assert!(err.message.contains("extension `fmt.pretty` is not registered"), "{}", err.message);

  let mut extensions = Extensions::standard();
  extensions.register("fmt.pretty", Reach::Render, |ambient, args| Ok(Value::str(format!("{}:{}", ambient.bcp47(), args.len()))));
  let interpreter = Interpreter::default().with_extensions(Arc::new(extensions));
  assert_eq!(interpreter.evaluate(&missing, Vec::new()).await.unwrap(), s("en:0"));
  assert!(interpreter.extensions().contains("fmt.pretty"));
}

#[test]
fn t_reads_the_catalog_under_the_locale_with_plurals_and_placeholders() {
  use std::collections::BTreeMap;
  let table = |pairs: &[(&str, &str)]| pairs.iter().map(|(k, v)| ((*k).to_owned(), (*v).to_owned())).collect::<BTreeMap<_, _>>();
  let mut tables = BTreeMap::new();
  tables.insert("en_US".to_owned(), table(&[("hi", "Hello {name}"), ("items.one", "{count} item"), ("items.other", "{count} items"), ("only.en", "English only")]));
  tables.insert("fr_FR".to_owned(), table(&[("hi", "Bonjour {name}"), ("items.one", "{count} article"), ("items.other", "{count} articles")]));
  let catalogs = Arc::new(snapfire_fsr_ir::Catalogs::from_tables("en_US", tables));
  assert_eq!(catalogs.lookup("fr_FR", "only.en"), Some("English only"), "a locale reads the default's keys under its own");
  assert_eq!(catalogs.lookup("de_DE", "hi"), Some("Hello {name}"), "an unknown locale reads the default");
  assert!(catalogs.json("fr_FR").unwrap().contains("\"only.en\":\"English only\""), "the table shipped is the merged one");
  assert_eq!(catalogs.rows(), vec![("en_US".to_owned(), 4), ("fr_FR".to_owned(), 3)]);

  let registry = Extensions::standard();
  let call = |locale: &str, args: Vec<Value>| registry.call("i18n.t", &Ambient::default().locale(locale.to_owned()).now(0).catalogs(Some(catalogs.clone())), &args).map_err(|e| e.message);
  let named = |pairs: Vec<(&str, Value)>| {
    let mut map = snapfire_fsr_core::ValueMap::default();
    for (k, v) in pairs {
      map.insert(k.to_owned(), v);
    }
    Value::Map(map)
  };
  assert_eq!(call("fr_FR", vec![s("hi"), named(vec![("name", s("Norm"))])]), Ok(s("Bonjour Norm")));
  assert_eq!(call("en_US", vec![s("hi")]), Ok(s("Hello {name}")), "an unfilled placeholder stays");
  assert_eq!(call("en_US", vec![s("items"), named(vec![("count", n(1.0))])]), Ok(s("1 item")));
  assert_eq!(call("fr_FR", vec![s("items"), named(vec![("count", n(0.0))])]), Ok(s("0 article")), "French counts zero as one");
  assert_eq!(call("en_US", vec![s("items"), named(vec![("count", Value::Int(12))])]), Ok(s("12 items")));
  assert_eq!(call("en_US", vec![s("missing.key")]), Ok(s("missing.key")));
  assert_eq!(registry.call("i18n.t", &Ambient::default(), &[s("hi")]).unwrap(), s("hi"), "no catalogs at all answers the key");
  assert_eq!(call("en_US", vec![s("hi"), named(vec![("name", s("Norm")), ("count", s("5"))])]), Ok(s("Hello Norm")), "a count that is not a number picks no plural form, as the browser's t does");
  assert_eq!(call("en_US", vec![s("items"), named(vec![("count", s("5"))])]), Ok(s("items")), "so only the bare key is looked up");
}

#[test]
fn to_fixed_prints_from_an_integer_exactly_as_the_float_formatter_would() {
  let interpreter = Interpreter::default();
  let runtime = tokio::runtime::Builder::new_current_thread().build().expect("runtime");
  let mut checked = 0;
  for digits in 0usize..=6 {
    for raw in [0.0f64, -0.0, 0.5, -0.5, 1.005, 2.675, 12.345, 0.001, -0.001, 1e15, 1.0 / 3.0, 99999.99999, -1234.5678, 8.0, -8.0, 1e-9] {
      for factor in [1.0f64, 7.0, 1234.0] {
        let value: f64 = raw * factor;
        let unit = 10f64.powi(digits as i32);
        let rounded = (value.abs() * unit + 0.5).floor() / unit;
        let rounded = if value < 0.0 { -rounded } else { rounded };
        let expected = format!("{rounded:.digits$}");
        let expr = Expr::Builtin {
          name: snapfire_fsr_ir::Builtin::ToFixed,
          args: vec![Expr::Lit(snapfire_fsr_ir::Lit::Float(value)), Expr::Lit(snapfire_fsr_ir::Lit::Int(digits as i128))],
        };
        let got = runtime.block_on(interpreter.evaluate(&expr, Vec::new())).expect("toFixed evaluates");
        let Value::Str(got) = got else { panic!("toFixed answers a string") };
        assert_eq!(&*got, expected.as_str(), "toFixed({value}, {digits})");
        checked += 1;
      }
    }
  }
  assert!(checked > 300, "the sweep covered {checked} cases");
}

#[test]
fn local_time_is_the_instant_shifted_by_this_machines_offset_as_to_locale_time_string_writes_it() {
  let ms = 1_726_011_440_123.0;
  let Ok(Value::F64(minutes)) = call("", "time.localOffset", vec![n(ms)]) else { panic!("a number") };
  assert!(minutes.fract() == 0.0 && minutes.abs() <= 14.0 * 60.0, "{minutes}");
  let seconds = ((ms - minutes * 60_000.0) / 1000.0).floor().rem_euclid(86_400.0) as u32;
  let (hours, minute, second) = (seconds / 3600, seconds / 60 % 60, seconds % 60);
  let expected = format!("{}:{minute:02}:{second:02}\u{202f}{}", if hours % 12 == 0 { 12 } else { hours % 12 }, if hours < 12 { "AM" } else { "PM" });
  assert_eq!(call("", "intl.localTime", vec![n(ms), s("en-US")]), Ok(s(&expected)));
  assert_eq!(call("", "intl.localTime", vec![n(f64::NAN), s("en-US")]), Ok(s("Invalid Date")));
  assert!(matches!(call("", "time.localOffset", vec![n(f64::NAN)]), Ok(Value::F64(m)) if m.is_nan()));
}

#[test]
fn strip_ansi_removes_escape_codes_as_ansi_regex_matches_them() {
  assert_eq!(call("", "terminal.stripAnsi", vec![s("\u{1b}[4m\u{1b}[42mcake\u{1b}[49m\u{1b}[24m")]), Ok(s("cake")));
  assert_eq!(call("", "terminal.stripAnsi", vec![s("\u{1b}]8;;https://x.dev\u{7}link\u{1b}]8;;\u{7} plain")]), Ok(s("link plain")), "an OSC hyperlink");
}

#[test]
fn string_width_counts_terminal_columns_as_string_width_does() {
  for (text, columns) in [("abc", 3.0), ("古", 2.0), ("안녕", 4.0), ("\u{1b}[1municorn\u{1b}[22m", 7.0), ("👍🏽", 2.0), ("👨‍👩‍👧", 2.0), ("e\u{301}", 1.0), ("", 0.0)] {
    assert_eq!(call("", "terminal.stringWidth", vec![s(text)]), Ok(n(columns)), "{text:?}");
  }
}

#[test]
fn wrap_ansi_wraps_at_columns_as_wrap_ansi_does() {
  let options = |hard: bool, trim: bool| {
    let mut map = snapfire_fsr_core::ValueMap::default();
    map.insert("hard".to_owned(), Value::Bool(hard));
    map.insert("trim".to_owned(), Value::Bool(trim));
    Value::Map(map)
  };
  let wrap = |text: &str, columns: f64, opts: Option<Value>| {
    let mut args = vec![s(text), n(columns)];
    args.extend(opts);
    call("", "terminal.wrapAnsi", args)
  };
  assert_eq!(wrap("The quick brown fox jumped", 10.0, None), Ok(s("The quick\nbrown fox\njumped")));
  assert_eq!(wrap("unicorns", 3.0, Some(options(true, true))), Ok(s("uni\ncor\nns")));
  assert_eq!(wrap("unicorns", 3.0, None), Ok(s("unicorns")), "soft wrapping leaves a long word whole");
  assert_eq!(wrap("Type:\n> ab ", 4.0, Some(options(true, false))), Ok(s("Type\n:\n> ab\n ")));
  assert_eq!(wrap("   ", 4.0, None), Ok(s("")));
}

#[test]
fn ms_formats_and_parses_as_the_ms_package_does() {
  let long = || {
    let mut map = snapfire_fsr_core::ValueMap::default();
    map.insert("long".to_owned(), Value::Bool(true));
    Value::Map(map)
  };
  for (ms, short, words) in [(500.0, "500ms", "500 ms"), (1000.0, "1s", "1 second"), (1500.0, "2s", "2 seconds"), (60_000.0, "1m", "1 minute"), (-3_600_000.0, "-1h", "-1 hour"), (2.0 * 86_400_000.0, "2d", "2 days")] {
    assert_eq!(call("", "terminal.ms", vec![n(ms)]), Ok(s(short)), "{ms}");
    assert_eq!(call("", "terminal.ms", vec![n(ms), long()]), Ok(s(words)), "{ms}");
  }
  for (text, ms) in [("2 days", 172_800_000.0), ("1h", 3_600_000.0), ("-.5h", -1_800_000.0), ("100", 100.0), ("1y", 31_557_600_000.0)] {
    assert_eq!(call("", "terminal.ms", vec![s(text)]), Ok(n(ms)), "{text}");
  }
  assert_eq!(call("", "terminal.ms", vec![s("soon")]), Ok(Value::Null), "undefined for what it cannot read");
}

fn map(fields: Vec<(&str, Value)>) -> Value {
  let mut map = snapfire_fsr_core::ValueMap::default();
  for (key, value) in fields {
    map.insert(key.to_owned(), value);
  }
  Value::Map(map)
}

#[test]
fn front_matter_reads_as_gray_matter_does() {
  let parsed = |data: Value, content: &str| map(vec![("data", data), ("content", s(content))]);
  let comments = || map(vec![("delimiters", Value::seq(vec![s("<!--"), s("-->")]))]);
  assert_eq!(
    call("", "text.frontMatter", vec![s("---\ntitle: Hi\ntags: [a, b]\ncount: 3\n---\nBody\n")]),
    Ok(parsed(map(vec![("title", s("Hi")), ("tags", Value::seq(vec![s("a"), s("b")])), ("count", Value::Int(3))]), "Body\n"))
  );
  assert_eq!(call("", "text.frontMatter", vec![s("<!--\nname: x\n-->\n\nText"), comments()]), Ok(parsed(map(vec![("name", s("x"))]), "\nText")));
  assert_eq!(call("", "text.frontMatter", vec![s("~~~\nname: y\n~~~\nText"), map(vec![("delimiters", s("~~~"))])]), Ok(parsed(map(vec![("name", s("y"))]), "Text")));
  assert_eq!(call("", "text.frontMatter", vec![s("Just text\n")]), Ok(parsed(map(vec![]), "Just text\n")));
  assert_eq!(call("", "text.frontMatter", vec![s("---\n---\nText")]), Ok(parsed(map(vec![]), "Text")));
  assert_eq!(call("", "text.frontMatter", vec![s("---\ntitle: Hi\n")]), Ok(parsed(map(vec![("title", s("Hi"))]), "")), "an unclosed block runs to the end");
  assert_eq!(call("", "text.frontMatter", vec![s("----\ntitle: Hi\n---\n")]), Ok(parsed(map(vec![]), "----\ntitle: Hi\n---\n")), "a longer run of the delimiter is not front matter");
  assert!(call("", "text.frontMatter", vec![s("---\na: [1, 2\n---\n")]).is_err());
}

#[test]
fn front_matter_writes_as_gray_matter_stringifies() {
  let comments = || map(vec![("delimiters", Value::seq(vec![s("<!--"), s("-->")]))]);
  let data = map(vec![("name", s("x")), ("variables", Value::seq(vec![s("A"), s("B")]))]);
  let written = call("", "text.withFrontMatter", vec![s("Body"), data.clone(), comments()]);
  assert_eq!(written, Ok(s("<!--\nname: x\nvariables:\n  - A\n  - B\n-->\nBody\n")));
  assert_eq!(call("", "text.withFrontMatter", vec![s("Body\n"), map(vec![])]), Ok(s("Body\n")), "no data writes no block");
  assert_eq!(call("", "text.withFrontMatter", vec![s("Body"), map(vec![("title", s("Hi"))])]), Ok(s("---\ntitle: Hi\n---\nBody\n")));
  let Ok(Value::Str(text)) = written else { panic!() };
  let read = call("", "text.frontMatter", vec![s(text.as_str()), comments()]);
  assert_eq!(read, Ok(map(vec![("data", data), ("content", s("Body\n"))])));
}

#[test]
fn a_file_stat_answers_the_kind_and_size_as_fields() {
  let dir = std::env::temp_dir().join(format!("fsr-std-stat-{}", std::process::id()));
  std::fs::create_dir_all(&dir).unwrap();
  let file = dir.join("five.txt");
  std::fs::write(&file, "hello").unwrap();
  for member in ["file.stat"] {
    let Ok(Value::Map(stats)) = call("", member, vec![s(file.to_str().unwrap())]) else { panic!("{member} answers a map") };
    assert_eq!(stats.get("isFile"), Some(&Value::Bool(true)), "{member}");
    assert_eq!(stats.get("isDirectory"), Some(&Value::Bool(false)), "{member}");
    assert_eq!(stats.get("size"), Some(&Value::Int(5)), "{member}");
    assert!(matches!(stats.get("mtimeMs"), Some(Value::F64(ms)) if *ms > 1.0e12), "{stats:?}");
  }
  let Ok(Value::Map(stats)) = call("", "file.stat", vec![s(dir.to_str().unwrap())]) else { panic!("a directory answers a map") };
  assert_eq!(stats.get("isDirectory"), Some(&Value::Bool(true)));
  let missing = call("", "file.stat", vec![s(dir.join("missing").to_str().unwrap())]).unwrap_err();
  assert!(missing.contains("ENOENT"), "{missing}");
}

#[test]
fn the_line_end_is_this_platforms() {
  assert_eq!(call("", "system.lineEnd", vec![]), Ok(s(if cfg!(windows) { "\r\n" } else { "\n" })));
}

#[test]
fn system_reads_the_machine_by_its_own_names() {
  assert_eq!(call("", "system.platform", vec![]), Ok(s(std::env::consts::OS)));
  unsafe { std::env::set_var("FSR_STD_SYSTEM_ENV", "set") };
  assert_eq!(call("", "system.env", vec![s("FSR_STD_SYSTEM_ENV")]), Ok(s("set")));
  assert_eq!(call("", "system.env", vec![s("FSR_STD_SYSTEM_UNSET")]), Ok(Value::Null));
  assert!(matches!(call("", "system.memory", vec![]), Ok(Value::Int(n)) if n > 0 || cfg!(not(any(target_os = "macos", target_os = "linux")))));
  assert!(matches!(call("", "system.cwd", vec![]), Ok(Value::Str(cwd)) if !cwd.is_empty()));
}

#[test]
fn random_bytes_are_hex_or_base64() {
  let Ok(Value::Str(hex)) = call("", "crypto.random", vec![n(6.0)]) else { panic!("hex") };
  assert_eq!(hex.len(), 12);
  let Ok(Value::Str(base64)) = call("", "crypto.random", vec![n(6.0), s("base64")]) else { panic!("base64") };
  assert_eq!(base64.len(), 8);
  assert!(call("", "crypto.random", vec![n(6.0), s("utf8")]).is_err());
}

#[test]
fn every_terminal_member_has_the_signature_its_declaration_is_written_from() {
  for member in STANDARD.iter().filter(|member| member.module == "terminal") {
    assert!(!member.signatures.is_empty(), "terminal.{}", member.name);
  }
  let ms = STANDARD.iter().find(|member| member.module == "terminal" && member.name == "ms").unwrap();
  assert!(ms.signatures[1].declare("ms").contains("export declare function ms(value: number, options?: { long?: boolean; }): string;"), "{}", ms.signatures[1].declare("ms"));
}
