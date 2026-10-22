//! Array construction and statements a component body or a helper writes,
//! each rendered on the server as the browser would compute it.

use std::path::PathBuf;
use std::sync::Arc;

use snapfire_fsr_core::ValueMap;
use snapfire_fsr_ir::render::{prepare, Components};
use snapfire_fsr_ir::Interpreter;
use snapfire_fsr_lower::component::ComponentSet;

fn app(tag: &str, files: &[(&str, &str)]) -> PathBuf {
  let dir = std::env::temp_dir().join(format!("fsr_statements_{}_{tag}", std::process::id()));
  let _ = std::fs::remove_dir_all(&dir);
  for (name, text) in files {
    let path = dir.join(name);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, text).unwrap();
  }
  dir
}

fn html(tag: &str, page: &str) -> Result<String, String> {
  html_files(tag, &[("routes/page.tsx", page)])
}

fn html_files(tag: &str, files: &[(&str, &str)]) -> Result<String, String> {
  let dir = app(tag, files);
  let mut set = ComponentSet::new(&dir);
  let result = set.lower("routes/page.tsx#default").map_err(|e| e.to_string());
  std::fs::remove_dir_all(&dir).unwrap();
  result?;
  set.settle_fills();
  if let Some((module, residue)) = set.browser_only.first() {
    return Err(format!("{module}: {}", residue.message));
  }
  let library: Components = set.components.iter().map(|(module, component)| (module.clone(), Arc::new(prepare(component)))).collect();
  let module = "routes/page.tsx#default";
  Interpreter::default().with_consts(Some(Arc::new(set.consts.clone()))).render_module(module, &library[module], &ValueMap::default(), &library).map(|r| r.html).map_err(|e| format!("{e:?}"))
}

#[test]
fn array_of_a_length_filled_holds_the_value_at_every_index() {
  let page = "import { useState } from \"react\";\nexport default function Page() {\n  const [squares, setSquares] = useState(Array(3).fill(null));\n  const marks = new Array(2).fill(\"x\");\n  return <p onClick={() => setSquares(squares)}>{squares.length}:{squares.map((s, i) => <i key={i}>{s === null ? \"-\" : s}</i>)}:{marks.join(\"\")}</p>;\n}\n";
  assert_eq!(html("fill", page).as_deref(), Ok("<p>3:<i>-</i><i>-</i><i>-</i>:xx</p>"));
}

#[test]
fn array_of_a_length_without_fill_is_not_lowered() {
  let page = "export default function Page() {\n  const holes = Array(3);\n  return <p>{holes.length}</p>;\n}\n";
  assert!(html("holes", page).is_err());
}

const WINNER: &str = "function calculateWinner(squares) {\n  const lines = [[0, 1, 2], [3, 4, 5], [0, 3, 6]];\n  for (let i = 0; i < lines.length; i++) {\n    const [a, b, c] = lines[i];\n    if (squares[a] && squares[a] === squares[b] && squares[a] === squares[c]) {\n      return squares[a];\n    }\n  }\n  return null;\n}\n";

fn board(squares: &str) -> String {
  format!("{WINNER}export default function Page() {{\n  const winner = calculateWinner({squares});\n  return <p>{{winner ? \"Winner: \" + winner : \"Next\"}}</p>;\n}}\n")
}

#[test]
fn a_loop_that_returns_gives_the_first_value_returned() {
  assert_eq!(html("win_row", &board("[\"X\", \"X\", \"X\", \"O\", \"O\", null, null, null, null]")).as_deref(), Ok("<p>Winner: X</p>"));
  assert_eq!(html("win_col", &board("[\"O\", \"X\", \"X\", \"O\", \"X\", null, \"O\", null, null]")).as_deref(), Ok("<p>Winner: O</p>"));
  assert_eq!(html("win_none", &board("[\"O\", \"X\", null, null, null, null, null, null, null]")).as_deref(), Ok("<p>Next</p>"));
}

#[test]
fn what_follows_a_return_in_an_iteration_runs_only_while_nothing_returned() {
  let page = "function firstBig(xs) {\n  let seen = 0;\n  for (const x of xs) {\n    if (x > 10) {\n      return x + \"@\" + seen;\n    }\n    seen = seen + 1;\n  }\n  return \"none@\" + seen;\n}\nexport default function Page() {\n  return <p>{firstBig([1, 2, 30, 40])}|{firstBig([1, 2])}</p>;\n}\n";
  assert_eq!(html("after_return", page).as_deref(), Ok("<p>30@2|none@2</p>"));
}

