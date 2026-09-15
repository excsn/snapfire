/**
 * What `"jsxImportSource": "@snapfire/fsr-authoring"` points the checker at:
 * the JSX namespace of the template dialect. The build writes that setting
 * into `tsconfig.json` for an application whose import map has no React, so
 * its templates are typed by this file and by `./template` rather than by
 * React's declarations. Nothing here runs: a template the browser mounts is
 * compiled by snapfirec against the React runtime as before.
 */

import type { Attributes, Children, ElementTemplates, Intrinsic, TemplateNode } from "./template";

export namespace JSX {
  type Element = TemplateNode;
  interface ElementClass {
    render?: unknown;
  }
  interface ElementChildrenAttribute {
    children: {};
  }
  interface IntrinsicAttributes {
    key?: string | number | bigint;
  }
  interface IntrinsicClassAttributes<T> {}
  // A tag in `ElementTemplates` takes its type from there alone: an intersection's property never includes the other side's index signature.
  type IntrinsicElements = Intrinsic & ElementTemplates;
  type ElementType = keyof Intrinsic | ((props: any) => TemplateNode | null) | (new (...args: any[]) => any);
  type LibraryManagedAttributes<C, P> = P;
  type ElementAttributes = Attributes;
  type Node = Children;
}

export function jsx(type: unknown, props: unknown, key?: unknown): JSX.Element;
export function jsxs(type: unknown, props: unknown, key?: unknown): JSX.Element;
export const Fragment: unique symbol;
