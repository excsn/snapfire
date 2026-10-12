/** When a script loads: now, once the main thread is idle or on the visitor's first pointer, key, touch or scroll. */
export type ScriptTiming = "load" | "idle" | "interaction";
export interface ScriptOptions {
	/** Defaults to `"load"`. */
	when?: ScriptTiming;
	/** Attributes the element carries, `integrity`, `crossorigin` or a vendor's `data-*` among them. `async` is set unless this names it. */
	attrs?: Record<string, string | boolean>;
}
/** Forgets every script loaded so far: what a new document starts from. The test runner's `load` calls it, since its documents share one module instance where a browser's do not. */
export declare function forgetLoadedScripts(): void;
/** Loads `src` as a classic script once per document, at the timing `options.when` names. Resolves when it has run and rejects when it fails to load. A second call for the same URL returns the first call's promise, so a module or a route can ask for its vendor on every navigation. */
export declare function loadScript(src: string, options?: ScriptOptions): Promise<void>;
/** Loads every script a route's `meta` declared under `root`, the inert `script[data-sf-script]` the server writes in the head. The boot runtime calls it for the document; a navigation adds no head markup, so the navigator loads the rows its payload's `H` row lists through `loadMarkedScript`. A failure is reported on the console rather than thrown, since no caller is waiting on it. */
export declare function loadDeclaredScripts(root?: ParentNode): void;
/** Loads one script row from the marker's attributes: `data-src`, the timing under `data-sf-script` and the rest as written, `data-type` as `type`. A navigation's `H` row carries the same attributes. */
export declare function loadMarkedScript(marker: Record<string, string>): void;
