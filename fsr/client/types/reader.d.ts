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
	p?: number[];
	s?: number;
	c: Segment[];
}
export interface Payload {
	format: number;
	encoding: string;
	tree: SfNode;
	segments: Segment | null;
	resolutions: {
		slot: number;
		node: SfNode;
	}[];
}
export declare function decodeNode(row: unknown): SfNode;
/** Parses a complete wire response: a V row, the N tree row, then S rows. */
export declare function parsePayload(text: string): Payload;
