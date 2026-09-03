import { useState } from "react";

import { actions, type ProductProps } from "@generated/client";
import { categoryLabel } from "@src/ui/categories";
import { addedToCart, failed } from "@src/ui/feedback";
import { Page } from "@src/ui/Page";
import { money, percentOff } from "@src/ui/money";
import { stockLine } from "@src/ui/ProductCard";
import { Stars } from "@src/ui/Stars";
import { Thumb } from "@src/ui/Thumb";

export default function ProductPage({ product, stock: level, inCart, cartCount }: ProductProps) {
  const [quantity, setQuantity] = useState(1);
  const stock = Number(product.stock);
  const line = stockLine(product.stock);
  const off = percentOff(product.price_cents, product.list_price_cents);
  const held = Number(inCart);
  const ingredients = product.attributes.find((a) => a.name === "Ingredients");
  const specs = product.attributes.filter((a) => a.name !== "Ingredients");

  async function add(): Promise<void> {
    try {
      const result = await actions.cart.addToCart({ product_id: product.id, quantity: BigInt(quantity) });
      addedToCart(product.name, result.count);
    } catch (e) {
      failed(e);
    }
  }

  return (
    <Page header={{ cartCount, category: product.category }} className="product">
      <nav className="crumbs" aria-label="Breadcrumb">
        <a href="/">All</a>
        <span>›</span>
        <a href={`/?category=${product.category}`}>{categoryLabel(product.category)}</a>
        <span>›</span>
        <span className="crumb-here">{product.name}</span>
      </nav>
      <div className="product-layout">
        <div className="product-hero">
          <Thumb image={product.image} size="hero" />
        </div>
        <div className="product-info">
          <h1>{product.name}</h1>
          <p className="product-brand">
            by <a href={`/?q=${encodeURIComponent(product.brand)}`}>{product.brand}</a>
          </p>
          <Stars rating={product.rating} reviews={product.reviews} />
          <hr />
          <p className="price-line price-line-big">
            {off > 0 ? <span className="deal">-{off}%</span> : null}
            <span className="price">{money(product.price_cents)}</span>
          </p>
          {off > 0 && product.list_price_cents != null ? (
            <p className="list-price-line">
              List price: <span className="list-price">{money(product.list_price_cents)}</span>
            </p>
          ) : null}
          <p className="product-description">{product.description}</p>
          {ingredients ? (
            <section className="ingredients">
              <h2>Ingredients</h2>
              <p>{ingredients.value}</p>
            </section>
          ) : null}
          <section className="specs">
            <h2>About this item</h2>
            <table>
              <tbody>
                {specs.map((a) => (
                  <tr key={a.name}>
                    <th scope="row">{a.name}</th>
                    <td>{a.value}</td>
                  </tr>
                ))}
              </tbody>
            </table>
          </section>
          <ul className="tags">
            {product.tags.map((t) => (
              <li key={t}>
                <a href={`/?q=${encodeURIComponent(t)}`}>{t}</a>
              </li>
            ))}
          </ul>
        </div>
        <aside className="buy-box">
          <p className="price-line price-line-big">
            <span className="price">{money(product.price_cents)}</span>
          </p>
          <p className={line.className}>{line.text}</p>
          {stock > 0 ? (
            <>
              <label className="qty-label">
                Quantity
                <select value={quantity} onChange={(e) => setQuantity(Number(e.target.value))}>
                  {Array.from({ length: Math.min(stock, 10) }, (_, i) => i + 1).map((n) => (
                    <option key={n} value={n}>
                      {n}
                    </option>
                  ))}
                </select>
              </label>
              <button className="btn btn-primary btn-block" onClick={() => void add()}>
                Add to cart
              </button>
            </>
          ) : (
            <button className="btn btn-primary btn-block" disabled>
              Add to cart
            </button>
          )}
          {held > 0 ? (
            <p className="in-cart">
              {held} in your cart. <a href="/cart">View cart</a>
            </p>
          ) : null}
          <p className="buy-note">
            {Number(level.on_hand)} on hand in the {level.warehouse} warehouse, bin {level.bins.join(", ")}
            {Number(level.reserved) > 0 ? `, ${Number(level.reserved)} reserved` : ""}.
          </p>
          <p className="buy-note">Ships from snapfire.shop. Sold by {product.brand}.</p>
        </aside>
      </div>
    </Page>
  );
}
