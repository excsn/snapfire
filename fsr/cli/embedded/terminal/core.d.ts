import type { ReactElement, ReactNode } from "react";

/** A number of cells or a percentage of the parent, such as `"50%"`. */
export type Extent = number | `${number}%`;

/** The props of a Box the terminal lays out and draws. */
export interface BoxProps {
  children?: ReactNode;
  key?: string | number;
  ref?: unknown;
  flexDirection?: "row" | "column";
  gap?: number;
  columnGap?: number;
  rowGap?: number;
  padding?: number;
  paddingX?: number;
  paddingY?: number;
  paddingTop?: number;
  paddingRight?: number;
  paddingBottom?: number;
  paddingLeft?: number;
  margin?: number;
  marginX?: number;
  marginY?: number;
  marginTop?: number;
  marginRight?: number;
  marginBottom?: number;
  marginLeft?: number;
  width?: Extent;
  height?: Extent;
  flexGrow?: number;
  flexShrink?: number;
  flexBasis?: Extent;
  justifyContent?: "flex-start" | "center" | "flex-end" | "space-between" | "space-around";
  alignItems?: "flex-start" | "center" | "flex-end" | "stretch";
  borderStyle?: "single" | "double" | "round" | "bold" | "classic";
  borderColor?: string;
  overflow?: "visible" | "hidden";
  overflowX?: "visible" | "hidden";
  overflowY?: "visible" | "hidden";
  contentOffsetX?: number;
  contentOffsetY?: number;
}

/** The props of a Text the terminal draws. */
export interface TextProps {
  children?: ReactNode;
  key?: string | number;
  color?: string;
  backgroundColor?: string;
  dimColor?: boolean;
  bold?: boolean;
  italic?: boolean;
  underline?: boolean;
  strikethrough?: boolean;
  inverse?: boolean;
  wrap?: "wrap" | "truncate" | "truncate-end";
}

export declare function Box(props: BoxProps): ReactElement;
export declare function Text(props: TextProps): ReactElement;
export declare function Newline(props: { count?: number }): ReactElement;
export declare function Spacer(): ReactElement;

/** The key a press names, beside the text it typed. */
export interface Key {
  upArrow: boolean;
  downArrow: boolean;
  leftArrow: boolean;
  rightArrow: boolean;
  pageUp: boolean;
  pageDown: boolean;
  return: boolean;
  escape: boolean;
  ctrl: boolean;
  shift: boolean;
  tab: boolean;
  backspace: boolean;
  delete: boolean;
  meta: boolean;
}

/** Runs `handler` on each key the terminal reads while `isActive` holds. */
export declare function useInput(handler: (input: string, key: Key) => void, options?: { isActive?: boolean }): void;

/** A frame count that steps every `interval` milliseconds (100 unless given) while `isActive` holds, with the milliseconds it stands for. */
export declare function useAnimation(options?: { interval?: number; isActive?: boolean }): { frame: number; time: number };

/** Hands the terminal `value` under `key` for this render. */
export declare function emit(key: string, value: unknown): void;

/** Gives the terminal up while the promise `run` returns is pending, then takes it back. */
export declare function suspendTerminal<T>(run: () => Promise<T>): Promise<T>;
