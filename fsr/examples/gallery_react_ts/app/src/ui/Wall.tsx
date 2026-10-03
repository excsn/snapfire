import { Picture } from "@snapfire/fsr-client/react";

import type { Photo } from "../content";
import { PHOTOS } from "./photos";

/** The wall: one figure per photo, each at the width its column renders, lazily. */
export function Wall({ photos }: { photos: Photo[] }) {
  return (
    <section className="wall">
      {photos.map((photo) => (
        <figure key={photo.file} data-zoom={photo.file}>
          <Picture src={PHOTOS[photo.file]} alt={photo.title} sizes="(max-width: 700px) 100vw, 30vw" className="shot" />
          <figcaption>
            <strong>{photo.title}</strong> {photo.place}, {photo.taken}
          </figcaption>
        </figure>
      ))}
    </section>
  );
}
