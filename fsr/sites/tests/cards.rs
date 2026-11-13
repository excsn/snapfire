//! An application that draws card images needs a host built with the `share_cards` feature, and one built without it says so at boot.

use snapfire_fsr_host::Host;
use snapfire_fsr_host::config::Config;

#[test]
fn an_application_with_card_images_boots_only_on_a_host_that_draws_them() {
  let nanos = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
  let root = snapfire_fsr_test_internal::scratch_root().join(format!("fsr-cards-{}-{nanos}", std::process::id()));
  std::fs::create_dir_all(root.join("app/generated")).unwrap();
  let plan = r#"(plan 2)
(route / (node 0 shell#document (slot content (node 1 routes/page.tsx#default))))
(image /opengraph-image
  routes/opengraph-image.tsx#default
  (size 1200 630)
  (type image/png)
  (page routes/page.tsx#default og ||))
"#;
  std::fs::write(root.join("app/generated/plan.sexp"), snapfire_fsr_plan::Manifest::from_text(plan).unwrap().to_sexpr()).unwrap();
  std::fs::write(root.join("app.toml"), "[app]\ndir = \"app\"\n[document]\ntitle = \"t\"\n[session]\nkey = \"k\"\n").unwrap();
  let built = Host::from_config(Config::load(&root).unwrap()).and_then(|builder| builder.build()).map(|_| ()).map_err(|e| e.to_string());
  match snapfire_fsr_host::SHARE_CARDS {
    true => assert!(built.is_ok(), "{built:?}"),
    false => assert!(built.as_ref().is_err_and(|e| e.contains("`share_cards`")), "{built:?}"),
  }
  let _ = std::fs::remove_dir_all(&root);
}
