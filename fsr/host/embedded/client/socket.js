import { decodeValue, encodeValue } from "./values.js";
import { key, set, transaction } from "./store.js";
export function socket(topic, options = {}) {
    if (typeof WebSocket !== "function") {
        return {
            send: ()=>{},
            open: ()=>false,
            close: ()=>{}
        };
    }
    const path = options.path ?? "/_sf/socket";
    const write = options.onRow ?? ((k, value)=>set(key(k), value));
    const first = options.backoffMs ?? 500;
    let live = null;
    let closed = false;
    let wait = first;
    let timer = null;
    function connect() {
        if (closed) return;
        const url = new URL(`${path}?topic=${encodeURIComponent(topic)}`, window.location.href);
        url.protocol = url.protocol === "https:" ? "wss:" : "ws:";
        const ws = new WebSocket(url);
        live = ws;
        ws.onopen = ()=>{
            wait = first;
            options.onOpen?.();
        };
        ws.onmessage = (event)=>{
            let rows = [];
            try {
                rows = JSON.parse(event.data).rows ?? [];
            } catch  {
                return;
            }
            transaction(()=>{
                for (const row of rows)write(row.key, decodeValue(row.value));
            });
        };
        ws.onclose = ()=>{
            live = null;
            if (closed) return;
            options.onClose?.();
            timer = setTimeout(connect, wait);
            wait = Math.min(wait * 2, 60_000);
        };
    }
    connect();
    return {
        send (k, value) {
            if (live?.readyState === WebSocket.OPEN) {
                live.send(JSON.stringify({
                    key: k,
                    value: encodeValue(value)
                }));
            }
        },
        open () {
            return live?.readyState === WebSocket.OPEN;
        },
        close () {
            closed = true;
            if (timer) clearTimeout(timer);
            live?.close();
            live = null;
        }
    };
}
//# sourceMappingURL=socket.js.map
