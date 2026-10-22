//! `Intl` formatting on the server: a locale written in the code, a currency
//! with its symbol, a date by its parts and a formatter kept as a value.

use std::path::PathBuf;
use std::sync::Arc;

use snapfire_fsr_core::ValueMap;
use snapfire_fsr_ir::render::{prepare, Components};
use snapfire_fsr_ir::Interpreter;
use snapfire_fsr_lower::component::ComponentSet;

fn app(tag: &str, files: &[(&str, &str)]) -> PathBuf {
  let dir = std::env::temp_dir().join(format!("fsr_formatting_{}_{tag}", std::process::id()));
  let _ = std::fs::remove_dir_all(&dir);
  for (name, text) in files {
    let path = dir.join(name);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, text).unwrap();
  }
  dir
}

fn html(tag: &str, files: &[(&str, &str)]) -> Result<String, String> {
  let dir = app(tag, files);
  let mut set = ComponentSet::new(&dir);
  let result = set.lower("routes/page.tsx#default").map_err(|e| e.to_string());
  std::fs::remove_dir_all(&dir).unwrap();
  result?;
  set.settle_fills();
  if let Some((module, residue)) = set.browser_only.first() {
    return Err(format!("{module}: {}", residue.message));
  }
  let library: Components = set.components.iter().map(|(module, component)| (module.clone(), Arc::new(prepare(component)))).collect();
  let module = "routes/page.tsx#default";
  Interpreter::default().with_consts(Some(Arc::new(set.consts.clone()))).render_module(module, &library[module], &ValueMap::default(), &library).map(|r| r.html).map_err(|e| format!("{e:?}"))
}

fn page(body: &str) -> Result<String, String> {
  let tag = format!("{:x}", body.len() ^ body.bytes().map(|b| b as usize).sum::<usize>());
  html(&tag, &[("routes/page.tsx", &format!("export default function Page() {{\n  const when = new Date(\"2025-06-15T12:00:00Z\");\n  return <p>{body}</p>;\n}}\n"))])
}

#[test]
fn a_locale_written_in_the_code_formats_under_it() {
  assert_eq!(page("{new Intl.NumberFormat(\"de-DE\").format(1234.5)}").as_deref(), Ok("<p>1.234,5</p>"));
  assert_eq!(page("{new Intl.NumberFormat(\"en-US\", { maximumFractionDigits: 1 }).format(1234.56)}").as_deref(), Ok("<p>1,234.6</p>"));
  assert_eq!(page("{new Intl.DateTimeFormat(\"en-US\", { dateStyle: \"medium\", timeZone: \"UTC\" }).format(when)}").as_deref(), Ok("<p>Jun 15, 2025</p>"));
  assert_eq!(page("{when.toLocaleDateString(\"de-DE\", { dateStyle: \"long\" })}").as_deref(), Ok("<p>15. Juni 2025</p>"));
}

#[test]
fn a_locale_read_at_render_time_formats_under_it() {
  let page = "function List({ locale, items }: { locale: string; items: string[] }) {\n  return <p>{new Intl.ListFormat(locale, { type: \"conjunction\" }).format(items)} {new Intl.NumberFormat(locale).format(1234.5)}</p>;\n}\nexport default function Page() {\n  return <main><List locale=\"fr-FR\" items={[\"a\", \"b\", \"c\"]} /><List locale=\"en-US\" items={[\"a\", \"b\", \"c\"]} /></main>;\n}\n";
  assert_eq!(html("runtime_locale", &[("routes/page.tsx", page)]).as_deref(), Ok("<main><p>a, b et c 1\u{202f}234,5</p><p>a, b, and c 1,234.5</p></main>"));
}

#[test]
fn to_locale_string_reads_its_options_as_number_format_does() {
  assert_eq!(page("{(100000).toLocaleString(\"en-US\", { minimumFractionDigits: 2, maximumFractionDigits: 2 })}").as_deref(), Ok("<p>100,000.00</p>"));
  assert_eq!(page("{(1234.5678).toLocaleString(\"en-US\", { maximumFractionDigits: 1 })}").as_deref(), Ok("<p>1,234.6</p>"));
  assert_eq!(page("{(9999).toLocaleString(\"en-US\", { style: \"currency\", currency: \"USD\" })}").as_deref(), Ok("<p>$9,999.00</p>"));
  assert_eq!(page("{(1234567).toLocaleString()}").as_deref(), Ok("<p>1,234,567</p>"));
  assert_eq!(page("{(1234567).toLocaleString(\"en-US\")}").as_deref(), Ok("<p>1,234,567</p>"));
  let out = page("{(0.5).toLocaleString(\"en-US\", { style: \"percent\" })}");
  assert!(out.as_ref().is_err_and(|e| e.contains("`style`")), "{out:?}");
}

