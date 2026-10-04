/**
 * What `<Picture>` writes, apart from any framework: the image's attributes
 * and the sources around it, under the policy the server rendered with.
 * Each adapter's `Picture` builds its own elements from these.
 */
/** An imported image, as the bundle binds it: the hashed original's URL and the size read from its header. */
export interface ImageAsset {
  src: string;
  width: number;
  height: number;
  /** An APNG, which is served as it is. */
  animated?: boolean;
  /** The `<source>` rows the build derived, present on the server where a `meta` preloads one of them and absent in the browser. */
  sources?: { type: string; srcset: string }[];
}

/** The policy the server rendered with, carried by the document's `sf:images` meta so the browser writes the same `<picture>`. */
interface ImagePolicy {
  widths: number[];
  formats: string[];
  base?: string | null;
  sources: { [name: string]: string };
}

const DEFAULT_POLICY: ImagePolicy = { widths: [640, 960, 1280, 1920, 2560], formats: ["avif", "webp"], sources: {} };
const MIME: { [format: string]: string } = { avif: "image/avif", webp: "image/webp" };
let policyRead: ImagePolicy | undefined;

function imagePolicy(): ImagePolicy {
  if (policyRead) return policyRead;
  let read: ImagePolicy | undefined;
  if (typeof document !== "undefined") {
    const meta = document.querySelector('meta[name="sf:images"]');
    const content = meta?.getAttribute("content");
    if (content) {
      try {
        read = { ...DEFAULT_POLICY, ...(JSON.parse(content) as Partial<ImagePolicy>) };
      } catch {
        read = undefined;
      }
    }
  }
  policyRead = read ?? DEFAULT_POLICY;
  return policyRead;
}

function servedAsIs(asset: ImageAsset): boolean {
  const ext = asset.src.split("?")[0].split("#")[0].split(".").pop()?.toLowerCase();
  return asset.animated === true || ext === "svg" || ext === "gif";
}

function fillTemplate(template: string, src: string, width: number): string {
  return template.split("{src}").join(src).split("{width}").join(String(width));
}

/** The attributes a `<Picture>` writes and the `<source>` rows around its `<img>`; `sources` is null when the image goes out as a bare `<img>`. Names are in JSX spelling, `srcSet` among them; `priority` is the attribute name the caller's renderer gives fetch priority. */
export function pictureParts(props: PictureOptions, priority = "fetchpriority"): { img: Record<string, unknown>; sources: Record<string, unknown>[] | null } {
  const { src, source, priority: high, widths, quality, sizes, loading, decoding, ...rest } = props;
  void quality;
  const policy = imagePolicy();
  const img: Record<string, unknown> = { ...rest, loading: loading ?? (high ? "eager" : "lazy"), decoding: decoding ?? "async" };
  if (high) img[priority] = "high";
  if (typeof src === "string") {
    const template = source ? policy.sources[source] : undefined;
    if (template) {
      const all = policy.widths.length ? policy.widths : DEFAULT_POLICY.widths;
      img.src = fillTemplate(template, src, Math.max(...all));
      img.srcSet = all.map((w) => `${fillTemplate(template, src, w)} ${w}w`).join(", ");
      img.sizes = sizes ?? "100vw";
    } else {
      img.src = src;
    }
    return { img, sources: null };
  }
  const base = policy.base ?? "";
  const url = base && src.src.startsWith("/") ? `${base}${src.src}` : src.src;
  img.src = url;
  if (img.width === undefined) img.width = src.width;
  if (img.height === undefined) img.height = src.height;
  if (servedAsIs(src)) return { img, sources: null };
  const chosen = (widths ?? policy.widths).filter((w) => w > 0 && w < src.width).sort((a, b) => a - b).filter((w, i, all) => i === 0 || all[i - 1] !== w);
  chosen.push(src.width);
  const chosenSizes = sizes ?? `(max-width: ${src.width}px) 100vw, ${src.width}px`;
  const stem = url.replace(/\.[^./]+$/, "");
  const sources = policy.formats.map((format) => ({ key: format, type: MIME[format] ?? `image/${format}`, srcSet: chosen.map((w) => `${stem}.${w}.${format} ${w}w`).join(", "), sizes: chosenSizes }));
  return { img, sources };
}

/** What a `<Picture>` takes in every adapter; anything else goes on the `<img>`. */
export interface PictureOptions {
  /** An imported image, or a string: a URL as written, or the value a named source's template takes. */
  src: ImageAsset | string;
  /** The `[images.sources]` entry a string `src` goes through. */
  source?: string;
  /** The largest contentful image: loaded eagerly at high priority and preloaded by the server. */
  priority?: boolean;
  /** The widths to offer instead of the policy's; never upscaled past the image's own. */
  widths?: number[];
  /** The quality the build encodes this image at, which changes the bytes and never the markup. */
  quality?: number | { avif?: number; webp?: number };
  sizes?: string;
  loading?: string;
  decoding?: string;
  [attribute: string]: unknown;
}
