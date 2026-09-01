import { action } from "@snapfire/fsr-client";

const addToCart = action("add_to_cart");

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
      <p>
        <a href="/cart">cart</a>
      </p>
      <ul>
        {products.map((p) => (
          <li key={String(p.id)}>
            <a href={`/product/${p.id}`}>{p.name}</a> {money(p.price_cents)}{" "}
            <span className={p.stock > 0 ? "in-stock" : "out-of-stock"}>
              {p.stock > 0 ? `${p.stock} in stock` : "out of stock"}
            </span>{" "}
            {p.stock > 0 ? (
              <button onClick={() => addToCart({ product_id: BigInt(p.id), quantity: 1n })}>add to cart</button>
            ) : null}
          </li>
        ))}
      </ul>
    </main>
  );
}
