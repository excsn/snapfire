//! `fsr ir view`: one row of the plan the last build wrote, or one section of
//! a source row, printed alone.

use std::path::PathBuf;

use snapfire_fsr_cli::{build, ir, write, Options};

static NEXT: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);

fn app(files: &[(&str, &str)]) -> PathBuf {
  let nanos = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
  let n = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
  let dir = std::env::temp_dir().join(format!("fsr-ir-{}-{n}-{nanos}", std::process::id()));
  std::fs::create_dir_all(dir.join("routes")).unwrap();
  std::fs::create_dir_all(dir.join("vendor")).unwrap();
  std::fs::write(dir.join("importmap.json"), r#"{"imports":{"@snapfire/fsr-client/react":"/r","react":"/r","react-dom/client":"/d"}}"#).unwrap();
  std::fs::write(dir.join("vendor/.fsr-vendor.json"), r#"{"packages":{"react":{"version":"18.3.1"},"react-dom":{"version":"18.3.1"}}}"#).unwrap();
  for (name, source) in files {
    let path = dir.join(name);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, source).unwrap();
  }
  dir
}

#[test]
fn a_view_prints_one_row_or_one_section_of_a_source_from_the_written_plan() {
  let dir = app(&[
    ("routes/page.tsx", "export default function Page({ data }: { data: { count: number } }) {\n  return <p>{data.count}</p>;\n}\n"),
    ("routes/page.loader.ts", "export async function load() {\n  return { count: 2 };\n}\nexport const meta = ({ data }: { data: { count: number } }) => ({ title: `${data.count} in the cart` });\n"),
    ("routes/actions.ts", "import { action } from \"@snapfire/fsr\";\nexport const add = action(async () => ({ ok: true }));\n"),
  ]);
  let missing = ir::view(&dir, "$root", None).unwrap_err().to_string();
  assert!(missing.contains("run `fsr build` first"), "{missing}");

  let built = build(&dir, &Options::default()).unwrap();
  write(&dir, &built).unwrap();

  let row = ir::view(&dir, "$root", None).unwrap();
  assert!(row.starts_with("(source $root\n  lowered\n") && row.contains("\n  (body") && row.contains("\n  (meta"), "{row}");
  let meta = ir::view(&dir, "$root", Some("meta")).unwrap();
  assert!(meta.starts_with("(meta") && !meta.contains("(body"), "{meta}");
  let action = ir::view(&dir, "$root.add", None).unwrap();
  assert!(action.starts_with("(action $root.add "), "{action}");
  let component = ir::view(&dir, "routes/page.tsx#default", None).unwrap();
  assert!(component.starts_with("(component routes/page.tsx#default"), "{component}");

  let absent = ir::view(&dir, "$root", Some("store")).unwrap_err().to_string();
  assert!(absent.contains("no `store` section") && absent.contains("body, meta"), "{absent}");
  let one = ir::view(&dir, "$root.add", Some("body")).unwrap_err().to_string();
  assert!(one.contains("one body and no sections"), "{one}");
  let unknown = ir::view(&dir, "nothing", None).unwrap_err().to_string();
  assert!(unknown.starts_with("no row is named `nothing`; the rows are:\n$root\n$root.add\nroutes/page.tsx#default"), "{unknown}");
  std::fs::remove_dir_all(&dir).unwrap();
}
