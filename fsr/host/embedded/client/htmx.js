import { adopt } from "./store.js";
import { scan } from "./boot.js";
export function bindHtmx(htmx) {
    const settled = ()=>{
        adopt();
        scan(document);
    };
    const rewire = ()=>htmx.process(document.body);
    document.body.addEventListener("htmx:afterSettle", settled);
    document.addEventListener("sf:navigate", rewire);
    document.addEventListener("sf:fill", rewire);
    return ()=>{
        document.body.removeEventListener("htmx:afterSettle", settled);
        document.removeEventListener("sf:navigate", rewire);
        document.removeEventListener("sf:fill", rewire);
    };
}
//# sourceMappingURL=htmx.js.map
