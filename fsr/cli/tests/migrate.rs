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
      ("app/page.tsx", "import Link from \"next/link\";\n\nexport const metadata = { title: \"Home\" };\n\nexport default function Page() {\n  return <Link href=\"/about\">About</Link>;\n}\n"),
      ("node_modules/next/index.js", "module.exports = {};\n"),
      (".next/cache/x", "built\n"),
    ],
  );
  let to = dir.join("fsr");
  let migrated = run(&from, &to, false).unwrap();
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
fn an_application_no_package_json_names_a_framework_for_is_refused() {
  let dir = scratch();
  let from = dir.join("plain");
  write(&from, &[("package.json", r#"{ "dependencies": { "express": "4" } }"#), ("index.js", "\n")]);
  let to = dir.join("fsr");
  let error = run(&from, &to, false).unwrap_err().to_string();
  assert!(error.contains("names neither next, @sveltejs/kit nor nuxt"), "{error}");
  assert!(!to.exists(), "nothing is written for a source migrate cannot read");
  std::fs::remove_dir_all(&dir).unwrap();
}
