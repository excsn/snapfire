import { refresh } from "./navigator.js";

export interface LiveOptions {
  /** What to do when a topic fires. Defaults to `refresh()`, which re-runs the route's loaders and patches the page in place. */
  onTopic?: (topic: string) => void;
  /** The endpoint, for a host mounted under a prefix. Defaults to `/_sf/live`. */
  path?: string;
}

/** Follows `topics` over the host's event stream and returns the function that stops following. The browser reconnects on its own when the stream drops, so a restarted server resumes without a reload. Does nothing where `EventSource` is absent, which is every server-side render. */
export function live(topics: string[], options: LiveOptions = {}): () => void {
  if (typeof EventSource !== "function" || topics.length === 0) return () => {};
  const path = options.path ?? "/_sf/live";
  const source = new EventSource(`${path}?topics=${encodeURIComponent(topics.join(","))}`);
  const onTopic = options.onTopic ?? (() => void refresh());
  source.onmessage = (event: MessageEvent) => {
    let topic = "";
    try {
      topic = (JSON.parse(event.data as string) as { topic?: string }).topic ?? "";
    } catch {
      return;
    }
    if (topic) onTopic(topic);
  };
  return () => source.close();
}
