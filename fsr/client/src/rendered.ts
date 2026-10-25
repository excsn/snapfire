/** The markup the server last wrote into each segment region, by the region's opening comment, so a morph can apply the server's own change and leave what a script set. A region a held page carries keeps its markup with its nodes. */
const regions = new WeakMap<Comment, string>();

/** The markup the server last wrote inside each island marker, which a server island's first step reads. */
const markers = new WeakMap<Element, string>();

const OPEN = "sf-g:";
const CLOSE = "/sf-g";

/** The global the document's capture script and fill script leave the server's markup in: the body as the parser first finished it, then each streamed resolution's template. */
const CAPTURED = "__sfServer";

interface Found {
  start: Comment;
  key: string;
  content: Node[];
}

function isOpen(node: Node): node is Comment {
  return node.nodeType === Node.COMMENT_NODE && (node as Comment).data.startsWith(OPEN);
}

function isClose(node: Node): boolean {
  return node.nodeType === Node.COMMENT_NODE && (node as Comment).data === CLOSE;
}

/** The nodes between `start` and the comment that closes it among its siblings. */
function contentOf(start: Comment): Node[] {
  const out: Node[] = [];
  let depth = 0;
  for (let n = start.nextSibling; n; n = n.nextSibling) {
    if (isOpen(n)) depth += 1;
    else if (isClose(n)) {
      if (depth === 0) return out;
      depth -= 1;
    }
    out.push(n);
  }
  return out;
}

/** Every region opened among `nodes` or inside them, outermost first. */
function regionsIn(nodes: Node[]): Found[] {
  const out: Found[] = [];
  const visit = (n: Node): void => {
    if (isOpen(n)) out.push({ start: n, key: n.data.slice(OPEN.length), content: contentOf(n) });
    for (let c = n.firstChild; c; c = c.nextSibling) visit(c);
  };
  for (const n of nodes) visit(n);
  return out;
}

function escapeText(text: string): string {
  return text.replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;");
}

/** The nodes `html` parses to, in an inert template. */
export function parsed(html: string): Node[] {
  const template = document.createElement("template");
  template.innerHTML = html;
  return Array.from(template.content.childNodes);
}

/** `nodes` written back as markup. */
export function serialize(nodes: Node[]): string {
  let out = "";
  for (const n of nodes) {
    if (n instanceof Element) out += n.outerHTML;
    else if (n.nodeType === Node.TEXT_NODE) out += escapeText(n.nodeValue ?? "");
    else if (n.nodeType === Node.COMMENT_NODE) out += `<!--${n.nodeValue ?? ""}-->`;
  }
  return out;
}

function markersIn(nodes: Node[]): Element[] {
  const out: Element[] = [];
  for (const n of nodes) {
    if (!(n instanceof Element)) continue;
    if (n.tagName === "SF-I") out.push(n);
    out.push(...Array.from(n.querySelectorAll("sf-i")));
  }
  return out;
}

/** Records `nodes` as markup the server wrote: each region opened among them and each island marker in them. Called on nodes the navigator has just put in the document, before anything else can touch them. */
export function recordWritten(nodes: Node[]): void {
  for (const found of regionsIn(nodes)) regions.set(found.start, serialize(found.content));
  for (const marker of markersIn(nodes)) markers.set(marker, marker.innerHTML);
}

/** Records the markup a morph wrote into the region opened at `start`, read from `fresh`, the markup it morphed to, for that region and each one inside it. */
export function recordMorphed(start: Comment, fresh: Node[]): void {
  const wrote = new Map(regionsIn(fresh).map((found) => [found.key, serialize(found.content)]));
  for (const found of regionsIn([start, ...contentOf(start)])) {
    const html = wrote.get(found.key);
    if (html !== undefined) regions.set(found.start, html);
  }
}

/** Records the markup inside a marker the server wrote. */
export function recordMarker(marker: Element, html: string): void {
  markers.set(marker, html);
}

let captured = false;

/** Takes what the document's capture left once, the first time a record is asked for: each region and marker the server wrote that nothing has recorded since gets the server's markup. */
function adoptCaptured(): void {
  if (captured || typeof window === "undefined") return;
  captured = true;
  const sources = (window as unknown as { [CAPTURED]?: string[] })[CAPTURED];
  delete (window as unknown as { [CAPTURED]?: string[] })[CAPTURED];
  if (!Array.isArray(sources)) return;
  const byKey = new Map<string, string>();
  const byId = new Map<string, string>();
  for (const html of sources) {
    const nodes = parsed(html);
    for (const found of regionsIn(nodes)) if (!byKey.has(found.key)) byKey.set(found.key, serialize(found.content));
    for (const marker of markersIn(nodes)) if (marker.id && !byId.has(marker.id)) byId.set(marker.id, marker.innerHTML);
  }
  for (const found of regionsIn([document.documentElement])) {
    const html = byKey.get(found.key);
    if (html !== undefined && !regions.has(found.start)) regions.set(found.start, html);
  }
  for (const [id, html] of byId) {
    const marker = document.getElementById(id);
    if (marker?.tagName === "SF-I" && !markers.has(marker)) markers.set(marker, html);
  }
}

/** Starts over for a new document, whose capture has yet to be taken. */
export function restartRecords(): void {
  captured = false;
}

/** Readies the records the document's capture holds, before anything retags or moves a region. */
export function readyRecords(): void {
  adoptCaptured();
}

/** The region opened at `start` as the server last wrote it, delimiters included, each region inside it as the server last wrote that one. Null when nothing recorded it. */
export function previousRegion(start: Comment): Node[] | null {
  adoptCaptured();
  const html = regions.get(start);
  if (html === undefined) return null;
  const content = parsed(html);
  const live = new Map(regionsIn(contentOf(start)).map((found) => [found.key, found.start]));
  const inside = regionsIn(content);
  const outermost = inside.filter((inner) => !inside.some((other) => other !== inner && other.content.some((n) => n === inner.start || n.contains(inner.start))));
  for (const inner of outermost) {
    const liveStart = live.get(inner.key);
    const newer = liveStart ? previousRegion(liveStart) : null;
    if (newer === null) continue;
    const parent = inner.start.parentNode;
    if (!parent) continue;
    for (const n of inner.content) parent.removeChild(n);
    const close = inner.start.nextSibling;
    for (const n of newer.slice(1, -1)) parent.insertBefore(n, close);
  }
  return [document.createComment(start.data), ...content, document.createComment(CLOSE)];
}

/** The markup inside `marker` as the server last wrote it, or null when nothing recorded it. */
export function previousMarker(marker: Element): string | null {
  adoptCaptured();
  return markers.get(marker) ?? null;
}