#[test]
fn for_each_runs_its_callback_over_every_item_and_return_skips_the_rest_of_one() {
  let page = "const products = [{ name: \"a\", stocked: true }, { name: \"b\", stocked: false }, { name: \"c\", stocked: true }];\nexport default function Page() {\n  const rows = [];\n  products.forEach((product, i) => {\n    if (!product.stocked) {\n      return;\n    }\n    rows.push(<li key={product.name}>{i}{product.name}</li>);\n  });\n  return <ul>{rows}</ul>;\n}\n";
  assert_eq!(html("for_each", page).as_deref(), Ok("<ul><li>0a</li><li>2c</li></ul>"));
}

#[test]
fn reverse_sort_pop_and_shift_rewrite_the_local_they_are_called_on() {
  let page = "export default function Page() {\n  const a = [1, 2, 3];\n  a.reverse();\n  const b = [3, 1, 2];\n  b.sort((x, y) => x - y);\n  const c = [1, 2, 3];\n  c.pop();\n  c.shift();\n  return <p>{a.join(\",\")}|{b.join(\",\")}|{c.join(\",\")}</p>;\n}\n";
  assert_eq!(html("in_place", page).as_deref(), Ok("<p>3,2,1|1,2,3|2</p>"));
}

#[test]
fn a_callback_a_custom_hook_only_hands_to_the_browser_lowers_the_hook() {
  let interval = "import { useEffect } from \"react\";\nexport function useInterval(onTick, delay) {\n  useEffect(() => {\n    const id = setInterval(onTick, delay);\n    return () => clearInterval(id);\n  }, [onTick, delay]);\n}\n";
  let page = "import { useState } from \"react\";\nimport { useInterval } from \"@src/useInterval\";\nexport default function Page() {\n  const [count, setCount] = useState(0);\n  useInterval(() => {\n    setCount((c) => c + 1);\n  }, 1000);\n  return <h1>{count}</h1>;\n}\n";
  let dir = app("hook_callback", &[("src/useInterval.ts", interval), ("routes/page.tsx", page)]);
  let mut set = ComponentSet::new(&dir);
  let result = set.lower("routes/page.tsx#default");
  std::fs::remove_dir_all(&dir).unwrap();
  assert!(result.is_ok(), "{result:?}");
  assert!(set.browser_only.is_empty(), "{:?}", set.browser_only);
}

#[test]
fn a_numeric_property_name_is_read_back_by_number_or_by_string() {
  let page = "const plan = {\n  0: { title: \"Root\", childIds: [1, 2] },\n  1: { title: \"Earth\", childIds: [] },\n  2: { title: \"Moon\", childIds: [] },\n  1.5: { title: \"half\" },\n};\nexport default function Page() {\n  const root = plan[0];\n  return <ul>{root.childIds.map((id) => <li key={id}>{plan[id].title}</li>)}<li>{plan[\"2\"].title}</li><li>{plan[1.5].title}</li></ul>;\n}\n";
  assert_eq!(html("numeric", page).as_deref(), Ok("<ul><li>Earth</li><li>Moon</li><li>Moon</li><li>half</li></ul>"));
}

#[test]
fn a_loop_variable_named_like_a_markup_local_is_the_loop_variable() {
  let page = "function Card({ cta }: { cta: any }) {\n  return <section>{cta}</section>;\n}\nfunction renderCta(cta: any, key: number) {\n  return <a key={key} href=\"/x\">{cta.label}</a>;\n}\nfunction renderCtas(ctas: any[]) {\n  return <div>{ctas.map((cta, index) => renderCta(cta, index))}</div>;\n}\nexport default function Page() {\n  const top = [{ label: \"Top\" }];\n  const cards = [{ ctas: [{ label: \"Buy\" }, { label: \"More\" }] }];\n  const cta = renderCtas(top);\n  return <main><Card cta={cta} />{cards.map((card, i) => <Card key={i} cta={renderCtas(card.ctas)} />)}</main>;\n}\n";
  assert_eq!(html("loop_shadows_markup", page).as_deref(), Ok("<main><section><div><a href=\"/x\">Top</a></div></section><section><div><a href=\"/x\">Buy</a><a href=\"/x\">More</a></div></section></main>"));
}

