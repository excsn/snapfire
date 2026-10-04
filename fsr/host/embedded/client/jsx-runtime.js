import { linkAttributes } from "./link.js";
import { pictureParts } from "./picture.js";
export const Fragment = Symbol.for("sf.fragment");
export function jsx(type, props, key) {
    return {
        type,
        props: props ?? {},
        key
    };
}
export const jsxs = jsx;
export function Link(props) {
    return jsx("a", linkAttributes(props));
}
export function Picture(props) {
    const { img, sources } = pictureParts(props);
    if (sources === null) return jsx("img", img);
    return jsx("picture", {
        children: [
            ...sources.map((source)=>jsx("source", source)),
            jsx("img", img)
        ]
    });
}
export function Island(props) {
    return jsx(Fragment, {
        children: props.children
    });
}
export function island(component) {
    return component;
}
export function Slot(props) {
    return jsx(Fragment, {
        children: props.children
    });
}
//# sourceMappingURL=jsx-runtime.js.map
