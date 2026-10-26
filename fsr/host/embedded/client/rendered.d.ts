/** The key of the innermost segment region holding `node`, null outside every region. */
export declare function segmentAround(node: Node): string | null;
/** Whether the document shows a region for the segment `key`. */
export declare function segmentShown(key: string): boolean;
/** The nodes `html` parses to, in an inert template. */
export declare function parsed(html: string): Node[];
/** `nodes` written back as markup. */
export declare function serialize(nodes: Node[]): string;
/** Records `nodes` as markup the server wrote: each region opened among them and each island marker in them. Called on nodes the navigator has just put in the document, before anything else can touch them. */
export declare function recordWritten(nodes: Node[]): void;
/** Records the markup a morph wrote into the region opened at `start`, read from `fresh`, the markup it morphed to, for that region and each one inside it. */
export declare function recordMorphed(start: Comment, fresh: Node[]): void;
/** Records the markup inside a marker the server wrote. */
export declare function recordMarker(marker: Element, html: string): void;
/** Starts over for a new document, whose capture has yet to be taken. */
export declare function restartRecords(): void;
/** Readies the records the document's capture holds, before anything retags or moves a region. */
export declare function readyRecords(): void;
/** The region opened at `start` as the server last wrote it, delimiters included, each region inside it as the server last wrote that one. Null when nothing recorded it. */
export declare function previousRegion(start: Comment): Node[] | null;
/** The markup inside `marker` as the server last wrote it, or null when nothing recorded it. */
export declare function previousMarker(marker: Element): string | null;
