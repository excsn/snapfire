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
export declare function placedPieces(): Piece[];
/** Moves each run that names its place into that place, after what it already holds, once the place is in the document. A run arrives outside its place when a deferred fill or a navigation brought it after the place was written. */
export declare function settlePlaced(): void;
/** The runs whose place is not in the document, by the island that placed them. A framework hydrating such an island looks its content up at the place. */
export declare function unplaced(): Map<string, Piece[]>;
/** Records the place each run in `pieces` sits in or names for the island that placed it. A run the server wrote inside its place carries no `data-sf-to`; its place is the element holding it. */
export declare function recordFilled(pieces: Piece[]): void;
/** The elements now in the document of the places each island has filled, by the island. */
export declare function filledPlaces(): Map<string, Element[]>;
