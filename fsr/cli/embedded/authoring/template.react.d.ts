/**
 * `./template` as a React application reads it. The build points the
 * `@snapfire/fsr-authoring/template` path here when the import map serves
 * React, so a file written against the dialect types under React's JSX: its
 * placements are the React module's and its `Children` is `ReactNode`. The
 * elements, attributes and timings are the dialect's own, re-exported.
 */

import type { ReactElement, ReactNode } from "react";

export type {
  AnchorAttributes,
  Attributes,
  ElementTemplates,
  ImageAttributes,
  InputAttributes,
  Intrinsic,
  MountTiming,
  PrefetchTiming,
  StyleValue,
} from "./template";

export type TemplateNode = ReactElement;
export type Children = ReactNode;
export type Component<P = {}> = (props: P) => ReactElement | null;

export { Island, island, Link, Slot } from "@snapfire/fsr-client/react";
export type { IslandProps, LinkProps, SlotProps } from "@snapfire/fsr-client/react";
