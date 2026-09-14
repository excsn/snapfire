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

export function sf(): Sf {
  const s = (globalThis as { __sf?: Sf }).__sf;
  if (!s) throw new Error("@snapfire/fsr-client/testing runs under `fsr test` only");
  return s;
}

/** Runs everything that happens now: microtasks, action calls, their re-renders and timers already due. A timer set for later waits for `advance`. */
export function settle(): Promise<void> {
  return sf().idle();
}

/** Moves the clock `ms` forward and settles, so timers due by then fire in order. Time never passes on its own. */
export function advance(ms: number): Promise<void> {
  return sf().advance(ms);
}

export class AssertionError extends Error {
  constructor(message: string) {
    super(message);
    this.name = "AssertionError";
  }
}

/** Values the way a test reads them: `1n` and `1` stay distinct, strings are quoted. */
export function show(value: unknown, depth = 0): string {
  if (typeof value === "bigint") return `${value}n`;
  if (typeof value === "string") return JSON.stringify(value);
  if (typeof value === "undefined") return "undefined";
  if (typeof value === "symbol") return String(value);
  if (typeof value === "function") {
    const named = value as { getMockName?: () => string; name?: string };
    return typeof named.getMockName === "function" ? named.getMockName() : `[Function ${named.name || "anonymous"}]`;
  }
  if (value === null || typeof value !== "object") return String(value);
  if (typeof (value as { asymmetricMatch?: unknown }).asymmetricMatch === "function") return String(value);
  if (value instanceof RegExp) return String(value);
  if (value instanceof Date) return `Date(${Number.isNaN(value.getTime()) ? "Invalid" : value.toISOString()})`;
  if (value instanceof Error) return `${value.name}: ${value.message}`;
  if (typeof (value as { nodeType?: unknown }).nodeType === "number") {
    const el = value as { nodeName: string; id?: string; className?: unknown };
    return `<${el.nodeName.toLowerCase()}${el.id ? `#${el.id}` : ""}${typeof el.className === "string" && el.className ? `.${el.className.split(" ").join(".")}` : ""}>`;
  }
  if (depth > 6) return "…";
  if (Array.isArray(value)) return `[${value.map((v) => show(v, depth + 1)).join(", ")}]`;
  if (value instanceof Map) return `Map { ${Array.from(value.entries()).map(([k, v]) => `${show(k, depth + 1)} => ${show(v, depth + 1)}`).join(", ")} }`;
  if (value instanceof Set) return `Set { ${Array.from(value.values()).map((v) => show(v, depth + 1)).join(", ")} }`;
  if (value instanceof Uint8Array) return `Uint8Array(${value.length})`;
  const entries = Object.entries(value as Record<string, unknown>).map(([k, v]) => `${JSON.stringify(k)}: ${show(v, depth + 1)}`);
  return entries.length === 0 ? "{}" : `{ ${entries.join(", ")} }`;
}
