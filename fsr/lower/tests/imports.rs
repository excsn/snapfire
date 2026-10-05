use std::path::PathBuf;

use snapfire_fsr_lower::component::ComponentSet;

fn app(tag: &str, files: &[(&str, &[u8])]) -> PathBuf {
  let dir = std::env::temp_dir().join(format!("fsr_imports_{}_{tag}", std::process::id()));
  let _ = std::fs::remove_dir_all(&dir);
  for (name, bytes) in files {
    let path = dir.join(name);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, bytes).unwrap();
  }
  dir
}

#[test]
fn the_reach_follows_value_imports_and_re_exports_to_scripts_only() {
  let dir = app(
    "reach",
    &[
      ("src/ui/Card.tsx", b"import type { Shape } from \"./shape\";\nimport { Label } from \"./parts\";\nimport logo from \"./logo.png\";\nexport function Card(s: Shape) {\n  return <Label text={logo} />;\n}\n"),
      ("src/ui/shape.ts", b"export type Shape = { n: number };\n"),
      ("src/ui/parts.ts", b"export { Label } from \"./Label\";\nexport * from \"./more\";\n"),
      ("src/ui/Label.tsx", b"export function Label({ text }: { text: string }) {\n  return <b>{text}</b>;\n}\n"),
      ("src/ui/more.ts", b"export const unit = \"px\";\n"),
      ("src/ui/logo.png", &[0x89, 0x50, 0x4e, 0x47, 0xff, 0xfe]),
    ],
  );
  let mut set = ComponentSet::new(&dir);
  assert_eq!(set.local_reach("src/ui/Card.tsx"), ["src/ui/Card.tsx", "src/ui/parts.ts", "src/ui/Label.tsx", "src/ui/more.ts"], "a type-only import and an asset are not followed");
  std::fs::remove_dir_all(&dir).unwrap();
}
