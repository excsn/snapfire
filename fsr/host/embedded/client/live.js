import { refresh } from "./navigator.js";
export function live(topics, options = {}) {
    if (typeof EventSource !== "function" || topics.length === 0) return ()=>{};
    const url = `${options.path ?? "/_sf/live"}?topics=${encodeURIComponent(topics.join(","))}`;
    const onTopic = options.onTopic ?? (()=>void refresh());
    let source = null;
    const open = ()=>{
        source = new EventSource(url);
        source.onmessage = (event)=>{
            let topic = "";
            try {
                topic = JSON.parse(event.data).topic ?? "";
            } catch  {
                return;
            }
            if (topic) onTopic(topic);
        };
    };
    const hide = ()=>{
        source?.close();
        source = null;
    };
    const show = (event)=>{
        if (event.persisted && !source) open();
    };
    addEventListener("pagehide", hide);
    addEventListener("pageshow", show);
    open();
    return ()=>{
        removeEventListener("pagehide", hide);
        removeEventListener("pageshow", show);
        hide();
    };
}
//# sourceMappingURL=live.js.map
