use super::*;

fn options(format: Format) -> Options {
  Options::new(format, Target::Lowered)
}

fn lowered(source: &str) -> Converted {
  convert(source, &options(Format::Mdx)).expect("converts")
}

fn body(converted: &Converted) -> String {
  let tsx = &converted.tsx;
  let from = tsx.find("return <>").expect("a body") + "return <>".len();
  let to = tsx[from..].find("</>;").expect("an end") + from;
  tsx[from..to].lines().map(str::trim).collect::<Vec<_>>().concat()
}

#[test]
fn markdown_constructs_become_the_elements_mdx_writes() {
  let c = lowered("# Title\n\nA *b* **c** `d` [e](/f \"g\") ![h](/i.png)\\\nnext\n\n> quote\n\n1. one\n2. two\n\n---\n\n```rust\nfn x() {}\n```\n");
  assert_eq!(
    body(&c),
    concat!(
      "<h1 id={\"title\"}>{\"Title\"}</h1>",
      "<p>{\"A \"}<em>{\"b\"}</em>{\" \"}<strong>{\"c\"}</strong>{\" \"}<code>{\"d\"}</code>{\" \"}<a href={\"/f\"} title={\"g\"}>{\"e\"}</a>{\" \"}<img src={\"/i.png\"} alt={\"h\"} /><br />{\"\\n\"}{\"next\"}</p>",
      "<blockquote><p>{\"quote\"}</p></blockquote>",
      "<ol><li>{\"one\"}</li><li>{\"two\"}</li></ol>",
      "<hr />",
      "<pre><code className={\"language-rust\"}>{\"fn x() {}\\n\"}</code></pre>",
    )
  );
}

#[test]
fn a_loose_list_keeps_its_paragraphs_and_an_ordered_one_its_start() {
  let c = lowered("3. one\n\n4. two\n");
  assert_eq!(body(&c), "<ol start={3}><li><p>{\"one\"}</p></li><li><p>{\"two\"}</p></li></ol>");
}

#[test]
fn reference_links_and_images_resolve_through_their_definitions() {
  let c = lowered("[a][x] and ![b][y]\n\n[x]: /x \"X\"\n[y]: /y.png\n");
  assert_eq!(body(&c), "<p><a href={\"/x\"} title={\"X\"}>{\"a\"}</a>{\" and \"}<img src={\"/y.png\"} alt={\"b\"} /></p>");
}

#[test]
fn gfm_writes_tables_strikethrough_task_lists_autolinks_and_footnotes() {
  let c = lowered("| a | b |\n| :- | -: |\n| 1 | 2 |\n\n~~gone~~ www.example.com\n\n- [ ] open\n- [x] done\n\nSee[^1] and again[^1].\n\n[^1]: The note.\n");
  let body = body(&c);
  assert!(body.starts_with("<table><thead><tr><th align={\"left\"}>{\"a\"}</th><th align={\"right\"}>{\"b\"}</th></tr></thead><tbody><tr><td align={\"left\"}>{\"1\"}</td><td align={\"right\"}>{\"2\"}</td></tr></tbody></table>"), "{body}");
  assert!(body.contains("<del>{\"gone\"}</del>{\" \"}<a href={\"http://www.example.com\"}>{\"www.example.com\"}</a>"), "{body}");
  assert!(body.contains("<ul className={\"contains-task-list\"}><li className={\"task-list-item\"}><input type={\"checkbox\"} disabled={true} />{\" \"}{\"open\"}</li><li className={\"task-list-item\"}><input type={\"checkbox\"} disabled={true} checked={true} />"), "{body}");
  assert!(body.contains("<sup><a href={\"#user-content-fn-1\"} id={\"user-content-fnref-1\"} data-footnote-ref={\"\"} aria-describedby={\"footnote-label\"}>{\"1\"}</a></sup>{\" and again\"}<sup><a href={\"#user-content-fn-1\"} id={\"user-content-fnref-1-2\"}"), "{body}");
  assert!(body.contains("<section data-footnotes={\"\"} className={\"footnotes\"}><h2 className={\"sr-only\"} id={\"footnote-label\"}>{\"Footnotes\"}</h2><ol><li id={\"user-content-fn-1\"}><p>{\"The note.\"}{\" \"}<a href={\"#user-content-fnref-1\"}"), "{body}");
  assert!(body.contains("aria-label={\"Back to reference 1-2\"} className={\"data-footnote-backref\"}>{\"↩\"}<sup>{\"2\"}</sup></a>"), "{body}");
}

#[test]
fn without_gfm_a_table_is_a_paragraph() {
  let mut o = options(Format::Mdx);
  o.gfm = false;
  let c = convert("| a |\n| - |\n\n~~x~~\n", &o).expect("converts");
  assert_eq!(body(&c), "<p>{\"| a |\\n| - |\"}</p><p>{\"~~x~~\"}</p>");
}