#[test]
fn bitwise_operators_answer_as_javascript_does() {
  let page = "function Text({ format }: { format: number }) {\n  const bold = (format & 1) !== 0 ? <strong>x</strong> : \"x\";\n  const italic = (format & 2) !== 0 ? <em>{bold}</em> : bold;\n  return (format & 8) !== 0 ? <u>{italic}</u> : italic;\n}\nexport default function Page() {\n  let mask = 0;\n  mask |= 4;\n  mask |= 1;\n  const values = [5 & 3, 5 | 3, 5 ^ 3, ~5, 1 << 31, -16 >> 2, -16 >>> 2, 1 << 33, 2.9 | 0, -2.9 | 0, 4294967297 | 0, (1 << 31) >>> 0, mask];\n  return <p>{[0, 1, 2, 3, 8, 11].map((format) => <span key={format}><Text format={format} /></span>)}<i>{values.join(\",\")}</i></p>;\n}\n";
  assert_eq!(
    html("bitwise", page).as_deref(),
    Ok("<p><span>x</span><span><strong>x</strong></span><span><em>x</em></span><span><em><strong>x</strong></em></span><span><u>x</u></span><span><u><em><strong>x</strong></em></u></span><i>1,7,6,-6,-2147483648,-4,1073741820,2,2,-2,1,2147483648,5</i></p>")
  );
}

fn html_with(tag: &str, files: &[(&str, &str)]) -> Result<String, String> {
  let dir = app(tag, files);
  let mut set = ComponentSet::new(&dir);
  let result = set.lower("routes/page.tsx#default").map_err(|e| e.to_string());
  std::fs::remove_dir_all(&dir).unwrap();
  result?;
  set.settle_fills();
  if let Some((module, residue)) = set.browser_only.first() {
    return Err(format!("{module}: {}", residue.message));
  }
  let library: Components = set.components.iter().map(|(module, component)| (module.clone(), Arc::new(prepare(component)))).collect();
  let module = "routes/page.tsx#default";
  Interpreter::default().with_consts(Some(Arc::new(set.consts.clone()))).render_module(module, &library[module], &ValueMap::default(), &library).map(|r| r.html).map_err(|e| format!("{e:?}"))
}

#[test]
fn a_bare_return_renders_nothing() {
  assert_eq!(html("bare_return", "export default function Page() {\n  return\n    <img src=\"/a.png\" />;\n}\n").as_deref(), Ok(""));
}

#[test]
fn an_imported_component_in_a_markup_prop_is_placed_by_its_module() {
  use snapfire_fsr_ir::ast::{Expr, Tmpl};
  fn placed(t: &Tmpl, out: &mut Vec<String>) {
    match t {
      Tmpl::Component { module, children, .. } | Tmpl::Island { module, children, .. } => {
        out.push(module.clone());
        children.iter().for_each(|c| placed(c, out));
      }
      Tmpl::Element { children, .. } | Tmpl::Fragment(children) | Tmpl::Baked { children, .. } | Tmpl::Place { children, .. } => children.iter().for_each(|c| placed(c, out)),
      Tmpl::If { then, r#else, .. } => {
        placed(then, out);
        if let Some(e) = r#else {
          placed(e, out);
        }
      }
      Tmpl::For { body, .. } | Tmpl::Let { then: body, .. } => placed(body, out),
      _ => {}
    }
  }
  let rich = "export default function Rich({ value }: { value: string }) {\n  return <p>{value}</p>;\n}\n";
  let page = "import Rich from \"./Rich\";\nfunction Card({ body }: { body: any }) {\n  return <section>{body}</section>;\n}\nexport default function Page({ blocks }: { blocks: string[] }) {\n  return <main>{blocks.map((b, i) => <Card key={i} body={<Rich value={b} />} />)}</main>;\n}\n";
  let dir = app("markup_prop_import", &[("routes/Rich.tsx", rich), ("routes/page.tsx", page)]);
  let mut set = ComponentSet::new(&dir);
  let result = set.lower("routes/page.tsx#default");
  std::fs::remove_dir_all(&dir).unwrap();
  result.unwrap_or_else(|e| panic!("{e}"));
  let (_, component) = set.components.iter().find(|(m, _)| m == "routes/page.tsx#default").expect("the page lowered");
  let mut modules = Vec::new();
  component.render.visit(&mut |e| {
    if let Expr::Markup(t) = e {
      placed(t, &mut modules);
    }
  });
  assert_eq!(modules, ["routes/Rich.tsx#default"], "{:?}", component.render);
}

