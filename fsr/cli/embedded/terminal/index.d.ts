/** Milliseconds for a duration such as `"2 days"` or `"1h"`; `undefined` for one it cannot read. */
export declare function ms(value: string): number | undefined;
/** A duration in milliseconds written short as `"2d"` or long as `"2 days"`. */
export declare function ms(value: number, options?: { long?: boolean }): string;

/** `text` with its ANSI escape codes removed. */
export declare function stripAnsi(text: string): string;

/** The columns `text` takes in a terminal, its escape codes left out and wide characters counted twice. */
export declare function stringWidth(text: string): number;

/** `text` wrapped to `columns`, its escape codes kept where they stand. */
export declare function wrapAnsi(text: string, columns: number, options?: { hard?: boolean; wordWrap?: boolean; trim?: boolean }): string;
