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
  p?: number[];
  s?: number;
  c: Segment[];
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
  resolutions: { slot: number; node: SfNode }[];
}

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

/** Parses a complete wire response: a V row, the N tree row, the G sidecar, then H and S rows. */
export function parsePayload(text: string): Payload {
  let format = 0;
  let encoding = "";
  let tree: SfNode | null = null;
  let segments: Segment | null = null;
  const resolutions: { slot: number; node: SfNode }[] = [];
  const heads: Head[] = [];

  for (const line of text.split("\n")) {
    if (line.length === 0) continue;
    const tag = line[0];
    if (tag === "V") {
      const v = JSON.parse(line.slice(2));
      format = v.fmt;
      encoding = v.enc;
    } else if (tag === "N") {
      tree = decodeNode(JSON.parse(line.slice(2)));
    } else if (tag === "G") {
      segments = JSON.parse(line.slice(2));
    } else if (tag === "H") {
      heads.push(JSON.parse(line.slice(2)));
    } else if (tag === "S") {
      const gap = line.indexOf(" ", 2);
      const slot = Number(line.slice(2, gap));
      resolutions.push({ slot, node: decodeNode(JSON.parse(line.slice(gap + 1))) });
    } else {
      throw new Error(`unknown payload row tag: ${tag}`);
    }
  }
  if (tree === null) throw new Error("payload has no N row");
  return { format, encoding, tree, segments, heads, resolutions };
}
