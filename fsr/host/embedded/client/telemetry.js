const MOBILE = /Mobi|Android|iPhone/;
const TABLET = /iPad|Tablet/;
const INTERACTION_FLOOR_MS = 40;
function deviceClass(agent) {
    if (TABLET.test(agent)) return "tablet";
    return MOBILE.test(agent) ? "mobile" : "desktop";
}
function routeOfPage() {
    return document.querySelector('meta[name="sf-route"]')?.getAttribute("content") ?? "other";
}
function observe(type, read, options = {}) {
    if (!PerformanceObserver.supportedEntryTypes?.includes(type)) return;
    new PerformanceObserver((list)=>list.getEntries().forEach(read)).observe({
        type,
        buffered: true,
        ...options
    });
}
export function enableTelemetry(options) {
    if (Math.random() >= (options.sample ?? 1)) return;
    const samples = new Map();
    const largest = (name, value)=>{
        samples.set(name, Math.max(samples.get(name) ?? 0, value));
    };
    const [navigation] = performance.getEntriesByType("navigation");
    if (navigation) samples.set("ttfb_ms", navigation.responseStart);
    observe("paint", (entry)=>{
        if (entry.name === "first-contentful-paint") samples.set("fcp_ms", entry.startTime);
    });
    observe("largest-contentful-paint", (entry)=>samples.set("lcp_ms", entry.startTime));
    observe("event", (entry)=>largest("inp_ms", entry.duration), {
        durationThreshold: INTERACTION_FLOOR_MS
    });
    let sent = false;
    document.addEventListener("visibilitychange", ()=>{
        if (sent || document.visibilityState !== "hidden") return;
        sent = true;
        const report = {
            app: options.app,
            version: options.version,
            route: routeOfPage(),
            device: deviceClass(navigator.userAgent),
            samples: [
                ...samples
            ],
            counts: []
        };
        void fetch(options.beacon, {
            method: "POST",
            keepalive: true,
            headers: {
                "content-type": "application/json"
            },
            body: JSON.stringify(report)
        }).catch(()=>{});
    });
}
//# sourceMappingURL=telemetry.js.map
