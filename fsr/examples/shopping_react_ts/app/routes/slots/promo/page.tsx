import type { LayoutPromoProps } from "@generated/client";
import { money } from "@src/ui/money";

export default function Promo({ snacks }: LayoutPromoProps) {
  const shown = snacks.filter((p) => p.tags.includes("snack"));
  return shown.length === 0 ? null : (
    <aside className="promo" aria-label="Snacks at the counter">
      <span className="promo-lead">Snacks at the counter</span>
      {shown.map((p) => (
        <a key={String(p.id)} className="promo-item" href={`/product/${String(p.id)}`}>
          <span aria-hidden="true">{p.image.emoji}</span> {p.name} <span className="promo-price">{money(p.price_cents)}</span>
        </a>
      ))}
    </aside>
  );
}