#[test]
fn headings_get_ids_and_a_toc_unless_turned_off() {
  let c = lowered("# Intro\n\n## Setup `fsr`\n\n## Setup fsr\n");
  assert!(body(&c).contains("<h2 id={\"setup-fsr-1\"}>"), "{}", body(&c));
  assert_eq!(c.toc, vec![TocEntry { depth: 1, id: "intro".into(), text: "Intro".into() }, TocEntry { depth: 2, id: "setup-fsr".into(), text: "Setup fsr".into() }, TocEntry { depth: 2, id: "setup-fsr-1".into(), text: "Setup fsr".into() }]);
  assert!(c.tsx.contains("export const toc = [{ depth: 1, id: \"intro\", text: \"Intro\" }, "), "{}", c.tsx);
  assert!(c.exports.contains(&"toc".to_owned()));
  let mut o = options(Format::Mdx);
  o.heading_ids = false;
  let c = convert("# Intro\n", &o).expect("converts");
  assert_eq!(body(&c), "<h1>{\"Intro\"}</h1>");
  assert!(!c.tsx.contains("toc"));
}

#[test]
fn jsx_expressions_and_esm_are_written_as_they_stand() {
  let source = "import { Card } from \"./card\"\nexport const meta = { title: \"Hello\" }\n\n<Card tone=\"ok\" n={2} {...rest}>\n  Inside **card** {meta.title}\n</Card>\n\n<Card />\n";
  let c = lowered(source);
  assert!(c.tsx.starts_with("import { Card } from \"./card\"\nexport const meta = { title: \"Hello\" }\n"), "{}", c.tsx);
  assert_eq!(body(&c), "<Card tone={\"ok\"} n={2} {...rest}><p>{\"Inside \"}<strong>{\"card\"}</strong>{\" \"}{meta.title}</p></Card><Card />");
  assert!(c.meta);
  assert_eq!(c.exports, ["meta", "toc"]);
}

#[test]
fn an_esm_statement_that_is_not_an_import_or_export_is_refused() {
  let error = convert("export const a = 1\nconsole.log(a)\n", &options(Format::Mdx)).expect_err("refused");
  let Error::Syntax { line, message, .. } = error else { panic!("a syntax error") };
  assert_eq!(line, 2);
  assert!(message.contains("only import and export"), "{message}");
}

#[test]
fn an_unclosed_jsx_element_names_its_line() {
  let error = convert("# a\n\n<Card>\n", &options(Format::Mdx)).expect_err("refused");
  assert!(matches!(error, Error::Syntax { line: 3, .. } | Error::Syntax { line: 4, .. }), "{error}");
}

#[test]
fn mapped_elements_are_placed_as_imported_components() {
  let mut o = options(Format::Mdx);
  o.mapped = vec![Mapped { key: "h1".into(), specifier: "../mdx-components".into(), export: "__mdx_h1".into() }, Mapped { key: "a".into(), specifier: "../mdx-components".into(), export: "__mdx_a".into() }, Mapped { key: "img".into(), specifier: "../mdx-components".into(), export: "__mdx_img".into() }];
  let c = convert("# Hi\n\n[x](/y)\n", &o).expect("converts");
  assert!(c.tsx.contains("import { __mdx_a as Mdx_a } from \"../mdx-components\";\nimport { __mdx_h1 as Mdx_h1 } from \"../mdx-components\";\n"), "{}", c.tsx);
  assert_eq!(body(&c), "<Mdx_h1 id={\"hi\"}>{\"Hi\"}</Mdx_h1><p><Mdx_a href={\"/y\"}>{\"x\"}</Mdx_a></p>");
  assert_eq!(c.placed.iter().map(String::as_str).collect::<Vec<_>>(), ["a", "h1"]);
}

#[test]
fn a_wrapper_or_a_layout_wraps_the_content() {
  let mut o = options(Format::Mdx);
  o.mapped = vec![Mapped { key: "wrapper".into(), specifier: "./c".into(), export: "__mdx_wrapper".into() }];
  let c = convert("Hi\n", &o).expect("converts");
  assert!(c.tsx.contains("export default function MDXContent(props) {\n  return <Mdx_wrapper><>\n<p>{\"Hi\"}</p>\n</></Mdx_wrapper>;\n}"), "{}", c.tsx);
  assert!(!c.tsx.contains("...props"), "nothing spreads the page's props: {}", c.tsx);

  let c = lowered("export default function Layout({ children }) { return <main>{children}</main> }\n\nHi\n");
  assert!(c.tsx.starts_with("function Layout({ children }) { return <main>{children}</main> }\n"), "{}", c.tsx);
  assert!(c.tsx.contains("return <Layout><>\n<p>{\"Hi\"}</p>\n</></Layout>;"), "{}", c.tsx);

  let c = lowered("export default (props) => <main>{props.children}</main>\n\nHi\n");
  assert!(c.tsx.starts_with("const MDXLayout = (props) => <main>{props.children}</main>\n"), "{}", c.tsx);
  assert!(c.tsx.contains("return <MDXLayout><>"), "{}", c.tsx);
}

