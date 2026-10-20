import { PLACED_END, PLACED_ISLAND, PLACED_OPEN, PLACED_TO } from "./render.js";

/** One run of placed content: its opening template, what it holds and its closing template. */
export interface Piece {
  /** The segment that placed it. */
  key: string;
  /** The island whose markup placed it, by its region key or else its id. */
  island: string | null;
  /** The place it belongs in when that is not the end of the body. */
  to: string | null;
  nodes: ChildNode[];
}

/** Every run of placed content in the document, at the end of the body or inside a place, in document order. */
export function placedPieces(): Piece[] {
  const out: Piece[] = [];
  for (const open of Array.from(document.querySelectorAll<HTMLTemplateElement>(`template[${PLACED_OPEN}]`))) {
    const nodes: ChildNode[] = [open];
    for (let n = open.nextSibling; n; n = n.nextSibling) {
      nodes.push(n);
      if (n instanceof HTMLTemplateElement && n.hasAttribute(PLACED_END)) break;
    }
    out.push({ key: open.getAttribute(PLACED_OPEN) ?? "", island: open.getAttribute(PLACED_ISLAND), to: open.getAttribute(PLACED_TO), nodes });
  }
  return out;
}

/** Moves each run that names its place into that place, after what it already holds, once the place is in the document. A run arrives outside its place when a deferred fill or a navigation brought it after the place was written. */
export function settlePlaced(): void {
  for (const piece of placedPieces()) {
    if (piece.to === null) continue;
    const place = document.getElementById(piece.to);
    if (place && piece.nodes[0].parentNode !== place) place.append(...piece.nodes);
  }
}

/** The runs whose place is not in the document, by the island that placed them. A framework hydrating such an island looks its content up at the place. */
export function unplaced(): Map<string, Piece[]> {
  const out = new Map<string, Piece[]>();
  for (const piece of placedPieces()) {
    if (piece.to === null || piece.island === null) continue;
    const place = document.getElementById(piece.to);
    if (place && piece.nodes[0].parentNode === place) continue;
    out.set(piece.island, [...(out.get(piece.island) ?? []), piece]);
  }
  return out;
}