#[test]
fn a_currency_writes_its_symbol_unless_the_code_is_asked_for() {
  assert_eq!(page("{new Intl.NumberFormat(\"en-US\", { style: \"currency\", currency: \"USD\", minimumFractionDigits: 0 }).format(9999)}").as_deref(), Ok("<p>$9,999</p>"));
  assert_eq!(page("{new Intl.NumberFormat(\"en-US\", { style: \"currency\", currency: \"USD\" }).format(1234.5)}").as_deref(), Ok("<p>$1,234.50</p>"));
  assert_eq!(page("{new Intl.NumberFormat(\"de-DE\", { style: \"currency\", currency: \"EUR\" }).format(1234.5)}").as_deref(), Ok("<p>1.234,50\u{a0}€</p>"));
  assert_eq!(page("{new Intl.NumberFormat(\"en-US\", { style: \"currency\", currency: \"USD\", currencyDisplay: \"code\" }).format(1234.5)}").as_deref(), Ok("<p>USD\u{a0}1,234.50</p>"));
  assert_eq!(page("{new Intl.NumberFormat(\"en-US\", { style: \"currency\", currency: \"CAD\", currencyDisplay: \"narrowSymbol\" }).format(5)}").as_deref(), Ok("<p>$5.00</p>"));
  assert_eq!(page("{new Intl.NumberFormat(\"en-US\", { style: \"currency\", currency: \"CAD\" }).format(5)}").as_deref(), Ok("<p>CA$5.00</p>"));
}

#[test]
fn a_date_formats_by_its_parts() {
  assert_eq!(page("{new Intl.DateTimeFormat(\"en-US\", { weekday: \"long\", timeZone: \"UTC\" }).format(when)}").as_deref(), Ok("<p>Sunday</p>"));
  assert_eq!(page("{new Intl.DateTimeFormat(\"en-US\", { weekday: \"short\" }).format(when)}").as_deref(), Ok("<p>Sun</p>"));
  assert_eq!(page("{new Intl.DateTimeFormat(\"en-US\", { month: \"long\", day: \"numeric\" }).format(when)}").as_deref(), Ok("<p>June 15</p>"));
  assert_eq!(page("{new Intl.DateTimeFormat(\"en-US\", { year: \"numeric\", month: \"short\", day: \"numeric\" }).format(when)}").as_deref(), Ok("<p>Jun 15, 2025</p>"));
  assert_eq!(page("{new Intl.DateTimeFormat(\"en-US\", { weekday: \"long\", year: \"numeric\", month: \"long\", day: \"numeric\" }).format(when)}").as_deref(), Ok("<p>Sunday, June 15, 2025</p>"));
  assert_eq!(page("{new Intl.DateTimeFormat(\"de-DE\", { weekday: \"long\" }).format(when)}").as_deref(), Ok("<p>Sonntag</p>"));
  assert_eq!(page("{new Intl.DateTimeFormat(\"en-US\", { year: \"numeric\", month: \"long\" }).format(when)}").as_deref(), Ok("<p>June 2025</p>"));
  let out = page("{new Intl.DateTimeFormat(\"en-US\", { year: \"numeric\", month: \"numeric\", day: \"numeric\" }).format(when)}");
  assert!(out.as_ref().is_err_and(|e| e.contains("a numeric month")), "{out:?}");
  let out = page("{new Intl.DateTimeFormat(\"en-US\", { hour: \"numeric\" }).format(when)}");
  assert!(out.as_ref().is_err_and(|e| e.contains("the date formatting option `hour`")), "{out:?}");
}

#[test]
fn a_formatter_kept_as_a_value_formats_as_the_inline_form_would() {
  let total = "const formatter = new Intl.NumberFormat('en-US', {\n  style: 'currency',\n  currency: 'USD',\n  minimumFractionDigits: 0,\n});\n\nexport default function Total({quantity}) {\n  return (\n    <div className=\"row total\">\n      <span>Total</span>\n      <span>{formatter.format(quantity * 9999)}</span>\n    </div>\n  );\n}\n";
  let out = html("module_level", &[("src/Total.js", total), ("routes/page.tsx", "import Total from \"@src/Total\";\nexport default function Page() {\n  return <Total quantity={2} />;\n}\n")]);
  assert_eq!(out.as_deref(), Ok("<div class=\"row total\"><span>Total</span><span>$19,998</span></div>"));
  let local = "export default function Page() {\n  const money = new Intl.NumberFormat(\"de-DE\", { style: \"currency\", currency: \"EUR\" });\n  const day = new Intl.DateTimeFormat(\"en-US\", { weekday: \"long\" });\n  return <p>{money.format(12.5)} {day.format(new Date(\"2025-06-15T12:00:00Z\"))}</p>;\n}\n";
  assert_eq!(html("local", &[("routes/page.tsx", local)]).as_deref(), Ok("<p>12,50\u{a0}€ Sunday</p>"));
  let shared = "export const money = new Intl.NumberFormat(\"en-US\", { style: \"currency\", currency: \"USD\" });\n";
  let out = html("imported", &[("src/money.ts", shared), ("routes/page.tsx", "import { money } from \"@src/money\";\nexport default function Page() {\n  return <p>{money.format(3)}</p>;\n}\n")]);
  assert_eq!(out.as_deref(), Ok("<p>$3.00</p>"));
}
