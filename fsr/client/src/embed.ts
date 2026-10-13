/** A video hosted elsewhere, shown as a still with a play button until the visitor plays it. */
export interface EmbedOptions {
  provider: "youtube" | "vimeo";
  id: string;
  /** Names the video on the placeholder and to a screen reader, and titles the player. */
  title: string;
  className?: string;
}

/** The attribute the server writes on a placeholder: the player the click puts in its place. */
export const EMBED_ATTR = "data-sf-embed";

const OUTER_STYLE = "position:relative;aspect-ratio:16/9;background:#000;overflow:hidden";
const BUTTON_STYLE = "all:unset;cursor:pointer;position:absolute;inset:0;display:block";
const STILL_STYLE = "width:100%;height:100%;object-fit:cover;display:block";
const TITLE_STYLE = "position:absolute;left:0;right:0;top:0;padding:12px 16px;color:#fff;font:600 16px/1.3 system-ui,sans-serif;text-align:left;background:linear-gradient(rgba(0,0,0,.65),transparent)";
const PLAY_STYLE = "position:absolute;left:50%;top:50%;transform:translate(-50%,-50%);width:68px;height:48px;border-radius:12px;background:rgba(0,0,0,.75);color:#fff;display:flex;align-items:center;justify-content:center;font:24px/1 sans-serif";

function player(provider: EmbedOptions["provider"], id: string): string {
  return provider === "vimeo" ? `https://player.vimeo.com/video/${id}?autoplay=1` : `https://www.youtube-nocookie.com/embed/${id}?autoplay=1`;
}

/** The placeholder's parts as an adapter renders them in the browser: YouTube's still from its CDN, none for Vimeo, whose still needs a lookup only the build makes. */
export function embedParts(props: EmbedOptions): { outer: Record<string, string>; button: Record<string, string>; still: Record<string, string> | null; title: Record<string, string>; play: Record<string, string> } {
  const outer: Record<string, string> = { [EMBED_ATTR]: player(props.provider, props.id), "data-sf-embed-title": props.title, style: OUTER_STYLE };
  if (props.className) outer.class = props.className;
  const still = props.provider === "youtube" ? { src: `https://i.ytimg.com/vi/${props.id}/hqdefault.jpg`, alt: "", width: "480", height: "360", loading: "lazy", decoding: "async", style: STILL_STYLE } : null;
  return { outer, button: { type: "button", style: BUTTON_STYLE }, still, title: { style: TITLE_STYLE }, play: { "aria-hidden": "true", style: PLAY_STYLE } };
}

/** `css` as the object React's `style` takes. */
export function styleObject(css: string): Record<string, string> {
  const out: Record<string, string> = {};
  for (const part of css.split(";")) {
    const at = part.indexOf(":");
    if (at < 0) continue;
    const name = part.slice(0, at).trim().replace(/-([a-z])/g, (_, c: string) => c.toUpperCase());
    out[name] = part.slice(at + 1).trim();
  }
  return out;
}

/** Swaps the placeholder `button` sits in for its player. */
export function playEmbed(button: Element): void {
  const holder = button.closest(`[${EMBED_ATTR}]`);
  const src = holder?.getAttribute(EMBED_ATTR);
  if (!holder || !src) return;
  const frame = document.createElement("iframe");
  frame.src = src;
  frame.title = holder.getAttribute("data-sf-embed-title") ?? "";
  frame.allow = "accelerometer; autoplay; clipboard-write; encrypted-media; gyroscope; picture-in-picture; web-share; fullscreen";
  frame.referrerPolicy = "strict-origin-when-cross-origin";
  frame.setAttribute("style", "position:absolute;inset:0;width:100%;height:100%;border:0");
  holder.replaceChildren(frame);
}

/** The document the click listener is on; the test runner's documents share this module, a browser's do not. */
let listening: Document | null = null;

/** Plays a placeholder when its button is clicked, wherever it sits in the document, a navigation's new page included. `boot` calls it. */
export function enableEmbeds(): void {
  if (listening === document) return;
  listening = document;
  document.addEventListener("click", (event) => {
    const button = (event.target as Element | null)?.closest?.(`[${EMBED_ATTR}] > button`);
    if (button) playEmbed(button);
  });
}