#[test]
fn children_passed_through_a_spread_render_inside_the_element() {
  let page = "function Button({ label, ...rest }: any) {\n  return <button {...rest} aria-label={label} />;\n}\nexport default function Page() {\n  return <Button label=\"x\" type=\"button\">Hover over me</Button>;\n}\n";
  assert_eq!(html("spread_children", page).as_deref(), Ok("<button type=\"button\" aria-label=\"x\">Hover over me</button>"));
  let page = "function Button(props: any) {\n  return <button {...props} />;\n}\nfunction Own({ label, ...rest }: any) {\n  return <b {...rest}>{label}</b>;\n}\nexport default function Page() {\n  return <p><Button type=\"button\">Go</Button><Own label=\"mine\">theirs</Own></p>;\n}\n";
  assert_eq!(html("spread_children_props", page).as_deref(), Ok("<p><button type=\"button\">Go</button><b>mine</b></p>"));
}

#[test]
fn use_after_an_early_return_reads_the_context_as_react_allows() {
  let page = "import { createContext, use } from \"react\";\nconst Theme = createContext(\"light\");\nfunction Label({ hidden }: { hidden: boolean }) {\n  if (hidden) {\n    return null;\n  }\n  const theme = use(Theme);\n  return <b>{theme}</b>;\n}\nexport default function Page() {\n  return <Theme.Provider value=\"dark\"><p><Label hidden={true} /><Label hidden={false} /></p></Theme.Provider>;\n}\n";
  assert_eq!(html("use_after_return", page).as_deref(), Ok("<p><b>dark</b></p>"));
}

#[test]
fn a_textarea_writes_its_value_as_its_text() {
  let page = "import { useState } from \"react\";\nexport default function Page() {\n  const [text, setText] = useState(\"Hello\");\n  return <p><textarea value={text} onChange={(e) => setText(e.target.value)} /><textarea placeholder=\"x\" defaultValue=\"d\" /><textarea defaultValue=\"d\" value=\"v\" readOnly /></p>;\n}\n";
  assert_eq!(html("textarea_value", page).as_deref(), Ok("<p><textarea>Hello</textarea><textarea placeholder=\"x\">d</textarea><textarea readonly=\"\">v</textarea></p>"));
}

#[test]
fn a_default_exported_value_is_read_where_it_is_imported() {
  let out = html_with("data_default", &[("src/quotes.ts", "export default [\"one\", \"two\"];\n"), ("routes/page.tsx", "import quotes from \"@src/quotes\";\nexport default function Page() {\n  return <p>{quotes[1]} of {quotes.length}</p>;\n}\n")]);
  assert_eq!(out.as_deref(), Ok("<p>two of 2</p>"));
}

#[test]
fn a_context_read_after_an_early_return_or_destructured_reads_the_provider() {
  let page = "import { createContext, use, useContext } from \"react\";\nconst Theme = createContext(\"light\");\nconst User = createContext(null);\nfunction Button({ show }) {\n  if (show) {\n    const theme = use(Theme);\n    return <b>{theme}</b>;\n  }\n  return null;\n}\nfunction Name() {\n  const { name } = useContext(User);\n  return <i>{name}</i>;\n}\nexport default function Page() {\n  return <Theme value=\"dark\"><User value={{ name: \"Ada\" }}><Button show={true} /><Button show={false} /><Name /></User></Theme>;\n}\n";
  assert_eq!(html("context_reads", page).as_deref(), Ok("<b>dark</b><i>Ada</i>"));
}

#[test]
fn has_on_a_set_held_in_state_reads_the_set() {
  let page = "import { useState } from \"react\";\nexport default function Page() {\n  const [picked, setPicked] = useState(new Set([1]));\n  return <p onClick={() => setPicked(new Set())}>{picked.has(1) ? \"one\" : \"none\"} {picked.has(2) ? \"two\" : \"none\"}</p>;\n}\n";
  assert_eq!(html("set_state", page).as_deref(), Ok("<p>one none</p>"));
}

