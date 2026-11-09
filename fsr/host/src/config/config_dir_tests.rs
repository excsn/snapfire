use super::*;

fn from_a_store_at(root: &Path) -> Config {
  let options = C5StoreOptions::default();
  let empty = root.join("empty.yaml");
  std::fs::write(&empty, "{}\n").unwrap();
  let (store, _mgr) = create_c5store(vec![empty], Some(options)).unwrap();
  Config::from_store_at(&store, root).unwrap()
}

#[test]
fn a_store_built_config_finds_its_directory_as_a_located_one_does() {
  let root = snapfire_fsr_test_internal::scratch_root().join(format!("fsr_config_dir_{}", std::process::id()));
  std::fs::create_dir_all(root.join("config")).unwrap();
  assert_eq!(from_a_store_at(&root).config_dir(), root.join("config"));
  std::fs::remove_dir_all(&root).unwrap();
}

#[test]
fn with_no_config_directory_it_is_the_root() {
  let root = snapfire_fsr_test_internal::scratch_root().join(format!("fsr_config_dir_flat_{}", std::process::id()));
  std::fs::create_dir_all(&root).unwrap();
  assert_eq!(from_a_store_at(&root).config_dir(), root);
  std::fs::remove_dir_all(&root).unwrap();
}