#[test]
fn the_browser_module_reads_the_map_at_render_time() {
  let mut o = Options::new(Format::Mdx, Target::Browser);
  o.provider = Some("../mdx-components".into());
  let c = convert("# Hi\n", &o).expect("converts");
  assert!(c.tsx.contains("import { useMDXComponents as _provideComponents } from \"../mdx-components\";"), "{}", c.tsx);
  assert!(c.tsx.contains("const _components = { h1: \"h1\", ..._provideComponents(), ...props.components };"), "{}", c.tsx);
  assert!(c.tsx.contains("<_components.h1 id={\"hi\"}>{\"Hi\"}</_components.h1>"), "{}", c.tsx);
  assert!(c.tsx.contains("const { wrapper: MDXWrapper } = { ..._provideComponents(), ...props.components };"), "{}", c.tsx);
  o.provider = None;
  let c = convert("# Hi\n", &o).expect("converts");
  assert!(c.tsx.contains("const _components = { h1: \"h1\", ...props.components };"), "{}", c.tsx);
}

#[test]
fn a_class_is_written_as_the_application_spells_it() {
  let mut o = options(Format::Mdx);
  o.class_attr = ClassAttr::Class;
  let c = convert("```ts\nx\n```\n", &o).expect("converts");
  assert!(body(&c).contains("<code class={\"language-ts\"}>"), "{}", body(&c));
}

#[test]
fn md_is_markdown_alone_and_leaves_raw_html_out_with_a_warning() {
  let c = convert("# A\n\n<div>raw</div>\n\n{not an expression}\n", &options(Format::Md)).expect("converts");
  assert_eq!(body(&c), "<h1 id={\"a\"}>{\"A\"}</h1><p>{\"{not an expression}\"}</p>");
  assert_eq!(c.warnings.len(), 1);
  assert_eq!((c.warnings[0].line, c.warnings[0].column), (3, 1));
  assert!(c.warnings[0].message.contains("raw HTML"), "{}", c.warnings[0].message);
  assert_eq!(Format::of("content/a.md"), Some(Format::Md));
  assert_eq!(Format::of("content/a.MDX"), Some(Format::Mdx));
  assert_eq!(Format::of("content/a.tsx"), None);
}

#[test]
fn every_written_byte_maps_back_to_the_source() {
  let source = "# Title\n\nText {value.missing}\n\n<Card n={broken.call()} />\n";
  let c = lowered(source);
  let expr = c.tsx.find("value.missing").expect("written");
  assert_eq!(c.origin(source, expr), (3, 7));
  assert_eq!(c.origin(source, expr + "value.".len()), (3, 13));
  let attr = c.tsx.find("broken.call").expect("written");
  assert_eq!(c.origin(source, attr), (5, 10));
  let heading = c.tsx.find("<h1").expect("written");
  assert_eq!(c.origin(source, heading), (1, 1));
}

#[test]
fn an_expression_inside_a_block_quote_maps_through_its_prefixes() {
  let source = "> {a +\n> b}\n";
  let c = lowered(source);
  let b = c.tsx.find("b\n}").expect("written");
  assert_eq!(c.origin(source, b), (2, 3));
}

#[test]
fn an_imported_name_in_esm_maps_to_its_line() {
  let source = "import { A } from \"./a\"\n\nexport const meta = { title: missing }\n";
  let c = lowered(source);
  let at = c.tsx.find("missing").expect("written");
  assert_eq!(c.origin(source, at), (3, 30));
}

#[test]
fn fence_meta_reads_a_title_marked_lines_and_line_numbers() {
  let fence = Fence::read("title=\"a b.ts\" {1,3-4} showLineNumbers");
  assert_eq!(fence.title.as_deref(), Some("a b.ts"));
  assert_eq!(fence.marked.iter().copied().collect::<Vec<_>>(), [1, 3, 4]);
  assert!(fence.line_numbers);
  assert_eq!(Fence::read(""), Fence::default());
}

