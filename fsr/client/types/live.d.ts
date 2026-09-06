export interface LiveOptions {
	/** What to do when a topic fires. Defaults to `refresh()`, which re-runs the route's loaders and patches the page in place. */
	onTopic?: (topic: string) => void;
	/** The endpoint, for a host mounted under a prefix. Defaults to `/_sf/live`. */
	path?: string;
}
/** Follows `topics` over the host's event stream and returns the function that stops following. The browser reconnects on its own when the stream drops, so a restarted server resumes without a reload. Does nothing where `EventSource` is absent, which is every server-side render. */
export declare function live(topics: string[], options?: LiveOptions): () => void;
