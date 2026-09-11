import { refresh } from "./navigator.js";
import { decodeValue, encodeValue } from "./values.js";
export class ActionFailure extends Error {
    kind;
    constructor(kind, message){
        super(message);
        this.name = "ActionFailure";
        this.kind = kind;
    }
}
function failure(status, statusText, text) {
    try {
        const body = JSON.parse(text);
        if (body !== null && typeof body === "object" && typeof body.kind === "string") {
            return new ActionFailure(body.kind, typeof body.message === "string" ? body.message : statusText);
        }
    } catch  {}
    return new ActionFailure(kindOf(status), text.trim() || statusText || `HTTP ${status}`);
}
function kindOf(status) {
    switch(status){
        case 400:
            return "invalid";
        case 401:
        case 403:
            return "unauthorized";
        case 404:
            return "not_found";
        case 409:
            return "conflict";
        case 503:
            return "unavailable";
        case 504:
            return "timeout";
        default:
            return "internal";
    }
}
export function action(id, opts) {
    return async (input = {})=>{
        const headers = {
            "content-type": "application/json"
        };
        if (typeof window !== "undefined") headers["x-sf-from"] = `${window.location.pathname}${window.location.search}`;
        const res = await fetch(`/_sf/action/${encodeURIComponent(id)}`, {
            method: "POST",
            headers,
            body: JSON.stringify(encodeValue(input))
        });
        const text = await res.text();
        if (!res.ok) {
            throw failure(res.status, res.statusText, text);
        }
        const result = decodeValue(JSON.parse(text));
        if (opts?.revalidate !== false) {
            await refresh();
        }
        return result;
    };
}
//# sourceMappingURL=actions.js.map
