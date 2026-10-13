/** A video hosted elsewhere, shown as a still with a play button until the visitor plays it. */
export interface EmbedOptions {
	provider: "youtube" | "vimeo";
	id: string;
	/** Names the video on the placeholder and to a screen reader, and titles the player. */
	title: string;
	className?: string;
}
/** The attribute the server writes on a placeholder: the player the click puts in its place. */
export declare const EMBED_ATTR = "data-sf-embed";
/** The placeholder's parts as an adapter renders them in the browser: YouTube's still from its CDN, none for Vimeo, whose still needs a lookup only the build makes. */
export declare function embedParts(props: EmbedOptions): {
	outer: Record<string, string>;
	button: Record<string, string>;
	still: Record<string, string> | null;
	title: Record<string, string>;
	play: Record<string, string>;
};
/** `css` as the object React's `style` takes. */
export declare function styleObject(css: string): Record<string, string>;
/** Swaps the placeholder `button` sits in for its player. */
export declare function playEmbed(button: Element): void;
/** Plays a placeholder when its button is clicked, wherever it sits in the document, a navigation's new page included. `boot` calls it. */
export declare function enableEmbeds(): void;
