//! A layout's or page's `export const theme` and `theme.ts`, the request's choice of the document's theme.

use snapfire_fsr_ir::sexpr::{print, stmt_to_sx};
use snapfire_fsr_lower::component::{ComponentSet, ThemeExport};

fn exported(source: &str) -> Result<Option<ThemeExport>, String> {
  ComponentSet::new(std::path::Path::new("")).provide("routes/layout.tsx", source).theme_export("routes/layout.tsx").map_err(|e| e.to_string())
}

#[test]
fn a_theme_export_is_a_name_or_a_forced_name() {
  let page = "export default function Layout() { return <main />; }";
  assert_eq!(exported(page).unwrap(), None);
  assert_eq!(exported(&format!("export const theme = \"editorial\";\n{page}")).unwrap(), Some(ThemeExport { name: "editorial".to_owned(), force: false }));
  assert_eq!(exported(&format!("export const theme = {{ name: \"ledger\", force: true }} as const;\n{page}")).unwrap(), Some(ThemeExport { name: "ledger".to_owned(), force: true }));
  assert_eq!(exported(&format!("export const theme = {{ name: `night` }};\n{page}")).unwrap(), Some(ThemeExport { name: "night".to_owned(), force: false }));
}

#[test]
fn a_theme_export_the_build_cannot_read_is_refused() {
  let page = "export default function Layout() { return <main />; }";
  for bad in ["export const theme = pick();", "export const theme = { name: \"a\", force: maybe };", "export const theme = { force: true };", "export const theme = { name: \"a\", tone: 1 };"] {
    let err = exported(&format!("{bad}\n{page}")).unwrap_err();
    assert!(err.contains("`theme` is a theme's name"), "{bad}: {err}");
  }
}

#[test]
fn theme_ts_reads_the_route_theme_as_its_input() {
  let source = "export function theme({ theme, session }) {\n  if (theme.forced) return { name: theme.name };\n  return { name: session.theme === \"\" ? theme.name : session.theme, scheme: \"dark\" };\n}\n";
  let body = snapfire_fsr_lower::lower_theme("theme.ts", source).unwrap();
  let printed = print(&body.iter().map(stmt_to_sx).collect::<Vec<_>>());
  assert!(printed.contains("(if (. (input) forced) ((ret (obj (name (. (input) name))))))"), "{printed}");
  assert!(printed.contains("(session theme)"), "{printed}");
}

#[test]
fn theme_ts_without_a_theme_export_is_refused() {
  let err = snapfire_fsr_lower::lower_theme("theme.ts", "export default function theme() { return {}; }").unwrap_err().to_string();
  assert!(err.contains("theme"), "{err}");
}
