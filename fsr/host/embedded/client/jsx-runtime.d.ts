/**
* FSR's JSX element builder: what TSX compiles against where no JSX
* framework is, a spec's JSX in an application without React and the
* dialect's placements as plain elements. An element is a description;
* nothing here mounts or renders one, since FSR is no JSX framework.
*/
import { type LinkOptions } from "./link.js";
import { type PictureOptions } from "./picture.js";
export declare const Fragment: unique symbol;
export type FsrComponent = (props: Record<string, unknown>) => FsrNode;
export interface FsrElement {
	type: string | typeof Fragment | FsrComponent;
	props: Record<string, unknown>;
	key?: unknown;
}
export type FsrNode = FsrElement | string | number | bigint | boolean | null | undefined | FsrNode[];
/** The types TypeScript checks a file compiled with `@jsxImportSource @snapfire/fsr-client` against: an element is a description and any tag takes HTML attributes as they are written. */
export declare namespace JSX {
	type Element = FsrElement;
	type ElementType = string | FsrComponent;
	interface ElementChildrenAttribute {
		children: {};
	}
	interface IntrinsicAttributes {
		key?: string | number | bigint;
	}
	interface IntrinsicElements {
		[tag: string]: Record<string, unknown>;
	}
}
export declare function jsx(type: FsrElement["type"], props: Record<string, unknown> | null, key?: unknown): FsrElement;
export declare const jsxs: typeof jsx;
/** An anchor with the marks the navigator reads. */
export declare function Link(props: LinkOptions & {
	children?: FsrNode;
}): FsrElement;
/** The `<picture>` the server writes for the same props. */
export declare function Picture(props: PictureOptions): FsrElement;
/** A placement as an element: its child where it stands. */
export declare function Island(props: {
	children?: FsrNode;
}): FsrElement;
export declare function island<P extends Record<string, unknown>>(component: (props: P) => FsrNode): (props: P) => FsrNode;
/** A slot as an element: its fallback. */
export declare function Slot(props: {
	children?: FsrNode;
}): FsrElement;
