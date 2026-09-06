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
/** Opens a socket on `topic` and keeps it open, reconnecting with a widening delay when it drops. Rows the server sends are written into the store by default. Sends made while the socket is down are dropped rather than queued. Returns a no-op socket where `WebSocket` is absent. */
export declare function socket(topic: string, options?: SocketOptions): Socket;