#[test]
fn a_feature_left_out_is_named() {
  let mut o = options(Format::Mdx);
  o.highlight = true;
  o.frontmatter = true;
  match (cfg!(feature = "highlight"), cfg!(feature = "frontmatter")) {
    (false, _) => assert_eq!(convert("a", &o).expect_err("refused"), Error::Feature("highlighted code", "highlight")),
    (true, false) => assert_eq!(convert("a", &o).expect_err("refused"), Error::Feature("frontmatter", "frontmatter")),
    (true, true) => assert!(convert("a", &o).is_ok()),
  }
}

#[cfg(feature = "frontmatter")]
#[test]
fn frontmatter_becomes_the_meta_export() {
  let mut o = options(Format::Md);
  o.frontmatter = true;
  let c = convert("---\ntitle: Hello\ntags: [a]\n---\n\n# Hi\n", &o).expect("converts");
  assert!(c.tsx.starts_with("export const meta = {\"tags\":[\"a\"],\"title\":\"Hello\"};\n"), "{}", c.tsx);
  assert!(c.meta);
  let c = convert("+++\ntitle = \"Toml\"\n+++\n\nx\n", &o).expect("converts");
  assert!(c.tsx.starts_with("export const meta = {\"title\":\"Toml\"};\n"), "{}", c.tsx);

  let mut o = options(Format::Mdx);
  o.frontmatter = true;
  let error = convert("---\ntitle: a\n---\n\nexport const meta = { title: \"b\" }\n", &o).expect_err("refused");
  assert!(error.to_string().contains("keep one"), "{error}");
  let error = convert("---\ntitle: [a\n---\n", &o).expect_err("refused");
  assert!(error.to_string().starts_with("1:1: frontmatter:"), "{error}");
}

#[test]
fn without_the_frontmatter_option_a_fence_of_dashes_is_markdown() {
  let c = convert("---\ntitle: a\n---\n", &options(Format::Md)).expect("converts");
  assert_eq!(body(&c), "<hr /><h2 id={\"title-a\"}>{\"title: a\"}</h2>");
}

#[cfg(feature = "highlight")]
#[test]
fn highlighted_code_is_classed_spans_per_line() {
  let mut o = options(Format::Mdx);
  o.highlight = true;
  let c = convert("```ts title=\"a.ts\" {2}\nconst a = 1;\nlet b = a;\n```\n\n```text\nplain\n```\n", &o).expect("converts");
  let body = body(&c);
  assert!(body.starts_with("<figure data-rehype-pretty-code-figure={\"\"}><figcaption data-rehype-pretty-code-title={\"\"} data-language={\"ts\"}>{\"a.ts\"}</figcaption><pre className={\"hl-code\"} data-language={\"ts\"}><code data-language={\"ts\"}><span data-line={\"\"}><span className={\"hl-source hl-ts\"}>"), "{body}");
  assert!(body.contains("{\"\\n\"}<span data-line={\"\"} data-highlighted-line={\"\"}>"), "{body}");
  assert!(body.contains("<pre data-language={\"text\"}><code className={\"language-text\"} data-language={\"text\"}>{\"plain\\n\"}</code></pre>"), "{body}");
  assert!(c.classes.contains("hl-storage") && c.classes.contains("hl-ts"), "{:?}", c.classes);
}

#[cfg(feature = "highlight")]
#[test]
fn a_fence_with_no_grammar_keeps_its_title_and_language() {
  let mut o = options(Format::Mdx);
  o.highlight = true;
  let c = convert("```tera title=\"app/routes/page.tera\"\n<ul>{{ x }}</ul>\n```\n", &o).expect("converts");
  let body = body(&c);
  assert!(body.contains("<figcaption data-rehype-pretty-code-title={\"\"} data-language={\"tera\"}>{\"app/routes/page.tera\"}</figcaption>"), "{body}");
  assert!(body.contains("<pre data-language={\"tera\"}><code className={\"language-tera\"} data-language={\"tera\"}>"), "{body}");
}

/// REQUESTS 11.53: an index had no way to read a file's opening paragraph,
/// since the body is a component and not data.
#[test]
fn the_excerpt_is_the_first_paragraph_as_plain_text() {
  let c = lowered("import { Card } from \"./card\"\nexport const meta = { title: \"T\" }\n\n# Title\n\n{meta.title}\n\nThe *first* paragraph,\nwith `code` and [a link](/x).\n\nThe second.\n");
  assert_eq!(c.excerpt.as_deref(), Some("The first paragraph, with code and a link."));
  assert_eq!(lowered("# Only a heading\n").excerpt, None);
}

#[cfg(feature = "frontmatter")]
#[test]
fn the_excerpt_skips_frontmatter() {
  let mut o = options(Format::Md);
  o.frontmatter = true;
  assert_eq!(convert("---\ntitle: x\n---\n\nAfter the fence.\n", &o).expect("converts").excerpt.as_deref(), Some("After the fence."));
}
