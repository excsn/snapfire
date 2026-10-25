use std::path::{Path, PathBuf};

use snapfire_fsr_cli::migrate::run;

static NEXT: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);

fn scratch() -> PathBuf {
  let n = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
  let dir = std::env::temp_dir().join(format!("fsr-cli-migrate-{}-{n}", std::process::id()));
  let _ = std::fs::remove_dir_all(&dir);
  std::fs::create_dir_all(&dir).unwrap();
  dir
}

fn write(root: &Path, files: &[(&str, &str)]) {
  for (name, text) in files {
    let path = root.join(name);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, text).unwrap();
  }
}

#[test]
fn a_next_app_becomes_a_project_whose_routes_are_its_own() {
  let dir = scratch();
  let from = dir.join("next");
  write(
    &from,
    &[
      ("package.json", r#"{ "dependencies": { "next": "15", "react": "19", "clsx": "^2" } }"#),
      ("app/layout.tsx", "import { Inter } from \"next/font/google\";\n\nconst inter = Inter({ subsets: [\"latin\"] });\n\nexport default function RootLayout({ children }) {\n  return (\n    <html lang=\"en\">\n      <body className={inter.className}>{children}</body>\n    </html>\n  );\n}\n"),
      ("app/page.tsx", "import Link from \"next/link\";\nimport clsx from \"clsx\";\n\nexport const metadata = { title: \"Home\" };\n\nexport default function Page() {\n  return <Link className={clsx(\"nav\")} href=\"/about\">About</Link>;\n}\n"),
      ("node_modules/next/index.js", "module.exports = {};\n"),
      (".next/cache/x", "built\n"),
    ],
  );
  let to = dir.join("fsr");
  let migrated = run(&from, &to, false, None).unwrap();
  let mut routes: Vec<String> = std::fs::read_dir(to.join("app/routes")).unwrap().flatten().map(|e| e.file_name().to_string_lossy().into_owned()).collect();
  routes.sort();
  assert_eq!(routes, ["layout.tsx", "page.loader.ts", "page.tsx"]);
  assert_eq!(std::fs::read_to_string(to.join("app/routes/page.loader.ts")).unwrap(), "export const meta = () => ({ title: \"Home\" });\n");
  assert!(std::fs::read_to_string(to.join("config/app.toml")).unwrap().contains("[fonts.inter]\nfamily = \"Inter\""));
  assert!(!to.join("app/src/node_modules").exists() && !to.join("app/src/.next").exists());
  assert_eq!(migrated.report.packages.keys().collect::<Vec<_>>(), ["clsx"]);
  std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn an_ink_big_text_font_comes_from_the_cfonts_package_installed_above_the_application() {
  let dir = scratch();
  let from = dir.join("packages/title");
  let font = r#"{ "name": "block", "colors": 1, "lines": 1, "buffer": [""], "letterspace": [""], "chars": { "A": ["a"] } }"#;
  write(&dir, &[("node_modules/cfonts/fonts/block.json", font), ("node_modules/cfonts/fonts/tiny.json", font)]);
  write(
    &from,
    &[
      ("package.json", r#"{ "type": "module", "dependencies": { "ink": "^5", "react": "^18", "ink-big-text": "^2" } }"#),
      ("src/cli.tsx", "import React from 'react';\nimport {render} from 'ink';\nimport BigText from 'ink-big-text';\n\nrender(<BigText text=\"hi\" />);\n"),
    ],
  );
  let to = dir.join("fsr");
  run(&from, &to, false, None).unwrap();
  assert!(std::fs::read_to_string(to.join("config/app.toml")).unwrap().contains("runs = \"terminal\""), "an Ink application is migrated to a terminal program");
  assert!(!to.join("app/routes/layout.tsx").exists() && !to.join("app/vendor").exists(), "no web scaffold and no vendored React");
  assert_eq!(std::fs::read_to_string(to.join("app/fonts/block.json")).unwrap(), font);
  assert!(!to.join("app/fonts/tiny.json").exists() && !to.join(".cfonts").exists());
  std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn an_application_no_package_json_names_a_framework_for_is_refused() {
  let dir = scratch();
  let from = dir.join("plain");
  write(&from, &[("package.json", r#"{ "dependencies": { "express": "4" } }"#), ("index.js", "\n")]);
  let to = dir.join("fsr");
  let error = run(&from, &to, false, None).unwrap_err().to_string();
  assert!(error.contains("names neither next, @sveltejs/kit, nuxt nor ink"), "{error}");
  assert!(!to.exists(), "nothing is written for a source migrate cannot read");
  std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn a_shell_package_in_the_workspace_moves_in_and_its_map_applies_unless_one_is_named() {
  let dir = scratch();
  write(
    &dir,
    &[
      ("pnpm-workspace.yaml", "packages:\n  - apps/*\n  - packages/*\ncatalog:\n  react: 19\n"),
      ("apps/web/package.json", r#"{ "dependencies": { "next": "15", "@acme/shell": "workspace:*" } }"#),
      ("apps/web/app/page.tsx", "import { getLocale } from \"@acme/shell/i18n\";\n\nexport default async function Page() {\n  const locale = await getLocale();\n  return <p>{locale}</p>;\n}\n"),
      ("packages/shell/package.json", r#"{ "name": "@acme/shell", "exports": { "./i18n": "./src/i18n.ts" } }"#),
      ("packages/shell/src/i18n.ts", "export const getLocale = async () => \"en\";\n"),
      ("packages/shell/migrate.toml", "[map.\"@acme/shell/i18n#getLocale\"]\ncall = \"params.locale\"\n"),
    ],
  );
  let to = dir.join("fsr");
  run(&dir.join("apps/web"), &to, false, None).unwrap();
  let loader = std::fs::read_to_string(to.join("app/routes/page.loader.ts")).unwrap();
  assert!(loader.contains("const locale = params.locale;"), "the shell's own map applies: {loader}");
  let named = dir.join("other.toml");
  std::fs::write(&named, "[map.\"@acme/shell/i18n#getLocale\"]\nnote = \"by hand\"\n").unwrap();
  let to = dir.join("fsr-named");
  let migrated = run(&dir.join("apps/web"), &to, false, Some(&named)).unwrap();
  let loader = std::fs::read_to_string(to.join("app/routes/page.loader.ts")).unwrap();
  assert!(loader.contains("from \"@src/packages/@acme/shell/src/i18n\"") && loader.contains("await getLocale()"), "a named map wins over the shell's: {loader}");
  assert!(to.join("app/src/packages/@acme/shell/src/i18n.ts").is_file());
  assert!(migrated.report.notes.iter().any(|n| n.message.ends_with("by hand")), "{:?}", migrated.report.notes);
  std::fs::remove_dir_all(&dir).unwrap();
}
