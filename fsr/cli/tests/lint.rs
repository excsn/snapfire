use std::path::PathBuf;

use snapfire_fsr_cli::new::{NewOptions, create};

static NEXT: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);

fn project() -> PathBuf {
  let n = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
  let root = snapfire_fsr_test_internal::scratch_root().join(format!("fsr-cli-lint-{}-{n}", std::process::id()));
  let _ = std::fs::remove_dir_all(&root);
  create(&root, NewOptions::default().fetch(false)).unwrap();
  root
}

#[test]
fn a_new_project_lints_clean() {
  let root = project();
  let report = snapfire_fsr_cli::lint::run(&root.join("app")).unwrap();
  assert!(report.files > 0, "the scaffold's routes are linted");
  assert!(report.findings.is_empty(), "{}", report.render());
  let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn a_finding_in_a_route_fails_and_vendored_packages_are_not_linted() {
  let root = project();
  let app = root.join("app");
  std::fs::write(app.join("routes/about.ts"), "export const about = 1;\ndebugger;\n").unwrap();
  std::fs::create_dir_all(app.join("vendor/left-pad")).unwrap();
  std::fs::write(app.join("vendor/left-pad/index.js"), "debugger;\n").unwrap();
  let report = snapfire_fsr_cli::lint::run(&app).unwrap();
  let found: Vec<_> = report.findings.iter().map(|f| (f.rule.as_str(), f.path.strip_prefix(&app).unwrap().to_owned())).collect();
  assert_eq!(found, [("eslint/no-debugger", PathBuf::from("routes/about.ts"))]);
  assert!(report.failed(false));
  let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn the_projects_lint_file_sets_levels() {
  let root = project();
  let app = root.join("app");
  std::fs::write(app.join("routes/about.ts"), "export const about = 1;\ndebugger;\n").unwrap();
  std::fs::write(root.join(snapfire_lint::CONFIG_FILE), "[rules]\n\"eslint/no-debugger\" = \"warn\"\n").unwrap();
  let report = snapfire_fsr_cli::lint::run(&app).unwrap();
  assert_eq!((report.errors(), report.warnings()), (0, 1), "{}", report.render());
  let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn htmx_attributes_are_known_properties() {
  let root = project();
  let app = root.join("app");
  std::fs::write(app.join("routes/notices.tsx"), "export default function Notices() {\n  return <div hx-get=\"/notices?__fragment\" hx-trigger=\"intersect once\" hx-swap=\"outerHTML\" hx-target=\"closest .page\">notices</div>;\n}\n").unwrap();
  std::fs::write(app.join("routes/typo.tsx"), "export default function Typo() {\n  return <div hx-gett=\"/\">typo</div>;\n}\n").unwrap();
  let report = snapfire_fsr_cli::lint::run(&app).unwrap();
  let found: Vec<_> = report.findings.iter().map(|f| (f.rule.as_str(), f.path.strip_prefix(&app).unwrap().to_owned())).collect();
  assert_eq!(found, [("react/no-unknown-property", PathBuf::from("routes/typo.tsx"))], "{}", report.render());
  let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn a_component_s_template_is_linted_through_its_plugin() {
  let root = project();
  let app = root.join("app");
  std::fs::create_dir_all(app.join("src/ui")).unwrap();
  std::fs::write(app.join("src/ui/List.vue"), "<template>\n  <ul><li v-for=\"x in xs\">{{ x }}</li></ul>\n</template>\n<script setup>\nconst xs = [1];\n</script>\n").unwrap();
  let report = snapfire_fsr_cli::lint::run(&app).unwrap();
  let rules: Vec<_> = report.findings.iter().map(|f| f.rule.as_str()).collect();
  assert!(rules == ["vue/require-v-for-key"] || rules == [snapfire_lint::COMPILER], "the template is linted, or the missing plugin is named: {}", report.render());
  let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn fix_writes_the_safe_fixes_and_reports_what_is_left() {
  let root = project();
  let app = root.join("app");
  std::fs::write(app.join("routes/pattern.ts"), "export const p = /a  b/;\nif (p) {}\n").unwrap();
  let (report, applied) = snapfire_fsr_cli::lint::fix(&app).unwrap();
  assert_eq!(applied, 1);
  assert_eq!(std::fs::read_to_string(app.join("routes/pattern.ts")).unwrap(), "export const p = /a {2}b/;\nif (p) {}\n");
  let rules: Vec<_> = report.findings.iter().map(|f| f.rule.as_str()).collect();
  assert_eq!(rules, ["eslint/no-empty"], "{}", report.render());
  let _ = std::fs::remove_dir_all(&root);
}

fn extraneous(report: &snapfire_lint::Report) -> Vec<String> {
  report.findings.iter().filter(|f| f.rule == "import/no-extraneous-dependencies").map(|f| f.message.clone()).collect()
}

#[test]
fn an_import_of_a_package_vendored_only_for_another_is_found() {
  let root = project();
  let app = root.join("app");
  std::fs::create_dir_all(app.join("vendor")).unwrap();
  let manifest = r#"{ "packages": {
    "svelte": { "version": "5.57.2", "published": true },
    "clsx": { "version": "2.1.1", "published": true, "dependency_of": ["svelte"] },
    "sweetalert2": { "version": "11.6.15" }
  } }"#;
  std::fs::write(app.join("vendor/.fsr-vendor.json"), manifest).unwrap();
  std::fs::write(app.join("routes/classes.ts"), "import clsx from \"clsx\";\nimport Swal from \"sweetalert2\";\nimport { writable } from \"svelte/store\";\nexport const all = [clsx, Swal, writable];\n").unwrap();
  let report = snapfire_fsr_cli::lint::run(&app).unwrap();
  let found = extraneous(&report);
  assert_eq!(found.len(), 1, "{}", report.render());
  assert!(found[0].contains("`clsx`") && found[0].contains("`svelte`"), "{}", found[0]);
  assert!(snapfire_fsr_cli::lint::rules().iter().any(|r| r.id == "import/no-extraneous-dependencies"));
  let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn an_xwpm_application_declares_its_modules_in_the_wmf() {
  let root = project();
  let app = root.join("app");
  std::fs::write(app.join("xwpm.wmf"), "vendor    = vendor\nimportmap = importmap.json\n\n[modules]\nnormansoven/editor = cove:0.1.0\n").unwrap();
  std::fs::write(app.join("importmap.json"), r#"{ "imports": { "normansoven/editor": "/static/js/vendor/normansoven/editor/index.js", "markdown-it": "/static/js/vendor/markdown-it/index.mjs", "@snapfire/fsr/client": "/static/js/fsr/client.js" } }"#).unwrap();
  std::fs::write(app.join("routes/editor.ts"), "import { Editor } from \"normansoven/editor\";\nimport MarkdownIt from \"markdown-it\";\nexport const both = [Editor, MarkdownIt];\n").unwrap();
  let report = snapfire_fsr_cli::lint::run(&app).unwrap();
  let found = extraneous(&report);
  assert_eq!(found.len(), 1, "{}", report.render());
  assert!(found[0].contains("`markdown-it`"), "{}", found[0]);
  let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn reacts_hooks_rules_run_only_where_the_app_has_react() {
  let composable = "export function useAuth() { return 1; }\nexport function signIn(ok: boolean) {\n  if (ok) {\n    return useAuth();\n  }\n  return 0;\n}\n";
  let hooks = |manifest: &str| {
    let root = project();
    let app = root.join("app");
    std::fs::create_dir_all(app.join("vendor")).unwrap();
    std::fs::write(app.join("vendor/.fsr-vendor.json"), manifest).unwrap();
    std::fs::create_dir_all(app.join("src")).unwrap();
    std::fs::write(app.join("src/auth.ts"), composable).unwrap();
    let report = snapfire_fsr_cli::lint::run(&app).unwrap();
    let _ = std::fs::remove_dir_all(&root);
    report.findings.iter().filter(|f| f.rule == "react/rules-of-hooks").count()
  };
  assert_eq!(hooks(r#"{ "packages": { "vue": { "version": "3.5.44" } } }"#), 0, "a Vue app's composables are not React hooks");
  assert_eq!(hooks(r#"{ "packages": { "react": { "version": "18.3.1" }, "react-dom": { "version": "18.3.1" } } }"#), 1);
}
