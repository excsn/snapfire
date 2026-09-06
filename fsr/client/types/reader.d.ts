import { SfValue } from "./values.js";
export type SfNode = {
	kind: "text";
	text: string;
} | {
	kind: "raw";
	html: string;
} | {
	kind: "seq";
	children: SfNode[];
} | {
	kind: "client";
	module: string;
	props: {
		[key: string]: SfValue;
	};
	children: SfNode[];
	ssr: SfNode | null;
} | {
	kind: "pending";
	slot: number;
	fallback: SfNode;
};
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
	seeds: {
		[key: string]: SfValue;
	}[];
	/** The locale the response was rendered in, as the application spells it; null when the server has none. */
	locale: string | null;
	/** The locale's message table, a `D` row, sent when the request did not say it already holds it; null otherwise. */
	catalog: {
		[key: string]: string;
	} | null;
	/** A module to load before this response's islands can mount, a mounted site's entry; null when the document's own entry covers them. */
	entry: string | null;
	resolutions: {
		slot: number;
		node: SfNode;
	}[];
}
/** One row of a payload, by its tag. */
export type Row = {
	tag: "V";
	format: number;
	encoding: string;
} | {
	tag: "N";
	tree: SfNode;
} | {
	tag: "G";
	segments: Segment;
} | {
	tag: "H";
	head: Head;
} | {
	tag: "T";
	seed: {
		[key: string]: SfValue;
	};
} | {
	tag: "L";
	locale: string;
} | {
	tag: "E";
	entry: string;
} | {
	tag: "D";
	catalog: {
		[key: string]: string;
	};
} | {
	tag: "S";
	slot: number;
	node: SfNode;
};
export declare function decodeNode(row: unknown): SfNode;
/** Reads one row: its tag, a space, then its body. Throws on a tag the grammar lacks. */
export declare function parseRow(line: string): Row;
/** The rows of a response body as they arrive: the byte stream decoded and cut at newlines, empty lines skipped, an unterminated last line flushed when the stream ends. A response without a body stream yields the whole text's rows at once. */
export declare function linesOf(res: Response): AsyncGenerator<string>;
/** Parses a complete wire response, whatever order its rows came in. Throws when no `N` row was present. */
export declare function parsePayload(text: string): Payload;
