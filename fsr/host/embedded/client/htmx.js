import { adopt } from "./store.js";
import { scan } from "./boot.js";
const SETTLED = [
    "htmx:after:settle",
    "htmx:afterSettle"
];
export function bindHtmx(htmx) {
    const settled = ()=>{
        adopt();
        scan(document);
    };
    const rewire = ()=>htmx.process(document.body);
    for (const name of SETTLED)document.body.addEventListener(name, settled);
    document.addEventListener("sf:navigate", rewire);
    document.addEventListener("sf:fill", rewire);
    return ()=>{
        for (const name of SETTLED)document.body.removeEventListener(name, settled);
        document.removeEventListener("sf:navigate", rewire);
        document.removeEventListener("sf:fill", rewire);
    };
}
//# sourceMappingURL=htmx.js.map
