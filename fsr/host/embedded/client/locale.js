let current = "";
const listeners = new Set();
const messageListeners = new Set();
export function currentLocale() {
    return current;
}
export function subscribeLocale(listener) {
    listeners.add(listener);
    return ()=>{
        listeners.delete(listener);
    };
}
export function subscribeMessages(listener) {
    messageListeners.add(listener);
    return ()=>{
        messageListeners.delete(listener);
    };
}
function messagesChanged() {
    for (const listener of Array.from(messageListeners))listener();
}
export function setLocale(tag) {
    if (tag === current) return;
    current = tag;
    if (typeof document !== "undefined") {
        document.documentElement.setAttribute("lang", tag.replace(/_/g, "-"));
        document.documentElement.setAttribute("data-sf-locale", tag);
    }
    for (const listener of Array.from(listeners))listener(tag);
    messagesChanged();
}
export function adoptLocale() {
    if (typeof document === "undefined") return;
    const tag = document.documentElement.getAttribute("data-sf-locale");
    if (tag) setLocale(tag);
}
const catalogs = new Map();
export function catalog(tag) {
    return catalogs.get(tag) ?? null;
}
export function setCatalog(tag, table) {
    catalogs.set(tag, table);
    messagesChanged();
}
export function adoptCatalog() {
    if (typeof document === "undefined") return;
    const script = document.querySelector("script[data-sf-i18n]");
    const tag = script?.getAttribute("data-sf-i18n");
    if (!script || !tag) return;
    try {
        setCatalog(tag, JSON.parse(script.textContent ?? "{}"));
    } catch  {}
}
//# sourceMappingURL=locale.js.map
