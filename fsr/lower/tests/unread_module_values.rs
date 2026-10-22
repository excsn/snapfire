//! A module-level value the build cannot read leaves what places it to the browser instead of refusing the build.

use std::path::PathBuf;

use snapfire_fsr_lower::component::ComponentSet;

fn app(tag: &str, files: &[(&str, &str)]) -> PathBuf {
  let dir = std::env::temp_dir().join(format!("fsr_unread_module_values_{}_{tag}", std::process::id()));
  let _ = std::fs::remove_dir_all(&dir);
  for (name, text) in files {
    let path = dir.join(name);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, text).unwrap();
  }
  dir
}

const NAVIGATION: &str = "import { createNavigation } from \"next-intl/navigation\";\nimport { routing } from \"./routing\";\n\nexport const { Link, redirect, usePathname } = createNavigation(routing);\n";
const ROUTING: &str = "export const routing = { locales: [\"en\", \"de\"], defaultLocale: \"en\" };\n";

#[test]
fn a_component_destructured_from_an_unread_call_is_left_to_the_browser_and_the_page_lowers() {
  let page = "import { Link } from \"@src/navigation\";\nexport default function Page() {\n  return <main><Link href=\"/a\">a</Link></main>;\n}\n";
  let dir = app("placed", &[("src/navigation.ts", NAVIGATION), ("src/routing.ts", ROUTING), ("routes/page.tsx", page)]);
  let mut set = ComponentSet::new(&dir);
  let lowered = set.lower("routes/page.tsx#default");
  std::fs::remove_dir_all(&dir).unwrap();
  lowered.unwrap_or_else(|e| panic!("the build is refused: {e}"));
  set.settle_fills();
  assert!(set.browser_only.iter().any(|(module, residue)| module == "src/navigation.ts#Link" && residue.message.contains("createNavigation")), "{:?}", set.browser_only);
  assert!(set.components.iter().any(|(module, _)| module == "routes/page.tsx#default"), "the page lowers around the island");
}

#[test]
fn a_route_layout_re_exporting_an_app_component_lowers_as_that_component() {
  let base = "export default function BaseLayout({ children }: { children: any }) {\n  return <div className=\"shell\">{children}</div>;\n}\n";
  for (tag, layout) in [("imported", "import BaseLayout from \"@src/BaseLayout\";\nexport default BaseLayout;\n"), ("re_exported", "export { default } from \"@src/BaseLayout\";\n")] {
    let dir = app(tag, &[("src/BaseLayout.tsx", base), ("routes/layout.tsx", layout)]);
    let mut set = ComponentSet::new(&dir);
    let lowered = set.lower("routes/layout.tsx#default");
    std::fs::remove_dir_all(&dir).unwrap();
    lowered.unwrap_or_else(|e| panic!("{tag}: {e}"));
    assert!(set.browser_only.is_empty(), "{tag}: {:?}", set.browser_only);
    assert!(set.components.iter().any(|(module, _)| module == "routes/layout.tsx#default"), "{tag}: {:?}", set.components.iter().map(|(module, _)| module).collect::<Vec<_>>());
  }
}
