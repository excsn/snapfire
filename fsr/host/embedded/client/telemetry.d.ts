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
/**
 * Reports this page view's web vitals once, when the page is hidden. It names the route pattern
 * the server gave and never the URL, and holds nothing about the person.
 */
export declare function enableTelemetry(options: TelemetryOptions): void;
