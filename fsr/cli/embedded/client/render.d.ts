import { Segment, SfNode } from "./reader.js";
import { SfValue } from "./values.js";
declare function scriptSafeJson(value: SfValue): string;
export interface IdAlloc {
	next: number;
}
/** Client-side ids use the `sf-c` prefix so they can never collide with the server's `sf-i` sequence. */
export declare function nodeToHtml(node: SfNode, ids: IdAlloc): string;
export declare function escapeKey(key: string): string;
declare function subtreeAt(node: SfNode, path: number[]): SfNode;
/** Mirrors the server's segment serialization: the subtree wrapped in comment delimiters, recursing into child segments, so a swapped-in region stays diffable on the next navigation. */
export declare function renderSegment(node: SfNode, seg: Segment, ids: IdAlloc): string;
/** The props key an island's region key rides under, written by the renderer. */
export declare const REGION_KEY = "$k";
/** What a payload says about one nested island region: the props to mount or patch it with, its own markup for a region that does not exist yet, and the regions inside it. */
export interface RegionSource {
	props: {
		[key: string]: SfValue;
	};
	html: string;
	nested: Map<string, RegionSource>;
}
/** The island regions `node` describes, by region key: the islands directly inside it, each carrying the ones inside itself. An island's own body is where its nested regions live, so a client node is descended into rather than collected at the top. */
export declare function regionSources(node: SfNode, ids: IdAlloc): Map<string, RegionSource>;
export { scriptSafeJson };
export { subtreeAt };
