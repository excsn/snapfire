//! Classes an application declares render on the server: an instance is an object of its fields and its members run where a call names them.

use std::path::PathBuf;
use std::sync::Arc;

use snapfire_fsr_core::ValueMap;
use snapfire_fsr_ir::render::{prepare, Components};
use snapfire_fsr_ir::Interpreter;
use snapfire_fsr_lower::component::ComponentSet;

fn app(tag: &str, files: &[(&str, &str)]) -> PathBuf {
  let dir = std::env::temp_dir().join(format!("fsr_classes_{}_{tag}", std::process::id()));
  let _ = std::fs::remove_dir_all(&dir);
  for (name, text) in files {
    let path = dir.join(name);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, text).unwrap();
  }
  dir
}

fn html(tag: &str, files: &[(&str, &str)]) -> Result<String, String> {
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

const SHAPES: &str = r#"export abstract class Shape {
  protected constructor(readonly name: string) {}

  get label(): string {
    return `${this.name} of ${this.area()}`;
  }

  describe(prefix: string): string {
    return `${prefix}: ${this.label}`;
  }
}

export class Square extends Shape {
  static unit = 1;
  sides = 4;

  private constructor(private readonly side: number) {
    super("square");
  }

  static of(side: number): Square {
    return new Square(side * Square.unit);
  }

  area(): number {
    return this.side * this.side;
  }

  grown(by: number): Square {
    return new Square(this.side + by);
  }
}

export class Circle extends Shape {
  constructor(private readonly r: number) {
    super("circle");
  }

  area(): number {
    return this.r * 3;
  }

  describe(prefix: string): string {
    return `(${super.describe(prefix)})`;
  }
}
"#;

#[test]
fn an_instance_answers_its_fields_getters_and_methods_through_its_class() {
  let page = "import { Square, Circle, Shape } from \"../src/shapes\";\n\nexport default function Page() {\n  const square = Square.of(2).grown(1);\n  const circle = new Circle(2);\n  return <ul><li>{square.describe(\"a\")}</li><li>{circle.describe(\"b\")}</li><li>{square.sides} {square.name}</li><li>{square instanceof Shape ? \"shape\" : \"no\"} {circle instanceof Square ? \"square\" : \"no\"}</li></ul>;\n}\n";
  let out = html("shapes", &[("src/shapes.ts", SHAPES), ("routes/page.tsx", page)]);
  assert_eq!(out.as_deref(), Ok("<ul><li>a: square of 9</li><li>(b: circle of 6)</li><li>4 square</li><li>shape no</li></ul>"));
}

const RESULT: &str = r#"class Ok<T> {
  constructor(readonly value: T) {}
  isOk(): boolean {
    return true;
  }
  getOrThrow(): T {
    return this.value;
  }
  getOrElse(fallback: T): T {
    return this.value;
  }
}

class Err<T> {
  constructor(readonly error: Error) {}
  isOk(): boolean {
    return false;
  }
  getOrThrow(): T {
    throw this.error;
  }
  getOrElse(fallback: T): T {
    return fallback;
  }
}

export type Result<T> = Ok<T> | Err<T>;
export const ok = <T,>(value: T): Result<T> => new Ok(value);
export const err = <T,>(error: Error): Result<T> => new Err(error);
"#;

const LINK: &str = r#"import { err, ok, Result } from "./result";

export class Link {
  private constructor(private readonly url: URL) {}

  static parse(text: string): Result<Link> {
    try {
      return ok(new Link(new URL(text)));
    } catch (e: unknown) {
      return err(e as Error);
    }
  }

  get href(): string {
    return this.url.href;
  }

  withPath(path: string): Link {
    const url = new URL(this.url.href);
    url.pathname = path;
    return new Link(url);
  }
}
"#;

#[test]
fn a_static_factory_answering_one_of_two_classes_dispatches_on_the_one_it_made() {
  let page = "import { Link } from \"../src/link\";\n\nconst home = Link.parse(\"https://www.example.com\").getOrThrow();\n\nexport default function Page() {\n  const broken = Link.parse(\"not a link\");\n  return <p>{home.withPath(\"/gift\").href} {broken.isOk() ? \"ok\" : \"bad\"} {broken.getOrElse(home).href}</p>;\n}\n";
  let out = html("result", &[("src/result.ts", RESULT), ("src/link.ts", LINK), ("routes/page.tsx", page)]);
  assert_eq!(out.as_deref(), Ok("<p>https://www.example.com/gift bad https://www.example.com/</p>"));
}

