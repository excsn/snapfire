import { refresh } from "./navigator.js";
export function live(topics, options = {}) {
    if (typeof EventSource !== "function" || topics.length === 0) return ()=>{};
    const path = options.path ?? "/_sf/live";
    const source = new EventSource(`${path}?topics=${encodeURIComponent(topics.join(","))}`);
    const onTopic = options.onTopic ?? (()=>void refresh());
    source.onmessage = (event)=>{
        let topic = "";
        try {
            topic = JSON.parse(event.data).topic ?? "";
        } catch  {
            return;
        }
        if (topic) onTopic(topic);
    };
    return ()=>source.close();
}
//# sourceMappingURL=live.js.map
