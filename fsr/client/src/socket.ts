import { decodeValue, encodeValue, type SfValue } from "./values.js";
import { key, set, transaction } from "./store.js";

export interface SocketOptions {
  /** What to do with a row that arrives. Defaults to writing it into the store under its key, so every island reading that key follows. */
  onRow?: (key: string, value: unknown) => void;
  /** Called when the socket opens and again after every reconnection. */
  onOpen?: () => void;
  /** Called when the connection drops, before the wait to reconnect, and not when `close` was asked for. */
  onClose?: () => void;
  /** The endpoint, for a host mounted under a prefix. Defaults to `/_sf/socket`. */
  path?: string;
  /** How long to wait before reconnecting, doubling up to a minute. Defaults to 500ms. */
  backoffMs?: number;
}

export interface Socket {
  /** Sends one row. What the server makes of it is the application's, and what comes back arrives as rows. */
  send(key: string, value: unknown): void;
  /** Whether a connection stands right now. */
  open(): boolean;
  close(): void;
}

/** Opens a socket on `topic` and keeps it open, reconnecting with a widening delay when it drops. Rows the server sends are written into the store by default. Sends made while the socket is down are dropped rather than queued, since what a wave sends is the state of a keystroke and the next one supersedes it. Returns a no-op socket where `WebSocket` is absent, which is every server-side render. */
export function socket(topic: string, options: SocketOptions = {}): Socket {
  if (typeof WebSocket !== "function") {
    return { send: () => {}, open: () => false, close: () => {} };
  }
  const path = options.path ?? "/_sf/socket";
  const write = options.onRow ?? ((k: string, value: unknown) => set(key(k), value));
  const first = options.backoffMs ?? 500;

  let live: WebSocket | null = null;
  let closed = false;
  let wait = first;
  let timer: ReturnType<typeof setTimeout> | null = null;

  function connect(): void {
    if (closed) return;
    const url = new URL(`${path}?topic=${encodeURIComponent(topic)}`, window.location.href);
    url.protocol = url.protocol === "https:" ? "wss:" : "ws:";
    const ws = new WebSocket(url);
    live = ws;
    ws.onopen = () => {
      wait = first;
      options.onOpen?.();
    };
    ws.onmessage = (event: MessageEvent) => {
      let rows: { key: string; value: unknown }[] = [];
      try {
        rows = (JSON.parse(event.data as string) as { rows?: { key: string; value: unknown }[] }).rows ?? [];
      } catch {
        return;
      }
      transaction(() => {
        for (const row of rows) write(row.key, decodeValue(row.value as SfValue));
      });
    };
    ws.onclose = () => {
      live = null;
      if (closed) return;
      options.onClose?.();
      timer = setTimeout(connect, wait);
      wait = Math.min(wait * 2, 60_000);
    };
  }

  connect();

  return {
    send(k: string, value: unknown): void {
      if (live?.readyState === WebSocket.OPEN) {
        live.send(JSON.stringify({ key: k, value: encodeValue(value) }));
      }
    },
    open(): boolean {
      return live?.readyState === WebSocket.OPEN;
    },
    close(): void {
      closed = true;
      if (timer) clearTimeout(timer);
      live?.close();
      live = null;
    },
  };
}
