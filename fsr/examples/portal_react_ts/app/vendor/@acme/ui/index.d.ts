import type { ReactElement } from "react";
export type Tone = "violet" | "teal";
/** A short label in a rounded pill. */
export declare function Badge({ tone, dot, children }: {
	tone?: Tone;
	dot?: boolean;
	children: string;
}): ReactElement;
