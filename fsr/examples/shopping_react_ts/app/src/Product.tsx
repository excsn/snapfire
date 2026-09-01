interface Product {
  id: bigint | number;
  name: string;
  price_cents: bigint | number;
  stock: number;
  tags: string[];
}

export default function ProductPage({ product }: { product: Product }) {
  return (
    <main className="product">
      <p>
        <a href="/">back to the catalog</a>
      </p>
      <h1>{product.name}</h1>
      <p className="price">${(Number(product.price_cents) / 100).toFixed(2)}</p>
      <p className="stock">{product.stock > 0 ? `${product.stock} in stock` : "out of stock"}</p>
      <ul className="tags">
        {product.tags.map((t) => (
          <li key={t}>{t}</li>
        ))}
      </ul>
    </main>
  );
}
