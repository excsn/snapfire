//! DEFECTS 1.98: the configuration listing in README.USAGE.md had `[mdx]`
//! between `[crawl]` and its keys, so copied as written `robots`, `sitemap`
//! and `disallow` were `[mdx]`'s and `[crawl]` served nothing. The listing is
//! read here as the TOML a reader copies.

fn listing() -> toml::Table {
  let guide = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/README.USAGE.md")).unwrap();
  let section = guide.split("## Writing config/app.toml").nth(1).expect("the configuration chapter");
  let block = section.split("```toml\n").nth(1).and_then(|rest| rest.split("\n```").next()).expect("its listing");
  block.parse().unwrap_or_else(|e| panic!("the listing is TOML: {e}"))
}

#[test]
fn each_key_in_the_configuration_listing_sits_under_its_own_section() {
  let table = listing();
  let crawl = table["crawl"].as_table().expect("[crawl]");
  for key in ["robots", "sitemap", "disallow"] {
    assert!(crawl.contains_key(key), "[crawl] {key}: {crawl:?}");
  }
  let mdx = table["mdx"].as_table().expect("[mdx]");
  for key in mdx.keys() {
    assert!(["gfm", "heading_ids", "frontmatter", "highlight", "collections"].contains(&key.as_str()), "[mdx] takes no {key}");
  }
}
