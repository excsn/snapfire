//! A module binds each name once at its top level, as JavaScript requires; a second binding is refused naming both.

use snapfire_fsr_lower::component::ComponentSet;

/// What lowering `page` as `routes/page.tsx` refuses, or `None` when it lowers.
fn refused(tag: &str, page: &str) -> Option<String> {
  let dir = std::env::temp_dir().join(format!("fsr_lower_bindings_{}_{tag}", std::process::id()));
  let _ = std::fs::remove_dir_all(&dir);
  std::fs::create_dir_all(dir.join("routes")).unwrap();
  std::fs::write(dir.join("routes/page.tsx"), page).unwrap();
  let mut set = ComponentSet::new(&dir);
  let result = set.lower("routes/page.tsx#default").map_err(|e| e.to_string());
  std::fs::remove_dir_all(&dir).unwrap();
  match result {
    Err(e) => Some(e),
    Ok(()) => set.browser_only.first().map(|(_, r)| r.message.clone()),
  }
}

#[test]
fn an_import_declared_again_with_let_is_refused_naming_both() {
  let page = "import { system } from \"@snapfire/fsr/terminal\";\n\nlet system: Promise<void> | undefined;\n\nexport default function Page() {\n  return <p>{system ? \"waiting\" : \"idle\"}</p>;\n}\n";
  let message = refused("let", page).expect("refused");
  assert!(message.contains("routes/page.tsx:3:5") && message.contains("`system`, bound twice at the top of the module") && message.contains("line 1"), "{message}");
}

#[test]
fn an_import_declared_again_as_a_function_is_refused() {
  let page = "import { t } from \"@snapfire/fsr/client\";\n\nfunction t(key: string) {\n  return key;\n}\n\nexport default function Page() {\n  return <p>{t(\"hi\")}</p>;\n}\n";
  let message = refused("function", page).expect("refused");
  assert!(message.contains("`t`, bound twice at the top of the module"), "{message}");
}

#[test]
fn one_name_imported_twice_is_refused() {
  let page = "import { text } from \"@snapfire/fsr/client\";\nimport { intl as text } from \"@snapfire/fsr/client\";\n\nexport default function Page() {\n  return <p>{text.slug(\"A b\")}</p>;\n}\n";
  let message = refused("imports", page).expect("refused");
  assert!(message.contains("`text`, bound twice at the top of the module"), "{message}");
}

#[test]
fn a_local_inside_a_function_may_shadow_an_import() {
  let page = "import { text } from \"@snapfire/fsr/client\";\n\nfunction shout(s: string) {\n  const text = s.toUpperCase();\n  return text;\n}\n\nexport default function Page() {\n  return <p>{shout(text.slug(\"A b\"))}</p>;\n}\n";
  assert_eq!(refused("shadow", page), None);
}
