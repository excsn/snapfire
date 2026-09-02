import { Segment, SfNode } from "./reader.js";
export interface IdAlloc {
	next: number;
}
/** Client-side ids use the `sf-c` prefix so they can never collide with the server's `sf-i` sequence. */
export declare function nodeToHtml(node: SfNode, ids: IdAlloc): string;
export declare function escapeKey(key: string): string;
declare function subtreeAt(node: SfNode, path: number[]): SfNode;
/** Mirrors the server's segment serialization: the subtree wrapped in comment delimiters, recursing into child segments, so a swapped-in region stays diffable on the next navigation. */
export declare function renderSegment(node: SfNode, seg: Segment, ids: IdAlloc): string;
export { subtreeAt };
