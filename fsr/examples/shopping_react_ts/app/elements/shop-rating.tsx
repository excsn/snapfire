import { Stars } from "@src/ui/Stars";

export default function ShopRating({ rating, reviews }: { rating: number; reviews: bigint | number }) {
  return (
    <>
      <style>{":host { display: block; } .stars { display: inline-flex; align-items: center; gap: 6px; font-size: 13px; } .stars-glyphs { color: var(--star); letter-spacing: 1px; font-size: 15px; } .stars-rating { color: var(--ink); } .stars-reviews { color: var(--link); }"}</style>
      <Stars rating={rating} reviews={reviews} />
    </>
  );
}
