import type { IndexProps } from "@generated/client";
import { categories, categoryLabel } from "@src/ui/categories";
import { ProductCard } from "@src/ui/ProductCard";

function chipHref(category: string, q: string | null | undefined): string {
  const pairs = [q ? `q=${encodeURIComponent(q)}` : "", category ? `category=${encodeURIComponent(category)}` : ""].filter((p) => p !== "");
  return pairs.length > 0 ? `/?${pairs.join("&")}` : "/";
}

export default function Catalog({ products, q, category }: IndexProps) {
  const active = category ?? "";
  const heading = q ? `Results for "${q}"` : active ? categoryLabel(active) : "Today's picks";

  return (
    <main className="page catalog">
      <nav className="chips" aria-label="Categories">
        <a className={active === "" ? "chip chip-active" : "chip"} href={chipHref("", q)}>
          All
        </a>
        {categories.map((c) => (
          <a key={c.key} className={active === c.key ? "chip chip-active" : "chip"} href={chipHref(c.key, q)}>
            {c.label}
          </a>
        ))}
      </nav>
      <div className="results-head">
        <h1>{heading}</h1>
        <p className="results-count">
          {products.length} result{products.length === 1 ? "" : "s"}
          {q && active ? ` in ${categoryLabel(active)}` : ""}
        </p>
      </div>
      {products.length === 0 ? (
        <section className="empty">
          <p className="empty-glyph">🔎</p>
          <h2>Nothing matched</h2>
          <p>Try fewer words, or clear the category.</p>
          <a className="btn btn-secondary" href="/">
            Show everything
          </a>
        </section>
      ) : (
        <section className="grid">
          {products.map((p) => (
            <ProductCard key={String(p.id)} product={p} />
          ))}
        </section>
      )}
    </main>
  );
}
