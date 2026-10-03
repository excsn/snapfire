import { Picture } from "@snapfire/fsr-client/react";

import type { Image } from "@generated/client";
import { PHOTOS } from "./photos";

/** A product photo on a card or a cart line, lazily, at the width the placement renders. */
export function Thumb({ image, size = "card" }: { image: Image; size?: "card" | "line" }) {
  if (size === "line") {
    return <Picture src={PHOTOS[image.file]} alt="" sizes="120px" className="thumb thumb-line" style={{ background: image.color }} />;
  }
  return <Picture src={PHOTOS[image.file]} alt="" sizes="(max-width: 640px) 100vw, (max-width: 1100px) 50vw, 300px" className="thumb thumb-card" style={{ background: image.color }} />;
}

/** The detail page's photo: the largest thing on the page, so it loads eagerly and the server preloads it. */
export function Hero({ image }: { image: Image }) {
  return <Picture src={PHOTOS[image.file]} alt="" sizes="(max-width: 900px) 100vw, 50vw" priority className="thumb thumb-hero" style={{ background: image.color }} />;
}