#[test]
fn a_helper_may_write_its_own_parameter() {
  let page = "function filterItems(items, query) {\n  query = query.toLowerCase();\n  return items.filter((item) => item.startsWith(query));\n}\nexport default function Page() {\n  return <p>{filterItems([\"apple\", \"banana\"], \"AP\").join(\",\")}</p>;\n}\n";
  assert_eq!(html("param_write", page).as_deref(), Ok("<p>apple</p>"));
}

#[test]
fn a_function_a_custom_hook_declares_and_returns_in_an_object_lowers_the_hook() {
  let hook = "import { useState } from \"react\";\nexport function useFormInput(initial) {\n  const [value, setValue] = useState(initial);\n  function handleChange(e) {\n    setValue(e.target.value);\n  }\n  const inputProps = { value: value, onChange: handleChange };\n  return inputProps;\n}\n";
  let page = "import { useFormInput } from \"@src/useFormInput\";\nexport default function Page() {\n  const props = useFormInput(\"Ada\");\n  return <input {...props} />;\n}\n";
  assert_eq!(html_with("hook_fn", &[("src/useFormInput.ts", hook), ("routes/page.tsx", page)]).as_deref(), Ok("<input value=\"Ada\">"));
}

#[test]
fn a_helper_reading_the_date_clock_is_left_to_the_browser_as_an_inline_read_is() {
  let card = "function now() {\n  return Date.now();\n}\nexport default function Stamp() {\n  return <p>{now()}</p>;\n}\n";
  let page = "import Stamp from \"@src/Stamp\";\nexport default function Page() {\n  return <main><Stamp /></main>;\n}\n";
  let dir = app("date_helper", &[("src/Stamp.tsx", card), ("routes/page.tsx", page)]);
  let mut set = ComponentSet::new(&dir);
  let result = set.lower("routes/page.tsx#default");
  std::fs::remove_dir_all(&dir).unwrap();
  assert!(result.is_ok(), "{result:?}");
  assert!(!set.browser_only.iter().any(|(m, _)| m == "src/Stamp.tsx#default"), "{:?}", set.browser_only);
  let text = set.browser_texts.iter().find(|(m, _)| m == "src/Stamp.tsx#default").map(|(_, site)| site.clone());
  assert!(text.as_deref().is_some_and(|t| t.starts_with("src/Stamp.tsx:5:14: `now` reads the current time on a render path")), "{text:?}");
}

#[test]
fn a_helper_calling_time_now_itself_is_still_refused() {
  let page = "import { time } from \"@snapfire/fsr-client/std\";\nfunction now() {\n  return time.now();\n}\nexport default function Page() {\n  return <p>{now()}</p>;\n}\n";
  let dir = app("time_helper", &[("routes/page.tsx", page)]);
  let mut set = ComponentSet::new(&dir);
  let result = set.lower("routes/page.tsx#default").map_err(|e| e.to_string());
  std::fs::remove_dir_all(&dir).unwrap();
  assert!(result.as_ref().is_err_and(|e| e.contains("calls `time.now` on a render path")), "{result:?}");
}

#[test]
fn children_for_each_runs_over_what_the_placement_passed() {
  let list = "import { Children } from \"react\";\nexport default function SeparatorList({ children }) {\n  const result = [];\n  Children.forEach(children, (child, index) => {\n    result.push(child);\n    result.push(<hr key={index} />);\n  });\n  result.pop();\n  return result;\n}\n";
  let page = "import SeparatorList from \"@src/SeparatorList\";\nexport default function Page() {\n  return <SeparatorList><p>a</p><p>b</p></SeparatorList>;\n}\n";
  assert_eq!(html_with("children_each", &[("src/SeparatorList.tsx", list), ("routes/page.tsx", page)]).as_deref(), Ok("<p>a</p><hr><p>b</p>"));
}

#[test]
fn a_page_whose_default_export_is_an_imported_component_renders_that_component() {
  let card = "export function Card({ title = \"t\" }: { title?: string }) {\n  return <h1>{title}</h1>;\n}\n";
  let page = "import { Card } from \"@src/Card\";\nexport default Card;\n";
  assert_eq!(html_with("default_reexport", &[("src/Card.tsx", card), ("routes/page.tsx", page)]).as_deref(), Ok("<h1>t</h1>"));
}

