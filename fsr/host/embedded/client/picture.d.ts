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
	sources?: {
		type: string;
		srcset: string;
	}[];
}
/** The attributes a `<Picture>` writes and the `<source>` rows around its `<img>`; `sources` is null when the image goes out as a bare `<img>`. Names are in JSX spelling, `srcSet` among them; `priority` is the attribute name the caller's renderer gives fetch priority. */
export declare function pictureParts(props: PictureOptions, priority?: string): {
	img: Record<string, unknown>;
	sources: Record<string, unknown>[] | null;
};
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
	quality?: number | {
		avif?: number;
		webp?: number;
	};
	sizes?: string;
	loading?: string;
	decoding?: string;
	[attribute: string]: unknown;
}
