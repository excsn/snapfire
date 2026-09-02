export function Stars({ rating, reviews }: { rating: number; reviews: bigint | number }) {
  const full = Math.round(rating);
  return (
    <span className="stars" title={`${rating.toFixed(1)} out of 5`}>
      <span className="stars-glyphs">{"★".repeat(full) + "☆".repeat(5 - full)}</span>
      <span className="stars-rating">{rating.toFixed(1)}</span>
      <span className="stars-reviews">({Number(reviews).toLocaleString("en-US")})</span>
    </span>
  );
}