#[test]
fn undefined_and_null_compare_and_type_as_javascript_has_them() {
  let page = "function Show({ v }: { v?: number | null }) {\n  return <i>{v === undefined ? \"u\" : v === null ? \"n\" : \"v\"}{v == null ? \"!\" : \".\"}{typeof v}{String(v)}</i>;\n}\nexport default function Page() {\n  const o: { a: null; b?: number } = { a: null };\n  return <p><Show v={o.a} /><Show v={o.b} /><Show v={1} /></p>;\n}\n";
  assert_eq!(html("undefined_null", page).as_deref(), Ok("<p><i>n!objectnull</i><i>u!undefinedundefined</i><i>v.number1</i></p>"));
}

#[test]
fn a_local_an_array_callback_writes_holds_each_write_for_the_next_call_and_after_the_call() {
  let page = "function colorGuess(solution: string, _guess: string) {\n  const remainingSolution = Array.from(solution);\n\n  const letters = Array.from(_guess)\n    .map((c, i) => {\n      if (remainingSolution[i] == c) {\n        remainingSolution[i] = \"_\";\n        return { color: \"green\" as const, letter: c };\n      } else {\n        return { color: \"not-green\" as const, letter: c };\n      }\n    })\n    .map((c) => {\n      switch (c.color) {\n        case \"green\":\n          return c;\n        case \"not-green\": {\n          const n = remainingSolution.indexOf(c.letter);\n          if (n == -1) {\n            return { color: \"gray\" as const, letter: c.letter };\n          } else {\n            remainingSolution[n] = \"_\";\n            return { color: \"yellow\" as const, letter: c.letter };\n          }\n        }\n      }\n    });\n\n  return { letters };\n}\n\nfunction total(ns: number[]) {\n  let sum = 0;\n  const doubled = ns.map((n) => {\n    sum += n;\n    return n * 2;\n  });\n  return `${doubled.join(\",\")}=${sum}`;\n}\n\nexport default function Page() {\n  const { letters } = colorGuess(\"ABBEY\", \"BABES\");\n  return <p>{letters.map((l) => `${l.letter}:${l.color}`).join(\" \")}|{total([1, 2, 3])}</p>;\n}\n";
  assert_eq!(html("callback_writes", page).as_deref(), Ok("<p>B:yellow A:yellow B:green E:green S:gray|2,4,6=6</p>"));
}

#[test]
fn array_from_a_string_is_its_code_points() {
  let page = "export default function Page() {\n  const letters = Array.from(\"ab👍\");\n  return <p>{letters.length}:{letters.map((c) => `[${c}]`).join(\"\")}</p>;\n}\n";
  assert_eq!(html("array_from_string", page).as_deref(), Ok("<p>3:[a][b][👍]</p>"));
}

#[test]
fn reduce_without_an_initial_value_starts_from_the_first_item_and_throws_on_an_empty_array() {
  let page = "const stringSum = (s: string) =>\n  Array.from(s)\n    .map((c) => c.charCodeAt(0))\n    .reduce((a, b) => a + b);\n\nexport default function Page() {\n  const indexed = [\"a\", \"b\", \"c\"].reduce((acc, x, i, all) => acc + i + x + all.length);\n  return <p>{stringSum(\"AB\")}|{indexed}</p>;\n}\n";
  assert_eq!(html("reduce_no_init", page).as_deref(), Ok("<p>131|a1b32c3</p>"));
  let empty = "export default function Page() {\n  const none: number[] = [];\n  return <p>{none.reduce((a, b) => a + b)}</p>;\n}\n";
  let failed = html("reduce_empty", empty).unwrap_err();
  assert!(failed.contains("Reduce of empty array with no initial value"), "{failed}");
}

#[test]
fn split_on_an_empty_separator_is_the_characters_and_an_astral_one_fails() {
  let page = "export default function Page() {\n  const sep = \"\";\n  const all = \"h\u{e9}llo\".split(\"\");\n  return <p>{all.join(\"-\")}|{\"abc\".split(sep, 2).join(\"-\")}|{\"\".split(\"\").length}</p>;\n}\n";
  assert_eq!(html("split_empty", page).as_deref(), Ok("<p>h-\u{e9}-l-l-o|a-b|0</p>"));
  let astral = "export default function Page() {\n  return <p>{\"a\u{1F600}\".split(\"\").length}</p>;\n}\n";
  let failed = html("split_astral", astral).unwrap_err();
  assert!(failed.contains("outside the Basic Multilingual Plane"), "{failed}");
}

