import { Segment, SfNode } from "./reader.js";
import { SfValue } from "./values.js";
declare function scriptSafeJson(value: SfValue): string;
/** An island's props script: its props as the server encoded them, else encoded here. */
declare function propsScript(node: {
	props: {
		[key: string]: SfValue;
	};
	encoded?: unknown;
}): string;
export interface IdAlloc {
	next: number;
	/** The key of the segment being rendered, which names the placed content inside it. */
	segment?: string;
	/** The island whose own markup is being rendered, by its region key or else its id, which names the placed content it holds. */
	island?: string;
}
/** The attribute of the template `nodeToHtml` holds placed content in, in place, naming the segment that placed it. Whatever writes the markup moves the content to its target. */
export declare const PLACED_ATTR = "data-sf-placed";
/** The attribute of the template opening a piece of placed content at the end of the body, naming the segment that placed it, as the server writes it after the document. */
export declare const PLACED_OPEN = "data-sf-l";
/** The attribute of the template closing one. */
export declare const PLACED_END = "data-sf-l-end";
/** The attribute naming the island whose markup placed a piece, by its region key or else its id. */
export declare const PLACED_ISLAND = "data-sf-island";
/** Client-side ids use the `sf-c` prefix so they can never collide with the server's `sf-i` sequence. */
export declare function nodeToHtml(node: SfNode, ids: IdAlloc): string;
export declare function escapeKey(key: string): string;
declare function subtreeAt(node: SfNode, path: number[]): SfNode;
/** Mirrors the server's segment serialization: the subtree wrapped in comment delimiters, recursing into child segments, so a swapped-in region stays diffable on the next navigation. */
export declare function renderSegment(node: SfNode, seg: Segment, ids: IdAlloc): string;
/** The props key an island's region key rides under, written by the renderer. */
export declare const REGION_KEY = "$k";
/** What a payload says about one nested island region: the props to mount or patch it with, its own markup for a region that does not exist yet, the regions inside it and the markup of its children region. */
export interface RegionSource {
	props: {
		[key: string]: SfValue;
	};
	/** `props` as the server encoded them, absent on a node no payload brought. */
	encoded?: unknown;
	html: string;
	nested: Map<string, RegionSource>;
	children: string | null;
	markup: {
		[n: string]: string;
	} | null;
}
/** The attribute of the region an island's children render in, which the mounter hands the component as its `children`. */
export declare const CHILDREN_ATTR = "data-sf-children";
/** The markup of an island's children region: the `<sf-s data-sf-children>` in its own markup that is not inside an island nested in it. Null for a node that is not an island or holds no such region. */
export declare function childrenOf(node: SfNode, ids: IdAlloc): string | null;
/** The attribute of the regions an island's markup props render in. */
export declare const MARKUP_ATTR = "data-sf-m";
/** The markup of each markup prop region in an island's own markup, by region number: the `<sf-s data-sf-m>` its render placed, else the `<template data-sf-m>` the server held it in, neither inside an island nested in it. Null for a node that is not an island or holds none. */
export declare function markupOf(node: SfNode, ids: IdAlloc): {
	[n: string]: string;
} | null;
/** The island regions `node` describes, by region key: the islands directly inside it, each carrying the ones inside itself. An island's own body is where its nested regions live, so a client node is descended into rather than collected at the top. */
export declare function regionSources(node: SfNode, ids: IdAlloc): Map<string, RegionSource>;
export { propsScript, scriptSafeJson };
export { subtreeAt };
