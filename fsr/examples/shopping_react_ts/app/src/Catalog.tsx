interface Product {
  id: bigint | number;
  name: string;
  price_cents: bigint | number;
  stock: number;
  tags: string[];
}

function money(cents: bigint | number): string {
  return `$${(Number(cents) / 100).toFixed(2)}`;
}

export default function Catalog({ products }: { products: Product[] }) {
  return (
    <main className="catalog">
      <h1>Catalog</h1>
      <ul>
        {products.map((p) => (
          <li key={String(p.id)}>
            <a href={`/product/${p.id}`}>{p.name}</a> {money(p.price_cents)}{" "}
            <span className={p.stock > 0 ? "in-stock" : "out-of-stock"}>
              {p.stock > 0 ? `${p.stock} in stock` : "out of stock"}
            </span>
          </li>
        ))}
      </ul>
    </main>
  );
}