const PARAMS: &str = r#"export class Params {
  private constructor(private readonly entries: Record<string, string>) {}

  static of(entries: Record<string, string>): Params {
    return new Params(entries);
  }

  get(name: string): string | null {
    return this.entries[name] ?? null;
  }
}
"#;

#[test]
fn a_helper_typed_with_a_class_it_imports_as_a_type_calls_its_methods() {
  let helper = "import type { Params } from \"./params\";\n\nexport const refOf = (params: Params): string => params.get(\"ref\") ?? \"none\";\n";
  let page = "import { refOf } from \"../src/helper\";\nimport { Params } from \"../src/params\";\n\nexport default function Page() {\n  return <p>{refOf(Params.of({ ref: \"gift\" }))}</p>;\n}\n";
  let out = html("type_import", &[("src/params.ts", PARAMS), ("src/helper.ts", helper), ("routes/page.tsx", page)]);
  assert_eq!(out.as_deref(), Ok("<p>gift</p>"));
}

const BRAND: &str = r#"declare const brand: unique symbol;
export type Branded<Name extends string, T> = T & { readonly [brand]: Name };
export const brandAs = <Name extends string, T>(x: T): Branded<Name, T> => x as Branded<Name, T>;
"#;

const SAFE: &str = r#"import { brandAs, Branded } from "./brand";

export type Owned<T> = Branded<"Owned", T>;

export class BadLink extends Error {
  constructor(readonly text: string) {
    super(`not a link: ${text}`, { cause: text });
  }
}

const copyOf = (url: URL): Owned<URL> => brandAs(new URL(url));

const scrub = <T,>(url: Owned<URL>, make: (url: Owned<URL>) => T): T => {
  url.password = "";
  url.username = "";
  return make(url);
};

export class SafeLink {
  private constructor(private readonly url: Owned<URL>) {}

  private static make(this: void, url: Owned<URL>): SafeLink {
    url.port = "";
    return new SafeLink(url);
  }

  static parse(text: string): SafeLink {
    try {
      return scrub(copyOf(new URL(text)), SafeLink.make);
    } catch {
      throw new BadLink(text);
    }
  }

  get href(): string {
    return this.url.href;
  }

  withPath(path: string): SafeLink {
    const url = copyOf(this.url);
    url.pathname = path;
    return new SafeLink(url);
  }

  toString(): string {
    return this.url.toString();
  }
}
"#;

#[test]
fn a_wrapper_over_url_keeps_its_kind_through_a_branded_alias_and_a_function_returning_one() {
  let page = "import { SafeLink, BadLink } from \"../src/safe\";\n\nconst home = SafeLink.parse(\"https://me:pw@www.example.com:8080/x\");\n\nconst failure = (text: string): string => {\n  try {\n    return SafeLink.parse(text).href;\n  } catch (e) {\n    return e instanceof BadLink ? `${e.name}: ${e.message}` : \"other\";\n  }\n};\n\nexport default function Page() {\n  return <p>{home.withPath(\"/gift\").href} {home.toString()} {failure(\"nope\")}</p>;\n}\n";
  let out = html("brand", &[("src/brand.ts", BRAND), ("src/safe.ts", SAFE), ("routes/page.tsx", page)]);
  assert_eq!(out.as_deref(), Ok("<p>https://www.example.com/gift https://www.example.com/x BadLink: not a link: nope</p>"));
}

#[test]
fn a_second_render_calls_the_methods_the_first_evaluated() {
  let page = "import { Link } from \"../src/link\";\n\nexport default function Page() {\n  return <p>{Link.parse(\"https://www.example.com\").getOrThrow().withPath(\"/a\").href}</p>;\n}\n";
  let dir = app("twice", &[("src/result.ts", RESULT), ("src/link.ts", LINK), ("routes/page.tsx", page)]);
  let mut set = ComponentSet::new(&dir);
  set.lower("routes/page.tsx#default").unwrap();
  std::fs::remove_dir_all(&dir).unwrap();
  let library: Components = set.components.iter().map(|(module, component)| (module.clone(), Arc::new(prepare(component)))).collect();
  let module = "routes/page.tsx#default";
  let interpreter = Interpreter::default().with_consts(Some(Arc::new(set.consts.clone())));
  for _ in 0..2 {
    let html = interpreter.render_module(module, &library[module], &ValueMap::default(), &library).map(|r| r.html).map_err(|e| format!("{e:?}"));
    assert_eq!(html.as_deref(), Ok("<p>https://www.example.com/a</p>"));
  }
}
