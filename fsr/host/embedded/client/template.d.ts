/**
* What `@snapfire/fsr-authoring/template` resolves to in the browser. Lowering
* removes the dialect from every page and layout and the build points an
* island's import of it at the island's framework, so this is reached only
* by a file nothing mounts: the placements as plain elements, loading no
* framework.
*/
export { Island, island, Link, Picture, Slot } from "./jsx-runtime.js";
export type { ImageAsset, PictureOptions as PictureProps } from "./picture.js";
export type { LinkOptions as LinkProps } from "./link.js";
