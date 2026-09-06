use snapfire::{SnapFireError, TeraWeb};
use std::fs;
use tempfile::TempDir;

fn upcase(value: &str, _: tera::Kwargs, _: &tera::State) -> String {
  value.to_uppercase()
}

fn write_templates(files: &[(&str, &str)]) -> (TempDir, String) {
  let dir = tempfile::tempdir().expect("temp dir");
  for (name, contents) in files {
    fs::write(dir.path().join(name), contents).expect("write template");
  }
  let glob = dir.path().join("*.html").to_str().unwrap().to_string();
  (dir, glob)
}

fn report(label: &str, result: Result<TeraWeb, SnapFireError>) {
  match result {
    Ok(_) => println!("  {label}: built"),
    Err(e) => println!("  {label}: {e}"),
  }
}

fn main() {
  println!("Tera 2 resolves filter, function, test and component names while it parses.");
  println!("Anything a template references has to be registered before the glob loads,");
  println!("so these all fail at build() rather than at render time.\n");

  println!("unregistered filter");
  let (_dir, glob) = write_templates(&[("index.html", "{{ name | upcase }}")]);
  report("without configure_tera", TeraWeb::builder(&glob).build());
  report(
    "with configure_tera",
    TeraWeb::builder(&glob)
      .configure_tera(|tera| tera.register_filter("upcase", upcase))
      .build(),
  );

  println!("\nunknown component");
  let (_dir, glob) = write_templates(&[("index.html", r#"{{ <missing_card title="hi"/> }}"#)]);
  report("component never defined", TeraWeb::builder(&glob).build());

  let (_dir, glob) = write_templates(&[
    (
      "components.html",
      "{% component card(title) %}<h2>{{ title }}</h2>{% endcomponent card %}",
    ),
    ("index.html", r#"{{ <card title="hi"/> }}"#),
  ]);
  report("component defined in the same glob", TeraWeb::builder(&glob).build());

  println!("\nmalformed template");
  let (_dir, glob) = write_templates(&[("index.html", "{% for item in items %}{{ item }}")]);
  report("unclosed for loop", TeraWeb::builder(&glob).build());

  println!("\nempty glob");
  let (_dir, glob) = write_templates(&[]);
  report("no templates matched", TeraWeb::builder(&glob).build());
}
