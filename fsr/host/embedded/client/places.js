import { PLACED_END, PLACED_ISLAND, PLACED_OPEN, PLACED_TO } from "./render.js";
export function placedPieces() {
    const out = [];
    for (const open of Array.from(document.querySelectorAll(`template[${PLACED_OPEN}]`))){
        const nodes = [
            open
        ];
        for(let n = open.nextSibling; n; n = n.nextSibling){
            nodes.push(n);
            if (n instanceof HTMLTemplateElement && n.hasAttribute(PLACED_END)) break;
        }
        out.push({
            key: open.getAttribute(PLACED_OPEN) ?? "",
            island: open.getAttribute(PLACED_ISLAND),
            to: open.getAttribute(PLACED_TO),
            nodes
        });
    }
    return out;
}
export function settlePlaced() {
    for (const piece of placedPieces()){
        if (piece.to === null) continue;
        const place = document.getElementById(piece.to);
        if (place && piece.nodes[0].parentNode !== place) place.append(...piece.nodes);
    }
}
export function unplaced() {
    const out = new Map();
    for (const piece of placedPieces()){
        if (piece.to === null || piece.island === null) continue;
        const place = document.getElementById(piece.to);
        if (place && piece.nodes[0].parentNode === place) continue;
        out.set(piece.island, [
            ...out.get(piece.island) ?? [],
            piece
        ]);
    }
    return out;
}
//# sourceMappingURL=places.js.map
