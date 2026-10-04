/**
 * FSR's JSX element builder: the dialect's placements as plain elements,
 * which the template entry serves to a file nothing mounts. An element is a
 * description; nothing here mounts or renders one, since FSR is no JSX
 * framework.
 */
import { linkAttributes, type LinkOptions } from "./link.js";
import { pictureParts, type PictureOptions } from "./picture.js";

export const Fragment: unique symbol = Symbol.for("sf.fragment") as never;

export type FsrComponent = (props: Record<string, unknown>) => FsrNode;

export interface FsrElement {
  type: string | typeof Fragment | FsrComponent;
  props: Record<string, unknown>;
  key?: unknown;
}

export type FsrNode = FsrElement | string | number | bigint | boolean | null | undefined | FsrNode[];

export function jsx(type: FsrElement["type"], props: Record<string, unknown> | null, key?: unknown): FsrElement {
  return { type, props: props ?? {}, key };
}

export const jsxs: typeof jsx = jsx;

/** An anchor with the marks the navigator reads. */
export function Link(props: LinkOptions & { children?: FsrNode }): FsrElement {
  return jsx("a", linkAttributes(props));
}

/** The `<picture>` the server writes for the same props. */
export function Picture(props: PictureOptions): FsrElement {
  const { img, sources } = pictureParts(props);
  if (sources === null) return jsx("img", img);
  return jsx("picture", { children: [...sources.map((source) => jsx("source", source)), jsx("img", img)] });
}

/** A placement as an element: its child where it stands. */
export function Island(props: { children?: FsrNode }): FsrElement {
  return jsx(Fragment, { children: props.children });
}

export function island<P extends Record<string, unknown>>(component: (props: P) => FsrNode): (props: P) => FsrNode {
  return component;
}

/** A slot as an element: its fallback. */
export function Slot(props: { children?: FsrNode }): FsrElement {
  return jsx(Fragment, { children: props.children });
}
