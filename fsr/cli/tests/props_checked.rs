//! A page's props are its loader's own return type as the checker reads it: what a `try` and its `catch` return, a value a `fail` guard rules out and a cast are all the checker's. Runs the real checker over declarations from registry.npmjs.org; when the registry does not answer or no checker is built, each test says so on the terminal and passes.

use std::io::Write;
use std::path::{Path, PathBuf};

use snapfire_fsr_cli::new::{create, NewOptions};
use snapfire_fsr_cli::typecheck::Typecheck;
use snapfire_fsr_cli::{check, Options};

const REGISTRY: &str = "https://registry.npmjs.org/@types%2freact";

/// `snapfiretc` from `$SNAPFIRETC` or the workspace above this one, when the registry answers; the reason on stderr otherwise.
fn ready(test: &str) -> Option<PathBuf> {
  let skip = |why: String| {
    let _ = writeln!(std::io::stderr(), "SKIPPED {test}: {why}");
    None
  };
  let checker = std::env::var_os("SNAPFIRETC").map(PathBuf::from).unwrap_or_else(|| Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/debug/snapfiretc"));
  if !checker.is_file() {
    return skip(format!("no checker at {}", checker.display()));
  }
  let probe = reqwest::blocking::Client::builder().timeout(std::time::Duration::from_secs(10)).build().and_then(|client| client.head(REGISTRY).send());
  match probe {
    Ok(response) if response.status().is_success() => Some(checker),
    Ok(response) => skip(format!("{REGISTRY} answered HTTP {}", response.status())),
    Err(e) => skip(format!("{REGISTRY} did not answer ({e})")),
  }
}

fn project(tag: &str, files: &[(&str, &str)]) -> PathBuf {
  let root = std::env::temp_dir().join(format!("fsr-cli-props-{}-{tag}", std::process::id()));
  let _ = std::fs::remove_dir_all(&root);
  create(&root, NewOptions::default()).unwrap();
  for (path, text) in files {
    let file = root.join("app").join(path);
    std::fs::create_dir_all(file.parent().unwrap()).unwrap();
    std::fs::write(file, text).unwrap();
  }
  root
}

/// The checker's diagnostics over the app at `root`.
fn diagnostics(root: &Path, checker: &Path) -> Vec<String> {
  let app = root.join("app");
  let typecheck = Typecheck::beside(&app).checker(Some(checker.to_path_buf()));
  let (_, checked) = check(&app, &Options::beside(&app), &typecheck, |_| {}).unwrap_or_else(|e| panic!("{e}"));
  checked.expect("the checker ran").diagnostics.iter().map(|d| d.to_string()).collect()
}

const CONTINENT_LOADER: &str = r#"import type { Failure } from "@snapfire/fsr";

function lookup(code: string): { name: string } {
  if (code === "") throw new Error("none");
  return { name: code };
}

export async function load() {
  try {
    const continent = lookup("EU");
    return { continent, failed: null };
  } catch (e) {
    return { continent: null, failed: (e as Failure).kind };
  }
}
"#;

const CONTINENT_PAGE: &str = r#"import type { RootProps } from "@generated/client";
import type { FailureKind } from "@snapfire/fsr";

export default function Index({ continent, failed }: RootProps) {
  const kind: FailureKind | null = failed;
  return <section>{continent ? <h1>{continent.name}</h1> : <p>{kind}</p>}</section>;
}
"#;

const RECIPES: &str = "export const RECIPES = [{ slug: \"a\", title: \"A\" }, { slug: \"b\", title: \"B\" }];\n";

const RECIPE_PAGE: &str = "import type { RecipeSlugProps } from \"@generated/client\";\n\nexport default function Recipe({ recipe }: RecipeSlugProps) {\n  return <h1>{recipe.title}</h1>;\n}\n";

fn recipe_loader(guard: &str) -> String {
  format!("import {{ fail }} from \"@snapfire/fsr\";\nimport type {{ Ctx }} from \"@snapfire/fsr\";\nimport {{ RECIPES }} from \"../../../src/recipes\";\n\nexport async function load({{ params }}: Ctx<\"/recipe/{{slug}}\">) {{\n  const recipe = RECIPES.find((r) => r.slug === params.slug);\n  {guard}\n  return {{ recipe }};\n}}\n")
}

/// DEFECTS 1.156 and 1.157: a union from `try` and `catch`, a cast in the `catch` and every guard that rules a value out type as the checker reads the loader.
#[test]
fn props_are_the_loaders_return_as_the_checker_reads_it() {
  let Some(checker) = ready("props_are_the_loaders_return_as_the_checker_reads_it") else { return };
  let guards = [
    "if (!recipe) fail(\"not_found\", `there is no recipe ${params.slug}`);",
    "if (recipe === undefined) fail(\"not_found\", \"gone\");",
    "if (recipe == null || params.slug === \"\") fail(\"not_found\", \"gone\");",
    "if (!recipe) {\n    fail(\"not_found\", \"gone\");\n  }",
    "if (!recipe) return { recipe: { slug: \"none\", title: \"None\" } };",
  ];
  for (i, guard) in guards.iter().enumerate() {
    let loader = recipe_loader(guard);
    let root = project(&format!("guard{i}"), &[("routes/page.loader.ts", CONTINENT_LOADER), ("routes/page.tsx", CONTINENT_PAGE), ("src/recipes.ts", RECIPES), ("routes/recipe/[slug]/page.loader.ts", &loader), ("routes/recipe/[slug]/page.tsx", RECIPE_PAGE)]);
    assert_eq!(diagnostics(&root, &checker), Vec::<String>::new(), "after `{guard}`");
    std::fs::remove_dir_all(&root).unwrap();
  }

  let loader = recipe_loader("");
  let root = project("unguarded", &[("src/recipes.ts", RECIPES), ("routes/recipe/[slug]/page.loader.ts", &loader), ("routes/recipe/[slug]/page.tsx", RECIPE_PAGE)]);
  let found = diagnostics(&root, &checker);
  assert!(found.iter().any(|d| d.contains("routes/recipe/[slug]/page.tsx") && d.contains("possibly 'undefined'")), "unguarded it stays possibly undefined: {found:?}");
  std::fs::remove_dir_all(&root).unwrap();
}
