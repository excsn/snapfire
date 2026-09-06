import { decodeValue, SfValue } from "./values.js";

export type SfNode =
  | { kind: "text"; text: string }
  | { kind: "raw"; html: string }
  | { kind: "seq"; children: SfNode[] }
  | {
      kind: "client";
      module: string;
      props: { [key: string]: SfValue };
      children: SfNode[];
      ssr: SfNode | null;
    }
  | { kind: "pending"; slot: number; fallback: SfNode };

export interface Segment {
  k: string;
  /** The slot this segment fills in its parent; absent at the root. */
  n?: string;
  p?: number[];
  s?: number;
  c: Segment[];
  /** Slots of this segment the payload left unfilled and the browser keeps as they stand. */
  keep?: string[];
}

/** What a route says about the document; a field left out keeps what the document has. */
export interface Head {
  title?: string;
  description?: string;
}

export interface Payload {
  format: number;
  encoding: string;
  tree: SfNode;
  segments: Segment | null;
  /** The document's title and description, from the eager wave then from each resolution that set them, in order. */
  heads: Head[];
  /** The store keys the route seeds, from the eager wave then from each resolution that seeded, in order. */
  seeds: { [key: string]: SfValue }[];
  /** The locale the response was rendered in, as the application spells it; null when the server has none. */
  locale: string | null;
  /** The locale's message table, a `D` row, sent when the request did not say it already holds it; null otherwise. */
  catalog: { [key: string]: string } | null;
  /** A module to load before this response's islands can mount, a mounted site's entry; null when the document's own entry covers them. */
  entry: string | null;
  resolutions: { slot: number; node: SfNode }[];
}

/** One row of a payload, by its tag. */
export type Row =
  | { tag: "V"; format: number; encoding: string }
  | { tag: "N"; tree: SfNode }
  | { tag: "G"; segments: Segment }
  | { tag: "H"; head: Head }
  | { tag: "T"; seed: { [key: string]: SfValue } }
  | { tag: "L"; locale: string }
  | { tag: "E"; entry: string }
  | { tag: "D"; catalog: { [key: string]: string } }
  | { tag: "S"; slot: number; node: SfNode };

export function decodeNode(row: unknown): SfNode {
  const arr = row as unknown[];
  switch (arr[0]) {
    case "t":
      return { kind: "text", text: arr[1] as string };
    case "r":
      return { kind: "raw", html: arr[1] as string };
    case "q":
      return { kind: "seq", children: (arr[1] as unknown[]).map(decodeNode) };
    case "c": {
      const body = arr[1] as { [key: string]: unknown };
      return {
        kind: "client",
        module: body["m"] as string,
        props: decodeValue(body["p"]) as { [key: string]: SfValue },
        children: ((body["ch"] as unknown[]) ?? []).map(decodeNode),
        ssr: body["s"] == null ? null : decodeNode(body["s"]),
      };
    }
    case "p":
      return { kind: "pending", slot: arr[1] as number, fallback: decodeNode(arr[2]) };
    default:
      throw new Error(`unknown node row kind: ${arr[0]}`);
  }
}

/** Reads one row: its tag, a space, then its body. Throws on a tag the grammar lacks. */
export function parseRow(line: string): Row {
  const tag = line[0];
  switch (tag) {
    case "V": {
      const v = JSON.parse(line.slice(2));
      return { tag, format: v.fmt, encoding: v.enc };
    }
    case "N":
      return { tag, tree: decodeNode(JSON.parse(line.slice(2))) };
    case "G":
      return { tag, segments: JSON.parse(line.slice(2)) };
    case "H":
      return { tag, head: JSON.parse(line.slice(2)) };
    case "T":
      return { tag, seed: decodeValue(JSON.parse(line.slice(2))) as { [key: string]: SfValue } };
    case "L":
      return { tag, locale: JSON.parse(line.slice(2)) as string };
    case "E":
      return { tag, entry: JSON.parse(line.slice(2)) as string };
    case "D":
      return { tag, catalog: JSON.parse(line.slice(2)) as { [key: string]: string } };
    case "S": {
      const gap = line.indexOf(" ", 2);
      return { tag, slot: Number(line.slice(2, gap)), node: decodeNode(JSON.parse(line.slice(gap + 1))) };
    }
    default:
      throw new Error(`unknown payload row tag: ${tag}`);
  }
}

/** The rows of a response body as they arrive: the byte stream decoded and cut at newlines, empty lines skipped, an unterminated last line flushed when the stream ends. A response without a body stream yields the whole text's rows at once. */
export async function* linesOf(res: Response): AsyncGenerator<string> {
  const body = res.body;
  if (!body) {
    for (const line of (await res.text()).split("\n")) {
      if (line.length > 0) yield line;
    }
    return;
  }
  const reader = body.getReader();
  const decoder = new TextDecoder();
  let carry = "";
  for (;;) {
    const { done, value } = await reader.read();
    carry += done ? decoder.decode() : decoder.decode(value, { stream: true });
    let cut = carry.indexOf("\n");
    while (cut !== -1) {
      const line = carry.slice(0, cut);
      carry = carry.slice(cut + 1);
      if (line.length > 0) yield line;
      cut = carry.indexOf("\n");
    }
    if (done) break;
  }
  if (carry.length > 0) yield carry;
}

/** Parses a complete wire response, whatever order its rows came in. Throws when no `N` row was present. */
export function parsePayload(text: string): Payload {
  let format = 0;
  let encoding = "";
  let tree: SfNode | null = null;
  let segments: Segment | null = null;
  const resolutions: { slot: number; node: SfNode }[] = [];
  const heads: Head[] = [];
  const seeds: { [key: string]: SfValue }[] = [];
  let locale: string | null = null;
  let catalog: { [key: string]: string } | null = null;
  let entry: string | null = null;

  for (const line of text.split("\n")) {
    if (line.length === 0) continue;
    const row = parseRow(line);
    switch (row.tag) {
      case "V":
        format = row.format;
        encoding = row.encoding;
        break;
      case "N":
        tree = row.tree;
        break;
      case "G":
        segments = row.segments;
        break;
      case "H":
        heads.push(row.head);
        break;
      case "T":
        seeds.push(row.seed);
        break;
      case "L":
        locale = row.locale;
        break;
      case "E":
        entry = row.entry;
        break;
      case "D":
        catalog = row.catalog;
        break;
      case "S":
        resolutions.push({ slot: row.slot, node: row.node });
        break;
    }
  }
  if (tree === null) throw new Error("payload has no N row");
  return { format, encoding, tree, segments, heads, seeds, locale, catalog, entry, resolutions };
}
