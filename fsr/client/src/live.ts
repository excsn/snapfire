import { refresh } from "./navigator.js";

export interface LiveOptions {
  /** What to do when a topic fires. Defaults to `refresh()`, which re-runs the route's loaders and patches the page in place. */
  onTopic?: (topic: string) => void;
  /** The endpoint, for a host mounted under a prefix. Defaults to `/_sf/live`. */
  path?: string;
}

/** Follows `topics` over the host's event stream and returns the function that stops following. The browser reconnects on its own when the stream drops, so a restarted server resumes without a reload. The stream closes when the page is left and opens again when the page comes back from the back-forward cache; a topic that fired while it was away is not replayed. Does nothing where `EventSource` is absent, which is every server-side render. */
export function live(topics: string[], options: LiveOptions = {}): () => void {
  if (typeof EventSource !== "function" || topics.length === 0) return () => {};
  const url = `${options.path ?? "/_sf/live"}?topics=${encodeURIComponent(topics.join(","))}`;
  const onTopic = options.onTopic ?? (() => void refresh());
  let source: EventSource | null = null;
  const open = () => {
    source = new EventSource(url);
    source.onmessage = (event: MessageEvent) => {
      let topic = "";
      try {
        topic = (JSON.parse(event.data as string) as { topic?: string }).topic ?? "";
      } catch {
        return;
      }
      if (topic) onTopic(topic);
    };
  };
  // A cached page holding the stream keeps a connection to the host, and a few
  // of those stall the next navigation behind the browser's per-host limit.
  const hide = () => {
    source?.close();
    source = null;
  };
  const show = (event: PageTransitionEvent) => {
    if (event.persisted && !source) open();
  };
  addEventListener("pagehide", hide);
  addEventListener("pageshow", show);
  open();
  return () => {
    removeEventListener("pagehide", hide);
    removeEventListener("pageshow", show);
    hide();
  };
}
