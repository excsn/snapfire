/** Revalidation after a mutation: re-fetches the current route's payload and force-replaces the top-level child segments, so the layout's DOM survives while mutated content refreshes. */
export declare function refresh(): Promise<void>;
export declare function navigate(href: string, push?: boolean): Promise<void>;
/** Reads the sidecar the server embedded, intercepts same-origin link clicks and owns history from then on. */
export declare function enableNavigation(): void;
