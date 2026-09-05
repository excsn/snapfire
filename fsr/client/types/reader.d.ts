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
	resolutions: {
		slot: number;
		node: SfNode;
	}[];
}
export declare function decodeNode(row: unknown): SfNode;
/** Parses a complete wire response: a V row, the N tree row, the G sidecar, then H and S rows. */
export declare function parsePayload(text: string): Payload;