#[test]
fn a_custom_hook_returns_or_throws_early_from_a_branch() {
  let page = "import { createContext, useContext } from \"react\";\n\nconst Count = createContext<{ n: number } | null>({ n: 2 });\n\nfunction useCount() {\n  const count = useContext(Count);\n  if (!count) {\n    throw new Error(\"useCount must be used within a provider\");\n  }\n  return count.n;\n}\n\nfunction useLabel(n: number) {\n  if (n > 5) return \"big\";\n  const doubled = n * 2;\n  return `small ${doubled}`;\n}\n\nexport default function Page() {\n  const n = useCount();\n  const small = useLabel(n);\n  const big = useLabel(9);\n  return <p>{n}|{small}|{big}</p>;\n}\n";
  assert_eq!(html("hook_early", page).as_deref(), Ok("<p>2|small 4|big</p>"));
  let missing = page.replace("({ n: 2 })", "(null)");
  let failed = html("hook_early_throw", &missing).unwrap_err();
  assert!(failed.contains("useCount must be used within a provider"), "{failed}");
}

#[test]
fn a_let_declared_without_a_value_is_undefined() {
  let page = "export default function Page() {\n  let local: number | undefined;\n  let set: number | undefined;\n  set = 1;\n  return <p>{local === undefined ? \"undefined\" : \"null\"}|{set}</p>;\n}\n";
  assert_eq!(html("let_undefined", page).as_deref(), Ok("<p>undefined|1</p>"));
}

#[test]
fn a_string_indexed_by_a_number_is_the_character_there() {
  let page = "export default function Page() {\n  const word = \"h\u{e9}llo\";\n  const i = 4;\n  return <p>{word[1]}{word[i]}|{word[9] === undefined ? \"undefined\" : \"set\"}</p>;\n}\n";
  assert_eq!(html("string_index", page).as_deref(), Ok("<p>\u{e9}o|undefined</p>"));
  let astral = "export default function Page() {\n  const face = \"a\u{1F600}\";\n  return <p>{face[1]}</p>;\n}\n";
  let failed = html("string_index_astral", astral).unwrap_err();
  assert!(failed.contains("surrogate"), "{failed}");
}

#[test]
fn a_json_file_imports_as_its_value() {
  let page = "import words from \"./words.json\";\nimport pkg from \"../package.json\";\n\nexport default function Page() {\n  return <p>{words.greeting[1]}|{pkg.version}</p>;\n}\n";
  let files = [("routes/page.tsx", page), ("routes/words.json", "{\n  \"greeting\": [\"hello\", \"bonjour\"]\n}\n"), ("package.json", "{ \"name\": \"app\", \"version\": \"1.2.0\" }\n")];
  assert_eq!(html_files("json_import", &files).as_deref(), Ok("<p>bonjour|1.2.0</p>"));
}

#[test]
fn a_thrown_type_error_fails_the_render_with_its_message() {
  let page = "function tone(name: string) {\n  if (name !== \"none\") {\n    throw new TypeError(`Unexpected tone: ${name}`);\n  }\n  return name;\n}\n\nexport default function Page() {\n  return <p>{tone(\"blue\")}</p>;\n}\n";
  let failed = html("type_error", page).unwrap_err();
  assert!(failed.contains("Unexpected tone: blue"), "{failed}");
}

#[test]
fn a_spread_of_an_array_of_a_length_is_that_many_undefineds() {
  let page = "export default function Page() {\n  const n = 3;\n  const blanks = [...Array(n)].map(() => \"_\");\n  const made = [...new Array(2)];\n  return <p>{blanks.join(\"\")}|{made.length}|{made[0] === undefined ? \"undefined\" : \"null\"}</p>;\n}\n";
  assert_eq!(html("spread_array_length", page).as_deref(), Ok("<p>___|2|undefined</p>"));
}

#[test]
fn an_array_added_to_a_value_is_its_items_joined_by_commas() {
  let page = "export default function Page() {\n  const letters = [\"c\", \"d\"];\n  const nested = [1, [2, null]];\n  const none: string[] = [];\n  return <p>{\"ab\" + letters}|{nested + 1}|{none + \"\"}|{letters + letters}</p>;\n}\n";
  assert_eq!(html("array_plus", page).as_deref(), Ok("<p>abc,d|1,2,1||c,dc,d</p>"));
}
