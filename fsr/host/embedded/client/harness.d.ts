/** What `fsr test` installs before a spec file loads. */
export interface Sf {
	ctx(spec: string): number;
	use(id: number): void;
	session(id: number): string;
	locale(id: number): string;
	calls(id: number): string;
	render(module: string, props: string): string | null;
	load(html: string, url: string): void;
	idle(): Promise<void>;
	advance(ms: number): Promise<void>;
}
export declare function sf(): Sf;
/** Runs everything that happens now: microtasks, action calls, their re-renders and timers already due. A timer set for later waits for `advance`. */
export declare function settle(): Promise<void>;
/** Moves the clock `ms` forward and settles, so timers due by then fire in order. Time never passes on its own. */
export declare function advance(ms: number): Promise<void>;
export declare class AssertionError extends Error {
	constructor(message: string);
}
/** Values the way a test reads them: `1n` and `1` stay distinct, strings are quoted. */
export declare function show(value: unknown, depth?: number): string;
