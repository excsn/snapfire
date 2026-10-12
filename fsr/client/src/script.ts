/** When a script loads: now, once the main thread is idle or on the visitor's first pointer, key, touch or scroll. */
export type ScriptTiming = "load" | "idle" | "interaction";

export interface ScriptOptions {
  /** Defaults to `"load"`. */
  when?: ScriptTiming;
  /** Attributes the element carries, `integrity`, `crossorigin` or a vendor's `data-*` among them. `async` is set unless this names it. */
  attrs?: Record<string, string | boolean>;
}

/** Each script by its resolved URL, so a second call for one returns the first call's promise and adds nothing. */
const loaded = new Map<string, Promise<void>>();

/** Forgets every script loaded so far: what a new document starts from. The test runner's `load` calls it, since its documents share one module instance where a browser's do not. */
export function forgetLoadedScripts(): void {
  loaded.clear();
}

function whenReady(when: ScriptTiming): Promise<void> {
  if (when === "load") return Promise.resolve();
  if (when === "idle") {
    const idle = (globalThis as { requestIdleCallback?: (cb: () => void) => void }).requestIdleCallback;
    return new Promise((resolve) => (typeof idle === "function" ? idle(() => resolve()) : setTimeout(resolve, 1)));
  }
  return new Promise((resolve) => {
    const events = ["pointerdown", "keydown", "touchstart", "scroll"] as const;
    const go = () => {
      for (const name of events) document.removeEventListener(name, go, true);
      resolve();
    };
    for (const name of events) document.addEventListener(name, go, { capture: true, passive: true });
  });
}

/** Loads `src` as a classic script once per document, at the timing `options.when` names. Resolves when it has run and rejects when it fails to load. A second call for the same URL returns the first call's promise, so a module or a route can ask for its vendor on every navigation. */
export function loadScript(src: string, options: ScriptOptions = {}): Promise<void> {
  const url = new URL(src, location.href).href;
  const held = loaded.get(url);
  if (held) return held;
  const promise = whenReady(options.when ?? "load").then(
    () =>
      new Promise<void>((resolve, reject) => {
        const script = document.createElement("script");
        const attrs = { async: true, ...options.attrs };
        for (const [name, value] of Object.entries(attrs)) {
          if (value === false) continue;
          script.setAttribute(name, value === true ? "" : value);
        }
        script.addEventListener("load", () => resolve(), { once: true });
        script.addEventListener("error", () => reject(new Error(`${src} did not load`)), { once: true });
        script.src = url;
        document.head.append(script);
      }),
  );
  loaded.set(url, promise);
  return promise;
}

/** Loads every script a route's `meta` declared under `root`, the inert `script[data-sf-script]` the server writes in the head. The boot runtime calls it for the document; a navigation adds no head markup, so the navigator loads the rows its payload's `H` row lists through `loadMarkedScript`. A failure is reported on the console rather than thrown, since no caller is waiting on it. */
export function loadDeclaredScripts(root: ParentNode = document): void {
  for (const marker of Array.from(root.querySelectorAll<HTMLScriptElement>("script[data-sf-script]"))) {
    loadMarkedScript(Object.fromEntries(Array.from(marker.attributes, ({ name, value }) => [name, value])));
  }
}

/** Loads one script row from the marker's attributes: `data-src`, the timing under `data-sf-script` and the rest as written, `data-type` as `type`. A navigation's `H` row carries the same attributes. */
export function loadMarkedScript(marker: Record<string, string>): void {
  const src = marker["data-src"];
  if (!src) return;
  const when = marker["data-sf-script"];
  const attrs: Record<string, string> = {};
  for (const [name, value] of Object.entries(marker)) {
    if (name === "type" || name === "data-src" || name === "data-sf-script") continue;
    attrs[name === "data-type" ? "type" : name] = value;
  }
  loadScript(src, { when: when === "idle" || when === "interaction" ? when : "load", attrs }).catch((err) => console.error(`sf: ${(err as Error).message}`));
}
