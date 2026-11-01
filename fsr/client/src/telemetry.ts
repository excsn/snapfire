export interface TelemetryOptions {
  /** Where the report goes, as a POST of JSON. */
  beacon: string;
  app: string;
  version: string;
  /** The share of page views that report, from 0 to 1. */
  sample?: number;
  /** A page whose largest paint takes at least this long reports the trace id the server named, so the report can lead to the server's side of it. Leave it out to report none. */
  slowMs?: number;
}

const MOBILE = /Mobi|Android|iPhone/;
const TABLET = /iPad|Tablet/;
const INTERACTION_FLOOR_MS = 40;

function deviceClass(agent: string): string {
  if (TABLET.test(agent)) return "tablet";
  return MOBILE.test(agent) ? "mobile" : "desktop";
}

function traceOfPage(): string | undefined {
  return document.querySelector('meta[name="sf-trace"]')?.getAttribute("content") ?? undefined;
}

function routeOfPage(): string {
  return document.querySelector('meta[name="sf-route"]')?.getAttribute("content") ?? "other";
}

function observe(type: string, read: (entry: PerformanceEntry) => void, options: PerformanceObserverInit = {}): void {
  if (!PerformanceObserver.supportedEntryTypes?.includes(type)) return;
  new PerformanceObserver((list) => list.getEntries().forEach(read)).observe({ type, buffered: true, ...options });
}

/**
 * Reports this page view's web vitals once, when the page is hidden. It names the route pattern
 * the server gave and never the URL, and holds nothing about the person.
 */
export function enableTelemetry(options: TelemetryOptions): void {
  if (Math.random() >= (options.sample ?? 1)) return;
  const samples = new Map<string, number>();
  const largest = (name: string, value: number): void => {
    samples.set(name, Math.max(samples.get(name) ?? 0, value));
  };
  const [navigation] = performance.getEntriesByType("navigation") as PerformanceNavigationTiming[];
  if (navigation) samples.set("ttfb_ms", navigation.responseStart);
  observe("paint", (entry) => {
    if (entry.name === "first-contentful-paint") samples.set("fcp_ms", entry.startTime);
  });
  observe("largest-contentful-paint", (entry) => samples.set("lcp_ms", entry.startTime));
  observe("event", (entry) => largest("inp_ms", entry.duration), { durationThreshold: INTERACTION_FLOOR_MS } as PerformanceObserverInit);

  let sent = false;
  document.addEventListener("visibilitychange", () => {
    if (sent || document.visibilityState !== "hidden") return;
    sent = true;
    const slow = options.slowMs !== undefined && (samples.get("lcp_ms") ?? 0) >= options.slowMs;
    const report = { app: options.app, version: options.version, route: routeOfPage(), device: deviceClass(navigator.userAgent), samples: [...samples], counts: [], traceId: slow ? traceOfPage() : undefined };
    void fetch(options.beacon, { method: "POST", keepalive: true, headers: { "content-type": "application/json" }, body: JSON.stringify(report) }).catch(() => {});
  });
}
