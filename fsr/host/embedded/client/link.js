import { currentAddressPath, currentDocumentPath } from "./navigator.js";
export function linkAttributes({ full, into, prefetch, native, keep, match, current, ...rest }) {
    const attrs = {
        ...rest
    };
    if (full) attrs["data-sf-full"] = "true";
    if (into) attrs["data-sf-into"] = into;
    if (prefetch) attrs["data-sf-prefetch"] = prefetch;
    if (native) attrs["data-sf-native"] = "true";
    if (keep !== undefined) attrs["data-sf-keep"] = keep ? "true" : "false";
    const rule = match ?? "exact";
    if (rule !== "none" && typeof rest.href === "string" && rest["aria-current"] === undefined) {
        attrs["data-sf-link"] = rule;
        if (current === "document") attrs["data-sf-current"] = "document";
        const at = current === "document" ? currentDocumentPath() : currentAddressPath();
        const cut = at.indexOf("?");
        const path = cut === -1 ? at : at.slice(0, cut);
        if (rest.href === path) attrs["aria-current"] = rule === "prefix" ? "true" : "page";
        else if (rule === "prefix" && path.startsWith(`${rest.href}/`)) attrs["aria-current"] = "true";
    }
    return attrs;
}
//# sourceMappingURL=link.js.map
