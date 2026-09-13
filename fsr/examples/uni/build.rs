//! Lowers the one component the server renders itself into the plan the host
//! reads at boot. Everything else here is a Tera template; this is the seam
//! showing that the two tiers sit in one application.

use std::path::Path;

fn main() {
  println!("cargo:rerun-if-changed=js/src/ui/Lot.tsx");
  let app = Path::new(env!("CARGO_MANIFEST_DIR"));
  let mut set = snapfire_fsr_lower::component::ComponentSet::new(app);
  set.lower("js/src/ui/Lot.tsx#default").expect("`Lot.tsx` lowers");
  let components = set
    .components
    .into_iter()
    .map(|(module, body)| snapfire_fsr_plan::ComponentEntry { module, body })
    .collect();
  let manifest = snapfire_fsr_plan::Manifest::new(Vec::new()).with_components(components);
  let out = Path::new(&std::env::var("OUT_DIR").expect("OUT_DIR")).join("plan.sexp");
  std::fs::write(&out, manifest.to_sexpr()).expect("the plan is writable");
}
