import type { Image } from "../../generated/client";

export function Thumb({ image, size = "card" }: { image: Image; size?: "card" | "line" | "hero" }) {
  return (
    <div className={`thumb thumb-${size}`} style={{ background: image.color }} aria-hidden="true">
      <span>{image.emoji}</span>
    </div>
  );
}
